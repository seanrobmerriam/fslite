//! First-run default database/workspace initialization.
use std::path::Path;

use fs2::FileExt;
use fslite_core::RequestContext;
use fslite_sqlite::SqliteFileSystem;

use crate::context::Context;
use crate::registry::Registry;

pub const DEFAULT_FILESYSTEM: &str = "default";
pub const DEFAULT_WORKSPACE: &str = "default";
pub const NOTICE: &str =
    "No database or workspace found, creating default database and workspace";

#[derive(Debug)]
pub struct BootstrapOutcome {
    pub created: bool,
}

pub async fn ensure_default() -> Result<BootstrapOutcome, Box<dyn std::error::Error>> {
    if active_context_is_valid().await? {
        return Ok(BootstrapOutcome { created: false });
    }

    let config = crate::paths::config_dir()?;
    std::fs::create_dir_all(&config)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(config.join("bootstrap.lock"))?;
    lock.lock_exclusive()?;

    if active_context_is_valid().await? {
        return Ok(BootstrapOutcome { created: false });
    }

    let mut registry = Registry::load()?;
    let db_path = crate::paths::data_dir()?.join("fslite.db");
    let registered_path = registry
        .filesystem_path(DEFAULT_FILESYSTEM)
        .map(Path::to_path_buf);

    if registered_path.is_none() && !registry.is_empty() {
        return Err("registered filesystems exist but none is active; run `fslite use <filesystem> -w <workspace>` instead of creating an implicit default".into());
    }

    let mut created = false;
    if let Some(path) = &registered_path {
        if path != &db_path {
            return Err(format!(
                "reserved filesystem \"default\" points to {}, expected {}; choose another name or repair the registry",
                path.display(),
                db_path.display()
            )
            .into());
        }
        if !path.exists() {
            return Err(format!(
                "registered default database is missing at {}; restore it or delete the default registry entry",
                path.display()
            )
            .into());
        }
    } else {
        let parent = db_path
            .parent()
            .ok_or_else(|| format!("default database path has no parent: {}", db_path.display()))?;
        std::fs::create_dir_all(parent)?;
        registry.register_filesystem(DEFAULT_FILESYSTEM.into(), db_path.clone());
        created = true;
    }

    if !registry.workspace_exists(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE) {
        let fs = SqliteFileSystem::open(&db_path, Default::default()).await?;
        let workspace = fs.create_workspace(Default::default()).await?;
        registry.register_workspace(
            DEFAULT_FILESYSTEM,
            DEFAULT_WORKSPACE.into(),
            workspace.id,
        );
        created = true;
    }

    registry.save()?;
    Context {
        filesystem: Some(DEFAULT_FILESYSTEM.into()),
        workspace: Some(DEFAULT_WORKSPACE.into()),
    }
    .save()?;
    Ok(BootstrapOutcome { created })
}

async fn active_context_is_valid() -> Result<bool, Box<dyn std::error::Error>> {
    let context = Context::load()?;
    let (filesystem, workspace) = match (context.filesystem, context.workspace) {
        (None, None) => return Ok(false),
        (Some(filesystem), Some(workspace)) => (filesystem, workspace),
        _ => {
            return Err("the active context is incomplete; run `fslite use <filesystem> -w <workspace>` to repair it".into());
        }
    };

    let registry = Registry::load()?;
    let path = registry.filesystem_path(&filesystem).ok_or_else(|| {
        format!(
            "the active filesystem {filesystem:?} is no longer registered; run `fslite use` again"
        )
    })?;
    if !path.exists() {
        return Err(format!(
            "the active filesystem {filesystem:?} references a missing database at {}; restore it or select another filesystem",
            path.display()
        )
        .into());
    }
    let workspace_id = registry
        .resolve_workspace_name(&filesystem, &workspace)
        .ok_or_else(|| {
            format!(
                "the active workspace {workspace:?} is no longer registered under filesystem {filesystem:?}; run `fslite use` again"
            )
        })?;
    let fs = SqliteFileSystem::open(path, Default::default()).await?;
    fs.workspace_usage(&RequestContext::trusted(workspace_id))
        .await
        .map_err(|error| {
            format!(
                "the active workspace {workspace:?} cannot be opened from {}: {error}; run `fslite use` again",
                path.display()
            )
        })?;
    Ok(true)
}

