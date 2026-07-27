//! Search and change-feed routes: `glob`, `find`, `search_content`, `changes`.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use fslite_core::{ChangeCursor, FindQuery, Page, PageRequest};
use serde::Deserialize;

use crate::dto::{query_u32, ContentQueryRequest, SearchMatchDto};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces/{workspace_id}/search/glob", get(glob))
        .route("/v1/workspaces/{workspace_id}/search/find", post(find))
        .route("/v1/workspaces/{workspace_id}/search/content", post(search_content))
        .route("/v1/workspaces/{workspace_id}/changes", get(changes))
}

fn page_request(params: &HashMap<String, String>) -> Result<PageRequest, ApiError> {
    Ok(PageRequest::default()
        .cursor(params.get("cursor").cloned())
        .limit(query_u32(params, "limit", fslite_core::DEFAULT_PAGE_LIMIT)?))
}

/// The wire shape of a `page` object in a JSON request body.
///
/// `PageRequest`'s derived `Deserialize` requires `limit` whenever the
/// `page` key is present, since `limit: u32` has no field-level default of
/// its own — only a container-level `#[serde(default)]` on the *caller's*
/// field covers a wholly absent `page` key, not a partial or empty `{}`
/// object. This mirrors every field as optional so clients may omit either
/// or both.
#[derive(Default, Deserialize)]
#[serde(default)]
struct PageRequestDto {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl From<PageRequestDto> for PageRequest {
    fn from(value: PageRequestDto) -> Self {
        PageRequest::default()
            .cursor(value.cursor)
            .limit(value.limit.unwrap_or(fslite_core::DEFAULT_PAGE_LIMIT))
    }
}

async fn glob(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Page<fslite_core::Node>>, ApiError> {
    let pattern = params
        .get("pattern")
        .ok_or_else(|| ApiError::MalformedBody("query parameter `pattern` is required".into()))?;
    let page = page_request(&params)?;
    Ok(Json(state.fs.glob(&ctx, pattern, page).await?))
}

/// Deserializes a `FindQuery`, filling any keys missing from the request
/// body with `FindQuery::default()`'s values first.
///
/// `FindQuery`'s derived `Deserialize` requires every field's key to be
/// present once the object itself is present — `attributes:
/// BTreeMap<String, Value>` has no field-level default of its own, even
/// though the type as a whole implements `Default`. This still reuses
/// `FindQuery`'s own `Deserialize` impl; it just backfills the gaps a
/// partial client body would otherwise trip over.
fn deserialize_find_query<'de, D>(deserializer: D) -> Result<FindQuery, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut value = serde_json::Value::deserialize(deserializer)?;
    if let serde_json::Value::Object(map) = &mut value {
        let default = serde_json::to_value(FindQuery::default()).map_err(serde::de::Error::custom)?;
        if let serde_json::Value::Object(default_map) = default {
            for (key, default_value) in default_map {
                map.entry(key).or_insert(default_value);
            }
        }
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
struct FindRequest {
    #[serde(deserialize_with = "deserialize_find_query")]
    query: FindQuery,
    #[serde(default)]
    page: PageRequestDto,
}

async fn find(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(body): Json<FindRequest>,
) -> Result<Json<Page<fslite_core::Node>>, ApiError> {
    Ok(Json(state.fs.find(&ctx, body.query, body.page.into()).await?))
}

#[derive(Deserialize)]
struct SearchContentRequest {
    #[serde(flatten)]
    query: ContentQueryRequest,
    #[serde(default)]
    page: PageRequestDto,
}

async fn search_content(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(body): Json<SearchContentRequest>,
) -> Result<Json<Page<SearchMatchDto>>, ApiError> {
    let query: fslite_core::ContentQuery = body.query.try_into()?;
    let page = state.fs.search_content(&ctx, query, body.page.into()).await?;
    Ok(Json(Page::new(
        page.items.into_iter().map(SearchMatchDto::from).collect(),
        page.next_cursor,
    )))
}

async fn changes(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Page<fslite_core::Change>>, ApiError> {
    let after = params.get("after").map(|raw| ChangeCursor::new(raw.clone()));
    let page = page_request(&params)?;
    Ok(Json(state.fs.changes(&ctx, after, page).await?))
}
