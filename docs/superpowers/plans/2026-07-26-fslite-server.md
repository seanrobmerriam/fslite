# fslite-server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `fslite-server`, an HTTP adapter that exposes the `fslite_core::FileSystem` trait (backend-agnostic — driven through `Arc<dyn FileSystem>`, never hardcoded to `SqliteFileSystem`) as a resource-oriented REST API, with pluggable authentication, byte-range reads, streaming bodies, a uniform JSON error envelope, health/readiness probes, request tracing, and an HTTP contract test suite.

**Architecture:** An `axum` router built from small per-resource route modules, all sharing one `AppState { fs: Arc<dyn FileSystem>, admin: Arc<dyn WorkspaceAdmin>, auth: Arc<dyn AuthProvider>, health_workspace: WorkspaceId }`. An `AuthProvider` trait maps inbound requests to a `fslite_core::RequestContext`; a custom `Ctx` extractor runs it and enforces the URL's `{workspace_id}` matches the authenticated actor's workspace. Every handler returns `Result<impl IntoResponse, ApiError>`, where `ApiError` implements `From<FsError>` via a fixed `ErrorCode → HTTP status` table and renders one JSON envelope shape for both domain and transport errors. Because most `fslite-core` types (`Node`, `Page<T>`, `TrashEntry`, `Change`, `WorkspaceUsage`, `BatchOperation`/`BatchResult`, `VirtualPath`, `LinkTarget`) already derive `Serialize`/`Deserialize`, most routes serialize the core type directly with no bespoke DTO; the only bespoke DTOs are for the two fields that are raw `Vec<u8>` in core (`ContentQuery.needle`, `SearchMatch.preview`), which get base64-wrapped for a sane wire format. File content flows as raw streamed bytes on dedicated `content` routes, never inside JSON.

**Tech Stack:** `axum` 0.8 (router, extractors, streaming `Body`), `tower` / `tower-http` (tracing middleware), `tracing` + `tracing-subscriber`, existing workspace deps (`fslite-core`, `fslite-sqlite`, `tokio`, `serde`, `serde_json`, `bytes`, `futures`, `base64`, `uuid`). No new HTTP client dependency is needed in this plan — contract tests drive the router in-process via `tower::ServiceExt::oneshot`, not over a real socket.

## Global Constraints

- Do not modify `fslite-core` or `fslite-sqlite`. Both are frozen, already-shipped contracts (see `main/README.md`: "The `FileSystem` trait and the SQLite backend are complete"). `fslite-server` only depends on their public API.
- `fslite-server`'s route handlers must be generic over `Arc<dyn FileSystem>` for every route that exists on the `FileSystem` trait. `create_workspace`/`delete_workspace` are `SqliteFileSystem` **inherent** methods, not trait methods — bridge them with a server-owned `WorkspaceAdmin` trait (defined in this crate) plus a `SqliteWorkspaceAdmin` adapter, never by downcasting `dyn FileSystem` back to `SqliteFileSystem`.
- Every JSON error body has the shape `{"error": {"code": "<snake_case>", "message": "<string>", "details": <json value>}}`. `code` values for domain errors are exactly the `fslite_core::ErrorCode` variant names under `#[serde(rename_all = "snake_case")]` (e.g. `"not_found"`, `"revision_conflict"`).
- Keep the repo green throughout: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` must all pass after every task's commit.
- New workspace dependencies are added once, in Task 1, to the root `Cargo.toml`'s `[workspace.dependencies]` table — never duplicated with ad hoc versions in a sub-crate's `Cargo.toml`.
- Every mutating route must round-trip `expected_revision` from the client into the corresponding `*Options.expected_revision` field so optimistic-concurrency semantics from `fslite-core` are preserved end-to-end (a mismatch must surface as HTTP 412, not a swallowed default).

---

## Route Table

All routes are namespaced under `/v1`. `{workspace_id}` and `{trash_id}` are UUID path segments; `{*path}` is an axum wildcard capturing the rest of the URL, which handlers percent-decode and pass to `VirtualPath::parse`.

| Method | Path | Trait/inherent call | Notes |
| --- | --- | --- | --- |
| GET | `/healthz` | — | liveness, no backend call |
| GET | `/readyz` | `exists` (health workspace root) | readiness, backend round-trip |
| POST | `/v1/workspaces` | `WorkspaceAdmin::create_workspace` | admin |
| DELETE | `/v1/workspaces/{workspace_id}` | `WorkspaceAdmin::delete_workspace` | admin |
| GET | `/v1/workspaces/{workspace_id}/usage` | `workspace_usage` | |
| GET | `/v1/workspaces/{workspace_id}/fs/{*path}` | `stat` | query: `follow_symlinks` |
| HEAD | `/v1/workspaces/{workspace_id}/fs/{*path}` | `exists` | query: `follow_symlinks` |
| PUT | `/v1/workspaces/{workspace_id}/fs/{*path}?type=directory` | `mkdir` | body: `CreateOptions`-ish JSON |
| PUT | `/v1/workspaces/{workspace_id}/fs/{*path}?type=symlink` | `symlink` | body: `{target, parents, exist_ok, expected_revision}` |
| DELETE | `/v1/workspaces/{workspace_id}/fs/{*path}` | `remove` | query: `recursive`, `expected_revision` |
| PATCH | `/v1/workspaces/{workspace_id}/fs/{*path}` | `touch` / `set_attribute` / `remove_attribute` | body: tagged op |
| GET | `/v1/workspaces/{workspace_id}/fs/{*path}/link-target` | `read_link` | |
| POST | `/v1/workspaces/{workspace_id}/fs/{*path}?action=copy` | `copy` | body: `{to, recursive, overwrite, expected_revision}` |
| POST | `/v1/workspaces/{workspace_id}/fs/{*path}?action=move` | `move_path` | body: `{to, overwrite, expected_revision}` |
| POST | `/v1/workspaces/{workspace_id}/fs/{*path}?action=trash` | `trash` | body: `{expected_revision}` |
| GET | `/v1/workspaces/{workspace_id}/directories/{*path}/children` | `read_dir` | query: `cursor`, `limit` |
| GET | `/v1/workspaces/{workspace_id}/directories/{*path}/tree` | `tree` | query: `cursor`, `limit`, `max_depth`, `follow_symlinks` |
| GET | `/v1/workspaces/{workspace_id}/content/{*path}` | `read` | supports `Range` header |
| PUT | `/v1/workspaces/{workspace_id}/content/{*path}` | `write` | streamed body; query: `create`, `expected_revision` |
| PATCH | `/v1/workspaces/{workspace_id}/content/{*path}?offset=N` | `write_at` | streamed body |
| POST | `/v1/workspaces/{workspace_id}/content/{*path}?action=append` | `append` | streamed body |
| POST | `/v1/workspaces/{workspace_id}/content/{*path}?action=truncate` | `truncate` | body: `{length, expected_revision}` |
| GET | `/v1/workspaces/{workspace_id}/trash` | `list_trash` | query: `cursor`, `limit` |
| POST | `/v1/workspaces/{workspace_id}/trash/{trash_id}/restore` | `restore` | body: `{destination, expected_revision}` |
| DELETE | `/v1/workspaces/{workspace_id}/trash/{trash_id}` | `purge` | |
| GET | `/v1/workspaces/{workspace_id}/search/glob` | `glob` | query: `pattern`, `cursor`, `limit` |
| POST | `/v1/workspaces/{workspace_id}/search/find` | `find` | body: `FindQuery`-shaped JSON + `page` |
| POST | `/v1/workspaces/{workspace_id}/search/content` | `search_content` | body: base64 `ContentQueryDto` + `page` |
| POST | `/v1/workspaces/{workspace_id}/batch` | `batch` | body: `Vec<BatchOperation>` (core type verbatim) |
| GET | `/v1/workspaces/{workspace_id}/changes` | `changes` | query: `after`, `cursor`, `limit` |

## `ErrorCode` → HTTP status table

| `ErrorCode` | HTTP status |
| --- | --- |
| `InvalidPathOrName` | 400 |
| `WorkspaceBoundaryViolation` | 400 |
| `InvalidCursor` | 400 |
| `PermissionDenied` | 403 |
| `NotFound` | 404 |
| `AlreadyExists` | 409 |
| `WrongNodeType` | 409 |
| `DirectoryNotEmpty` | 409 |
| `LinkLoop` | 409 |
| `BrokenLink` | 409 |
| `QuotaExceeded` | 409 |
| `RevisionConflict` | 412 |
| `InvalidRange` | 416 |
| `StorageBusy` | 503 (with `Retry-After: 1`) |
| `InternalStorageFailure` | 500 |

Transport-level `ApiError` variants (not backed by `FsError`): `Unauthenticated` → 401, `WorkspaceMismatch` → 403, `MalformedBody` → 400, `RouteNotFound` → 404, `MethodNotAllowed` → 405, `PayloadTooLarge` → 413, `Internal` → 500.

---

## Task 1: Crate scaffolding, `AppState`, health endpoint

**Files:**
- Modify: `main/Cargo.toml` (workspace members + new `[workspace.dependencies]` entries)
- Create: `main/crates/fslite-server/Cargo.toml`
- Create: `main/crates/fslite-server/src/lib.rs`
- Create: `main/crates/fslite-server/src/state.rs`
- Create: `main/crates/fslite-server/src/main.rs`
- Create: `main/crates/fslite-server/src/routes/mod.rs`
- Test: `main/crates/fslite-server/tests/health.rs`

**Interfaces:**
- Produces: `pub struct AppState { pub fs: Arc<dyn FileSystem>, pub admin: Arc<dyn WorkspaceAdmin>, pub auth: Arc<dyn AuthProvider>, pub health_workspace: WorkspaceId }` (fields for `admin`/`auth` are placeholders until Tasks 3 and 13 exist — this task only needs `fs` and `health_workspace`, defined now so later tasks extend the struct instead of redesigning it), `pub fn app(state: AppState) -> Router` in `lib.rs`.

- [ ] **Step 1: Add workspace members and dependencies**

Edit `main/Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = [
  "crates/fslite-core",
  "crates/fslite-sqlite",
  "crates/fslite-conformance",
  "crates/fslite-server",
]

