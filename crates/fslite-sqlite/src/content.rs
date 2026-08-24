//! Immutable, chunked content generations and bounded-memory streamed reads/writes.

use std::pin::Pin;

use bytes::Bytes;
use fslite_core::{
    ByteRange, ByteStream, ChangeKind, FileRead, FsError, FsResult, MutationOptions, Node,
    ReadOptions, Revision, TouchOptions, VirtualPath, WorkspaceId, WriteOptions, WriteSource,
};
use futures::{Stream, StreamExt, stream};
use rusqlite::{Connection as RusqliteConnection, OptionalExtension, params};
use tokio_rusqlite::Connection;

use crate::change;
use crate::db::{self, now_ms};
use crate::resolve::{self, DIRECTORY_KIND, RawNode};

/// The size, in bytes, of one immutable content chunk.
pub(crate) const CHUNK_SIZE: usize = 1024 * 1024;

const FILE_KIND: i64 = 1;

fn chunk_index_of(offset: u64) -> i64 {
    (offset / CHUNK_SIZE as u64) as i64
}

fn chunk_local_offset(offset: u64) -> usize {
    (offset % CHUNK_SIZE as u64) as usize
}

// --- low-level generation/chunk SQL -----------------------------------------

pub(crate) fn create_generation(
    conn: &RusqliteConnection,
    workspace_id: &str,
) -> rusqlite::Result<String> {
    use fslite_core::NodeId;
    let id = NodeId::new().to_string();
    conn.execute(
        "INSERT INTO content_generations(id, workspace_id, complete, length, created_at_ms) \
         VALUES (?1, ?2, 0, 0, ?3)",
        params![id, workspace_id, now_ms()],
    )?;
    Ok(id)
}

fn upsert_chunk(
    conn: &RusqliteConnection,
    generation_id: &str,
    chunk_index: i64,
    bytes: &[u8],
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO content_chunks(generation_id, chunk_index, bytes) VALUES (?1, ?2, ?3) \
         ON CONFLICT(generation_id, chunk_index) DO UPDATE SET bytes = excluded.bytes",
        params![generation_id, chunk_index, bytes],
    )?;
    Ok(())
}

fn copy_chunk_range(
    conn: &RusqliteConnection,
    from_generation: &str,
    to_generation: &str,
    from_index_inclusive: i64,
    to_index_exclusive: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO content_chunks(generation_id, chunk_index, bytes) \
         SELECT ?1, chunk_index, bytes FROM content_chunks \
         WHERE generation_id = ?2 AND chunk_index >= ?3 AND chunk_index < ?4",
        params![
            to_generation,
            from_generation,
            from_index_inclusive,
            to_index_exclusive
        ],
    )?;
    Ok(())
}

pub(crate) fn fetch_chunk_bytes(
    conn: &RusqliteConnection,
    generation_id: &str,
    chunk_index: i64,
) -> rusqlite::Result<Vec<u8>> {
    conn.query_row(
        "SELECT bytes FROM content_chunks WHERE generation_id = ?1 AND chunk_index = ?2",
        params![generation_id, chunk_index],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or_default())
}

pub(crate) fn finalize_generation(
    conn: &RusqliteConnection,
    generation_id: &str,
    length: u64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE content_generations SET complete = 1, length = ?2 WHERE id = ?1",
        params![generation_id, length as i64],
    )?;
    Ok(())
}

fn delete_generation(conn: &RusqliteConnection, generation_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM content_generations WHERE id = ?1",
        params![generation_id],
    )?;
    Ok(())
}

fn fetch_max_file_bytes(
    conn: &RusqliteConnection,
    workspace_id: &str,
) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT max_file_bytes FROM workspaces WHERE id = ?1",
        params![workspace_id],
        |row| row.get(0),
    )
    .optional()
}

