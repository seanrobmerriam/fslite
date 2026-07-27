# CLI Residual Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two narrow residuals parked (not fixed) during the `fslite-command-cli` plan's final whole-branch review: `fslite-cli --help` printing the `FSLITE_TOKEN` value in plain text, and the CLI's error-printing code still allowing a hostile filename to forge a fake extra stderr line via an embedded newline.

**Architecture:** Both fixes live entirely in `fslite-cli`'s `src/main.rs` (one `clap` attribute, one function-name swap) plus new regression tests in its existing `tests/e2e_local.rs`. No production code outside `fslite-cli` changes; `fslite-command`'s `sanitize_name` function (the stricter of its two sanitizers) already exists and is already used everywhere else in the renderer — this plan only makes `main.rs`'s two error-printing call sites use it too.

**Tech Stack:** Rust, `clap` (derive), the existing `fslite-command`/`fslite-cli` crates and their established end-to-end test pattern (spawn the compiled binary via `std::process::Command`, inspect real stdout/stderr).

## Global Constraints

- Do not modify `fslite-core`, `fslite-sqlite`, `fslite-server`, or `fslite-command`. Everything needed (the stricter `sanitize_name` sanitizer, the `env = "FSLITE_TOKEN"` clap wiring) already exists; this plan only changes how `fslite-cli` calls into it.
- Every step must produce a real, runnable, git-committed change — no `TODO`s, no follow-up plans deferred further. These are the last two known issues from the prior plan's final review; there is no next review to catch anything left undone here.
- Keep the repo green throughout: `cargo fmt -p fslite-cli --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` must all pass after each task's commit.
- Match the existing test style in `crates/fslite-cli/tests/e2e_local.rs` exactly: spawn the real compiled binary via `Command::new(env!("CARGO_BIN_EXE_fslite-cli"))` (the `cli()` helper already defined at the top of that file), assert on real captured stdout/stderr — never call `fslite_command`/`fslite-cli` internals directly as a shortcut.

---

## Task 1: Stop `--help` from printing the `FSLITE_TOKEN` value

**Files:**
- Modify: `crates/fslite-cli/src/main.rs:33-38` (the `token` field's `#[arg(...)]` attribute)
- Test: `crates/fslite-cli/tests/e2e_local.rs` (new test, appended to the existing file)

**Interfaces:**
- Consumes: nothing new — `clap`'s `env` feature is already enabled in the root `Cargo.toml`'s `[workspace.dependencies]` (`clap = { version = "4", features = ["derive", "env"] }`); `hide_env_values` is a plain `#[arg(...)]` attribute clap's derive macro already supports with that feature set, no `Cargo.toml` change needed.
- Produces: nothing new is exposed — this only changes `--help`'s rendered output for the existing `--token` flag.

The current field, exactly as it stands today:

```rust
    /// Bearer token for remote mode. Prefer `FSLITE_TOKEN` over this flag:
    /// on Linux, argv (and therefore a flag value) is world-readable via
    /// `/proc/<pid>/cmdline` for the process's lifetime and also lands in
    /// shell history, while an environment variable does not.
    #[arg(long, env = "FSLITE_TOKEN", requires = "server")]
    token: Option<String>,
```

`clap`'s derive macro renders an env-sourced argument's *current value* into `--help` output by default (e.g. `[env: FSLITE_TOKEN=s3cr3t-live-token]`) — which defeats the entire point of preferring `FSLITE_TOKEN` over `--token` in the first place, since a user is far more likely to casually paste `--help` output (into a bug report, a terminal recording, a chat message) than argv or `/proc`.

- [ ] **Step 1: Write the failing test**

Append to `crates/fslite-cli/tests/e2e_local.rs` (the file already has `use std::process::Command;` and a `cli()` helper at the top — no new imports needed):

```rust
/// Regression test: `clap`'s default `env` rendering prints the *value* of
/// an env-sourced argument in `--help` output. Since `FSLITE_TOKEN` exists
/// specifically to keep the bearer token off argv and out of shell
/// history, having `--help` echo it right back in plain text would defeat
/// the whole point — `--help` output gets pasted into bug reports and
/// terminal recordings far more casually than anyone would share their
/// shell history. Sets a real, distinctive token via `FSLITE_TOKEN` and
/// asserts it never appears in `--help`'s stdout, while the flag and its
/// env var name still do (i.e. this isn't testing that `--help` stopped
/// documenting the flag at all).
#[test]
fn help_does_not_print_the_fslite_token_value() {
    let output = cli()
        .env("FSLITE_TOKEN", "s3cr3t-do-not-print-me")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("s3cr3t-do-not-print-me"),
        "--help printed the FSLITE_TOKEN value: {stdout}"
    );
    assert!(
        stdout.contains("--token") && stdout.contains("FSLITE_TOKEN"),
        "expected --help to still document the --token flag and its env var, got: {stdout}"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p fslite-cli --test e2e_local help_does_not_print_the_fslite_token_value`
Expected: FAIL — the assertion `!stdout.contains("s3cr3t-do-not-print-me")` fails, because `--help` currently prints `[env: FSLITE_TOKEN=s3cr3t-do-not-print-me]`.

- [ ] **Step 3: Write the fix**

In `crates/fslite-cli/src/main.rs`, add `hide_env_values = true` to the `token` field's attribute:

```rust
    /// Bearer token for remote mode. Prefer `FSLITE_TOKEN` over this flag:
    /// on Linux, argv (and therefore a flag value) is world-readable via
    /// `/proc/<pid>/cmdline` for the process's lifetime and also lands in
    /// shell history, while an environment variable does not.
    #[arg(long, env = "FSLITE_TOKEN", hide_env_values = true, requires = "server")]
    token: Option<String>,
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p fslite-cli --test e2e_local help_does_not_print_the_fslite_token_value`
Expected: PASS. `--help` now shows `[env: FSLITE_TOKEN=]` (or equivalent, with the value elided) instead of the real value.

- [ ] **Step 5: Run the full e2e_local suite to confirm no regression**

Run: `cargo test -p fslite-cli --test e2e_local`
Expected: PASS — all existing tests in that file still pass unmodified.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-cli/src/main.rs crates/fslite-cli/tests/e2e_local.rs
git commit -m "fix(fslite-cli): stop --help from printing the FSLITE_TOKEN value"
```

---

## Task 2: Stop error messages from forging fake extra stderr lines

**Files:**
- Modify: `crates/fslite-cli/src/main.rs` — the two `eprintln!` sites in `run_line` and `run_repl`
- Test: `crates/fslite-cli/tests/e2e_local.rs` (new test, appended to the existing file)

**Interfaces:**
- Consumes: `fslite_command::render::sanitize_name(raw: &str) -> String` — already implemented in `crates/fslite-command/src/render.rs` (it is the stricter sibling of `sanitize_for_terminal`, additionally stripping `\n`/`\t`), reachable via its full path `fslite_command::render::sanitize_name` exactly like the current code already reaches `fslite_command::render::sanitize_for_terminal` — no new re-export needed, no `fslite-command` change at all.
- Produces: nothing new — this only changes which sanitizer the two existing error-printing call sites use.

The current code, exactly as it stands today, in two places:

`run_line` (one-shot mode):

```rust
async fn run_line(executor: &dyn Executor, ctx: &RequestContext, line: &str, json: bool) {
    let command = match fslite_command::parser::parse(line) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("parse error: {err:?}");
            std::process::exit(2);
        }
    };
    match executor.execute(ctx, command).await {
        Ok(output) => print_output(&output, json),
        Err(err) => {
            eprintln!(
                "error: {} ({:?})",
                fslite_command::render::sanitize_for_terminal(err.message()),
                err.code()
            );
            std::process::exit(1);
        }
    }
}
```

`run_repl` (REPL mode), the equivalent arm inside its loop:

```rust
            Ok(command) => match executor.execute(ctx, command).await {
                Ok(output) => print_output(&output, json),
                Err(err) => eprintln!(
                    "error: {} ({:?})",
                    fslite_command::render::sanitize_for_terminal(err.message()),
                    err.code()
                ),
            },
