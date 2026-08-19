use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use fslite_command::{
    CommandOutput, Executor, LocalExecutor, RemoteExecutor, render_human, render_json,
};
use fslite_core::{FileSystem, RequestContext, WorkspaceId};
use fslite_sqlite::SqliteFileSystem;

mod bootstrap;
mod cli;
mod context;
mod paths;
mod persistence;
mod registry;
#[cfg(test)]
mod test_support;

use cli::{Action, Cli};
use context::Context;
use registry::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // `help` is read-only metadata about the CLI itself — dispatch it
    // before any workspace/executor setup so a user can run
    // `fslite help` (or `fslite help <verb>`) without configuring
    // --db/--memory/--server/--workspace first.
    if let Some(Action::Help { verb }) = &cli.action {
        return handle_help(verb.as_deref());
    }

    match &cli.action {
        Some(Action::Create {
            name,
            file,
            workspace_name,
        }) => return create_filesystem(name, file, workspace_name.as_deref()).await,
        Some(Action::Delete { name, yes }) => return delete_filesystem(name, *yes),
        Some(Action::Use {
            name,
            workspace_name,
        }) => return use_context(name, workspace_name),
        _ => {}
    }

    let eligible_for_bootstrap = matches!(&cli.action, Some(Action::Verb(_)))
        && cli.db.is_none()
        && !cli.memory
        && cli.server.is_none()
        && cli.filesystem.is_none()
        && !cli.create_workspace;

    if eligible_for_bootstrap {
        let outcome = bootstrap::ensure_default().await?;
        if outcome.created {
            eprintln!("{}", bootstrap::NOTICE);
        }
    }

    let (target, filesystem_source) = resolve_target(&cli)?;

    if cli.create_workspace {
        let fs = open_target(&target).await?;
        let workspace = fs.create_workspace(Default::default()).await?;
        println!("{}", workspace.id);
        return Ok(());
    }

    let executor: Arc<dyn Executor> = if let Target::Remote(server) = &target {
        let token = cli.token.clone().ok_or("remote mode requires --token")?;
        Arc::new(RemoteExecutor::new(server.clone(), token))
    } else {
        let fs = open_target(&target).await?;
        Arc::new(LocalExecutor::new(Arc::new(fs) as Arc<dyn FileSystem>))
    };

    let workspace_id = resolve_workspace(&cli, &filesystem_source)?;
    let ctx = RequestContext::trusted(workspace_id);

    if cli.repl {
        run_repl(executor.as_ref(), &ctx, cli.json).await;
        return Ok(());
    }

    let words: &[String] = match &cli.action {
        Some(Action::Verb(words)) => words,
        _ => &[],
    };
    if words.is_empty() {
        return Err("no command given (pass a verb, or use --repl)".into());
    }
    let line = quote_line(words);
    run_line(executor.as_ref(), &ctx, &line, cli.json).await;
    Ok(())
}

/// Where a `FileSystem` connection should be opened from, resolved once per
/// invocation from (in precedence order) explicit `--db`/`--memory`/
/// `--server`, explicit `--filesystem <name>`, or the persisted context —
/// exactly mirroring `resolve_workspace`'s precedence for the workspace
/// half of the same decision.
enum Target {
    Local(PathBuf),
    Memory,
    Remote(String),
}

/// Which provenance (if any) produced a resolved filesystem name.
/// `resolve_workspace`'s fallback to the persisted context's *workspace*
/// name is only correct when the filesystem name it's being resolved
/// against came from that *same* persisted context — mixing an explicit
/// `--filesystem <name>` override with a workspace silently inherited from
/// a stale `fslite use` would resolve the wrong workspace against the
/// right filesystem (a real filesystem override silently paired with the
/// wrong workspace, not a data-loss bug, but a misleading one). Keeping
/// this as three explicit states (rather than a bare `Option<String>`)
/// makes that distinction impossible to lose at the call site.
enum FilesystemSource {
    /// The target never touched the registry at all (`--db`/`--memory`/
    /// `--server`), so there is no filesystem name to resolve a workspace
    /// name against.
    None,
    /// `--filesystem <name>` was given explicitly on this invocation.
    Explicit(String),
    /// The name came from the persisted context (set by `fslite use`).
    FromContext(String),
}

impl FilesystemSource {
    fn name(&self) -> Option<&str> {
        match self {
            FilesystemSource::None => None,
            FilesystemSource::Explicit(name) | FilesystemSource::FromContext(name) => Some(name),
        }
    }
}

