//! Node metadata routes: `stat` (GET), `exists` (HEAD), `remove` (DELETE),
//! `mkdir`/`symlink` (PUT, disambiguated by `?type=`).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{CreateOptions, LinkTarget, RemoveOptions, StatOptions, VirtualPath};
use serde::Deserialize;

use crate::dto::{query_bool, query_revision};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/fs/{*path}",
        get(stat).head(exists).delete(remove).put(put_node),
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

#[derive(Deserialize)]
struct MkdirBody {
    #[serde(default)]
    parents: bool,
    #[serde(default)]
    exist_ok: bool,
    expected_revision: Option<u64>,
}

#[derive(Deserialize)]
struct SymlinkBody {
    target: String,
    #[serde(default)]
    parents: bool,
    #[serde(default)]
    exist_ok: bool,
    expected_revision: Option<u64>,
}

fn revision_from(raw: Option<u64>) -> Result<Option<fslite_core::Revision>, ApiError> {
    match raw {
        None => Ok(None),
        Some(0) => Err(ApiError::MalformedBody("expected_revision must be nonzero".into())),
        Some(v) => Ok(fslite_core::Revision::new(v)),
    }
}

async fn put_node(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    match params.get("type").map(String::as_str) {
        Some("directory") => {
            let body: MkdirBody = if body.is_empty() {
                MkdirBody { parents: false, exist_ok: false, expected_revision: None }
            } else {
                serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?
            };
            let options = CreateOptions::default()
                .parents(body.parents)
                .exist_ok(body.exist_ok)
                .expected_revision(revision_from(body.expected_revision)?);
            Ok(Json(state.fs.mkdir(&ctx, &path, options).await?))
        }
        Some("symlink") => {
            let body: SymlinkBody = serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?;
            let target = LinkTarget::parse(&body.target).map_err(|e| ApiError::MalformedBody(e.message().to_string()))?;
            let options = CreateOptions::default()
                .parents(body.parents)
                .exist_ok(body.exist_ok)
                .expected_revision(revision_from(body.expected_revision)?);
            Ok(Json(state.fs.symlink(&ctx, &target, &path, options).await?))
        }
        _ => Err(ApiError::MalformedBody("query parameter `type` must be `directory` or `symlink`".into())),
    }
}
