use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;

const FIRST_RUN_MESSAGE: &str =
    "No database or workspace found, creating default database and workspace";

struct RunningServer {
    child: Option<Child>,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
}

impl RunningServer {
    fn start(database_path: &Path, config_path: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-server"))
            .args([
                "--db",
                database_path.to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
                "--bind",
                "127.0.0.1:0",
            ])
            .env_remove("FSLITE_TOKEN")
            .env_remove("FSLITE_TOKEN_FILE")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, stdout_lines) = mpsc::channel();
        std::thread::spawn(move || {
            forward_lines(stdout, sender);
        });
        let (sender, stderr_lines) = mpsc::channel();
        std::thread::spawn(move || {
            forward_lines(stderr, sender);
        });
        Self {
            child: Some(child),
            stdout: stdout_lines,
            stderr: stderr_lines,
        }
    }

    fn wait_for_listening(&mut self) -> (String, Vec<String>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        let mut stderr = Vec::new();
        while Instant::now() < deadline {
            drain_lines(&self.stderr, &mut stderr);
            if let Some(status) = self.child_exit_status() {
                panic!(
                    "server exited before reporting a listening address ({status}); stderr: {}",
                    stderr.join("\n")
                );
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self
                .stdout
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(line) => {
                    if let Some(address) = line.strip_prefix("fslite-server listening on http://") {
                        return (address.to_owned(), output);
                    }
                    output.push(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
            }
        }
        drain_lines(&self.stderr, &mut stderr);
        panic!(
            "server did not report a listening address; stderr: {}",
            stderr.join("\n")
        );
    }

    fn child_exit_status(&mut self) -> Option<ExitStatus> {
        self.child
            .as_mut()
            .and_then(|child| child.try_wait().expect("failed to inspect server process"))
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn forward_lines(reader: impl Read, sender: mpsc::Sender<String>) {
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else {
            break;
        };
        if sender.send(line).is_err() {
            break;
        }
    }
}

fn drain_lines(receiver: &Receiver<String>, output: &mut Vec<String>) {
    output.extend(receiver.try_iter());
}

#[tokio::test]
async fn binary_bootstraps_once_and_reuses_the_persisted_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let database_path = dir.path().join("server.db");
    let config_path = dir.path().join("server.json");

    let mut first = RunningServer::start(&database_path, &config_path);
    let (address, first_output) = first.wait_for_listening();
    assert_eq!(
        first_output
            .iter()
            .filter(|line| line.as_str() == FIRST_RUN_MESSAGE)
            .count(),
        1
    );
    assert_eq!(advertised_address(&first_output), address);
    first.stop();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let workspace_id = state["workspace_id"].as_str().unwrap().to_owned();
    let token = state["token"].as_str().unwrap().to_owned();

    let sqlite = SqliteFileSystem::open(&database_path, Default::default())
        .await
        .unwrap();
    let workspace = fslite_core::WorkspaceId::parse(&workspace_id).unwrap();
    let path = VirtualPath::parse("/persists.txt").unwrap();
    sqlite
        .write(
            &RequestContext::trusted(workspace),
            &path,
            WriteSource::from_bytes(b"persistent file".to_vec()),
            Default::default(),
        )
        .await
        .unwrap();
    drop(sqlite);

    let mut second = RunningServer::start(&database_path, &config_path);
    let (address, second_output) = second.wait_for_listening();
    assert!(
        !second_output
            .iter()
            .any(|line| line.as_str() == FIRST_RUN_MESSAGE)
    );

    let me = identity_response(&address, &token);
    assert_eq!(me["workspace_id"], workspace_id);

    let sqlite = SqliteFileSystem::open(&database_path, Default::default())
        .await
        .unwrap();
    assert!(
        sqlite
            .exists(
                &RequestContext::trusted(workspace),
                &path,
                Default::default()
            )
            .await
            .unwrap()
    );
    second.stop();
}

fn identity_response(address: &str, token: &str) -> serde_json::Value {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(
            format!(
                "GET /v1/me HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 200"));
    serde_json::from_str(body).unwrap()
}

fn advertised_address(output: &[String]) -> String {
    output
        .iter()
        .find_map(|line| {
            line.split_once(" fslite --server http://")
                .and_then(|(_, command)| command.split_once(" --workspace"))
                .map(|(address, _)| address.to_owned())
        })
        .expect("startup did not print a connection command")
}
