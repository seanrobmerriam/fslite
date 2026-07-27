use async_trait::async_trait;
use fslite_core::{FsResult, RequestContext};

use crate::{Command, CommandOutput};

/// Executes one typed [`Command`] against some backend, local or remote.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Runs `command` under `ctx` and returns its typed result.
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput>;
}
