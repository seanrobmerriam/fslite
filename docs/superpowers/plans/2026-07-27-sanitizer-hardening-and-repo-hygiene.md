# Sanitizer Hardening & Repo Hygiene Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four Minor findings parked during the `cli-residual-hardening` plan's final whole-branch review: `sanitize_name` not reachable from `fslite-command`'s crate root, search-match previews still able to forge a fake output row via an embedded newline in file content, `sanitize_name`/`sanitize_for_terminal` not stripping Unicode bidi-override or line/paragraph-separator characters, and `Cargo.lock` being untracked with no `.gitignore` in the repo at all.

**Architecture:** Tasks 1–3 live entirely in `fslite-command`'s renderer (`src/render.rs`, `src/lib.rs`, `tests/render.rs`) — this plan explicitly lifts the "do not modify `fslite-command`" constraint that bound the two prior plans, since fixing these findings requires touching exactly the module those plans deliberately left alone. Task 4 is unrelated repo hygiene (a root `.gitignore` plus committing the existing, already-resolved `Cargo.lock`) and touches no Rust source at all. Tasks 2 and 3 both edit the same three sanitizer functions in `render.rs`; they're sequenced so each lands a complete, independently-correct, independently-tested increment — Task 3 extends what Task 2 introduces, not the other way around.

**Tech Stack:** Rust, the existing `fslite-command` crate and its `tests/render.rs` integration-test file (no new dependencies).

## Global Constraints

- This plan modifies `fslite-command` — unlike the two prior plans (`fslite-command-cli` and `cli-residual-hardening`), which explicitly froze it. Do not modify `fslite-core`, `fslite-sqlite`, `fslite-server`, or `fslite-cli`; nothing in `fslite-cli` needs to change (it already calls `sanitize_name` correctly per the prior plan's fixes) and nothing here requires touching the frozen crates.
- Keep the repo green throughout: `cargo fmt -p fslite-command --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` must all pass after each task's commit.
- Every public sanitizer function `fslite-command` defines must be reachable both via `fslite_command::render::<name>` and via the crate-root re-export `fslite_command::<name>` — Task 1 exists specifically because `sanitize_name` broke this rule; don't reintroduce the same asymmetry for `sanitize_preview` in Task 2.
- Every step must produce a real, runnable, git-committed change — no `TODO`s, no further-deferred follow-ups. These are the last four known issues from the prior plan's final review; there is no next review queued to catch anything left undone here.

---

## Task 1: Re-export `sanitize_name` from the `fslite-command` crate root

**Files:**
- Modify: `crates/fslite-command/src/lib.rs:20`
- Test: `crates/fslite-command/tests/render.rs` (new test, appended to the existing file)

**Interfaces:**
- Consumes: `render::sanitize_name(raw: &str) -> String`, already implemented in `crates/fslite-command/src/render.rs` and already used internally throughout that file's `render_human` (this task only changes what's re-exported, not the function itself).
- Produces: `fslite_command::sanitize_name` becomes a valid crate-root path, matching `fslite_command::sanitize_for_terminal`, `fslite_command::render_human`, and `fslite_command::render_json`, which are already re-exported this way.

The current re-export line, exactly as it stands today:

```rust
pub use render::{render_human, render_json, sanitize_for_terminal};
```

`sanitize_name` is `pub fn` inside `render.rs` and reachable via the longer path `fslite_command::render::sanitize_name`, but — unlike its sibling `sanitize_for_terminal` — it was never added to this crate-root re-export line. Since `sanitize_name` is the *stricter*, generally-preferred sanitizer (used for every structured field in the renderer), requiring callers to know the internal module path for it while its weaker sibling gets the short path is a real ergonomic trap, not just an inconsistency.

- [ ] **Step 1: Write the failing test**

Append to `crates/fslite-command/tests/render.rs`:

```rust
/// Regression test: `sanitize_name` is `fslite-command`'s stricter, more
/// generally-applicable sanitizer, but was missing from the crate-root
/// re-export line in `lib.rs` — reachable only via the longer
/// `fslite_command::render::sanitize_name` path, unlike its sibling
/// `sanitize_for_terminal`. This is a compile-time proof: if the
/// crate-root path doesn't resolve, this test fails to compile at all.
#[test]
fn sanitize_name_is_reachable_from_the_crate_root() {
    let clean = fslite_command::sanitize_name("a\nb");
    assert_eq!(clean, "ab");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p fslite-command --test render sanitize_name_is_reachable_from_the_crate_root`