async fn start_new_generation(conn: &Connection, workspace_id: &str) -> FsResult<String> {
    let workspace_id = workspace_id.to_string();
    conn.call(move |conn| Ok(create_generation(conn, &workspace_id)?))
        .await
        .map_err(db::map_call_error)
}

// --- chunk writer: bounded-memory ingestion into a new generation ----------

/// Accumulates streamed bytes into a new content generation, flushing whole
/// [`CHUNK_SIZE`] chunks as they fill and never holding more than one partial
/// chunk in memory.
struct ChunkWriter {
    conn: Connection,
    generation_id: String,
    max_file_bytes: u64,
    next_chunk_index: i64,
    /// Bytes already flushed to storage as complete chunks.
    committed_len: u64,
    /// Bytes staged in memory, not yet flushed (always `< CHUNK_SIZE`).
    buffer: Vec<u8>,
}

impl ChunkWriter {
    fn new(conn: Connection, generation_id: String, max_file_bytes: u64) -> Self {
        Self {
            conn,
            generation_id,
            max_file_bytes,
            next_chunk_index: 0,
            committed_len: 0,
            buffer: Vec::new(),
        }
    }

    fn total_len(&self) -> u64 {
        self.committed_len + self.buffer.len() as u64
    }

    /// Advances the writer past chunks already committed to storage (via a
    /// bulk SQL copy) without materializing their bytes in memory.
    fn skip_committed_chunks(&mut self, whole_chunk_count: i64) {
        self.next_chunk_index += whole_chunk_count;
        self.committed_len += whole_chunk_count as u64 * CHUNK_SIZE as u64;
    }

    async fn write_bytes(&mut self, mut bytes: Bytes) -> FsResult<()> {
        while !bytes.is_empty() {
            let space = CHUNK_SIZE - self.buffer.len();
            let take = space.min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..take]);
            bytes = bytes.slice(take..);

            if self.total_len() > self.max_file_bytes {
                return Err(FsError::quota_exceeded(format!(
                    "file would exceed the workspace's {}-byte file quota",
                    self.max_file_bytes
                )));
            }

