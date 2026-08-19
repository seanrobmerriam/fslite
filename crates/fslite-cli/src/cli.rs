//! Command-line argument definitions. Kept separate from `main.rs` so the
//! parsing surface (what flags/subcommands exist) is reviewable independent
//! of dispatch logic.
//!
//! The global flags below select *how* to connect and which named
//! filesystem/workspace to target. `create`/`delete`/`use` are real `clap`
//! subcommands managing this crate's local registry/context state
//! (`crate::registry`/`crate::context`); everything else is an opaque
//! data-plane verb line (`mkdir`, `write`, `cat`, `rm`, `ls`, ...) captured
//! by `Action::Verb` via `#[command(external_subcommand)]` and handed,
//! byte-for-byte, to `fslite_command`'s own hand-written grammar — `clap`
//! never inspects or re-parses those words itself.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `fslite` — a constrained shell-like client for `fslite`, local or remote.
#[derive(Parser)]
#[command(
    name = "fslite",
    // The `Action::Help` variant below would otherwise collide with
    // clap's auto-generated `help` subcommand (which prints the same
    // content as the `--help` flag). Disabling that subcommand leaves
    // `--help`/`-h` working (which users reach for by default) while
    // letting `fslite help [<verb>]` print the per-verb table.
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Path to a local SQLite database (local mode).
    #[arg(long, global = true, conflicts_with_all = ["memory", "server", "filesystem"])]
    pub db: Option<PathBuf>,

    /// Use a private in-memory database (local mode).
    #[arg(long, global = true, conflicts_with_all = ["db", "server", "filesystem"])]
    pub memory: bool,

    /// Base URL of a running fslite-server (remote mode).
    #[arg(long, global = true, conflicts_with_all = ["db", "memory", "filesystem"])]
    pub server: Option<String>,

    /// Bearer token for remote mode. Prefer `FSLITE_TOKEN` over this flag:
    /// on Linux, argv (and therefore a flag value) is world-readable via
    /// `/proc/<pid>/cmdline` for the process's lifetime and also lands in
    /// shell history, while an environment variable does not.
    #[arg(
        long,
        global = true,
        env = "FSLITE_TOKEN",
        hide_env_values = true,
        requires = "server"
    )]
    pub token: Option<String>,

    /// A registered filesystem name, overriding the persisted context for
    /// this invocation only (does not call `use` — the persisted context
    /// is left as-is).
    #[arg(long, global = true, conflicts_with_all = ["db", "memory", "server"])]
    pub filesystem: Option<String>,

    /// The target workspace: a raw workspace id, or a name registered
    /// against the selected filesystem.
    #[arg(long, global = true)]
    pub workspace: Option<String>,

    /// Creates a new workspace directly in an already-selected database,
    /// prints its raw id, and exits. Unnamed and unregistered — for
    /// scripting; prefer `create` for the named-registry workflow.
    #[arg(long, global = true)]
    pub create_workspace: bool,

    /// Reads commands from stdin, one per line, until EOF or `exit`.
    #[arg(long, global = true)]
    pub repl: bool,

    /// Renders output as JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Subcommand)]
pub enum Action {
    /// Creates a new filesystem (a SQLite database file) and registers it
    /// under a name, optionally creating a starter workspace inside it.
    Create {
        /// The name to register this filesystem under.
        name: String,
        /// The SQLite database file to create. Must not already exist.
        #[arg(short = 'f', long)]
        file: PathBuf,
        /// If given, also creates and registers a workspace with this name.
        #[arg(short = 'w', long = "workspace-name")]
        workspace_name: Option<String>,
    },
    /// Permanently deletes a registered filesystem's database file and
    /// forgets it (and its workspaces) from the registry.
    Delete {
        /// The registered filesystem name to delete.
        name: String,
        /// Skips the interactive confirmation prompt.
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Sets the default filesystem/workspace used by verb commands that
    /// omit --db/--memory/--server/--filesystem/--workspace.
    Use {
        /// A registered filesystem name.
        name: String,
        /// A workspace name registered against that filesystem.
        #[arg(short = 'w', long = "workspace-name")]
        workspace_name: String,
    },
    /// List every CLI verb, or show one verb's full flag table. Bypasses
    /// the `external_subcommand` catcher below so a user can discover the
    /// verb surface without reading the README first.
    Help {
        /// Optional verb name to show detail for. Omit to list all verbs.
        verb: Option<String>,
    },
    /// Catches every other first word: a data-plane verb (mkdir, touch,
    /// write, rm, ls, ...) and its arguments, passed through untouched to
    /// `fslite_command::parser::parse`.
    #[command(external_subcommand)]
    Verb(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_package_matches_the_installed_command_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "fslite");
    }