Expected: FAIL to compile — `error[E0433]: failed to resolve: could not find 'sanitize_name' in 'fslite_command'` (or similar), since only `fslite_command::render::sanitize_name` currently resolves.

- [ ] **Step 3: Write the fix**

In `crates/fslite-command/src/lib.rs`, add `sanitize_name` to the existing re-export line:

```rust
pub use render::{render_human, render_json, sanitize_for_terminal, sanitize_name};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p fslite-command --test render sanitize_name_is_reachable_from_the_crate_root`
Expected: PASS.

- [ ] **Step 5: Run the full render test suite to confirm no regression**

Run: `cargo test -p fslite-command --test render`
Expected: PASS — all existing tests in that file still pass unmodified.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-command/src/lib.rs crates/fslite-command/tests/render.rs
git commit -m "fix(fslite-command): re-export sanitize_name from the crate root"
```

---

## Task 2: Stop search-match previews from forging a fake row via an embedded newline

**Files:**
- Modify: `crates/fslite-command/src/render.rs` (new `sanitize_preview` function; `render_human`'s `SearchMatches` arm switches to it)
- Modify: `crates/fslite-command/src/lib.rs:20` (add `sanitize_preview` to the crate-root re-export, per this plan's Global Constraints)
- Test: `crates/fslite-command/tests/render.rs` (new test, appended to the existing file)

**Interfaces:**
- Consumes: `render::sanitize_for_terminal(raw: &str) -> String`, already implemented (this task wraps it, doesn't change it — Task 3 changes it).
- Produces: `render::sanitize_preview(raw: &str) -> String`, also re-exported as `fslite_command::sanitize_preview`. Task 3 extends this function's escaping to two more characters; its signature and crate-root reachability don't change.

The current `SearchMatches` arm of `render_human`, exactly as it stands today, in `crates/fslite-command/src/render.rs`:

```rust
        CommandOutput::SearchMatches(page) => page
            .items
            .iter()
            .map(|m| {
                format!(
                    "{}: {}",
                    sanitize_name(m.path.as_str()),
                    sanitize_for_terminal(&String::from_utf8_lossy(&m.preview))
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
```

`sanitize_for_terminal` deliberately preserves `\n` because search-match previews are genuinely free-text file content, where a literal newline can be legitimate. But printing that newline *raw* inside a `"{path}: {preview}"` row still lets a hostile file forge a fake extra search-result row that looks like a real, unrelated match (e.g. a file whose content contains `needle\n/etc/shadow: root:x:0:0` renders as if `/etc/shadow: root:x:0:0` were a second, real match). The fix isn't to strip the newline (that would silently corrupt legitimate preview content) — it's to make it visible without letting it act as a row separator: escape it into the two-character sequence `\n` (backslash, then the letter n) instead of passing the raw byte through.

- [ ] **Step 1: Write the failing test**

Append to `crates/fslite-command/tests/render.rs`:

```rust
/// Regression test: search-match previews are free-text file content,
/// where a literal newline can be legitimate — but printing it raw inside
/// `"{path}: {preview}"` still let a hostile file forge a fake extra
/// search-result row that looked like a real, unrelated match. Escaping
/// the newline into a visible two-character `\n` sequence keeps the
/// content visible without letting it masquerade as a row boundary.
#[test]
fn human_rendering_of_search_matches_does_not_forge_an_extra_row_from_a_newline_in_the_preview() {
    let search_match = SearchMatch {
        node: sample_node("ignored"),
        path: VirtualPath::parse("/real.txt").unwrap(),
        range: ByteRange::new(0, 5),
        preview: b"needle\n/etc/shadow: root:x:0:0".to_vec(),
    };
    let output = CommandOutput::SearchMatches(Page::new(vec![search_match], None));
    let rendered = render_human(&output);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "rendered output had a forged extra line: {rendered:?}"
    );
    assert!(
        rendered.contains("needle\\n/etc/shadow"),
        "expected the embedded newline to survive as a visible escape sequence, got: {rendered:?}"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p fslite-command --test render human_rendering_of_search_matches_does_not_forge_an_extra_row_from_a_newline_in_the_preview`
Expected: FAIL — `lines.len()` is `2` (the raw `\n` in the preview splits the rendered output into two lines: `"/real.txt: needle"` and `"/etc/shadow: root:x:0:0"`), not `1`.

- [ ] **Step 3: Write the fix**

In `crates/fslite-command/src/render.rs`, add `sanitize_preview` immediately after the existing `sanitize_name` function:

```rust
/// [`sanitize_for_terminal`], with `\n`/`\t` then escaped into visible
/// two-character sequences instead of passed through raw. Use this for
/// free-text content rendered *inline* within a single table-shaped
/// output row (currently only search-match previews): a real newline in
/// the underlying file content stays visible to the user, but can never
/// be mistaken for a row boundary the way a raw `\n` could.
pub fn sanitize_preview(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in sanitize_for_terminal(raw).chars() {
        match ch {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}
```

Then update `render_human`'s `SearchMatches` arm to use it instead of `sanitize_for_terminal`:

```rust
        CommandOutput::SearchMatches(page) => page
            .items
            .iter()
            .map(|m| {
                format!(
                    "{}: {}",
                    sanitize_name(m.path.as_str()),
                    sanitize_preview(&String::from_utf8_lossy(&m.preview))
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
```

Finally, in `crates/fslite-command/src/lib.rs`, add `sanitize_preview` to the crate-root re-export line (which Task 1 already extended once):

```rust
pub use render::{render_human, render_json, sanitize_for_terminal, sanitize_name, sanitize_preview};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p fslite-command --test render human_rendering_of_search_matches_does_not_forge_an_extra_row_from_a_newline_in_the_preview`
Expected: PASS.

- [ ] **Step 5: Run the full render test suite to confirm no regression**

Run: `cargo test -p fslite-command --test render`
Expected: PASS. In particular, the existing `human_rendering_of_search_matches_sanitizes_path_and_preview` test (preview `b"safe\x1b[31mtext"`, no newline) must still pass — `sanitize_preview` wraps `sanitize_for_terminal`, which still strips the ESC byte exactly as before; that test never asserts anything about `\n` handling, so it's unaffected.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-command/src/render.rs crates/fslite-command/src/lib.rs crates/fslite-command/tests/render.rs
git commit -m "fix(fslite-command): escape newlines in search previews instead of passing them through raw"
```

---

## Task 3: Strip Unicode bidi-override and line/paragraph-separator characters

**Files:**
- Modify: `crates/fslite-command/src/render.rs` (two new private helper functions; `sanitize_for_terminal`, `sanitize_name`, and `sanitize_preview` all updated to use them)
- Test: `crates/fslite-command/tests/render.rs` (new tests, appended to the existing file)

**Interfaces:**
- Consumes: nothing new from earlier tasks beyond the three sanitizer functions Task 1 and Task 2 already established (`sanitize_for_terminal`, `sanitize_name`, `sanitize_preview`) — this task changes their internals, not their signatures.
- Produces: nothing new is exposed; the three existing public functions become stricter.

`char::is_control()` — the filter all three sanitizers are built on — only covers Unicode general category Cc (control). It does **not** catch:
- **Bidirectional-override characters** (Unicode category Cf, "format"): U+202A–U+202E (`LRE`/`RLE`/`PDF`/`LRO`/`RLO`) and U+2066–U+2069 (`LRI`/`RLI`/`FSI`/`PDI`). These can silently reorder how surrounding text *displays* without changing its underlying bytes — the classic filename-spoofing trick is a name like `"harmless\u{202E}gpj.exe"`, which contains the literal bytes `.exe` but *displays* as if it ends in `.jpg` reversed, because the RLO character tells bidi-aware renderers (including many terminals) to render everything after it right-to-left.
- **Unicode line/paragraph separators** (categories Zl/Zp): U+2028 (`LINE SEPARATOR`) and U+2029 (`PARAGRAPH SEPARATOR`). These render as line breaks in many terminals — the same row-forging risk `\n` has — but `char::is_control()` doesn't catch them either.

- [ ] **Step 1: Write the failing tests**

Append to `crates/fslite-command/tests/render.rs`:

```rust
/// Regression test: a Unicode right-to-left-override character can make a
/// name *display* with a spoofed extension (e.g. a name ending
/// `\u{202E}gpj.exe` can render as if it ends `.jpg`) without changing the
/// underlying bytes — `char::is_control()` doesn't catch it, since RLO is
/// Unicode category Cf (format), not Cc (control).
#[test]
fn sanitize_name_strips_bidi_override_characters() {
    let hostile_name = "harmless\u{202E}gpj.exe";
    let clean = sanitize_name(hostile_name);
    assert!(
        !clean.contains('\u{202E}'),
        "RLO character survived: {clean:?}"
    );
    assert!(clean.contains("harmless"));
}

/// Regression test: the Unicode line/paragraph separators U+2028/U+2029
/// render as line breaks in many terminals — the same row-forging risk as
/// `\n` — but aren't caught by `char::is_control()` (categories Zl/Zp, not
/// Cc). `sanitize_name` must strip them like it strips `\n`.
#[test]
fn sanitize_name_strips_unicode_line_and_paragraph_separators() {
    let hostile_name = "a.txt\u{2028}file 999 IMPORTANT.txt";
    let clean = sanitize_name(hostile_name);
    assert_eq!(clean, "a.txtfile 999 IMPORTANT.txt");
}

/// `sanitize_for_terminal` must strip bidi overrides (they're never
/// legitimate in any context) while still preserving the Unicode
/// line/paragraph separators for genuinely free-text content, exactly
/// like it already preserves `\n`/`\t`.
#[test]
fn sanitize_for_terminal_strips_bidi_overrides_but_preserves_unicode_linebreaks() {
    let input = "safe\u{202E}text\u{2028}more";
    let clean = sanitize_for_terminal(input);
    assert_eq!(clean, "safetext\u{2028}more");
}

/// `sanitize_preview` must escape the Unicode line separator into a
/// visible sequence exactly like it already escapes `\n`, for the same
/// row-forging reason.
#[test]
fn sanitize_preview_escapes_unicode_line_separator_too() {
    let input = "needle\u{2028}more content";
    let escaped = sanitize_preview(input);
    assert!(!escaped.contains('\u{2028}'));
    assert!(escaped.contains("needle\\u{2028}more"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p fslite-command --test render sanitize_name_strips_bidi_override_characters sanitize_name_strips_unicode_line_and_paragraph_separators sanitize_for_terminal_strips_bidi_overrides_but_preserves_unicode_linebreaks sanitize_preview_escapes_unicode_line_separator_too`
Expected: all four FAIL — none of the three sanitizers currently touch bidi-override or Unicode line-separator characters at all, so each assertion that expects them stripped or escaped fails.

- [ ] **Step 3: Write the fix**

In `crates/fslite-command/src/render.rs`, add two private helper functions right before `sanitize_for_terminal`:

```rust
/// Unicode bidirectional-control characters that can silently reorder how
/// surrounding text *displays* without changing its underlying bytes —
/// e.g. a name ending `\u{202E}gpj.exe` can display as if it ends `.jpg`
/// reversed. `char::is_control()` does not catch these; they are Unicode
/// general category Cf (format), not Cc (control).
fn is_bidi_override(ch: char) -> bool {
    matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Unicode line/paragraph separators that render as line breaks in many
/// terminals but aren't caught by `char::is_control()` either (categories
/// Zl/Zp, not Cc).
fn is_unicode_linebreak(ch: char) -> bool {
    matches!(ch, '\u{2028}' | '\u{2029}')
}
```

Then update `sanitize_for_terminal` to strip bidi overrides while continuing to preserve line breaks (now including the two Unicode ones):

```rust
pub fn sanitize_for_terminal(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| {
            ch == '\n'
                || ch == '\t'
                || is_unicode_linebreak(ch)
                || (!ch.is_control() && !is_bidi_override(ch))
        })
        .collect()
}
```

Update `sanitize_name` to also strip bidi overrides and the Unicode line separators:

```rust
pub fn sanitize_name(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| !ch.is_control() && !is_bidi_override(ch) && !is_unicode_linebreak(ch))
        .collect()
}
```

Update `sanitize_preview`'s escaping `match` to also escape the two Unicode line-separator characters, alongside the `\n`/`\t` it already escapes:

```rust
pub fn sanitize_preview(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in sanitize_for_terminal(raw).chars() {
        match ch {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\u{2028}' => escaped.push_str("\\u{2028}"),
            '\u{2029}' => escaped.push_str("\\u{2029}"),
            other => escaped.push(other),
        }
    }
    escaped
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p fslite-command --test render sanitize_name_strips_bidi_override_characters sanitize_name_strips_unicode_line_and_paragraph_separators sanitize_for_terminal_strips_bidi_overrides_but_preserves_unicode_linebreaks sanitize_preview_escapes_unicode_line_separator_too`
Expected: PASS, all four.

- [ ] **Step 5: Run the full render test suite to confirm no regression**

Run: `cargo test -p fslite-command --test render`
Expected: PASS — in particular, confirm the pre-existing `sanitize_strips_other_control_bytes_but_keeps_newline_and_tab` (input `"a\x07b\nc\td"`, expects `"ab\nc\td"`) and `sanitize_name_strips_newline_and_tab_in_addition_to_other_control_bytes` (input `"a\x07b\nc\td\x1be"`, expects `"abcde"`) tests still pass unmodified: neither input contains a bidi-override or Unicode-linebreak character, so adding those checks with `&&`/additional `||` branches as shown above cannot change what happens to bytes that were already being kept or already being stripped.

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-command/src/render.rs crates/fslite-command/tests/render.rs
git commit -m "fix(fslite-command): strip Unicode bidi-override and line-separator characters"
```

---

## Task 4: Commit `Cargo.lock`, add a `.gitignore`

**Files:**
- Create: `.gitignore` (repo root)
- Modify: `Cargo.lock` (repo root — currently untracked; this task adds it to version control as-is, unmodified in content)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing code-facing; this is a pure repo-hygiene task with no test cycle in the usual sense (there's no function to call). Verification is by inspecting `git status` and confirming a clean build doesn't dirty the lockfile.

There is currently no `.gitignore` anywhere in this repository (`ls .gitignore` at the repo root returns "No such file or directory"), and `Cargo.lock` — the resolved dependency-version lockfile — has never been committed, despite this workspace producing two binary crates (`fslite-server`, `fslite-cli`). Rust's standard guidance is: workspaces that produce binaries (as opposed to library-only crates) should commit `Cargo.lock`, since it's what makes a `cargo build`/`cargo install` reproducible across machines and time — without it, a future build can silently resolve different dependency versions than the ones this codebase was actually developed and tested against.

- [ ] **Step 1: Create the `.gitignore`**

Create `.gitignore` at the repository root (same directory as the workspace's `Cargo.toml`):

```gitignore
/target
```

- [ ] **Step 2: Verify `target/` no longer shows as untracked**

Run: `git status --short`
Expected: `target/` no longer appears in the output (it's now ignored); `.gitignore` appears as a new untracked file; `Cargo.lock` still appears as untracked (it isn't ignored — that's the point, it gets added in Step 3).

- [ ] **Step 3: Stage and commit both files**

```bash
git add .gitignore Cargo.lock
git commit -m "chore: add .gitignore, commit Cargo.lock for reproducible builds"
```

- [ ] **Step 4: Verify a clean build doesn't dirty the committed lockfile**

Run: `cargo build --workspace --all-features && cargo test --workspace --all-features && git status --short`
Expected: both commands succeed, and the final `git status --short` prints nothing (a completely clean tree) — proving the `Cargo.lock` just committed is exactly what the current dependency set resolves to, not a stale or hand-edited copy.

---

## Self-Review

**Spec coverage:** all four parked findings from the `cli-residual-hardening` final review are covered — Task 1 (export asymmetry), Task 2 (search-preview newline forging), Task 3 (bidi overrides + Unicode line separators), Task 4 (untracked `Cargo.lock` / missing `.gitignore`).

**Placeholder scan:** no `TODO`/`TBD`; every step has real, complete code or a real, runnable command; Task 4's lack of a traditional red/green test cycle is because it's genuinely not a code change (there's no function under test) — its verification steps (Steps 2 and 4) are the equivalent real, checkable gate.

**Type consistency:** `sanitize_preview(raw: &str) -> String`, introduced in Task 2, keeps that exact signature through Task 3's internal changes to its escaping logic. `is_bidi_override(ch: char) -> bool` and `is_unicode_linebreak(ch: char) -> bool`, introduced in Task 3, are used identically across all three public sanitizer functions. `sanitize_name`'s signature (`fn(raw: &str) -> String`) is unchanged from Task 1 through Task 3 — only its internal filter predicate gets stricter.
