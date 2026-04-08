//! Backend-agnostic conformance suite for [`fslite_core::FileSystem`]
//! implementations.
//!
//! This crate does not duplicate a backend's own exhaustive test suite; it
//! captures the canonical contract every implementation must satisfy, so a
//! new backend can prove basic compliance by implementing
//! [`ConformanceFactory`] and calling [`run_conformance`].

mod attributes;
mod batches;
mod changes;
mod directories;
mod files;
mod links;
mod mutations;
mod paths;
mod search;
mod security;
mod trash;

use fslite_core::{FileSystem, WorkspaceId};

/// Constructs fresh, isolated backend instances and workspaces for one
/// conformance case.
///
/// Workspace lifecycle (creating/naming a workspace) is backend-specific and
/// intentionally not part of the canonical [`FileSystem`] contract, so it
/// cannot be expressed generically over `dyn FileSystem`. Each conformance
/// case instead calls [`fresh`](ConformanceFactory::fresh) to obtain a new
/// backend and immediately calls
/// [`workspace`](ConformanceFactory::workspace) on that same instance before
/// calling `fresh` again for the next case; implementations may rely on
/// this fresh-then-workspace pairing happening without interleaving (for
/// example, by tracking the most recently created workspace in interior
/// state rather than deriving it from the `fs` argument's identity).
#[async_trait::async_trait]
pub trait ConformanceFactory: Send + Sync {
    /// Returns a new, empty backend instance.
    async fn fresh(&self) -> Box<dyn FileSystem>;
    /// Returns a workspace to use on the backend most recently returned by
    /// [`fresh`](ConformanceFactory::fresh).
    async fn workspace(&self, fs: &dyn FileSystem) -> WorkspaceId;
}

/// Runs every conformance case against `factory`, panicking on the first
/// violated guarantee.
pub async fn run_conformance(factory: &dyn ConformanceFactory) {
    paths::run(factory).await;
    directories::run(factory).await;
    files::run(factory).await;
    mutations::run(factory).await;
    links::run(factory).await;
    trash::run(factory).await;
    attributes::run(factory).await;
    batches::run(factory).await;
    search::run(factory).await;
    changes::run(factory).await;
    security::run(factory).await;
}
