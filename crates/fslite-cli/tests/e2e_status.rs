use std::process::Command;

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
}

#[test]
fn status_before_bootstrap_reports_no_active_filesystem_without_erroring() {
    let fixture = Fixture::new();
    let output = fixture.cli().arg("status").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No active filesystem yet")
    );
    assert!(!fixture.data.path().join("fslite.db").exists());
}

#[test]
fn status_after_bootstrap_reports_persisted_selection_and_usage() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().arg("status").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Filesystem: default"));
    assert!(stdout.contains("Workspace:  default"));
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Selection:  persisted (context.json)"));
}

#[test]
fn status_with_explicit_filesystem_flag_reports_explicit_selection() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture
        .cli()
        .args([
            "--filesystem",
            "default",
            "--workspace",
            "default",
            "status",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Selection:  explicit (--filesystem)"));
}

#[test]
fn status_json_output_is_well_formed() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().args(["--json", "status"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["filesystem_name"], "default");
    assert_eq!(value["selection"], "persisted");
    assert!(value["usage"]["active_nodes"].is_number());
}

#[test]
fn status_reports_corrupt_context_without_crashing() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::write(fixture.config.path().join("context.json"), "{not-json").unwrap();

    let output = fixture.cli().arg("status").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("fslite doctor"));
}

#[test]
fn status_rejects_raw_db_flag() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    let db = fixture.data.path().join("fslite.db");

    let output = fixture
        .cli()
        .args(["--db", db.to_str().unwrap(), "status"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("registered filesystems only"));
}
