use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_core::{Capability, WorkspaceId};
use fslite_server::{app, AppState, AuthenticatedActor, BearerTokenAuthProvider};
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
    for pair in raw.split(',').filter(|s| !s.is_empty()) {
        let Some((token, workspace_id)) = pair.split_once('=') else {
            tracing::warn!(pair, "ignoring malformed FSLITE_TOKENS entry");
            continue;
        };
        let Ok(workspace_id) = WorkspaceId::parse(workspace_id.trim()) else {
            tracing::warn!(pair, "ignoring FSLITE_TOKENS entry with invalid workspace id");
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

    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let health_workspace = WorkspaceId::new();
    let state = AppState {
        fs: Arc::new(fs),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens_from_env())),
        health_workspace,
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("fslite-server listening on {}", listener.local_addr()?);
    axum::serve(listener, app(state)).await?;
    Ok(())
}
