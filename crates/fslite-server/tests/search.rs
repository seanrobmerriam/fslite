mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use base64::Engine;
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn glob_finds_matching_paths() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/search/glob?pattern=/*.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Node> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn find_accepts_the_core_query_shape_directly() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();
    // A non-matching sibling proves `name_contains` is actually applied by
    // the handler, rather than the endpoint just echoing back every node.
    state.fs.write(&ctx, &VirtualPath::parse("/zzz.txt").unwrap(), WriteSource::from_bytes(b"y".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/search/find"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"query": {"root": "/", "name_contains": "a"}, "page": {}}).to_string()))
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

#[tokio::test]
async fn search_content_base64_encodes_needle_and_preview() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"hello world".to_vec()), Default::default()).await.unwrap();
    let needle = base64::engine::general_purpose::STANDARD.encode(b"world");

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/search/content"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"root": "/", "needle_base64": needle, "page": {}}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert!(json["items"][0]["preview_base64"].is_string());
}

#[tokio::test]
async fn changes_lists_committed_mutations() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/changes"))).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Change> = serde_json::from_slice(&body).unwrap();
    assert!(!page.items.is_empty());
}
