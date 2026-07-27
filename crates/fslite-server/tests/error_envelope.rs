mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use axum::response::IntoResponse;
use fslite_core::FsError;
use fslite_server::{ApiError, app};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn not_found_maps_to_404_with_stable_code() {
    let err: ApiError = FsError::not_found("/a/b.txt").into();
    let response = err.into_response();
    assert_eq!(response.status(), 404);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "not_found");
    assert_eq!(json["error"]["message"], "not found: /a/b.txt");
}

#[tokio::test]
async fn revision_conflict_maps_to_412() {
    let err: ApiError = FsError::revision_conflict("/a").into();
    assert_eq!(err.into_response().status(), 412);
}

#[tokio::test]
async fn invalid_range_maps_to_416() {
    let err: ApiError = FsError::invalid_range("/a").into();
    assert_eq!(err.into_response().status(), 416);
}

#[tokio::test]
async fn quota_exceeded_maps_to_409() {
    let err: ApiError = FsError::quota_exceeded("/a").into();
    assert_eq!(err.into_response().status(), 409);
}

#[tokio::test]
async fn permission_denied_maps_to_403() {
    let err: ApiError = FsError::permission_denied("/a").into();
    assert_eq!(err.into_response().status(), 403);
}

#[tokio::test]
async fn storage_busy_maps_to_503_with_retry_after() {
    let err: ApiError = FsError::storage_busy("db").into();
    let response = err.into_response();
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers().get("retry-after").unwrap(), "1");
}

#[tokio::test]
async fn unauthenticated_has_its_own_envelope() {
    let response = ApiError::Unauthenticated("missing bearer token".into()).into_response();
    assert_eq!(response.status(), 401);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "unauthenticated");
}

/// Live-verified gap: previously `GET /v1/nope` returned an empty-bodied 404
/// with no `content-type`, because `app()` never registered a
/// `Router::fallback`. Now every unmatched route renders through the same
/// JSON envelope as any other error.
#[tokio::test]
async fn unmatched_route_returns_404_through_the_json_envelope() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/v1/nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "route_not_found");
}

/// Live-verified gap: previously `POST /v1/workspaces/{id}/trash` (a route
/// that only registers `GET`) returned an empty-bodied 405. axum 0.8 has no
/// single router-level method-not-allowed fallback, so each route's own
/// `MethodRouter` now carries `.fallback(method_not_allowed)`, which routes
/// this case through `ApiError` instead of axum's bare default response.
#[tokio::test]
async fn unsupported_method_on_a_real_route_returns_405_through_the_json_envelope() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/v1/workspaces/{workspace_id}/trash")),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 405);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "method_not_allowed");
}
