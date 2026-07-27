use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn repl_mode_executes_piped_commands_line_by_line() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();

    let create = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--workspace", &workspace_id, "--repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "mkdir /docs").unwrap();
        writeln!(stdin, "write /docs/a.txt --text=\"from repl\"").unwrap();
        writeln!(stdin, "cat /docs/a.txt").unwrap();
        writeln!(stdin, "exit").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("from repl"), "stdout was: {stdout}");
}

/// REPL companion to `e2e_local.rs`'s
/// `one_shot_mode_never_writes_a_raw_escape_byte_to_stderr_on_error` — the
/// REPL is the worse case for this finding, since it is a persistent
/// interactive session where the injected escape sequence could do more
/// (e.g. set the terminal title, or worse) across many further prompts.
/// Same repro: a hostile ESC-laden path triggers a real `AlreadyExists`
/// domain error, and the raw ESC byte must never reach the REPL's stderr.
#[test]
fn repl_mode_never_writes_a_raw_escape_byte_to_stderr_on_error() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--workspace", &workspace_id, "--repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        // Single-quoted so the lexer accepts the embedded ESC byte and
        // space as one literal path argument (see `read_single_quoted`,
        // which preserves quoted content byte-for-byte with no special
        // handling of control bytes).
        writeln!(stdin, "mkdir '/evil\x1b[31mFAKE ERROR\x1b[0m.txt'").unwrap();
        writeln!(stdin, "mkdir '/evil\x1b[31mFAKE ERROR\x1b[0m.txt'").unwrap();
        writeln!(stdin, "exit").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        !output.stderr.contains(&0x1b),
        "raw ESC byte reached REPL stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr_text.contains("evil") && stderr_text.contains("FAKE"),
        "expected an error mentioning the hostile path's benign text, got: {stderr_text}"
    );
}

#[test]
fn repl_mode_reports_parse_errors_on_stderr_without_exiting() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--workspace", &workspace_id, "--repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "ls /a | rm /b").unwrap(); // rejected metacharacter
        writeln!(stdin, "usage").unwrap(); // proves the REPL kept running
        writeln!(stdin, "exit").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("parse error"), "stderr was: {stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("active"), "stdout was: {stdout}"); // from `usage`'s human rendering
}
