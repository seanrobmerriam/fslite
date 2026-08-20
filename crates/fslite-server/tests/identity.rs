mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn me_returns_safe_authenticated_identity() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/v1/me")
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["workspace_id"], workspace_id.to_string());
    assert!(value["capabilities"].as_array().unwrap().len() >= 5);
    assert!(value.get("actor_metadata").is_none());
    assert!(!String::from_utf8_lossy(&bytes).contains(support::TOKEN));
}

#[tokio::test]
async fn me_requires_a_bearer_token() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/v1/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
