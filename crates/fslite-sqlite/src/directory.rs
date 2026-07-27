//! Stat, directory listing, recursive tree traversal, and directory creation.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{
    ChangeKind, CreateOptions, ErrorCode, FsError, FsResult, Node, Page, PageRequest, TreeEntry,
    TreeOptions, VirtualPath, WorkspaceId,
};
use rusqlite::{Connection as RusqliteConnection, params};
use serde::{Deserialize, Serialize};
use tokio_rusqlite::Connection;

use crate::change;
use crate::db::{self, now_ms};
use crate::resolve::{self, DIRECTORY_KIND, NODE_COLUMNS, RawNode, ResolveOutcome, map_row};

pub(crate) async fn stat(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    follow_symlinks: bool,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();
    let outcome = conn
        .call(move |conn| {
            Ok(resolve::resolve_following(
                conn,
                &workspace_id_str,
                &path,
                follow_symlinks,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    match outcome {
        ResolveOutcome::Found(row) => row.into_node(),
        ResolveOutcome::NotFound => Err(FsError::not_found(error_path)),
        ResolveOutcome::BrokenLink => Err(FsError::broken_link(error_path)),
        ResolveOutcome::LinkLoop => Err(FsError::link_loop(error_path)),
    }
}

pub(crate) async fn exists(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    follow_symlinks: bool,
) -> FsResult<bool> {
    match stat(conn, workspace_id, path, follow_symlinks).await {
        Ok(_) => Ok(true),
        Err(err) if err.code() == ErrorCode::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

// --- mkdir -----------------------------------------------------------------

pub(crate) enum MkdirOutcome {
    Created(RawNode),
    Existing(RawNode),
    AlreadyExists,
    WrongNodeType,
    ParentNotFound,
    RevisionConflict,
}

/// Converts a [`MkdirOutcome`] into its public result, given the path the
/// operation was requested against.
pub(crate) fn mkdir_result(outcome: MkdirOutcome, path: VirtualPath) -> FsResult<Node> {
    match outcome {
        MkdirOutcome::Created(row) | MkdirOutcome::Existing(row) => row.into_node(),
        MkdirOutcome::AlreadyExists => Err(FsError::already_exists(path)),
        MkdirOutcome::WrongNodeType => Err(FsError::wrong_node_type(path)),
        MkdirOutcome::ParentNotFound => Err(FsError::not_found(path)),
        MkdirOutcome::RevisionConflict => Err(FsError::revision_conflict(path)),
    }
}

pub(crate) async fn mkdir(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: CreateOptions,
    actor_json: String,
) -> FsResult<Node> {
    let Some(_) = path.name() else {
        return Err(FsError::already_exists(path));
    };

    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();
    let segments: Vec<String> = path.segments().map(str::to_owned).collect();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = mkdir_tx(&tx, &workspace_id_str, &segments, options, &actor_json)?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    mkdir_result(outcome, error_path)
}

pub(crate) fn mkdir_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    segments: &[String],
    options: CreateOptions,
    actor_json: &str,
) -> rusqlite::Result<MkdirOutcome> {
    let Some(mut current) = resolve::fetch_root(tx, workspace_id)? else {
        return Ok(MkdirOutcome::ParentNotFound);
    };
    let mut current_path = VirtualPath::root();

    let (ancestors, leaf) = segments.split_at(segments.len() - 1);
    let leaf_name = &leaf[0];

    for segment in ancestors {
        if current.kind != DIRECTORY_KIND {
            return Ok(MkdirOutcome::WrongNodeType);
        }

        current = match resolve::fetch_child(tx, workspace_id, &current.id, segment)? {
            Some(child) => child,
            None => {
                if !options.parents {
                    return Ok(MkdirOutcome::ParentNotFound);
                }
                create_directory(
                    tx,
                    workspace_id,
                    &current.id,
                    segment,
                    &current_path.join(segment).expect("segment has no slash"),
                    actor_json,
                )?
            }
        };
        current_path = current_path.join(segment).expect("segment has no slash");
    }

    if current.kind != DIRECTORY_KIND {
        return Ok(MkdirOutcome::WrongNodeType);
    }

    if let Some(existing) = resolve::fetch_child(tx, workspace_id, &current.id, leaf_name)? {
        if existing.kind != DIRECTORY_KIND {
            return Ok(MkdirOutcome::WrongNodeType);
        }

        if !options.exist_ok {
            return Ok(MkdirOutcome::AlreadyExists);
        }

        if let Some(expected) = options.expected_revision
            && existing.revision != expected.get() as i64
        {
            return Ok(MkdirOutcome::RevisionConflict);
        }

        return Ok(MkdirOutcome::Existing(existing));
    }

    let leaf_path = current_path.join(leaf_name).expect("segment has no slash");
    let created = create_directory(
        tx,
        workspace_id,
        &current.id,
        leaf_name,
        &leaf_path,
        actor_json,
    )?;
    Ok(MkdirOutcome::Created(created))
}

fn create_directory(
    conn: &RusqliteConnection,
    workspace_id: &str,
    parent_id: &str,
    name: &str,
    full_path: &VirtualPath,
    actor_json: &str,
) -> rusqlite::Result<RawNode> {
    use fslite_core::NodeId;

    let node_id = NodeId::new();
    let node_id_str = node_id.to_string();
    let now = now_ms();
    conn.execute(
        "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, created_at_ms, modified_at_ms, accessed_at_ms) \
         VALUES (?1, ?2, ?3, ?4, 0, 0, 1, ?5, ?5, ?5)",
        params![node_id_str, workspace_id, parent_id, name, now],
    )?;
    change::append(
        conn,
        workspace_id,
        ChangeKind::Created,
        Some(&node_id_str),
        None,
        Some(full_path.as_str()),
        Some(1),
        actor_json,
        now,
    )?;

    Ok(RawNode {
        id: node_id_str,
        workspace_id: workspace_id.to_string(),
        parent_id: Some(parent_id.to_string()),
        name: name.to_string(),
        kind: DIRECTORY_KIND,
        size: 0,
        revision: 1,
        created_at_ms: now,
        modified_at_ms: now,
        accessed_at_ms: now,
        content_generation_id: None,
        symlink_target: None,
    })
}

// --- read_dir ----------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct DirCursor {
    v: u8,
    workspace_id: String,
    parent_id: String,
    last_name: String,
    last_id: String,
}

fn encode_dir_cursor(
    workspace_id: WorkspaceId,
    parent_id: &str,
    last_name: &str,
    last_id: &str,
) -> String {
    let payload = DirCursor {
        v: 1,
        workspace_id: workspace_id.to_string(),
        parent_id: parent_id.to_string(),
        last_name: last_name.to_string(),
        last_id: last_id.to_string(),
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload is serializable");
    URL_SAFE_NO_PAD.encode(json)
}

fn decode_dir_cursor(raw: &str, workspace_id: WorkspaceId) -> FsResult<DirCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| FsError::invalid_cursor(raw))?;
    let payload: DirCursor =
        serde_json::from_slice(&bytes).map_err(|_| FsError::invalid_cursor(raw))?;
    if payload.v != 1 || payload.workspace_id != workspace_id.to_string() {
        return Err(FsError::invalid_cursor(raw));
    }
    Ok(payload)
}

enum ReadDirRaw {
    Found {
        parent_id: String,
        rows: Vec<RawNode>,
    },
    NotFound,
    NotDirectory,
    CursorMismatch,
}

fn fetch_children_page(
    conn: &RusqliteConnection,
    workspace_id: &str,
    parent_id: &str,
    after_name: &str,
    after_id: &str,
    limit: i64,
) -> rusqlite::Result<Vec<RawNode>> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE workspace_id = ?1 AND parent_id = ?2 AND trashed_at_ms IS NULL \
           AND (name > ?3 OR (name = ?3 AND id > ?4)) \
         ORDER BY name, id LIMIT ?5"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![workspace_id, parent_id, after_name, after_id, limit],
        map_row,
    )?;
    rows.collect()
}

pub(crate) async fn read_dir(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    page: PageRequest,
) -> FsResult<Page<Node>> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();

    let after = match page.cursor.as_deref() {
        Some(raw) => Some(decode_dir_cursor(raw, workspace_id)?),
        None => None,
    };
    let (after_name, after_id) = after
        .as_ref()
        .map(|c| (c.last_name.clone(), c.last_id.clone()))
        .unwrap_or_default();

    let limit = i64::from(page.limit.max(1));

    let raw = conn
        .call(move |conn| {
            let Some(parent) = resolve::resolve(conn, &workspace_id_str, &path)? else {
                return Ok(ReadDirRaw::NotFound);
            };
            if parent.kind != DIRECTORY_KIND {
                return Ok(ReadDirRaw::NotDirectory);
            }
            if let Some(cursor) = &after
                && cursor.parent_id != parent.id
            {
                return Ok(ReadDirRaw::CursorMismatch);
            }

            let rows = fetch_children_page(
                conn,
                &workspace_id_str,
                &parent.id,
                &after_name,
                &after_id,
                limit + 1,
            )?;
            Ok(ReadDirRaw::Found {
                parent_id: parent.id,
                rows,
            })
        })
        .await
        .map_err(db::map_call_error)?;

    let (parent_id, mut rows) = match raw {
        ReadDirRaw::Found { parent_id, rows } => (parent_id, rows),
        ReadDirRaw::NotFound => return Err(FsError::not_found(error_path)),
        ReadDirRaw::NotDirectory => return Err(FsError::wrong_node_type(error_path)),
        ReadDirRaw::CursorMismatch => return Err(FsError::invalid_cursor(error_path)),
    };

    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }

    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("has_more implies at least one row");
        encode_dir_cursor(workspace_id, &parent_id, &last.name, &last.id)
    });

    let nodes = rows
        .into_iter()
        .map(RawNode::into_node)
        .collect::<FsResult<Vec<_>>>()?;

    Ok(Page::new(nodes, next_cursor))
}