[workspace.dependencies]
async-trait = "0.1"
axum = "0.8"
base64 = "0.22"
bytes = "1"
futures = "0.3"
http-body-util = "0.1"
proptest = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync"] }
tokio-rusqlite = "0.6"
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["serde", "v7"] }
```

(`http-body-util` is a dev-dependency of `fslite-server`'s contract tests, added here for a single shared version.)

- [ ] **Step 2: Write the crate manifest**

Create `main/crates/fslite-server/Cargo.toml`:

```toml
[package]
name = "fslite-server"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
async-trait.workspace = true
axum.workspace = true
base64.workspace = true
bytes.workspace = true
fslite-core = { path = "../fslite-core" }
fslite-sqlite = { path = "../fslite-sqlite" }
futures.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
tower.workspace = true
tower-http.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true

[dev-dependencies]
http-body-util.workspace = true
```

- [ ] **Step 3: Write the failing test**

Create `main/crates/fslite-server/tests/health.rs`:

```rust
use fslite_server::{app, AppState};
use fslite_core::WorkspaceId;
use fslite_sqlite::SqliteFileSystem;
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok_without_touching_the_backend() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let state = AppState {
        fs: Arc::new(fs),
        health_workspace: WorkspaceId::new(),
    };

    let response = app(state)
        .oneshot(
            axum::http::Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test health`
Expected: FAIL to compile — `fslite_server` crate / `app` / `AppState` do not exist yet.

- [ ] **Step 4: Write the minimal implementation**

Create `main/crates/fslite-server/src/state.rs`:

```rust
use std::sync::Arc;

use fslite_core::{FileSystem, WorkspaceId};

/// Shared, cloneable application state handed to every route.
#[derive(Clone)]
pub struct AppState {
    /// The backend-agnostic filesystem every data route is driven through.
    pub fs: Arc<dyn FileSystem>,
    /// The workspace `/readyz` probes with a cheap `exists(root)` call.
    pub health_workspace: WorkspaceId,
}
```

Create `main/crates/fslite-server/src/routes/mod.rs`:

```rust
use axum::Json;
use axum::routing::get;
use axum::Router;
use serde_json::json;

use crate::state::AppState;

pub fn health_router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
```

Create `main/crates/fslite-server/src/lib.rs`:

```rust
//! HTTP adapter exposing `fslite_core::FileSystem` as a resource-oriented API.

mod routes;
mod state;

use axum::Router;

pub use state::AppState;

/// Builds the complete application router from shared state.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_router())
        .with_state(state)
}
```

Create `main/crates/fslite-server/src/main.rs`:

```rust
use std::sync::Arc;

use fslite_core::WorkspaceId;
use fslite_server::{app, AppState};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let health_workspace = WorkspaceId::new();
    let state = AppState {
        fs: Arc::new(fs),
        health_workspace,
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("fslite-server listening on {}", listener.local_addr()?);
    axum::serve(listener, app(state)).await?;
    Ok(())
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p fslite-server --test health`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/fslite-server
git commit -m "feat(fslite-server): scaffold crate with health endpoint"
```

---

## Task 2: JSON error envelope (`ApiError`)

**Files:**
- Create: `main/crates/fslite-server/src/error.rs`
- Modify: `main/crates/fslite-server/src/lib.rs` (add `mod error;`)
- Test: `main/crates/fslite-server/tests/error_envelope.rs`

**Interfaces:**
- Consumes: `fslite_core::{FsError, ErrorCode}`.
- Produces: `pub enum ApiError { Domain(FsError), Unauthenticated(String), WorkspaceMismatch, MalformedBody(String), RouteNotFound, MethodNotAllowed, PayloadTooLarge, Internal(String) }`, `impl From<FsError> for ApiError`, `impl axum::response::IntoResponse for ApiError`. Every later handler task returns `Result<T, ApiError>` and gets `?`-propagation of `FsError` for free via `From`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/error_envelope.rs`:

```rust
use fslite_core::FsError;
use fslite_server::ApiError;
use axum::response::IntoResponse;
use http_body_util::BodyExt;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn not_found_maps_to_404_with_stable_code() {
    let err: ApiError = FsError::not_found("/a/b.txt").into();
    let response = err.into_response();
    assert_eq!(response.status(), 404);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "not_found");
    assert_eq!(json["error"]["message"], "not found: /a/b.txt");
}

#[tokio::test]
async fn revision_conflict_maps_to_412() {
    let err: ApiError = FsError::revision_conflict("/a").into();
    assert_eq!(err.into_response().status(), 412);
}

#[tokio::test]
async fn invalid_range_maps_to_416() {
    let err: ApiError = FsError::invalid_range("/a").into();
    assert_eq!(err.into_response().status(), 416);
}

#[tokio::test]
async fn quota_exceeded_maps_to_409() {
    let err: ApiError = FsError::quota_exceeded("/a").into();
    assert_eq!(err.into_response().status(), 409);
}

#[tokio::test]
async fn permission_denied_maps_to_403() {
    let err: ApiError = FsError::permission_denied("/a").into();
    assert_eq!(err.into_response().status(), 403);
}

#[tokio::test]
async fn storage_busy_maps_to_503_with_retry_after() {
    let err: ApiError = FsError::storage_busy("db").into();
    let response = err.into_response();
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers().get("retry-after").unwrap(), "1");
}

#[tokio::test]
async fn unauthenticated_has_its_own_envelope() {
    let response = ApiError::Unauthenticated("missing bearer token".into()).into_response();
    assert_eq!(response.status(), 401);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "unauthenticated");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test error_envelope`
Expected: FAIL to compile — `ApiError` does not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/error.rs`:

```rust
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
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
    // `ErrorCode` already derives `Serialize` with `rename_all = "snake_case"`;
    // reuse that instead of hand-maintaining a second name table.
    match serde_json::to_value(code).expect("ErrorCode always serializes") {
        serde_json::Value::String(s) => Box::leak(s.into_boxed_str()),
        _ => unreachable!("ErrorCode serializes as a string"),
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
            ApiError::Unauthenticated(message) => {
                (StatusCode::UNAUTHORIZED, "unauthenticated", message, json!({}))
            }
            ApiError::WorkspaceMismatch => (
                StatusCode::FORBIDDEN,
                "workspace_mismatch",
                "credential does not authorize this workspace".to_string(),
                json!({}),
            ),
            ApiError::MalformedBody(message) => {
                (StatusCode::BAD_REQUEST, "malformed_body", message, json!({}))
            }
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
            ApiError::Internal(message) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", message, json!({}))
            }
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
```

