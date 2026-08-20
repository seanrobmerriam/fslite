mod credential_store;
mod server_bootstrap;
mod server_config;

use clap::Parser;

use crate::server_config::{CliArgs, ResolvedServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = CliArgs::parse();
    let config = ResolvedServerConfig::load(args)?;
    let boot = server_bootstrap::bootstrap(config).await?;
    if let Some(message) = boot.bootstrap_message() {
        println!("{message}");
    }
    boot.print_connection_guidance();

    let state = boot.app_state();
    let listener = tokio::net::TcpListener::bind(boot.bind).await?;
    println!(
        "fslite-server listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, fslite_server::app(state)).await?;
    Ok(())
}
