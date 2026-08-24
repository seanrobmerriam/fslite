//! Copy, move, permanent removal, symbolic links, attributes, atomic
//! batches, and their shared subtree and destination-resolution helpers.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{
    BatchOperation, BatchResult, Capability, ChangeKind, CopyOptions, CreateOptions, FsError,
    FsResult, LinkTarget, MoveOptions, MutationOptions, Node, NodeId, RemoveOptions,
    RequestContext, VirtualPath, WorkspaceId,
};
use rusqlite::params;
use tokio_rusqlite::Connection;

use crate::change;
use crate::content;
use crate::db::{self, now_ms};
use crate::directory;
use crate::resolve::{self, DIRECTORY_KIND, FILE_KIND, RawNode, ResolveOutcome, SYMLINK_KIND};
use crate::trash;

// --- shared destination resolution and subtree removal ---------------------

enum Destination {
    Ready {
        parent_id: String,
        existing: Option<Box<RawNode>>,
    },
    IsRoot,
    ParentNotFound,
    ParentNotDirectory,
}

fn resolve_destination(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    to: &VirtualPath,
) -> rusqlite::Result<Destination> {
    let Some(name) = to.name() else {
        return Ok(Destination::IsRoot);
    };
    let parent_path = to.parent().expect("a named path has a parent");

    match resolve::resolve_following(tx, workspace_id, &parent_path, true)? {
        ResolveOutcome::Found(parent) if parent.kind == DIRECTORY_KIND => {
            let existing = resolve::fetch_child(tx, workspace_id, &parent.id, name)?;
            Ok(Destination::Ready {
                parent_id: parent.id,
                existing: existing.map(Box::new),
            })
        }
        ResolveOutcome::Found(_) => Ok(Destination::ParentNotDirectory),
        _ => Ok(Destination::ParentNotFound),
    }
}