            if self.buffer.len() == CHUNK_SIZE {
                self.flush_full_buffer().await?;
            }
        }
        Ok(())
    }

    async fn flush_full_buffer(&mut self) -> FsResult<()> {
        let chunk = std::mem::take(&mut self.buffer);
        let index = self.next_chunk_index;
        let generation_id = self.generation_id.clone();
        self.conn
            .call(move |conn| Ok(upsert_chunk(conn, &generation_id, index, &chunk)?))
            .await
            .map_err(db::map_call_error)?;
        self.committed_len += CHUNK_SIZE as u64;
        self.next_chunk_index += 1;
        Ok(())
    }

    /// Flushes any remaining partial buffer as the final chunk and returns
    /// the generation id and total logical length written.
    async fn finish(mut self) -> FsResult<(String, u64)> {
        if !self.buffer.is_empty() {
            let len = self.buffer.len() as u64;
            let chunk = std::mem::take(&mut self.buffer);
            let index = self.next_chunk_index;
            let generation_id = self.generation_id.clone();
            self.conn
                .call(move |conn| Ok(upsert_chunk(conn, &generation_id, index, &chunk)?))
                .await
                .map_err(db::map_call_error)?;
            self.committed_len += len;
        }
        Ok((self.generation_id, self.committed_len))
    }

    async fn abort(self) {
        let generation_id = self.generation_id.clone();
        let _ = self
            .conn
            .call(move |conn| Ok(delete_generation(conn, &generation_id)?))
            .await;
    }

    /// Copies `[0, upto)` of `old_generation` verbatim into this writer.
    async fn copy_prefix_from(&mut self, old_generation: &str, upto: u64) -> FsResult<()> {
        if upto == 0 {
            return Ok(());
        }

        let whole_chunks = chunk_index_of(upto);
        if whole_chunks > 0 {
            let from = old_generation.to_string();
            let to = self.generation_id.clone();
            self.conn
                .call(move |conn| Ok(copy_chunk_range(conn, &from, &to, 0, whole_chunks)?))
                .await
                .map_err(db::map_call_error)?;
            self.skip_committed_chunks(whole_chunks);
        }

        let remainder = chunk_local_offset(upto);
        if remainder > 0 {
            let from = old_generation.to_string();
            let index = whole_chunks;
            let bytes = self
                .conn
                .call(move |conn| Ok(fetch_chunk_bytes(conn, &from, index)?))
                .await
                .map_err(db::map_call_error)?;
            let keep = remainder.min(bytes.len());
            self.write_bytes(Bytes::from(bytes).slice(..keep)).await?;
        }

        Ok(())
    }

    /// Copies `[from, to)` of `old_generation` verbatim into this writer.
    async fn copy_suffix_from(&mut self, old_generation: &str, from: u64, to: u64) -> FsResult<()> {
        if from >= to {
            return Ok(());
        }

        let start_chunk = chunk_index_of(from);
        let end_chunk = chunk_index_of(to - 1);

        for index in start_chunk..=end_chunk {
            let generation = old_generation.to_string();
            let bytes = self
                .conn
                .call(move |conn| Ok(fetch_chunk_bytes(conn, &generation, index)?))
                .await
                .map_err(db::map_call_error)?;
            let mut chunk = Bytes::from(bytes);

            if index == end_chunk {
                let keep = chunk_local_offset(to - 1) + 1;
                chunk = chunk.slice(..keep.min(chunk.len()));
            }
            if index == start_chunk {
                let skip = chunk_local_offset(from).min(chunk.len());
                chunk = chunk.slice(skip..);
            }

            self.write_bytes(chunk).await?;
        }

        Ok(())
    }

    /// Writes `len` zero bytes in bounded-size pieces.
    async fn zero_fill(&mut self, len: u64) -> FsResult<()> {
        let mut remaining = len;
        while remaining > 0 {
            let piece = remaining.min(CHUNK_SIZE as u64) as usize;
            self.write_bytes(Bytes::from(vec![0u8; piece])).await?;
            remaining -= piece as u64;
        }
        Ok(())
    }
}

/// Consumes a [`WriteSource`], staging it into a new content generation.
///
/// On any error from the source stream (or a quota violation), the staged
/// generation is deleted and the error is returned; nothing about the
/// target node is touched.
async fn stage_source(
    conn: &Connection,
    generation_id: String,
    max_file_bytes: u64,
    mut source: Pin<Box<dyn Stream<Item = FsResult<Bytes>> + Send>>,
) -> FsResult<(String, u64)> {
    let mut writer = ChunkWriter::new(conn.clone(), generation_id, max_file_bytes);

    while let Some(next) = source.next().await {
        match next {
            Ok(bytes) => {
                if let Err(err) = writer.write_bytes(bytes).await {
                    writer.abort().await;
                    return Err(err);
                }
            }
            Err(err) => {
                writer.abort().await;
                return Err(err);
            }
        }
    }

    writer.finish().await
}

// --- shared finalize: switch the node's generation and append a change ----

struct FinalizeParams {
    allow_create: bool,
    expected_revision: Option<Revision>,
}

enum FinalizeOutcome {
    Applied(RawNode),
    NotFound,
    WrongNodeType,
    RevisionConflict,
}

