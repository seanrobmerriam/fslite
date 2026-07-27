mod support;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use fslite_core::{Capability, FileSystem, WorkspaceId};
use fslite_server::{
    AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin, app,
};
use fslite_sqlite::SqliteFileSystem;
use http_body_util::BodyExt;
use serde::Deserialize;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

/// Mirrors `routes::workspaces::WorkspaceDto`'s wire shape so tests can parse
/// the response without depending on a private type.
#[derive(Deserialize)]
struct WorkspaceDto {
    id: WorkspaceId,
    created_at_ms: i64,
    max_bytes: u64,
    max_nodes: u64,
    max_file_bytes: u64,
}

#[tokio::test]
async fn usage_reports_active_node_count() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/usage")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let usage: fslite_core::WorkspaceUsage = serde_json::from_slice(&body).unwrap();
    assert_eq!(usage.active_nodes, 1); // just the workspace root
}

#[tokio::test]
async fn create_workspace_returns_a_new_workspace() {
    let (state, existing_workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().method("POST").uri("/v1/workspaces"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let workspace: WorkspaceDto = serde_json::from_slice(&body).unwrap();

    // A genuinely new workspace, not the fixture's own.
    assert_ne!(workspace.id, existing_workspace_id);
    assert!(workspace.created_at_ms > 0);

    // Created with the backend's default limits.
    let defaults = fslite_sqlite::WorkspaceOptions::default();
    assert_eq!(workspace.max_bytes, defaults.max_bytes);
    assert_eq!(workspace.max_nodes, defaults.max_nodes);
    assert_eq!(workspace.max_file_bytes, defaults.max_file_bytes);
}

#[tokio::test]
async fn create_workspace_without_workspace_admin_capability_is_forbidden() {
    let sqlite_fs = Arc::new(
        SqliteFileSystem::open_in_memory(Default::default())
            .await
            .unwrap(),
    );
    let workspace = sqlite_fs
        .create_workspace(Default::default())
        .await
        .unwrap();

    let limited_token = "limited-token";
    let mut tokens = HashMap::new();
    tokens.insert(
        limited_token.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            // Deliberately missing `WorkspaceAdmin`.
            capabilities: BTreeSet::from([Capability::Read, Capability::Write]),
            actor_metadata: Default::default(),
        },
    );
    let state = AppState {
        fs: sqlite_fs.clone() as Arc<dyn FileSystem>,
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        health_workspace: workspace.id,
    };

    let response = app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/workspaces")
                .header("authorization", format!("Bearer {limited_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn delete_workspace_removes_it() {
    let (state, workspace_id) = support::fixture().await;
    let router = app(state);

    let response = router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/workspaces/{workspace_id}")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // The workspace is actually gone: the same token can no longer read its
    // usage, because there's nothing left to read.
    let response = router
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/usage")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), 200);
}

#[tokio::test]
async fn delete_workspace_token_for_a_different_workspace_is_forbidden() {
    let (state, _workspace_id) = support::fixture().await;
    let other = WorkspaceId::new();
    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/workspaces/{other}")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}
