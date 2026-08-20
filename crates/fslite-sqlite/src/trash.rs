//! Recoverable trash: trashing, listing, restoring, and permanent purge.
//!
//! Trashing a node only marks its own row with `trashed_at_ms`; descendants
//! are never individually marked. Every active-node lookup in `resolve.rs`
//! already filters `trashed_at_ms IS NULL` at each step, so a trashed
//! subtree simply becomes unreachable through its (now-invisible) root —
//! there is nothing else to hide.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{
    ChangeKind, FsError, FsResult, MutationOptions, Node, Page, PageRequest, TrashEntry, TrashId,
    VirtualPath, WorkspaceId,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio_rusqlite::Connection;

use crate::change;
use crate::db::{self, now_ms};
use crate::mutate::purge_subtree;
use crate::resolve::{self, DIRECTORY_KIND, RawNode, ResolveOutcome};

// --- trash ---------------------------------------------------------------

pub(crate) enum TrashOutcome {
    Trashed {
        trash_id: String,
        node: Box<RawNode>,
        deleted_at_ms: i64,
    },
    IsRoot,
    NotFound,
    LinkLoop,
    RevisionConflict,
}

pub(crate) fn trash_result(
    outcome: TrashOutcome,
    path: VirtualPath,
    actor_metadata: BTreeMap<String, serde_json::Value>,
) -> FsResult<TrashEntry> {
    match outcome {
        TrashOutcome::Trashed {
            trash_id,
            node,
            deleted_at_ms,
        } => Ok(TrashEntry {
            id: TrashId::parse(&trash_id).map_err(FsError::internal_storage_failure)?,
            node: node.into_node()?,
            original_path: path,
            trashed_at_ms: deleted_at_ms,
            actor_metadata,
        }),
        TrashOutcome::IsRoot => Err(FsError::permission_denied(path)),
        TrashOutcome::NotFound => Err(FsError::not_found(path)),
        TrashOutcome::LinkLoop => Err(FsError::link_loop(path)),
        TrashOutcome::RevisionConflict => Err(FsError::revision_conflict(path)),
    }
}

pub(crate) fn trash_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    options: MutationOptions,
    actor_json: &str,
) -> rusqlite::Result<TrashOutcome> {
    if path.name().is_none() {
        return Ok(TrashOutcome::IsRoot);
    }

    let node = match resolve::resolve_following(tx, workspace_id, path, false)? {
        ResolveOutcome::Found(node) => node,
        ResolveOutcome::NotFound | ResolveOutcome::BrokenLink => return Ok(TrashOutcome::NotFound),
        ResolveOutcome::LinkLoop => return Ok(TrashOutcome::LinkLoop),
    };

    if let Some(expected) = options.expected_revision {
        if node.revision != expected.get() as i64 {
            return Ok(TrashOutcome::RevisionConflict);
        }
    }

    let trash_id = TrashId::new().to_string();
    let now = now_ms();
    let original_parent_id = node
        .parent_id
        .clone()
        .expect("a non-root node has a parent");

    tx.execute(
        "INSERT INTO trash(id, workspace_id, node_id, original_parent_id, original_name, deleted_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![trash_id, workspace_id, node.id, original_parent_id, node.name, now],
    )?;
    tx.execute(
        "UPDATE nodes SET trashed_at_ms = ?2 WHERE id = ?1",
        params![node.id, now],
    )?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Trashed,
        Some(&node.id),
        Some(path.as_str()),
        None,
        Some(node.revision),
        actor_json,
        now,
    )?;

    Ok(TrashOutcome::Trashed {
        trash_id,
        node: Box::new(node),
        deleted_at_ms: now,
    })
}

