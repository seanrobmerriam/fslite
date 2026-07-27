use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_core::{Capability, RequestContext, WorkspaceId};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider};
use fslite_sqlite::SqliteFileSystem;

pub const TOKEN: &str = "test-token";

/// Builds an in-memory backend, a trusted workspace, a bearer token that
/// authenticates as that workspace with every capability, and the
/// `AppState` wiring them together.
pub async fn fixture() -> (AppState, WorkspaceId) {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let health_workspace = workspace.id;

    let mut tokens = HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: BTreeSet::from([
                Capability::Read,
                Capability::Write,
                Capability::Delete,
                Capability::TrashRestore,
                Capability::WorkspaceAdmin,
            ]),
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: Arc::new(fs),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace,
    };
    (state, workspace.id)
}

#[allow(dead_code)]
pub fn trusted_ctx(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::trusted(workspace_id)
}
