//! Structural guard: `fslite` must never shell out. `fslite-command`
//! already has an equivalent guard scanning its own `src/` for
//! `process::Command`/`Command::new` (see
//! `crates/fslite-command/tests/parser_security.rs`), but that guard only
//! covers `fslite-command`'s source — `fslite`'s `src/main.rs` (the
//! actual binary, and the crate most likely to eventually grow a "just
//! shell out for `$EDITOR`/a pager" feature) was completely unscanned by any
//! test. This closes that gap.
//!
//! `main.rs` legitimately calls `std::process::exit(...)` in a couple of
//! places to set the process exit code on parse/execution errors — that is
//! fine and intentionally not forbidden here. Only the actual
//! subprocess-spawning APIs are forbidden.

#[test]
fn crate_source_never_references_process_command() {
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let forbidden = ["process::Command", "Command::new", "Stdio"];
    for entry in walk(src_dir) {
        let contents = std::fs::read_to_string(&entry).unwrap();
        for needle in forbidden {
            assert!(
                !contents.contains(needle),
                "found a process-spawning call (`{needle}`) in {entry:?} — fslite must never shell out"
            );
        }
    }
}

#[test]
fn user_facing_cargo_commands_use_the_fslite_package_name() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent().unwrap().parent().unwrap();
    for relative in ["README.md", "RELEASE.md", "CHANGELOG.md"] {
        let path = root.join(relative);
        let contents = std::fs::read_to_string(&path).unwrap();
        for obsolete in ["cargo install fslite-cli", "cargo run -p fslite-cli"] {
            assert!(
                !contents.contains(obsolete),
                "{path:?} contains obsolete package command {obsolete:?}"
            );
        }
    }
}

fn walk(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(path.to_str().unwrap()));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}
