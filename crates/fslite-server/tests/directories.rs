mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn put_with_type_directory_creates_a_directory() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/docs?type=directory"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"parents": true, "exist_ok": false}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(state
        .fs
        .exists(&ctx, &VirtualPath::parse("/docs").unwrap(), Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn children_lists_direct_descendants() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default())
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/directories//children")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Node> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name, "a.txt");
}
