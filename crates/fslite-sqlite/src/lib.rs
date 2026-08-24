//! Persistent, multi-workspace SQLite implementation of the `fslite-core` filesystem contract.
//!
//! One SQLite database can hold many isolated workspaces; every stored
//! query is scoped by workspace, so two workspaces may contain identically
//! named paths without collision and cannot observe one another's nodes,
//! trash, attributes, or change feed.
//!
//! ```
//! use fslite_core::{RequestContext, VirtualPath, WriteSource};
//! use fslite_sqlite::SqliteFileSystem;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
//! let workspace = fs.create_workspace(Default::default()).await?;
//! let ctx = RequestContext::trusted(workspace.id);
//!
//! let path = VirtualPath::parse("/hello.txt")?;
//! fs.write(&ctx, &path, WriteSource::from_bytes(b"hello".to_vec()), Default::default())
//!     .await?;
//! assert!(fs.exists(&ctx, &path, Default::default()).await?);
//! # Ok(())
//! # }
//! ```

mod backend;
mod change;
mod content;
mod db;
mod directory;
mod mutate;
mod resolve;
mod search;
mod trash;
mod workspace;

use std::path::Path;

use fslite_core::{
    BatchOperation, BatchResult, Capability, Change, ChangeCursor, ContentQuery, CopyOptions,
    CreateOptions, FileRead, FindQuery, FsError, FsResult, LinkTarget, MoveOptions,
    MutationOptions, Node, Page, PageRequest, ReadOptions, RemoveOptions, RequestContext,
    SearchMatch, StatOptions, TouchOptions, TrashEntry, TrashId, TreeEntry, TreeOptions,
    VirtualPath, WorkspaceId, WorkspaceUsage, WriteOptions, WriteSource,
};

pub use db::ConnectOptions;
pub use fslite_core::FileSystem;
pub use workspace::{Workspace, WorkspaceOptions};

/// A persistent, multi-workspace filesystem backed by a single SQLite database.
///
/// One database may contain many isolated workspaces; every stored query is
/// scoped to a workspace so no operation observes another workspace's nodes.
pub struct SqliteFileSystem {
    conn: tokio_rusqlite::Connection,
}

impl SqliteFileSystem {
    /// Opens (creating if absent) a file-backed SQLite database.
    pub async fn open(path: impl AsRef<Path>, options: ConnectOptions) -> FsResult<Self> {
        let conn = db::open_file(path.as_ref(), options).await?;
        Ok(Self { conn })
    }

    /// Opens a private, in-memory SQLite database.
    pub async fn open_in_memory(options: ConnectOptions) -> FsResult<Self> {
        let conn = db::open_memory(options).await?;
        Ok(Self { conn })
    }

    /// Creates a new isolated workspace with its root directory.
    pub async fn create_workspace(&self, options: WorkspaceOptions) -> FsResult<Workspace> {
        workspace::create_workspace(&self.conn, options).await
    }

    /// Permanently deletes a workspace and everything it contains.
    pub async fn delete_workspace(&self, workspace_id: WorkspaceId) -> FsResult<()> {
        workspace::delete_workspace(&self.conn, workspace_id).await
    }

    /// Atomically returns a workspace to its empty initial state.
    pub async fn reset_workspace(&self, workspace_id: WorkspaceId) -> FsResult<()> {
        workspace::reset_workspace(&self.conn, workspace_id).await
    }

