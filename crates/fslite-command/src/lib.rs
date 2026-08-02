//! A typed operation codec, constrained shell-like parser/renderer, and
//! local/remote executors for driving any `fslite_core::FileSystem` backend
//! from a command line.

mod bytes_b64;
mod command;
mod executor;
pub mod help;
pub mod lexer;
mod local;
mod output;
pub mod parser;
mod remote;
pub mod render;

pub use command::Command;
pub use executor::Executor;
pub use help::{VERB_HELP, VerbHelp, print_verb_help, print_verb_table};
pub use local::LocalExecutor;
pub use output::CommandOutput;
pub use remote::RemoteExecutor;
pub use render::{
    render_human, render_json, sanitize_for_terminal, sanitize_name, sanitize_preview,
};
