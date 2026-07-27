//! Shared query-string parsing helpers reused by every data route module.

use std::collections::HashMap;

use fslite_core::Revision;

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
///
/// Not yet called within this task's own routes — it's shared infrastructure
/// for upcoming tasks (7+) that take `u32` query params (e.g. `limit`).
#[allow(dead_code)]
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
