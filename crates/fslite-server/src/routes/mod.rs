use axum::Json;
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use fslite_core::{RequestContext, StatOptions, VirtualPath};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub mod directories;
pub mod nodes;

pub fn health_router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn readyz(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = RequestContext::trusted(state.health_workspace);
    state
        .fs
        .exists(&ctx, &VirtualPath::root(), StatOptions::default())
        .await?;
    Ok(Json(json!({ "status": "ready" })))
}
