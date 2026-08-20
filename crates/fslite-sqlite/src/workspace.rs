use fslite_core::{FsError, FsResult, NodeId, WorkspaceId, WorkspaceUsage};
use rusqlite::{OptionalExtension, params};
use tokio_rusqlite::Connection;

use crate::db::{self, now_ms};

/// Configurable per-workspace resource limits.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceOptions {
    /// The maximum total logical bytes the workspace may hold.
    pub max_bytes: u64,
    /// The maximum number of active nodes the workspace may hold.
    pub max_nodes: u64,
    /// The maximum logical size of a single regular file.
    pub max_file_bytes: u64,
}

impl Default for WorkspaceOptions {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024 * 1024,
            max_nodes: 1_000_000,
            max_file_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// A created workspace and its configured limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Workspace {
    /// The stable identity of the workspace.
    pub id: WorkspaceId,
    /// The Unix timestamp in milliseconds when the workspace was created.
    pub created_at_ms: i64,
    /// The Unix timestamp in milliseconds when the workspace was last updated.
    pub updated_at_ms: i64,
    /// The configured logical-byte quota.
    pub max_bytes: u64,
    /// The configured node-count quota.
    pub max_nodes: u64,
    /// The configured maximum size of one regular file.
    pub max_file_bytes: u64,
}

pub(crate) async fn create_workspace(
    conn: &Connection,
    options: WorkspaceOptions,
) -> FsResult<Workspace> {
    conn.call(move |conn| {
        let workspace_id = WorkspaceId::new();
        let root_id = NodeId::new();
        let now = now_ms();

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO workspaces(id, created_at_ms, updated_at_ms, change_seq, max_bytes, max_nodes, max_file_bytes) \
             VALUES (?1, ?2, ?2, 0, ?3, ?4, ?5)",
            params![
                workspace_id.to_string(),
                now,
                options.max_bytes as i64,
                options.max_nodes as i64,
                options.max_file_bytes as i64,
            ],
        )?;
        tx.execute(
            "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, created_at_ms, modified_at_ms, accessed_at_ms) \
             VALUES (?1, ?2, NULL, '', 0, 0, 1, ?3, ?3, ?3)",
            params![root_id.to_string(), workspace_id.to_string(), now],
        )?;
        tx.commit()?;

        Ok(Workspace {
            id: workspace_id,
            created_at_ms: now,
            updated_at_ms: now,
            max_bytes: options.max_bytes,
            max_nodes: options.max_nodes,
            max_file_bytes: options.max_file_bytes,
        })
    })
    .await
    .map_err(db::map_call_error)
}

pub(crate) async fn delete_workspace(conn: &Connection, workspace_id: WorkspaceId) -> FsResult<()> {
    let workspace_id_str = workspace_id.to_string();
    conn.call(move |conn| {
        conn.execute(
            "DELETE FROM workspaces WHERE id = ?1",
            params![workspace_id_str],
        )?;
        Ok(())
    })
    .await
    .map_err(db::map_call_error)
}

pub(crate) async fn reset_workspace(conn: &Connection, workspace_id: WorkspaceId) -> FsResult<()> {
    let workspace_id_str = workspace_id.to_string();
    let reset = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
                params![workspace_id_str],
                |row| row.get(0),
            )?;
            if !exists {
                return Ok(false);
            }

            tx.execute(
                "DELETE FROM nodes WHERE workspace_id = ?1",
                params![workspace_id_str],
            )?;
            tx.execute(
                "DELETE FROM content_generations WHERE workspace_id = ?1",
                params![workspace_id_str],
            )?;
            tx.execute(
                "DELETE FROM changes WHERE workspace_id = ?1",
                params![workspace_id_str],
            )?;

            let root_id = NodeId::new();
            let now = now_ms();
            tx.execute(
                "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, \
                 created_at_ms, modified_at_ms, accessed_at_ms) \
                 VALUES (?1, ?2, NULL, '', 0, 0, 1, ?3, ?3, ?3)",
                params![root_id.to_string(), workspace_id_str, now],
            )?;
            tx.execute(
                "UPDATE workspaces SET change_seq = 0, updated_at_ms = ?2 WHERE id = ?1",
                params![workspace_id_str, now],
            )?;
            tx.commit()?;
            Ok(true)
        })
        .await
        .map_err(db::map_call_error)?;

    reset
        .then_some(())
        .ok_or_else(|| FsError::not_found(workspace_id))
}

struct RawUsage {
    active_logical_bytes: i64,
    trashed_logical_bytes: i64,
    staged_bytes: i64,
    active_nodes: i64,
    trashed_nodes: i64,
    max_bytes: i64,
    max_nodes: i64,
    max_file_bytes: i64,
}

impl RawUsage {
    fn into_usage(self, workspace_id: WorkspaceId) -> WorkspaceUsage {
        WorkspaceUsage {
            workspace_id,
            active_logical_bytes: self.active_logical_bytes as u64,
            trashed_logical_bytes: self.trashed_logical_bytes as u64,
            staged_bytes: self.staged_bytes as u64,
            active_nodes: self.active_nodes as u64,
            trashed_nodes: self.trashed_nodes as u64,
            max_logical_bytes: self.max_bytes as u64,
            max_nodes: self.max_nodes as u64,
            max_file_bytes: self.max_file_bytes as u64,
        }
    }
}

fn fetch_usage(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> rusqlite::Result<Option<RawUsage>> {
    let workspace_row: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT max_bytes, max_nodes, max_file_bytes FROM workspaces WHERE id = ?1",
            params![workspace_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let Some((max_bytes, max_nodes, max_file_bytes)) = workspace_row else {
        return Ok(None);
    };

    let (active_logical_bytes, active_nodes): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM nodes \
         WHERE workspace_id = ?1 AND trashed_at_ms IS NULL",
        params![workspace_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let (trashed_logical_bytes, trashed_nodes): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM nodes \
         WHERE workspace_id = ?1 AND trashed_at_ms IS NOT NULL",
        params![workspace_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let staged_bytes: i64 = conn.query_row(
        "SELECT COALESCE(SUM(length), 0) FROM content_generations \
         WHERE workspace_id = ?1 AND complete = 0",
        params![workspace_id],
        |row| row.get(0),
    )?;

    Ok(Some(RawUsage {
        active_logical_bytes,
        trashed_logical_bytes,
        staged_bytes,
        active_nodes,
        trashed_nodes,
        max_bytes,
        max_nodes,
        max_file_bytes,
    }))
}

pub(crate) async fn workspace_usage(
    conn: &Connection,
    workspace_id: WorkspaceId,
) -> FsResult<WorkspaceUsage> {
    let workspace_id_str = workspace_id.to_string();
    let raw = conn
        .call(move |conn| Ok(fetch_usage(conn, &workspace_id_str)?))
        .await
        .map_err(db::map_call_error)?;

    raw.map(|usage| usage.into_usage(workspace_id))
        .ok_or_else(|| FsError::not_found(workspace_id))
}