/// Resolves which database this invocation targets, and (for local
/// targets reached via `--filesystem` or the persisted context) which
/// registered filesystem name that was *and where it came from*, so
/// `resolve_workspace` can look up a workspace *name* against it without
/// conflating an explicit override with an inherited default. Returns
/// `FilesystemSource::None` when the target came from a raw
/// `--db`/`--memory`/`--server` flag — those never touch the registry, so
/// a plain `--workspace <uuid>` continues to work with zero
/// registry/context involvement, exactly as it does today.
fn resolve_target(cli: &Cli) -> Result<(Target, FilesystemSource), Box<dyn std::error::Error>> {
    if let Some(db) = &cli.db {
        return Ok((Target::Local(db.clone()), FilesystemSource::None));
    }
    if cli.memory {
        return Ok((Target::Memory, FilesystemSource::None));
    }
    if let Some(server) = &cli.server {
        return Ok((Target::Remote(server.clone()), FilesystemSource::None));
    }
    if let Some(name) = &cli.filesystem {
        let registry = Registry::load()?;
        let path = registry
            .filesystem_path(name)
            .ok_or_else(|| format!("no registered filesystem named {name:?}"))?
            .to_path_buf();
        return Ok((
            Target::Local(path),
            FilesystemSource::Explicit(name.clone()),
        ));
    }
    let context = Context::load()?;
    let name = context.filesystem.ok_or(
        "no filesystem selected: pass --db/--memory/--server/--filesystem, or run `fslite use <filesystem> -w <workspace>` first",
    )?;
    let registry = Registry::load()?;
    let path = registry
        .filesystem_path(&name)
        .ok_or_else(|| {
            format!(
                "the active filesystem {name:?} is no longer registered (it may have been deleted) — run `fslite use` again"
            )
        })?
        .to_path_buf();
    Ok((Target::Local(path), FilesystemSource::FromContext(name)))
}

async fn open_target(target: &Target) -> Result<SqliteFileSystem, Box<dyn std::error::Error>> {
    match target {
        Target::Local(path) => Ok(SqliteFileSystem::open(path, Default::default()).await?),
        Target::Memory => Ok(SqliteFileSystem::open_in_memory(Default::default()).await?),
        Target::Remote(_) => Err(
            "this operation requires a local database (--db/--memory/--filesystem), not --server"
                .into(),
        ),
    }
}

/// Resolves the target workspace id: an explicit `--workspace` is tried as
/// a raw `WorkspaceId` first — unconditionally, before touching the
/// registry at all — so a plain `--db <path> --workspace <uuid>`
/// invocation (as used throughout this crate's existing tests) never
/// depends on `--filesystem` or any registered name. Only when that parse
/// fails does `--workspace` get treated as a name, resolved against
/// `filesystem_source`'s name (whatever its provenance).
///
/// Absent `--workspace` entirely, this only falls back to the persisted
/// context's workspace name when the filesystem name *also* came from
/// that context (`FilesystemSource::FromContext`). When `--filesystem
/// <name>` was given explicitly and `--workspace` was not, falling back to
/// the persisted context's workspace would silently pair an
/// explicitly-chosen filesystem with a workspace name left over from a
/// *different* `fslite use` — so that combination is a hard error instead.
fn resolve_workspace(
    cli: &Cli,
    filesystem_source: &FilesystemSource,
) -> Result<WorkspaceId, Box<dyn std::error::Error>> {
    if let Some(reference) = &cli.workspace {
        if let Ok(id) = WorkspaceId::parse(reference) {
            return Ok(id);
        }
        let name = filesystem_source.name().ok_or_else(|| {
            format!(
                "{reference:?} is not a valid workspace id, and no named filesystem is selected to resolve it as a name"
            )
        })?;
        let registry = Registry::load()?;
        return registry
            .resolve_workspace_name(name, reference)
            .ok_or_else(|| {
                format!("no workspace named {reference:?} registered under filesystem {name:?}")
                    .into()
            });
    }

    if let FilesystemSource::Explicit(name) = filesystem_source {
        return Err(format!(
            "--filesystem {name:?} was given without --workspace — pass --workspace <name-or-id> too, or use `fslite use` to set both persistently"
        )
        .into());
    }

    let context = Context::load()?;
    let workspace_name = context.workspace.ok_or(
        "no workspace selected: pass --workspace, or run `fslite use <filesystem> -w <workspace>` first",
    )?;
    let name = filesystem_source
        .name()
        .ok_or("the active context has a workspace but no filesystem — run `fslite use` again")?;
    let registry = Registry::load()?;
    registry
        .resolve_workspace_name(name, &workspace_name)
        .ok_or_else(|| {
            format!(
                "the active workspace {workspace_name:?} is no longer registered under filesystem {name:?}"
            )
            .into()
        })
}

async fn create_filesystem(
    name: &str,
    file: &std::path::Path,
    workspace_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::load()?;
    if registry.filesystem_exists(name) {
        return Err(format!("a filesystem named {name:?} is already registered").into());
    }
    if file.exists() {
        return Err(format!(
            "{file:?} already exists — refusing to overwrite or silently adopt an existing file; choose a different path or remove it first"
        )
        .into());
    }

    let fs = SqliteFileSystem::open(file, Default::default()).await?;
    let absolute_path = std::fs::canonicalize(file)?;
    registry.register_filesystem(name.to_string(), absolute_path.clone());
    println!(
        "created filesystem {name:?} at {}",
        fslite_command::render::sanitize_name(&absolute_path.display().to_string())
    );

    if let Some(workspace_name) = workspace_name {
        let workspace = fs.create_workspace(Default::default()).await?;
        registry.register_workspace(name, workspace_name.to_string(), workspace.id);
        println!("created workspace {workspace_name:?} ({})", workspace.id);
    }

    registry.save()?;
    Ok(())
}