```

`err.message()` is a `Display`-formatted string that can embed attacker-influenced path text verbatim (e.g. `FsError::already_exists(path)` renders as `"already exists: {path}"`), and `VirtualPath::parse` only rejects a missing leading `/` and embedded NUL — a literal newline character is a perfectly legal byte in a path. `sanitize_for_terminal` strips ANSI/control bytes but, by design, keeps `\n`/`\t` (correct for the genuinely free-text fields it's meant for, like search previews) — so a hostile path containing `\n` followed by fake error-looking text still produces two stderr lines: the real error, and a forged second line that looks like an unrelated message. `sanitize_name` is the stricter sanitizer already used everywhere else in the renderer for exactly this reason (node names, paths, link targets); it strips `\n`/`\t` too. Since it strips a strict superset of what `sanitize_for_terminal` strips, swapping it in cannot reintroduce the already-fixed ANSI-escape issue — it only closes the remaining newline gap.

- [ ] **Step 1: Write the failing test**

Append to `crates/fslite-cli/tests/e2e_local.rs`:

```rust
/// Regression test for the newline half of the terminal-injection finding
/// left standing after the ESC-byte fix
/// (`one_shot_mode_never_writes_a_raw_escape_byte_to_stderr_on_error`,
/// above): a node name/path can legally contain a raw `\n`, and
/// `sanitize_for_terminal` — used at the time by `run_line`'s error
/// printer — preserves `\n` by design (it's meant for free-text fields
/// like search previews, not structured fields like paths). A hostile path
/// containing `\n` followed by fake error-looking text therefore forged a
/// second, fake stderr line that looked like an unrelated message. This
/// creates a node with such a name, triggers a real domain error against
/// it (a duplicate `mkdir`, same shape as the ESC-byte test), and asserts
/// the CLI's real stderr is exactly one line — the real error — not two.
#[test]
fn one_shot_mode_does_not_forge_an_extra_stderr_line_from_a_newline_in_an_error_message() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli()
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let hostile_path =
        "/evil\nerror: workspace quota exceeded, contact admin (InternalStorageFailure).txt";

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
    // `FsError` message embeds the raw hostile path text, newline included.
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
    let stderr_text = String::from_utf8_lossy(&second.stderr);
    assert_eq!(
        stderr_text.lines().count(),
        1,
        "expected exactly one stderr line (no forged extra line), got: {stderr_text}"
    );
    assert!(
        stderr_text.contains("evil") && stderr_text.contains("quota exceeded"),
        "expected the sanitized message to retain the benign surrounding text, got: {stderr_text}"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p fslite-cli --test e2e_local one_shot_mode_does_not_forge_an_extra_stderr_line_from_a_newline_in_an_error_message`
Expected: FAIL — `stderr_text.lines().count()` is `2` (the injected `\n` splits the real error text from the forged `"error: workspace quota exceeded..."` line), not `1`.

- [ ] **Step 3: Write the fix**

In `crates/fslite-cli/src/main.rs`, change both `eprintln!` sites from `sanitize_for_terminal` to `sanitize_name`. In `run_line`:

```rust
async fn run_line(executor: &dyn Executor, ctx: &RequestContext, line: &str, json: bool) {
    let command = match fslite_command::parser::parse(line) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("parse error: {err:?}");
            std::process::exit(2);
        }
    };
    match executor.execute(ctx, command).await {
        Ok(output) => print_output(&output, json),
        Err(err) => {
            eprintln!(
                "error: {} ({:?})",
                fslite_command::render::sanitize_name(err.message()),
                err.code()
            );
            std::process::exit(1);
        }
    }
}
```

And in `run_repl`'s matching arm:

```rust
            Ok(command) => match executor.execute(ctx, command).await {
                Ok(output) => print_output(&output, json),
                Err(err) => eprintln!(
                    "error: {} ({:?})",
                    fslite_command::render::sanitize_name(err.message()),
                    err.code()
                ),
            },
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p fslite-cli --test e2e_local one_shot_mode_does_not_forge_an_extra_stderr_line_from_a_newline_in_an_error_message`
Expected: PASS.

- [ ] **Step 5: Run the full e2e_local and e2e_repl suites to confirm no regression**

Run: `cargo test -p fslite-cli --test e2e_local --test e2e_repl`
Expected: PASS — in particular, `repl_mode_never_writes_a_raw_escape_byte_to_stderr_on_error` (in `e2e_repl.rs`) must still pass: `sanitize_name` strips every ASCII control byte `sanitize_for_terminal` already stripped (including ESC) *plus* `\n`/`\t`, so it is a strict superset — nothing that test currently asserts (`!contains(0x1b)`, message retains `"evil"`/`"FAKE"`) can regress.

**A scope note on `run_repl`, worth understanding before you consider this task "fully tested from both entry points":** unlike the ESC-byte case, this specific newline-forging bug cannot be *reproduced* by typing a hostile argument directly into the REPL's stdin — `run_repl` reads lines via `std::io::BufRead::lines()`, which splits strictly on `\n` before `main.rs` ever sees a line, so no string the REPL hands to the parser can ever contain a raw newline byte in the first place. (Confirmed by grepping `fslite-sqlite`'s error sites: every `FsError` subject is either the exact path the user typed, or — for `restore`/`purge` — a plain trash-id UUID with no user-controlled content; there is no code path where a *previously stored* hostile string reaches an error message without the caller re-supplying it as an argument.) The fix still applies to `run_repl` (same commit, same swap, verified above to not regress the existing REPL ESC test), because the same hostile data *can* reach it via a channel that isn't stdin-typed — namely a `RemoteExecutor` talking to a compromised or buggy `fslite-server`, whose JSON error response arrives over HTTP, not through `BufRead::lines()`. Building a reliable, verified test for that specific scenario would require confirming exactly how a raw newline byte in a `VirtualPath` round-trips through URL construction (`reqwest`) and path extraction (`axum`) end to end — genuinely uncertain behavior this plan has not verified and shouldn't guess at. If remote-mode error-message injection via a hostile server turns out to matter to you, treat it as a follow-up investigation, not a checkbox this plan can honestly claim to close.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-cli/src/main.rs crates/fslite-cli/tests/e2e_local.rs
git commit -m "fix(fslite-cli): stop hostile newlines from forging fake stderr lines"
```

---

## Self-Review

**Spec coverage:** both parked residuals from the `fslite-command-cli` final review are covered — Task 1 closes the `--help`/`FSLITE_TOKEN` leak, Task 2 closes the newline-forging gap in `main.rs`'s two error-printing call sites (with an explicit, honest scope note on why a REPL-stdin-typed reproduction isn't possible for this specific finding, rather than a padded or fake test standing in for one).

**Placeholder scan:** no `TODO`/`TBD`; every step has real, complete code; the one deliberately-not-covered angle (remote-mode HTTP-sourced newline injection reaching `run_repl`) is called out explicitly as out of scope with its reasoning, not silently dropped.

**Type consistency:** `sanitize_name(raw: &str) -> String` is used identically in both call sites in Task 2, matching its existing signature in `crates/fslite-command/src/render.rs`; `hide_env_values` in Task 1 is a bare `clap` derive attribute, not a new type, so there's nothing to drift.
