# Persistent fslite-server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `fslite-server` a persistent zero-configuration binary with credential identity and an atomic same-workspace reset API suitable for the Astro showcase.

**Architecture:** Add the reset primitive at the SQLite administrative boundary, then expose identity and reset through the existing Axum/auth abstractions. Keep path resolution, persisted credential state, and first-run bootstrap in small binary-only modules so embedders of the `fslite-server` library do not inherit process-level policy.

**Tech Stack:** Rust 1.85, edition 2024, Tokio, Axum 0.8, rusqlite/tokio-rusqlite, Clap 4, directories 6, atomic-write-file, serde, UUID v4/v7, Docker.

## Global Constraints

- Preserve Rust 1.85 and edition 2024 compatibility.
- Print exactly `No database or workspace found, creating default database and workspace` when the database or configured workspace is absent.
- Default to a persistent platform data directory and `127.0.0.1:8080`; Docker overrides to `/data` and `0.0.0.0:8080`.
- Never accept a plaintext `--token` argument; accept `FSLITE_TOKEN`, `FSLITE_TOKEN_FILE`, or `--token-file`.
- Generated credential files are atomically written and mode `0600` on Unix.
- `GET /v1/me` returns only workspace ID and capabilities.
- Reset preserves workspace ID and quotas while removing non-root nodes, content, attributes, trash, usage, and change history in one SQLite transaction.
- Additive public SQLite and server APIs release as `0.2.0` under `SEMVER.md`.
- Do not publish crates, create tags, push images, or deploy services during implementation.

---

## File Map

### SQLite reset

- Modify `crates/fslite-sqlite/src/workspace.rs`: transactional reset query.
- Modify `crates/fslite-sqlite/src/lib.rs`: public `SqliteFileSystem::reset_workspace` administrative method.
- Modify `crates/fslite-sqlite/tests/workspaces.rs`: reset completeness, quota preservation, persistence, and rollback tests.

### HTTP identity and reset

- Create `crates/fslite-server/src/routes/identity.rs`: authenticated `GET /v1/me`.
- Modify `crates/fslite-server/src/routes/mod.rs`: register the identity module.
- Modify `crates/fslite-server/src/lib.rs`: merge the identity router.
- Modify `crates/fslite-server/src/admin.rs`: extend `WorkspaceAdmin` with reset.
- Modify `crates/fslite-server/src/routes/workspaces.rs`: protected reset route.
- Create `crates/fslite-server/tests/identity.rs`: identity contract tests.
- Modify `crates/fslite-server/tests/workspaces.rs`: reset route authorization and behavior.

### Binary configuration and bootstrap

- Create `crates/fslite-server/src/server_config.rs`: CLI/environment/config precedence and platform paths.
- Create `crates/fslite-server/src/credential_store.rs`: state parsing, atomic writes, and permissions.
- Create `crates/fslite-server/src/server_bootstrap.rs`: database/workspace/token bootstrap.
- Modify `crates/fslite-server/src/main.rs`: parse, bootstrap, wire state, print connection guidance, and serve.
- Create `crates/fslite-server/tests/binary_bootstrap.rs`: installed-binary first-run/restart smoke coverage.
- Modify `Cargo.toml` and `crates/fslite-server/Cargo.toml`: required dependencies and UUID v4.

### Packaging and documentation

- Create `crates/fslite-server/Dockerfile`: non-root multi-stage image.
- Create `crates/fslite-server/docker-entrypoint.sh`: validate writable data path without embedding secrets.
- Create `crates/fslite-server/tests/container_smoke.sh`: repeatable first-run/restart container smoke.
- Modify `.dockerignore`: bounded build context.
- Modify `README.md`, `CHANGELOG.md`, `RELEASE.md`, `crates/fslite-sqlite/RELEASE-NOTES.md`, and `crates/fslite-server/RELEASE-NOTES.md`: user and release documentation.
- Modify affected `Cargo.toml` files and `Cargo.lock`: `0.2.0` release preparation and compatible local dependency requirements.

---

### Task 1: Transactional SQLite Workspace Reset

**Files:**
- Modify: `crates/fslite-sqlite/src/workspace.rs`
- Modify: `crates/fslite-sqlite/src/lib.rs`
- Test: `crates/fslite-sqlite/tests/workspaces.rs`

**Interfaces:**
- Consumes: `WorkspaceId`, `FsResult`, the existing `workspaces`, `nodes`, `content_generations`, `changes`, `attributes`, and `trash` schema.
- Produces: `SqliteFileSystem::reset_workspace(&self, workspace_id: WorkspaceId) -> FsResult<()>` for `SqliteWorkspaceAdmin` in Task 3.

