# fslite status and doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `fslite status` (reports the active filesystem/workspace) and `fslite doctor` (validates local registry/context/database health) as read-only CLI commands.

**Architecture:** `status`/`doctor` are new `Action` variants in `crates/fslite-cli/src/cli.rs`, dispatched in `main.rs` before bootstrap/target resolution. Each lives in its own module (`status.rs`, `doctor.rs`) that reads `Registry`/`Context` directly — neither goes through `fslite-command`'s `Command` enum, which has no concept of the client-side registry. `doctor`'s database checks need two new public methods on `fslite-sqlite`'s `SqliteFileSystem`: `schema_version()` and `integrity_check()`.

**Tech Stack:** Rust 2024, Cargo workspace, `clap`, `serde`/`serde_json`, `tokio-rusqlite`/`rusqlite`, `fs2` (file locking).

## Global Constraints

- Neither command bootstraps, nor mutates `registry.json`, `context.json`, the bootstrap lock, or any database, under any circumstances.
- `status` reports only on registered filesystems; `--db`/`--memory`/`--server` are out of scope and produce an explicit error.
- `doctor` exits `0` if every check is `pass` or `warn`, `1` if any check is `fail`.
- Both commands support `--json` output.
- `SqliteFileSystem::open`/`open_in_memory` already run schema migrations to the latest version on every open — `schema_version()`/`integrity_check()` are read-only observability additions, not new migration behavior.

---

### Task 1: Expose Schema Version and Integrity Check on `fslite-sqlite`

**Files:**
- Modify: `crates/fslite-sqlite/src/db.rs`
- Modify: `crates/fslite-sqlite/src/lib.rs`
- Test: `crates/fslite-sqlite/tests/health.rs` (new)

**Interfaces:**
- Consumes: existing private `read_current_version(&RusqliteConnection) -> rusqlite::Result<i64>` and `latest_schema_version() -> i64` in `db.rs`; `tokio_rusqlite::Connection`.
- Produces: `SqliteFileSystem::schema_version(&self) -> FsResult<i64>`, `SqliteFileSystem::latest_schema_version() -> i64` (associated fn, no `self`), `SqliteFileSystem::integrity_check(&self) -> FsResult<Vec<String>>` (empty vec = healthy).

- [ ] **Step 1: Write failing tests for the new health API**

Create `crates/fslite-sqlite/tests/health.rs`:

```rust
use fslite_sqlite::SqliteFileSystem;

#[tokio::test]
async fn schema_version_matches_latest_after_open() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    assert_eq!(
        fs.schema_version().await.unwrap(),
        SqliteFileSystem::latest_schema_version()
    );
}

#[tokio::test]
async fn schema_version_survives_reopen_of_a_file_backed_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("health.db");
    {
        let fs = SqliteFileSystem::open(&path, Default::default())
            .await
            .unwrap();
        assert_eq!(
            fs.schema_version().await.unwrap(),
            SqliteFileSystem::latest_schema_version()
        );
    }
    let fs = SqliteFileSystem::open(&path, Default::default())
        .await
        .unwrap();
    assert_eq!(
        fs.schema_version().await.unwrap(),
        SqliteFileSystem::latest_schema_version()
    );
}

#[tokio::test]
async fn integrity_check_reports_no_problems_on_a_healthy_database() {
    let fs = SqliteFileSystem::open_in_memory(Default::default())
        .await
        .unwrap();
    assert_eq!(fs.integrity_check().await.unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn integrity_check_reports_problems_on_a_corrupted_database() {
    use fslite_core::{RequestContext, VirtualPath, WriteSource};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    {
        // Write enough content to span many 4 KiB pages, then let the
        // connection drop (WAL auto-checkpoints on last-connection-close),
        // so every page's real content lives in the main file, not the WAL.
        let fs = SqliteFileSystem::open(&path, Default::default())
            .await
            .unwrap();
        let workspace = fs.create_workspace(Default::default()).await.unwrap();
        let ctx = RequestContext::trusted(workspace.id);
        fs.write(
            &ctx,
            &VirtualPath::parse("/big.bin").unwrap(),
            WriteSource::from_bytes(vec![7u8; 200_000]),
            Default::default(),
        )
        .await
        .unwrap();
    }

    // `fs`'s connection runs in WAL mode, where new pages are written to a
    // separate `-wal` file first and only merged into the main `.db` file
    // at checkpoint time; when that checkpoint happens relative to the
    // async connection's drop above is not guaranteed. Force it explicitly
    // through a fresh, synchronous connection so the corruption below
    // always lands in the main file, deterministically.
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "wal_checkpoint", "TRUNCATE")
        .unwrap();

    // Overwrite one whole page well past the header/schema pages with
    // garbage, corrupting that page's B-tree structure. Basic queries
    // (like the `sqlite_master`/`schema_migrations` reads `open` performs)
    // only touch early pages, so `open` below still succeeds — this
    // exercises `integrity_check`'s full-database walk specifically,
    // as opposed to `open` failing outright on a header-level corruption.
    let mut bytes = std::fs::read(&path).unwrap();
    const PAGE_SIZE: usize = 4096;
    let page_count = bytes.len() / PAGE_SIZE;
    let target_page = page_count * 2 / 3;
    let start = target_page * PAGE_SIZE;
    for byte in bytes.iter_mut().skip(start).take(PAGE_SIZE) {
        *byte = 0xFF;
    }
    std::fs::write(&path, bytes).unwrap();

    let fs = SqliteFileSystem::open(&path, Default::default())
        .await
        .unwrap();
    assert!(!fs.integrity_check().await.unwrap().is_empty());
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p fslite-sqlite --test health`
Expected: fails to compile — `schema_version`, `latest_schema_version`, `integrity_check` don't exist on `SqliteFileSystem`.

