use std::collections::{BTreeMap, BTreeSet, HashMap};

use async_trait::async_trait;
use axum::extract::{FromRef, FromRequestParts, Path};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use fslite_core::{Capability, RequestContext, WorkspaceId};
use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

/// The workspace and capabilities a credential resolves to.
#[derive(Clone, Debug)]
pub struct AuthenticatedActor {
    /// The single workspace this credential is scoped to.
    pub workspace_id: WorkspaceId,
    /// The capabilities granted within that workspace.
    pub capabilities: BTreeSet<Capability>,
    /// Safe actor fields copied verbatim into `RequestContext::actor_metadata`.
    pub actor_metadata: BTreeMap<String, Value>,
}

/// Resolves inbound request headers to an authenticated actor.
///
/// Implementations may look up bearer tokens, verify JWTs, call an external
/// identity service, etc. `fslite-server` ships one reference implementation,
/// [`BearerTokenAuthProvider`]; production deployments are expected to
/// provide their own.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticates a request from its headers alone.
    async fn authenticate(&self, headers: &HeaderMap) -> Result<AuthenticatedActor, ApiError>;
}

/// A static bearer-token credential store: `Authorization: Bearer <token>`.
pub struct BearerTokenAuthProvider {
    tokens: HashMap<String, AuthenticatedActor>,
}

impl BearerTokenAuthProvider {
    /// Builds a provider from a fixed token → actor map.
    pub fn new(tokens: HashMap<String, AuthenticatedActor>) -> Self {
        Self { tokens }
    }
}

#[async_trait]
impl AuthProvider for BearerTokenAuthProvider {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<AuthenticatedActor, ApiError> {
        let header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthenticated("missing authorization header".into()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthenticated("expected a Bearer token".into()))?;

        self.tokens
            .get(token)
            .cloned()
            .ok_or_else(|| ApiError::Unauthenticated("unrecognized token".into()))
    }
}

/// An extractor that authenticates the request and enforces that the
/// authenticated actor's workspace matches the `{workspace_id}` path segment.
pub struct Ctx(pub RequestContext);

impl<S> FromRequestParts<S> for Ctx
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let actor = app_state.auth.authenticate(&parts.headers).await?;

        // `Path<WorkspaceId>` only deserializes cleanly when the matched
        // route has exactly one captured segment. Routes nested under
        // `/fs/{*path}` also capture the wildcard tail, so we read the raw
        // param map instead and parse the `workspace_id` entry by name —
        // this works regardless of how many other segments the route captures.
        let Path(raw_params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::MalformedBody("invalid path parameters".into()))?;
        let raw_workspace_id = raw_params
            .get("workspace_id")
            .ok_or_else(|| ApiError::MalformedBody("missing workspace id in path".into()))?;
        let workspace_id = WorkspaceId::parse(raw_workspace_id)
            .map_err(|_| ApiError::MalformedBody("invalid workspace id in path".into()))?;

        if actor.workspace_id != workspace_id {
            return Err(ApiError::WorkspaceMismatch);
        }

        Ok(Ctx(RequestContext::new(
            workspace_id,
            actor.actor_metadata,
            actor.capabilities,
        )))
    }
}
