use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use fslite_core::{
    BatchOperation, CopyOptions, CreateOptions, MoveOptions, MutationOptions, RemoveOptions,
    TouchOptions,
};
use serde::Deserialize;

use crate::Ctx;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/batch",
        post(batch).fallback(super::method_not_allowed),
    )
}

/// Returns the serialized default for the `options` object a given
/// `BatchOperation` tag carries, or `None` for tags with no `options` field.
fn default_options_json(tag: &str) -> Option<serde_json::Value> {
    let value = match tag {
        "mkdir" | "symlink" => serde_json::to_value(CreateOptions::default()),
        "touch" => serde_json::to_value(TouchOptions::default()),
        "copy" => serde_json::to_value(CopyOptions::default()),
        "move" => serde_json::to_value(MoveOptions::default()),
        "remove" => serde_json::to_value(RemoveOptions::default()),
        "trash" | "restore" | "set_attribute" | "remove_attribute" => {
            serde_json::to_value(MutationOptions::default())
        }
        _ => return None,
    };
    value.ok()
}

/// Deserializes a `Vec<BatchOperation>`, filling any keys missing from each
/// element's nested `options` object with that operation's own
/// `*Options::default()` values first.
///
/// Every `*Options` type `BatchOperation` embeds (`CreateOptions`,
/// `TouchOptions`, `CopyOptions`, `MoveOptions`, `RemoveOptions`,
/// `MutationOptions`) derives `Deserialize` without a `#[serde(default)]`
/// container attribute, so a bare `bool` field like `parents` or `recursive`
/// has no field-level default of its own — a partial or empty `options: {}}`
/// object fails with "missing field" even though the options type as a
/// whole implements `Default`. This still reuses `BatchOperation`'s own
/// `Deserialize` impl; it just backfills the gaps a client that omits
/// `options` fields would otherwise trip over. Mirrors
/// `search::deserialize_find_query`.
fn deserialize_operations<'de, D>(deserializer: D) -> Result<Vec<BatchOperation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut raw: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    for op in &mut raw {
        let serde_json::Value::Object(map) = op else {
            continue;
        };
        let Some((tag, inner)) = map.iter_mut().next() else {
            continue;
        };
        let serde_json::Value::Object(inner_map) = inner else {
            continue;
        };
        let Some(serde_json::Value::Object(options_map)) = inner_map.get_mut("options") else {
            continue;
        };
        let Some(serde_json::Value::Object(default_map)) = default_options_json(tag) else {
            continue;
        };
        for (key, default_value) in default_map {
            options_map.entry(key).or_insert(default_value);
        }
    }
    raw.into_iter()
        .map(|value| serde_json::from_value(value).map_err(serde::de::Error::custom))
        .collect()
}

#[derive(Deserialize)]
struct BatchRequest {
    #[serde(deserialize_with = "deserialize_operations")]
    operations: Vec<BatchOperation>,
}

async fn batch(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body: BatchRequest =
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?;
    let results = state.fs.batch(&ctx, body.operations).await?;
    Ok(Json(serde_json::json!({ "results": results })))
}
