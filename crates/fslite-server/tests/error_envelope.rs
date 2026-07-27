use fslite_core::FsError;
use fslite_server::ApiError;
use axum::response::IntoResponse;
use http_body_util::BodyExt;

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
