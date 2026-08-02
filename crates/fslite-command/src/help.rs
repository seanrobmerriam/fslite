//! Per-verb documentation metadata for the `fslite help` command.
//!
//! Each [`VerbHelp`] entry carries the verb's canonical name, a one-line
//! summary, and the list of `--flag` names the parser accepts for that
//! verb (mirroring [`parse`](crate::parser::parse) at
//! `crates/fslite-command/src/parser.rs:177`, where every verb dispatches
//! with `args.check_known_flags(verb, &[flags])`). The renderer uses this
//! to avoid listing flags the verb does not accept.
//!
//! This list is the single source of truth for the `fslite help` output
//! and for the `reference/cli-verbs.md` page in the docs site. When a
//! verb gains or loses a flag in the parser, update the matching entry
//! here in the same commit.

/// Documentation metadata for one CLI verb.
pub struct VerbHelp {
    /// The canonical verb name as spelled on the command line
    /// (e.g. `"write-at"`, `"trash-ls"`).
    pub name: &'static str,
    /// A single-line summary shown in `fslite help` and as the lead line
    /// of `fslite help <verb>`.
    pub summary: &'static str,
    /// The list of `--flag` names this verb accepts. Empty for verbs that
    /// take only positional arguments.
    pub flags: &'static [&'static str],
}

/// The full per-verb metadata table. Order is canonical: alphabetical by
/// `name`. The total count matches the number of variant arms in
/// [`parse`](crate::parser::parse) — 28 verbs.
pub const VERB_HELP: &[VerbHelp] = &[
    VerbHelp {
        name: "append",
        summary: "Append bytes to a regular file (creates if missing).",
        flags: &["text", "expected-revision"],
    },
    VerbHelp {
        name: "batch",
        summary: "Atomically execute a list of metadata operations from a file or stdin.",
        flags: &["file"],
    },
    VerbHelp {
        name: "cat",
        summary: "Stream a regular file's contents to stdout.",
        flags: &["range", "no-follow"],
    },
    VerbHelp {
        name: "changes",
        summary: "Stream the workspace change feed (cursor-paginated).",
        flags: &["after", "cursor", "limit"],
    },
    VerbHelp {
        name: "cp",
        summary: "Copy a node within the workspace.",
        flags: &["recursive", "overwrite", "expected-revision"],
    },
    VerbHelp {
        name: "exists",
        summary: "Test whether a path resolves to a visible node (exit 0 if yes).",
        flags: &["no-follow"],
    },
    VerbHelp {
        name: "find",
        summary: "Match nodes by bounded metadata predicates (kind, size, mtime, name).",
        flags: &[
            "name-contains",
            "kind",
            "min-size",
            "max-size",
            "modified-after",
            "modified-before",
            "cursor",
            "limit",
        ],
    },
    VerbHelp {
        name: "glob",
        summary: "Match absolute paths by shape (e.g. `/logs/*.txt`).",
        flags: &["cursor", "limit"],
    },
    VerbHelp {
        name: "grep",
        summary: "Literal byte matches inside regular files (returns matched ranges).",
        flags: &["cursor", "limit"],
    },
    VerbHelp {
        name: "ln",
        summary: "Create a symbolic link.",
        flags: &["parents", "exist-ok", "expected-revision"],
    },
    VerbHelp {
        name: "ls",
        summary: "List one directory's direct children (cursor-paginated).",
        flags: &["cursor", "limit"],
    },
    VerbHelp {
        name: "mkdir",
        summary: "Create a directory.",
        flags: &["parents", "exist-ok", "expected-revision"],
    },
    VerbHelp {
        name: "mv",
        summary: "Move a node within the workspace.",
        flags: &["overwrite", "expected-revision"],
    },
    VerbHelp {
        name: "purge",
        summary: "Permanently delete a trashed node.",
        flags: &[],
    },
    VerbHelp {
        name: "readlink",
        summary: "Print a symbolic link's stored target.",
        flags: &[],
    },
    VerbHelp {
        name: "restore",
        summary: "Restore a trashed node (optionally to a different path).",
        flags: &["to", "expected-revision"],
    },
    VerbHelp {
        name: "rm",
        summary: "Permanently remove a node and (recursively) its subtree.",
        flags: &["recursive", "expected-revision"],
    },
    VerbHelp {
        name: "rmattr",
        summary: "Remove a custom attribute from a node.",
        flags: &["expected-revision"],
    },
    VerbHelp {
        name: "setattr",
        summary: "Set an opaque custom attribute on a node.",
        flags: &["value", "expected-revision"],
    },
    VerbHelp {
        name: "stat",
        summary: "Print metadata for one path.",
        flags: &["no-follow"],
    },
    VerbHelp {
        name: "touch",
        summary: "Update timestamps or create an empty regular file.",
        flags: &["no-create", "expected-revision"],
    },
    VerbHelp {
        name: "trash",
        summary: "Move a node into recoverable trash.",
        flags: &["expected-revision"],
    },
    VerbHelp {
        name: "trash-ls",
        summary: "List recoverable trash records (cursor-paginated).",
        flags: &["cursor", "limit"],
    },
    VerbHelp {
        name: "tree",
        summary: "Recursively enumerate a subtree (cursor-paginated).",
        flags: &["max-depth", "follow-symlinks", "cursor", "limit"],
    },
    VerbHelp {
        name: "truncate",
        summary: "Set a regular file's logical length.",
        flags: &["length", "expected-revision"],
    },
    VerbHelp {
        name: "usage",
        summary: "Print workspace byte/node usage and quota limits.",
        flags: &[],
    },
    VerbHelp {
        name: "write",
        summary: "Replace a regular file's contents (creates by default).",
        flags: &["text", "no-create", "expected-revision"],
    },
    VerbHelp {
        name: "write-at",
        summary: "Write bytes beginning at a logical offset.",
        flags: &["offset", "text", "no-create", "expected-revision"],
    },
];

/// Print the full verb table to stdout, sorted alphabetically by name.
/// Each line is two-space-indented `<name>  <summary>` with names left-
/// padded to the widest name.
pub fn print_verb_table() {
    let mut entries: Vec<&VerbHelp> = VERB_HELP.iter().collect();
    entries.sort_by_key(|e| e.name);
    let name_width = entries.iter().map(|e| e.name.len()).max().unwrap_or(0);
    for entry in entries {
        println!(
            "  {:<width$}  {}",
            entry.name,
            entry.summary,
            width = name_width,
        );
    }
}

/// Print one verb's help to stdout: summary line, blank line, then either
/// `(none)` for verbs without flags or one `--flag` per line under a
/// `Flags:` header.
///
/// Returns the verb's [`VerbHelp`] if found, or `None` if no entry exists
/// (so the caller can print a distinct "unknown verb" message and choose
/// its exit code).
pub fn print_verb_help(verb: &str) -> Option<&'static VerbHelp> {
    let entry = VERB_HELP.iter().find(|e| e.name == verb)?;
    println!("{}", entry.summary);
    println!();
    println!("Flags:");
    if entry.flags.is_empty() {
        println!("  (none)");
    } else {
        for flag in entry.flags {
            println!("  --{flag}");
        }
    }
    Some(entry)
}
