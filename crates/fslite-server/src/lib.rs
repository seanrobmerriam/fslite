//! HTTP adapter exposing `fslite_core::FileSystem` as a resource-oriented API.

mod auth;
mod error;
mod routes;
mod state;

use axum::routing::any;
use axum::Router;

pub use auth::{AuthProvider, AuthenticatedActor, BearerTokenAuthProvider, Ctx};
pub use error::ApiError;
pub use state::AppState;

/// Builds the complete application router from shared state.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_router())
        .route("/v1/workspaces/{workspace_id}/usage", any(usage_placeholder))
        .with_state(state)
}

/// Temporary stand-in for `GET /v1/workspaces/{workspace_id}/usage`, which a
/// later task (workspace admin routes) replaces with the real handler.
///
/// It exists so [`Ctx`] — authentication plus the workspace-match check —
/// runs end-to-end through the real router today, which is what
/// `tests/auth.rs` exercises. Once the real `/usage` route is registered,
/// **this route registration must be deleted**: axum panics at router-build
/// time on two handlers registered for the same path and method.
async fn usage_placeholder(_ctx: Ctx) -> ApiError {
    ApiError::RouteNotFound
}
