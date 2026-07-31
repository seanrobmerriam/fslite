use std::process::Command;

/// Every test in this file gets its own isolated `FSLITE_CONFIG_DIR` (a
/// fresh temp directory) so `create`/`delete`/`use`'s registry/context
/// state never leaks between tests or touches a real `$HOME`.
struct Fixture {
    config_dir: tempfile::TempDir,
    db_dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            config_dir: tempfile::tempdir().unwrap(),
            db_dir: tempfile::tempdir().unwrap(),
        }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fslite"));
        command.env("FSLITE_CONFIG_DIR", self.config_dir.path());
        command
    }

    fn db_path(&self, name: &str) -> String {
        self.db_dir.path().join(name).to_str().unwrap().to_string()
    }
}

#[test]
fn create_use_and_verb_dispatch_end_to_end() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");

    let create = fixture
        .cli()
        .args([
            "create",
            "filesystem-main",
            "-f",
            &db_path,
            "-w",
            "workspace-main",
        ])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let create_stdout = String::from_utf8(create.stdout).unwrap();
    assert!(create_stdout.contains("filesystem-main"));
    assert!(create_stdout.contains("workspace-main"));

    let use_cmd = fixture
        .cli()
        .args(["use", "filesystem-main", "-w", "workspace-main"])
        .output()
        .unwrap();
    assert!(
        use_cmd.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&use_cmd.stderr)
    );

    let mkdir = fixture.cli().args(["mkdir", "/docs"]).output().unwrap();
    assert!(
        mkdir.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&mkdir.stderr)
    );

    let write = fixture
        .cli()
        .args(["write", "/docs/a.txt", "--text=hello context"])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let cat = fixture.cli().args(["cat", "/docs/a.txt"]).output().unwrap();
    assert!(cat.status.success());
    assert_eq!(
        String::from_utf8(cat.stdout).unwrap().trim(),
        "hello context"
    );
}

#[test]
fn create_refuses_a_duplicate_name() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");

    let first = fixture
        .cli()
        .args(["create", "filesystem-main", "-f", &db_path])
        .output()
        .unwrap();
    assert!(first.status.success());

    let second_db_path = fixture.db_path("other.db");
    let second = fixture
        .cli()
        .args(["create", "filesystem-main", "-f", &second_db_path])
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already registered"));
}

#[test]
fn create_refuses_to_overwrite_an_existing_file() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    std::fs::write(&db_path, b"not a real database").unwrap();

    let create = fixture
        .cli()
        .args(["create", "filesystem-main", "-f", &db_path])
        .output()
        .unwrap();
    assert!(!create.status.success());
    assert!(String::from_utf8_lossy(&create.stderr).contains("already exists"));
}

#[test]
fn delete_without_yes_requires_typed_confirmation() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    fixture
        .cli()
        .args(["create", "filesystem-main", "-f", &db_path])
        .output()
        .unwrap();

    let mut delete = fixture
        .cli()
        .args(["delete", "filesystem-main"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    delete
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"wrong-name\n")
        .unwrap();
    let output = delete.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        std::path::Path::new(&db_path).exists(),
        "file must survive a mismatched confirmation"
    );
}

#[test]
fn delete_yes_skips_confirmation_and_removes_the_file() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    fixture
        .cli()
        .args(["create", "filesystem-main", "-f", &db_path])
        .output()
        .unwrap();

    let delete = fixture
        .cli()
        .args(["delete", "filesystem-main", "-y"])
        .output()
        .unwrap();
    assert!(
        delete.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&delete.stderr)
    );
    assert!(!std::path::Path::new(&db_path).exists());
}

#[test]
fn delete_clears_a_matching_context_so_later_verbs_fail_clearly() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    fixture
        .cli()
        .args([
            "create",
            "filesystem-main",
            "-f",
            &db_path,
            "-w",
            "workspace-main",
        ])
        .output()
        .unwrap();
    fixture
        .cli()
        .args(["use", "filesystem-main", "-w", "workspace-main"])
        .output()
        .unwrap();
    fixture
        .cli()
        .args(["delete", "filesystem-main", "-y"])
        .output()
        .unwrap();

    let mkdir = fixture.cli().args(["mkdir", "/docs"]).output().unwrap();
    assert!(!mkdir.status.success());
    assert!(
        String::from_utf8_lossy(&mkdir.stderr).contains("no filesystem selected"),
        "stderr: {}",
        String::from_utf8_lossy(&mkdir.stderr)
    );
}

#[test]
fn explicit_db_and_workspace_flags_bypass_context_entirely() {
    // Regression guard for the plan's core backward-compatibility claim:
    // an explicit --db/--workspace invocation must work identically
    // whether or not any context/registry state exists at all.
    let fixture = Fixture::new();
    let db_path = fixture.db_path("standalone.db");

    let create = fixture
        .cli()
        .args(["--db", &db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mkdir = fixture
        .cli()
        .args([
            "--db",
            &db_path,
            "--workspace",
            &workspace_id,
            "mkdir",
            "/docs",
        ])
        .output()
        .unwrap();
    assert!(
        mkdir.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&mkdir.stderr)
    );
}