    fn parse(args: &[&str]) -> Cli {
        let mut full = vec!["fslite"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap_or_else(|err| panic!("failed to parse {args:?}: {err}"))
    }

    /// Every one of these mirrors a real invocation shape from
    /// `crates/fslite-cli/tests/{e2e_local,e2e_remote,e2e_repl}.rs` — this
    /// is the regression guard for the `external_subcommand` design.
    #[test]
    fn legacy_create_workspace_flag_has_no_positional_action() {
        let cli = parse(&["--db", "X", "--create-workspace"]);
        assert!(cli.create_workspace);
        assert!(cli.action.is_none());
    }

    #[test]
    fn legacy_repl_flag_has_no_positional_action() {
        let cli = parse(&["--db", "X", "--workspace", "ID", "--repl"]);
        assert!(cli.repl);
        assert_eq!(cli.workspace.as_deref(), Some("ID"));
        assert!(cli.action.is_none());
    }

    #[test]
    fn legacy_verb_with_plain_args_is_captured_verbatim() {
        let cli = parse(&["--db", "X", "--workspace", "ID", "mkdir", "/docs"]);
        match cli.action {
            Some(Action::Verb(words)) => assert_eq!(words, vec!["mkdir", "/docs"]),
            other => panic!(
                "expected Action::Verb, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn legacy_json_flag_before_verb_still_applies_globally() {
        let cli = parse(&["--db", "X", "--workspace", "ID", "--json", "usage"]);
        assert!(cli.json);
        match cli.action {
            Some(Action::Verb(words)) => assert_eq!(words, vec!["usage"]),
            other => panic!(
                "expected Action::Verb, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn verb_args_that_look_like_flags_pass_through_untouched() {
        let cli = parse(&[
            "--db",
            "X",
            "--workspace",
            "ID",
            "write",
            "/docs/a.txt",
            "--text=hello cli",
        ]);
        match cli.action {
            Some(Action::Verb(words)) => {
                assert_eq!(words, vec!["write", "/docs/a.txt", "--text=hello cli"]);
            }
            other => panic!(
                "expected Action::Verb, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn verb_args_containing_an_unrelated_long_flag_name_pass_through_untouched() {
        let cli = parse(&["mkdir", "--parents", "/a/b/c"]);
        match cli.action {
            Some(Action::Verb(words)) => assert_eq!(words, vec!["mkdir", "--parents", "/a/b/c"]),
            other => panic!(
                "expected Action::Verb, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn help_subcommand_matches_bare_help() {
        let cli = parse(&["help"]);
        match cli.action {
            Some(Action::Help { verb }) => assert!(verb.is_none(), "got verb={verb:?}"),
            other => panic!(
                "expected Action::Help, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn help_subcommand_with_verb_captures_verb_name() {
        let cli = parse(&["help", "write"]);
        match cli.action {
            Some(Action::Help { verb }) => assert_eq!(verb.as_deref(), Some("write")),
            other => panic!(
                "expected Action::Help, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    /// Regression guard: `help <unknown>` must NOT fall through to the
    /// `external_subcommand` catcher; it is always matched by the
    /// `Action::Help` variant first.
    #[test]
    fn help_subcommand_consumes_unknown_verb_rather_than_externalizing() {
        let cli = parse(&["help", "nonexistent"]);
        match cli.action {
            Some(Action::Help { verb }) => assert_eq!(verb.as_deref(), Some("nonexistent")),
            other => panic!(
                "expected Action::Help, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn create_subcommand_parses_name_file_and_workspace_name() {
        let cli = parse(&["create", "main", "-f", "main.db", "-w", "primary"]);
        match cli.action {
            Some(Action::Create {
                name,
                file,
                workspace_name,
            }) => {
                assert_eq!(name, "main");
                assert_eq!(file, PathBuf::from("main.db"));
                assert_eq!(workspace_name.as_deref(), Some("primary"));
            }
            other => panic!(
                "expected Action::Create, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn use_subcommand_local_workspace_name_does_not_leak_into_global_workspace_flag() {
        // Regression test for the collision documented in this file's
        // design note: a *different* value passed to the top-level
        // `--workspace` flag before the `use` subcommand must not be
        // silently overwritten by `use`'s own `-w`.
        let cli = parse(&[
            "--workspace",
            "GLOBAL-VALUE",
            "use",
            "main",
            "-w",
            "LOCAL-VALUE",
        ]);
        assert_eq!(cli.workspace.as_deref(), Some("GLOBAL-VALUE"));
        match cli.action {
            Some(Action::Use {
                name,
                workspace_name,
            }) => {
                assert_eq!(name, "main");
                assert_eq!(workspace_name, "LOCAL-VALUE");
            }
            other => panic!(
                "expected Action::Use, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    #[test]
    fn delete_subcommand_yes_flag() {
        let cli = parse(&["delete", "main", "-y"]);
        match cli.action {
            Some(Action::Delete { name, yes }) => {
                assert_eq!(name, "main");
                assert!(yes);
            }
            other => panic!(
                "expected Action::Delete, got {other:?}",
                other = debug_action(&other)
            ),
        }
    }

    fn debug_action(action: &Option<Action>) -> &'static str {
        match action {
            Some(Action::Create { .. }) => "Create",
            Some(Action::Delete { .. }) => "Delete",
            Some(Action::Use { .. }) => "Use",
            Some(Action::Help { .. }) => "Help",
            Some(Action::Verb(_)) => "Verb",
            None => "None",
        }
    }
}
