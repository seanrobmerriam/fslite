#![allow(dead_code)] // Task 5 consumes these binary-only configuration interfaces.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use fslite_core::WorkspaceId;
use serde::{Deserialize, Serialize};

use crate::credential_store::{generate_token, load_state, read_token_file};

const DEFAULT_BIND: &str = "127.0.0.1:8080";

/// Command-line and environment configuration. Each option remains optional
/// until persisted configuration has had a chance to supply it.
#[derive(Debug, Default, Parser)]
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

/// The directories used for the server's database and durable metadata.
#[derive(Clone, Debug)]
pub(crate) struct ServerPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl ServerPaths {
    pub(crate) fn platform_default() -> Result<Self, ConfigError> {
        let project = directories::ProjectDirs::from("", "", "fslite")
            .ok_or(ConfigError::PlatformDirectoriesUnavailable)?;
        Ok(Self {
            data_dir: project.data_local_dir().to_path_buf(),
            config_dir: project.config_dir().to_path_buf(),
        })
    }
}

/// Workspace quotas persisted with the default workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct WorkspaceLimits {
    pub max_bytes: u64,
    pub max_nodes: u64,
    pub max_file_bytes: u64,
}

impl Default for WorkspaceLimits {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024 * 1024,
            max_nodes: 1_000_000,
            max_file_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// The durable, non-process-specific server state.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct StoredServerState {
    pub database_path: PathBuf,
    pub bind: SocketAddr,
    pub workspace_id: WorkspaceId,
    pub token: String,
    pub workspace_limits: WorkspaceLimits,
}

impl fmt::Debug for StoredServerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredServerState")
            .field("database_path", &self.database_path)
            .field("bind", &self.bind)
            .field("workspace_id", &self.workspace_id)
            .field("token", &"[REDACTED]")
            .field("workspace_limits", &self.workspace_limits)
            .finish()
    }
}

/// Errors raised while resolving or persisting server configuration.
#[derive(Debug)]
pub(crate) enum ConfigError {
    Io(std::io::Error),
    Json(serde_json::Error),
    PlatformDirectoriesUnavailable,
    EmptyTokenFile(PathBuf),
    EmptyStoredToken,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "configuration I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid configuration state: {error}"),
            Self::PlatformDirectoriesUnavailable => {
                formatter.write_str("platform configuration directories are unavailable")
            }
            Self::EmptyTokenFile(_) | Self::EmptyStoredToken => {
                formatter.write_str("credential is empty")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::PlatformDirectoriesUnavailable
            | Self::EmptyTokenFile(_)
            | Self::EmptyStoredToken => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// The complete process configuration after CLI/environment, persisted state,
/// and platform defaults have been combined.
pub(crate) struct ResolvedServerConfig {
    pub database_path: PathBuf,
    pub bind: SocketAddr,
    pub config_path: PathBuf,
    pub workspace_id: Option<WorkspaceId>,
    pub token: String,
    pub workspace_limits: WorkspaceLimits,
}

impl fmt::Debug for ResolvedServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedServerConfig")
            .field("database_path", &self.database_path)
            .field("bind", &self.bind)
            .field("config_path", &self.config_path)
            .field("workspace_id", &self.workspace_id)
            .field("token", &"[REDACTED]")
            .field("workspace_limits", &self.workspace_limits)
            .finish()
    }
}

impl ResolvedServerConfig {
    /// Loads durable state from the CLI/environment-selected config file,
    /// then resolves it with the process-level values and platform defaults.
    pub(crate) fn load(args: CliArgs) -> Result<Self, ConfigError> {
        let paths = ServerPaths::platform_default()?;
        let config_path = args
            .config
            .clone()
            .unwrap_or_else(|| paths.config_dir.join("server.json"));
        let stored = load_state(&config_path)?;
        Self::resolve_with_paths(args, stored, paths)
    }

    pub(crate) fn resolve(
        args: CliArgs,
        stored: Option<StoredServerState>,
    ) -> Result<Self, ConfigError> {
        Self::resolve_with_paths(args, stored, ServerPaths::platform_default()?)
    }