Add `mod error;` and `pub use error::ApiError;` to `main/crates/fslite-server/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test error_envelope`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src/error.rs crates/fslite-server/src/lib.rs crates/fslite-server/tests/error_envelope.rs
git commit -m "feat(fslite-server): uniform JSON error envelope"
```

---

## Task 3: `AuthProvider`, capability mapping, `Ctx` extractor

**Files:**
- Create: `main/crates/fslite-server/src/auth.rs`
- Modify: `main/crates/fslite-server/src/state.rs` (add `auth: Arc<dyn AuthProvider>`)
- Modify: `main/crates/fslite-server/src/lib.rs` (`mod auth;` + re-exports)
- Modify: `main/crates/fslite-server/tests/health.rs` (add `auth` field to every `AppState` literal — from this task on, all test fixtures build `AppState` through a shared helper, introduced here)
- Create: `main/crates/fslite-server/tests/support/mod.rs`
- Test: `main/crates/fslite-server/tests/auth.rs`

**Interfaces:**
- Produces:
  - `pub struct AuthenticatedActor { pub workspace_id: WorkspaceId, pub capabilities: BTreeSet<Capability>, pub actor_metadata: BTreeMap<String, Value> }`
  - `#[async_trait] pub trait AuthProvider: Send + Sync { async fn authenticate(&self, headers: &HeaderMap) -> Result<AuthenticatedActor, ApiError>; }`
  - `pub struct BearerTokenAuthProvider { tokens: HashMap<String, AuthenticatedActor> }` with `pub fn new(tokens: HashMap<String, AuthenticatedActor>) -> Self`.
  - `pub struct Ctx(pub RequestContext);` implementing `axum::extract::FromRequestParts<AppState>`, extracting the `{workspace_id}` path param, calling `state.auth.authenticate`, and returning `ApiError::WorkspaceMismatch` if `AuthenticatedActor.workspace_id != path workspace_id`.
- Consumes: `AppState` (Task 1), `ApiError` (Task 2).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/support/mod.rs` (shared test fixture, used by every subsequent test file):

```rust
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_core::{Capability, RequestContext, WorkspaceId};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider};
use fslite_sqlite::SqliteFileSystem;

pub const TOKEN: &str = "test-token";

/// Builds an in-memory backend, a trusted workspace, a bearer token that
/// authenticates as that workspace with every capability, and the
/// `AppState` wiring them together.
pub async fn fixture() -> (AppState, WorkspaceId) {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let health_workspace = workspace.id;

    let mut tokens = HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: BTreeSet::from([
                Capability::Read,
                Capability::Write,
                Capability::Delete,
                Capability::TrashRestore,
                Capability::WorkspaceAdmin,
            ]),
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: Arc::new(fs),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace,
    };
    (state, workspace.id)
}

#[allow(dead_code)]
pub fn trusted_ctx(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::trusted(workspace_id)
}
```

Create `main/crates/fslite-server/tests/auth.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn missing_bearer_token_is_401() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn unknown_token_is_401() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .header("authorization", "Bearer nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn token_for_a_different_workspace_is_403() {
    let (state, _workspace_id) = support::fixture().await;
    let other = fslite_core::WorkspaceId::new();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{other}/usage"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn valid_token_reaches_the_handler() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/usage"))
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // The usage route itself is built in Task 13; until then this asserts
    // "not 401/403", i.e. auth passed and routing took over — expect 404
    // (no route yet) rather than an auth failure.
    assert_eq!(response.status(), 404);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test auth`
Expected: FAIL to compile — `AuthenticatedActor`, `BearerTokenAuthProvider`, `AppState.auth` do not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/auth.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet, HashMap};

use async_trait::async_trait;
use axum::extract::{FromRef, FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use fslite_core::{Capability, RequestContext, WorkspaceId};
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

/// The workspace and capabilities a credential resolves to.
#[derive(Clone, Debug)]
pub struct AuthenticatedActor {
    /// The single workspace this credential is scoped to.
    pub workspace_id: WorkspaceId,
    /// The capabilities granted within that workspace.
    pub capabilities: BTreeSet<Capability>,
    /// Safe actor fields copied verbatim into `RequestContext::actor_metadata`.
    pub actor_metadata: BTreeMap<String, Value>,
}

/// Resolves inbound request headers to an authenticated actor.
///
/// Implementations may look up bearer tokens, verify JWTs, call an external
/// identity service, etc. `fslite-server` ships one reference implementation,
/// [`BearerTokenAuthProvider`]; production deployments are expected to
/// provide their own.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticates a request from its headers alone.
    async fn authenticate(&self, headers: &HeaderMap) -> Result<AuthenticatedActor, ApiError>;
}

/// A static bearer-token credential store: `Authorization: Bearer <token>`.
pub struct BearerTokenAuthProvider {
    tokens: HashMap<String, AuthenticatedActor>,
}

impl BearerTokenAuthProvider {
    /// Builds a provider from a fixed token → actor map.
    pub fn new(tokens: HashMap<String, AuthenticatedActor>) -> Self {
        Self { tokens }
    }
}

#[async_trait]
impl AuthProvider for BearerTokenAuthProvider {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<AuthenticatedActor, ApiError> {
        let header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthenticated("missing authorization header".into()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthenticated("expected a Bearer token".into()))?;

        self.tokens
            .get(token)
            .cloned()
            .ok_or_else(|| ApiError::Unauthenticated("unrecognized token".into()))
    }
}

/// An extractor that authenticates the request and enforces that the
/// authenticated actor's workspace matches the `{workspace_id}` path segment.
pub struct Ctx(pub RequestContext);

#[async_trait]
impl<S> FromRequestParts<S> for Ctx
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let actor = app_state.auth.authenticate(&parts.headers).await?;

        let Path(workspace_id) = Path::<WorkspaceId>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedBody("invalid workspace id in path".into()))?;

        if actor.workspace_id != workspace_id {
            return Err(ApiError::WorkspaceMismatch);
        }

        Ok(Ctx(RequestContext::new(
            workspace_id,
            actor.actor_metadata,
            actor.capabilities,
        )))
    }
}
```

`WorkspaceId` needs `Deserialize`/path-extraction support: it already derives `Deserialize` (`#[serde(transparent)]` over a `Uuid` string), which is what `axum::extract::Path<WorkspaceId>` uses — no extra glue required.

Update `main/crates/fslite-server/src/state.rs`:

```rust
use std::sync::Arc;

use fslite_core::{FileSystem, WorkspaceId};

use crate::auth::AuthProvider;

/// Shared, cloneable application state handed to every route.
#[derive(Clone)]
pub struct AppState {
    /// The backend-agnostic filesystem every data route is driven through.
    pub fs: Arc<dyn FileSystem>,
    /// Resolves inbound credentials to a workspace and capability set.
    pub auth: Arc<dyn AuthProvider>,
    /// The workspace `/readyz` probes with a cheap `exists(root)` call.
    pub health_workspace: WorkspaceId,
}
```

Update `main/crates/fslite-server/src/lib.rs` to add `mod auth;` and re-export `pub use auth::{AuthProvider, AuthenticatedActor, BearerTokenAuthProvider, Ctx};`.

Update `main/crates/fslite-server/src/main.rs`'s `AppState` construction to add an `auth` field (a `BearerTokenAuthProvider` seeded from a `FSLITE_TOKENS` env var — `token=workspace_uuid` pairs, comma-separated — documented in a comment; keep this minimal, it is not this task's focus).