// --- tree --------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct TreeCursor {
    v: u8,
    workspace_id: String,
    root_id: String,
    last_depth: i64,
    last_name: String,
    last_id: String,
}

fn encode_tree_cursor(
    workspace_id: WorkspaceId,
    root_id: &str,
    last_depth: i64,
    last_name: &str,
    last_id: &str,
) -> String {
    let payload = TreeCursor {
        v: 1,
        workspace_id: workspace_id.to_string(),
        root_id: root_id.to_string(),
        last_depth,
        last_name: last_name.to_string(),
        last_id: last_id.to_string(),
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload is serializable");
    URL_SAFE_NO_PAD.encode(json)
}

fn decode_tree_cursor(raw: &str, workspace_id: WorkspaceId) -> FsResult<TreeCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| FsError::invalid_cursor(raw))?;
    let payload: TreeCursor =
        serde_json::from_slice(&bytes).map_err(|_| FsError::invalid_cursor(raw))?;
    if payload.v != 1 || payload.workspace_id != workspace_id.to_string() {
        return Err(FsError::invalid_cursor(raw));
    }
    Ok(payload)
}

struct RawTreeRow {
    relative_path: String,
    depth: i64,
    node: RawNode,
}

impl RawTreeRow {
    fn into_entry(self, base_path: &VirtualPath) -> FsResult<TreeEntry> {
        let path = base_path.join(&self.relative_path)?;
        Ok(TreeEntry {
            path,
            depth: self.depth as u32,
            node: self.node.into_node()?,
        })
    }
}