/// Permanently deletes a node and its entire subtree, including every
/// descendant's content generation and chunks.
///
/// Node rows cascade automatically (`nodes.parent_id ... ON DELETE CASCADE`),
/// but content generations do not cascade from the node that references
/// them, so their ids are collected before the cascading delete runs.
pub(crate) fn purge_subtree(tx: &rusqlite::Transaction<'_>, node_id: &str) -> rusqlite::Result<()> {
    let mut stmt = tx.prepare(
        "WITH RECURSIVE subtree(id, content_generation_id) AS ( \
             SELECT id, content_generation_id FROM nodes WHERE id = ?1 \
             UNION ALL \
             SELECT n.id, n.content_generation_id FROM nodes n \
             JOIN subtree s ON n.parent_id = s.id \
         ) \
         SELECT content_generation_id FROM subtree WHERE content_generation_id IS NOT NULL",
    )?;
    let generation_ids: Vec<String> = stmt
        .query_map(params![node_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    tx.execute("DELETE FROM nodes WHERE id = ?1", params![node_id])?;

    for generation_id in generation_ids {
        tx.execute(
            "DELETE FROM content_generations WHERE id = ?1",
            params![generation_id],
        )?;
    }

    Ok(())
}

fn is_same_or_descendant(ancestor: &VirtualPath, candidate: &VirtualPath) -> bool {
    if candidate.as_str() == ancestor.as_str() {
        return true;
    }
    let prefix = if ancestor.as_str() == "/" {
        "/".to_string()
    } else {
        format!("{}/", ancestor.as_str())
    };
    candidate.as_str().starts_with(&prefix)
}

// --- copy --------------------------------------------------------------------

pub(crate) enum CopyOutcome {
    Created(RawNode),
    SourceNotFound,
    SourceLinkLoop,
    SourceNotRecursive,
    RevisionConflict,
    DestIsRoot,
    DestParentNotFound,
    DestParentNotDirectory,
    DestAlreadyExists,
}

pub(crate) fn copy_result(
    outcome: CopyOutcome,
    from: VirtualPath,
    to: VirtualPath,
) -> FsResult<Node> {
    match outcome {
        CopyOutcome::Created(row) => row.into_node(),
        CopyOutcome::SourceNotFound => Err(FsError::not_found(from)),
        CopyOutcome::SourceLinkLoop => Err(FsError::link_loop(from)),
        CopyOutcome::SourceNotRecursive => Err(FsError::wrong_node_type(from)),
        CopyOutcome::RevisionConflict => Err(FsError::revision_conflict(from)),
        CopyOutcome::DestIsRoot | CopyOutcome::DestAlreadyExists => {
            Err(FsError::already_exists(to))
        }
        CopyOutcome::DestParentNotFound => Err(FsError::not_found(to)),
        CopyOutcome::DestParentNotDirectory => Err(FsError::wrong_node_type(to)),
    }
}

pub(crate) async fn copy(
    conn: &Connection,
    workspace_id: WorkspaceId,
    from: VirtualPath,
    to: VirtualPath,
    options: CopyOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let from_for_tx = from.clone();
    let to_for_tx = to.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = copy_tx(
                &tx,
                &workspace_id_str,
                &from_for_tx,
                &to_for_tx,
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    copy_result(outcome, from, to)
}

pub(crate) fn copy_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    from: &VirtualPath,
    to: &VirtualPath,
    options: CopyOptions,
    actor_json: &str,
) -> rusqlite::Result<CopyOutcome> {
    let source = match resolve::resolve_following(tx, workspace_id, from, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            return Ok(CopyOutcome::SourceNotFound);
        }
        ResolveOutcome::LinkLoop => return Ok(CopyOutcome::SourceLinkLoop),
    };

    if let Some(expected) = options.expected_revision {
        if source.revision != expected.get() as i64 {
            return Ok(CopyOutcome::RevisionConflict);
        }
    }

    if source.kind == DIRECTORY_KIND && !options.recursive {
        return Ok(CopyOutcome::SourceNotRecursive);
    }

    let (dest_parent_id, existing, dest_name) = match resolve_destination(tx, workspace_id, to)? {
        Destination::Ready {
            parent_id,
            existing,
        } => (parent_id, existing, to.name().expect("checked by Ready")),
        Destination::IsRoot => return Ok(CopyOutcome::DestIsRoot),
        Destination::ParentNotFound => return Ok(CopyOutcome::DestParentNotFound),
        Destination::ParentNotDirectory => return Ok(CopyOutcome::DestParentNotDirectory),
    };

    if let Some(existing_node) = existing {
        if !options.overwrite {
            return Ok(CopyOutcome::DestAlreadyExists);
        }
        purge_subtree(tx, &existing_node.id)?;
    }

    let now = now_ms();
    let new_node = copy_subtree(tx, workspace_id, &source, &dest_parent_id, dest_name, now)?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Copied,
        Some(&new_node.id),
        Some(from.as_str()),
        Some(to.as_str()),
        Some(1),
        actor_json,
        now,
    )?;

    Ok(CopyOutcome::Created(new_node))
}

/// Iteratively copies `source` (and, if it is a directory, its entire
/// subtree) under `dest_parent_id` as `dest_name`, assigning every copied
/// node a fresh id and every copied file a fresh, independent content
/// generation. Uses an explicit stack rather than Rust-level recursion so
/// traversal depth is not bounded by the call stack.
fn copy_subtree(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    source: &RawNode,
    dest_parent_id: &str,
    dest_name: &str,
    now: i64,
) -> rusqlite::Result<RawNode> {
    let mut stack: Vec<(RawNode, String, String)> = vec![(
        fetch_full(tx, workspace_id, &source.id)?,
        dest_parent_id.to_string(),
        dest_name.to_string(),
    )];
    let mut top: Option<RawNode> = None;

    while let Some((old_node, new_parent_id, new_name)) = stack.pop() {
        let new_id = NodeId::new().to_string();

        let content_generation_id = match old_node.kind {
            DIRECTORY_KIND | SYMLINK_KIND => None,
            FILE_KIND => match &old_node.content_generation_id {
                Some(old_generation_id) => {
                    let new_generation_id = content::create_generation(tx, workspace_id)?;
                    tx.execute(
                        "INSERT INTO content_chunks(generation_id, chunk_index, bytes) \
                         SELECT ?1, chunk_index, bytes FROM content_chunks WHERE generation_id = ?2",
                        params![new_generation_id, old_generation_id],
                    )?;
                    content::finalize_generation(tx, &new_generation_id, old_node.size as u64)?;
                    Some(new_generation_id)
                }
                None => None,
            },
            _ => {
                return Err(rusqlite::Error::InvalidColumnType(
                    4,
                    "kind".to_string(),
                    rusqlite::types::Type::Integer,
                ));
            }
        };

        tx.execute(
            "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, \
             created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?7, ?8, ?9)",
            params![
                new_id,
                workspace_id,
                new_parent_id,
                new_name,
                old_node.kind,
                old_node.size,
                now,
                content_generation_id,
                old_node.symlink_target,
            ],
        )?;
        tx.execute(
            "INSERT INTO attributes(node_id, key, value) \
             SELECT ?1, key, value FROM attributes WHERE node_id = ?2",
            params![new_id, old_node.id],
        )?;

        let new_node = RawNode {
            id: new_id.clone(),
            workspace_id: workspace_id.to_string(),
            parent_id: Some(new_parent_id),
            name: new_name,
            kind: old_node.kind,
            size: old_node.size,
            revision: 1,
            created_at_ms: now,
            modified_at_ms: now,
            accessed_at_ms: now,
            content_generation_id,
            symlink_target: old_node.symlink_target.clone(),
        };

        if old_node.kind == DIRECTORY_KIND {
            for child in resolve::fetch_children(tx, workspace_id, &old_node.id)? {
                let child_name = child.name.clone();
                stack.push((child, new_id.clone(), child_name));
            }
        }

        if top.is_none() {
            top = Some(new_node);
        }
    }

    Ok(top.expect("the source node is always copied first"))
}

