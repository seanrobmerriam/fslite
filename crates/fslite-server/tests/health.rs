use fslite_server::{app, AppState};
use fslite_core::WorkspaceId;
use fslite_sqlite::SqliteFileSystem;
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok_without_touching_the_backend() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let state = AppState {
        fs: Arc::new(fs),
        health_workspace: WorkspaceId::new(),
    };

    let response = app(state)
        .oneshot(
            axum::http::Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
}
