//! A typed operation codec, constrained shell-like parser/renderer, and
//! local/remote executors for driving any `fslite_core::FileSystem` backend
//! from a command line.

mod bytes_b64;
mod command;
mod output;

pub use command::Command;
pub use output::CommandOutput;
