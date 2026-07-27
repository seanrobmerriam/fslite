//! A typed operation codec, constrained shell-like parser/renderer, and
//! local/remote executors for driving any `fslite_core::FileSystem` backend
//! from a command line.

mod bytes_b64;
mod command;
mod executor;
pub mod lexer;
mod local;
mod output;
pub mod parser;

pub use command::Command;
pub use executor::Executor;
pub use local::LocalExecutor;
pub use output::CommandOutput;
