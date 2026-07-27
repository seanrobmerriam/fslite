use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
}

#[test]
fn create_workspace_then_mkdir_write_cat_via_local_mode() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();

    let create = cli()
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let stdout = String::from_utf8(create.stdout).unwrap();
    let workspace_id = stdout.trim();
    assert!(!workspace_id.is_empty());

    let mkdir = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            workspace_id,
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

    let write = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            workspace_id,
            "write",
            "/docs/a.txt",
            "--text=hello cli",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    let cat = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            workspace_id,
            "cat",
            "/docs/a.txt",
        ])
        .output()
        .unwrap();
    assert!(cat.status.success());
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello cli");

    let rm = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            workspace_id,
            "rm",
            "/docs/a.txt",
        ])
        .output()
        .unwrap();
    assert!(rm.status.success());

    let stat_after_rm = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            workspace_id,
            "stat",
            "/docs/a.txt",
        ])
        .output()
        .unwrap();
    assert!(!stat_after_rm.status.success());
}

/// Regression test for the live-verified ANSI/OSC injection finding: a node
/// name/path can legally contain the ESC byte (`VirtualPath::parse` only
/// rejects NUL and a missing leading `/`), and `fslite-sqlite` embeds the
/// raw path text verbatim in error messages like `FsError::already_exists`.
/// Before the fix, `run_line`'s `eprintln!("error: {} ...", err.message(),
/// ...)` used `Display` formatting, writing that raw ESC byte (and here, a
/// full ANSI color-reset sequence) straight to the real terminal. This
/// creates a node with a hostile, ESC-laden name via local mode, triggers a
/// real domain error against it (a duplicate `mkdir`), captures the CLI's
/// actual compiled-binary stderr, and asserts no raw `\x1b` byte survived.
#[test]
fn one_shot_mode_never_writes_a_raw_escape_byte_to_stderr_on_error() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli()
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let hostile_path = "/evil\x1b[31mFAKE\x1b[0m.txt";

    let first = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            &workspace_id,
            "mkdir",
            hostile_path,
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    // A second `mkdir` of the same path fails with `AlreadyExists`, whose
    // `FsError` message embeds the raw hostile path text.
    let second = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            &workspace_id,
            "mkdir",
            hostile_path,
        ])
        .output()
        .unwrap();
    assert!(
        !second.status.success(),
        "expected the duplicate mkdir to fail"
    );
    assert!(
        !second.stderr.contains(&0x1b),
        "raw ESC byte reached stderr: {:?}",
        String::from_utf8_lossy(&second.stderr)
    );
    let stderr_text = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr_text.contains("evil") && stderr_text.contains("FAKE"),
        "expected the sanitized message to retain the benign surrounding text, got: {stderr_text}"
    );
}

/// Regression test for the live-verified row-forging finding: writing a
/// file named `a.txt\nfile          999 IMPORTANT-SYSTEM-FILE.txt` and then
/// running `ls` used to produce a completely fabricated extra row in the
/// listing that looked like a real file, because the renderer's node-name
/// sanitizer preserved `\n` (correct for free-text fields, wrong for a
/// structured field like a name). Asserts the real `ls` output has exactly
/// one line per node actually written (two), not three.
#[test]
fn ls_does_not_forge_an_extra_row_from_a_newline_in_a_node_name() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli()
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            &workspace_id,
            "write",
            "/legit.txt",
            "--text=x",
        ])
        .output()
        .unwrap();

    let hostile_name = "/a.txt\nfile          999 IMPORTANT-SYSTEM-FILE.txt";
    let write_hostile = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            &workspace_id,
            "write",
            hostile_name,
            "--text=y",
        ])
        .output()
        .unwrap();
    assert!(
        write_hostile.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write_hostile.stderr)
    );

    let ls = cli()
        .args(["--db", db_path, "--workspace", &workspace_id, "ls", "/"])
        .output()
        .unwrap();
    assert!(
        ls.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ls.stderr)
    );
    let stdout = String::from_utf8(ls.stdout).unwrap();
    // Two nodes were written, so a correct listing has exactly two lines —
    // a forged third line would mean the embedded `\n` leaked through.
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "ls output had a forged extra row: {stdout:?}"
    );
}

#[test]
fn json_flag_prints_machine_readable_output() {
    // Single process pair sharing one `--db` file: creation happens in one
    // invocation, and the `--json` query happens in a second invocation
    // against the same persisted database, so the workspace created by the
    // first process is visible to the second.
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli()
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let usage = cli()
        .args([
            "--db",
            db_path,
            "--workspace",
            &workspace_id,
            "--json",
            "usage",
        ])
        .output()
        .unwrap();
    assert!(usage.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert!(parsed["usage"]["active_nodes"].is_number());
}