fn fetch_full(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    id: &str,
) -> rusqlite::Result<RawNode> {
    resolve::fetch_by_id(tx, workspace_id, id)?.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
}

// --- move --------------------------------------------------------------------

pub(crate) enum MoveOutcome {
    Moved(RawNode),
    SourceIsRoot,
    SourceNotFound,
    SourceLinkLoop,
    DestInsideSource,
    RevisionConflict,
    DestIsRoot,
    DestParentNotFound,
    DestParentNotDirectory,
    DestAlreadyExists,
}

pub(crate) fn move_result(
    outcome: MoveOutcome,
    from: VirtualPath,
    to: VirtualPath,
) -> FsResult<Node> {
    match outcome {
        MoveOutcome::Moved(row) => row.into_node(),
        MoveOutcome::SourceIsRoot | MoveOutcome::DestInsideSource => {
            Err(FsError::permission_denied(from))
        }
        MoveOutcome::SourceNotFound => Err(FsError::not_found(from)),
        MoveOutcome::SourceLinkLoop => Err(FsError::link_loop(from)),
        MoveOutcome::RevisionConflict => Err(FsError::revision_conflict(from)),
        MoveOutcome::DestIsRoot | MoveOutcome::DestAlreadyExists => {
            Err(FsError::already_exists(to))
        }
        MoveOutcome::DestParentNotFound => Err(FsError::not_found(to)),
        MoveOutcome::DestParentNotDirectory => Err(FsError::wrong_node_type(to)),
    }
}