- [ ] **Step 1: Write the reset completeness and quota-preservation test**

Append a test that creates non-default quotas, a directory, content, an
attribute, a trashed node, and change records, then resets and asserts only a
fresh root remains:

```rust
#[tokio::test]
async fn reset_workspace_preserves_identity_and_quotas_but_clears_state() {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await.unwrap();
    let mut limits = fslite_sqlite::WorkspaceOptions::default();
    limits.max_bytes = 10 * 1024 * 1024;
    limits.max_nodes = 250;
    limits.max_file_bytes = 1024 * 1024;
    let workspace = fs.create_workspace(limits).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    let docs = VirtualPath::parse("/docs").unwrap();
    let file = VirtualPath::parse("/docs/readme.md").unwrap();

    fs.mkdir(&ctx, &docs, Default::default()).await.unwrap();
    fs.write(
        &ctx,
        &file,
        fslite_core::WriteSource::from_bytes(b"hello".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();
    fs.set_attribute(&ctx, &file, "demo", b"yes", Default::default())
        .await
        .unwrap();
    fs.trash(&ctx, &file, Default::default()).await.unwrap();

    fs.reset_workspace(workspace.id).await.unwrap();

    let root = fs
        .stat(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert_eq!(root.kind, NodeKind::Directory);
    let page = fs
        .read_dir(&ctx, &VirtualPath::root(), Default::default())
        .await
        .unwrap();
    assert!(page.items.is_empty());
    assert!(fs.list_trash(&ctx, Default::default()).await.unwrap().items.is_empty());
    assert!(fs.changes(&ctx, None, Default::default()).await.unwrap().items.is_empty());

    let usage = fs.workspace_usage(&ctx).await.unwrap();
    assert_eq!(usage.workspace_id, workspace.id);
    assert_eq!(usage.active_nodes, 1);
    assert_eq!(usage.trashed_nodes, 0);
    assert_eq!(usage.active_logical_bytes, 0);
    assert_eq!(usage.staged_bytes, 0);
    assert_eq!(usage.max_logical_bytes, limits.max_bytes);
    assert_eq!(usage.max_nodes, limits.max_nodes);
    assert_eq!(usage.max_file_bytes, limits.max_file_bytes);
}
```

- [ ] **Step 2: Write persistence, missing-workspace, and rollback tests**

Use a `NamedTempFile` for persistence. For rollback, create a persistent
SQLite trigger from a second `rusqlite::Connection` that aborts root insertion;
after `reset_workspace` fails, assert the original file still exists:

```rust
#[tokio::test]
async fn failed_reset_rolls_back_the_original_workspace() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let fs = SqliteFileSystem::open(file.path(), Default::default()).await.unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    let path = VirtualPath::parse("/keep.txt").unwrap();
    fs.write(
        &ctx,
        &path,
        fslite_core::WriteSource::from_bytes(b"keep".to_vec()),
        Default::default(),
    )
    .await
    .unwrap();

    let raw = rusqlite::Connection::open(file.path()).unwrap();
    raw.execute_batch(
        "CREATE TRIGGER fail_reset_root BEFORE INSERT ON nodes
         WHEN NEW.parent_id IS NULL
         BEGIN SELECT RAISE(ABORT, 'forced reset failure'); END;",
    )
    .unwrap();

    assert!(fs.reset_workspace(workspace.id).await.is_err());
    assert!(fs.exists(&ctx, &path, Default::default()).await.unwrap());
}
```

Add `reset_workspace_survives_reopen` and
`reset_missing_workspace_returns_not_found` with explicit assertions on
`ErrorCode::NotFound`.

- [ ] **Step 3: Run the focused tests and verify they fail**

Run:

```bash
cargo test -p fslite-sqlite --test workspaces reset_workspace -- --nocapture
```

Expected: compilation fails because `SqliteFileSystem::reset_workspace` does
not exist.

- [ ] **Step 4: Implement the transactional reset**

Add `workspace::reset_workspace` using one transaction. Delete nodes before
content generations because nodes reference generations; node deletion
cascades attributes and trash. Recreate the root and reset `change_seq`:

