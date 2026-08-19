use std::process::Command;
use std::sync::Arc;

use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

const TOKEN: &str = "cli-remote-e2e-token";

/// Boots a real `fslite-server` in-process on an ephemeral port and returns
/// its base URL plus the workspace id a `fslite --server` invocation
/// should target.
async fn spawn_server() -> (String, fslite_core::WorkspaceId) {
    let sqlite_fs = Arc::new(
        SqliteFileSystem::open_in_memory(Default::default())
            .await
            .unwrap(),
    );
    let workspace = sqlite_fs
        .create_workspace(Default::default())
        .await
        .unwrap();

    let mut tokens = std::collections::HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: fslite_core::RequestContext::trusted(workspace.id).capabilities,
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: sqlite_fs.clone(),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace: workspace.id,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fslite_server::app(state))
            .await
            .unwrap();
    });

    (format!("http://{addr}"), workspace.id)
}

// This test spawns the axum server as a `tokio::spawn`ed task and then
// makes a *blocking* `std::process::Command::output()` call (the real
// `fslite` binary, run as a subprocess) on the same async task. With
// the default current-thread test runtime, that blocking call would starve
// the executor and the spawned server task would never get polled to
// accept/serve the child process's HTTP request — a deadlock, not a real
// bug in `fslite`, `fslite-server`, or the parser/executor stack.
// `flavor = "multi_thread"` gives the server task its own OS thread to run
// on while the blocking `Command::output()` call executes.
#[tokio::test(flavor = "multi_thread")]
async fn cli_remote_mode_matches_local_mode_behavior() {
    let (base_url, workspace_id) = spawn_server().await;

    let write = Command::new(env!("CARGO_BIN_EXE_fslite"))
        .args([
            "--server",
            &base_url,
            "--token",
            TOKEN,
            "--workspace",
            &workspace_id.to_string(),
            "write",
            "/a.txt",
            "--text=hello over http",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let cat = Command::new(env!("CARGO_BIN_EXE_fslite"))
        .args([
            "--server",
            &base_url,
            "--token",
            TOKEN,
            "--workspace",
            &workspace_id.to_string(),
            "cat",
            "/a.txt",
        ])
        .output()
        .unwrap();
    assert!(
        cat.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cat.stderr)
    );
    assert_eq!(
        String::from_utf8(cat.stdout).unwrap().trim(),
        "hello over http"
    );
}

/// Regression test: `--token` on argv is world-readable via
/// `/proc/<pid>/cmdline` on Linux for the process's lifetime and lands in
/// shell history. `FSLITE_TOKEN` must work as an equivalent, so the token
/// never has to appear on argv at all. This runs the CLI with no `--token`
/// flag present, supplying the token only via the environment.
#[tokio::test(flavor = "multi_thread")]
async fn token_can_be_supplied_via_fslite_token_env_var_instead_of_argv() {
    let (base_url, workspace_id) = spawn_server().await;

    let write = Command::new(env!("CARGO_BIN_EXE_fslite"))
        .env("FSLITE_TOKEN", TOKEN)
        .args([
            "--server",
            &base_url,
            "--workspace",
            &workspace_id.to_string(),
            "write",
            "/a.txt",
            "--text=hello from env token",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let cat = Command::new(env!("CARGO_BIN_EXE_fslite"))
        .env("FSLITE_TOKEN", TOKEN)
        .args([
            "--server",
            &base_url,
            "--workspace",
            &workspace_id.to_string(),
            "cat",
            "/a.txt",
        ])
        .output()
        .unwrap();
    assert!(
        cat.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cat.stderr)
    );
    assert_eq!(
        String::from_utf8(cat.stdout).unwrap().trim(),
        "hello from env token"
    );
}