pub(crate) async fn move_path(
    conn: &Connection,
    workspace_id: WorkspaceId,
    from: VirtualPath,
    to: VirtualPath,
    options: MoveOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let from_for_tx = from.clone();
    let to_for_tx = to.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = move_tx(
                &tx,
                &workspace_id_str,
                &from_for_tx,
                &to_for_tx,
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    move_result(outcome, from, to)
}

pub(crate) fn move_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    from: &VirtualPath,
    to: &VirtualPath,
    options: MoveOptions,
    actor_json: &str,
) -> rusqlite::Result<MoveOutcome> {
    if from.name().is_none() {
        return Ok(MoveOutcome::SourceIsRoot);
    }

    let source = match resolve::resolve_following(tx, workspace_id, from, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            return Ok(MoveOutcome::SourceNotFound);
        }
        ResolveOutcome::LinkLoop => return Ok(MoveOutcome::SourceLinkLoop),
    };

    if let Some(expected) = options.expected_revision {
        if source.revision != expected.get() as i64 {
            return Ok(MoveOutcome::RevisionConflict);
        }
    }

    if is_same_or_descendant(from, to) {
        return Ok(MoveOutcome::DestInsideSource);
    }

    let (dest_parent_id, existing, dest_name) = match resolve_destination(tx, workspace_id, to)? {
        Destination::Ready {
            parent_id,
            existing,
        } => (parent_id, existing, to.name().expect("checked by Ready")),
        Destination::IsRoot => return Ok(MoveOutcome::DestIsRoot),
        Destination::ParentNotFound => return Ok(MoveOutcome::DestParentNotFound),
        Destination::ParentNotDirectory => return Ok(MoveOutcome::DestParentNotDirectory),
    };

    if let Some(existing_node) = existing {
        if !options.overwrite {
            return Ok(MoveOutcome::DestAlreadyExists);
        }
        purge_subtree(tx, &existing_node.id)?;
    }

    let now = now_ms();
    let new_revision = source.revision + 1;
    tx.execute(
        "UPDATE nodes SET parent_id = ?2, name = ?3, revision = ?4, modified_at_ms = ?5 WHERE id = ?1",
        params![source.id, dest_parent_id, dest_name, new_revision, now],
    )?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Moved,
        Some(&source.id),
        Some(from.as_str()),
        Some(to.as_str()),
        Some(new_revision),
        actor_json,
        now,
    )?;

    Ok(MoveOutcome::Moved(RawNode {
        id: source.id,
        workspace_id: workspace_id.to_string(),
        parent_id: Some(dest_parent_id),
        name: dest_name.to_string(),
        kind: source.kind,
        size: source.size,
        revision: new_revision,
        created_at_ms: source.created_at_ms,
        modified_at_ms: now,
        accessed_at_ms: source.accessed_at_ms,
        content_generation_id: source.content_generation_id,
        symlink_target: source.symlink_target,
    }))
}

// --- remove --------------------------------------------------------------------

pub(crate) enum RemoveOutcome {
    Removed,
    IsRoot,
    NotFound,
    LinkLoop,
    DirectoryNotEmpty,
    RevisionConflict,
}

pub(crate) fn remove_result(outcome: RemoveOutcome, path: VirtualPath) -> FsResult<()> {
    match outcome {
        RemoveOutcome::Removed => Ok(()),
        RemoveOutcome::IsRoot => Err(FsError::permission_denied(path)),
        RemoveOutcome::NotFound => Err(FsError::not_found(path)),
        RemoveOutcome::LinkLoop => Err(FsError::link_loop(path)),
        RemoveOutcome::DirectoryNotEmpty => Err(FsError::directory_not_empty(path)),
        RemoveOutcome::RevisionConflict => Err(FsError::revision_conflict(path)),
    }
}

pub(crate) async fn remove(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: RemoveOptions,
    actor_json: String,
) -> FsResult<()> {
    let workspace_id_str = workspace_id.to_string();
    let path_for_tx = path.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = remove_tx(&tx, &workspace_id_str, &path_for_tx, options, &actor_json)?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    remove_result(outcome, path)
}

pub(crate) fn remove_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    options: RemoveOptions,
    actor_json: &str,
) -> rusqlite::Result<RemoveOutcome> {
    if path.name().is_none() {
        return Ok(RemoveOutcome::IsRoot);
    }

    let node = match resolve::resolve_following(tx, workspace_id, path, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            return Ok(RemoveOutcome::NotFound);
        }
        ResolveOutcome::LinkLoop => return Ok(RemoveOutcome::LinkLoop),
    };

    if let Some(expected) = options.expected_revision {
        if node.revision != expected.get() as i64 {
            return Ok(RemoveOutcome::RevisionConflict);
        }
    }

    if node.kind == DIRECTORY_KIND
        && !options.recursive
        && resolve::has_active_children(tx, workspace_id, &node.id)?
    {
        return Ok(RemoveOutcome::DirectoryNotEmpty);
    }

    purge_subtree(tx, &node.id)?;

    let now = now_ms();
    change::append(
        tx,
        workspace_id,
        ChangeKind::Removed,
        Some(&node.id),
        Some(path.as_str()),
        None,
        None,
        actor_json,
        now,
    )?;

    Ok(RemoveOutcome::Removed)
}

// --- symlink / read_link -------------------------------------------------------

pub(crate) enum SymlinkOutcome {
    Created(RawNode),
    Existing(RawNode),
    AlreadyExists,
    WrongNodeType,
    ParentNotFound,
    RevisionConflict,
}

