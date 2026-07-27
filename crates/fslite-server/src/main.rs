use std::sync::Arc;

use fslite_core::WorkspaceId;
use fslite_server::{app, AppState};
use fslite_sqlite::SqliteFileSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
    let health_workspace = WorkspaceId::new();
    let state = AppState {
        fs: Arc::new(fs),
        health_workspace,
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("fslite-server listening on {}", listener.local_addr()?);
    axum::serve(listener, app(state)).await?;
    Ok(())
}
