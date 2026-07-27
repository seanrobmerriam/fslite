use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
}

#[test]
fn create_workspace_then_mkdir_write_cat_via_local_mode() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();

    let create = cli().args(["--db", db_path, "--create-workspace"]).output().unwrap();
    assert!(create.status.success(), "stderr: {}", String::from_utf8_lossy(&create.stderr));
    let stdout = String::from_utf8(create.stdout).unwrap();
    let workspace_id = stdout.trim();
    assert!(!workspace_id.is_empty());

    let mkdir = cli().args(["--db", db_path, "--workspace", workspace_id, "mkdir", "/docs"]).output().unwrap();
    assert!(mkdir.status.success(), "stderr: {}", String::from_utf8_lossy(&mkdir.stderr));

    let write = cli()
        .args(["--db", db_path, "--workspace", workspace_id, "write", "/docs/a.txt", "--text=hello cli"])
        .output()
        .unwrap();
    assert!(write.status.success(), "stderr: {}", String::from_utf8_lossy(&write.stderr));

    let cat = cli().args(["--db", db_path, "--workspace", workspace_id, "cat", "/docs/a.txt"]).output().unwrap();
    assert!(cat.status.success());
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello cli");

    let rm = cli().args(["--db", db_path, "--workspace", workspace_id, "rm", "/docs/a.txt"]).output().unwrap();
    assert!(rm.status.success());

    let stat_after_rm = cli().args(["--db", db_path, "--workspace", workspace_id, "stat", "/docs/a.txt"]).output().unwrap();
    assert!(!stat_after_rm.status.success());
}

#[test]
fn json_flag_prints_machine_readable_output() {
    // Single process pair sharing one `--db` file: creation happens in one
    // invocation, and the `--json` query happens in a second invocation
    // against the same persisted database, so the workspace created by the
    // first process is visible to the second.
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli().args(["--db", db_path, "--create-workspace"]).output().unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let usage = cli()
        .args(["--db", db_path, "--workspace", &workspace_id, "--json", "usage"])
        .output()
        .unwrap();
    assert!(usage.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert!(parsed["usage"]["active_nodes"].is_number());
}
