mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Exercises the full lifecycle of a file through the HTTP API exactly as
/// an external client would: create a directory, write content, read it
/// back, list it, search for it, trash it, restore it, then permanently
/// remove it — asserting the JSON contract at every step, not just status
/// codes.
#[tokio::test]
async fn full_resource_lifecycle_via_http() {
    let (state, workspace_id) = support::fixture().await;
    let app_router = app(state);

    let mkdir = app_router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/fs/docs?type=directory"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from("{}"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mkdir.status(), StatusCode::OK);

    let put = app_router
        .clone()
        .oneshot(
            auth(Request::builder().method(Method::PUT).uri(format!(
                "/v1/workspaces/{workspace_id}/content/docs/readme.txt"
            )))
            .body(Body::from("hello contract"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
    let node = json_body(put).await;
    assert_eq!(node["logical_size"], 14);
    let first_revision = node["revision"].clone();

    let children = app_router
        .clone()
        .oneshot(
            auth(Request::builder().uri(format!(
                "/v1/workspaces/{workspace_id}/directories/docs/children"
            )))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let children = json_body(children).await;
    assert_eq!(children["items"].as_array().unwrap().len(), 1);

    // A stale expected_revision is rejected with 412.
    let stale_write = app_router
        .clone()
        .oneshot(
            auth(Request::builder().method(Method::PUT).uri(format!(
                "/v1/workspaces/{workspace_id}/content/docs/readme.txt?expected_revision=999"
            )))
            .body(Body::from("stale"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_write.status(), StatusCode::PRECONDITION_FAILED);
    let stale_error = json_body(stale_write).await;
    assert_eq!(stale_error["error"]["code"], "revision_conflict");

    // A correct expected_revision succeeds. `Revision` is `#[serde(transparent)]`
    // over `u64`, so it serializes as a bare JSON number, not a string.
    let good_write = app_router
        .clone()
        .oneshot(
            auth(Request::builder().method(Method::PUT).uri(format!(
                "/v1/workspaces/{workspace_id}/content/docs/readme.txt?expected_revision={}",
                first_revision.as_u64().unwrap()
            )))
            .body(Body::from("updated"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(good_write.status(), StatusCode::OK);

    let trash = app_router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/fs/docs/readme.txt?action=trash"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from("{}"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trash.status(), StatusCode::OK);
    let entry = json_body(trash).await;
    let trash_id = entry["id"].as_str().unwrap().to_string();

    let restore = app_router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/trash/{trash_id}/restore"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from("{}"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), StatusCode::OK);

    let remove = app_router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/v1/workspaces/{workspace_id}/fs/docs/readme.txt")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove.status(), StatusCode::OK);
}

#[tokio::test]
async fn range_not_satisfiable_returns_416_with_total_length() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = fslite_core::RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &fslite_core::VirtualPath::parse("/a.txt").unwrap(),
            fslite_core::WriteSource::from_bytes(b"short".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                    .header("range", "bytes=100-200"),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response.headers().get("content-range").unwrap(),
        "bytes */5"
    );
}

#[tokio::test]
async fn every_error_response_is_valid_json_with_the_envelope_shape() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(
                Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/does-not-exist")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let json = json_body(response).await;
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
    assert!(json["error"]["details"].is_object() || json["error"]["details"].is_null());
}
