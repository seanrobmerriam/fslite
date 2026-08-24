//! Runs `fslite-server`'s HTTP API in-process and drives it with
//! `fslite-command`'s `RemoteExecutor` — the same executor `fslite-cli`
//! uses in `--server` mode — over a real TCP connection.
//!
//! Run with `cargo run -p fslite-server --example server_and_remote_cli`.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fslite_command::{Command, Executor, RemoteExecutor, render_human};
use fslite_core::{Capability, FileSystem, RequestContext, VirtualPath};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

const TOKEN: &str = "example-token";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the workspace first, so the token below can be bound to a
    // workspace id that already exists — see this example's module doc.
    let sqlite_fs = Arc::new(SqliteFileSystem::open_in_memory(Default::default()).await?);
    let workspace = sqlite_fs.create_workspace(Default::default()).await?;

    let mut tokens = HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
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

    let state = AppState {
        fs: sqlite_fs.clone() as Arc<dyn FileSystem>,
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        health_workspace: workspace.id,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, fslite_server::app(state))
            .await
            .expect("server task failed");
    });

    let executor = RemoteExecutor::new(format!("http://{addr}"), TOKEN);
    let ctx = RequestContext::trusted(workspace.id);

    executor
        .execute(
            &ctx,
            Command::Mkdir {
                path: VirtualPath::parse("/docs")?,
                options: Default::default(),
            },
        )
        .await?;

    let write_output = executor
        .execute(
            &ctx,
            Command::Write {
                path: VirtualPath::parse("/docs/hello.txt")?,
                bytes: b"hello over HTTP".to_vec(),
                options: Default::default(),
            },
        )
        .await?;
    println!("write: {}", render_human(&write_output));

    let read_output = executor
        .execute(
            &ctx,
            Command::Read {
                path: VirtualPath::parse("/docs/hello.txt")?,
                options: Default::default(),
            },
        )
        .await?;
    println!("read: {}", render_human(&read_output));

    let ls_output = executor
        .execute(
            &ctx,
            Command::ReadDir {
                path: VirtualPath::parse("/docs")?,
                page: Default::default(),
            },
        )
        .await?;
    println!("ls: {}", render_human(&ls_output));

    Ok(())
}