    /// Returns logical and quota usage for the context's workspace.
    pub async fn workspace_usage(&self, ctx: &RequestContext) -> FsResult<WorkspaceUsage> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        workspace::workspace_usage(&self.conn, ctx.workspace_id).await
    }

    /// Returns this database's current schema version.
    pub async fn schema_version(&self) -> FsResult<i64> {
        db::schema_version(&self.conn).await
    }

    /// The schema version this build of `fslite-sqlite` migrates to on open.
    pub fn latest_schema_version() -> i64 {
        db::latest_schema_version()
    }

    /// Runs SQLite's built-in integrity check, returning any problems found.
    /// An empty vec means the database is healthy.
    pub async fn integrity_check(&self) -> FsResult<Vec<String>> {
        db::integrity_check(&self.conn).await
    }

    /// Returns metadata for a path.
    pub async fn stat(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Read, path)?;
        directory::stat(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options.follow_symlinks,
        )
        .await
    }

    /// Returns whether a path resolves to a visible node.
    pub async fn exists(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<bool> {
        require_capability(ctx, Capability::Read, path)?;
        directory::exists(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options.follow_symlinks,
        )
        .await
    }

    /// Returns one cursor-paginated page of direct directory children.
    pub async fn read_dir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        require_capability(ctx, Capability::Read, path)?;
        directory::read_dir(&self.conn, ctx.workspace_id, path.clone(), page).await
    }

    /// Returns one cursor-paginated page of a recursive tree traversal.
    pub async fn tree(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TreeOptions,
        page: PageRequest,
    ) -> FsResult<Page<TreeEntry>> {
        require_capability(ctx, Capability::Read, path)?;
        directory::tree(&self.conn, ctx.workspace_id, path.clone(), options, page).await
    }

    /// Creates a directory.
    pub async fn mkdir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        directory::mkdir(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Opens a bounded-memory byte stream for a regular file.
    pub async fn read(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: ReadOptions,
    ) -> FsResult<FileRead> {
        require_capability(ctx, Capability::Read, path)?;
        content::read(&self.conn, ctx.workspace_id, path.clone(), options).await
    }

    /// Atomically creates or replaces a regular file from a byte stream.
    pub async fn write(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        content::write(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            source,
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Atomically writes streamed bytes beginning at a logical offset.
    pub async fn write_at(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        offset: u64,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        content::write_at(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            offset,
            source,
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Atomically appends streamed bytes to a regular file.
    pub async fn append(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        content::append(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            source,
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Changes a regular file's logical length.
    pub async fn truncate(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        length: u64,
        options: MutationOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        content::truncate(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            length,
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Updates timestamps or creates an empty regular file.
    pub async fn touch(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TouchOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        content::touch(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Copies a node within the context's workspace.
    pub async fn copy(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: CopyOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, to)?;
        mutate::copy(
            &self.conn,
            ctx.workspace_id,
            from.clone(),
            to.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Moves a node within the context's workspace.
    pub async fn move_path(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: MoveOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, to)?;
        mutate::move_path(
            &self.conn,
            ctx.workspace_id,
            from.clone(),
            to.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Permanently removes a node.
    pub async fn remove(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: RemoveOptions,
    ) -> FsResult<()> {
        require_capability(ctx, Capability::Delete, path)?;
        mutate::remove(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Creates a symbolic link containing a relative or absolute virtual target.
    pub async fn symlink(
        &self,
        ctx: &RequestContext,
        target: &LinkTarget,
        link: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, link)?;
        mutate::symlink(
            &self.conn,
            ctx.workspace_id,
            target.clone(),
            link.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Returns the stored target of a symbolic link without resolving it.
    pub async fn read_link(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
    ) -> FsResult<LinkTarget> {
        require_capability(ctx, Capability::Read, path)?;
        mutate::read_link(&self.conn, ctx.workspace_id, path.clone()).await
    }

    /// Moves a node into recoverable trash.
    pub async fn trash(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: MutationOptions,
    ) -> FsResult<TrashEntry> {
        require_capability(ctx, Capability::TrashRestore, path)?;
        trash::trash(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Returns one cursor-paginated page of recoverable trash records.
    pub async fn list_trash(
        &self,
        ctx: &RequestContext,
        page: PageRequest,
    ) -> FsResult<Page<TrashEntry>> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        trash::list_trash(&self.conn, ctx.workspace_id, page).await
    }

    /// Restores a recoverable trash record.
    pub async fn restore(
        &self,
        ctx: &RequestContext,
        trash_id: TrashId,
        destination: Option<&VirtualPath>,
        options: MutationOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::TrashRestore, ctx.workspace_id)?;
        trash::restore(
            &self.conn,
            ctx.workspace_id,
            trash_id,
            destination.cloned(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Permanently purges a recoverable trash record.
    pub async fn purge(&self, ctx: &RequestContext, trash_id: TrashId) -> FsResult<()> {
        require_capability(ctx, Capability::Delete, ctx.workspace_id)?;
        trash::purge(&self.conn, ctx.workspace_id, trash_id, actor_json(ctx)).await
    }

    /// Sets an opaque custom attribute on a node.
    pub async fn set_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        value: &[u8],
        options: MutationOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        mutate::set_attribute(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            key.to_string(),
            value.to_vec(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Removes a custom attribute from a node.
    pub async fn remove_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        options: MutationOptions,
    ) -> FsResult<Node> {
        require_capability(ctx, Capability::Write, path)?;
        mutate::remove_attribute(
            &self.conn,
            ctx.workspace_id,
            path.clone(),
            key.to_string(),
            options,
            actor_json(ctx),
        )
        .await
    }

    /// Executes non-streaming operations atomically in request order.
    pub async fn batch(
        &self,
        ctx: &RequestContext,
        operations: Vec<BatchOperation>,
    ) -> FsResult<Vec<BatchResult>> {
        mutate::batch(&self.conn, ctx, operations, actor_json(ctx)).await
    }

    /// Returns nodes whose absolute paths match a virtual-path glob.
    pub async fn glob(
        &self,
        ctx: &RequestContext,
        pattern: &str,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        search::glob(&self.conn, ctx.workspace_id, pattern.to_string(), page).await
    }

    /// Returns nodes matching bounded metadata predicates.
    pub async fn find(
        &self,
        ctx: &RequestContext,
        query: FindQuery,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        search::find(&self.conn, ctx.workspace_id, query, page).await
    }

    /// Returns bounded literal byte matches from regular files.
    pub async fn search_content(
        &self,
        ctx: &RequestContext,
        query: ContentQuery,
        page: PageRequest,
    ) -> FsResult<Page<SearchMatch>> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        search::search_content(&self.conn, ctx.workspace_id, query, page).await
    }

    /// Returns committed changes after an optional opaque cursor.
    pub async fn changes(
        &self,
        ctx: &RequestContext,
        after: Option<ChangeCursor>,
        page: PageRequest,
    ) -> FsResult<Page<Change>> {
        require_capability(ctx, Capability::Read, ctx.workspace_id)?;
        change::changes(&self.conn, ctx.workspace_id, after, page).await
    }
}

fn actor_json(ctx: &RequestContext) -> String {
    serde_json::to_string(&ctx.actor_metadata).unwrap_or_else(|_| "{}".to_string())
}

fn require_capability(
    ctx: &RequestContext,
    capability: Capability,
    subject: impl std::fmt::Display,
) -> FsResult<()> {
    if ctx.has_capability(capability) {
        Ok(())
    } else {
        Err(FsError::permission_denied(subject))
    }
}