Update `main/crates/fslite-server/tests/health.rs`'s `AppState` literal to add the `auth: Arc::new(fslite_server::BearerTokenAuthProvider::new(Default::default()))` field so it still compiles.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test auth --test health`
Expected: PASS (the fourth `auth` test asserts 404, since no `/usage` route exists until Task 13 — this is intentional and documented in the test's comment).

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src crates/fslite-server/tests
git commit -m "feat(fslite-server): auth provider, capability mapping, Ctx extractor"
```

---

## Task 4: `WorkspaceAdmin` trait + readiness probe

**Files:**
- Create: `main/crates/fslite-server/src/admin.rs`
- Modify: `main/crates/fslite-server/src/state.rs` (add `admin: Arc<dyn WorkspaceAdmin>`)
- Modify: `main/crates/fslite-server/src/routes/mod.rs` (add `/readyz`)
- Modify: `main/crates/fslite-server/src/lib.rs`, `src/main.rs`, `tests/support/mod.rs`, `tests/health.rs`, `tests/auth.rs` (thread the new `admin` field through)
- Test: `main/crates/fslite-server/tests/readiness.rs`

**Interfaces:**
- Produces:
  - `#[async_trait] pub trait WorkspaceAdmin: Send + Sync { async fn create_workspace(&self) -> FsResult<Workspace>; async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()>; }` (bridges `SqliteFileSystem`'s inherent `create_workspace`/`delete_workspace`, which are not part of `FileSystem`, into something the server can hold as `Arc<dyn WorkspaceAdmin>` without downcasting).
  - `pub struct SqliteWorkspaceAdmin(pub Arc<SqliteFileSystem>);` implementing it.
  - `GET /readyz` handler using `state.fs.exists(&RequestContext::trusted(state.health_workspace), &VirtualPath::root(), StatOptions::default())`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/readiness.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn readyz_succeeds_once_the_backend_is_reachable() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test readiness`
Expected: FAIL — no `/readyz` route (404).

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/admin.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use fslite_core::{FsResult, WorkspaceId};
use fslite_sqlite::{SqliteFileSystem, Workspace};

/// Workspace lifecycle operations. Not part of `fslite_core::FileSystem`
/// (creating/naming a workspace is backend-specific), so `fslite-server`
/// defines its own narrow trait and adapts each backend to it explicitly
/// rather than downcasting `Arc<dyn FileSystem>`.
#[async_trait]
pub trait WorkspaceAdmin: Send + Sync {
    /// Creates a new isolated workspace with default limits.
    async fn create_workspace(&self) -> FsResult<Workspace>;
    /// Permanently deletes a workspace and everything it contains.
    async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()>;
}

/// Adapts [`SqliteFileSystem`]'s inherent workspace methods to [`WorkspaceAdmin`].
pub struct SqliteWorkspaceAdmin(pub Arc<SqliteFileSystem>);

#[async_trait]
impl WorkspaceAdmin for SqliteWorkspaceAdmin {
    async fn create_workspace(&self) -> FsResult<Workspace> {
        self.0.create_workspace(Default::default()).await
    }

    async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()> {
        self.0.delete_workspace(id).await
    }
}
```

Update `main/crates/fslite-server/src/state.rs` to add `pub admin: Arc<dyn WorkspaceAdmin>,`.

Update `main/crates/fslite-server/src/routes/mod.rs`:

```rust
use axum::Json;
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use fslite_core::{RequestContext, StatOptions, VirtualPath};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

pub fn health_router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn readyz(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = RequestContext::trusted(state.health_workspace);
    state
        .fs
        .exists(&ctx, &VirtualPath::root(), StatOptions::default())
        .await?;
    Ok(Json(json!({ "status": "ready" })))
}
```

Update `main/crates/fslite-server/src/lib.rs` to add `mod admin;` and re-export `pub use admin::{SqliteWorkspaceAdmin, WorkspaceAdmin};`.

Update `main/crates/fslite-server/src/main.rs` to build `admin: Arc::new(SqliteWorkspaceAdmin(Arc::new(fs)))` — note this now requires holding the `SqliteFileSystem` in an `Arc` shared between `fs: Arc<dyn FileSystem>` (via `Arc::new(fs.clone-of-the-same-Arc-cast)`) and `admin`; construct one `Arc<SqliteFileSystem>` first, then produce `fs: sqlite_fs.clone() as Arc<dyn FileSystem>` and `admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs))`.

Update `main/crates/fslite-server/tests/support/mod.rs`'s `fixture()` to build the same way (one `Arc<SqliteFileSystem>`, cloned into both `fs` and wrapped for `admin`), and add `admin` to the returned `AppState`. Update `tests/health.rs` and `tests/auth.rs` `AppState` literals accordingly (or, if they already call `support::fixture()`, no change needed — only `health.rs`'s standalone literal from Task 1 needs updating; migrate it to use `support::fixture()` too, removing the duplication).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server`
Expected: PASS for all tests so far.

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src crates/fslite-server/tests crates/fslite-server/src/main.rs
git commit -m "feat(fslite-server): WorkspaceAdmin trait and readiness probe"
```

---

## Task 5: Tracing middleware and request IDs

**Files:**
- Create: `main/crates/fslite-server/src/tracing_mw.rs`
- Modify: `main/crates/fslite-server/src/lib.rs` (apply the layer in `app()`)
- Modify: `main/crates/fslite-server/src/error.rs` (include `request_id` in error `details` when present)
- Test: `main/crates/fslite-server/tests/tracing.rs`

**Interfaces:**
- Produces: `pub fn request_id_layer() -> impl tower::Layer<...> + Clone` — a small middleware that reads `x-request-id` from the request or generates a UUIDv7, inserts it into `Request::extensions`, and echoes it back as a response header; `pub fn trace_layer() -> tower_http::trace::TraceLayer<...>` configured to log method, path, status, and latency per request. `ApiError` gains an optional `request_id: Option<String>` field threaded from extensions.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/tracing.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::Request;
use fslite_server::app;
use tower::ServiceExt;

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn a_client_supplied_request_id_is_echoed_back() {
    let (state, _workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "caller-supplied-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers().get("x-request-id").unwrap(), "caller-supplied-id");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test tracing`
Expected: FAIL — no `x-request-id` header present yet.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/tracing_mw.rs`:

```rust
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Ensures every request carries an `x-request-id`, generating one when
/// absent, and echoes it back on the response for client-side correlation.
pub async fn request_id(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    request
        .extensions_mut()
        .insert(RequestId(id.clone()));

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// The per-request correlation id, available to handlers via `Extension<RequestId>`.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// A `tower-http` layer logging method, path, status, and latency per request.
pub fn trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}
```

Update `main/crates/fslite-server/src/lib.rs`:

```rust
mod admin;
mod auth;
mod error;
mod routes;
mod state;
mod tracing_mw;

use axum::Router;
use axum::middleware;

pub use admin::{SqliteWorkspaceAdmin, WorkspaceAdmin};
pub use auth::{AuthProvider, AuthenticatedActor, BearerTokenAuthProvider, Ctx};
pub use error::ApiError;
pub use state::AppState;
pub use tracing_mw::RequestId;

/// Builds the complete application router from shared state.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(routes::health_router())
        .with_state(state)
        .layer(middleware::from_fn(tracing_mw::request_id))
        .layer(tracing_mw::trace_layer())
}
```

(Layer ordering matters: `request_id` runs first so the id exists before `trace_layer` logs the request, and both wrap the already-`with_state`'d router.)

Update `main/crates/fslite-server/src/error.rs`'s `IntoResponse for ApiError` to accept an optional ambient request id: since `IntoResponse::into_response(self)` has no access to extensions, add a companion `pub fn into_response_with_request_id(self, request_id: Option<&str>) -> Response` used by a small wrapper, OR simpler — keep `IntoResponse` as-is for Task 2's already-passing unit tests, and instead read the `RequestId` extension inside each handler that wants it in `details`. For this task, only add the field to the envelope for the general default path (no handler-level `details.request_id` yet — that is Task 6+'s concern per-handler, not required for this task's tests to pass). No further change to `error.rs` is required to satisfy this task's two tests.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test tracing`
Expected: PASS

- [ ] **Step 5: Run the full test suite to confirm no regression**

Run: `cargo test -p fslite-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): request-id middleware and HTTP tracing"
```

---

## Task 6: Node metadata routes — `stat`, `exists`, `remove`

**Files:**
- Create: `main/crates/fslite-server/src/dto.rs`
- Create: `main/crates/fslite-server/src/routes/nodes.rs`
- Modify: `main/crates/fslite-server/src/lib.rs` (`mod dto;`, nest `nodes::router()`)
- Test: `main/crates/fslite-server/tests/nodes.rs`

**Interfaces:**
- Produces:
  - `dto.rs`: `pub fn query_bool(params: &HashMap<String, String>, key: &str, default: bool) -> Result<bool, ApiError>`, `pub fn query_u32(...)`, `pub fn query_revision(...) -> Result<Option<Revision>, ApiError>` — small shared query-string parsing helpers reused by every later route module.
  - `routes/nodes.rs`: `pub fn router() -> Router<AppState>` nested at `/v1/workspaces/{workspace_id}/fs/{*path}`, with `GET` → `stat`, `HEAD` → `exists`, `DELETE` → `remove`.
- Consumes: `Ctx` (Task 3), `ApiError` (Task 2), `AppState` (Task 4).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/nodes.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn stat_returns_node_json_for_an_existing_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &VirtualPath::parse("/a.txt").unwrap(),
            WriteSource::from_bytes(b"hi".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.name, "a.txt");
    assert_eq!(node.logical_size, 2);
}

#[tokio::test]
async fn stat_missing_path_is_404_with_not_found_code() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/missing.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn head_reports_existence_with_no_body() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::HEAD)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/missing.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn delete_removes_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state
        .fs
        .write(&ctx, &path, WriteSource::from_bytes(b"hi".to_vec()), Default::default())
        .await
        .unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    assert!(!state.fs.exists(&ctx, &path, Default::default()).await.unwrap());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test nodes`
Expected: FAIL — no `/fs/{*path}` route (404 for stat test too, but the specific `not_found` code assertion and node-body assertions fail).

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/dto.rs`:

```rust
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
```

Create `main/crates/fslite-server/src/routes/nodes.rs`:

```rust
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, method_routing::delete};
use axum::{Json, Router};
use fslite_core::{RemoveOptions, StatOptions, VirtualPath};

use crate::dto::{query_bool, query_revision};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces/{workspace_id}/fs/{*path}",
        get(stat).head(exists).delete(remove),
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
```

