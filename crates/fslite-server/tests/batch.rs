mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn batch_runs_operations_atomically_and_reports_the_failing_index() {
    let (state, workspace_id) = support::fixture().await;

    let ops = json!({
        "operations": [
            {"mkdir": {"path": "/a", "options": {}}},
            {"mkdir": {"path": "/a", "options": {}}}
        ]
    });

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/v1/workspaces/{workspace_id}/batch"))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(ops.to_string()))
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409); // AlreadyExists on the second mkdir
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["details"]["index"], 1);
}

/// `batch` used to take axum's `Json<T>` extractor directly, so a body that
/// failed to deserialize produced axum's built-in 422 `text/plain` rejection
/// instead of the crate's JSON error envelope. It now buffers `Bytes` and
/// deserializes manually via `serde_json::from_slice`, mapping failures to
/// `ApiError::MalformedBody` (400) like every other handler.
#[tokio::test]
async fn malformed_body_returns_the_json_envelope_not_a_422_text_plain_rejection() {
    let (state, workspace_id) = support::fixture().await;

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/v1/workspaces/{workspace_id}/batch"))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(r#"{"operations": [{"mkdir": {"#)) // truncated/incomplete JSON
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "malformed_body");
}
