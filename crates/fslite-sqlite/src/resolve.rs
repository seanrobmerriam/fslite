//! Workspace-scoped path and node resolution shared by directory operations.

use std::collections::{BTreeMap, VecDeque};

use fslite_core::{
    FsError, FsResult, LinkTarget, Node, NodeId, NodeKind, Revision, VirtualPath, WorkspaceId,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

/// The stored `kind` code for a directory node.
pub(crate) const DIRECTORY_KIND: i64 = 0;
/// The stored `kind` code for a regular file node.
pub(crate) const FILE_KIND: i64 = 1;
/// The stored `kind` code for a symbolic-link node.
pub(crate) const SYMLINK_KIND: i64 = 2;

/// The maximum number of symbolic links resolved in one path lookup.
pub(crate) const MAX_LINK_HOPS: u32 = 40;

pub(crate) const NODE_COLUMNS: &str = "id, workspace_id, parent_id, name, kind, size, revision, \
     created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target";

/// A row fetched from the `nodes` table before conversion to a core [`Node`].
#[derive(Clone)]
pub(crate) struct RawNode {
    pub(crate) id: String,
    pub(crate) workspace_id: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: i64,
    pub(crate) size: i64,
    pub(crate) revision: i64,
    pub(crate) created_at_ms: i64,
    pub(crate) modified_at_ms: i64,
    pub(crate) accessed_at_ms: i64,
    pub(crate) content_generation_id: Option<String>,
    pub(crate) symlink_target: Option<String>,
}

impl RawNode {
    pub(crate) fn into_node(self) -> FsResult<Node> {
        let kind = match self.kind {
            0 => NodeKind::Directory,
            1 => NodeKind::File,
            2 => NodeKind::Symlink,
            other => {
                return Err(FsError::internal_storage_failure(format!(
                    "unknown stored node kind {other}"
                )));
            }
        };

        Ok(Node {
            workspace_id: WorkspaceId::parse(&self.workspace_id)
                .map_err(FsError::internal_storage_failure)?,
            id: NodeId::parse(&self.id).map_err(FsError::internal_storage_failure)?,
            parent_id: self
                .parent_id
                .as_deref()
                .map(NodeId::parse)
                .transpose()
                .map_err(FsError::internal_storage_failure)?,
            name: self.name,
            kind,
            logical_size: self.size as u64,
            created_at_ms: self.created_at_ms,
            modified_at_ms: self.modified_at_ms,
            accessed_at_ms: self.accessed_at_ms,
            revision: Revision::new(self.revision as u64)
                .ok_or_else(|| FsError::internal_storage_failure("stored revision was zero"))?,
            attributes: BTreeMap::new(),
        })
    }
}

pub(crate) fn map_row(row: &Row<'_>) -> rusqlite::Result<RawNode> {
    Ok(RawNode {
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
    })
}

pub(crate) fn fetch_root(
    conn: &Connection,
    workspace_id: &str,
) -> rusqlite::Result<Option<RawNode>> {
    conn.query_row(
        &format!(
            "SELECT {NODE_COLUMNS} FROM nodes \
             WHERE workspace_id = ?1 AND parent_id IS NULL AND trashed_at_ms IS NULL"
        ),
        params![workspace_id],
        map_row,
    )
    .optional()
}

pub(crate) fn fetch_child(
    conn: &Connection,
    workspace_id: &str,
    parent_id: &str,
    name: &str,
) -> rusqlite::Result<Option<RawNode>> {
    conn.query_row(
        &format!(
            "SELECT {NODE_COLUMNS} FROM nodes \
             WHERE workspace_id = ?1 AND parent_id = ?2 AND name = ?3 AND trashed_at_ms IS NULL"
        ),
        params![workspace_id, parent_id, name],
        map_row,
    )
    .optional()
}

pub(crate) fn fetch_children(
    conn: &Connection,
    workspace_id: &str,
    parent_id: &str,
) -> rusqlite::Result<Vec<RawNode>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE workspace_id = ?1 AND parent_id = ?2 AND trashed_at_ms IS NULL \
         ORDER BY name, id"
    ))?;
    let rows = stmt.query_map(params![workspace_id, parent_id], map_row)?;
    rows.collect()
}

pub(crate) fn has_active_children(
    conn: &Connection,
    workspace_id: &str,
    parent_id: &str,
) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM nodes WHERE workspace_id = ?1 AND parent_id = ?2 AND trashed_at_ms IS NULL)",
        params![workspace_id, parent_id],
        |row| row.get(0),
    )
}

/// Resolves a virtual path to its stored node by walking one segment at a time.
///
/// Returns `Ok(None)` when the path does not resolve to a visible node, either
/// because a segment is missing or because a non-directory segment blocks
/// further descent.
pub(crate) fn resolve(
    conn: &Connection,
    workspace_id: &str,
    path: &VirtualPath,
) -> rusqlite::Result<Option<RawNode>> {
    let Some(mut current) = fetch_root(conn, workspace_id)? else {
        return Ok(None);
    };

    for segment in path.segments() {
        if current.kind != DIRECTORY_KIND {
            return Ok(None);
        }

        match fetch_child(conn, workspace_id, &current.id, segment)? {
            Some(child) => current = child,
            None => return Ok(None),
        }
    }

    Ok(Some(current))
}

pub(crate) fn fetch_by_id(
    conn: &Connection,
    workspace_id: &str,
    id: &str,
) -> rusqlite::Result<Option<RawNode>> {
    conn.query_row(
        &format!(
            "SELECT {NODE_COLUMNS} FROM nodes \
             WHERE workspace_id = ?1 AND id = ?2 AND trashed_at_ms IS NULL"
        ),
        params![workspace_id, id],
        map_row,
    )
    .optional()
}

