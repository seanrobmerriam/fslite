//! A local, CLI-managed registry mapping human-friendly filesystem and
//! workspace names to their underlying SQLite file paths and workspace ids.
//! `fslite-core`/`fslite-sqlite` have no concept of a name — a `Workspace`
//! is identified purely by `WorkspaceId` (a UUID) — so this registry exists
//! entirely client-side, in `fslite-cli`, and is invisible to every other
//! consumer of the workspace (a remote `fslite-server`, another client,
//! `fslite-command`'s own executors).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fslite_core::WorkspaceId;
use serde::{Deserialize, Serialize};

use crate::paths::config_dir;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Registry {
    filesystems: BTreeMap<String, PathBuf>,
    workspaces: BTreeMap<String, BTreeMap<String, WorkspaceId>>,
}

impl Registry {
    fn path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        Ok(config_dir()?.join("registry.json"))
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::path()?;
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(serde_json::from_str(&contents)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Registry::default()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::path()?;
        crate::persistence::write_json(&path, self)
    }

    pub fn filesystem_exists(&self, name: &str) -> bool {
        self.filesystems.contains_key(name)
    }

    pub fn filesystem_path(&self, name: &str) -> Option<&Path> {
        self.filesystems.get(name).map(PathBuf::as_path)
    }

    pub fn register_filesystem(&mut self, name: String, path: PathBuf) {
        self.filesystems.insert(name, path);
    }

    /// Forgets a filesystem and every workspace name registered under it.
    /// Does not touch anything on disk — the caller deletes the actual db
    /// file separately.
    pub fn remove_filesystem(&mut self, name: &str) {
        self.filesystems.remove(name);
        self.workspaces.remove(name);
    }

    pub fn workspace_exists(&self, filesystem: &str, workspace_name: &str) -> bool {
        self.workspaces
            .get(filesystem)
            .is_some_and(|workspaces| workspaces.contains_key(workspace_name))
    }

    /// Returns every workspace name registered under `filesystem`, for
    /// confirmation prompts (e.g. `delete` listing what it's about to
    /// forget).
    pub fn workspace_names(&self, filesystem: &str) -> Vec<&str> {
        self.workspaces
            .get(filesystem)
            .map(|workspaces| workspaces.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn register_workspace(
        &mut self,
        filesystem: &str,
        workspace_name: String,
        id: WorkspaceId,
    ) {
        self.workspaces
            .entry(filesystem.to_string())
            .or_default()
            .insert(workspace_name, id);
    }

    /// Resolves a workspace *name* (not a raw id — callers that also need
    /// to accept a raw `WorkspaceId` string must try `WorkspaceId::parse`
    /// themselves first, since that check has no dependency on the
    /// registry or on any filesystem being registered at all).
    pub fn resolve_workspace_name(
        &self,
        filesystem: &str,
        workspace_name: &str,
    ) -> Option<WorkspaceId> {
        self.workspaces
            .get(filesystem)?
            .get(workspace_name)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::with_temp_config_dir;

    #[test]
    fn round_trips_through_save_and_load() {
        with_temp_config_dir(|| {
            let mut registry = Registry::load().unwrap();
            assert!(!registry.filesystem_exists("main"));

            registry.register_filesystem("main".to_string(), PathBuf::from("/tmp/main.db"));
            let id = WorkspaceId::new();
            registry.register_workspace("main", "primary".to_string(), id);
            registry.save().unwrap();

            let reloaded = Registry::load().unwrap();
            assert_eq!(
                reloaded.filesystem_path("main"),
                Some(Path::new("/tmp/main.db"))
            );
            assert_eq!(reloaded.resolve_workspace_name("main", "primary"), Some(id));
            assert!(!reloaded.workspace_exists("main", "missing"));
        });
    }

    #[test]
    fn remove_filesystem_forgets_its_workspaces_too() {
        with_temp_config_dir(|| {
            let mut registry = Registry::load().unwrap();
            registry.register_filesystem("main".to_string(), PathBuf::from("/tmp/main.db"));
            registry.register_workspace("main", "primary".to_string(), WorkspaceId::new());

            registry.remove_filesystem("main");

            assert!(!registry.filesystem_exists("main"));
            assert!(!registry.workspace_exists("main", "primary"));
        });
    }

    #[test]
    fn missing_registry_file_loads_as_empty_default() {
        with_temp_config_dir(|| {
            let registry = Registry::load().unwrap();
            assert!(!registry.filesystem_exists("anything"));
        });
    }
}
