//! Node metadata routes: `stat` (GET), `exists` (HEAD), `remove` (DELETE).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{RemoveOptions, StatOptions, VirtualPath};

use crate::dto::{query_bool, query_revision};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/fs/{*path}",
        get(stat).head(exists).delete(remove),
    )
}

fn parse_path(raw: &str) -> Result<VirtualPath, ApiError> {
    VirtualPath::parse(&format!("/{raw}"))
        .map_err(|err| ApiError::MalformedBody(err.message().to_string()))
}

async fn stat(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    let follow_symlinks = query_bool(&params, "follow_symlinks", true)?;
    let node = state
        .fs
        .stat(&ctx, &path, StatOptions::default().follow_symlinks(follow_symlinks))
        .await?;
    Ok(Json(node))
}

async fn exists(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let path = parse_path(&path)?;
    let follow_symlinks = query_bool(&params, "follow_symlinks", true)?;
    let found = state
        .fs
        .exists(&ctx, &path, StatOptions::default().follow_symlinks(follow_symlinks))
        .await?;
    Ok(if found { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

async fn remove(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let path = parse_path(&path)?;
    let recursive = query_bool(&params, "recursive", false)?;
    let expected_revision = query_revision(&params)?;
    state
        .fs
        .remove(
            &ctx,
            &path,
            RemoveOptions::default()
                .recursive(recursive)
                .expected_revision(expected_revision),
        )
        .await?;
    Ok(StatusCode::OK)
}