pub(crate) fn symlink_result(outcome: SymlinkOutcome, link: VirtualPath) -> FsResult<Node> {
    match outcome {
        SymlinkOutcome::Created(row) | SymlinkOutcome::Existing(row) => row.into_node(),
        SymlinkOutcome::AlreadyExists => Err(FsError::already_exists(link)),
        SymlinkOutcome::WrongNodeType => Err(FsError::wrong_node_type(link)),
        SymlinkOutcome::ParentNotFound => Err(FsError::not_found(link)),
        SymlinkOutcome::RevisionConflict => Err(FsError::revision_conflict(link)),
    }
}

pub(crate) async fn symlink(
    conn: &Connection,
    workspace_id: WorkspaceId,
    target: LinkTarget,
    link: VirtualPath,
    options: CreateOptions,
    actor_json: String,
) -> FsResult<Node> {
    let Some(_) = link.name() else {
        return Err(FsError::already_exists(link));
    };

    let workspace_id_str = workspace_id.to_string();
    let link_for_tx = link.clone();
    let target_str = target.as_str().to_string();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = symlink_tx(
                &tx,
                &workspace_id_str,
                &link_for_tx,
                &target_str,
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    symlink_result(outcome, link)
}

pub(crate) fn symlink_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    link: &VirtualPath,
    target_str: &str,
    options: CreateOptions,
    actor_json: &str,
) -> rusqlite::Result<SymlinkOutcome> {
    let name = link.name().expect("checked by caller");
    let parent_path = link.parent().expect("a named path has a parent");

    let parent = match resolve::resolve_following(tx, workspace_id, &parent_path, true)? {
        ResolveOutcome::Found(node) if node.kind == DIRECTORY_KIND => node,
        ResolveOutcome::Found(_) => return Ok(SymlinkOutcome::WrongNodeType),
        _ => return Ok(SymlinkOutcome::ParentNotFound),
    };

    if let Some(existing) = resolve::fetch_child(tx, workspace_id, &parent.id, name)? {
        if existing.kind != SYMLINK_KIND {
            return Ok(SymlinkOutcome::WrongNodeType);
        }
        if !options.exist_ok {
            return Ok(SymlinkOutcome::AlreadyExists);
        }
        if let Some(expected) = options.expected_revision {
            if existing.revision != expected.get() as i64 {
                return Ok(SymlinkOutcome::RevisionConflict);
            }
        }
        return Ok(SymlinkOutcome::Existing(existing));
    }

    let node_id = NodeId::new().to_string();
    let now = now_ms();
    tx.execute(
        "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, \
         created_at_ms, modified_at_ms, accessed_at_ms, symlink_target) \
         VALUES (?1, ?2, ?3, ?4, 2, 0, 1, ?5, ?5, ?5, ?6)",
        params![node_id, workspace_id, parent.id, name, now, target_str],
    )?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Created,
        Some(&node_id),
        None,
        Some(link.as_str()),
        Some(1),
        actor_json,
        now,
    )?;

    Ok(SymlinkOutcome::Created(RawNode {
        id: node_id,
        workspace_id: workspace_id.to_string(),
        parent_id: Some(parent.id),
        name: name.to_string(),
        kind: SYMLINK_KIND,
        size: 0,
        revision: 1,
        created_at_ms: now,
        modified_at_ms: now,
        accessed_at_ms: now,
        content_generation_id: None,
        symlink_target: Some(target_str.to_string()),
    }))
}