#[allow(clippy::too_many_arguments)]
fn finalize_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    new_generation_id: &str,
    new_length: u64,
    params: FinalizeParams,
    actor_json: &str,
) -> rusqlite::Result<FinalizeOutcome> {
    let Some(existing) = resolve::resolve(tx, workspace_id, path)? else {
        let Some(parent) = path.parent() else {
            return Ok(FinalizeOutcome::NotFound);
        };
        let Some(parent_node) = resolve::resolve(tx, workspace_id, &parent)? else {
            return Ok(FinalizeOutcome::NotFound);
        };
        if parent_node.kind != DIRECTORY_KIND {
            return Ok(FinalizeOutcome::WrongNodeType);
        }
        if !params.allow_create {
            return Ok(FinalizeOutcome::NotFound);
        }

        use fslite_core::NodeId;
        let node_id = NodeId::new().to_string();
        let now = now_ms();
        let name = path.name().expect("non-root path has a name");
        tx.execute(
            "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision, \
             created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5, 1, ?6, ?6, ?6, ?7)",
            params![
                node_id,
                workspace_id,
                parent_node.id,
                name,
                new_length as i64,
                now,
                new_generation_id,
            ],
        )?;
        finalize_generation(tx, new_generation_id, new_length)?;
        change::append(
            tx,
            workspace_id,
            ChangeKind::Created,
            Some(&node_id),
            None,
            Some(path.as_str()),
            Some(1),
            actor_json,
            now,
        )?;

        return Ok(FinalizeOutcome::Applied(RawNode {
            id: node_id,
            workspace_id: workspace_id.to_string(),
            parent_id: Some(parent_node.id),
            name: name.to_string(),
            kind: FILE_KIND,
            size: new_length as i64,
            revision: 1,
            created_at_ms: now,
            modified_at_ms: now,
            accessed_at_ms: now,
            content_generation_id: Some(new_generation_id.to_string()),
            symlink_target: None,
        }));
    };

    if existing.kind != FILE_KIND {
        return Ok(FinalizeOutcome::WrongNodeType);
    }
    if let Some(expected) = params.expected_revision {
        if existing.revision != expected.get() as i64 {
            return Ok(FinalizeOutcome::RevisionConflict);
        }
    }

    let now = now_ms();
    let new_revision = existing.revision + 1;
    tx.execute(
        "UPDATE nodes SET content_generation_id = ?2, size = ?3, revision = ?4, \
         modified_at_ms = ?5, accessed_at_ms = ?5 WHERE id = ?1",
        params![
            existing.id,
            new_generation_id,
            new_length as i64,
            new_revision,
            now
        ],
    )?;
    finalize_generation(tx, new_generation_id, new_length)?;
    change::append(
        tx,
        workspace_id,
        ChangeKind::Modified,
        Some(&existing.id),
        None,
        Some(path.as_str()),
        Some(new_revision),
        actor_json,
        now,
    )?;

    if let Some(old_generation_id) = &existing.content_generation_id {
        if old_generation_id != new_generation_id {
            delete_generation(tx, old_generation_id)?;
        }
    }

    Ok(FinalizeOutcome::Applied(RawNode {
        id: existing.id,
        workspace_id: workspace_id.to_string(),
        parent_id: existing.parent_id,
        name: existing.name,
        kind: FILE_KIND,
        size: new_length as i64,
        revision: new_revision,
        created_at_ms: existing.created_at_ms,
        modified_at_ms: now,
        accessed_at_ms: now,
        content_generation_id: Some(new_generation_id.to_string()),
        symlink_target: None,
    }))
}

async fn finalize(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    new_generation_id: String,
    new_length: u64,
    params: FinalizeParams,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();
    let generation_id_for_tx = new_generation_id.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = finalize_tx(
                &tx,
                &workspace_id_str,
                &path,
                &generation_id_for_tx,
                new_length,
                params,
                &actor_json,
            )?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error);

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            delete_generation_async(conn, &new_generation_id).await;
            return Err(err);
        }
    };

    match outcome {
        FinalizeOutcome::Applied(row) => row.into_node(),
        FinalizeOutcome::NotFound => {
            delete_generation_async(conn, &new_generation_id).await;
            Err(FsError::not_found(error_path))
        }
        FinalizeOutcome::WrongNodeType => {
            delete_generation_async(conn, &new_generation_id).await;
            Err(FsError::wrong_node_type(error_path))
        }
        FinalizeOutcome::RevisionConflict => {
            delete_generation_async(conn, &new_generation_id).await;
            Err(FsError::revision_conflict(error_path))
        }
    }
}

