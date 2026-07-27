//! Shared query-string parsing helpers reused by every data route module.

use std::collections::HashMap;

use base64::Engine;
use fslite_core::{ByteRange, ContentQuery, Node, Revision, SearchMatch, VirtualPath};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

/// Reads a boolean query parameter, defaulting when absent.
pub fn query_bool(params: &HashMap<String, String>, key: &str, default: bool) -> Result<bool, ApiError> {
    match params.get(key) {
        None => Ok(default),
        Some(value) => value
            .parse()
            .map_err(|_| ApiError::MalformedBody(format!("query parameter `{key}` must be a boolean"))),
    }
}

/// Reads a `u32` query parameter, defaulting when absent.
pub fn query_u32(params: &HashMap<String, String>, key: &str, default: u32) -> Result<u32, ApiError> {
    match params.get(key) {
        None => Ok(default),
        Some(value) => value
            .parse()
            .map_err(|_| ApiError::MalformedBody(format!("query parameter `{key}` must be a non-negative integer"))),
    }
}

/// Reads an optional `expected_revision` query parameter.
pub fn query_revision(params: &HashMap<String, String>) -> Result<Option<Revision>, ApiError> {
    match params.get("expected_revision") {
        None => Ok(None),
        Some(value) => {
            let raw: u64 = value
                .parse()
                .map_err(|_| ApiError::MalformedBody("expected_revision must be a positive integer".into()))?;
            Revision::new(raw)
                .map(Some)
                .ok_or_else(|| ApiError::MalformedBody("expected_revision must be nonzero".into()))
        }
    }
}

/// The wire shape of a content-search request: `ContentQuery` with its raw
/// `needle: Vec<u8>` field replaced by a base64 string for a sane JSON body.
#[derive(Deserialize)]
pub struct ContentQueryRequest {
    pub root: VirtualPath,
    pub needle_base64: String,
}

impl TryFrom<ContentQueryRequest> for ContentQuery {
    type Error = ApiError;

    fn try_from(value: ContentQueryRequest) -> Result<Self, Self::Error> {
        let needle = base64::engine::general_purpose::STANDARD
            .decode(value.needle_base64)
            .map_err(|e| ApiError::MalformedBody(format!("invalid base64 needle: {e}")))?;
        Ok(ContentQuery::default().root(value.root).needle(needle))
    }
}

/// The wire shape of a `SearchMatch`: `preview: Vec<u8>` becomes base64.
#[derive(Serialize)]
pub struct SearchMatchDto {
    pub node: Node,
    pub path: VirtualPath,
    pub range: ByteRange,
    pub preview_base64: String,
}

impl From<SearchMatch> for SearchMatchDto {
    fn from(value: SearchMatch) -> Self {
        Self {
            node: value.node,
            path: value.path,
            range: value.range,
            preview_base64: base64::engine::general_purpose::STANDARD.encode(value.preview),
        }
    }
}
