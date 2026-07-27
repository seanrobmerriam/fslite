mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use fslite_server::{AppState, Ctx};
use tower::ServiceExt;

/// A tiny router that exists only to exercise `Ctx::from_request_parts` in
/// isolation. It is local to this test file and has zero footprint on
/// `fslite_server::app()` — nothing here ships in the real router.
fn probe_router(state: AppState) -> Router {
    Router::new()
        .route("/probe/{workspace_id}", get(probe))
        .with_state(state)
}

async fn probe(Ctx(_ctx): Ctx) -> StatusCode {
    StatusCode::OK
}

#[tokio::test]
async fn missing_bearer_token_is_401() {
    let (state, workspace_id) = support::fixture().await;
    let response = probe_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/probe/{workspace_id}"))
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
    let response = probe_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/probe/{workspace_id}"))
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
    let response = probe_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/probe/{other}"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn valid_token_authenticates_and_reaches_the_handler() {
    let (state, workspace_id) = support::fixture().await;
    let response = probe_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/probe/{workspace_id}"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // A valid token whose workspace matches the path clears `Ctx` and the
    // handler runs, proving the extractor's success path end to end.
    assert_eq!(response.status(), 200);
}
