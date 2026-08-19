# Relative CLI Paths Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow every CLI virtual-path operand and glob pattern to be written relative to the workspace root while preserving absolute paths and strict core/wire invariants.

**Architecture:** Normalize user text only in `fslite-command`'s parser. `parse_path` selects `VirtualPath::parse` for absolute input and `VirtualPath::root().join` for relative input; a separate glob helper uses the same root join after removing leading slashes so wildcard segments remain intact. Executors, codecs, backends, and `fslite-core` continue receiving canonical absolute values.

**Tech Stack:** Rust 2024, Cargo workspace, `fslite-core::VirtualPath`, `fslite-command`, `fslite` CLI, SQLite end-to-end tests.

## Global Constraints

- Relative virtual paths resolve from `/`, never from the host working directory.
- Existing absolute paths remain supported without changing their canonical output.
- `fslite-core::VirtualPath::parse`, serialized commands, HTTP requests, and batch JSON remain absolute-only.
- A relative `ln` target retains symlink-relative semantics; only the link location is workspace-root-relative.
- Host paths such as `--db` and `batch --file` are unchanged.
- Traversal is clamped at the workspace root and NUL bytes remain invalid.

---

### Task 1: Normalize Virtual-Path Operands at the Parser Boundary

**Files:**
- Modify: `crates/fslite-command/tests/parser.rs`
- Modify: `crates/fslite-command/tests/parser_security.rs`
- Modify: `crates/fslite-cli/tests/e2e_bootstrap.rs`
- Modify: `crates/fslite-command/src/parser.rs`

**Interfaces:**
- Consumes: `VirtualPath::parse(&str)` and `VirtualPath::root().join(&str)`.
- Produces: existing private `parse_path(verb, name, raw) -> Result<VirtualPath, ParseError>` with relative-input support and no command enum changes.

- [ ] **Step 1: Write failing parser tests for each distinct path position**

Add tests to `crates/fslite-command/tests/parser.rs` covering the shared
single-path helper, `./` normalization, both two-path operands, a flag path,
search roots, and the `ln` target boundary:

```rust
#[test]
fn relative_virtual_paths_resolve_from_the_workspace_root() {
    assert_eq!(
        parse("mkdir docs --parents").unwrap(),
        Command::Mkdir {
            path: path("/docs"),
            options: CreateOptions::default().parents(true),
        }
    );
    assert_eq!(
        parse("cat ./docs/readme.md").unwrap(),
        Command::Read {
            path: path("/docs/readme.md"),
            options: ReadOptions::default(),
        }
    );
    match parse("cp source.txt docs/copy.txt").unwrap() {
        Command::Copy { from, to, .. } => {
            assert_eq!(from, path("/source.txt"));
            assert_eq!(to, path("/docs/copy.txt"));
        }
        other => panic!("expected Copy, got {other:?}"),
    }
    match parse("mv docs/copy.txt archive/copy.txt").unwrap() {
        Command::Move { from, to, .. } => {
            assert_eq!(from, path("/docs/copy.txt"));
            assert_eq!(to, path("/archive/copy.txt"));
        }
        other => panic!("expected Move, got {other:?}"),
    }

    let trash = fslite_core::TrashId::new();
    match parse(&format!("restore {trash} --to=restored/file.txt")).unwrap() {
        Command::Restore { destination, .. } => {
            assert_eq!(destination, Some(path("/restored/file.txt")));
        }
        other => panic!("expected Restore, got {other:?}"),
    }

    assert_eq!(
        parse("find docs --kind=file").unwrap(),
        Command::Find {
            query: FindQuery::default()
                .root(path("/docs"))
                .kind(Some(NodeKind::File)),
            page: PageRequest::default(),
        }
    );
    assert_eq!(
        parse("grep docs needle").unwrap(),
        Command::SearchContent {
            query: ContentQuery::default()
                .root(path("/docs"))
                .needle(b"needle".to_vec()),
            page: PageRequest::default(),
        }
    );

    match parse("ln ../target docs/link").unwrap() {
        Command::Symlink { target, link, .. } => {
            assert_eq!(target, LinkTarget::parse("../target").unwrap());
            assert_eq!(link, path("/docs/link"));
        }
        other => panic!("expected Symlink, got {other:?}"),
    }
}
```

- [ ] **Step 2: Write failing containment and validation tests**

Add to `crates/fslite-command/tests/parser_security.rs`:

```rust
#[test]
fn relative_path_traversal_is_clamped_to_the_workspace_root() {
    match parse("stat ../../../../etc/passwd").unwrap() {
        Command::Stat { path, .. } => assert_eq!(path.as_str(), "/etc/passwd"),
        other => panic!("expected Stat, got {other:?}"),
    }
}

#[test]
fn relative_paths_with_nul_bytes_are_rejected() {
    let error = parse("stat docs\0secret").unwrap_err();
    assert!(matches!(
        error,
        fslite_command::parser::ParseError::InvalidArgument {
            verb: "stat",
            name: "path",
            ..
        }
    ));
}
```