Update `main/crates/fslite-server/src/lib.rs`: add `mod dto;`, and in `app()` add `.merge(routes::nodes::router())` (make `routes::nodes` reachable by adding `pub mod nodes;` inside `src/routes/mod.rs`, or re-export via `mod nodes;` + `pub use nodes::router as nodes_router;` — keep consistent with how `health_router` is already exposed: add `pub mod nodes;` to `routes/mod.rs` and call `routes::nodes::router()` from `lib.rs`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test nodes`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): stat/exists/remove node routes"
```

---

## Task 7: Directory routes — `mkdir`, `read_dir`, `tree`

**Files:**
- Create: `main/crates/fslite-server/src/routes/directories.rs`
- Modify: `main/crates/fslite-server/src/lib.rs` / `src/routes/mod.rs` (nest the new router)
- Modify: `main/crates/fslite-server/src/routes/nodes.rs` (extend the `PUT` verb onto `/fs/{*path}` for `mkdir`/`symlink`, disambiguated by `?type=`)
- Test: `main/crates/fslite-server/tests/directories.rs`

**Interfaces:**
- Produces:
  - `routes/directories.rs`: `pub fn router() -> Router<AppState>` with `GET /v1/workspaces/{workspace_id}/directories/{*path}/children` → `read_dir`, `GET .../tree` → `tree`.
  - `routes/nodes.rs` gains a `PUT` handler: `?type=directory` (body `{"parents":bool,"exist_ok":bool,"expected_revision":Option<u64>}`) → `mkdir`; `?type=symlink` (body `{"target":"...","parents":bool,"exist_ok":bool,"expected_revision":Option<u64>}`) → `symlink`; missing/unrecognized `type` → `ApiError::MalformedBody`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/directories.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn put_with_type_directory_creates_a_directory() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/docs?type=directory"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"parents": true, "exist_ok": false}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(state
        .fs
        .exists(&ctx, &VirtualPath::parse("/docs").unwrap(), Default::default())
        .await
        .unwrap());
}

#[tokio::test]
async fn children_lists_direct_descendants() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default())
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/directories//children")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Node> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name, "a.txt");
}
```

(The root path is represented as an empty wildcard segment, i.e. `.../directories//children` — the leading `/` from the route plus an empty capture. Confirm this against axum 0.8's wildcard-matching behavior in Step 4; if an empty wildcard segment does not match, use a dedicated `.../directories/children` route — without a trailing path segment — for the root case, added alongside the wildcard route, and prefer the non-wildcard route when the captured path is empty. Encode whichever behavior is actually observed; do not guess further at plan-writing time.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test directories`
Expected: FAIL — no matching routes.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/routes/directories.rs`:

```rust
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{Page, PageRequest, TreeEntry, TreeOptions, WorkspaceId};

use crate::dto::{query_bool, query_u32};
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces/{workspace_id}/directories/{*path}", get(dispatch))
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
        Some(v) => Some(v.parse::<u32>().map_err(|_| ApiError::MalformedBody("max_depth must be a non-negative integer".into()))?),
    };
    let follow_symlinks = query_bool(&params, "follow_symlinks", false)?;
    let options = TreeOptions::default().max_depth(max_depth).follow_symlinks(follow_symlinks);
    let result: Page<TreeEntry> = state.fs.tree(&ctx, &path, options, page).await?;
    Ok(Json(result).into_response())
}
```

Note: `dispatch`'s use of `axum::response::IntoResponse` requires `use axum::response::IntoResponse;` in scope for `.into_response()` — add that import.

Extend `main/crates/fslite-server/src/routes/nodes.rs`'s router to add `.put(put_node)` on the same route, and implement:

```rust
use axum::extract::Json as JsonBody;
use fslite_core::{CreateOptions, LinkTarget};
use serde::Deserialize;

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
```

Wire `.put(put_node)` into the router in `nodes.rs`, and add `use axum::extract::Query;` / `use std::collections::HashMap;` if not already imported.

Update `routes/mod.rs` to add `pub mod directories;` and merge `directories::router()` in `lib.rs`'s `app()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test directories --test nodes`
Expected: PASS. If the root-wildcard routing assumption from Step 1 doesn't hold under axum 0.8, fix the route pattern (not the test's intent) and re-run.

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): mkdir, symlink, read_dir, tree routes"
```

---

## Task 8: Node actions — `copy`, `move`, `trash`, `touch`, attributes, `read_link`

**Files:**
- Modify: `main/crates/fslite-server/src/routes/nodes.rs` (add `POST` action dispatch, `PATCH` dispatch, `GET .../link-target`)
- Test: `main/crates/fslite-server/tests/node_actions.rs`

**Interfaces:**
- Produces (all appended to `routes/nodes.rs`'s router):
  - `POST /v1/workspaces/{workspace_id}/fs/{*path}?action=copy|move|trash` — body `{"to": "...", "recursive": bool, "overwrite": bool, "expected_revision": Option<u64>}` for `copy`/`move` (fields not applicable to the chosen action are ignored), `{"expected_revision": Option<u64>}` for `trash` (returns `TrashEntry` JSON).
  - `PATCH /v1/workspaces/{workspace_id}/fs/{*path}` — tagged body: `{"op":"touch","create":bool,"expected_revision":Option<u64>}` | `{"op":"set_attribute","key":"...","value_base64":"...","expected_revision":Option<u64>}` | `{"op":"remove_attribute","key":"...","expected_revision":Option<u64>}`.
  - `GET /v1/workspaces/{workspace_id}/fs/{*path}/link-target` — reuses the same "strip a known suffix off the wildcard capture" technique from Task 7's `directories::dispatch`, added as a second route on the existing `/fs/{*path}` `GET` handler (check the suffix before treating the whole capture as a stat path).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/node_actions.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use base64::Engine;
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

async fn seed_file(state: &fslite_server::AppState, workspace_id: fslite_core::WorkspaceId, path: &str) {
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(&ctx, &VirtualPath::parse(path).unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn action_copy_duplicates_a_file() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt?action=copy"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"to": "/b.txt"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let ctx = RequestContext::trusted(workspace_id);
    assert!(state.fs.exists(&ctx, &VirtualPath::parse("/b.txt").unwrap(), Default::default()).await.unwrap());
}

#[tokio::test]
async fn action_trash_moves_a_file_to_trash() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt?action=trash"))
                .header("content-type", "application/json"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let entry: fslite_core::TrashEntry = serde_json::from_slice(&body).unwrap();
    assert_eq!(entry.original_path.as_str(), "/a.txt");
}

#[tokio::test]
async fn patch_set_attribute_round_trips_arbitrary_bytes() {
    let (state, workspace_id) = support::fixture().await;
    seed_file(&state, workspace_id, "/a.txt").await;
    let value = base64::engine::general_purpose::STANDARD.encode(b"\x00\x01binary");

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PATCH)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/a.txt"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"op": "set_attribute", "key": "k", "value_base64": value}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert!(node.attributes.contains_key("k"));
}

#[tokio::test]
async fn link_target_returns_the_stored_target_without_resolving_it() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    seed_file(&state, workspace_id, "/target.txt").await;
    state
        .fs
        .symlink(
            &ctx,
            &fslite_core::LinkTarget::parse("/target.txt").unwrap(),
            &VirtualPath::parse("/link").unwrap(),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/link/link-target")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["target"], "/target.txt");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test node_actions`
Expected: FAIL — no action dispatch, no `PATCH`, no `link-target` route.

- [ ] **Step 3: Write the minimal implementation**

Append to `main/crates/fslite-server/src/routes/nodes.rs` (and change `stat`'s registration to route through a `get(get_dispatch)` that first checks for a `/link-target` suffix, mirroring Task 7's technique):

```rust
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
```

Rename the existing `stat` handler to `stat_inner` (same body, signature adjusted to match the call from `get_dispatch` above), and change the router registration to:

```rust
.route(
    "/v1/workspaces/{workspace_id}/fs/{*path}",
    get(get_dispatch).head(exists).delete(remove).put(put_node).patch(patch_node).post(post_action),
)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test node_actions --test nodes`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src/routes/nodes.rs
git commit -m "feat(fslite-server): copy/move/trash actions, touch/attribute PATCH, read_link"
```

---

## Task 9: Trash routes — `list_trash`, `restore`, `purge`

**Files:**
- Create: `main/crates/fslite-server/src/routes/trash.rs`
- Modify: `main/crates/fslite-server/src/routes/mod.rs`, `src/lib.rs`
- Test: `main/crates/fslite-server/tests/trash.rs`

**Interfaces:**
- Produces: `pub fn router() -> Router<AppState>` with `GET /v1/workspaces/{workspace_id}/trash` → `list_trash`, `POST /v1/workspaces/{workspace_id}/trash/{trash_id}/restore` → `restore` (body `{"destination": Option<String>, "expected_revision": Option<u64>}`), `DELETE /v1/workspaces/{workspace_id}/trash/{trash_id}` → `purge`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/trash.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn restore_and_purge_round_trip() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state.fs.write(&ctx, &path, WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();
    let entry = state.fs.trash(&ctx, &path, Default::default()).await.unwrap();

    let list = app(state.clone())
        .oneshot(auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/trash"))).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body = list.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::TrashEntry> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);

    let restore = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/trash/{}/restore", entry.id))
                .header("content-type", "application/json"))
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), 200);
    assert!(state.fs.exists(&ctx, &path, Default::default()).await.unwrap());

    let entry2 = state.fs.trash(&ctx, &path, Default::default()).await.unwrap();
    let purge = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/workspaces/{workspace_id}/trash/{}", entry2.id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(purge.status(), 200);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test trash`
Expected: FAIL — no trash routes yet.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/routes/trash.rs`:

```rust
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use fslite_core::{PageRequest, TrashId, VirtualPath, WorkspaceId};
use serde::Deserialize;

use crate::dto::query_u32;
use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces/{workspace_id}/trash", get(list_trash))
        .route(
            "/v1/workspaces/{workspace_id}/trash/{trash_id}",
            axum::routing::delete(purge),
        )
        .route(
            "/v1/workspaces/{workspace_id}/trash/{trash_id}/restore",
            axum::routing::post(restore),
        )
}

async fn list_trash(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<fslite_core::Page<fslite_core::TrashEntry>>, ApiError> {
    let page = PageRequest::default()
        .cursor(params.get("cursor").cloned())
        .limit(query_u32(&params, "limit", fslite_core::DEFAULT_PAGE_LIMIT)?);
    Ok(Json(state.fs.list_trash(&ctx, page).await?))
}

fn parse_trash_id(raw: &str) -> Result<TrashId, ApiError> {
    TrashId::parse(raw).map_err(|_| ApiError::MalformedBody("invalid trash id".into()))
}

async fn purge(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, trash_id)): Path<(WorkspaceId, String)>,
) -> Result<StatusCode, ApiError> {
    let trash_id = parse_trash_id(&trash_id)?;
    state.fs.purge(&ctx, trash_id).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RestoreBody {
    destination: Option<String>,
    expected_revision: Option<u64>,
}

async fn restore(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Path((_workspace_id, trash_id)): Path<(WorkspaceId, String)>,
    body: axum::body::Bytes,
) -> Result<Json<fslite_core::Node>, ApiError> {
    let trash_id = parse_trash_id(&trash_id)?;
    let parsed: RestoreBody = if body.is_empty() {
        RestoreBody { destination: None, expected_revision: None }
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedBody(e.to_string()))?
    };
    let destination = parsed
        .destination
        .map(|raw| VirtualPath::parse(&raw))
        .transpose()
        .map_err(|e| ApiError::MalformedBody(e.message().to_string()))?;
    let expected_revision = match parsed.expected_revision {
        None => None,
        Some(0) => return Err(ApiError::MalformedBody("expected_revision must be nonzero".into())),
        Some(v) => fslite_core::Revision::new(v),
    };
    let options = fslite_core::MutationOptions::default().expected_revision(expected_revision);
    Ok(Json(
        state
            .fs
            .restore(&ctx, trash_id, destination.as_ref(), options)
            .await?,
    ))
}
```

Update `routes/mod.rs` with `pub mod trash;`, and `lib.rs`'s `app()` with `.merge(routes::trash::router())`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test trash`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): trash list/restore/purge routes"
```

---

## Task 10: Byte-range resolution + content routes (`read`, `write`, `write_at`, `append`, `truncate`)

**Files:**
- Create: `main/crates/fslite-server/src/range.rs`
- Create: `main/crates/fslite-server/src/routes/content.rs`
- Modify: `main/crates/fslite-server/src/routes/mod.rs`, `src/lib.rs`
- Test: `main/crates/fslite-server/tests/range.rs` (unit-level, no HTTP)
- Test: `main/crates/fslite-server/tests/content.rs`

**Interfaces:**
- Produces:
  - `range.rs`: `pub fn resolve_range(header: &str, logical_size: u64) -> Result<fslite_core::ByteRange, RangeError>`, `pub enum RangeError { Malformed, MultiRangeUnsupported, Unsatisfiable }`. Supports `bytes=start-end` (inclusive end), `bytes=start-` (open), `bytes=-suffix_len` (suffix).
  - `content.rs`: `pub fn router() -> Router<AppState>` — `GET /v1/workspaces/{workspace_id}/content/{*path}` (streams body, honors `Range`, sets `Accept-Ranges: bytes`, returns 206 + `Content-Range` when a range was requested), `PUT` (streamed write), `PATCH?offset=N` (write_at), `POST?action=append`, `POST?action=truncate` (JSON body `{"length": u64, "expected_revision": Option<u64>}`).

- [ ] **Step 1: Write the failing test — range resolution (pure function, no server)**

Create `main/crates/fslite-server/tests/range.rs`:

```rust
use fslite_server::range::{resolve_range, RangeError};

