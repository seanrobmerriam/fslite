mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn missing_bearer_token_is_401() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn unknown_token_is_401() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .header("authorization", "Bearer nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn token_for_a_different_workspace_is_403() {
    let (state, _workspace_id) = support::fixture().await;
    let other = fslite_core::WorkspaceId::new();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{other}/usage"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn valid_token_reaches_the_handler() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // The usage route itself is built in Task 13; until then this asserts
    // "not 401/403", i.e. auth passed and routing took over — expect 404
    // (no route yet) rather than an auth failure.
    assert_eq!(response.status(), 404);
}