- [ ] **Step 3: Write a failing installed-style workflow test**

Replace `first_verb_bootstraps_once_and_keeps_stdout_clean` in
`crates/fslite-cli/tests/e2e_bootstrap.rs` with:

```rust
#[test]
fn first_relative_verb_bootstraps_once_and_supports_a_complete_workflow() {
    let fixture = Fixture::new();
    let first = fixture.cli().args(["mkdir", "docs"]).output().unwrap();
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(String::from_utf8(first.stderr).unwrap(), NOTICE);
    assert!(!String::from_utf8(first.stdout).unwrap().contains(NOTICE));
    assert!(fixture.default_db().exists());

    let write = fixture
        .cli()
        .args(["write", "docs/hello.txt", "--text=hello"])
        .output()
        .unwrap();
    assert!(write.status.success());
    assert!(write.stderr.is_empty());

    let cat = fixture
        .cli()
        .args(["cat", "./docs/hello.txt"])
        .output()
        .unwrap();
    assert!(cat.status.success());
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello");
    assert!(cat.stderr.is_empty());
}
```

- [ ] **Step 4: Run tests to verify RED**

Run:

```bash
cargo test -p fslite-command --test parser relative_virtual_paths_resolve_from_the_workspace_root
cargo test -p fslite-command --test parser_security relative_path
cargo test -p fslite --test e2e_bootstrap first_relative_verb_bootstraps_once_and_supports_a_complete_workflow
```

Expected: all fail because `parse_path` rejects non-absolute input.

- [ ] **Step 5: Implement the minimal parser change**

Replace `parse_path` in `crates/fslite-command/src/parser.rs` with:

```rust
fn parse_path(
    verb: &'static str,
    name: &'static str,
    raw: &str,
) -> Result<VirtualPath, ParseError> {
    let parsed = if raw.starts_with('/') {
        VirtualPath::parse(raw)
    } else {
        VirtualPath::root().join(raw)
    };
    parsed.map_err(|e| ParseError::InvalidArgument {
        verb,
        name,
        reason: e.message().to_string(),
    })
}
```

- [ ] **Step 6: Run focused suites to verify GREEN**

Run:

```bash
cargo test -p fslite-command --test parser
cargo test -p fslite-command --test parser_security
cargo test -p fslite --test e2e_bootstrap
```

Expected: all tests pass.

- [ ] **Step 7: Commit path behavior**

```bash
git add crates/fslite-command/src/parser.rs crates/fslite-command/tests/parser.rs crates/fslite-command/tests/parser_security.rs crates/fslite-cli/tests/e2e_bootstrap.rs
git commit -m "feat(fslite): accept workspace-relative CLI paths"
```

---

### Task 2: Normalize Relative Glob Patterns

**Files:**
- Modify: `crates/fslite-command/tests/parser.rs`
- Modify: `crates/fslite-command/tests/parser_security.rs`
- Modify: `crates/fslite-command/src/parser.rs`

**Interfaces:**
- Consumes: `VirtualPath::root().join(&str)` for segment normalization while preserving wildcard text.
- Produces: private `parse_glob_pattern(raw) -> Result<String, ParseError>` returning an absolute pattern.

- [ ] **Step 1: Write failing glob tests**

Add to `parser.rs`:

```rust
#[test]
fn glob_patterns_resolve_from_the_workspace_root() {
    for (input, expected) in [
        ("glob 'docs/*.md'", "/docs/*.md"),
        ("glob './docs/**/*.md'", "/docs/**/*.md"),
        ("glob '/docs/*.md'", "/docs/*.md"),
        ("glob '../../docs/*.md'", "/docs/*.md"),
    ] {
        assert_eq!(
            parse(input).unwrap(),
            Command::Glob {
                pattern: expected.to_string(),
                page: PageRequest::default(),
            }
        );
    }
}
```

Add to `parser_security.rs`:

```rust
#[test]
fn glob_patterns_with_nul_bytes_are_rejected() {
    let error = parse("glob 'docs/\0*.txt'").unwrap_err();
    assert!(matches!(
        error,
        fslite_command::parser::ParseError::InvalidArgument {
            verb: "glob",
            name: "pattern",
            ..
        }
    ));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p fslite-command --test parser glob_patterns_resolve_from_the_workspace_root
cargo test -p fslite-command --test parser_security glob_patterns_with_nul_bytes_are_rejected
```

Expected: relative strings remain unchanged and NUL is accepted by the parser.

- [ ] **Step 3: Implement glob normalization**

Add beside `parse_path`:

```rust
fn parse_glob_pattern(raw: &str) -> Result<String, ParseError> {
    VirtualPath::root()
        .join(raw.trim_start_matches('/'))
        .map(|pattern| pattern.as_str().to_string())
        .map_err(|e| ParseError::InvalidArgument {
            verb: "glob",
            name: "pattern",
            reason: e.message().to_string(),
        })
}
```