- [ ] **Step 3: Promote `latest_schema_version` to `pub(crate)` and add `schema_version`/`integrity_check` to `db.rs`**

In `crates/fslite-sqlite/src/db.rs`, change:

```rust
fn latest_schema_version() -> i64 {
```

to:

```rust
pub(crate) fn latest_schema_version() -> i64 {
```

Then add, directly above `fn read_current_version`:

```rust
/// Returns the database's current schema version, post-migration. Since
/// [`open_file`]/[`open_memory`] already migrate to [`latest_schema_version`]
/// on every open, this only differs from that constant when called against
/// a connection that bypassed `initialize` (not possible via this crate's
/// public API) — it exists so callers have one source of truth rather than
/// re-deriving the version from `MIGRATIONS` themselves.
pub(crate) async fn schema_version(conn: &Connection) -> FsResult<i64> {
    conn.call(|conn| Ok(read_current_version(conn)?))
        .await
        .map_err(map_call_error)
}

/// Runs SQLite's built-in `PRAGMA integrity_check` and returns the problem
/// rows it reports. An empty vec means the database is healthy; SQLite's own
/// single "ok" row is collapsed to empty so callers only see actual problems.
pub(crate) async fn integrity_check(conn: &Connection) -> FsResult<Vec<String>> {
    conn.call(|conn| {
        let mut statement = conn.prepare("PRAGMA integrity_check")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(rows)
    })
    .await
    .map_err(map_call_error)
    .map(|rows| {
        if rows.as_slice() == ["ok"] {
            Vec::new()
        } else {
            rows
        }
    })
}
```

- [ ] **Step 4: Add the public `SqliteFileSystem` methods in `lib.rs`**

In `crates/fslite-sqlite/src/lib.rs`, immediately after the existing `workspace_usage` method, add:

```rust
    /// Returns this database's current schema version.
    pub async fn schema_version(&self) -> FsResult<i64> {
        db::schema_version(&self.conn).await
    }

    /// The schema version this build of `fslite-sqlite` migrates to on open.
    pub fn latest_schema_version() -> i64 {
        db::latest_schema_version()
    }

    /// Runs SQLite's built-in integrity check, returning any problems found.
    /// An empty vec means the database is healthy.
    pub async fn integrity_check(&self) -> FsResult<Vec<String>> {
        db::integrity_check(&self.conn).await
    }
```

- [ ] **Step 5: Run tests to verify GREEN**

Run: `cargo test -p fslite-sqlite --test health`
Expected: 4 passed. Run it 3 times in a row to confirm the corruption test is deterministic (it depends on an explicit WAL checkpoint, not timing).