pub(crate) async fn trash(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: MutationOptions,
    actor_json: String,
) -> FsResult<TrashEntry> {
    let workspace_id_str = workspace_id.to_string();
    let path_for_tx = path.clone();
    let actor_json_for_tx = actor_json.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = trash_tx(
                &tx,
                &workspace_id_str,
                &path_for_tx,
                options,
                &actor_json_for_tx,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    let actor_metadata = serde_json::from_str(&actor_json).unwrap_or_default();
    trash_result(outcome, path, actor_metadata)
}

// --- list_trash ------------------------------------------------------------

struct RawTrashRow {
    id: String,
    node_id: String,
    deleted_at_ms: i64,
}

fn fetch_trash_page(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    after_id: &str,
    limit: i64,
) -> rusqlite::Result<Vec<RawTrashRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_id, deleted_at_ms FROM trash \
         WHERE workspace_id = ?1 AND id > ?2 \
         ORDER BY id LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![workspace_id, after_id, limit], |row| {
        Ok(RawTrashRow {
            id: row.get(0)?,
            node_id: row.get(1)?,
            deleted_at_ms: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// The most recent `trashed` change recorded for a node: its prior path and
/// the actor metadata captured when it was trashed. `trash_tx` always
/// appends this change in the same transaction as the trash record, so a
/// missing row indicates data corruption rather than a normal condition.
fn fetch_trash_change_context(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    node_id: &str,
) -> rusqlite::Result<Option<(Option<String>, String)>> {
    conn.query_row(
        "SELECT old_path, actor_json FROM changes \
         WHERE workspace_id = ?1 AND node_id = ?2 AND kind = 'trashed' \
         ORDER BY sequence DESC LIMIT 1",
        params![workspace_id, node_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

fn build_trash_entry(
    row: RawTrashRow,
    node: Option<RawNode>,
    context: Option<(Option<String>, String)>,
) -> FsResult<TrashEntry> {
    let node =
        node.ok_or_else(|| FsError::internal_storage_failure("trashed node record is missing"))?;
    let (old_path, actor_json) = context.unwrap_or_else(|| (None, "{}".to_string()));
    let original_path = match old_path {
        Some(raw) => VirtualPath::parse(&raw)?,
        None => VirtualPath::root(),
    };
    let actor_metadata = serde_json::from_str(&actor_json).unwrap_or_default();

    Ok(TrashEntry {
        id: TrashId::parse(&row.id).map_err(FsError::internal_storage_failure)?,
        node: node.into_node()?,
        original_path,
        trashed_at_ms: row.deleted_at_ms,
        actor_metadata,
    })
}

#[derive(Serialize, Deserialize)]
struct TrashCursor {
    v: u8,
    workspace_id: String,
    last_id: String,
}

fn encode_trash_cursor(workspace_id: WorkspaceId, last_id: &str) -> String {
    let payload = TrashCursor {
        v: 1,
        workspace_id: workspace_id.to_string(),
        last_id: last_id.to_string(),
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload is serializable");
    URL_SAFE_NO_PAD.encode(json)
}

fn decode_trash_cursor(raw: &str, workspace_id: WorkspaceId) -> FsResult<TrashCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| FsError::invalid_cursor(raw))?;
    let payload: TrashCursor =
        serde_json::from_slice(&bytes).map_err(|_| FsError::invalid_cursor(raw))?;
    if payload.v != 1 || payload.workspace_id != workspace_id.to_string() {
        return Err(FsError::invalid_cursor(raw));
    }
    Ok(payload)
}

pub(crate) async fn list_trash(
    conn: &Connection,
    workspace_id: WorkspaceId,
    page: PageRequest,
) -> FsResult<Page<TrashEntry>> {
    let after_id = match page.cursor.as_deref() {
        Some(raw) => decode_trash_cursor(raw, workspace_id)?.last_id,
        None => String::new(),
    };
    let limit = i64::from(page.limit.max(1));
    let workspace_id_str = workspace_id.to_string();

    let (rows, next_last_id) = conn
        .call(move |conn| {
            let mut rows = fetch_trash_page(conn, &workspace_id_str, &after_id, limit + 1)?;
            let has_more = rows.len() as i64 > limit;
            if has_more {
                rows.truncate(limit as usize);
            }
            let next_last_id =
                has_more.then(|| rows.last().expect("has_more implies a row").id.clone());

            let mut enriched = Vec::with_capacity(rows.len());
            for row in rows {
                let node = resolve::fetch_by_id_any(conn, &workspace_id_str, &row.node_id)?;
                let context = fetch_trash_change_context(conn, &workspace_id_str, &row.node_id)?;
                enriched.push((row, node, context));
            }
            Ok((enriched, next_last_id))
        })
        .await
        .map_err(db::map_call_error)?;

    let next_cursor = next_last_id.map(|id| encode_trash_cursor(workspace_id, &id));
    let items = rows
        .into_iter()
        .map(|(row, node, context)| build_trash_entry(row, node, context))
        .collect::<FsResult<Vec<_>>>()?;

    Ok(Page::new(items, next_cursor))
}

// --- restore -----------------------------------------------------------------

fn fetch_trash_row(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    trash_id: &str,
) -> rusqlite::Result<Option<(String, String, String)>> {
    conn.query_row(
        "SELECT node_id, original_parent_id, original_name FROM trash \
         WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, trash_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

pub(crate) enum RestoreOutcome {
    Restored(RawNode),
    TrashNotFound,
    RevisionConflict,
    DestIsRoot,
    DestParentNotFound,
    DestParentNotDirectory,
    DestAlreadyExists,
    OriginalLocationGone,
}

pub(crate) fn restore_result(outcome: RestoreOutcome, trash_id: TrashId) -> FsResult<Node> {
    match outcome {
        RestoreOutcome::Restored(row) => row.into_node(),
        RestoreOutcome::TrashNotFound
        | RestoreOutcome::OriginalLocationGone
        | RestoreOutcome::DestParentNotFound => Err(FsError::not_found(trash_id)),
        RestoreOutcome::RevisionConflict => Err(FsError::revision_conflict(trash_id)),
        RestoreOutcome::DestIsRoot | RestoreOutcome::DestAlreadyExists => {
            Err(FsError::already_exists(trash_id))
        }
        RestoreOutcome::DestParentNotDirectory => Err(FsError::wrong_node_type(trash_id)),
    }
}

pub(crate) fn restore_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    trash_id: &str,
    destination: Option<&VirtualPath>,
    options: MutationOptions,
    actor_json: &str,
) -> rusqlite::Result<RestoreOutcome> {
    let Some((node_id, original_parent_id, original_name)) =
        fetch_trash_row(tx, workspace_id, trash_id)?
    else {
        return Ok(RestoreOutcome::TrashNotFound);
    };

    let Some(node) = resolve::fetch_by_id_any(tx, workspace_id, &node_id)? else {
        return Ok(RestoreOutcome::TrashNotFound);
    };

    if let Some(expected) = options.expected_revision {
        if node.revision != expected.get() as i64 {
            return Ok(RestoreOutcome::RevisionConflict);
        }
    }

    let (dest_parent_id, dest_name) = match destination {
        Some(dest_path) => {
            let Some(name) = dest_path.name() else {
                return Ok(RestoreOutcome::DestIsRoot);
            };
            let parent_path = dest_path.parent().expect("a named path has a parent");
            match resolve::resolve_following(tx, workspace_id, &parent_path, true)? {
                ResolveOutcome::Found(parent) if parent.kind == DIRECTORY_KIND => {
                    (parent.id, name.to_string())
                }
                ResolveOutcome::Found(_) => return Ok(RestoreOutcome::DestParentNotDirectory),
                _ => return Ok(RestoreOutcome::DestParentNotFound),
            }
        }
        None => match resolve::fetch_by_id(tx, workspace_id, &original_parent_id)? {
            Some(parent) if parent.kind == DIRECTORY_KIND => (original_parent_id, original_name),
            Some(_) => return Ok(RestoreOutcome::DestParentNotDirectory),
            None => return Ok(RestoreOutcome::OriginalLocationGone),
        },
    };

    if resolve::fetch_child(tx, workspace_id, &dest_parent_id, &dest_name)?.is_some() {
        return Ok(RestoreOutcome::DestAlreadyExists);
    }

    let now = now_ms();
    let new_revision = node.revision + 1;
    tx.execute(
        "UPDATE nodes SET parent_id = ?2, name = ?3, trashed_at_ms = NULL, revision = ?4, \
         modified_at_ms = ?5 WHERE id = ?1",
        params![node.id, dest_parent_id, dest_name, new_revision, now],
    )?;
    tx.execute("DELETE FROM trash WHERE id = ?1", params![trash_id])?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Restored,
        Some(&node.id),
        None,
        None,
        Some(new_revision),
        actor_json,
        now,
    )?;

    let restored = resolve::fetch_by_id(tx, workspace_id, &node.id)?
        .expect("the node was just restored in this transaction");
    Ok(RestoreOutcome::Restored(restored))
}

pub(crate) async fn restore(
    conn: &Connection,
    workspace_id: WorkspaceId,
    trash_id: TrashId,
    destination: Option<VirtualPath>,
    options: MutationOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let trash_id_str = trash_id.to_string();
    let destination_for_tx = destination.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = restore_tx(
                &tx,
                &workspace_id_str,
                &trash_id_str,
                destination_for_tx.as_ref(),
                options,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    restore_result(outcome, trash_id)
}

// --- purge -------------------------------------------------------------------

pub(crate) enum PurgeOutcome {
    Purged,
    NotFound,
}

pub(crate) fn purge_result(outcome: PurgeOutcome, trash_id: TrashId) -> FsResult<()> {
    match outcome {
        PurgeOutcome::Purged => Ok(()),
        PurgeOutcome::NotFound => Err(FsError::not_found(trash_id)),
    }
}

pub(crate) fn purge_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    trash_id: &str,
    actor_json: &str,
) -> rusqlite::Result<PurgeOutcome> {
    let Some((node_id, _, _)) = fetch_trash_row(tx, workspace_id, trash_id)? else {
        return Ok(PurgeOutcome::NotFound);
    };

    let now = now_ms();
    change::append(
        tx,
        workspace_id,
        ChangeKind::Purged,
        Some(&node_id),
        None,
        None,
        None,
        actor_json,
        now,
    )?;
    purge_subtree(tx, &node_id)?;

    Ok(PurgeOutcome::Purged)
}

pub(crate) async fn purge(
    conn: &Connection,
    workspace_id: WorkspaceId,
    trash_id: TrashId,
    actor_json: String,
) -> FsResult<()> {
    let workspace_id_str = workspace_id.to_string();
    let trash_id_str = trash_id.to_string();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = purge_tx(&tx, &workspace_id_str, &trash_id_str, &actor_json)?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    purge_result(outcome, trash_id)
}
