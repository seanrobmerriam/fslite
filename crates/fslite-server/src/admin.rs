use std::sync::Arc;

use async_trait::async_trait;
use fslite_core::{FsResult, WorkspaceId};
use fslite_sqlite::{SqliteFileSystem, Workspace};

/// Workspace lifecycle operations. Not part of `fslite_core::FileSystem`
/// (creating/naming a workspace is backend-specific), so `fslite-server`
/// defines its own narrow trait and adapts each backend to it explicitly
/// rather than downcasting `Arc<dyn FileSystem>`.
#[async_trait]
pub trait WorkspaceAdmin: Send + Sync {
    /// Creates a new isolated workspace with default limits.
    async fn create_workspace(&self) -> FsResult<Workspace>;
    /// Permanently deletes a workspace and everything it contains.
    async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()>;
    /// Atomically returns a workspace to its empty initial state.
    async fn reset_workspace(&self, id: WorkspaceId) -> FsResult<()>;
}

/// Adapts [`SqliteFileSystem`]'s inherent workspace methods to [`WorkspaceAdmin`].
pub struct SqliteWorkspaceAdmin(pub Arc<SqliteFileSystem>);

#[async_trait]
impl WorkspaceAdmin for SqliteWorkspaceAdmin {
    async fn create_workspace(&self) -> FsResult<Workspace> {
        self.0.create_workspace(Default::default()).await
    }

    async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()> {
        self.0.delete_workspace(id).await
    }

    async fn reset_workspace(&self, id: WorkspaceId) -> FsResult<()> {
        self.0.reset_workspace(id).await
    }
}
