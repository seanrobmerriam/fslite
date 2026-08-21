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
fn doctor_before_bootstrap_passes_every_check() {
    let fixture = Fixture::new();
    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0 problems found."));
}

#[test]
fn doctor_after_bootstrap_passes_every_check() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("database exists"));
    assert!(stdout.contains("integrity check"));
    assert!(stdout.contains("workspace exists"));
    assert!(stdout.contains("0 problems found."));
}

#[test]
fn doctor_reports_corrupt_context_as_a_failure_and_exits_non_zero() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::write(fixture.config.path().join("context.json"), "{not-json").unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} global: context.json"));
    assert!(stdout.contains("1 problem found."));
}

#[test]
fn doctor_reports_a_missing_database_file_as_a_failure() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    std::fs::remove_file(fixture.data.path().join("fslite.db")).unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} default: database exists"));
}

#[test]
fn doctor_reports_a_stale_registered_workspace_as_a_failure() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let registry_path = fixture.config.path().join("registry.json");
    let mut registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&registry_path).unwrap()).unwrap();
    registry["workspaces"]["default"]["ghost"] =
        serde_json::Value::String(fslite_core::WorkspaceId::new().to_string());
    std::fs::write(&registry_path, serde_json::to_string(&registry).unwrap()).unwrap();

    let output = fixture.cli().arg("doctor").output().unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{2717} default: workspace exists (\"ghost\""));
}

#[test]
fn doctor_json_output_is_well_formed() {
    let fixture = Fixture::new();
    fixture.cli().args(["mkdir", "docs"]).output().unwrap();

    let output = fixture.cli().args(["--json", "doctor"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let checks = value.as_array().unwrap();
    assert!(!checks.is_empty());
    assert!(
        checks
            .iter()
            .all(|check| check["status"] == "pass" || check["status"] == "warn")
    );
}
