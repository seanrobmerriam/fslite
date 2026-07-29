use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Identifies an isolated filesystem workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    /// Creates a time-ordered UUIDv7 workspace identifier.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Parses a UUID workspace identifier.
    pub fn parse(input: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(input).map(Self)
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Identifies a filesystem node within a workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Creates a time-ordered UUIDv7 node identifier.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Parses a UUID node identifier.
    pub fn parse(input: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(input).map(Self)
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A positive, monotonically increasing node revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// The first valid revision.
    pub const INITIAL: Self = Self(1);

    /// Returns a revision when `value` is nonzero.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Returns the next revision.
    ///
    /// # Panics
    ///
    /// Panics if the revision is already `u64::MAX`.
    pub const fn next(self) -> Self {
        match self.0.checked_add(1) {
            Some(value) => Self(value),
            None => panic!("revision overflow"),
        }
    }

    /// Returns the underlying positive integer value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The type of a filesystem node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A container of child nodes.
    Directory,
    /// A node containing byte content.
    File,
    /// A node referring to another path.
    Symlink,
}

/// Transport-independent metadata for a filesystem node.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Node {
    /// The workspace that owns this node.
    pub workspace_id: WorkspaceId,
    /// The stable identity of this node.
    pub id: NodeId,
    /// The node's parent, or `None` for a workspace root.
    pub parent_id: Option<NodeId>,
    /// The basename used by the parent to address this node.
    pub name: String,
    /// The node's filesystem type.
    pub kind: NodeKind,
    /// The logical byte size of the node's content.
    pub logical_size: u64,
    /// The Unix timestamp in milliseconds when the node was created.
    pub created_at_ms: i64,
    /// The Unix timestamp in milliseconds when the node was last modified.
    pub modified_at_ms: i64,
    /// The Unix timestamp in milliseconds when the node was last accessed.
    pub accessed_at_ms: i64,
    /// The current optimistic-concurrency revision.
    pub revision: Revision,
    /// Application-defined metadata associated with this node.
    pub attributes: BTreeMap<String, Value>,
}
