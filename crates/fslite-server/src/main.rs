mod credential_store;
mod server_bootstrap;
mod server_config;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_core::{Capability, FileSystem, WorkspaceId};
use fslite_server::{
    AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin, app,
};
use fslite_sqlite::SqliteFileSystem;

/// Parses `FSLITE_TOKENS`, a comma-separated list of `token=workspace_uuid`
/// pairs, into a bearer-token credential map. Each token authenticates as
/// its workspace with every capability. This is a minimal reference wiring
/// for local/dev use; production deployments should supply their own
/// `AuthProvider`.
fn tokens_from_env() -> HashMap<String, AuthenticatedActor> {
    let mut tokens = HashMap::new();
    let Ok(raw) = std::env::var("FSLITE_TOKENS") else {
        return tokens;
    };
    for (index, pair) in raw.split(',').filter(|s| !s.is_empty()).enumerate() {
        // Never log `pair` (or the token substring within it) — it carries
        // the bearer secret. Only a non-secret identifier (the entry's
        // index, or the parsed-but-invalid workspace-id substring) is safe
        // to write to logs.
        let Some((token, workspace_id)) = pair.split_once('=') else {
            tracing::warn!(index, "ignoring malformed FSLITE_TOKENS entry");
            continue;
        };
        let workspace_id_raw = workspace_id.trim();
        let Ok(workspace_id) = WorkspaceId::parse(workspace_id_raw) else {
            tracing::warn!(
                index,
                workspace_id = workspace_id_raw,
                "ignoring FSLITE_TOKENS entry with invalid workspace id"
            );
            continue;
        };
        tokens.insert(
            token.trim().to_string(),
            AuthenticatedActor {
                workspace_id,
                capabilities: BTreeSet::from([
                    Capability::Read,
                    Capability::Write,
                    Capability::Delete,
                    Capability::TrashRestore,
                    Capability::WorkspaceAdmin,
                ]),
                actor_metadata: Default::default(),
            },
        );
    }
    tokens
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let sqlite_fs = Arc::new(SqliteFileSystem::open_in_memory(Default::default()).await?);
    let health_workspace = WorkspaceId::new();
    let state = AppState {
        fs: sqlite_fs.clone() as Arc<dyn FileSystem>,
        auth: Arc::new(BearerTokenAuthProvider::new(tokens_from_env())),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        health_workspace,
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("fslite-server listening on {}", listener.local_addr()?);
    axum::serve(listener, app(state)).await?;
    Ok(())
}