fn delete_filesystem(name: &str, yes: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::load()?;
    let path = registry
        .filesystem_path(name)
        .ok_or_else(|| format!("no registered filesystem named {name:?}"))?
        .to_path_buf();

    if !yes {
        let workspace_names = registry.workspace_names(name);
        println!(
            "This will permanently delete {} and forget {} registered workspace(s) ({}).",
            fslite_command::render::sanitize_name(&path.display().to_string()),
            workspace_names.len(),
            fslite_command::render::sanitize_name(&workspace_names.join(", "))
        );
        print!("Type the filesystem name ({name:?}) to confirm: ");
        std::io::stdout().flush().ok();
        let mut confirmation = String::new();
        std::io::stdin().lock().read_line(&mut confirmation)?;
        if confirmation.trim() != name {
            return Err("confirmation did not match — nothing was deleted".into());
        }
    }

    match std::fs::remove_file(&path) {
        Ok(()) => println!(
            "deleted {}",
            fslite_command::render::sanitize_name(&path.display().to_string())
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "{} was already gone; forgetting it anyway",
                fslite_command::render::sanitize_name(&path.display().to_string())
            );
        }
        Err(err) => return Err(err.into()),
    }

    registry.remove_filesystem(name);
    registry.save()?;
    Context::clear_if_filesystem(name)?;
    Ok(())
}

fn use_context(name: &str, workspace_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::load()?;
    if !registry.filesystem_exists(name) {
        return Err(format!("no registered filesystem named {name:?}").into());
    }
    if !registry.workspace_exists(name, workspace_name) {
        return Err(format!(
            "no workspace named {workspace_name:?} registered under filesystem {name:?}"
        )
        .into());
    }

    let context = Context {
        filesystem: Some(name.to_string()),
        workspace: Some(workspace_name.to_string()),
    };
    context.save()?;
    println!("now using filesystem {name:?}, workspace {workspace_name:?}");
    Ok(())
}

/// Re-joins the outer argv's trailing command words into one line for
/// `fslite_command`'s own lexer/parser.
///
/// The OS already split these words on argv boundaries (so a shell-quoted
/// argument like `--text=hello cli` arrives here as a single `String`
/// containing a space); naively joining with `" "` would lose that
/// boundary and let the second-stage lexer re-split it into two tokens.
/// Each word that would be ambiguous once re-tokenized (contains
/// whitespace, a quote character, or one of the lexer's rejected
/// metacharacters) is re-quoted so the round trip through
/// `fslite_command::lexer::tokenize` reconstructs the original word.
fn quote_line(words: &[String]) -> String {
    words
        .iter()
        .map(|word| quote_word(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_word(word: &str) -> String {
    let needs_quoting = word.is_empty()
        || word
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '\'' || ch == '"' || "|;&<>`".contains(ch));
    if !needs_quoting {
        return word.to_string();
    }
    if !word.contains('\'') {
        return format!("'{word}'");
    }
    // The word itself contains a single quote, so single-quoting it would
    // terminate the quoted segment early; fall back to double quotes,
    // backslash-escaping the two characters the lexer's double-quote
    // reader treats specially (`"` and `\`).
    let mut escaped = String::with_capacity(word.len() + 2);
    escaped.push('"');
    for ch in word.chars() {
        if ch == '"' || ch == '\\' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('"');
    escaped
}

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

fn print_output(output: &CommandOutput, json: bool) {
    if json {
        println!("{}", render_json(output));
    } else {
        println!("{}", render_human(output));
    }
}

async fn run_repl(executor: &dyn Executor, ctx: &RequestContext, json: bool) {
    let stdin = std::io::stdin();
    print!("fslite> ");
    std::io::stdout().flush().ok();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            print!("fslite> ");
            std::io::stdout().flush().ok();
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }
        match fslite_command::parser::parse(trimmed) {
            Ok(command) => match executor.execute(ctx, command).await {
                Ok(output) => print_output(&output, json),
                Err(err) => eprintln!(
                    "error: {} ({:?})",
                    fslite_command::render::sanitize_name(err.message()),
                    err.code()
                ),
            },
            Err(err) => eprintln!("parse error: {err:?}"),
        }
        print!("fslite> ");
        std::io::stdout().flush().ok();
    }
}

/// Print per-verb help for the `fslite` CLI without requiring any
/// workspace/database connection. Reads from
/// [`fslite_command::VERB_HELP`], the canonical 28-verb metadata table.
///
/// `fslite help`           — list every verb with one-line summary.
/// `fslite help <verb>`    — print `<verb>`'s summary plus every flag it accepts.
/// `fslite help bogus`     — print "unknown verb" and exit with code 2.
fn handle_help(verb: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    match verb {
        None => {
            println!("fslite verbs:");
            fslite_command::print_verb_table();
            Ok(())
        }
        Some(name) => {
            if fslite_command::print_verb_help(name).is_none() {
                eprintln!("unknown verb: {name:?} (run `fslite help` for the list)");
                std::process::exit(2);
            }
            Ok(())
        }
    }
}
