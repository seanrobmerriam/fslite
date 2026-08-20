//! Resolves durable server state before accepting requests.

use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use fslite_core::{Capability, ErrorCode, FileSystem, RequestContext, WorkspaceId};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::{SqliteFileSystem, WorkspaceOptions};

use crate::credential_store::{generate_token, load_state, save_state};
use crate::server_config::{ResolvedServerConfig, StoredServerState, TokenSource, WorkspaceLimits};

const FIRST_RUN_MESSAGE: &str =
    "No database or workspace found, creating default database and workspace";

/// The durable resources and request state selected during startup.
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

impl BootstrapResult {
    pub(crate) fn bootstrap_message(&self) -> Option<&'static str> {
        self.created_database_or_workspace
            .then_some(FIRST_RUN_MESSAGE)
    }

    pub(crate) fn app_state(&self) -> AppState {
        let actor = AuthenticatedActor {
            workspace_id: self.workspace_id,
            capabilities: BTreeSet::from([
                Capability::Read,
                Capability::Write,
                Capability::Delete,
                Capability::TrashRestore,
                Capability::WorkspaceAdmin,
            ]),
            actor_metadata: Default::default(),
        };
        let mut tokens = HashMap::new();
        tokens.insert(self.token.clone(), actor);
        AppState {
            fs: self.sqlite.clone() as Arc<dyn FileSystem>,
            auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
            admin: Arc::new(SqliteWorkspaceAdmin(self.sqlite.clone())),
            health_workspace: self.workspace_id,
        }
    }

    pub(crate) fn print_connection_guidance(&self) {
        debug_assert!(self.database_path.exists());
        let server = format!("http://{}", self.bind);
        if self.generated_token {
            println!(
                "FSLITE_TOKEN={} fslite --server {} --workspace {} ls /",
                self.token, server, self.workspace_id
            );
        } else {
            println!(
                "Using persisted server configuration at {}",
                self.config_path.display()
            );
            println!(
                "FSLITE_TOKEN=$FSLITE_TOKEN fslite --server {} --workspace {} ls /",
                server, self.workspace_id
            );
        }
    }
}

/// Opens the selected database, makes sure the default workspace exists, and
/// saves any durable state changes without replacing a process credential.
pub(crate) async fn bootstrap(
    config: ResolvedServerConfig,
) -> Result<BootstrapResult, Box<dyn std::error::Error>> {
    let database_previously_existed = config.database_path.exists();
    if let Some(parent) = config
        .database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let stored = load_state(&config.config_path)?;
    let sqlite = Arc::new(SqliteFileSystem::open(&config.database_path, Default::default()).await?);

    let (workspace_id, created_workspace) = match config.workspace_id {
        Some(workspace_id) => match sqlite
            .workspace_usage(&RequestContext::trusted(workspace_id))
            .await
        {
            Ok(_) => (workspace_id, false),
            Err(error) if error.code() == ErrorCode::NotFound => (
                create_workspace(sqlite.as_ref(), config.workspace_limits).await?,
                true,
            ),
            Err(error) => return Err(error.into()),
        },
        None => (
            create_workspace(sqlite.as_ref(), config.workspace_limits).await?,
            true,
        ),
    };

    let durable_token = durable_token(&config, stored.as_ref());
    let next_state = StoredServerState {
        database_path: config.database_path.clone(),
        bind: config.bind,
        workspace_id,
        token: durable_token,
        workspace_limits: config.workspace_limits,
    };
    if stored
        .as_ref()
        .is_none_or(|state| state_changed(state, &next_state))
    {
        save_state(&config.config_path, &next_state)?;
    }

    Ok(BootstrapResult {
        sqlite,
        workspace_id,
        token: config.token,
        bind: config.bind,
        database_path: config.database_path,
        config_path: config.config_path,
        created_database_or_workspace: !database_previously_existed || created_workspace,
        generated_token: matches!(config.token_source, TokenSource::Generated),
    })
}

async fn create_workspace(
    sqlite: &SqliteFileSystem,
    limits: WorkspaceLimits,
) -> Result<WorkspaceId, fslite_core::FsError> {
    let mut options = WorkspaceOptions::default();
    options.max_bytes = limits.max_bytes;
    options.max_nodes = limits.max_nodes;
    options.max_file_bytes = limits.max_file_bytes;
    let workspace = sqlite.create_workspace(options).await?;
    Ok(workspace.id)
}

fn durable_token(config: &ResolvedServerConfig, stored: Option<&StoredServerState>) -> String {
    if config.token_source.is_process_override() {
        stored
            .map(|state| state.token.clone())
            .unwrap_or_else(generate_token)
    } else {
        config.token.clone()
    }
}

