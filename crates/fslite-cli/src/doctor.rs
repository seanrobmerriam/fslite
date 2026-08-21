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
