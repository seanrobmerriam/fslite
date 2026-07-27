mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn readyz_succeeds_once_the_backend_is_reachable() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}