/// Fetches a node by id regardless of trashed state.
pub(crate) fn fetch_by_id_any(
    conn: &Connection,
    workspace_id: &str,
    id: &str,
) -> rusqlite::Result<Option<RawNode>> {
    conn.query_row(
        &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE workspace_id = ?1 AND id = ?2"),
        params![workspace_id, id],
        map_row,
    )
    .optional()
}

/// The outcome of a symlink-aware path resolution.
pub(crate) enum ResolveOutcome {
    /// The path resolved to a visible node.
    Found(RawNode),
    /// A path segment or the target itself does not exist.
    NotFound,
    /// A symbolic link's stored target does not resolve to a visible node.
    BrokenLink,
    /// Resolution exceeded [`MAX_LINK_HOPS`] symbolic-link hops.
    LinkLoop,
}

enum FollowStep {
    /// Continue resolution from `base`, walking `remaining` segments first.
    Next {
        base: Box<RawNode>,
        remaining: VecDeque<String>,
    },
    Broken,
    Loop,
}

/// Follows one symbolic link, returning the node/segments resolution should
/// continue from.
///
/// An absolute target restarts from the workspace root; a relative target
/// resolves against the link's *containing directory* (its parent), walking
/// any leading `..` segments up through ancestors without escaping the root.
fn follow_one(
    conn: &Connection,
    workspace_id: &str,
    symlink_node: &RawNode,
    hops: &mut u32,
) -> rusqlite::Result<FollowStep> {
    *hops += 1;
    if *hops > MAX_LINK_HOPS {
        return Ok(FollowStep::Loop);
    }

    let Some(target_str) = &symlink_node.symlink_target else {
        return Ok(FollowStep::Broken);
    };
    let Ok(target) = LinkTarget::parse(target_str) else {
        return Ok(FollowStep::Broken);
    };

    if target.is_absolute() {
        let Some(root) = fetch_root(conn, workspace_id)? else {
            return Ok(FollowStep::Broken);
        };
        let remaining = split_segments(target.as_str());
        Ok(FollowStep::Next {
            base: Box::new(root),
            remaining,
        })
    } else {
        let Some(parent_id) = &symlink_node.parent_id else {
            return Ok(FollowStep::Broken);
        };
        let Some(mut base) = fetch_by_id(conn, workspace_id, parent_id)? else {
            return Ok(FollowStep::Broken);
        };

        let mut remaining = split_segments(target.as_str());
        while remaining.front().map(String::as_str) == Some("..") {
            remaining.pop_front();
            if let Some(grandparent_id) = base.parent_id.clone() {
                base = match fetch_by_id(conn, workspace_id, &grandparent_id)? {
                    Some(node) => node,
                    None => return Ok(FollowStep::Broken),
                };
            }
            // Already at the root: further ".." segments stay at the root,
            // matching `VirtualPath` normalization semantics.
        }

        Ok(FollowStep::Next {
            base: Box::new(base),
            remaining,
        })
    }
}

fn split_segments(path: &str) -> VecDeque<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(String::from)
        .collect()
}

/// Resolves a virtual path, transparently following symbolic links found in
/// intermediate path segments, and optionally following a symbolic link
/// found at the final segment too.
///
/// Returns [`ResolveOutcome::LinkLoop`] after [`MAX_LINK_HOPS`] hops and
/// [`ResolveOutcome::BrokenLink`] when a followed link's target does not
/// resolve to a visible node.
pub(crate) fn resolve_following(
    conn: &Connection,
    workspace_id: &str,
    path: &VirtualPath,
    follow_final: bool,
) -> rusqlite::Result<ResolveOutcome> {
    let Some(mut current) = fetch_root(conn, workspace_id)? else {
        return Ok(ResolveOutcome::NotFound);
    };
    let mut pending: VecDeque<String> = path.segments().map(String::from).collect();
    let mut hops = 0u32;
    // How many segments at the front of `pending` came from a symlink's own
    // stored target (as opposed to the original path). A lookup failure on
    // one of these means the link's target doesn't resolve (`BrokenLink`); a
    // failure on any segment beyond them is an ordinary `NotFound`.
    let mut link_target_segments = 0usize;

    loop {
        if pending.is_empty() {
            if follow_final && current.kind == SYMLINK_KIND {
                match follow_one(conn, workspace_id, &current, &mut hops)? {
                    FollowStep::Next { base, remaining } => {
                        current = *base;
                        link_target_segments = remaining.len();
                        pending = remaining;
                        continue;
                    }
                    FollowStep::Broken => return Ok(ResolveOutcome::BrokenLink),
                    FollowStep::Loop => return Ok(ResolveOutcome::LinkLoop),
                }
            }
            return Ok(ResolveOutcome::Found(current));
        }

        match current.kind {
            DIRECTORY_KIND => {
                let resolving_link_target = link_target_segments > 0;
                link_target_segments = link_target_segments.saturating_sub(1);
                let segment = pending.pop_front().expect("pending is non-empty");
                current = match fetch_child(conn, workspace_id, &current.id, &segment)? {
                    Some(child) => child,
                    None if resolving_link_target => return Ok(ResolveOutcome::BrokenLink),
                    None => return Ok(ResolveOutcome::NotFound),
                };
            }
            SYMLINK_KIND => match follow_one(conn, workspace_id, &current, &mut hops)? {
                FollowStep::Next { base, remaining } => {
                    current = *base;
                    link_target_segments = remaining.len();
                    for segment in remaining.into_iter().rev() {
                        pending.push_front(segment);
                    }
                }
                FollowStep::Broken => return Ok(ResolveOutcome::BrokenLink),
                FollowStep::Loop => return Ok(ResolveOutcome::LinkLoop),
            },
            _ => return Ok(ResolveOutcome::NotFound),
        }
    }
}