#[test]
fn fully_specified_range_is_inclusive_end_converted_to_exclusive() {
    let range = resolve_range("bytes=0-9", 100).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 10);
}

#[test]
fn open_ended_range_extends_to_the_logical_size() {
    let range = resolve_range("bytes=90-", 100).unwrap();
    assert_eq!(range.start, 90);
    assert_eq!(range.end, 100);
}

#[test]
fn suffix_range_takes_the_last_n_bytes() {
    let range = resolve_range("bytes=-10", 100).unwrap();
    assert_eq!(range.start, 90);
    assert_eq!(range.end, 100);
}

#[test]
fn suffix_longer_than_the_file_clamps_to_the_whole_file() {
    let range = resolve_range("bytes=-1000", 100).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 100);
}

#[test]
fn start_beyond_the_file_is_unsatisfiable() {
    assert!(matches!(resolve_range("bytes=200-300", 100), Err(RangeError::Unsatisfiable)));
}

#[test]
fn multiple_ranges_are_rejected() {
    assert!(matches!(resolve_range("bytes=0-9,20-29", 100), Err(RangeError::MultiRangeUnsupported)));
}

#[test]
fn malformed_unit_is_rejected() {
    assert!(matches!(resolve_range("items=0-9", 100), Err(RangeError::Malformed)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test range`
Expected: FAIL to compile — `fslite_server::range` does not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/range.rs`:

```rust
use fslite_core::ByteRange;

/// Why an HTTP `Range` header could not be resolved to a concrete `ByteRange`.
#[derive(Debug, Eq, PartialEq)]
pub enum RangeError {
    /// The header was not a well-formed single `bytes=` range.
    Malformed,
    /// The header requested more than one range; unsupported.
    MultiRangeUnsupported,
    /// The requested range starts at or beyond the content length.
    Unsatisfiable,
}

/// Resolves a single-range `Range: bytes=...` header value (without the
/// leading header name) against a known content length. Supports
/// `start-end` (inclusive end), `start-` (open-ended), and `-suffix_len`
/// (last `suffix_len` bytes, clamped to the content length).
pub fn resolve_range(header: &str, logical_size: u64) -> Result<ByteRange, RangeError> {
    let spec = header.strip_prefix("bytes=").ok_or(RangeError::Malformed)?;
    if spec.contains(',') {
        return Err(RangeError::MultiRangeUnsupported);
    }

    let (start_str, end_str) = spec.split_once('-').ok_or(RangeError::Malformed)?;

    if start_str.is_empty() {
        // Suffix range: "-N" = last N bytes.
        let suffix_len: u64 = end_str.parse().map_err(|_| RangeError::Malformed)?;
        if suffix_len == 0 {
            return Err(RangeError::Malformed);
        }
        let start = logical_size.saturating_sub(suffix_len);
        return Ok(ByteRange::new(start, logical_size));
    }

    let start: u64 = start_str.parse().map_err(|_| RangeError::Malformed)?;
    if start >= logical_size {
        return Err(RangeError::Unsatisfiable);
    }

    let end = if end_str.is_empty() {
        logical_size
    } else {
        let inclusive_end: u64 = end_str.parse().map_err(|_| RangeError::Malformed)?;
        (inclusive_end + 1).min(logical_size)
    };

    Ok(ByteRange::new(start, end))
}
```

Update `main/crates/fslite-server/src/lib.rs` to add `pub mod range;` (public, since the test imports `fslite_server::range::{resolve_range, RangeError}`).

- [ ] **Step 4: Run test to verify range tests pass**

Run: `cargo test -p fslite-server --test range`
Expected: PASS

- [ ] **Step 5: Write the failing test — content routes**

Create `main/crates/fslite-server/tests/content.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn put_then_get_round_trips_bytes() {
    let (state, workspace_id) = support::fixture().await;

    let put = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt")))
                .body(Body::from("hello world"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), 200);

    let get = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/content/a.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    assert_eq!(get.headers().get("accept-ranges").unwrap(), "bytes");
    let body = get.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello world");
}

#[tokio::test]
async fn get_with_range_header_returns_206_and_a_slice() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state
        .fs
        .write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"hello world".to_vec()), Default::default())
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                .header("range", "bytes=0-4"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers().get("content-range").unwrap(), "bytes 0-4/11");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn action_append_extends_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state.fs.write(&ctx, &path, WriteSource::from_bytes(b"hello ".to_vec()), Default::default()).await.unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt?action=append")))
                .body(Body::from("world"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let read = state.fs.read(&ctx, &path, Default::default()).await.unwrap();
    let mut stream = read.into_stream();
    let mut bytes = Vec::new();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(bytes, b"hello world");
}

#[tokio::test]
async fn action_truncate_shortens_a_file() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    let path = VirtualPath::parse("/a.txt").unwrap();
    state.fs.write(&ctx, &path, WriteSource::from_bytes(b"hello world".to_vec()), Default::default()).await.unwrap();

    let response = app(state.clone())
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt?action=truncate"))
                .header("content-type", "application/json"))
                .body(Body::from(serde_json::json!({"length": 5}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let node: fslite_core::Node = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.logical_size, 5);
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test -p fslite-server --test content`
Expected: FAIL — no `/content/{*path}` routes.

- [ ] **Step 7: Write the minimal implementation**

Create `main/crates/fslite-server/src/routes/content.rs`:

```rust
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
```

Update `routes/mod.rs` with `pub mod content;`, and `lib.rs`'s `app()` with `.merge(routes::content::router())`.

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p fslite-server --test content --test range`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): streaming content routes with HTTP range support"
```

---

## Task 11: Search & changes routes — `glob`, `find`, `search_content`, `changes`

**Files:**
- Create: `main/crates/fslite-server/src/routes/search.rs`
- Modify: `main/crates/fslite-server/src/dto.rs` (add base64 DTOs)
- Modify: `main/crates/fslite-server/src/routes/mod.rs`, `src/lib.rs`
- Test: `main/crates/fslite-server/tests/search.rs`

**Interfaces:**
- Produces:
  - `dto.rs`: `pub struct ContentQueryRequest { pub root: VirtualPath, pub needle_base64: String }` with `impl TryFrom<ContentQueryRequest> for ContentQuery`; `pub struct SearchMatchDto { pub node: Node, pub path: VirtualPath, pub range: ByteRange, pub preview_base64: String }` with `impl From<SearchMatch> for SearchMatchDto`.
  - `routes/search.rs`: `pub fn router() -> Router<AppState>` — `GET /v1/workspaces/{workspace_id}/search/glob` (query: `pattern`, `cursor`, `limit`), `POST /v1/workspaces/{workspace_id}/search/find` (body: `FindQuery` fields + `page`, both deserialized directly since `FindQuery` already implements `Deserialize`), `POST /v1/workspaces/{workspace_id}/search/content` (body: `{"root":..,"needle_base64":..,"page":..}`, response `Page<SearchMatchDto>`), `GET /v1/workspaces/{workspace_id}/changes` (query: `after`, `cursor`, `limit`).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/search.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use base64::Engine;
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn glob_finds_matching_paths() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/search/glob?pattern=/*.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Node> = serde_json::from_slice(&body).unwrap();
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn find_accepts_the_core_query_shape_directly() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/search/find"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"query": {"root": "/", "name_contains": "a"}, "page": {}}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn search_content_base64_encodes_needle_and_preview() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"hello world".to_vec()), Default::default()).await.unwrap();
    let needle = base64::engine::general_purpose::STANDARD.encode(b"world");

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/search/content"))
                .header("content-type", "application/json"))
                .body(Body::from(json!({"root": "/", "needle_base64": needle, "page": {}}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert!(json["items"][0]["preview_base64"].is_string());
}

#[tokio::test]
async fn changes_lists_committed_mutations() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = RequestContext::trusted(workspace_id);
    state.fs.write(&ctx, &VirtualPath::parse("/a.txt").unwrap(), WriteSource::from_bytes(b"x".to_vec()), Default::default()).await.unwrap();

    let response = app(state)
        .oneshot(auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/changes"))).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: fslite_core::Page<fslite_core::Change> = serde_json::from_slice(&body).unwrap();
    assert!(!page.items.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test search`
Expected: FAIL — no `/search/*` or `/changes` routes.

- [ ] **Step 3: Write the minimal implementation**

Append to `main/crates/fslite-server/src/dto.rs`:

```rust
use base64::Engine;
use fslite_core::{ByteRange, ContentQuery, Node, SearchMatch, VirtualPath};
use serde::{Deserialize, Serialize};

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
```

Create `main/crates/fslite-server/src/routes/search.rs`:

```rust
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

#[derive(Deserialize)]
struct FindRequest {
    query: FindQuery,
    #[serde(default)]
    page: PageRequest,
}

async fn find(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(body): Json<FindRequest>,
) -> Result<Json<Page<fslite_core::Node>>, ApiError> {
    Ok(Json(state.fs.find(&ctx, body.query, body.page).await?))
}

#[derive(Deserialize)]
struct SearchContentRequest {
    #[serde(flatten)]
    query: ContentQueryRequest,
    #[serde(default)]
    page: PageRequest,
}

async fn search_content(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(body): Json<SearchContentRequest>,
) -> Result<Json<Page<SearchMatchDto>>, ApiError> {
    let query: fslite_core::ContentQuery = body.query.try_into()?;
    let page = state.fs.search_content(&ctx, query, body.page).await?;
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
```

(`PageRequest` needs `Deserialize` with a `#[serde(default)]`-friendly shape for `#[serde(default)] page: PageRequest` to work when the client omits `page` entirely — it already derives `Deserialize`, and `Default` is implemented, so `#[serde(default)]` on the field works as-is.)

`Page::new` used in `search_content` requires `pub fn new` — already present on `fslite_core::Page<T>`.

Update `routes/mod.rs` with `pub mod search;`, and `lib.rs`'s `app()` with `.merge(routes::search::router())`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test search`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): glob, find, search_content, changes routes"
```

---

## Task 12: Batch route

**Files:**
- Create: `main/crates/fslite-server/src/routes/batch.rs`
- Modify: `main/crates/fslite-server/src/routes/mod.rs`, `src/lib.rs`
- Test: `main/crates/fslite-server/tests/batch.rs`

**Interfaces:**
- Produces: `pub fn router() -> Router<AppState>` — `POST /v1/workspaces/{workspace_id}/batch`, body `{"operations": [BatchOperation, ...]}` deserialized directly via `fslite_core::BatchOperation`'s own `Deserialize` impl, response `{"results": [BatchResult, ...]}`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/batch.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn batch_runs_operations_atomically_and_reports_the_failing_index() {
    let (state, workspace_id) = support::fixture().await;

    let ops = json!({
        "operations": [
            {"mkdir": {"path": "/a", "options": {}}},
            {"mkdir": {"path": "/a", "options": {}}}
        ]
    });

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/batch"))
                .header("content-type", "application/json"))
                .body(Body::from(ops.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409); // AlreadyExists on the second mkdir
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["details"]["index"], 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test batch`
Expected: FAIL — no `/batch` route (also confirms the exact JSON shape `{"mkdir": {...}}` that `BatchOperation`'s default derive produces; if the observed shape differs, adjust the test body to match what `serde_json::to_string(&BatchOperation::Mkdir{..})` actually produces rather than guessing further here).

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/routes/batch.rs`:

```rust
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use fslite_core::BatchOperation;
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/workspaces/{workspace_id}/batch", post(batch))
}

#[derive(Deserialize)]
struct BatchRequest {
    operations: Vec<BatchOperation>,
}

async fn batch(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
    Json(body): Json<BatchRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let results = state.fs.batch(&ctx, body.operations).await?;
    Ok(Json(serde_json::json!({ "results": results })))
}
```

Update `routes/mod.rs` with `pub mod batch;`, and `lib.rs`'s `app()` with `.merge(routes::batch::router())`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-server --test batch`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/src
git commit -m "feat(fslite-server): batch route"
```

---

## Task 13: Workspace admin routes — create, delete, usage

**Files:**
- Create: `main/crates/fslite-server/src/routes/workspaces.rs`
- Modify: `main/crates/fslite-server/src/routes/mod.rs`, `src/lib.rs`
- Test: `main/crates/fslite-server/tests/workspaces.rs`

This is the task that finally makes the `auth.rs`'s `valid_token_reaches_the_handler` test (Task 3) meaningful — update that test's final assertion from `404` to `200` once this task lands.

**Interfaces:**
- Produces: `pub fn router() -> Router<AppState>` — `POST /v1/workspaces` (uses `state.admin`, response `Workspace`-shaped JSON — note `fslite_sqlite::Workspace` does **not** derive `Serialize`; define a small local DTO), `DELETE /v1/workspaces/{workspace_id}` (uses `state.admin`; since these two routes create/destroy the workspace itself, they authenticate via the same bearer token scheme but **do not** use the `Ctx` extractor's workspace-match check — a token authorizes `create`/`delete` for its own `AuthenticatedActor.workspace_id` only when deleting; `create` has no workspace to match against yet, so it only requires `Capability::WorkspaceAdmin` on the token used, checked directly against `state.auth.authenticate`), `GET /v1/workspaces/{workspace_id}/usage` (uses `Ctx`, calls `workspace_usage`).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-server/tests/workspaces.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request};
use fslite_server::app;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

#[tokio::test]
async fn usage_reports_active_node_count() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/usage"))).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let usage: fslite_core::WorkspaceUsage = serde_json::from_slice(&body).unwrap();
    assert_eq!(usage.active_nodes, 1); // just the workspace root
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-server --test workspaces`
Expected: FAIL — no `/usage` route (404).

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-server/src/routes/workspaces.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use fslite_core::WorkspaceId;
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;
use crate::Ctx;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces", post(create_workspace))
        .route("/v1/workspaces/{workspace_id}", axum::routing::delete(delete_workspace))
        .route("/v1/workspaces/{workspace_id}/usage", get(usage))
}

#[derive(Serialize)]
struct WorkspaceDto {
    id: WorkspaceId,
    created_at_ms: i64,
    max_bytes: u64,
    max_nodes: u64,
    max_file_bytes: u64,
}

async fn create_workspace(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<WorkspaceDto>, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    if !actor.capabilities.contains(&fslite_core::Capability::WorkspaceAdmin) {
        return Err(ApiError::Domain(fslite_core::FsError::permission_denied("create_workspace")));
    }
    let workspace = state.admin.create_workspace().await?;
    Ok(Json(WorkspaceDto {
        id: workspace.id,
        created_at_ms: workspace.created_at_ms,
        max_bytes: workspace.max_bytes,
        max_nodes: workspace.max_nodes,
        max_file_bytes: workspace.max_file_bytes,
    }))
}

async fn delete_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<WorkspaceId>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    if actor.workspace_id != workspace_id || !actor.capabilities.contains(&fslite_core::Capability::WorkspaceAdmin) {
        return Err(ApiError::WorkspaceMismatch);
    }
    state.admin.delete_workspace(workspace_id).await?;
    Ok(StatusCode::OK)
}

async fn usage(
    State(state): State<AppState>,
    Ctx(ctx): Ctx,
) -> Result<Json<fslite_core::WorkspaceUsage>, ApiError> {
    Ok(Json(state.fs.workspace_usage(&ctx).await?))
}
```

Update `routes/mod.rs` with `pub mod workspaces;`, and `lib.rs`'s `app()` with `.merge(routes::workspaces::router())`.

- [ ] **Step 4: Update Task 3's now-stale assertion**

In `main/crates/fslite-server/tests/auth.rs`, change `valid_token_reaches_the_handler`'s final assertion from `assert_eq!(response.status(), 404);` to `assert_eq!(response.status(), 200);` and update its comment to say the usage route now exists.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p fslite-server`
Expected: PASS — full suite.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-server/src crates/fslite-server/tests/auth.rs
git commit -m "feat(fslite-server): workspace create/delete/usage routes"
```

---

## Task 14: HTTP contract test suite

**Files:**
- Create: `main/crates/fslite-server/tests/contract.rs`

**Interfaces:**
- Consumes: everything built in Tasks 1–13, plus `support::fixture()`.
- Produces: no new production code — this task is pure test coverage proving the full router behaves as one coherent contract, not just per-route unit tests.

- [ ] **Step 1: Write the contract test**

Create `main/crates/fslite-server/tests/contract.rs`:

```rust
mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use fslite_server::app;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

fn auth(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header("authorization", format!("Bearer {}", support::TOKEN))
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Exercises the full lifecycle of a file through the HTTP API exactly as
/// an external client would: create a directory, write content, read it
/// back, list it, search for it, trash it, restore it, then permanently
/// remove it — asserting the JSON contract at every step, not just status
/// codes.
#[tokio::test]
async fn full_resource_lifecycle_via_http() {
    let (state, workspace_id) = support::fixture().await;
    let app_router = app(state);

    let mkdir = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/docs?type=directory"))
                .header("content-type", "application/json"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mkdir.status(), StatusCode::OK);

    let put = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/content/docs/readme.txt")))
                .body(Body::from("hello contract"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
    let node = json_body(put).await;
    assert_eq!(node["logical_size"], 14);
    let first_revision = node["revision"].clone();

    let children = app_router
        .clone()
        .oneshot(auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/directories/docs/children"))).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let children = json_body(children).await;
    assert_eq!(children["items"].as_array().unwrap().len(), 1);

    // A stale expected_revision is rejected with 412.
    let stale_write = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!("/v1/workspaces/{workspace_id}/content/docs/readme.txt?expected_revision=999")))
                .body(Body::from("stale"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_write.status(), StatusCode::PRECONDITION_FAILED);
    let stale_error = json_body(stale_write).await;
    assert_eq!(stale_error["error"]["code"], "revision_conflict");

    // A correct expected_revision succeeds.
    let good_write = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::PUT)
                .uri(format!(
                    "/v1/workspaces/{workspace_id}/content/docs/readme.txt?expected_revision={}",
                    first_revision.as_str().unwrap_or(&first_revision.to_string())
                )))
                .body(Body::from("updated"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(good_write.status(), StatusCode::OK);

    let trash = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/docs/readme.txt?action=trash"))
                .header("content-type", "application/json"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trash.status(), StatusCode::OK);
    let entry = json_body(trash).await;
    let trash_id = entry["id"].as_str().unwrap().to_string();

    let restore = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/workspaces/{workspace_id}/trash/{trash_id}/restore"))
                .header("content-type", "application/json"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), StatusCode::OK);

    let remove = app_router
        .clone()
        .oneshot(
            auth(Request::builder()
                .method(Method::DELETE)
                .uri(format!("/v1/workspaces/{workspace_id}/fs/docs/readme.txt")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove.status(), StatusCode::OK);
}

#[tokio::test]
async fn range_not_satisfiable_returns_416_with_total_length() {
    let (state, workspace_id) = support::fixture().await;
    let ctx = fslite_core::RequestContext::trusted(workspace_id);
    state
        .fs
        .write(
            &ctx,
            &fslite_core::VirtualPath::parse("/a.txt").unwrap(),
            fslite_core::WriteSource::from_bytes(b"short".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();

    let response = app(state)
        .oneshot(
            auth(Request::builder()
                .uri(format!("/v1/workspaces/{workspace_id}/content/a.txt"))
                .header("range", "bytes=100-200"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(response.headers().get("content-range").unwrap(), "bytes */5");
}

#[tokio::test]
async fn every_error_response_is_valid_json_with_the_envelope_shape() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            auth(Request::builder().uri(format!("/v1/workspaces/{workspace_id}/fs/does-not-exist")))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = json_body(response).await;
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
    assert!(json["error"]["details"].is_object() || json["error"]["details"].is_null());
}
```

- [ ] **Step 2: Run test to verify current state**

Run: `cargo test -p fslite-server --test contract`
Expected: Mostly PASS if Tasks 1–13 are complete and correct; any failure here indicates a real integration bug between route modules (e.g. a query-param name mismatch between the `write` route's `expected_revision` and what `stat`/`write` responses actually name the revision field) — fix the production code, not the test, unless the test's expectation is itself wrong per the route table above.

- [ ] **Step 3: Fix any integration gaps found**

There is no separate "implementation" step here — Task 14 is a pure integration checkpoint. If a failure surfaces a genuine gap (e.g. `Revision`'s JSON representation is a bare number, not a string, since it's `#[serde(transparent)]` over `u64` — adjust the `good_write` request above to use `first_revision.as_u64().unwrap()` in the URL instead of `.as_str()`), fix the test to match the real, verified wire shape and re-run.

- [ ] **Step 4: Run the full workspace test suite**

Run: `cargo test --workspace --all-features && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-server/tests/contract.rs
git commit -m "test(fslite-server): end-to-end HTTP contract suite"
```

---

## Self-Review Notes

- **Spec coverage:** authentication adapter (Task 3), capability mapping (Task 3, via `AuthenticatedActor.capabilities` → `RequestContext`), resource-oriented HTTP API (Tasks 6–13, route table above), ranges (Task 10), streaming (Task 10, request and response bodies), JSON errors (Task 2), health (Task 1), readiness (Task 4), tracing (Task 5), HTTP contract tests (Task 14). All 28 `FileSystem` trait methods plus `create_workspace`/`delete_workspace` are routed.
- **Known, deliberately documented gaps** (call out to the implementer, not silent TODOs): `FsError`'s client-body-stream errors are mapped to `InternalStorageFailure` for lack of a more specific `ErrorCode` (Task 10) — this is a limitation of the frozen `fslite-core` enum, not something either plan should patch around by modifying core. Multi-range `Range` headers are rejected outright (Task 10) rather than partially supported. `BearerTokenAuthProvider` is a reference/dev implementation, not a production auth system — the `AuthProvider` trait is the extension point.
