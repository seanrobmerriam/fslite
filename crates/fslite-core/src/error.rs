use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A stable, transport-independent category for filesystem failures.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// A path or node name is invalid.
    InvalidPathOrName,
    /// A requested node or resource does not exist.
    NotFound,
    /// Creating or restoring a resource would collide with an existing one.
    AlreadyExists,
    /// An operation does not support the target node kind.
    WrongNodeType,
    /// A directory cannot be removed because it has children.
    DirectoryNotEmpty,
    /// Resolving a symbolic link exceeded the permitted number of links.
    LinkLoop,
    /// A symbolic link target does not exist.
    BrokenLink,
    /// An operation attempted to cross a workspace isolation boundary.
    WorkspaceBoundaryViolation,
    /// The caller is not permitted to perform the requested operation.
    PermissionDenied,
    /// An optimistic concurrency precondition did not match the current revision.
    RevisionConflict,
    /// The operation would exceed a configured quota.
    QuotaExceeded,
    /// A byte range is malformed or outside the target content.
    InvalidRange,
    /// A pagination cursor is malformed or belongs to another workspace.
    InvalidCursor,
    /// The storage backend is temporarily unavailable.
    StorageBusy,
    /// The storage backend encountered an unexpected internal failure.
    InternalStorageFailure,
}

/// A filesystem error with a stable code, human-readable message, and safe details.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct FsError {
    code: ErrorCode,
    message: String,
    details: Value,
}

/// The result type used by filesystem domain operations.
pub type FsResult<T> = Result<T, FsError>;

impl FsError {
    /// Creates an error with the supplied stable code, message, and structured details.
    pub fn new(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    /// Creates an invalid path or name error.
    pub fn invalid_path_or_name(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(
            ErrorCode::InvalidPathOrName,
            "invalid path or name",
            subject,
        )
    }

    /// Creates a not-found error.
    pub fn not_found(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::NotFound, "not found", subject)
    }

    /// Creates an already-exists error.
    pub fn already_exists(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::AlreadyExists, "already exists", subject)
    }

    /// Creates a wrong-node-type error.
    pub fn wrong_node_type(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::WrongNodeType, "wrong node type", subject)
    }

    /// Creates a directory-not-empty error.
    pub fn directory_not_empty(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::DirectoryNotEmpty, "directory not empty", subject)
    }

    /// Creates a link-loop error.
    pub fn link_loop(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::LinkLoop, "link loop", subject)
    }

    /// Creates a broken-link error.
    pub fn broken_link(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::BrokenLink, "broken link", subject)
    }

    /// Creates a workspace-boundary-violation error.
    pub fn workspace_boundary_violation(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(
            ErrorCode::WorkspaceBoundaryViolation,
            "workspace boundary violation",
            subject,
        )
    }

    /// Creates a permission-denied error.
    pub fn permission_denied(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::PermissionDenied, "permission denied", subject)
    }

    /// Creates a revision-conflict error.
    pub fn revision_conflict(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::RevisionConflict, "revision conflict", subject)
    }

    /// Creates a quota-exceeded error.
    pub fn quota_exceeded(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::QuotaExceeded, "quota exceeded", subject)
    }

    /// Creates an invalid-range error.
    pub fn invalid_range(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::InvalidRange, "invalid range", subject)
    }

    /// Creates an invalid-cursor error.
    pub fn invalid_cursor(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::InvalidCursor, "invalid cursor", subject)
    }

    /// Creates a storage-busy error.
    pub fn storage_busy(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(ErrorCode::StorageBusy, "storage busy", subject)
    }

    /// Creates an internal-storage-failure error.
    pub fn internal_storage_failure(subject: impl std::fmt::Display) -> Self {
        Self::for_subject(
            ErrorCode::InternalStorageFailure,
            "internal storage failure",
            subject,
        )
    }

    /// Returns the stable machine-readable error code.
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the safe structured error details.
    pub const fn details(&self) -> &Value {
        &self.details
    }

    fn for_subject(code: ErrorCode, label: &str, subject: impl std::fmt::Display) -> Self {
        Self::new(code, format!("{label}: {subject}"), Value::Null)
    }
}

