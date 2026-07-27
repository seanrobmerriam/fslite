//! HTTP adapter exposing `fslite_core::FileSystem` as a resource-oriented API.

mod admin;
mod auth;
mod dto;
mod error;
pub mod range;
mod routes;
mod state;
mod tracing_mw;

use axum::middleware;
use axum::Router;

pub use admin::{SqliteWorkspaceAdmin, WorkspaceAdmin};
pub use auth::{AuthProvider, AuthenticatedActor, BearerTokenAuthProvider, Ctx};
pub use error::ApiError;
pub use state::AppState;
pub use tracing_mw::RequestId;

/// Builds the complete application router from shared state.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_router())
        .merge(routes::nodes::router())
        .merge(routes::directories::router())
        .merge(routes::trash::router())
        .merge(routes::content::router())
        .merge(routes::search::router())
        .with_state(state)
        .layer(middleware::from_fn(tracing_mw::request_id))
        .layer(tracing_mw::trace_layer())
}
