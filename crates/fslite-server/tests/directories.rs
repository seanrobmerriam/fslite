mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{NodeKind, RequestContext, VirtualPath, WriteSource};
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
            auth(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/fs/docs?type=directory"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(
                json!({"parents": true, "exist_ok": false}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(
        state
            .fs
            .exists(
                &ctx,
                &VirtualPath::parse("/docs").unwrap(),
                Default::default()
            )
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn children_lists_direct_descendants() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"x".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!(
                "/v1/workspaces/{workspace_id}/directories//children"
            )))
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

#[tokio::test]
async fn put_with_type_symlink_creates_a_symlink() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/fs/link?type=symlink"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(
                json!({"target": "/docs/readme.txt"}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.name, "link");
    assert_eq!(node.kind, NodeKind::Symlink);

    let ctx = RequestContext::trusted(workspace_id);
    let stored_target = state
        .fs
        .read_link(&ctx, &VirtualPath::parse("/link").unwrap())
        .await
        .unwrap();
    assert_eq!(stored_target.as_str(), "/docs/readme.txt");
}

#[tokio::test]
async fn tree_lists_a_nested_file_with_its_depth() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .mkdir(
            &ctx,
            &VirtualPath::parse("/docs").unwrap(),
            Default::default(),
        )
        .await
        .unwrap();
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/docs/readme.txt").unwrap(),
            WriteSource::from_bytes(b"hi".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(
                Request::builder().uri(format!("/v1/workspaces/{workspace_id}/directories//tree")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::TreeEntry> = serde_json::from_slice(&body).unwrap();

    let docs = page
        .items
        .iter()
        .find(|entry| entry.path.as_str() == "/docs")
        .expect("tree includes the /docs directory");
    assert_eq!(docs.depth, 1);
    assert_eq!(docs.node.kind, NodeKind::Directory);

    let readme = page
        .items
        .iter()
        .find(|entry| entry.path.as_str() == "/docs/readme.txt")
        .expect("tree includes the nested file");
    assert_eq!(readme.depth, 2);
    assert_eq!(readme.node.kind, NodeKind::File);
}

#[tokio::test]
async fn dispatch_rejects_a_suffix_that_is_neither_children_nor_tree() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!(
                "/v1/workspaces/{workspace_id}/directories/docs/bogus"
            )))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "route_not_found");
}
