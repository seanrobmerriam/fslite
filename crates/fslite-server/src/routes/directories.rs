//! Directory listing routes: `read_dir` (`/children`) and `tree` (`/tree`).
//!
//! Both are served by a single wildcard route because axum's `{*path}`
//! segment swallows any trailing literal path components too — there is no
//! way to register `/directories/{*path}/children` and
//! `/directories/{*path}/tree` as distinct routes. `dispatch` recovers the
//! intended operation by stripping the known suffix back off the capture.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{Page, PageRequest, TreeEntry, TreeOptions, WorkspaceId};

use crate::dto::{query_bool, query_u32};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/directories/{*path}",
        get(dispatch),
    )
}

/// A single handler for both `/children` and `/tree` because axum's
/// wildcard segment swallows the trailing literal; this splits it back out.
async fn dispatch(
    state: State<AppState>,
    ctx: Ctx,
    path: Path<(WorkspaceId, String)>,
    query: Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    let (_workspace_id, raw) = &path.0;
    if let Some(prefix) = raw.strip_suffix("/children") {
        return read_dir(state, ctx, prefix.to_string(), query).await;
    }
    if let Some(prefix) = raw.strip_suffix("/tree") {
        return tree(state, ctx, prefix.to_string(), query).await;
    }
    Err(ApiError::RouteNotFound)
}

fn parse_path(raw: &str) -> Result<fslite_core::VirtualPath, ApiError> {
    fslite_core::VirtualPath::parse(&format!("/{raw}"))
        .map_err(|err| ApiError::MalformedBody(err.message().to_string()))
}

fn page_request(params: &HashMap<String, String>) -> Result<PageRequest, ApiError> {
    Ok(PageRequest::default()
        .cursor(params.get("cursor").cloned())
        .limit(query_u32(params, "limit", fslite_core::DEFAULT_PAGE_LIMIT)?))
}

async fn read_dir(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    raw_path: String,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    let path = parse_path(&raw_path)?;
    let page = page_request(&params)?;
    let result = state.fs.read_dir(&ctx, &path, page).await?;
    Ok(Json(result).into_response())
}

async fn tree(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    raw_path: String,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    let path = parse_path(&raw_path)?;
    let page = page_request(&params)?;
    let max_depth = match params.get("max_depth") {
        None => None,
        Some(v) => Some(
            v.parse::<u32>()
                .map_err(|_| ApiError::MalformedBody("max_depth must be a non-negative integer".into()))?,
        ),
    };
    let follow_symlinks = query_bool(&params, "follow_symlinks", false)?;
    let options = TreeOptions::default()
        .max_depth(max_depth)
        .follow_symlinks(follow_symlinks);
    let result: Page<TreeEntry> = state.fs.tree(&ctx, &path, options, page).await?;
    Ok(Json(result).into_response())
}