```rust
pub(crate) async fn reset_workspace(
    conn: &Connection,
    workspace_id: WorkspaceId,
) -> FsResult<()> {
    let workspace_id_str = workspace_id.to_string();
    conn.call(move |conn| {
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
            params![workspace_id_str],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(false);
        }

        tx.execute("DELETE FROM nodes WHERE workspace_id = ?1", params![workspace_id_str])?;
        tx.execute(
            "DELETE FROM content_generations WHERE workspace_id = ?1",
            params![workspace_id_str],
        )?;
        tx.execute("DELETE FROM changes WHERE workspace_id = ?1", params![workspace_id_str])?;

        let root_id = NodeId::new();
        let now = now_ms();
        tx.execute(
            "INSERT INTO nodes(id, workspace_id, parent_id, name, kind, size, revision,
             created_at_ms, modified_at_ms, accessed_at_ms)
             VALUES (?1, ?2, NULL, '', 0, 0, 1, ?3, ?3, ?3)",
            params![root_id.to_string(), workspace_id_str, now],
        )?;
        tx.execute(
            "UPDATE workspaces SET change_seq = 0, updated_at_ms = ?2 WHERE id = ?1",
            params![workspace_id_str, now],
        )?;
        tx.commit()?;
        Ok(true)
    })
    .await
    .map_err(db::map_call_error)?
    .then_some(())
    .ok_or_else(|| FsError::not_found(workspace_id))
}
```

Expose it without adding it to the transport-independent `FileSystem` trait:

```rust
/// Atomically returns a workspace to its empty initial state.
pub async fn reset_workspace(&self, workspace_id: WorkspaceId) -> FsResult<()> {
    workspace::reset_workspace(&self.conn, workspace_id).await
}
```

- [ ] **Step 5: Run SQLite reset and complete SQLite tests**

Run:

```bash
cargo test -p fslite-sqlite --test workspaces
cargo test -p fslite-sqlite
```

Expected: all tests pass, including rollback and reopen coverage.

- [ ] **Step 6: Commit the SQLite reset primitive**

```bash
git add -- crates/fslite-sqlite/src/workspace.rs crates/fslite-sqlite/src/lib.rs crates/fslite-sqlite/tests/workspaces.rs
git commit -m "feat(sqlite): reset workspaces atomically"
```

### Task 2: Authenticated Identity Endpoint

**Files:**
- Create: `crates/fslite-server/src/routes/identity.rs`
- Modify: `crates/fslite-server/src/routes/mod.rs`
- Modify: `crates/fslite-server/src/lib.rs`
- Create: `crates/fslite-server/tests/identity.rs`

**Interfaces:**
- Consumes: `AuthProvider::authenticate(&HeaderMap) -> Result<AuthenticatedActor, ApiError>`.
- Produces: `GET /v1/me` with `{ workspace_id, capabilities }`, consumed by the Astro runtime plan.

- [ ] **Step 1: Write identity contract tests**

Create tests for a valid token, no token, and safe output. The valid response
must contain the fixture workspace and every fixture capability, and must not
contain the token or actor metadata:

```rust
#[tokio::test]
async fn me_returns_safe_authenticated_identity() {
    let (state, workspace_id) = support::fixture().await;
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/v1/me")
                .header("authorization", format!("Bearer {}", support::TOKEN))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["workspace_id"], workspace_id.to_string());
    assert!(value["capabilities"].as_array().unwrap().len() >= 5);
    assert!(value.get("actor_metadata").is_none());
    assert!(!String::from_utf8_lossy(&bytes).contains(support::TOKEN));
}
```

- [ ] **Step 2: Run the identity tests and verify the route is missing**

Run: `cargo test -p fslite-server --test identity`

Expected: FAIL because `/v1/me` returns `404 route_not_found`.

- [ ] **Step 3: Implement and register the identity router**

Create a direct-auth route because it has no workspace path:

```rust
#[derive(serde::Serialize)]
struct IdentityDto {
    workspace_id: WorkspaceId,
    capabilities: BTreeSet<Capability>,
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/v1/me",
        get(me).fallback(crate::routes::method_not_allowed),
    )
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<IdentityDto>, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    Ok(Json(IdentityDto {
        workspace_id: actor.workspace_id,
        capabilities: actor.capabilities,
    }))
}
```

Declare `mod identity;` and merge `routes::identity::router()` before the
fallback in `app`.

- [ ] **Step 4: Verify the identity contract and complete server suite**

Run:

```bash
cargo test -p fslite-server --test identity
cargo test -p fslite-server
```

Expected: all tests pass.

- [ ] **Step 5: Commit the endpoint**