    pub(crate) fn resolve_with_paths(
        args: CliArgs,
        stored: Option<StoredServerState>,
        paths: ServerPaths,
    ) -> Result<Self, ConfigError> {
        let stored = stored.as_ref();
        let database_path = args.db.unwrap_or_else(|| {
            stored.map_or_else(
                || paths.data_dir.join("fslite.db"),
                |state| state.database_path.clone(),
            )
        });
        let bind = args
            .bind
            .or_else(|| stored.map(|state| state.bind))
            .unwrap_or_else(|| {
                DEFAULT_BIND
                    .parse()
                    .expect("the default bind address is valid")
            });
        let config_path = args
            .config
            .unwrap_or_else(|| paths.config_dir.join("server.json"));
        let default_limits = WorkspaceLimits::default();
        let workspace_limits = WorkspaceLimits {
            max_bytes: args
                .max_bytes
                .or_else(|| stored.map(|state| state.workspace_limits.max_bytes))
                .unwrap_or(default_limits.max_bytes),
            max_nodes: args
                .max_nodes
                .or_else(|| stored.map(|state| state.workspace_limits.max_nodes))
                .unwrap_or(default_limits.max_nodes),
            max_file_bytes: args
                .max_file_bytes
                .or_else(|| stored.map(|state| state.workspace_limits.max_file_bytes))
                .unwrap_or(default_limits.max_file_bytes),
        };
        let token = resolve_token(token_from_env(), args.token_file.as_deref(), stored)?;

        Ok(Self {
            database_path,
            bind,
            config_path,
            workspace_id: stored.map(|state| state.workspace_id),
            token,
            workspace_limits,
        })
    }
}

