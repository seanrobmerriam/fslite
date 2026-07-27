use std::sync::Arc;

use fslite_core::{FileSystem, WorkspaceId};

/// Shared, cloneable application state handed to every route.
#[derive(Clone)]
pub struct AppState {
    /// The backend-agnostic filesystem every data route is driven through.
    pub fs: Arc<dyn FileSystem>,
    /// The workspace `/readyz` probes with a cheap `exists(root)` call.
    pub health_workspace: WorkspaceId,
}