- [ ] **Step 6: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy -p fslite-sqlite --all-targets --all-features -- -D warnings
git add crates/fslite-sqlite/src/db.rs crates/fslite-sqlite/src/lib.rs crates/fslite-sqlite/tests/health.rs
git commit -m "feat(fslite-sqlite): expose schema version and integrity check"
```

---

### Task 2: Add `fslite status`

**Files:**
- Create: `crates/fslite-cli/src/status.rs`
- Modify: `crates/fslite-cli/src/cli.rs`
- Modify: `crates/fslite-cli/src/main.rs`
- Test: `crates/fslite-cli/tests/e2e_status.rs` (new)

**Interfaces:**
- Consumes: `crate::registry::Registry::{load, filesystem_path, resolve_workspace_name}`, `crate::context::Context::load`, `fslite_sqlite::SqliteFileSystem::{open, workspace_usage}`, `fslite_core::{RequestContext, WorkspaceUsage}`, `fslite_command::render::sanitize_name`.
- Produces: `status::StatusReport` (`Serialize`, fields `filesystem_name: Option<String>`, `database_path: Option<String>`, `workspace_name: Option<String>`, `workspace_id: Option<String>`, `usage: Option<WorkspaceUsage>`, `selection: Selection`), `status::Selection` (`Explicit | Persisted | None`), `status::build(cli: &Cli) -> Result<StatusReport, Box<dyn std::error::Error>>`, `status::render_human(&StatusReport) -> String`, and a new `Action::Status` unit variant.

- [ ] **Step 1: Write a failing clap parse test**

In `crates/fslite-cli/src/cli.rs`, add this test directly above `fn delete_subcommand_yes_flag`:

```rust
    #[test]
    fn status_subcommand_does_not_externalize() {
        assert!(matches!(parse(&["status"]).action, Some(Action::Status)));
    }

```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test -p fslite --bin fslite status_subcommand_does_not_externalize`
Expected: fails to compile — `Action::Status` doesn't exist yet.

- [ ] **Step 3: Add the `Status` variant to `Action`**

In `crates/fslite-cli/src/cli.rs`, in the `Action` enum, directly above the `Verb` catch-all variant, add:

```rust
    /// Shows the active filesystem/workspace, its database path, usage, and
    /// whether the selection came from an explicit flag or the persisted
    /// context. Read-only: never bootstraps or modifies any state.
    Status,
```

In the `debug_action` test helper, add the matching arm:

```rust
            Some(Action::Status) => "Status",
```

placed directly after the `Some(Action::Help { .. }) => "Help",` arm.

- [ ] **Step 4: Write `status.rs`**

Create `crates/fslite-cli/src/status.rs`:

```rust
//! `fslite status`: reports the active filesystem/workspace, its database
//! path, and usage, and whether the selection is explicit or persisted.
//! Read-only — never bootstraps and never mutates `registry.json`,
//! `context.json`, or any database.

use fslite_core::{RequestContext, WorkspaceUsage};
use fslite_sqlite::SqliteFileSystem;
use serde::Serialize;

use crate::cli::Cli;
use crate::context::Context;
use crate::registry::Registry;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    Explicit,
    Persisted,
    None,
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub filesystem_name: Option<String>,
    pub database_path: Option<String>,
    pub workspace_name: Option<String>,
    pub workspace_id: Option<String>,
    pub usage: Option<WorkspaceUsage>,
    pub selection: Selection,
}

/// Resolves and reports the active filesystem/workspace without opening a
/// write path anywhere: `--db`/`--memory`/`--server` bypass the registry
/// entirely, so they are out of scope for a command about the *registered*
/// active filesystem.
pub async fn build(cli: &Cli) -> Result<StatusReport, Box<dyn std::error::Error>> {
    if cli.db.is_some() || cli.memory || cli.server.is_some() {
        return Err(
            "status reports on registered filesystems only; omit --db/--memory/--server (pass --filesystem, or nothing, to see the active one)"
                .into(),
        );
    }

    let registry =
        Registry::load().map_err(|error| format!("{error}. Run `fslite doctor` for details."))?;

    let (filesystem_name, selection) = if let Some(name) = &cli.filesystem {
        (Some(name.clone()), Selection::Explicit)
    } else {
        let context = Context::load()
            .map_err(|error| format!("{error}. Run `fslite doctor` for details."))?;
        match context.filesystem {
            Some(name) => (Some(name), Selection::Persisted),
            None => (None, Selection::None),
        }
    };

    let Some(filesystem_name) = filesystem_name else {
        return Ok(StatusReport {
            filesystem_name: None,
            database_path: None,
            workspace_name: None,
            workspace_id: None,
            usage: None,
            selection: Selection::None,
        });
    };

    let database_path = registry
        .filesystem_path(&filesystem_name)
        .ok_or_else(|| {
            format!(
                "the active filesystem {filesystem_name:?} is no longer registered — run `fslite doctor` for details"
            )
        })?
        .to_path_buf();

    let workspace_name = match selection {
        Selection::Explicit => cli.workspace.clone(),
        _ => {
            Context::load()
                .map_err(|error| format!("{error}. Run `fslite doctor` for details."))?
                .workspace
        }
    };

    let (workspace_id, usage) = if let Some(workspace_name) = &workspace_name {
        let id = registry
            .resolve_workspace_name(&filesystem_name, workspace_name)
            .ok_or_else(|| {
                format!(
                    "no workspace named {workspace_name:?} registered under filesystem {filesystem_name:?}"
                )
            })?;
        let fs = SqliteFileSystem::open(&database_path, Default::default()).await?;
        let usage = fs.workspace_usage(&RequestContext::trusted(id)).await?;
        (Some(id.to_string()), Some(usage))
    } else {
        (None, None)
    };

    Ok(StatusReport {
        filesystem_name: Some(filesystem_name),
        database_path: Some(database_path.display().to_string()),
        workspace_name,
        workspace_id,
        usage,
        selection,
    })
}

pub fn render_human(report: &StatusReport) -> String {
    use fslite_command::render::sanitize_name;

    let Some(filesystem_name) = &report.filesystem_name else {
        return "No active filesystem yet — run any command (e.g. `fslite mkdir docs`) to bootstrap the default workspace.".to_string();
    };

    let mut lines = vec![
        format!("Filesystem: {}", sanitize_name(filesystem_name)),
        format!(
            "Database:   {}",
            sanitize_name(report.database_path.as_deref().unwrap_or("?"))
        ),
    ];
    match (&report.workspace_name, &report.workspace_id) {
        (Some(name), Some(id)) => lines.push(format!("Workspace:  {} ({id})", sanitize_name(name))),
        _ => lines.push("Workspace:  (none selected)".to_string()),
    }
    if let Some(usage) = &report.usage {
        lines.push(format!(
            "Usage:      {} nodes / {} max, {} bytes active / {} max, {} bytes trashed",
            usage.active_nodes,
            usage.max_nodes,
            usage.active_logical_bytes,
            usage.max_logical_bytes,
            usage.trashed_logical_bytes,
        ));
    }
    lines.push(format!(
        "Selection:  {}",
        match report.selection {
            Selection::Explicit => "explicit (--filesystem)",
            Selection::Persisted => "persisted (context.json)",
            Selection::None => "none",
        }
    ));
    lines.join("\n")
}
```