```bash
git add -- crates/fslite-server/src/routes/identity.rs crates/fslite-server/src/routes/mod.rs crates/fslite-server/src/lib.rs crates/fslite-server/tests/identity.rs
git commit -m "feat(server): expose authenticated identity"
```

### Task 3: Protected HTTP Workspace Reset

**Files:**
- Modify: `crates/fslite-server/src/admin.rs`
- Modify: `crates/fslite-server/src/routes/workspaces.rs`
- Modify: `crates/fslite-server/tests/workspaces.rs`
- Modify: `crates/fslite-server/tests/support/mod.rs`

**Interfaces:**
- Consumes: `SqliteFileSystem::reset_workspace(WorkspaceId) -> FsResult<()>` from Task 1.
- Produces: `WorkspaceAdmin::reset_workspace`, and `POST /v1/workspaces/{workspace_id}/reset -> WorkspaceUsage` for the Astro reset coordinator.

- [ ] **Step 1: Write route authorization and behavior tests**

Add tests that seed a file and trash entry through the real router, call reset,
then verify usage is one root node, the file is gone, and trash is empty. Add
separate assertions for missing token (`401`), mismatched workspace (`403`),
and a same-workspace token without `WorkspaceAdmin` (`403`). The success call
is:

```rust
let response = router
    .clone()
    .oneshot(
        auth(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/workspaces/{workspace_id}/reset")),
        )
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(response.status(), StatusCode::OK);
let body = response.into_body().collect().await.unwrap().to_bytes();
let usage: WorkspaceUsage = serde_json::from_slice(&body).unwrap();
assert_eq!(usage.workspace_id, workspace_id);
assert_eq!(usage.active_nodes, 1);
```

- [ ] **Step 2: Run the route tests and verify they fail**

Run: `cargo test -p fslite-server --test workspaces reset -- --nocapture`

Expected: FAIL because the route returns `404` or the trait method is absent.

- [ ] **Step 3: Extend the administrative boundary**

Add the method to the trait and SQLite adapter:

```rust
#[async_trait]
pub trait WorkspaceAdmin: Send + Sync {
    async fn create_workspace(&self) -> FsResult<Workspace>;
    async fn delete_workspace(&self, id: WorkspaceId) -> FsResult<()>;
    async fn reset_workspace(&self, id: WorkspaceId) -> FsResult<()>;
}

async fn reset_workspace(&self, id: WorkspaceId) -> FsResult<()> {
    self.0.reset_workspace(id).await
}
```

- [ ] **Step 4: Implement the route with direct authentication**

Register `POST /v1/workspaces/{workspace_id}/reset`. Authenticate directly,
enforce matching workspace and `WorkspaceAdmin`, construct a request context
from the actor, reset, then return current usage:

```rust
async fn reset_workspace(
    State(state): State<AppState>,
    Path(workspace_id): Path<WorkspaceId>,
    headers: HeaderMap,
) -> Result<Json<WorkspaceUsage>, ApiError> {
    let actor = state.auth.authenticate(&headers).await?;
    if actor.workspace_id != workspace_id {
        return Err(ApiError::WorkspaceMismatch);
    }
    if !actor.capabilities.contains(&Capability::WorkspaceAdmin) {
        return Err(ApiError::Domain(FsError::permission_denied("reset_workspace")));
    }
    let ctx = RequestContext::new(
        workspace_id,
        actor.actor_metadata,
        actor.capabilities,
    );
    state.admin.reset_workspace(workspace_id).await?;
    Ok(Json(state.fs.workspace_usage(&ctx).await?))
}
```

- [ ] **Step 5: Verify reset routes and all server tests**

Run:

```bash
cargo test -p fslite-server --test workspaces
cargo test -p fslite-server
```

Expected: all tests pass.

- [ ] **Step 6: Commit the HTTP reset boundary**

```bash
git add -- crates/fslite-server/src/admin.rs crates/fslite-server/src/routes/workspaces.rs crates/fslite-server/tests/workspaces.rs crates/fslite-server/tests/support/mod.rs
git commit -m "feat(server): reset authorized workspaces"
```