pub(crate) async fn read_link(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
) -> FsResult<LinkTarget> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();

    let outcome = conn
        .call(move |conn| {
            Ok(resolve::resolve_following(
                conn,
                &workspace_id_str,
                &path,
                false,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    match outcome {
        ResolveOutcome::Found(node) if node.kind == SYMLINK_KIND => {
            let target = node.symlink_target.ok_or_else(|| {
                FsError::internal_storage_failure("symlink node missing a stored target")
            })?;
            LinkTarget::parse(&target)
        }
        ResolveOutcome::Found(_) => Err(FsError::wrong_node_type(error_path)),
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            Err(FsError::not_found(error_path))
        }
        ResolveOutcome::LinkLoop => Err(FsError::link_loop(error_path)),
    }
}

// --- attributes ----------------------------------------------------------

/// Attribute sizes and counts are quota-controlled, as application metadata
/// rather than POSIX extended attributes.
const MAX_ATTRIBUTE_KEY_BYTES: usize = 256;
const MAX_ATTRIBUTE_VALUE_BYTES: usize = 4096;
const MAX_ATTRIBUTES_PER_NODE: i64 = 64;

pub(crate) enum AttributeOutcome {
    Updated(Box<RawNode>, BTreeMap<String, serde_json::Value>),
    NotFound,
    LinkLoop,
    InvalidKey,
    QuotaExceeded,
    RevisionConflict,
}

/// Converts an [`AttributeOutcome`] into its public result. Unlike other
/// node conversions, this attaches the node's current attribute map, since
/// [`RawNode::into_node`] otherwise always reports an empty one (populating
/// it universally would require every node-fetching call site across the
/// crate to query the `attributes` table; only the attribute-mutation
/// results carry it today).
pub(crate) fn attribute_result(outcome: AttributeOutcome, path: VirtualPath) -> FsResult<Node> {
    match outcome {
        AttributeOutcome::Updated(row, attributes) => {
            let mut node = row.into_node()?;
            node.attributes = attributes;
            Ok(node)
        }
        AttributeOutcome::NotFound => Err(FsError::not_found(path)),
        AttributeOutcome::LinkLoop => Err(FsError::link_loop(path)),
        AttributeOutcome::InvalidKey => Err(FsError::invalid_path_or_name(path)),
        AttributeOutcome::QuotaExceeded => Err(FsError::quota_exceeded(path)),
        AttributeOutcome::RevisionConflict => Err(FsError::revision_conflict(path)),
    }
}

/// Attribute values are arbitrary bytes, but [`Node::attributes`] is a JSON
/// map; each value is base64url-encoded into a JSON string so it round-trips
/// without assuming the bytes are valid UTF-8.
pub(crate) fn build_attributes_map(
    conn: &rusqlite::Connection,
    node_id: &str,
) -> rusqlite::Result<BTreeMap<String, serde_json::Value>> {
    let mut stmt =
        conn.prepare("SELECT key, value FROM attributes WHERE node_id = ?1 ORDER BY key")?;
    let rows = stmt.query_map(params![node_id], |row| {
        let key: String = row.get(0)?;
        let value: Vec<u8> = row.get(1)?;
        Ok((key, value))
    })?;

    let mut map = BTreeMap::new();
    for row in rows {
        let (key, value) = row?;
        map.insert(
            key,
            serde_json::Value::String(URL_SAFE_NO_PAD.encode(value)),
        );
    }
    Ok(map)
}

pub(crate) fn set_attribute_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    key: &str,
    value: &[u8],
    options: MutationOptions,
    actor_json: &str,
) -> rusqlite::Result<AttributeOutcome> {
    let node = match resolve::resolve_following(tx, workspace_id, path, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            return Ok(AttributeOutcome::NotFound);
        }
        ResolveOutcome::LinkLoop => return Ok(AttributeOutcome::LinkLoop),
    };

    if key.is_empty() || key.len() > MAX_ATTRIBUTE_KEY_BYTES {
        return Ok(AttributeOutcome::InvalidKey);
    }
    if value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
        return Ok(AttributeOutcome::QuotaExceeded);
    }
    if let Some(expected) = options.expected_revision {
        if node.revision != expected.get() as i64 {
            return Ok(AttributeOutcome::RevisionConflict);
        }
    }

    let already_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM attributes WHERE node_id = ?1 AND key = ?2)",
        params![node.id, key],
        |row| row.get(0),
    )?;
    if !already_exists {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM attributes WHERE node_id = ?1",
            params![node.id],
            |row| row.get(0),
        )?;
        if count >= MAX_ATTRIBUTES_PER_NODE {
            return Ok(AttributeOutcome::QuotaExceeded);
        }
    }

    tx.execute(
        "INSERT INTO attributes(node_id, key, value) VALUES (?1, ?2, ?3) \
         ON CONFLICT(node_id, key) DO UPDATE SET value = excluded.value",
        params![node.id, key, value],
    )?;

    let now = now_ms();
    let new_revision = node.revision + 1;
    tx.execute(
        "UPDATE nodes SET revision = ?2, modified_at_ms = ?3 WHERE id = ?1",
        params![node.id, new_revision, now],
    )?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::AttributeSet,
        Some(&node.id),
        None,
        Some(path.as_str()),
        Some(new_revision),
        actor_json,
        now,
    )?;

    let updated =
        resolve::fetch_by_id(tx, workspace_id, &node.id)?.expect("the node was just updated");
    let attributes = build_attributes_map(tx, &node.id)?;
    Ok(AttributeOutcome::Updated(Box::new(updated), attributes))
}