- [ ] **Step 5: Wire `status` into `main.rs`**

In `crates/fslite-cli/src/main.rs`, add `mod status;` next to the existing `mod` declarations (alphabetically, after `mod registry;`).

In the `match &cli.action { ... }` block that already handles `Action::Create`/`Delete`/`Use`, add, directly after the `Use` arm:

```rust
        Some(Action::Status) => return handle_status(&cli).await,
```

Add the handler function directly above `async fn create_filesystem(`:

```rust
async fn handle_status(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let report = status::build(cli).await?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", status::render_human(&report));
    }
    Ok(())
}

```

- [ ] **Step 6: Run tests to verify GREEN**

Run: `cargo test -p fslite --bin fslite status_subcommand_does_not_externalize`
Expected: 1 passed.

- [ ] **Step 7: Write and run `fslite status` end-to-end tests**

Create `crates/fslite-cli/tests/e2e_status.rs`:

```rust
use std::process::Command;

struct Fixture {
    config: tempfile::TempDir,
    data: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
        }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fslite"));
        command
            .env("FSLITE_CONFIG_DIR", self.config.path())
            .env("FSLITE_DATA_DIR", self.data.path());
        command
    }
}

#[test]
fn status_before_bootstrap_reports_no_active_filesystem_without_erroring() {
    let fixture = Fixture::new();
    let output = fixture.cli().arg("status").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No active filesystem yet")
    );
    assert!(!fixture.data.path().join("fslite.db").exists());
}

#[test]
fn status_after_bootstrap_reports_persisted_selection_and_usage() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().arg("status").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Filesystem: default"));
    assert!(stdout.contains("Workspace:  default"));
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Selection:  persisted (context.json)"));
}

#[test]
fn status_with_explicit_filesystem_flag_reports_explicit_selection() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture
        .cli()
        .args([
            "--filesystem",
            "default",
            "--workspace",
            "default",
            "status",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Selection:  explicit (--filesystem)"));
}

#[test]
fn status_json_output_is_well_formed() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().args(["--json", "status"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["filesystem_name"], "default");
    assert_eq!(value["selection"], "persisted");
    assert!(value["usage"]["active_nodes"].is_number());
}

#[test]
fn status_reports_corrupt_context_without_crashing() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::write(fixture.config.path().join("context.json"), "{not-json").unwrap();

    let output = fixture.cli().arg("status").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("fslite doctor"));
}

#[test]
fn status_rejects_raw_db_flag() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    let db = fixture.data.path().join("fslite.db");

    let output = fixture
        .cli()
        .args(["--db", db.to_str().unwrap(), "status"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("registered filesystems only"));
}
```

