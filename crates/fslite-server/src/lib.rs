//! HTTP adapter exposing `fslite_core::FileSystem` as a resource-oriented API.

mod error;
mod routes;
mod state;

use axum::Router;

pub use error::ApiError;
pub use state::AppState;

/// Builds the complete application router from shared state.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_router())
        .with_state(state)
}
