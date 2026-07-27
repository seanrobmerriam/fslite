use std::sync::Arc;

use fslite_core::{FileSystem, WorkspaceId};

use crate::auth::AuthProvider;

/// Shared, cloneable application state handed to every route.
#[derive(Clone)]
pub struct AppState {
    /// The backend-agnostic filesystem every data route is driven through.
    pub fs: Arc<dyn FileSystem>,
    /// Resolves inbound credentials to a workspace and capability set.
    pub auth: Arc<dyn AuthProvider>,
    /// The workspace `/readyz` probes with a cheap `exists(root)` call.
    pub health_workspace: WorkspaceId,
}