enum TreeRaw {
    Found {
        root_id: String,
        rows: Vec<RawTreeRow>,
    },
    NotFound,
    NotDirectory,
    CursorMismatch,
}

fn fetch_tree_page(
    conn: &RusqliteConnection,
    workspace_id: &str,
    root_id: &str,
    max_depth: Option<i64>,
    after: (i64, &str, &str),
    limit: i64,
) -> rusqlite::Result<Vec<RawTreeRow>> {
    let (after_depth, after_name, after_id) = after;
    let sql = "WITH RECURSIVE descendants(id, workspace_id, parent_id, name, kind, size, revision, \
             created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, depth, relpath) AS ( \
           SELECT id, workspace_id, parent_id, name, kind, size, revision, \
                  created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, 1, name \
           FROM nodes WHERE workspace_id = ?1 AND parent_id = ?2 AND trashed_at_ms IS NULL \
           UNION ALL \
           SELECT n.id, n.workspace_id, n.parent_id, n.name, n.kind, n.size, n.revision, \
                  n.created_at_ms, n.modified_at_ms, n.accessed_at_ms, n.content_generation_id, \
                  n.symlink_target, d.depth + 1, d.relpath || '/' || n.name \
           FROM nodes n JOIN descendants d ON n.parent_id = d.id \
           WHERE n.trashed_at_ms IS NULL AND (?3 IS NULL OR d.depth < ?3) \
         ) \
         SELECT id, workspace_id, parent_id, name, kind, size, revision, \
                created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, depth, relpath \
         FROM descendants \
         WHERE depth > ?4 OR (depth = ?4 AND name > ?5) OR (depth = ?4 AND name = ?5 AND id > ?6) \
         ORDER BY depth, name, id \
         LIMIT ?7";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![
            workspace_id,
            root_id,
            max_depth,
            after_depth,
            after_name,
            after_id,
            limit
        ],
        |row| {
            Ok(RawTreeRow {
                node: RawNode {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    name: row.get(3)?,
                    kind: row.get(4)?,
                    size: row.get(5)?,
                    revision: row.get(6)?,
                    created_at_ms: row.get(7)?,
                    modified_at_ms: row.get(8)?,
                    accessed_at_ms: row.get(9)?,
                    content_generation_id: row.get(10)?,
                    symlink_target: row.get(11)?,
                },
                depth: row.get(12)?,
                relative_path: row.get(13)?,
            })
        },
    )?;

    rows.collect()
}