fn state_changed(current: &StoredServerState, next: &StoredServerState) -> bool {
    current.database_path != next.database_path
        || current.bind != next.bind
        || current.workspace_id != next.workspace_id
        || current.workspace_limits != next.workspace_limits
        || current.token != next.token
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::path::Path;

    use fslite_core::{ErrorCode, RequestContext};
    use fslite_sqlite::SqliteFileSystem;

    use super::bootstrap;
    use crate::credential_store::load_state;
    use crate::server_config::{ResolvedServerConfig, TokenSource, WorkspaceLimits};

    fn limits() -> WorkspaceLimits {
        WorkspaceLimits {
            max_bytes: 100,
            max_nodes: 20,
            max_file_bytes: 10,
        }
    }

    fn config(dir: &Path) -> ResolvedServerConfig {
        ResolvedServerConfig {
            database_path: dir.join("fslite.db"),
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            config_path: dir.join("server.json"),
            workspace_id: None,
            token: "generated-default-token".to_owned(),
            token_source: TokenSource::Generated,
            workspace_limits: limits(),
        }
    }

    #[tokio::test]
    async fn no_files_create_workspace_state_and_generated_token() {
        let dir = tempfile::tempdir().unwrap();
        let result = bootstrap(config(dir.path())).await.unwrap();

        assert!(result.created_database_or_workspace);
        assert!(result.generated_token);
        assert!(result.database_path.exists());
        assert_eq!(
            result.bootstrap_message(),
            Some("No database or workspace found, creating default database and workspace")
        );
        let state = load_state(&result.config_path).unwrap().unwrap();
        assert_eq!(state.workspace_id, result.workspace_id);
        assert_eq!(state.token, result.token);
        assert_eq!(
            result
                .sqlite
                .workspace_usage(&RequestContext::trusted(result.workspace_id))
                .await
                .unwrap()
                .max_logical_bytes,
            limits().max_bytes
        );
    }

    #[tokio::test]
    async fn missing_database_parent_is_created_before_opening_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let mut resolved = config(dir.path());
        resolved.database_path = dir.path().join("new-data").join("fslite.db");

        let result = bootstrap(resolved).await.unwrap();

        assert!(result.database_path.exists());
    }

    #[tokio::test]
    async fn restart_reuses_database_workspace_and_token() {
        let dir = tempfile::tempdir().unwrap();
        let first = bootstrap(config(dir.path())).await.unwrap();
        let stored = load_state(&first.config_path).unwrap().unwrap();
        let mut restart = config(dir.path());
        restart.workspace_id = Some(stored.workspace_id);
        restart.token = stored.token.clone();
        restart.token_source = TokenSource::Stored;

        let second = bootstrap(restart).await.unwrap();

        assert!(!second.created_database_or_workspace);
        assert!(!second.generated_token);
        assert_eq!(second.workspace_id, first.workspace_id);
        assert_eq!(second.token, first.token);
    }

    #[tokio::test]
    async fn supplied_token_overrides_stored_token_without_writing_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let first = bootstrap(config(dir.path())).await.unwrap();
        let stored = load_state(&first.config_path).unwrap().unwrap();
        let mut override_config = config(dir.path());
        override_config.workspace_id = Some(stored.workspace_id);
        override_config.token = "process-override-token".to_owned();
        override_config.token_source = TokenSource::Environment;

        let overridden = bootstrap(override_config).await.unwrap();

        assert_eq!(overridden.token, "process-override-token");
        assert_eq!(
            load_state(&overridden.config_path).unwrap().unwrap().token,
            first.token
        );
    }

    #[tokio::test]
    async fn existing_database_without_state_gets_new_workspace_without_touching_others() {
        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("fslite.db");
        let sqlite = SqliteFileSystem::open(&database_path, Default::default())
            .await
            .unwrap();
        let unrelated = sqlite.create_workspace(Default::default()).await.unwrap();
        drop(sqlite);

        let result = bootstrap(config(dir.path())).await.unwrap();

        assert_ne!(result.workspace_id, unrelated.id);
        let reopened = SqliteFileSystem::open(&database_path, Default::default())
            .await
            .unwrap();
        reopened
            .workspace_usage(&RequestContext::trusted(unrelated.id))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deleted_stored_workspace_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let first = bootstrap(config(dir.path())).await.unwrap();
        first
            .sqlite
            .delete_workspace(first.workspace_id)
            .await
            .unwrap();
        let stored = load_state(&first.config_path).unwrap().unwrap();
        let mut restart = config(dir.path());
        restart.workspace_id = Some(stored.workspace_id);
        restart.token = stored.token;
        restart.token_source = TokenSource::Stored;

        let replacement = bootstrap(restart).await.unwrap();

        assert_ne!(replacement.workspace_id, first.workspace_id);
        assert_eq!(
            first
                .sqlite
                .workspace_usage(&RequestContext::trusted(first.workspace_id))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            load_state(&replacement.config_path)
                .unwrap()
                .unwrap()
                .workspace_id,
            replacement.workspace_id
        );
    }

    #[tokio::test]
    async fn configured_limits_apply_only_when_creating_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let first = bootstrap(config(dir.path())).await.unwrap();
        let stored = load_state(&first.config_path).unwrap().unwrap();
        let mut changed = config(dir.path());
        changed.workspace_id = Some(stored.workspace_id);
        changed.token = stored.token;
        changed.token_source = TokenSource::Stored;
        changed.workspace_limits = WorkspaceLimits {
            max_bytes: 999,
            max_nodes: 888,
            max_file_bytes: 777,
        };

        let restarted = bootstrap(changed).await.unwrap();

        let usage = restarted
            .sqlite
            .workspace_usage(&RequestContext::trusted(restarted.workspace_id))
            .await
            .unwrap();
        assert_eq!(usage.max_logical_bytes, limits().max_bytes);
        assert_eq!(usage.max_nodes, limits().max_nodes);
        assert_eq!(usage.max_file_bytes, limits().max_file_bytes);
    }
}