### Task 4: Server Configuration and Credential Store

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/fslite-server/Cargo.toml`
- Create: `crates/fslite-server/src/server_config.rs`
- Create: `crates/fslite-server/src/credential_store.rs`
- Modify: `crates/fslite-server/src/main.rs`

**Interfaces:**
- Produces: `CliArgs`, `ServerPaths`, `StoredServerState`, `WorkspaceLimits`, and `ResolvedServerConfig` for Task 5.
- Consumes: platform directories, JSON state, environment variables, and token files.

- [ ] **Step 1: Add dependencies and declare binary-only modules**

Add `atomic-write-file`, `clap`, and `directories` to server dependencies,
`tempfile` to dev-dependencies, and add UUID `v4` to workspace features:

```toml
uuid = { version = "1", features = ["serde", "v4", "v7"] }
```

At the top of `main.rs`, declare:

```rust
mod credential_store;
mod server_bootstrap;
mod server_config;
```

- [ ] **Step 2: Write configuration precedence and validation tests**

In `server_config.rs`, test that explicit CLI values beat environment-derived
`CliArgs`, those values beat persisted state, empty token files fail, invalid
socket addresses fail in Clap, and the defaults are `127.0.0.1:8080`,
`fslite.db`, and `server.json` under the resolved directories. Construct args
directly rather than mutating process environment:

```rust
#[test]
fn cli_values_override_stored_values() {
    let dir = tempfile::tempdir().unwrap();
    let args = CliArgs {
        db: Some(dir.path().join("explicit.db")),
        bind: Some("127.0.0.1:9000".parse().unwrap()),
        config: Some(dir.path().join("explicit.json")),
        token_file: None,
        max_bytes: Some(10),
        max_nodes: Some(20),
        max_file_bytes: Some(5),
    };
    let resolved = ResolvedServerConfig::resolve(args, None).unwrap();
    assert_eq!(resolved.database_path, dir.path().join("explicit.db"));
    assert_eq!(resolved.bind.to_string(), "127.0.0.1:9000");
    assert_eq!(resolved.workspace_limits.max_nodes, 20);
}
```

- [ ] **Step 3: Define Clap and persisted-state types**

Use optional Clap fields so persisted values can participate after Clap has
already applied CLI-over-environment precedence:

```rust
#[derive(clap::Parser, Debug)]
pub(crate) struct CliArgs {
    #[arg(long, env = "FSLITE_DB")]
    pub db: Option<PathBuf>,
    #[arg(long, env = "FSLITE_BIND")]
    pub bind: Option<SocketAddr>,
    #[arg(long, env = "FSLITE_CONFIG")]
    pub config: Option<PathBuf>,
    #[arg(long, env = "FSLITE_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,
    #[arg(long, env = "FSLITE_MAX_BYTES")]
    pub max_bytes: Option<u64>,
    #[arg(long, env = "FSLITE_MAX_NODES")]
    pub max_nodes: Option<u64>,
    #[arg(long, env = "FSLITE_MAX_FILE_BYTES")]
    pub max_file_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(crate) struct WorkspaceLimits {
    pub max_bytes: u64,
    pub max_nodes: u64,
    pub max_file_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StoredServerState {
    pub database_path: PathBuf,
    pub bind: SocketAddr,
    pub workspace_id: WorkspaceId,
    pub token: String,
    pub workspace_limits: WorkspaceLimits,
}
```

`ServerPaths::platform_default()` uses
`directories::ProjectDirs::from("", "", "fslite")`, with database under
`data_local_dir()` and `server.json` under `config_dir()`.

- [ ] **Step 4: Write credential persistence tests**

Test pretty JSON round-trip, atomic replacement of invalid old contents, and
Unix mode `0600`:

```rust
#[cfg(unix)]
#[test]
fn saved_state_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.json");
    save_state(&path, &fixture_state()).unwrap();
    assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
}
```

- [ ] **Step 5: Implement atomic owner-only state writes and token loading**

Use `AtomicWriteFile`, set permissions before commit on Unix, `sync_all`, and
reject empty trimmed tokens. Generate fallback tokens from two UUIDv4 values:

```rust
pub(crate) fn generate_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub(crate) fn read_token_file(path: &Path) -> Result<String, ConfigError> {
    let token = std::fs::read_to_string(path)?.trim().to_owned();
    if token.is_empty() {
        return Err(ConfigError::EmptyTokenFile(path.to_path_buf()));
    }
    Ok(token)
}
```

Resolve a process token in this order: `FSLITE_TOKEN` captured by a dedicated
`token_from_env()` helper, resolved token file, persisted token, generated
token. Never include the token in `Debug` output or error messages.

- [ ] **Step 6: Run module tests and Clippy**

Run:

```bash
cargo test -p fslite-server --bin fslite-server server_config
cargo test -p fslite-server --bin fslite-server credential_store
cargo clippy -p fslite-server --all-targets -- -D warnings
```

Expected: all focused tests and Clippy pass.

- [ ] **Step 7: Commit configuration and storage**

```bash
git add -- Cargo.toml Cargo.lock crates/fslite-server/Cargo.toml crates/fslite-server/src/main.rs crates/fslite-server/src/server_config.rs crates/fslite-server/src/credential_store.rs
git commit -m "feat(server): add secure persistent configuration"
```

### Task 5: Persistent Bootstrap and Usable Binary Wiring

**Files:**
- Create: `crates/fslite-server/src/server_bootstrap.rs`
- Modify: `crates/fslite-server/src/main.rs`
- Create: `crates/fslite-server/tests/binary_bootstrap.rs`

**Interfaces:**
- Consumes: configuration/store types from Task 4, `SqliteFileSystem`, `BearerTokenAuthProvider`, and `SqliteWorkspaceAdmin`.
- Produces: a running persistent server, `BootstrapResult`, exact first-run output, and connection guidance.

- [ ] **Step 1: Write bootstrap module tests**

Test these cases with explicit temp database/config paths:

1. no files creates one workspace, state, and generated token;
2. restart reuses database, workspace, and token;
3. supplied token overrides the stored token for the process without writing
   it back;
4. existing database plus absent config creates a new default workspace but
   retains unrelated workspaces;
5. state pointing at a deleted workspace creates a replacement;
6. configured limits apply only when creating the workspace.

The first test asserts:

```rust
let result = bootstrap(config).await.unwrap();
assert!(result.created_database_or_workspace);
assert!(result.generated_token);
assert!(result.database_path.exists());
assert_eq!(
    result.bootstrap_message(),
    Some("No database or workspace found, creating default database and workspace")
);
```

- [ ] **Step 2: Run bootstrap tests and verify the module is absent**

Run: `cargo test -p fslite-server --bin fslite-server server_bootstrap`

Expected: compilation fails because `server_bootstrap.rs` does not exist.

- [ ] **Step 3: Implement bootstrap state resolution**

Define:

```rust
pub(crate) struct BootstrapResult {
    pub sqlite: Arc<SqliteFileSystem>,
    pub workspace_id: WorkspaceId,
    pub token: String,
    pub bind: SocketAddr,
    pub database_path: PathBuf,
    pub config_path: PathBuf,
    pub created_database_or_workspace: bool,
    pub generated_token: bool,
}
```

Before opening SQLite, record `database_path.exists()`. Open the database,
then validate a stored workspace by calling `workspace_usage` with
`RequestContext::trusted`. On `NotFound`, create with the resolved limits.
Build and atomically persist `StoredServerState` only when state is new or its
workspace/path/settings changed. Do not persist an environment/token-file
override over the stored generated token.

- [ ] **Step 4: Wire the binary to persistent state**

Replace in-memory reference wiring in `main.rs`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = CliArgs::parse();
    let config = ResolvedServerConfig::load(args)?;
    let boot = server_bootstrap::bootstrap(config).await?;
    if boot.created_database_or_workspace {
        println!("No database or workspace found, creating default database and workspace");
    }
    boot.print_connection_guidance();

    let state = boot.app_state();
    let listener = tokio::net::TcpListener::bind(boot.bind).await?;
    println!("fslite-server listening on http://{}", listener.local_addr()?);
    axum::serve(listener, fslite_server::app(state)).await?;
    Ok(())
}
```

`app_state()` grants the default credential all five existing capabilities.
Connection guidance prints an exact `FSLITE_TOKEN=... fslite --server ...`
command only when a token is newly generated; later starts print the config
path and a command using `$FSLITE_TOKEN`.

- [ ] **Step 5: Write installed-binary smoke tests**

Launch `env!("CARGO_BIN_EXE_fslite-server")` with temp `--db`, `--config`, and
`--bind 127.0.0.1:0`. Read stdout until the listening line, then terminate the
child. Assert the exact bootstrap message occurs once. Open the database
directly, write a file into the persisted workspace, restart the child, call
`GET /v1/me` using the stored token, and verify the same workspace ID and file
remain. Ensure child cleanup occurs through a guard even on test failure.

- [ ] **Step 6: Run binary, package, and full server tests**

Run:

```bash
cargo test -p fslite-server --test binary_bootstrap -- --nocapture
cargo test -p fslite-server
cargo run -p fslite-server -- --help
```

Expected: tests pass; help documents `--db`, `--bind`, `--config`, token-file,
and quota controls, with no `--token` flag.

- [ ] **Step 7: Commit the persistent binary**

```bash
git add -- crates/fslite-server/src/main.rs crates/fslite-server/src/server_bootstrap.rs crates/fslite-server/tests/binary_bootstrap.rs
git commit -m "feat(server): bootstrap persistent default workspace"
```

### Task 6: Server Container Image

**Files:**
- Create: `.dockerignore`
- Create: `crates/fslite-server/Dockerfile`
- Create: `crates/fslite-server/docker-entrypoint.sh`
- Create: `crates/fslite-server/tests/container_smoke.sh`

**Interfaces:**
- Consumes: the persistent binary and its environment contract from Task 5.
- Produces: a non-root image exposing port `8080`, storing state under `/data`, and suitable for the showcase Compose service.

- [ ] **Step 1: Write a container smoke script before the image**

Create `tests/container_smoke.sh` with `set -eu`. It accepts the image name as
`${1:-fslite-server:local}`, uses the exact container
`fslite-server-smoke` and volume `fslite-server-smoke-data`, creates a token in
a `mktemp -d` directory, and installs a trap that removes only those names and
that exact temporary directory. The script:

```sh
docker volume create fslite-server-smoke-data >/dev/null
docker run -d --name fslite-server-smoke \
  -p 127.0.0.1:18080:8080 \
  -v fslite-server-smoke-data:/data \
  -v "$smoke_dir/token:/run/secrets/fslite_token:ro" \
  -e FSLITE_TOKEN_FILE=/run/secrets/fslite_token \
  "$image" >/dev/null
```

It polls `/readyz`, parses `workspace_id` from `/v1/me`, writes `persist.txt`
through `PUT /v1/workspaces/$workspace_id/content/persist.txt`, removes and
recreates the exact container with the same volume, then asserts GET returns
`persistent`. It fails if port 18080 is already occupied rather than removing
an unrelated container.

- [ ] **Step 2: Add the bounded Docker build context**

Create `.dockerignore`:

```text
.git
.worktrees
.superpowers
target
showcase/node_modules
showcase/dist
*.db
*.db-shm
*.db-wal
```

- [ ] **Step 3: Add multi-stage non-root image and entrypoint**

Use `rust:1.85-bookworm` to build only `fslite-server`, then
`debian:bookworm-slim` with `ca-certificates` and `curl`. Create UID/GID 10001,
own `/data`, copy the binary and entrypoint, and configure:

```dockerfile
ENV FSLITE_DB=/data/fslite.db \
    FSLITE_CONFIG=/data/server.json \
    FSLITE_BIND=0.0.0.0:8080
EXPOSE 8080
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/usr/local/bin/fslite-server"]
HEALTHCHECK --interval=5s --timeout=3s --start-period=10s --retries=12 \
  CMD curl --fail --silent http://127.0.0.1:8080/readyz || exit 1
```

The entrypoint checks that the parent directories of `FSLITE_DB` and
`FSLITE_CONFIG` are writable, reports paths without token contents, and uses
`exec "$@"`.

- [ ] **Step 4: Build and execute persistence smoke checks**

Run:

```bash
docker build -f crates/fslite-server/Dockerfile -t fslite-server:local .
docker image inspect fslite-server:local --format '{{.Config.User}}'
```

Expected: build succeeds and configured user is `10001:10001`. Run the smoke
flow from Step 1 and expect `/v1/me`, write, restart, and read to succeed.

- [ ] **Step 5: Commit the server container**

```bash
git add -- .dockerignore crates/fslite-server/Dockerfile crates/fslite-server/docker-entrypoint.sh crates/fslite-server/tests/container_smoke.sh
git commit -m "build(server): add persistent non-root image"
```

### Task 7: Documentation, Packaging, and Release Preparation

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Modify: `RELEASE.md`
- Modify: `crates/fslite-sqlite/RELEASE-NOTES.md`
- Modify: `crates/fslite-server/RELEASE-NOTES.md`
- Modify: `crates/fslite-cli/RELEASE-NOTES.md`
- Modify: `crates/fslite-sqlite/Cargo.toml`
- Modify: `crates/fslite-server/Cargo.toml`
- Modify: `crates/fslite-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Move: `examples/server_and_remote_cli.rs` to `crates/fslite-server/examples/server_and_remote_cli.rs`
- Modify: README links that reference the moved example

**Interfaces:**
- Consumes: all completed Rust behavior and image contracts.
- Produces: accurate quick starts and package metadata for `fslite-sqlite 0.2.0`, `fslite-server 0.2.0`, and the next compatible `fslite` release.

- [ ] **Step 1: Add README server quick starts**

Replace the reference-wiring warning with:

```bash
cargo install fslite-server
fslite-server

# Docker/private-network deployment
FSLITE_TOKEN_FILE=/run/secrets/fslite_token \
FSLITE_DB=/data/fslite.db \
FSLITE_BIND=0.0.0.0:8080 \
fslite-server
```

Document first-run output, config/database locations, `GET /v1/me`, reset
authorization, quota flags, token-file guidance, and that public browsers must
use a server-side gateway rather than receive the bearer token.

- [ ] **Step 2: Correct server package metadata and example packaging**

Set `documentation = "https://docs.rs/fslite-server"` in the server package.
Move `examples/server_and_remote_cli.rs` into
`crates/fslite-server/examples/server_and_remote_cli.rs`, remove the explicit
external-path `[[example]]` target, and update README links. Cargo then
auto-discovers and packages the example. Require a dry run with no
missing-example warning.

- [ ] **Step 3: Apply semver-consistent versions**

Set `fslite-sqlite` and `fslite-server` package versions to `0.2.0`, require
`fslite-sqlite = "0.2.0"` from the server, and update the local CLI's SQLite
path dependency to accept the additive release. Because the next CLI package
cannot retain its already-published `0.1.1` manifest with a changed dependency,
prepare `fslite` as `0.2.0` as well. Leave unchanged core, conformance, and
command package versions intact. Update `RELEASE.md` from the obsolete
single-version statement to the actual per-crate dependency order.

- [ ] **Step 4: Update changelog and crate release notes**

Record persistent server bootstrap, identity, reset, Docker image, security
behavior, SQLite reset semantics, and the `0.2.0` policy rationale. State the
publish order explicitly:

1. `fslite-sqlite 0.2.0`;
2. wait for crates.io indexing;
3. `fslite-server 0.2.0`;
4. `fslite 0.2.0` if its manifest is released with the new SQLite dependency.

- [ ] **Step 5: Run package and quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo publish --dry-run -p fslite-sqlite
cargo package -p fslite-server --allow-dirty --no-verify
cargo package -p fslite --allow-dirty --no-verify
git diff --check
```

Expected: all workspace checks pass; the SQLite dry run succeeds; package file
lists contain their examples/release notes and no secrets or local databases.
The server publish dry run remains a post-index release check and is not
misreported as complete before `fslite-sqlite 0.2.0` exists on crates.io.

- [ ] **Step 6: Commit release-ready documentation and manifests**

```bash
git add -- README.md CHANGELOG.md RELEASE.md Cargo.toml Cargo.lock crates/fslite-sqlite/Cargo.toml crates/fslite-sqlite/RELEASE-NOTES.md crates/fslite-server/Cargo.toml crates/fslite-server/RELEASE-NOTES.md examples/server_and_remote_cli.rs crates/fslite-server/examples/server_and_remote_cli.rs crates/fslite-cli/Cargo.toml crates/fslite-cli/RELEASE-NOTES.md
git commit -m "release: prepare persistent fslite server"
```

### Task 8: Rust Plan Final Verification

**Files:**
- Verify only; modify failures at their owning task's file and commit separately.

**Interfaces:**
- Produces: the stable server contract required by the Astro showcase plan.

- [ ] **Step 1: Run the complete Rust quality matrix from a clean state**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git diff --check
git status --short
```

Expected: formatting, Clippy, and all tests pass; status contains no unintended
files.

- [ ] **Step 2: Run installed-style local smoke**

Install to a temporary Cargo root, start with temporary data/config paths, and
exercise identity and persistence:

```bash
cargo install --path crates/fslite-server --root /tmp/fslite-server-install --force
```

Run the installed binary with `--bind 127.0.0.1:0`, capture the printed port,
call `/healthz`, `/readyz`, and `/v1/me`, write through the HTTP API, restart,
and read the same bytes. Use a task-specific `mktemp -d` directory and remove
only that exact directory after the smoke completes.

- [ ] **Step 3: Re-run container persistence smoke**

Build `fslite-server:local`, confirm user `10001:10001`, run the named-volume
first-run/restart flow, and verify the token never appears in `docker inspect`
command arguments.

- [ ] **Step 4: Record verification evidence**

Add no code if everything passes. Capture command names, pass counts, package
warnings, and any environment-only skipped check in the implementation handoff.
Do not claim crates are published or images are deployed.
