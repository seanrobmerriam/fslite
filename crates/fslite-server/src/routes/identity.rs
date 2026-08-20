use std::collections::BTreeSet;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{Capability, WorkspaceId};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(serde::Serialize)]
struct IdentityDto {
    workspace_id: WorkspaceId,
    capabilities: BTreeSet<Capability>,
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/me",
        get(me).fallback(crate::routes::method_not_allowed),
    )
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<IdentityDto>, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    Ok(Json(IdentityDto {
        workspace_id: actor.workspace_id,
        capabilities: actor.capabilities,
    }))
}
