//! Trash routes: `list_trash`, `restore`, `purge`.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{PageRequest, TrashId, VirtualPath, WorkspaceId};
use serde::Deserialize;

use crate::dto::query_u32;
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces/{workspace_id}/trash", get(list_trash))
        .route(
            "/v1/workspaces/{workspace_id}/trash/{trash_id}",
            axum::routing::delete(purge),
        )
        .route(
            "/v1/workspaces/{workspace_id}/trash/{trash_id}/restore",
            axum::routing::post(restore),
        )
}

async fn list_trash(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<fslite_core::Page<fslite_core::TrashEntry>>, ApiError> {
    let page = PageRequest::default()
        .cursor(params.get("cursor").cloned())
        .limit(query_u32(&params, "limit", fslite_core::DEFAULT_PAGE_LIMIT)?);
    Ok(Json(state.fs.list_trash(&ctx, page).await?))
}

fn parse_trash_id(raw: &str) -> Result<TrashId, ApiError> {
    TrashId::parse(raw).map_err(|_| ApiError::MalformedBody("invalid trash id".into()))
}

async fn purge(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, trash_id)): Path<(WorkspaceId, String)>,
) -> Result<StatusCode, ApiError> {
    let trash_id = parse_trash_id(&trash_id)?;
    state.fs.purge(&ctx, trash_id).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RestoreBody {
    destination: Option<String>,
    expected_revision: Option<u64>,
}

async fn restore(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, trash_id)): Path<(WorkspaceId, String)>,
    body: axum::body::Bytes,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let trash_id = parse_trash_id(&trash_id)?;
    let parsed: RestoreBody = if body.is_empty() {
        RestoreBody { destination: None, expected_revision: None }
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?
    };
    let destination = parsed
        .destination
        .map(|raw| VirtualPath::parse(&raw))
        .transpose()
        .map_err(|e| ApiError::MalformedBody(e.message().to_string()))?;
    let expected_revision = match parsed.expected_revision {
        None => None,
        Some(0) => return Err(ApiError::MalformedBody("expected_revision must be nonzero".into())),
        Some(v) => fslite_core::Revision::new(v),
    };
    let options = fslite_core::MutationOptions::default().expected_revision(expected_revision);
    Ok(Json(
        state
            .fs
            .restore(&ctx, trash_id, destination.as_ref(), options)
            .await?,
    ))
}