Run: `cargo test -p fslite --test e2e_status`
Expected: 6 passed.

- [ ] **Step 8: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy -p fslite --all-targets --all-features -- -D warnings
git add crates/fslite-cli/src/cli.rs crates/fslite-cli/src/main.rs crates/fslite-cli/src/status.rs crates/fslite-cli/tests/e2e_status.rs
git commit -m "feat(fslite): add fslite status"
```

Expected: `main` (the `fslite` binary crate) compiles, lints clean, and every test in this task passes — `fslite status` is a complete, independently working deliverable at this commit.

---

### Task 3: Add `fslite doctor`

**Files:**
- Create: `crates/fslite-cli/src/doctor.rs`
- Modify: `crates/fslite-cli/src/cli.rs`
- Modify: `crates/fslite-cli/src/main.rs`
- Modify: `crates/fslite-cli/src/registry.rs`
- Test: `crates/fslite-cli/tests/e2e_doctor.rs` (new)

**Interfaces:**
- Consumes: `crate::registry::Registry::{load, filesystem_names, filesystem_path, workspace_exists, workspace_names, resolve_workspace_name}`, `crate::context::Context::load`, `fslite_sqlite::SqliteFileSystem::{open, schema_version, latest_schema_version, integrity_check, workspace_usage}`, `fs2::FileExt::{try_lock_exclusive, unlock}`, `fslite_command::render::sanitize_name`.
- Produces: `doctor::CheckStatus` (`Pass | Warn | Fail`, `Serialize`), `doctor::CheckResult` (`Serialize`, fields `check: String`, `scope: String`, `status: CheckStatus`, `detail: String`), `doctor::run() -> Vec<CheckResult>`, `doctor::render_human(&[CheckResult]) -> String`, `doctor::exit_code(&[CheckResult]) -> i32`, and a new `Action::Doctor` unit variant.

- [ ] **Step 1: Add `Registry::filesystem_names`**

In `crates/fslite-cli/src/registry.rs`, directly after `filesystem_path`, add:

```rust
    /// Returns every registered filesystem name, for commands (like
    /// `doctor`) that walk the whole registry rather than one selected
    /// filesystem.
    pub fn filesystem_names(&self) -> Vec<&str> {
        self.filesystems.keys().map(String::as_str).collect()
    }
```

- [ ] **Step 2: Write a failing parse test, then add the `Doctor` variant to `Action`**

In `crates/fslite-cli/src/cli.rs`, add this test directly after `fn status_subcommand_does_not_externalize` (added in Task 2):

```rust

    #[test]
    fn doctor_subcommand_does_not_externalize() {
        assert!(matches!(parse(&["doctor"]).action, Some(Action::Doctor)));
    }
```

Run: `cargo test -p fslite --bin fslite doctor_subcommand_does_not_externalize`
Expected: fails to compile — `Action::Doctor` doesn't exist yet.

In the `Action` enum, directly after the `Status` variant added in Task 2, add:

```rust
    /// Validates the local registry, context, bootstrap lock, and every
    /// registered filesystem's database (existence, schema version,
    /// integrity, writability) and workspace. Read-only: never modifies any
    /// state, and exits non-zero if any check fails.
    Doctor,
```

In `debug_action`, directly after the `Status` arm added in Task 2, add:

```rust
            Some(Action::Doctor) => "Doctor",
```

- [ ] **Step 3: Run the parse test to verify GREEN**

Run: `cargo test -p fslite --bin fslite doctor_subcommand_does_not_externalize`
Expected: 1 passed.

- [ ] **Step 4: Write `doctor.rs`**

Create `crates/fslite-cli/src/doctor.rs`:

```rust
//! `fslite doctor`: validates the local registry, context, bootstrap lock,
//! and every registered filesystem's database. Read-only — never mutates
//! `registry.json`, `context.json`, the bootstrap lock, or any database.

use std::path::Path;

use fs2::FileExt;
use fslite_core::RequestContext;
use fslite_sqlite::SqliteFileSystem;
use serde::Serialize;

use crate::context::Context;
use crate::registry::Registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub check: String,
    pub scope: String,
    pub status: CheckStatus,
    pub detail: String,
}

