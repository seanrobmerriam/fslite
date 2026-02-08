use fslite_conformance::ConformanceFactory;
use fslite_core::{FileSystem, WorkspaceId};
use fslite_sqlite::SqliteFileSystem;
use tokio::sync::Mutex;

/// Supplies fresh, in-memory SQLite backends and workspaces to the
/// backend-agnostic conformance suite.
///
/// Workspace creation is a `SqliteFileSystem`-specific inherent method, not
/// part of the canonical `FileSystem` contract, so `fresh` creates both the
/// backend and its workspace and stashes the resulting id here for the
/// immediately following `workspace` call to pick up (see
/// `ConformanceFactory`'s documentation for why this pairing is safe).
#[derive(Default)]
struct SqliteConformanceFactory {
    last_workspace: Mutex<Option<WorkspaceId>>,
}

#[async_trait::async_trait]
impl ConformanceFactory for SqliteConformanceFactory {
    async fn fresh(&self) -> Box<dyn FileSystem> {
        let fs = SqliteFileSystem::open_in_memory(Default::default())
            .await
            .expect("in-memory SQLite database always opens");
        let workspace = fs
            .create_workspace(Default::default())
            .await
            .expect("workspace creation always succeeds on a fresh database");
        *self.last_workspace.lock().await = Some(workspace.id);
        Box::new(fs)
    }

    async fn workspace(&self, _fs: &dyn FileSystem) -> WorkspaceId {
        self.last_workspace
            .lock()
            .await
            .expect("fresh() must be called before workspace()")
    }
}

#[tokio::test]
async fn sqlite_backend_satisfies_the_conformance_suite() {
    let factory = SqliteConformanceFactory::default();
    fslite_conformance::run_conformance(&factory).await;
}

