//! Guards that the per-verb metadata table (used by `fslite help`) stays
//! in sync with the parser grammar:
//!   1. every verb the parser recognizes appears in `VERB_HELP`,
//!   2. `print_verb_help` reports missing verbs distinctly from present
//!      ones,
//!   3. the metadata set is stable in size.

use fslite_command::{VERB_HELP, parser::parse, print_verb_help};

/// One canonical sample line per verb. Each line must parse without
/// error AND every verb name extracted from the resulting `Command`
/// variant must appear in `VERB_HELP`.
///
/// Lines are minimal — a verb plus the fewest flags/positionals it can
/// accept. The exact shape matters less than that the verb is reachable
/// from a user-typed command.
///
/// `batch` reads its file at parse time, so the sample path is computed
/// inside the test (which writes a minimal `[]` JSON batch to a temp
/// path) rather than being a string literal here.
fn sample_verb_lines() -> Vec<String> {
    let batch_path = std::env::temp_dir().join("fslite-help-test-batch.json");
    std::fs::write(&batch_path, "[]").expect("write batch sample file");
    let batch_line = format!("batch --file={}", batch_path.display());
    vec![
        "usage".into(),
        "stat /f".into(),
        "exists /f".into(),
        "ls /".into(),
        "tree /".into(),
        "mkdir /a".into(),
        "cat /f".into(),
        "write /f --text=hi".into(),
        "write-at /f --offset=0 --text=hi".into(),
        "append /f --text=hi".into(),
        "truncate /f --length=0".into(),
        "touch /f".into(),
        "cp /a /b".into(),
        "mv /a /b".into(),
        "rm /a".into(),
        "ln /target /link".into(),
        "readlink /link".into(),
        "trash /a".into(),
        "trash-ls".into(),
        "restore 019fbe44-865f-7222-bcfb-78895800892b".into(), // TrashId is a UUID
        "purge 019fbe44-865f-7222-bcfb-78895800892b".into(),
        "setattr /f k --value=YQ==".into(),
        "rmattr /f k".into(),
        "glob '/logs/*.txt'".into(),
        "find /".into(),
        "grep / needle".into(),
        "changes".into(),
        batch_line,
    ]
}

/// For every parser verb, return the canonical CLI verb name. Tied to
/// the 28-variant `Command` enum (`crates/fslite-command/src/command.rs`).
fn verb_name_of(cmd: &fslite_command::Command) -> &'static str {
    use fslite_command::Command::*;
    match cmd {
        WorkspaceUsage => "usage",
        Stat { .. } => "stat",
        Exists { .. } => "exists",
        ReadDir { .. } => "ls",
        Tree { .. } => "tree",
        Mkdir { .. } => "mkdir",
        Read { .. } => "cat",
        Write { .. } => "write",
        WriteAt { .. } => "write-at",
        Append { .. } => "append",
        Truncate { .. } => "truncate",
        Touch { .. } => "touch",
        Copy { .. } => "cp",
        Move { .. } => "mv",
        Remove { .. } => "rm",
        Symlink { .. } => "ln",
        ReadLink { .. } => "readlink",
        Trash { .. } => "trash",
        ListTrash { .. } => "trash-ls",
        Restore { .. } => "restore",
        Purge { .. } => "purge",
        SetAttribute { .. } => "setattr",
        RemoveAttribute { .. } => "rmattr",
        Glob { .. } => "glob",
        Find { .. } => "find",
        SearchContent { .. } => "grep",
        Changes { .. } => "changes",
        Batch(_) => "batch",
    }
}

#[test]
fn every_parser_verb_has_help_metadata() {
    let lines = sample_verb_lines();
    let mut seen = std::collections::BTreeSet::new();
    for line in &lines {
        let cmd = parse(line).unwrap_or_else(|_| panic!("sample line must parse: {line:?}"));
        let name = verb_name_of(&cmd);
        assert!(
            VERB_HELP.iter().any(|v| v.name == name),
            "missing help metadata for verb {name:?} (sample line: {line:?})"
        );
        assert!(
            seen.insert(name),
            "duplicate verb {name:?} in sample_verb_lines"
        );
    }
    assert_eq!(
        seen.len(),
        VERB_HELP.len(),
        "VERB_HELP length ({}) differs from parser-verb count ({})",
        VERB_HELP.len(),
        seen.len(),
    );
}

#[test]
fn print_verb_help_returns_some_for_known_verb() {
    let entry = print_verb_help("write");
    assert_eq!(entry.map(|v| v.name), Some("write"));
}

#[test]
fn print_verb_help_returns_none_for_unknown_verb() {
    assert!(print_verb_help("nonexistent").is_none());
}

#[test]
fn verb_count_is_stable() {
    // 28 verbs = 28 trait methods on `FileSystem` + 1 (WorkspaceUsage).
    // Pin this number so a regression in either the parser or the
    // metadata table is loudly visible.
    assert_eq!(VERB_HELP.len(), 28);
}