pub(crate) async fn tree(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: TreeOptions,
    page: PageRequest,
) -> FsResult<Page<TreeEntry>> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();
    let base_path = path.clone();

    let after = match page.cursor.as_deref() {
        Some(raw) => Some(decode_tree_cursor(raw, workspace_id)?),
        None => None,
    };
    let (after_depth, after_name, after_id) = after
        .as_ref()
        .map(|c| (c.last_depth, c.last_name.clone(), c.last_id.clone()))
        .unwrap_or_default();

    let limit = i64::from(page.limit.max(1));
    let max_depth = options.max_depth.map(i64::from);

    let raw = conn
        .call(move |conn| {
            let Some(root) = resolve::resolve(conn, &workspace_id_str, &path)? else {
                return Ok(TreeRaw::NotFound);
            };
            if root.kind != DIRECTORY_KIND {
                return Ok(TreeRaw::NotDirectory);
            }
            if let Some(cursor) = &after
                && cursor.root_id != root.id
            {
                return Ok(TreeRaw::CursorMismatch);
            }

            let rows = fetch_tree_page(
                conn,
                &workspace_id_str,
                &root.id,
                max_depth,
                (after_depth, &after_name, &after_id),
                limit + 1,
            )?;
            Ok(TreeRaw::Found {
                root_id: root.id,
                rows,
            })
        })
        .await
        .map_err(db::map_call_error)?;

    let (root_id, mut rows) = match raw {
        TreeRaw::Found { root_id, rows } => (root_id, rows),
        TreeRaw::NotFound => return Err(FsError::not_found(error_path)),
        TreeRaw::NotDirectory => return Err(FsError::wrong_node_type(error_path)),
        TreeRaw::CursorMismatch => return Err(FsError::invalid_cursor(error_path)),
    };

    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }

    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("has_more implies at least one row");
        encode_tree_cursor(
            workspace_id,
            &root_id,
            last.depth,
            &last.node.name,
            &last.node.id,
        )
    });

    let entries = rows
        .into_iter()
        .map(|row| row.into_entry(&base_path))
        .collect::<FsResult<Vec<_>>>()?;

    Ok(Page::new(entries, next_cursor))
}
