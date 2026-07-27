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
async fn put_then_get_round_trips_bytes() {
    let (state, workspace_id) = support::fixture().await;

    let put = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt")),
            )
            .body(Body::from("hello world"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), 200);

    let get = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/content/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    assert_eq!(get.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(get.headers().get("content-length").unwrap(), "11");
    assert!(get.headers().get("content-range").is_none());
    let body = get.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello world");
}

#[tokio::test]
async fn get_with_range_header_returns_206_and_a_slice() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                    .header("range", "bytes=0-4"),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(
        response.headers().get("content-range").unwrap(),
        "bytes 0-4/11"
    );
    assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn get_with_unsatisfiable_range_returns_416_with_content_range() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                    .header("range", "bytes=200-300"),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 416);
    assert_eq!(
        response.headers().get("content-range").unwrap(),
        "bytes */11"
    );
}

#[tokio::test]
async fn get_with_malformed_range_returns_400() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                    .header("range", "bytes=0-9,20-29"),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn action_append_extends_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello ".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder().method(Method::POST).uri(format!(
                "/v1/workspaces/{workspace_id}/content/a.txt?action=append"
            )))
            .body(Body::from("world"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let read = state
        .fs
        .read(&ctx, &path, Default::default())
        .await
        .unwrap();
    let mut stream = read.into_stream();
    let mut bytes = Vec::new();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(bytes, b"hello world");
}

#[tokio::test]
async fn action_truncate_shortens_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/content/a.txt?action=truncate"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(serde_json::json!({"length": 5}).to_string()))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.logical_size, 5);
}

#[tokio::test]
async fn patch_write_at_writes_bytes_at_an_offset() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder().method(Method::PATCH).uri(format!(
                "/v1/workspaces/{workspace_id}/content/a.txt?offset=6"
            )))
            .body(Body::from("Rust!"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let read = state
        .fs
        .read(&ctx, &path, Default::default())
        .await
        .unwrap();
    let mut stream = read.into_stream();
    let mut bytes = Vec::new();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(bytes, b"hello Rust!");
}

#[tokio::test]
async fn write_at_without_offset_query_param_is_rejected() {
    let (state, workspace_id) = support::fixture().await;

    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt")),
            )
            .body(Body::from("data"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

// --- expected_revision round-trip regression tests ------------------------
//
// `expected_revision` was, until this fix wave, exercised end-to-end on
// only one route (`write`, in `tests/contract.rs`) across the entire suite —
// the systemic gap that let `append` (below) silently drop it in the first
// place. These tests close that gap for `content`'s remaining
// revision-aware routes.

/// Doubles as the regression test for the `append` fix itself: `append`
/// used to build `WriteOptions::default()` without ever reading
/// `expected_revision` from the query string, so a stale value was silently
/// ignored instead of being rejected with 412.
#[tokio::test]
async fn action_append_with_stale_expected_revision_is_412() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello ".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let revision = support::current_revision(app(state.clone()), workspace_id, "a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder().method(Method::POST).uri(format!(
                "/v1/workspaces/{workspace_id}/content/a.txt?action=append&expected_revision={}",
                revision + 1
            )))
            .body(Body::from("world"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 412);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "revision_conflict");

    // The stale append must not have gone through.
    let read = state
        .fs
        .read(&ctx, &path, Default::default())
        .await
        .unwrap();
    let mut stream = read.into_stream();
    let mut bytes = Vec::new();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(bytes, b"hello ");
}

#[tokio::test]
async fn action_truncate_with_stale_expected_revision_is_412() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let revision = support::current_revision(app(state.clone()), workspace_id, "a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/content/a.txt?action=truncate"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(
                serde_json::json!({"length": 5, "expected_revision": revision + 1}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 412);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "revision_conflict");

    let node = state
        .fs
        .stat(&ctx, &path, Default::default())
        .await
        .unwrap();
    assert_eq!(
        node.logical_size, 11,
        "the stale truncate must not have gone through"
    );
}

#[tokio::test]
async fn patch_write_at_with_stale_expected_revision_is_412() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let revision = support::current_revision(app(state.clone()), workspace_id, "a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder().method(Method::PATCH).uri(format!(
                "/v1/workspaces/{workspace_id}/content/a.txt?offset=6&expected_revision={}",
                revision + 1
            )))
            .body(Body::from("Rust!"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 412);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "revision_conflict");

    let read = state
        .fs
        .read(&ctx, &path, Default::default())
        .await
        .unwrap();
    let mut stream = read.into_stream();
    let mut bytes = Vec::new();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(
        bytes, b"hello world",
        "the stale write_at must not have gone through"
    );
}

/// Fix 6 regression: `?action=truncate` used to buffer the request body with
/// `axum::body::to_bytes(body, usize::MAX)` — no size limit — even though
/// its JSON payload is at most a few dozen bytes. It's now capped at 64 KiB;
/// this sends a body well over that cap and confirms it's rejected with 413
/// instead of being buffered without bound.
#[tokio::test]
async fn action_truncate_with_an_oversized_body_is_413() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(
            &ctx,
            &path,
            WriteSource::from_bytes(b"hello world".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    // Valid JSON, but padded well past the 64 KiB cap with a `padding`
    // field the DTO doesn't recognize — `to_bytes`'s limit is enforced
    // during buffering, before deserialization ever gets a chance to
    // reject the unknown field.
    let oversized_padding = "x".repeat(100 * 1024);
    let body = serde_json::json!({ "length": 5, "padding": oversized_padding }).to_string();

    let response = app(state.clone())
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/v1/workspaces/{workspace_id}/content/a.txt?action=truncate"
                    ))
                    .header("content-type", "application/json"),
            )
            .body(Body::from(body))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
}

#[tokio::test]
async fn post_action_without_a_recognized_action_is_rejected() {
    let (state, workspace_id) = support::fixture().await;

    let response = app(state)
        .oneshot(
            auth(Request::builder().method(Method::POST).uri(format!(
                "/v1/workspaces/{workspace_id}/content/a.txt?action=bogus"
            )))
            .body(Body::from("data"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
