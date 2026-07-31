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

#[test]
fn explicit_filesystem_and_workspace_names_bypass_the_persisted_context() {
    // Regression test for the finding where `resolve_workspace` fell back
    // to the *persisted context's* workspace name even when the
    // filesystem name came from an explicit `--filesystem` override on
    // this invocation, silently pairing the wrong workspace with the
    // right filesystem. Creates two named filesystems with different
    // workspaces, `use`s one, then writes through an explicit
    // `--filesystem <other> --workspace <other-workspace-name>` override
    // and confirms it landed in the *other* filesystem, not the one set
    // via `use`.
    let fixture = Fixture::new();
    let db_path1 = fixture.db_path("fs1.db");
    let db_path2 = fixture.db_path("fs2.db");

    let create1 = fixture
        .cli()
        .args(["create", "fs1", "-f", &db_path1, "-w", "ws1"])
        .output()
        .unwrap();
    assert!(
        create1.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create1.stderr)
    );

    let create2 = fixture
        .cli()
        .args(["create", "fs2", "-f", &db_path2, "-w", "ws2"])
        .output()
        .unwrap();
    assert!(
        create2.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create2.stderr)
    );

    let use_cmd = fixture
        .cli()
        .args(["use", "fs1", "-w", "ws1"])
        .output()
        .unwrap();
    assert!(
        use_cmd.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&use_cmd.stderr)
    );

    // Write via the explicit override — must land in fs2/ws2, not the
    // `use`'d fs1/ws1.
    let write = fixture
        .cli()
        .args([
            "--filesystem",
            "fs2",
            "--workspace",
            "ws2",
            "write",
            "/only-in-fs2.txt",
            "--text=override worked",
        ])
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );

    // Reading back through the same override sees it.
    let cat_override = fixture
        .cli()
        .args([
            "--filesystem",
            "fs2",
            "--workspace",
            "ws2",
            "cat",
            "/only-in-fs2.txt",
        ])
        .output()
        .unwrap();
    assert!(
        cat_override.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cat_override.stderr)
    );
    assert_eq!(
        String::from_utf8(cat_override.stdout).unwrap().trim(),
        "override worked"
    );

    // The `use`'d filesystem (fs1/ws1, reached via the persisted context,
    // no override flags) does not have the file.
    let cat_context = fixture
        .cli()
        .args(["cat", "/only-in-fs2.txt"])
        .output()
        .unwrap();
    assert!(
        !cat_context.status.success(),
        "expected fs1 (the `use`'d filesystem) to not contain the file written via the fs2 override"
    );
}

#[test]
fn explicit_filesystem_without_explicit_workspace_and_a_different_persisted_context_errors_clearly()
{
    // Regression test for the exact scenario the finding described:
    // `use fs1 -w ws1`, then `--filesystem fs2` alone (no --workspace).
    // Before the fix this silently resolved workspace `ws1` (from the
    // persisted context) against `fs2` (the explicit override), producing
    // a misleading "the active workspace ... is no longer registered"
    // error that sounded like data was deleted. After the fix, this must
    // be a clear, distinct error telling the user --workspace is required
    // alongside an explicit --filesystem.
    let fixture = Fixture::new();
    let db_path1 = fixture.db_path("fs1.db");
    let db_path2 = fixture.db_path("fs2.db");

    fixture
        .cli()
        .args(["create", "fs1", "-f", &db_path1, "-w", "ws1"])
        .output()
        .unwrap();
    fixture
        .cli()
        .args(["create", "fs2", "-f", &db_path2, "-w", "ws2"])
        .output()
        .unwrap();
    fixture
        .cli()
        .args(["use", "fs1", "-w", "ws1"])
        .output()
        .unwrap();

    let ls = fixture
        .cli()
        .args(["--filesystem", "fs2", "ls", "/"])
        .output()
        .unwrap();
    assert!(
        !ls.status.success(),
        "expected --filesystem without --workspace to fail, not silently resolve ws1 against fs2"
    );
    let stderr = String::from_utf8_lossy(&ls.stderr);
    assert!(
        !stderr.contains("is no longer registered"),
        "got the old misleading error text, expected the new clear one: {stderr}"
    );
    assert!(
        stderr.contains("--filesystem") && stderr.contains("fs2") && stderr.contains("--workspace"),
        "expected a clear error naming --filesystem, fs2, and --workspace, got: {stderr}"
    );
}

/// Regression test for the finding that `create_filesystem`/
/// `delete_filesystem` printed raw, unescaped `.display()`/`.join()`
/// output to stdout while every other user-controlled string in this file
/// already went through `{name:?}` (Debug, which escapes control bytes).
/// A filesystem name containing a raw ESC byte therefore reached the
/// CLI's own stdout unsanitized via `create`'s "created filesystem ... at
/// <path>" line and `delete`'s confirmation prompt. Mirrors the pattern in
/// `e2e_local.rs`'s `one_shot_mode_never_writes_a_raw_escape_byte_to_stderr_on_error`.
#[test]
fn create_and_delete_never_write_a_raw_escape_byte_to_stdout() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    let hostile_name = "fs\x1b[31mFAKE\x1b[0mname";

    let create = fixture
        .cli()
        .args(["create", hostile_name, "-f", &db_path, "-w", "ws1"])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    assert!(
        !create.stdout.contains(&0x1b),
        "raw ESC byte reached stdout on create: {:?}",
        String::from_utf8_lossy(&create.stdout)
    );

    let delete = fixture
        .cli()
        .args(["delete", hostile_name, "-y"])
        .output()
        .unwrap();
    assert!(
        delete.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&delete.stderr)
    );
    assert!(
        !delete.stdout.contains(&0x1b),
        "raw ESC byte reached stdout on delete: {:?}",
        String::from_utf8_lossy(&delete.stdout)
    );
}

/// Sibling of `create_and_delete_never_write_a_raw_escape_byte_to_stdout`
/// specifically exercising `delete`'s interactive confirmation prompt
/// (the `-y`-skipped codepath), which prints the registered *workspace
/// names* via `.join(", ")` — the other unsanitized site the finding
/// called out.
#[test]
fn delete_confirmation_prompt_never_writes_a_raw_escape_byte_to_stdout() {
    let fixture = Fixture::new();
    let db_path = fixture.db_path("main.db");
    let hostile_workspace_name = "ws\x1b[31mFAKE\x1b[0mname";

    let create = fixture
        .cli()
        .args([
            "create",
            "filesystem-main",
            "-f",
            &db_path,
            "-w",
            hostile_workspace_name,
        ])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );

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
        .write_all(b"filesystem-main\n")
        .unwrap();
    let output = delete.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "raw ESC byte reached stdout from delete's confirmation prompt: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
