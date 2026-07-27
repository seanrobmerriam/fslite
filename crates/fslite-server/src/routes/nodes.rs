//! Node metadata and action routes: `stat`/`read_link` (GET, disambiguated by
//! a `/link-target` suffix on the wildcard capture), `exists` (HEAD),
//! `remove` (DELETE), `mkdir`/`symlink` (PUT, disambiguated by `?type=`),
//! `touch`/`set_attribute`/`remove_attribute` (PATCH, disambiguated by an
//! `op` tag in the body), and `copy`/`move`/`trash` (POST, disambiguated by
//! `?action=`).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
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
        get(get_dispatch).head(exists).delete(remove).put(put_node).patch(patch_node).post(post_action),
    )
}

fn parse_path(raw: &str) -> Result<VirtualPath, ApiError> {
    VirtualPath::parse(&format!("/{raw}"))
        .map_err(|err| ApiError::MalformedBody(err.message().to_string()))
}

/// A single handler for both plain `stat` and `/link-target` because axum's
/// wildcard segment swallows the trailing literal; this splits it back out
/// (mirroring `directories::dispatch`).
async fn get_dispatch(
    state: State<AppState>,
    ctx: Ctx,
    Path((workspace_id, raw)): Path<(fslite_core::WorkspaceId, String)>,
    query: Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    if let Some(prefix) = raw.strip_suffix("/link-target") {
        return read_link(state, ctx, prefix.to_string()).await;
    }
    stat_inner(state, ctx, Path((workspace_id, raw)), query).await
}

async fn read_link(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    raw_path: String,
) -> Result<axum::response::Response, ApiError> {
    let path = parse_path(&raw_path)?;
    let target = state.fs.read_link(&ctx, &path).await?;
    Ok(Json(serde_json::json!({
        "target": target.as_str(),
        "absolute": target.is_absolute(),
    }))
    .into_response())
}

async fn stat_inner(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    let path = parse_path(&path)?;
    let follow_symlinks = query_bool(&params, "follow_symlinks", true)?;
    let node = state
        .fs
        .stat(&ctx, &path, StatOptions::default().follow_symlinks(follow_symlinks))
        .await?;
    Ok(Json(node).into_response())
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

#[derive(serde::Deserialize)]
struct ActionBody {
    to: Option<String>,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    overwrite: bool,
    expected_revision: Option<u64>,
}

async fn post_action(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Result<axum::response::Response, ApiError> {
    let from = parse_path(&path)?;
    let parsed: ActionBody = if body.is_empty() {
        ActionBody { to: None, recursive: false, overwrite: false, expected_revision: None }
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?
    };
    let expected_revision = revision_from(parsed.expected_revision)?;

    match params.get("action").map(String::as_str) {
        Some("copy") => {
            let to = parsed.to.ok_or_else(|| ApiError::MalformedBody("`to` is required for action=copy".into()))?;
            let to = parse_path(&to)?;
            let options = fslite_core::CopyOptions::default()
                .recursive(parsed.recursive)
                .overwrite(parsed.overwrite)
                .expected_revision(expected_revision);
            Ok(Json(state.fs.copy(&ctx, &from, &to, options).await?).into_response())
        }
        Some("move") => {
            let to = parsed.to.ok_or_else(|| ApiError::MalformedBody("`to` is required for action=move".into()))?;
            let to = parse_path(&to)?;
            let options = fslite_core::MoveOptions::default()
                .overwrite(parsed.overwrite)
                .expected_revision(expected_revision);
            Ok(Json(state.fs.move_path(&ctx, &from, &to, options).await?).into_response())
        }
        Some("trash") => {
            let options = fslite_core::MutationOptions::default().expected_revision(expected_revision);
            Ok(Json(state.fs.trash(&ctx, &from, options).await?).into_response())
        }
        _ => Err(ApiError::MalformedBody("query parameter `action` must be `copy`, `move`, or `trash`".into())),
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum PatchBody {
    Touch { #[serde(default)] create: bool, expected_revision: Option<u64> },
    SetAttribute { key: String, value_base64: String, expected_revision: Option<u64> },
    RemoveAttribute { key: String, expected_revision: Option<u64> },
}

async fn patch_node(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(fslite_core::WorkspaceId, String)>,
    axum::extract::Json(body): axum::extract::Json<PatchBody>,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    match body {
        PatchBody::Touch { create, expected_revision } => {
            let options = fslite_core::TouchOptions::default()
                .create(create)
                .expected_revision(revision_from(expected_revision)?);
            Ok(Json(state.fs.touch(&ctx, &path, options).await?))
        }
        PatchBody::SetAttribute { key, value_base64, expected_revision } => {
            use base64::Engine;
            let value = base64::engine::general_purpose::STANDARD
                .decode(value_base64)
                .map_err(|e| ApiError::MalformedBody(format!("invalid base64 value: {e}")))?;
            let options = fslite_core::MutationOptions::default().expected_revision(revision_from(expected_revision)?);
            Ok(Json(state.fs.set_attribute(&ctx, &path, &key, &value, options).await?))
        }
        PatchBody::RemoveAttribute { key, expected_revision } => {
            let options = fslite_core::MutationOptions::default().expected_revision(revision_from(expected_revision)?);
            Ok(Json(state.fs.remove_attribute(&ctx, &path, &key, options).await?))
        }
    }
}
