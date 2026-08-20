use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_core::{Capability, FileSystem, RequestContext, WorkspaceId};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

pub const TOKEN: &str = "test-token";

/// Builds an in-memory backend, a trusted workspace, a bearer token that
/// authenticates as that workspace with every capability, and the
/// `AppState` wiring them together.
pub async fn fixture() -> (AppState, WorkspaceId) {
    fixture_with_capabilities(BTreeSet::from([
        Capability::Read,
        Capability::Write,
        Capability::Delete,
        Capability::TrashRestore,
        Capability::WorkspaceAdmin,
    ]))
    .await
}

/// Builds the standard in-memory fixture with a token scoped to the created
/// workspace and exactly the supplied capabilities.
pub async fn fixture_with_capabilities(
    capabilities: BTreeSet<Capability>,
) -> (AppState, WorkspaceId) {
    let sqlite_fs = Arc::new(
        SqliteFileSystem::open_in_memory(Default::default())
            .await
            .unwrap(),
    );
    let workspace = sqlite_fs
        .create_workspace(Default::default())
        .await
        .unwrap();
    let health_workspace = workspace.id;

    let mut tokens = HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities,
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: sqlite_fs.clone() as Arc<dyn FileSystem>,
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        health_workspace,
    };
    (state, workspace.id)
}

#[allow(dead_code)]
pub fn trusted_ctx(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::trusted(workspace_id)
}

/// Captures a node's current revision via a real `GET .../fs/{path}` HTTP
/// call (not a guessed/hardcoded value, and not a direct `state.fs.stat`
/// call) so stale-`expected_revision` tests assert against what the API
/// actually reports, not an assumption about what the first revision number
/// happens to be.
#[allow(dead_code)]
pub async fn current_revision(
    app_router: axum::Router,
    workspace_id: WorkspaceId,
    path: &str,
) -> u64 {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let response = app_router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/fs/{path}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "expected stat to succeed while capturing revision for {path}"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let node: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    node["revision"]
        .as_u64()
        .expect("node JSON has a numeric `revision` field")
}