pub(crate) async fn set_attribute(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    key: String,
    value: Vec<u8>,
    options: MutationOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let path_for_tx = path.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = set_attribute_tx(
                &tx,
                &workspace_id_str,
                &path_for_tx,
                &key,
                &value,
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    attribute_result(outcome, path)
}

pub(crate) fn remove_attribute_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    key: &str,
    options: MutationOptions,
    actor_json: &str,
) -> rusqlite::Result<AttributeOutcome> {
    let node = match resolve::resolve_following(tx, workspace_id, path, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => {
            return Ok(AttributeOutcome::NotFound);
        }
        ResolveOutcome::LinkLoop => return Ok(AttributeOutcome::LinkLoop),
    };

    if let Some(expected) = options.expected_revision {
        if node.revision != expected.get() as i64 {
            return Ok(AttributeOutcome::RevisionConflict);
        }
    }

    let deleted = tx.execute(
        "DELETE FROM attributes WHERE node_id = ?1 AND key = ?2",
        params![node.id, key],
    )?;

    let final_node = if deleted > 0 {
        let now = now_ms();
        let new_revision = node.revision + 1;
        tx.execute(
            "UPDATE nodes SET revision = ?2, modified_at_ms = ?3 WHERE id = ?1",
            params![node.id, new_revision, now],
        )?;
        change::append(
            tx,
            workspace_id,
            ChangeKind::AttributeRemoved,
            Some(&node.id),
            None,
            Some(path.as_str()),
            Some(new_revision),
            actor_json,
            now,
        )?;
        resolve::fetch_by_id(tx, workspace_id, &node.id)?.expect("the node was just updated")
    } else {
        node
    };

    let attributes = build_attributes_map(tx, &final_node.id)?;
    Ok(AttributeOutcome::Updated(Box::new(final_node), attributes))
}

