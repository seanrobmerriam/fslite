use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use fslite_core::{RequestContext, StatOptions, VirtualPath};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub mod batch;
pub mod content;
pub mod directories;
pub mod nodes;
pub mod search;
pub mod trash;
pub mod workspaces;

pub fn health_router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz).fallback(method_not_allowed))
        .route("/readyz", get(readyz).fallback(method_not_allowed))
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

/// axum 0.8 has no single router-level "method not allowed" fallback (unlike
/// older axum's `Router::method_not_allowed_fallback`): a request whose path
/// matches a registered route but whose method doesn't is handled entirely
/// inside that route's `MethodRouter`, before `Router::fallback` ever runs
/// (`Router::fallback` only fires when no path matches at all). So every
/// `.route(...)` registration in this crate chains
/// `.fallback(method_not_allowed)` onto its `MethodRouter`, which axum calls
/// for exactly this "path matched, method didn't" case, keeping the 405
/// response inside the JSON error envelope instead of axum's default empty
/// body.
pub(crate) async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed
}
