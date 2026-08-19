use std::process::Command;

const NOTICE: &str =
    "No database or workspace found, creating default database and workspace\n";

struct Fixture {
    config: tempfile::TempDir,
    data: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
        }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fslite"));
        command
            .env("FSLITE_CONFIG_DIR", self.config.path())
            .env("FSLITE_DATA_DIR", self.data.path());
        command
    }

    fn default_db(&self) -> std::path::PathBuf {
        self.data.path().join("fslite.db")
    }
}

#[test]
fn first_verb_bootstraps_once_and_keeps_stdout_clean() {
    let fixture = Fixture::new();
    let first = fixture.cli().args(["mkdir", "/docs"]).output().unwrap();
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(String::from_utf8(first.stderr).unwrap(), NOTICE);
    assert!(!String::from_utf8(first.stdout).unwrap().contains(NOTICE));
    assert!(fixture.default_db().exists());

    let second = fixture.cli().args(["ls", "/"]).output().unwrap();
    assert!(second.status.success());
    assert!(second.stderr.is_empty());
    assert!(String::from_utf8(second.stdout).unwrap().contains("docs"));
}

#[test]
fn cat_and_json_stdout_remain_machine_safe() {
    let fixture = Fixture::new();
    fixture
        .cli()
        .args(["write", "/hello.txt", "--text=hello"])
        .output()
        .unwrap();

    let cat = fixture.cli().args(["cat", "/hello.txt"]).output().unwrap();
    assert!(cat.status.success());
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello");
    assert!(cat.stderr.is_empty());

    let json = fixture.cli().args(["--json", "usage"]).output().unwrap();
    assert!(json.status.success());
    let stdout = String::from_utf8(json.stdout).unwrap();
    serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap();
    assert!(json.stderr.is_empty());
}

#[test]
fn discovery_and_explicit_targets_do_not_bootstrap() {
    let fixture = Fixture::new();
    let help = fixture.cli().arg("help").output().unwrap();
    assert!(help.status.success());
    assert!(!fixture.default_db().exists());

    let explicit_db = fixture.data.path().join("explicit.db");
    let created = fixture
        .cli()
        .args([
            "--db",
            explicit_db.to_str().unwrap(),
            "--create-workspace",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    assert!(explicit_db.exists());
    assert!(!fixture.default_db().exists());
}

#[test]
fn corrupt_registry_fails_without_replacing_user_state() {
    let fixture = Fixture::new();
    let registry = fixture.config.path().join("registry.json");
    std::fs::write(&registry, "{not-json").unwrap();

    let output = fixture.cli().args(["mkdir", "/docs"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("registry"));
    assert_eq!(std::fs::read_to_string(registry).unwrap(), "{not-json");
    assert!(!fixture.default_db().exists());
}

#[test]
fn simultaneous_first_commands_converge_on_one_workspace() {
    let fixture = Fixture::new();
    let first = fixture
        .cli()
        .arg("usage")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let second = fixture
        .cli()
        .arg("usage")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert!(first.status.success());
    assert!(second.status.success());
    let notices = [first.stderr, second.stderr]
        .iter()
        .filter(|stderr| stderr.as_slice() == NOTICE.as_bytes())
        .count();
    assert_eq!(notices, 1);

    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture.config.path().join("registry.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        registry["workspaces"]["default"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert!(registry["workspaces"]["default"]["default"].is_string());

}
