//! Atomic change-feed row insertion, and paginated querying of the feed.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{
    Change, ChangeCursor, ChangeKind, FsError, FsResult, NodeId, Page, PageRequest, Revision,
    VirtualPath, WorkspaceId,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio_rusqlite::Connection as AsyncConnection;

use crate::db;

fn kind_str(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Created => "created",
        ChangeKind::Modified => "modified",
        ChangeKind::Copied => "copied",
        ChangeKind::Moved => "moved",
        ChangeKind::Removed => "removed",
        ChangeKind::Trashed => "trashed",
        ChangeKind::Restored => "restored",
        ChangeKind::Purged => "purged",
        ChangeKind::AttributeSet => "attribute_set",
        ChangeKind::AttributeRemoved => "attribute_removed",
    }
}

fn kind_from_str(value: &str) -> FsResult<ChangeKind> {
    match value {
        "created" => Ok(ChangeKind::Created),
        "modified" => Ok(ChangeKind::Modified),
        "copied" => Ok(ChangeKind::Copied),
        "moved" => Ok(ChangeKind::Moved),
        "removed" => Ok(ChangeKind::Removed),
        "trashed" => Ok(ChangeKind::Trashed),
        "restored" => Ok(ChangeKind::Restored),
        "purged" => Ok(ChangeKind::Purged),
        "attribute_set" => Ok(ChangeKind::AttributeSet),
        "attribute_removed" => Ok(ChangeKind::AttributeRemoved),
        other => Err(FsError::internal_storage_failure(format!(
            "unknown stored change kind {other}"
        ))),
    }
}

/// Bumps the workspace's change sequence and appends one change row.
///
/// Callers run this against the same connection/transaction used for the
/// mutation itself, so the sequence bump, change row, and mutation commit or
/// roll back together.
#[allow(clippy::too_many_arguments)]
pub(crate) fn append(
    conn: &Connection,
    workspace_id: &str,
    kind: ChangeKind,
    node_id: Option<&str>,
    old_path: Option<&str>,
    new_path: Option<&str>,
    revision: Option<i64>,
    actor_json: &str,
    now_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE workspaces SET change_seq = change_seq + 1, updated_at_ms = ?2 WHERE id = ?1",
        params![workspace_id, now_ms],
    )?;
    let sequence: i64 = conn.query_row(
        "SELECT change_seq FROM workspaces WHERE id = ?1",
        params![workspace_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO changes(workspace_id, sequence, kind, node_id, old_path, new_path, revision, created_at_ms, actor_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            workspace_id,
            sequence,
            kind_str(kind),
            node_id,
            old_path,
            new_path,
            revision,
            now_ms,
            actor_json,
        ],
    )?;
    Ok(())
}

// --- paginated querying -----------------------------------------------------

struct RawChange {
    sequence: i64,
    kind: String,
    node_id: Option<String>,
    old_path: Option<String>,
    new_path: Option<String>,
    revision: Option<i64>,
    created_at_ms: i64,
    actor_json: String,
}

impl RawChange {
    fn into_change(self) -> FsResult<Change> {
        Ok(Change {
            sequence: self.sequence as u64,
            kind: kind_from_str(&self.kind)?,
            node_id: self
                .node_id
                .as_deref()
                .map(NodeId::parse)
                .transpose()
                .map_err(FsError::internal_storage_failure)?,
            old_path: self
                .old_path
                .as_deref()
                .map(VirtualPath::parse)
                .transpose()?,
            new_path: self
                .new_path
                .as_deref()
                .map(VirtualPath::parse)
                .transpose()?,
            revision: self
                .revision
                .map(|value| {
                    Revision::new(value as u64).ok_or_else(|| {
                        FsError::internal_storage_failure("stored revision was zero")
                    })
                })
                .transpose()?,
            created_at_ms: self.created_at_ms,
            actor_metadata: serde_json::from_str(&self.actor_json).unwrap_or_default(),
        })
    }
}

fn fetch_changes_page(
    conn: &Connection,
    workspace_id: &str,
    after_sequence: i64,
    limit: i64,
) -> rusqlite::Result<Vec<RawChange>> {
    let mut stmt = conn.prepare(
        "SELECT sequence, kind, node_id, old_path, new_path, revision, created_at_ms, actor_json \
         FROM changes WHERE workspace_id = ?1 AND sequence > ?2 \
         ORDER BY sequence LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![workspace_id, after_sequence, limit], |row| {
        Ok(RawChange {
            sequence: row.get(0)?,
            kind: row.get(1)?,
            node_id: row.get(2)?,
            old_path: row.get(3)?,
            new_path: row.get(4)?,
            revision: row.get(5)?,
            created_at_ms: row.get(6)?,
            actor_json: row.get(7)?,
        })
    })?;
    rows.collect()
}

#[derive(Serialize, Deserialize)]
struct RawChangeCursor {
    v: u8,
    workspace_id: String,
    last_sequence: i64,
}

fn encode_change_cursor(workspace_id: WorkspaceId, last_sequence: i64) -> ChangeCursor {
    let payload = RawChangeCursor {
        v: 1,
        workspace_id: workspace_id.to_string(),
        last_sequence,
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload is serializable");
    ChangeCursor::new(URL_SAFE_NO_PAD.encode(json))
}

fn decode_change_cursor(cursor: &ChangeCursor, workspace_id: WorkspaceId) -> FsResult<i64> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| FsError::invalid_cursor(cursor.as_str()))?;
    let payload: RawChangeCursor =
        serde_json::from_slice(&bytes).map_err(|_| FsError::invalid_cursor(cursor.as_str()))?;
    if payload.v != 1 || payload.workspace_id != workspace_id.to_string() {
        return Err(FsError::invalid_cursor(cursor.as_str()));
    }
    Ok(payload.last_sequence)
}

pub(crate) async fn changes(
    conn: &AsyncConnection,
    workspace_id: WorkspaceId,
    after: Option<ChangeCursor>,
    page: PageRequest,
) -> FsResult<Page<Change>> {
    let after_sequence = match after {
        Some(cursor) => decode_change_cursor(&cursor, workspace_id)?,
        None => 0,
    };
    let limit = i64::from(page.limit.max(1));
    let workspace_id_str = workspace_id.to_string();

    let mut rows = conn
        .call(move |conn| {
            Ok(fetch_changes_page(
                conn,
                &workspace_id_str,
                after_sequence,
                limit + 1,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = has_more.then(|| {
        let last_sequence = rows.last().expect("has_more implies a row").sequence;
        encode_change_cursor(workspace_id, last_sequence)
    });

    let items = rows
        .into_iter()
        .map(RawChange::into_change)
        .collect::<FsResult<Vec<_>>>()?;

    Ok(Page::new(
        items,
        next_cursor.map(|c| c.as_str().to_string()),
    ))
}
