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
