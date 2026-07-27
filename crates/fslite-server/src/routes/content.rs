//! Streaming file content routes: `read` (GET, honors `Range`), `write`
//! (PUT), `write_at` (PATCH `?offset=N`), and `append`/`truncate` (POST,
//! disambiguated by `?action=`).

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{MutationOptions, ReadOptions, StatOptions, VirtualPath, WorkspaceId, WriteOptions, WriteSource};
use futures::TryStreamExt;

use crate::dto::query_revision;
use crate::error::ApiError;
use crate::range::{resolve_range, RangeError};
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/content/{*path}",
        get(read).put(write).patch(write_at).post(post_action),
    )
}

fn parse_path(raw: &str) -> Result<VirtualPath, ApiError> {
    VirtualPath::parse(&format!("/{raw}")).map_err(|err| ApiError::MalformedBody(err.message().to_string()))
}

async fn read(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(WorkspaceId, String)>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let path = parse_path(&path)?;

    let range = match headers.get(axum::http::header::RANGE) {
        None => None,
        Some(value) => {
            let header = value.to_str().map_err(|_| ApiError::MalformedBody("invalid Range header encoding".into()))?;
            let node = state.fs.stat(&ctx, &path, StatOptions::default()).await?;
            match resolve_range(header, node.logical_size) {
                Ok(range) => Some(range),
                Err(RangeError::Unsatisfiable) => {
                    let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                    response.headers_mut().insert(
                        axum::http::header::CONTENT_RANGE,
                        HeaderValue::from_str(&format!("bytes */{}", node.logical_size)).unwrap(),
                    );
                    return Ok(response);
                }
                Err(_) => return Err(ApiError::MalformedBody("unsupported Range header".into())),
            }
        }
    };

    let requested_range = range.is_some();
    let file = state
        .fs
        .read(&ctx, &path, ReadOptions::default().range(range))
        .await?;

    let content_range_header = format!(
        "bytes {}-{}/{}",
        file.range.start,
        file.range.end.saturating_sub(1),
        file.logical_length
    );
    let logical_length = file.logical_length;
    let stream = file.into_stream();
    let body = Body::from_stream(stream);

    let mut response = if requested_range {
        (StatusCode::PARTIAL_CONTENT, body).into_response()
    } else {
        (StatusCode::OK, body).into_response()
    };
    response.headers_mut().insert(axum::http::header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response.headers_mut().insert(axum::http::header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    if requested_range {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_RANGE, HeaderValue::from_str(&content_range_header).unwrap());
    } else {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_LENGTH, HeaderValue::from_str(&logical_length.to_string()).unwrap());
    }
    Ok(response)
}

fn body_write_source(body: Body) -> WriteSource {
    let stream = body
        .into_data_stream()
        .map_err(|err| fslite_core::FsError::internal_storage_failure(format!("client body stream error: {err}")));
    WriteSource::new(stream)
}

async fn write(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: Body,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    let create = crate::dto::query_bool(&params, "create", true)?;
    let expected_revision = query_revision(&params)?;
    let options = WriteOptions::default().create(create).expected_revision(expected_revision);
    let node = state.fs.write(&ctx, &path, body_write_source(body), options).await?;
    Ok(Json(node))
}

async fn write_at(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: Body,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    let offset: u64 = params
        .get("offset")
        .ok_or_else(|| ApiError::MalformedBody("query parameter `offset` is required".into()))?
        .parse()
        .map_err(|_| ApiError::MalformedBody("offset must be a non-negative integer".into()))?;
    let expected_revision = query_revision(&params)?;
    let options = WriteOptions::default().expected_revision(expected_revision);
    let node = state.fs.write_at(&ctx, &path, offset, body_write_source(body), options).await?;
    Ok(Json(node))
}

#[derive(serde::Deserialize)]
struct TruncateBody {
    length: u64,
    expected_revision: Option<u64>,
}

async fn post_action(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, path)): Path<(WorkspaceId, String)>,
    Query(params): Query<HashMap<String, String>>,
    body: Body,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let path = parse_path(&path)?;
    match params.get("action").map(String::as_str) {
        Some("append") => {
            let node = state.fs.append(&ctx, &path, body_write_source(body), WriteOptions::default()).await?;
            Ok(Json(node))
        }
        Some("truncate") => {
            let bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .map_err(|e| ApiError::MalformedBody(e.to_string()))?;
            let parsed: TruncateBody = serde_json::from_slice(&bytes).map_err(|e| ApiError::MalformedBody(e.to_string()))?;
            let expected_revision = match parsed.expected_revision {
                None => None,
                Some(0) => return Err(ApiError::MalformedBody("expected_revision must be nonzero".into())),
                Some(v) => fslite_core::Revision::new(v),
            };
            let options = MutationOptions::default().expected_revision(expected_revision);
            let node = state.fs.truncate(&ctx, &path, parsed.length, options).await?;
            Ok(Json(node))
        }
        _ => Err(ApiError::MalformedBody("query parameter `action` must be `append` or `truncate`".into())),
    }
}