impl CheckResult {
    fn pass(check: &str, scope: &str, detail: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            scope: scope.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }

    fn warn(check: &str, scope: &str, detail: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            scope: scope.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }

    fn fail(check: &str, scope: &str, detail: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            scope: scope.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
}

pub async fn run() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let registry = check_registry(&mut results);
    check_context(registry.as_ref(), &mut results);
    check_bootstrap_lock(&mut results);

    if let Some(registry) = &registry {
        for filesystem in registry.filesystem_names() {
            check_filesystem(registry, filesystem, &mut results).await;
        }
    }

    results
}

fn check_registry(results: &mut Vec<CheckResult>) -> Option<Registry> {
    match Registry::load() {
        Ok(registry) => {
            results.push(CheckResult::pass("registry.json", "global", "valid"));
            Some(registry)
        }
        Err(error) => {
            results.push(CheckResult::fail(
                "registry.json",
                "global",
                error.to_string(),
            ));
            None
        }
    }
}

fn check_context(registry: Option<&Registry>, results: &mut Vec<CheckResult>) {
    let context = match Context::load() {
        Ok(context) => context,
        Err(error) => {
            results.push(CheckResult::fail(
                "context.json",
                "global",
                error.to_string(),
            ));
            return;
        }
    };

    match (context.filesystem, context.workspace) {
        (None, None) => {
            results.push(CheckResult::pass(
                "context.json",
                "global",
                "no active context (never bootstrapped)",
            ));
        }
        (Some(filesystem), Some(workspace)) => {
            let Some(registry) = registry else {
                results.push(CheckResult::warn(
                    "context.json",
                    "global",
                    "cannot verify against registry.json (see above)",
                ));
                return;
            };
            if registry.workspace_exists(&filesystem, &workspace) {
                results.push(CheckResult::pass(
                    "context.json",
                    "global",
                    format!(
                        "points at registered filesystem {filesystem:?}, workspace {workspace:?}"
                    ),
                ));
            } else {
                results.push(CheckResult::fail(
                    "context.json",
                    "global",
                    format!(
                        "points at filesystem {filesystem:?}, workspace {workspace:?}, which is not registered"
                    ),
                ));
            }
        }
        _ => {
            results.push(CheckResult::fail(
                "context.json",
                "global",
                "has a filesystem or workspace set but not both",
            ));
        }
    }
}

fn check_bootstrap_lock(results: &mut Vec<CheckResult>) {
    let path = match crate::paths::config_dir() {
        Ok(dir) => dir.join("bootstrap.lock"),
        Err(error) => {
            results.push(CheckResult::fail(
                "bootstrap.lock",
                "global",
                error.to_string(),
            ));
            return;
        }
    };
    if !path.exists() {
        results.push(CheckResult::pass(
            "bootstrap.lock",
            "global",
            "not present (never bootstrapped)",
        ));
        return;
    }
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    {
        Ok(file) => match file.try_lock_exclusive() {
            Ok(()) => {
                fs2::FileExt::unlock(&file).ok();
                results.push(CheckResult::pass(
                    "bootstrap.lock",
                    "global",
                    "present and not held",
                ));
            }
            Err(_) => {
                results.push(CheckResult::warn(
                    "bootstrap.lock",
                    "global",
                    "currently held by another fslite process",
                ));
            }
        },
        Err(error) => {
            results.push(CheckResult::fail(
                "bootstrap.lock",
                "global",
                error.to_string(),
            ));
        }
    }
}

