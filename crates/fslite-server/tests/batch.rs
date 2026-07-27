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
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/batch"))
                .header("content-type", "application/json"))
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
