//! The "current" filesystem/workspace `fslite use` sets, so later commands
//! that omit `--db`/`--workspace`/`--filesystem` operate against it by
//! default. Entirely client-side, like `crate::registry` — this has no
//! effect on, or visibility into, `fslite-core`/`fslite-sqlite`/`fslite-server`.

use serde::{Deserialize, Serialize};

use crate::paths::config_dir;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Context {
    pub filesystem: Option<String>,
    pub workspace: Option<String>,
}

impl Context {
    fn path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        Ok(config_dir()?.join("context.json"))
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::path()?;
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(serde_json::from_str(&contents)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Context::default()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Clears the persisted context if it currently points at `filesystem`
    /// — called by `delete` so a later bare verb command doesn't silently
    /// resolve against a filesystem that no longer exists.
    pub fn clear_if_filesystem(filesystem: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut context = Self::load()?;
        if context.filesystem.as_deref() == Some(filesystem) {
            context.filesystem = None;
            context.workspace = None;
            context.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::with_temp_config_dir;

    #[test]
    fn round_trips_through_save_and_load() {
        with_temp_config_dir(|| {
            let context = Context {
                filesystem: Some("main".to_string()),
                workspace: Some("primary".to_string()),
            };
            context.save().unwrap();

            let reloaded = Context::load().unwrap();
            assert_eq!(reloaded.filesystem.as_deref(), Some("main"));
            assert_eq!(reloaded.workspace.as_deref(), Some("primary"));
        });
    }

    #[test]
    fn missing_context_file_loads_as_empty_default() {
        with_temp_config_dir(|| {
            let context = Context::load().unwrap();
            assert!(context.filesystem.is_none());
            assert!(context.workspace.is_none());
        });
    }

    #[test]
    fn clear_if_filesystem_only_clears_a_matching_context() {
        with_temp_config_dir(|| {
            Context {
                filesystem: Some("main".to_string()),
                workspace: Some("primary".to_string()),
            }
            .save()
            .unwrap();

            Context::clear_if_filesystem("other").unwrap();
            assert_eq!(Context::load().unwrap().filesystem.as_deref(), Some("main"));

            Context::clear_if_filesystem("main").unwrap();
            assert!(Context::load().unwrap().filesystem.is_none());
        });
    }
}