pub(crate) async fn remove_attribute(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    key: String,
    options: MutationOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let path_for_tx = path.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = remove_attribute_tx(
                &tx,
                &workspace_id_str,
                &path_for_tx,
                &key,
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    attribute_result(outcome, path)
}

// --- batch -----------------------------------------------------------------

enum BatchStepOutcome {
    Result(BatchResult),
    Failed(FsError),
}

enum BatchOutcome {
    Completed(Vec<BatchResult>),
    Failed { index: usize, error: FsError },
}

fn required_capability(operation: &BatchOperation) -> Capability {
    match operation {
        BatchOperation::Mkdir { .. }
        | BatchOperation::Touch { .. }
        | BatchOperation::Copy { .. }
        | BatchOperation::Move { .. }
        | BatchOperation::Symlink { .. }
        | BatchOperation::SetAttribute { .. }
        | BatchOperation::RemoveAttribute { .. } => Capability::Write,
        BatchOperation::Remove { .. } | BatchOperation::Purge { .. } => Capability::Delete,
        BatchOperation::Trash { .. } | BatchOperation::Restore { .. } => Capability::TrashRestore,
    }
}

fn to_step<T>(result: FsResult<T>, wrap: impl FnOnce(T) -> BatchResult) -> BatchStepOutcome {
    match result {
        Ok(value) => BatchStepOutcome::Result(wrap(value)),
        Err(err) => BatchStepOutcome::Failed(err),
    }
}

/// Executes one [`BatchOperation`] against the shared batch transaction,
/// using each operation's transaction-level `_tx` helper directly rather
/// than the async wrapper (which would try to acquire another connection
/// call from within this one and deadlock).
fn execute_batch_operation(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    operation: BatchOperation,
    actor_json: &str,
) -> rusqlite::Result<BatchStepOutcome> {
    Ok(match operation {
        BatchOperation::Mkdir { path, options } => {
            if path.name().is_none() {
                return Ok(BatchStepOutcome::Failed(FsError::already_exists(path)));
            }
            let segments: Vec<String> = path.segments().map(str::to_owned).collect();
            let outcome = directory::mkdir_tx(tx, workspace_id, &segments, options, actor_json)?;
            to_step(directory::mkdir_result(outcome, path), BatchResult::Node)
        }
        BatchOperation::Touch { path, options } => {
            let outcome = content::touch_tx(tx, workspace_id, &path, options, actor_json)?;
            to_step(content::touch_result(outcome, path), BatchResult::Node)
        }
        BatchOperation::Copy { from, to, options } => {
            let outcome = copy_tx(tx, workspace_id, &from, &to, options, actor_json)?;
            to_step(copy_result(outcome, from, to), BatchResult::Node)
        }
        BatchOperation::Move { from, to, options } => {
            let outcome = move_tx(tx, workspace_id, &from, &to, options, actor_json)?;
            to_step(move_result(outcome, from, to), BatchResult::Node)
        }
        BatchOperation::Remove { path, options } => {
            let outcome = remove_tx(tx, workspace_id, &path, options, actor_json)?;
            to_step(remove_result(outcome, path), |()| BatchResult::Unit)
        }
        BatchOperation::Symlink {
            target,
            link,
            options,
        } => {
            if link.name().is_none() {
                return Ok(BatchStepOutcome::Failed(FsError::already_exists(link)));
            }
            let target_str = target.as_str().to_string();
            let outcome = symlink_tx(tx, workspace_id, &link, &target_str, options, actor_json)?;
            to_step(symlink_result(outcome, link), BatchResult::Node)
        }
        BatchOperation::Trash { path, options } => {
            let outcome = trash::trash_tx(tx, workspace_id, &path, options, actor_json)?;
            let actor_metadata = serde_json::from_str(actor_json).unwrap_or_default();
            to_step(
                trash::trash_result(outcome, path, actor_metadata),
                BatchResult::Trash,
            )
        }
        BatchOperation::Restore {
            trash: trash_id,
            destination,
            options,
        } => {
            let trash_id_str = trash_id.to_string();
            let outcome = trash::restore_tx(
                tx,
                workspace_id,
                &trash_id_str,
                destination.as_ref(),
                options,
                actor_json,
            )?;
            to_step(trash::restore_result(outcome, trash_id), BatchResult::Node)
        }
        BatchOperation::Purge { trash: trash_id } => {
            let trash_id_str = trash_id.to_string();
            let outcome = trash::purge_tx(tx, workspace_id, &trash_id_str, actor_json)?;
            to_step(trash::purge_result(outcome, trash_id), |()| {
                BatchResult::Unit
            })
        }
        BatchOperation::SetAttribute {
            path,
            key,
            value,
            options,
        } => {
            let outcome =
                set_attribute_tx(tx, workspace_id, &path, &key, &value, options, actor_json)?;
            to_step(attribute_result(outcome, path), BatchResult::Node)
        }
        BatchOperation::RemoveAttribute { path, key, options } => {
            let outcome = remove_attribute_tx(tx, workspace_id, &path, &key, options, actor_json)?;
            to_step(attribute_result(outcome, path), BatchResult::Node)
        }
    })
}

pub(crate) async fn batch(
    conn: &Connection,
    ctx: &RequestContext,
    operations: Vec<BatchOperation>,
    actor_json: String,
) -> FsResult<Vec<BatchResult>> {
    let workspace_id_str = ctx.workspace_id.to_string();
    let capabilities: BTreeSet<Capability> = ctx.capabilities.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let mut results = Vec::with_capacity(operations.len());

            for (index, operation) in operations.into_iter().enumerate() {
                let required = required_capability(&operation);
                if !capabilities.contains(&required) {
                    return Ok(BatchOutcome::Failed {
                        index,
                        error: FsError::permission_denied(format!(
                            "batch operation {index} requires {required:?}"
                        )),
                    });
                }

                match execute_batch_operation(&tx, &workspace_id_str, operation, &actor_json)? {
                    BatchStepOutcome::Result(result) => results.push(result),
                    BatchStepOutcome::Failed(error) => {
                        return Ok(BatchOutcome::Failed { index, error });
                    }
                }
            }

            tx.commit()?;
            Ok(BatchOutcome::Completed(results))
        })
        .await
        .map_err(db::map_call_error)?;

    match outcome {
        BatchOutcome::Completed(results) => Ok(results),
        BatchOutcome::Failed { index, error } => {
            let details = serde_json::json!({ "index": index });
            Err(FsError::new(
                error.code(),
                error.message().to_string(),
                details,
            ))
        }
    }
}
