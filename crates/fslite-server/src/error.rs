use axum::Json;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use fslite_core::{ErrorCode, FsError};
use serde_json::json;

/// The uniform error type every `fslite-server` handler returns.
#[derive(Debug)]
pub enum ApiError {
    /// A domain error surfaced by `fslite-core`.
    Domain(FsError),
    /// The request carried no, or an unrecognized, credential.
    Unauthenticated(String),
    /// The authenticated actor's workspace does not match the URL.
    WorkspaceMismatch,
    /// The request body was not valid JSON, or failed local validation.
    MalformedBody(String),
    /// No route matched the request.
    RouteNotFound,
    /// The route exists but not for this HTTP method.
    MethodNotAllowed,
    /// The request body exceeded a transport-level size limit.
    PayloadTooLarge,
    /// An unexpected server-side failure outside the `FsError` domain.
    Internal(String),
}

impl From<FsError> for ApiError {
    fn from(err: FsError) -> Self {
        ApiError::Domain(err)
    }
}

fn domain_status(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::InvalidPathOrName
        | ErrorCode::WorkspaceBoundaryViolation
        | ErrorCode::InvalidCursor => StatusCode::BAD_REQUEST,
        ErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::AlreadyExists
        | ErrorCode::WrongNodeType
        | ErrorCode::DirectoryNotEmpty
        | ErrorCode::LinkLoop
        | ErrorCode::BrokenLink
        | ErrorCode::QuotaExceeded => StatusCode::CONFLICT,
        ErrorCode::RevisionConflict => StatusCode::PRECONDITION_FAILED,
        ErrorCode::InvalidRange => StatusCode::RANGE_NOT_SATISFIABLE,
        ErrorCode::StorageBusy => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::InternalStorageFailure => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn code_str(code: ErrorCode) -> &'static str {
    // Hand-maintained to match `ErrorCode`'s `#[serde(rename_all =
    // "snake_case")]` output exactly, without leaking a new heap allocation
    // per error response the way `Box::leak` would.
    match code {
        ErrorCode::InvalidPathOrName => "invalid_path_or_name",
        ErrorCode::NotFound => "not_found",
        ErrorCode::AlreadyExists => "already_exists",
        ErrorCode::WrongNodeType => "wrong_node_type",
        ErrorCode::DirectoryNotEmpty => "directory_not_empty",
        ErrorCode::LinkLoop => "link_loop",
        ErrorCode::BrokenLink => "broken_link",
        ErrorCode::WorkspaceBoundaryViolation => "workspace_boundary_violation",
        ErrorCode::PermissionDenied => "permission_denied",
        ErrorCode::RevisionConflict => "revision_conflict",
        ErrorCode::QuotaExceeded => "quota_exceeded",
        ErrorCode::InvalidRange => "invalid_range",
        ErrorCode::InvalidCursor => "invalid_cursor",
        ErrorCode::StorageBusy => "storage_busy",
        ErrorCode::InternalStorageFailure => "internal_storage_failure",
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            ApiError::Domain(err) => (
                domain_status(err.code()),
                code_str(err.code()),
                err.message().to_string(),
                err.details().clone(),
            ),
            ApiError::Unauthenticated(message) => (
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                message,
                json!({}),
            ),
            ApiError::WorkspaceMismatch => (
                StatusCode::FORBIDDEN,
                "workspace_mismatch",
                "credential does not authorize this workspace".to_string(),
                json!({}),
            ),
            ApiError::MalformedBody(message) => (
                StatusCode::BAD_REQUEST,
                "malformed_body",
                message,
                json!({}),
            ),
            ApiError::RouteNotFound => (
                StatusCode::NOT_FOUND,
                "route_not_found",
                "no route matched this request".to_string(),
                json!({}),
            ),
            ApiError::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "this route does not support this method".to_string(),
                json!({}),
            ),
            ApiError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "request body exceeded the configured limit".to_string(),
                json!({}),
            ),
            ApiError::Internal(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                message,
                json!({}),
            ),
        };

        let mut response = (
            status,
            Json(json!({ "error": { "code": code, "message": message, "details": details } })),
        )
            .into_response();

        if status == StatusCode::SERVICE_UNAVAILABLE {
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("1"));
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `code_str` hand-maintains the same strings `ErrorCode`'s derived
    /// `#[serde(rename_all = "snake_case")] Serialize` impl already produces
    /// (it used to just reuse that impl via `Box::leak`, which leaked memory
    /// per error response). This guards the two from drifting apart.
    #[test]
    fn code_str_matches_error_code_serde_serialization_for_every_variant() {
        let all = [
            ErrorCode::InvalidPathOrName,
            ErrorCode::NotFound,
            ErrorCode::AlreadyExists,
            ErrorCode::WrongNodeType,
            ErrorCode::DirectoryNotEmpty,
            ErrorCode::LinkLoop,
            ErrorCode::BrokenLink,
            ErrorCode::WorkspaceBoundaryViolation,
            ErrorCode::PermissionDenied,
            ErrorCode::RevisionConflict,
            ErrorCode::QuotaExceeded,
            ErrorCode::InvalidRange,
            ErrorCode::InvalidCursor,
            ErrorCode::StorageBusy,
            ErrorCode::InternalStorageFailure,
        ];
        for code in all {
            let serde_str = match serde_json::to_value(code).unwrap() {
                serde_json::Value::String(s) => s,
                _ => panic!("ErrorCode serializes as a string"),
            };
            assert_eq!(
                code_str(code),
                serde_str,
                "code_str drifted from serde for {code:?}"
            );
        }
    }
}
