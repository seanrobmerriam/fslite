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
async fn restore_and_purge_round_trip() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"x".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    let entry = state
        .fs
        .trash(&ctx, &path, Default::default())
        .await
        .unwrap();

    let list = app(state.clone())
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/trash")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body = list.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::TrashEntry> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);

    let restore = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/trash/{}/restore",
                        entry.id
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), 200);
    assert!(
        state
            .fs
            .exists(&ctx, &path, Default::default())
            .await
            .unwrap()
    );

    let entry2 = state
        .fs
        .trash(&ctx, &path, Default::default())
        .await
        .unwrap();
    let purge = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/v1/workspaces/{workspace_id}/trash/{}", entry2.id)),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(purge.status(), 200);
}

/// The brief's round-trip test only ever restores to the original path (an
/// empty `{}` body). This test closes that gap by exercising the
/// `destination` override, confirming the restored node lands at the new
/// path (and not the original one).
#[tokio::test]
async fn restore_with_destination_override_moves_to_new_path() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let original = VirtualPath::parse("/a.txt").unwrap();
    let destination = VirtualPath::parse("/b.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &original,
            WriteSource::from_bytes(b"x".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    let entry = state
        .fs
        .trash(&ctx, &original, Default::default())
        .await
        .unwrap();

    let restore = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/trash/{}/restore",
                        entry.id
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(json!({ "destination": "/b.txt" }).to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), 200);
    let body = restore.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.name, "b.txt");

    assert!(
        state
            .fs
            .exists(&ctx, &destination, Default::default())
            .await
            .unwrap()
    );
    assert!(
        !state
            .fs
            .exists(&ctx, &original, Default::default())
            .await
            .unwrap()
    );
}

/// `expected_revision` round-tripping was, until this test, exercised on
/// only one route (`write`) across the entire suite. This closes that gap
/// for `restore`. The trashed node's revision is captured from the real
/// `action=trash` HTTP response (`TrashEntry.node.revision`), not guessed.
#[tokio::test]
async fn restore_with_stale_expected_revision_is_412() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"x".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let app_router = app(state.clone());
    let trash_response = app_router
        .clone()
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/fs/a.txt?action=trash"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(json!({}).to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trash_response.status(), 200);
    let body = trash_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let entry: fslite_core::TrashEntry = serde_json::from_slice(&body).unwrap();
    let stale_revision = entry.node.revision.get() + 1;

    let restore = app_router
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/trash/{}/restore",
                        entry.id
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(
                json!({ "expected_revision": stale_revision }).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), 412);
    let body = restore.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "revision_conflict");

    // The stale request must not have restored the node.
    assert!(
        !state
            .fs
            .exists(&ctx, &path, Default::default())
            .await
            .unwrap()
    );
}