Change the `glob` arm to:

```rust
let pattern = parse_glob_pattern(args.positional("glob", 0, "pattern")?)?;
```

- [ ] **Step 4: Run tests to verify GREEN and commit**

Run:

```bash
cargo test -p fslite-command
git add crates/fslite-command/src/parser.rs crates/fslite-command/tests/parser.rs crates/fslite-command/tests/parser_security.rs
git commit -m "feat(fslite): accept workspace-relative glob patterns"
```

Expected: every `fslite-command` test passes before the commit.

---

### Task 3: Explain Root-Relative Paths in Help and Quick Starts

**Files:**
- Modify: `crates/fslite-command/tests/help.rs`
- Modify: `crates/fslite-command/src/help.rs`
- Modify: `crates/fslite-cli/src/cli.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `VERB_HELP` and Clap command metadata.
- Produces: discoverable help stating that paths may be absolute or workspace-root-relative.

- [ ] **Step 1: Write a failing help test**

Add to `crates/fslite-command/tests/help.rs`:

```rust
#[test]
fn path_help_does_not_require_absolute_cli_input() {
    let mkdir = VERB_HELP.iter().find(|entry| entry.name == "mkdir").unwrap();
    assert!(mkdir.summary.contains("workspace-root-relative"));

    let glob = VERB_HELP.iter().find(|entry| entry.name == "glob").unwrap();
    assert!(glob.summary.contains("docs/*.txt"));
    assert!(!glob.summary.contains("absolute paths"));
}
```

- [ ] **Step 2: Run the test to verify RED**

Run:

```bash
cargo test -p fslite-command --test help path_help_does_not_require_absolute_cli_input
```

Expected: FAIL on the current `mkdir` and `glob` summaries.

- [ ] **Step 3: Update help text**

Change the two entries in `crates/fslite-command/src/help.rs`:

```rust
VerbHelp {
    name: "glob",
    summary: "Match path shapes from the workspace root (e.g. `docs/*.txt`).",
    flags: &["cursor", "limit"],
},
VerbHelp {
    name: "mkdir",
    summary: "Create a directory (paths may be workspace-root-relative).",
    flags: &["parents", "exist-ok", "expected-revision"],
},
```

Add this field to the `#[command(...)]` metadata on `Cli`:

```rust
after_long_help = "Virtual paths may be absolute (/docs/file.txt) or relative to the workspace root (docs/file.txt)."
```

- [ ] **Step 4: Update README examples**

Change the Quick start and named-workspace examples to use relative paths:

```console
cargo install fslite
fslite mkdir docs
fslite write docs/hello.txt --text=hello
fslite cat docs/hello.txt
```

After the first code block, add:

```markdown
CLI paths may be absolute (`/docs/hello.txt`) or relative to the active
workspace root (`docs/hello.txt`). fslite has no virtual current-directory
state, so relative paths always start at that workspace root.
```

- [ ] **Step 5: Verify help and commit**

Run:

```bash
cargo test -p fslite-command --test help
cargo test -p fslite cli::tests
cargo run -q -p fslite -- --help
cargo run -q -p fslite -- help glob
git add crates/fslite-command/src/help.rs crates/fslite-command/tests/help.rs crates/fslite-cli/src/cli.rs README.md
git commit -m "docs(fslite): explain workspace-relative paths"
```

Expected: tests pass; both path forms appear in `--help`; `help glob` shows a relative example.

---

### Task 4: Workspace Verification

**Files:**
- Verify all modified files.

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: a formatted, lint-clean, fully tested branch ready for review.

- [ ] **Step 1: Format and check the diff**

Run:

```bash
cargo fmt --all -- --check
git diff --check
git status --short
```

Expected: formatting and whitespace checks pass; only intentional changes and the user's pre-existing `.DS_Store`/`.worktrees/` entries appear.

- [ ] **Step 2: Run Clippy and the complete workspace test suite**

Run:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: both commands exit 0 without warnings or test failures.

- [ ] **Step 3: Perform a clean install smoke test**

Run:

```bash
SMOKE_ROOT=$(mktemp -d)
CARGO_TARGET_DIR="$SMOKE_ROOT/target" cargo install --path crates/fslite-cli --root "$SMOKE_ROOT/install"
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" mkdir docs
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" write docs/hello.txt --text=hello
FSLITE_CONFIG_DIR="$SMOKE_ROOT/config" FSLITE_DATA_DIR="$SMOKE_ROOT/data" "$SMOKE_ROOT/install/bin/fslite" cat docs/hello.txt
```

Expected: the first command emits the approved bootstrap notice once, the following commands emit no notice, and the final command prints `hello`.

- [ ] **Step 4: Commit only if formatting changed tracked files**

```bash
git add crates/fslite-command crates/fslite-cli README.md
git commit -m "style: format relative path changes"
```

If formatting produced no tracked changes, do not create an empty commit.