/// Captures a non-empty process token without exposing it in diagnostics.
pub(crate) fn token_from_env() -> Option<String> {
    std::env::var("FSLITE_TOKEN")
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

fn resolve_token(
    process_token: Option<String>,
    token_file: Option<&std::path::Path>,
    stored: Option<&StoredServerState>,
) -> Result<String, ConfigError> {
    if let Some(token) = process_token {
        return Ok(token);
    }
    if let Some(path) = token_file {
        return read_token_file(path);
    }
    let Some(state) = stored else {
        return Ok(generate_token());
    };
    let token = state.token.trim();
    if token.is_empty() {
        return Err(ConfigError::EmptyStoredToken);
    }
    Ok(token.to_owned())
}

#[cfg(test)]
#[allow(unsafe_code)] // Test-only environment mutation is serialized below.
mod tests {
    use std::net::SocketAddr;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::{CliArgs, ResolvedServerConfig, ServerPaths, WorkspaceLimits};
    use fslite_core::WorkspaceId;

    fn stored_state() -> super::StoredServerState {
        super::StoredServerState {
            database_path: "/stored/fslite.db".into(),
            bind: "127.0.0.1:7000".parse().unwrap(),
            workspace_id: WorkspaceId::new(),
            token: "persisted-token".to_owned(),
            workspace_limits: WorkspaceLimits {
                max_bytes: 1,
                max_nodes: 2,
                max_file_bytes: 3,
            },
        }
    }

    fn environment_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct EnvironmentVariableGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvironmentVariableGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: `environment_lock` is held for the test's complete
            // lifetime, including argument parsing and restoration.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvironmentVariableGuard {
        fn drop(&mut self) {
            // SAFETY: `environment_lock` remains held while the guard drops.
            unsafe {
                if let Some(value) = &self.previous {
                    std::env::set_var(self.key, value);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

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

        let resolved = ResolvedServerConfig::resolve(args, Some(stored_state())).unwrap();

        assert_eq!(resolved.database_path, dir.path().join("explicit.db"));
        assert_eq!(resolved.bind.to_string(), "127.0.0.1:9000");
        assert_eq!(resolved.config_path, dir.path().join("explicit.json"));
        assert_eq!(
            resolved.workspace_limits,
            WorkspaceLimits {
                max_bytes: 10,
                max_nodes: 20,
                max_file_bytes: 5,
            }
        );
    }

    #[test]
    fn supplied_values_override_persisted_values_individually() {
        let args = CliArgs {
            db: None,
            bind: Some("127.0.0.1:9001".parse().unwrap()),
            config: None,
            token_file: None,
            max_bytes: None,
            max_nodes: Some(50),
            max_file_bytes: None,
        };

        let resolved = ResolvedServerConfig::resolve(args, Some(stored_state())).unwrap();

        assert_eq!(
            resolved.database_path,
            std::path::PathBuf::from("/stored/fslite.db")
        );
        assert_eq!(
            resolved.bind,
            "127.0.0.1:9001".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(resolved.workspace_limits.max_bytes, 1);
        assert_eq!(resolved.workspace_limits.max_nodes, 50);
        assert_eq!(resolved.workspace_limits.max_file_bytes, 3);
    }

    #[test]
    fn defaults_use_resolved_platform_paths() {
        let paths = ServerPaths {
            data_dir: "/data/fslite".into(),
            config_dir: "/config/fslite".into(),
        };

        let resolved =
            ResolvedServerConfig::resolve_with_paths(CliArgs::default(), None, paths).unwrap();

        assert_eq!(
            resolved.database_path,
            std::path::PathBuf::from("/data/fslite/fslite.db")
        );
        assert_eq!(
            resolved.config_path,
            std::path::PathBuf::from("/config/fslite/server.json")
        );
        assert_eq!(
            resolved.bind,
            "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn invalid_socket_address_is_rejected_by_clap() {
        use clap::Parser;

        let error =
            CliArgs::try_parse_from(["fslite-server", "--bind", "not-an-address"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn explicit_clap_input_overrides_the_environment_value() {
        use clap::Parser;

        let _lock = environment_lock();
        let _environment = EnvironmentVariableGuard::set("FSLITE_BIND", "127.0.0.1:7000");

        let args = CliArgs::try_parse_from(["fslite-server", "--bind", "127.0.0.1:9000"]).unwrap();

        assert_eq!(args.bind.unwrap().to_string(), "127.0.0.1:9000");
    }

    #[test]
    fn defaults_use_the_sqlite_workspace_quota_defaults() {
        let resolved = ResolvedServerConfig::resolve_with_paths(
            CliArgs::default(),
            None,
            ServerPaths {
                data_dir: "/data".into(),
                config_dir: "/config".into(),
            },
        )
        .unwrap();

        assert_eq!(
            resolved.workspace_limits,
            WorkspaceLimits {
                max_bytes: 10 * 1024 * 1024 * 1024,
                max_nodes: 1_000_000,
                max_file_bytes: 1024 * 1024 * 1024,
            }
        );
    }

    #[test]
    fn debug_output_redacts_persisted_and_resolved_tokens() {
        let state = stored_state();
        let resolved = ResolvedServerConfig::resolve_with_paths(
            CliArgs::default(),
            Some(state.clone()),
            ServerPaths {
                data_dir: "/data".into(),
                config_dir: "/config".into(),
            },
        )
        .unwrap();

        assert!(!format!("{state:?}").contains("persisted-token"));
        assert!(!format!("{resolved:?}").contains("persisted-token"));
    }

    #[test]
    fn process_token_has_precedence_over_file_and_persisted_token() {
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("credential");
        std::fs::write(&token_file, "file-token").unwrap();
        let state = stored_state();

        let token = super::resolve_token(
            Some("environment-token".to_owned()),
            Some(&token_file),
            Some(&state),
        )
        .unwrap();

        assert_eq!(token, "environment-token");
    }

    #[test]
    fn load_reads_persisted_state_from_the_resolved_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("server.json");
        let state = stored_state();
        crate::credential_store::save_state(&config, &state).unwrap();
        let args = CliArgs {
            config: Some(config.clone()),
            ..CliArgs::default()
        };

        let resolved = ResolvedServerConfig::load(args).unwrap();

        assert_eq!(resolved.config_path, config);
        assert_eq!(
            resolved.database_path,
            std::path::PathBuf::from("/stored/fslite.db")
        );
        assert_eq!(resolved.workspace_id, Some(state.workspace_id));
    }

    #[test]
    fn resolution_rejects_an_empty_persisted_token_without_leakage() {
        let mut state = stored_state();
        state.token = " \n\t ".to_owned();

        let error = ResolvedServerConfig::resolve_with_paths(
            CliArgs::default(),
            Some(state),
            ServerPaths {
                data_dir: "/data".into(),
                config_dir: "/config".into(),
            },
        )
        .unwrap_err();

        assert!(matches!(error, super::ConfigError::EmptyStoredToken));
        assert!(!error.to_string().contains("token"));
    }
}
