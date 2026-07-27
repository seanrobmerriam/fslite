//! `fslite_core::FileSystem` trait implementation for [`SqliteFileSystem`].
//!
//! Each method delegates to the identically named inherent method on
//! [`SqliteFileSystem`] where one exists (Rust resolves `self.<name>(..)`
//! calls below to the inherent method, not back into this trait impl).
//! Operations not yet implemented by an earlier task are `unimplemented!()`
//! and will be filled in by the task that owns them.

use async_trait::async_trait;
use fslite_core::{
    BatchOperation, BatchResult, Change, ChangeCursor, ContentQuery, CopyOptions, CreateOptions,
    FileRead, FileSystem, FindQuery, FsResult, LinkTarget, MoveOptions, MutationOptions, Node,
    Page, PageRequest, ReadOptions, RemoveOptions, RequestContext, SearchMatch, StatOptions,
    TouchOptions, TrashEntry, TrashId, TreeEntry, TreeOptions, VirtualPath, WorkspaceUsage,
    WriteOptions, WriteSource,
};

use crate::SqliteFileSystem;

#[async_trait]
impl FileSystem for SqliteFileSystem {
    async fn workspace_usage(&self, ctx: &RequestContext) -> FsResult<WorkspaceUsage> {
        self.workspace_usage(ctx).await
    }

    async fn stat(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<Node> {
        self.stat(ctx, path, options).await
    }

    async fn exists(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<bool> {
        self.exists(ctx, path, options).await
    }

    async fn read_dir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        self.read_dir(ctx, path, page).await
    }

    async fn tree(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TreeOptions,
        page: PageRequest,
    ) -> FsResult<Page<TreeEntry>> {
        self.tree(ctx, path, options, page).await
    }

    async fn mkdir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node> {
        self.mkdir(ctx, path, options).await
    }

    async fn read(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: ReadOptions,
    ) -> FsResult<FileRead> {
        self.read(ctx, path, options).await
    }

    async fn write(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        self.write(ctx, path, source, options).await
    }

    async fn write_at(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        offset: u64,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        self.write_at(ctx, path, offset, source, options).await
    }

    async fn append(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node> {
        self.append(ctx, path, source, options).await
    }

    async fn truncate(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        length: u64,
        options: MutationOptions,
    ) -> FsResult<Node> {
        self.truncate(ctx, path, length, options).await
    }

    async fn touch(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TouchOptions,
    ) -> FsResult<Node> {
        self.touch(ctx, path, options).await
    }

    async fn copy(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: CopyOptions,
    ) -> FsResult<Node> {
        self.copy(ctx, from, to, options).await
    }

    async fn move_path(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: MoveOptions,
    ) -> FsResult<Node> {
        self.move_path(ctx, from, to, options).await
    }

    async fn remove(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: RemoveOptions,
    ) -> FsResult<()> {
        self.remove(ctx, path, options).await
    }

    async fn symlink(
        &self,
        ctx: &RequestContext,
        target: &LinkTarget,
        link: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node> {
        self.symlink(ctx, target, link, options).await
    }

    async fn read_link(&self, ctx: &RequestContext, path: &VirtualPath) -> FsResult<LinkTarget> {
        self.read_link(ctx, path).await
    }

    async fn trash(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: MutationOptions,
    ) -> FsResult<TrashEntry> {
        self.trash(ctx, path, options).await
    }

    async fn list_trash(
        &self,
        ctx: &RequestContext,
        page: PageRequest,
    ) -> FsResult<Page<TrashEntry>> {
        self.list_trash(ctx, page).await
    }

    async fn restore(
        &self,
        ctx: &RequestContext,
        trash: TrashId,
        destination: Option<&VirtualPath>,
        options: MutationOptions,
    ) -> FsResult<Node> {
        self.restore(ctx, trash, destination, options).await
    }

    async fn purge(&self, ctx: &RequestContext, trash: TrashId) -> FsResult<()> {
        self.purge(ctx, trash).await
    }

    async fn set_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        value: &[u8],
        options: MutationOptions,
    ) -> FsResult<Node> {
        self.set_attribute(ctx, path, key, value, options).await
    }

    async fn remove_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        options: MutationOptions,
    ) -> FsResult<Node> {
        self.remove_attribute(ctx, path, key, options).await
    }

    async fn glob(
        &self,
        ctx: &RequestContext,
        pattern: &str,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        self.glob(ctx, pattern, page).await
    }

    async fn find(
        &self,
        ctx: &RequestContext,
        query: FindQuery,
        page: PageRequest,
    ) -> FsResult<Page<Node>> {
        self.find(ctx, query, page).await
    }

    async fn search_content(
        &self,
        ctx: &RequestContext,
        query: ContentQuery,
        page: PageRequest,
    ) -> FsResult<Page<SearchMatch>> {
        self.search_content(ctx, query, page).await
    }

    async fn batch(
        &self,
        ctx: &RequestContext,
        operations: Vec<BatchOperation>,
    ) -> FsResult<Vec<BatchResult>> {
        self.batch(ctx, operations).await
    }

    async fn changes(
        &self,
        ctx: &RequestContext,
        after: Option<ChangeCursor>,
        page: PageRequest,
    ) -> FsResult<Page<Change>> {
        self.changes(ctx, after, page).await
    }
}
