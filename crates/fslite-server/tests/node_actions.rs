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

async fn seed_file(state: &fslite_server::AppState, workspace_id: fslite_core::WorkspaceId, path: &str) {
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(&ctx, &VirtualPath::parse(path).unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn action_copy_duplicates_a_file() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt?action=copy"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"to": "/b.txt"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(state.fs.exists(&ctx, &VirtualPath::parse("/b.txt").unwrap(), Default::default()).await.unwrap());
}

#[tokio::test]
async fn action_move_relocates_a_file() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt?action=move"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"to": "/c.txt"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(!state.fs.exists(&ctx, &VirtualPath::parse("/a.txt").unwrap(), Default::default()).await.unwrap());
    assert!(state.fs.exists(&ctx, &VirtualPath::parse("/c.txt").unwrap(), Default::default()).await.unwrap());
}

#[tokio::test]
async fn action_trash_moves_a_file_to_trash() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt?action=trash"))
                .header("content-type", "application/json"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let entry: fslite_core::TrashEntry = serde_json::from_slice(&body).unwrap();
    assert_eq!(entry.original_path.as_str(), "/a.txt");
}

#[tokio::test]
async fn patch_touch_creates_a_missing_file() {
    let (state, workspace_id) = support::fixture().await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PATCH)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/new.txt"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"op": "touch", "create": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(state.fs.exists(&ctx, &VirtualPath::parse("/new.txt").unwrap(), Default::default()).await.unwrap());
}

#[tokio::test]
async fn patch_set_attribute_round_trips_arbitrary_bytes() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;
    let value = base64::engine::general_purpose::STANDARD.encode(b"\x00\x01binary");

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PATCH)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"op": "set_attribute", "key": "k", "value_base64": value}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert!(node.attributes.contains_key("k"));
}

#[tokio::test]
async fn patch_remove_attribute_deletes_a_previously_set_attribute() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .set_attribute(&ctx, &VirtualPath::parse("/a.txt").unwrap(), "k", b"v", Default::default())
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PATCH)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"op": "remove_attribute", "key": "k"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert!(!node.attributes.contains_key("k"));
}

#[tokio::test]
async fn link_target_returns_the_stored_target_without_resolving_it() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    seed_file(&state, workspace_id, "/target.txt").await;
    state
        .fs
        .symlink(
            &ctx,
            &fslite_core::LinkTarget::parse("/target.txt").unwrap(),
            &VirtualPath::parse("/link").unwrap(),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/link/link-target")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["target"], "/target.txt");
}
