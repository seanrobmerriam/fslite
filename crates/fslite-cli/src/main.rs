use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use fslite_command::{render_human, render_json, CommandOutput, Executor, LocalExecutor, RemoteExecutor};
use fslite_core::{FileSystem, RequestContext, WorkspaceId};
use fslite_sqlite::SqliteFileSystem;

/// `fslite-cli` — a constrained shell-like client for `fslite`, local or remote.
///
/// The outer flags below (parsed by `clap`) select *how* to connect; the
/// verb and its arguments (everything after them) are parsed by
/// `fslite-command`'s own hand-written grammar, not by `clap` — the two
/// parsers are deliberately separate.
#[derive(Parser)]
#[command(name = "fslite-cli")]
struct Cli {
    /// Path to a local SQLite database (local mode).
    #[arg(long, conflicts_with_all = ["memory", "server"])]
    db: Option<PathBuf>,

    /// Use a private in-memory database (local mode).
    #[arg(long, conflicts_with_all = ["db", "server"])]
    memory: bool,

    /// Base URL of a running fslite-server (remote mode).
    #[arg(long, conflicts_with_all = ["db", "memory"])]
    server: Option<String>,

    /// Bearer token for remote mode.
    #[arg(long, requires = "server")]
    token: Option<String>,

    /// Creates a new workspace, prints its id, and exits.
    #[arg(long)]
    create_workspace: bool,

    /// The workspace to operate in (required unless --create-workspace).
    #[arg(long)]
    workspace: Option<String>,

    /// Reads commands from stdin, one per line, until EOF or `exit`.
    #[arg(long)]
    repl: bool,

    /// Renders output as JSON instead of human-readable text.
    #[arg(long)]
    json: bool,

    /// The command verb and its arguments (one-shot mode only).
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.create_workspace {
        let fs = open_local(&cli).await?;
        let workspace = fs.create_workspace(Default::default()).await?;
        println!("{}", workspace.id);
        return Ok(());
    }

    let executor: Arc<dyn Executor> = if let Some(server) = &cli.server {
        let token = cli.token.clone().ok_or("remote mode requires --token")?;
        Arc::new(RemoteExecutor::new(server.clone(), token))
    } else {
        let fs = open_local(&cli).await?;
        Arc::new(LocalExecutor::new(Arc::new(fs) as Arc<dyn FileSystem>))
    };

    let workspace_id: WorkspaceId = WorkspaceId::parse(
        cli.workspace
            .as_deref()
            .ok_or("--workspace is required (or use --create-workspace first)")?,
    )
    .map_err(|_| "invalid --workspace id")?;
    let ctx = RequestContext::trusted(workspace_id);

    if cli.repl {
        run_repl(executor.as_ref(), &ctx, cli.json).await;
        return Ok(());
    }

    if cli.command.is_empty() {
        return Err("no command given (pass a verb, or use --repl)".into());
    }
    let line = quote_line(&cli.command);
    run_line(executor.as_ref(), &ctx, &line, cli.json).await;
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
    words.iter().map(|word| quote_word(word)).collect::<Vec<_>>().join(" ")
}

fn quote_word(word: &str) -> String {
    let needs_quoting = word.is_empty()
        || word.chars().any(|ch| ch.is_whitespace() || ch == '\'' || ch == '"' || "|;&<>`".contains(ch));
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

async fn open_local(cli: &Cli) -> Result<SqliteFileSystem, Box<dyn std::error::Error>> {
    if cli.memory {
        Ok(SqliteFileSystem::open_in_memory(Default::default()).await?)
    } else {
        let path = cli.db.clone().ok_or("local mode requires --db <path> or --memory")?;
        Ok(SqliteFileSystem::open(path, Default::default()).await?)
    }
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
            eprintln!("error: {} ({:?})", err.message(), err.code());
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
                Err(err) => eprintln!("error: {} ({:?})", err.message(), err.code()),
            },
            Err(err) => eprintln!("parse error: {err:?}"),
        }
        print!("fslite> ");
        std::io::stdout().flush().ok();
    }
}
