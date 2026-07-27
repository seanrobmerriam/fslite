use axum::Json;
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::state::AppState;

pub fn health_router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