async fn delete_generation_async(conn: &Connection, generation_id: &str) {
    let generation_id = generation_id.to_string();
    let _ = conn
        .call(move |conn| Ok(delete_generation(conn, &generation_id)?))
        .await;
}

// --- public operations -------------------------------------------------------

pub(crate) async fn write(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    source: WriteSource,
    options: WriteOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let max_file_bytes = conn
        .call(move |conn| Ok(fetch_max_file_bytes(conn, &workspace_id_str)?))
        .await
        .map_err(db::map_call_error)?
        .ok_or_else(|| FsError::not_found(workspace_id))? as u64;

    let generation_id = start_new_generation(conn, &workspace_id.to_string()).await?;
    let (generation_id, length) =
        stage_source(conn, generation_id, max_file_bytes, source.into_stream()).await?;

    finalize(
        conn,
        workspace_id,
        path,
        generation_id,
        length,
        FinalizeParams {
            allow_create: options.create,
            expected_revision: options.expected_revision,
        },
        actor_json,
    )
    .await
}

async fn resolve_existing_file(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: &VirtualPath,
) -> FsResult<Option<RawNode>> {
    let workspace_id_str = workspace_id.to_string();
    let lookup_path = path.clone();
    let raw = conn
        .call(move |conn| Ok(resolve::resolve(conn, &workspace_id_str, &lookup_path)?))
        .await
        .map_err(db::map_call_error)?;

    match raw {
        Some(node) if node.kind == FILE_KIND => Ok(Some(node)),
        Some(_) => Err(FsError::wrong_node_type(path.clone())),
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn write_at_impl(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    offset: u64,
    source: Option<ByteStream>,
    create: bool,
    expected_revision: Option<Revision>,
    actor_json: String,
    preserve_old_suffix: bool,
) -> FsResult<Node> {
    let existing = resolve_existing_file(conn, workspace_id, &path).await?;

    if existing.is_none() && !create {
        return Err(FsError::not_found(path));
    }

    let old_length = existing.as_ref().map(|n| n.size as u64).unwrap_or(0);
    let old_generation_id = existing
        .as_ref()
        .and_then(|n| n.content_generation_id.clone());

    let workspace_id_str = workspace_id.to_string();
    let max_file_bytes = conn
        .call(move |conn| Ok(fetch_max_file_bytes(conn, &workspace_id_str)?))
        .await
        .map_err(db::map_call_error)?
        .ok_or_else(|| FsError::not_found(workspace_id))? as u64;

    let generation_id = start_new_generation(conn, &workspace_id.to_string()).await?;
    let mut writer = ChunkWriter::new(conn.clone(), generation_id, max_file_bytes);

    let prefix_end = offset.min(old_length);
    if let Some(old_generation_id) = &old_generation_id {
        if let Err(err) = writer.copy_prefix_from(old_generation_id, prefix_end).await {
            writer.abort().await;
            return Err(err);
        }
    }

    if offset > old_length {
        if let Err(err) = writer.zero_fill(offset - old_length).await {
            writer.abort().await;
            return Err(err);
        }
    }

    if let Some(mut source) = source {
        while let Some(next) = source.next().await {
            match next {
                Ok(bytes) => {
                    if let Err(err) = writer.write_bytes(bytes).await {
                        writer.abort().await;
                        return Err(err);
                    }
                }
                Err(err) => {
                    writer.abort().await;
                    return Err(err);
                }
            }
        }
    }

    let new_data_end = writer.total_len();

    if preserve_old_suffix && new_data_end < old_length {
        let old_generation_id = old_generation_id
            .clone()
            .expect("old_length > 0 implies a generation exists");
        if let Err(err) = writer
            .copy_suffix_from(&old_generation_id, new_data_end, old_length)
            .await
        {
            writer.abort().await;
            return Err(err);
        }
    }

    let (generation_id, length) = writer.finish().await?;

    finalize(
        conn,
        workspace_id,
        path,
        generation_id,
        length,
        FinalizeParams {
            allow_create: create,
            expected_revision,
        },
        actor_json,
    )
    .await
}

pub(crate) async fn write_at(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    offset: u64,
    source: WriteSource,
    options: WriteOptions,
    actor_json: String,
) -> FsResult<Node> {
    write_at_impl(
        conn,
        workspace_id,
        path,
        offset,
        Some(source.into_stream()),
        options.create,
        options.expected_revision,
        actor_json,
        true,
    )
    .await
}

pub(crate) async fn append(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    source: WriteSource,
    options: WriteOptions,
    actor_json: String,
) -> FsResult<Node> {
    let existing = resolve_existing_file(conn, workspace_id, &path).await?;
    let offset = existing.map(|n| n.size as u64).unwrap_or(0);

    write_at_impl(
        conn,
        workspace_id,
        path,
        offset,
        Some(source.into_stream()),
        options.create,
        options.expected_revision,
        actor_json,
        true,
    )
    .await
}

pub(crate) async fn truncate(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    length: u64,
    options: MutationOptions,
    actor_json: String,
) -> FsResult<Node> {
    write_at_impl(
        conn,
        workspace_id,
        path,
        length,
        None,
        false,
        options.expected_revision,
        actor_json,
        false,
    )
    .await
}

pub(crate) enum TouchOutcome {
    Touched(RawNode),
    Created(RawNode),
    NotFound,
    WrongNodeType,
    RevisionConflict,
}

pub(crate) fn touch_result(outcome: TouchOutcome, path: VirtualPath) -> FsResult<Node> {
    match outcome {
        TouchOutcome::Touched(row) | TouchOutcome::Created(row) => row.into_node(),
        TouchOutcome::NotFound => Err(FsError::not_found(path)),
        TouchOutcome::WrongNodeType => Err(FsError::wrong_node_type(path)),
        TouchOutcome::RevisionConflict => Err(FsError::revision_conflict(path)),
    }
}

/// Bumps timestamps/revision for an existing file, or creates a fresh empty
/// file when absent and `options.create`, entirely within `tx`.
pub(crate) fn touch_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: &str,
    path: &VirtualPath,
    options: TouchOptions,
    actor_json: &str,
) -> rusqlite::Result<TouchOutcome> {
    match resolve::resolve(tx, workspace_id, path)? {
        Some(node) => {
            if node.kind != FILE_KIND {
                return Ok(TouchOutcome::WrongNodeType);
            }
            if let Some(expected) = options.expected_revision {
                if node.revision != expected.get() as i64 {
                    return Ok(TouchOutcome::RevisionConflict);
                }
            }

            let now = now_ms();
            let new_revision = node.revision + 1;
            tx.execute(
                "UPDATE nodes SET revision = ?2, modified_at_ms = ?3, accessed_at_ms = ?3 WHERE id = ?1",
                params![node.id, new_revision, now],
            )?;
            let updated = resolve::fetch_by_id(tx, workspace_id, &node.id)?
                .expect("the node was just updated in this transaction");
            Ok(TouchOutcome::Touched(updated))
        }
        None => {
            if !options.create {
                return Ok(TouchOutcome::NotFound);
            }

            let generation_id = create_generation(tx, workspace_id)?;
            finalize_generation(tx, &generation_id, 0)?;
            let outcome = finalize_tx(
                tx,
                workspace_id,
                path,
                &generation_id,
                0,
                FinalizeParams {
                    allow_create: true,
                    expected_revision: None,
                },
                actor_json,
            )?;
            Ok(match outcome {
                FinalizeOutcome::Applied(row) => TouchOutcome::Created(row),
                FinalizeOutcome::NotFound => TouchOutcome::NotFound,
                FinalizeOutcome::WrongNodeType => TouchOutcome::WrongNodeType,
                FinalizeOutcome::RevisionConflict => TouchOutcome::RevisionConflict,
            })
        }
    }
}

pub(crate) async fn touch(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: TouchOptions,
    actor_json: String,
) -> FsResult<Node> {
    let workspace_id_str = workspace_id.to_string();
    let path_for_tx = path.clone();

    let outcome = conn
        .call(move |conn| {
            let tx = conn.transaction()?;
            let outcome = touch_tx(&tx, &workspace_id_str, &path_for_tx, options, &actor_json)?;
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .map_err(db::map_call_error)?;

    touch_result(outcome, path)
}

// --- read --------------------------------------------------------------------

pub(crate) async fn read(
    conn: &Connection,
    workspace_id: WorkspaceId,
    path: VirtualPath,
    options: ReadOptions,
) -> FsResult<FileRead> {
    let workspace_id_str = workspace_id.to_string();
    let error_path = path.clone();
    let raw = conn
        .call(move |conn| Ok(resolve::resolve(conn, &workspace_id_str, &path)?))
        .await
        .map_err(db::map_call_error)?;

    let node = raw.ok_or_else(|| FsError::not_found(error_path.clone()))?;
    if node.kind != FILE_KIND {
        return Err(FsError::wrong_node_type(error_path));
    }

    let logical_size = node.size as u64;
    let revision = Revision::new(node.revision as u64)
        .ok_or_else(|| FsError::internal_storage_failure("stored revision was zero"))?;

    let requested = options.range.unwrap_or(ByteRange::new(0, logical_size));
    if requested.start > logical_size {
        return Err(FsError::invalid_range(error_path));
    }
    let start = requested.start;
    let end = requested.end.min(logical_size);

    let stream: ByteStream = if start >= end {
        Box::pin(stream::empty())
    } else {
        let generation_id = node
            .content_generation_id
            .clone()
            .expect("a file with nonzero length has a content generation");
        Box::pin(chunk_range_stream(conn.clone(), generation_id, start, end))
    };

    Ok(FileRead {
        logical_length: logical_size,
        revision,
        range: ByteRange::new(start, end),
        stream,
    })
}

struct ChunkRangeState {
    conn: Connection,
    generation_id: String,
    current_chunk: i64,
    end_chunk: i64,
    start: u64,
    end: u64,
    done: bool,
}

fn chunk_range_stream(
    conn: Connection,
    generation_id: String,
    start: u64,
    end: u64,
) -> impl Stream<Item = FsResult<Bytes>> {
    let state = ChunkRangeState {
        conn,
        generation_id,
        current_chunk: chunk_index_of(start),
        end_chunk: chunk_index_of(end - 1),
        start,
        end,
        done: false,
    };

    stream::unfold(state, |mut state| async move {
        if state.done || state.current_chunk > state.end_chunk {
            return None;
        }

        let index = state.current_chunk;
        let generation_id = state.generation_id.clone();
        let result = state
            .conn
            .call(move |conn| Ok(fetch_chunk_bytes(conn, &generation_id, index)?))
            .await
            .map_err(db::map_call_error);

        state.current_chunk += 1;

        match result {
            Ok(bytes) => {
                let mut bytes = Bytes::from(bytes);
                if index == state.end_chunk {
                    let keep = chunk_local_offset(state.end - 1) + 1;
                    bytes = bytes.slice(..keep.min(bytes.len()));
                }
                if index == chunk_index_of(state.start) {
                    let skip = chunk_local_offset(state.start).min(bytes.len());
                    bytes = bytes.slice(skip..);
                }
                Some((Ok(bytes), state))
            }
            Err(err) => {
                state.done = true;
                Some((Err(err), state))
            }
        }
    })
}