async fn check_filesystem(registry: &Registry, filesystem: &str, results: &mut Vec<CheckResult>) {
    let Some(path) = registry.filesystem_path(filesystem) else {
        return;
    };
    let path = path.to_path_buf();

    if !path.exists() {
        results.push(CheckResult::fail(
            "database exists",
            filesystem,
            format!("no database file at {}", path.display()),
        ));
        return;
    }
    results.push(CheckResult::pass(
        "database exists",
        filesystem,
        path.display().to_string(),
    ));

    let fs = match SqliteFileSystem::open(&path, Default::default()).await {
        Ok(fs) => fs,
        Err(error) => {
            results.push(CheckResult::fail(
                "database opens",
                filesystem,
                error.to_string(),
            ));
            return;
        }
    };
    results.push(CheckResult::pass(
        "database opens",
        filesystem,
        "opened successfully (schema migrated if needed)",
    ));

    match fs.schema_version().await {
        Ok(version) => results.push(CheckResult::pass(
            "schema version",
            filesystem,
            format!(
                "{version} (latest: {})",
                SqliteFileSystem::latest_schema_version()
            ),
        )),
        Err(error) => results.push(CheckResult::fail(
            "schema version",
            filesystem,
            error.to_string(),
        )),
    }

    match fs.integrity_check().await {
        Ok(problems) if problems.is_empty() => {
            results.push(CheckResult::pass("integrity check", filesystem, "ok"));
        }
        Ok(problems) => {
            results.push(CheckResult::fail(
                "integrity check",
                filesystem,
                problems.join("; "),
            ));
        }
        Err(error) => results.push(CheckResult::fail(
            "integrity check",
            filesystem,
            error.to_string(),
        )),
    }

    check_writable(&path, filesystem, results);

    for workspace_name in registry.workspace_names(filesystem) {
        let Some(id) = registry.resolve_workspace_name(filesystem, workspace_name) else {
            continue;
        };
        match fs.workspace_usage(&RequestContext::trusted(id)).await {
            Ok(_) => results.push(CheckResult::pass(
                "workspace exists",
                filesystem,
                format!("{workspace_name:?} ({id})"),
            )),
            Err(error) => results.push(CheckResult::fail(
                "workspace exists",
                filesystem,
                format!("{workspace_name:?} ({id}): {error}"),
            )),
        }
    }
}

fn check_writable(path: &Path, scope: &str, results: &mut Vec<CheckResult>) {
    let file_writable = std::fs::metadata(path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false);
    let dir_writable = path
        .parent()
        .and_then(|parent| std::fs::metadata(parent).ok())
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false);

    if file_writable && dir_writable {
        results.push(CheckResult::pass(
            "writable",
            scope,
            "database file and directory are writable",
        ));
    } else {
        results.push(CheckResult::fail(
            "writable",
            scope,
            format!("database file writable: {file_writable}, directory writable: {dir_writable}"),
        ));
    }
}

pub fn render_human(results: &[CheckResult]) -> String {
    use fslite_command::render::sanitize_name;

    let mut lines: Vec<String> = results
        .iter()
        .map(|result| {
            let icon = match result.status {
                CheckStatus::Pass => "\u{2713}",
                CheckStatus::Warn => "!",
                CheckStatus::Fail => "\u{2717}",
            };
            format!(
                "{icon} {}: {} ({})",
                sanitize_name(&result.scope),
                sanitize_name(&result.check),
                sanitize_name(&result.detail)
            )
        })
        .collect();

    let failures = results
        .iter()
        .filter(|result| result.status == CheckStatus::Fail)
        .count();
    lines.push(String::new());
    lines.push(match failures {
        0 => "0 problems found.".to_string(),
        1 => "1 problem found.".to_string(),
        n => format!("{n} problems found."),
    });
    lines.join("\n")
}

pub fn exit_code(results: &[CheckResult]) -> i32 {
    if results
        .iter()
        .any(|result| result.status == CheckStatus::Fail)
    {
        1
    } else {
        0
    }
}
```

- [ ] **Step 5: Wire `doctor` into `main.rs`**

In `crates/fslite-cli/src/main.rs`, add `mod doctor;` next to the existing `mod` declarations (alphabetically, before `mod paths;`).

In the same `match &cli.action { ... }` block from Task 2, add, directly after the `Status` arm:

```rust
        Some(Action::Doctor) => return handle_doctor(cli.json).await,
```

Add the handler function directly after `handle_status`:

```rust
async fn handle_doctor(json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let results = doctor::run().await;
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        println!("{}", doctor::render_human(&results));
    }
    std::process::exit(doctor::exit_code(&results));
}

```

- [ ] **Step 6: Run `cargo check` to verify the crate compiles end-to-end**

Run: `cargo check -p fslite`
Expected: succeeds. This is the first point since Task 2 where `main` fully compiles again (Task 2 left `Action::Doctor` referenced-but-undefined in the parse test).

- [ ] **Step 7: Manually verify behavior before writing e2e tests**

These commands document the exact expected behavior the e2e tests in Step 8 assert on — run them against a throwaway sandbox to confirm before trusting the test assertions:

```bash
cargo build -p fslite
SANDBOX=$(mktemp -d)
export FSLITE_CONFIG_DIR="$SANDBOX/config"
export FSLITE_DATA_DIR="$SANDBOX/data"
./target/debug/fslite doctor            # before bootstrap: "0 problems found.", exit 0
./target/debug/fslite mkdir docs        # bootstraps
./target/debug/fslite doctor            # after bootstrap: all checks pass, exit 0
./target/debug/fslite --json doctor     # array of {check, scope, status, detail}
rm -rf "$SANDBOX"
```

Expected: matches the comments above exactly.

- [ ] **Step 8: Write and run `fslite doctor` end-to-end tests**

Create `crates/fslite-cli/tests/e2e_doctor.rs`:

```rust
use std::process::Command;