#[cfg(test)]
#[allow(unsafe_code)] // SAFETY: TestEnvironment holds the shared env lock.
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::MutexGuard;

    use fslite_sqlite::SqliteFileSystem;

    use super::*;
    use crate::context::Context;
    use crate::registry::Registry;

    struct TestEnvironment {
        _config: tempfile::TempDir,
        data: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    impl TestEnvironment {
        fn new() -> Self {
            let guard = crate::test_support::lock();
            let config = tempfile::tempdir().unwrap();
            let data = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("FSLITE_CONFIG_DIR", config.path());
                std::env::set_var("FSLITE_DATA_DIR", data.path());
            }
            Self {
                _config: config,
                data,
                _guard: guard,
            }
        }

        fn db_path(&self) -> PathBuf {
            self.data.path().join("fslite.db")
        }
    }

    impl Drop for TestEnvironment {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("FSLITE_CONFIG_DIR");
                std::env::remove_var("FSLITE_DATA_DIR");
            }
        }
    }

    async fn seed_default(db_path: &Path) -> fslite_core::WorkspaceId {
        let fs = SqliteFileSystem::open(db_path, Default::default())
            .await
            .unwrap();
        fs.create_workspace(Default::default()).await.unwrap().id
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_state_creates_and_selects_one_default_workspace() {
        let env = TestEnvironment::new();
        let outcome = ensure_default().await.unwrap();
        assert!(outcome.created);
        assert!(env.db_path().exists());
        let registry = Registry::load().unwrap();
        assert!(
            registry
                .resolve_workspace_name(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE)
                .is_some()
        );
        let context = Context::load().unwrap();
        assert_eq!(context.filesystem.as_deref(), Some(DEFAULT_FILESYSTEM));
        assert_eq!(context.workspace.as_deref(), Some(DEFAULT_WORKSPACE));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn valid_active_context_is_a_noop() {
        let env = TestEnvironment::new();
        let workspace_id = seed_default(&env.db_path()).await;
        let mut registry = Registry::load().unwrap();
        registry.register_filesystem(DEFAULT_FILESYSTEM.into(), env.db_path());
        registry.register_workspace(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE.into(), workspace_id);
        registry.save().unwrap();
        Context {
            filesystem: Some(DEFAULT_FILESYSTEM.into()),
            workspace: Some(DEFAULT_WORKSPACE.into()),
        }
        .save()
        .unwrap();

        let outcome = ensure_default().await.unwrap();
        assert!(!outcome.created);
        assert_eq!(
            Registry::load()
                .unwrap()
                .resolve_workspace_name(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE),
            Some(workspace_id)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn existing_default_is_activated_without_a_second_workspace() {
        let env = TestEnvironment::new();
        let workspace_id = seed_default(&env.db_path()).await;
        let mut registry = Registry::load().unwrap();
        registry.register_filesystem(DEFAULT_FILESYSTEM.into(), env.db_path());
        registry.register_workspace(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE.into(), workspace_id);
        registry.save().unwrap();

        let outcome = ensure_default().await.unwrap();
        assert!(!outcome.created);
        assert_eq!(
            Registry::load()
                .unwrap()
                .resolve_workspace_name(DEFAULT_FILESYSTEM, DEFAULT_WORKSPACE),
            Some(workspace_id)
        );
        assert_eq!(
            Context::load().unwrap().workspace.as_deref(),
            Some(DEFAULT_WORKSPACE)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_registered_database_is_an_error() {
        let env = TestEnvironment::new();
        let mut registry = Registry::load().unwrap();
        registry.register_filesystem(DEFAULT_FILESYSTEM.into(), env.db_path());
        registry.save().unwrap();

        let error = ensure_default().await.unwrap_err().to_string();
        assert!(error.contains("missing"), "{error}");
        assert!(error.contains(&env.db_path().display().to_string()), "{error}");
        assert!(Context::load().unwrap().filesystem.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conflicting_default_path_is_not_overwritten() {
        let env = TestEnvironment::new();
        let other = env.data.path().join("other.db");
        let mut registry = Registry::load().unwrap();
        registry.register_filesystem(DEFAULT_FILESYSTEM.into(), other.clone());
        registry.save().unwrap();

        let error = ensure_default().await.unwrap_err().to_string();
        assert!(error.contains("reserved"), "{error}");
        assert!(error.contains(&other.display().to_string()), "{error}");
        assert!(error.contains(&env.db_path().display().to_string()), "{error}");
        assert_eq!(
            Registry::load().unwrap().filesystem_path(DEFAULT_FILESYSTEM),
            Some(other.as_path())
        );
    }
}
