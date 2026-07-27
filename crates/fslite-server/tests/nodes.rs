mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn stat_returns_node_json_for_an_existing_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"hi".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.name, "a.txt");
    assert_eq!(node.logical_size, 2);
}

#[tokio::test]
async fn stat_missing_path_is_404_with_not_found_code() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/missing.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn head_reports_existence_with_no_body() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::HEAD)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/missing.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn delete_removes_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(&ctx, &path, WriteSource::from_bytes(b"hi".to_vec()), Default::default())
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    assert!(!state.fs.exists(&ctx, &path, Default::default()).await.unwrap());
}
