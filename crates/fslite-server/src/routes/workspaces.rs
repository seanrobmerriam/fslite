//! Workspace admin routes: `create_workspace`, `delete_workspace`, `workspace_usage`.
//!
//! `create`/`delete` authenticate directly against `state.auth` instead of
//! going through the `Ctx` extractor: `create` has no workspace in the URL to
//! match against yet, and `delete`'s workspace-match check has a different
//! shape (it must also require `Capability::WorkspaceAdmin`, which `Ctx`
//! doesn't enforce). `usage` is ordinary workspace-scoped data access, so it
//! uses `Ctx` like every other route.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::WorkspaceId;
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces", axum::routing::post(create_workspace))
        .route(
            "/v1/workspaces/{workspace_id}",
            axum::routing::delete(delete_workspace),
        )
        .route("/v1/workspaces/{workspace_id}/usage", get(usage))
}

/// The wire shape of a created workspace: `fslite_sqlite::Workspace` does not
/// derive `Serialize`, so this mirrors its public fields.
#[derive(Serialize)]
struct WorkspaceDto {
    id: WorkspaceId,
    created_at_ms: i64,
    max_bytes: u64,
    max_nodes: u64,
    max_file_bytes: u64,
}

async fn create_workspace(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<WorkspaceDto>, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    if !actor.capabilities.contains(&fslite_core::Capability::WorkspaceAdmin) {
        return Err(ApiError::Domain(fslite_core::FsError::permission_denied("create_workspace")));
    }
    let workspace = state.admin.create_workspace().await?;
    Ok(Json(WorkspaceDto {
        id: workspace.id,
        created_at_ms: workspace.created_at_ms,
        max_bytes: workspace.max_bytes,
        max_nodes: workspace.max_nodes,
        max_file_bytes: workspace.max_file_bytes,
    }))
}

async fn delete_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<WorkspaceId>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    if actor.workspace_id != workspace_id || !actor.capabilities.contains(&fslite_core::Capability::WorkspaceAdmin) {
        return Err(ApiError::WorkspaceMismatch);
    }
    state.admin.delete_workspace(workspace_id).await?;
    Ok(StatusCode::OK)
}

async fn usage(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
) -> Result<Json<fslite_core::WorkspaceUsage>, ApiError> {
    Ok(Json(state.fs.workspace_usage(&ctx).await?))
}