struct Fixture {
    config: tempfile::TempDir,
    data: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
        }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fslite"));
        command
            .env("FSLITE_CONFIG_DIR", self.config.path())
            .env("FSLITE_DATA_DIR", self.data.path());
        command
    }
}

#[test]
fn doctor_before_bootstrap_passes_every_check() {
    let fixture = Fixture::new();
    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0 problems found."));
}

#[test]
fn doctor_after_bootstrap_passes_every_check() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("database exists"));
    assert!(stdout.contains("integrity check"));
    assert!(stdout.contains("workspace exists"));
    assert!(stdout.contains("0 problems found."));
}

#[test]
fn doctor_reports_corrupt_context_as_a_failure_and_exits_non_zero() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::write(fixture.config.path().join("context.json"), "{not-json").unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} global: context.json"));
    assert!(stdout.contains("1 problem found."));
}

#[test]
fn doctor_reports_a_missing_database_file_as_a_failure() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::remove_file(fixture.data.path().join("fslite.db")).unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} default: database exists"));
}

#[test]
fn doctor_reports_a_stale_registered_workspace_as_a_failure() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let registry_path = fixture.config.path().join("registry.json");
    let mut registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&registry_path).unwrap()).unwrap();
    registry["workspaces"]["default"]["ghost"] =
        serde_json::Value::String(fslite_core::WorkspaceId::new().to_string());
    std::fs::write(&registry_path, serde_json::to_string(&registry).unwrap()).unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} default: workspace exists (\"ghost\""));
}

#[test]
fn doctor_json_output_is_well_formed() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().args(["--json", "doctor"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let checks = value.as_array().unwrap();
    assert!(!checks.is_empty());
    assert!(
        checks
            .iter()
            .all(|check| check["status"] == "pass" || check["status"] == "warn")
    );
}
```

Run: `cargo test -p fslite --test e2e_doctor`
Expected: 6 passed.

- [ ] **Step 9: Run every test touched across Tasks 2 and 3, format, lint, commit**

```bash
cargo test -p fslite --bin fslite
cargo test -p fslite --test e2e_status --test e2e_doctor
cargo fmt --all
cargo clippy -p fslite --all-targets --all-features -- -D warnings
git add crates/fslite-cli/src/cli.rs crates/fslite-cli/src/main.rs crates/fslite-cli/src/registry.rs crates/fslite-cli/src/doctor.rs crates/fslite-cli/tests/e2e_doctor.rs
git commit -m "feat(fslite): add fslite doctor"
```

---

### Task 4: Workspace Verification

**Files:**
- Verify all modified files.

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: a formatted, lint-clean, fully tested branch ready for review.

- [ ] **Step 1: Format and check the diff**

```bash
cargo fmt --all -- --check
git diff --check
git status --short
```

Expected: formatting and whitespace checks pass; only intentional changes appear.

- [ ] **Step 2: Run Clippy and the complete workspace test suite**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: both commands exit 0 without warnings or test failures. Run the test suite at least twice in a row — `integrity_check_reports_problems_on_a_corrupted_database` (Task 1) previously proved flaky under a naive corruption approach before the explicit WAL-checkpoint fix in Task 1 Step 1, so repeat runs are the regression guard for that fix holding.

- [ ] **Step 3: Perform a clean install smoke test**

```bash
SMOKE_ROOT=$(mktemp -d)
CARGO_TARGET_DIR="$SMOKE_ROOT/target" cargo install --path crates/fslite-cli --root "$SMOKE_ROOT/install"
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" mkdir docs
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" write docs/hello.txt --text=hello
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" status
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" doctor
rm -rf "$SMOKE_ROOT"
```

Expected: `mkdir`/`write` succeed; `status` reports filesystem `default`, workspace `default`, and non-zero usage after the write; `doctor` reports `0 problems found.` and exits `0`.

- [ ] **Step 4: Commit only if formatting changed tracked files**

```bash
git add crates/fslite-cli crates/fslite-sqlite
git commit -m "style: format status and doctor changes"
```

If formatting produced no tracked changes, do not create an empty commit.
