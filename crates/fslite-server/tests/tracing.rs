mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn a_client_supplied_request_id_is_echoed_back() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "caller-supplied-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "caller-supplied-id"
    );
}
