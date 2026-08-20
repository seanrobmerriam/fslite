#![allow(dead_code)] // Task 5 persists and loads state through these helpers.

use std::io::Write;
use std::path::Path;

use atomic_write_file::AtomicWriteFile;

use crate::server_config::{ConfigError, StoredServerState};

pub(crate) fn load_state(path: &Path) -> Result<Option<StoredServerState>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn save_state(path: &Path, state: &StoredServerState) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(state)?;
    let mut file = AtomicWriteFile::open(path)?;
    #[cfg(unix)]
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&json)?;
    file.sync_all()?;
    file.commit()?;
    Ok(())
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

#[cfg(test)]
mod tests {
    use super::{generate_token, load_state, read_token_file, save_state};
    use crate::server_config::{StoredServerState, WorkspaceLimits};
    use fslite_core::WorkspaceId;

    fn fixture_state() -> StoredServerState {
        StoredServerState {
            database_path: "/tmp/fslite.db".into(),
            bind: "127.0.0.1:8080".parse().unwrap(),
            workspace_id: WorkspaceId::new(),
            token: "top-secret-token".to_owned(),
            workspace_limits: WorkspaceLimits {
                max_bytes: 100,
                max_nodes: 20,
                max_file_bytes: 10,
            },
        }
    }

    #[test]
    fn state_round_trips_as_pretty_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.json");
        let state = fixture_state();

        save_state(&path, &state).unwrap();

        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\n  \"database_path\"")
        );
        let loaded = load_state(&path).unwrap().unwrap();
        assert_eq!(loaded.database_path, state.database_path);
        assert_eq!(loaded.bind, state.bind);
        assert_eq!(loaded.workspace_id, state.workspace_id);
        assert_eq!(loaded.token, state.token);
        assert_eq!(loaded.workspace_limits, state.workspace_limits);
    }

    #[test]
    fn save_atomically_replaces_invalid_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.json");
        std::fs::write(&path, "not JSON").unwrap();

        save_state(&path, &fixture_state()).unwrap();

        assert!(load_state(&path).unwrap().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn saved_state_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.json");
        save_state(&path, &fixture_state()).unwrap();

        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn empty_token_file_is_a_typed_error_without_token_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, " \n\t ").unwrap();

        let error = read_token_file(&path).unwrap_err();

        assert!(
            matches!(error, crate::server_config::ConfigError::EmptyTokenFile(ref found) if *found == path)
        );
        assert!(!error.to_string().contains("token"));
    }

    #[test]
    fn token_file_is_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential");
        std::fs::write(&path, "\n  secret-value\t").unwrap();

        assert_eq!(read_token_file(&path).unwrap(), "secret-value");
    }

    #[test]
    fn generated_token_is_two_uuid_v4_values_without_separators() {
        let token = generate_token();

        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn missing_state_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            load_state(&dir.path().join("missing.json"))
                .unwrap()
                .is_none()
        );
    }
}
