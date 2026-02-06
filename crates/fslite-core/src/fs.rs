use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    ByteRange, ContentQuery, CopyOptions, CreateOptions, FindQuery, FsResult, LinkTarget,
    MoveOptions, MutationOptions, Node, NodeId, PageRequest, ReadOptions, RemoveOptions, Revision,
    StatOptions, TouchOptions, TreeOptions, VirtualPath, WorkspaceId, WriteOptions,
};

/// A pinned asynchronous stream of filesystem byte chunks.
pub type ByteStream = Pin<Box<dyn Stream<Item = FsResult<Bytes>> + Send + 'static>>;

/// A caller capability enforced within one workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Inspect metadata and read file contents.
    Read,
    /// Create and mutate nodes and file contents.
    Write,
    /// Permanently delete nodes and purge trash entries.
    Delete,
    /// Trash and restore nodes.
    TrashRestore,
    /// Administer workspace-level configuration and lifecycle.
    WorkspaceAdmin,
}

/// Transport-independent authorization and audit context for an operation.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RequestContext {
    /// The workspace in which the operation executes.
    pub workspace_id: WorkspaceId,
    /// Safe host-supplied actor fields suitable for audit records.
    pub actor_metadata: BTreeMap<String, Value>,
    /// The workspace-scoped capabilities granted to the caller.
    pub capabilities: BTreeSet<Capability>,
}

impl RequestContext {
    /// Creates a request context from host-provided actor metadata and capabilities.
    pub fn new(
        workspace_id: WorkspaceId,
        actor_metadata: BTreeMap<String, Value>,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Self {
        Self {
            workspace_id,
            actor_metadata,
            capabilities: capabilities.into_iter().collect(),
        }
    }

    /// Creates an explicit trusted context with every capability.
    pub fn trusted(workspace_id: WorkspaceId) -> Self {
        Self::new(
            workspace_id,
            BTreeMap::new(),
            [
                Capability::Read,
                Capability::Write,
                Capability::Delete,
                Capability::TrashRestore,
                Capability::WorkspaceAdmin,
            ],
        )
    }

    /// Returns whether this context contains a capability.
    pub fn has_capability(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// A source of byte chunks consumed by a streamed write.
pub struct WriteSource {
    stream: ByteStream,
}

impl fmt::Debug for WriteSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WriteSource")
            .finish_non_exhaustive()
    }
}

impl WriteSource {
    /// Wraps a sendable stream of byte chunks and filesystem errors.
    pub fn new(source: impl Stream<Item = FsResult<Bytes>> + Send + 'static) -> Self {
        Self {
            stream: Box::pin(source),
        }
    }

    /// Creates a one-chunk source from in-memory bytes.
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        let bytes = bytes.into();
        Self::new(stream::once(async move { Ok(bytes) }))
    }

    /// Returns the wrapped stream.
    pub fn into_stream(self) -> ByteStream {
        self.stream
    }
}

/// Metadata accompanying a streamed file read.
pub struct FileRead {
    /// The complete logical length of the file.
    pub logical_length: u64,
    /// The revision from which the stream reads.
    pub revision: Revision,
    /// The actual inclusive-start, exclusive-end range returned.
    pub range: ByteRange,
    /// The requested byte chunks.
    pub stream: ByteStream,
}

impl fmt::Debug for FileRead {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileRead")
            .field("logical_length", &self.logical_length)
            .field("revision", &self.revision)
            .field("range", &self.range)
            .field("stream", &"<byte stream>")
            .finish()
    }
}

impl FileRead {
    /// Returns the file's byte stream.
    pub fn into_stream(self) -> ByteStream {
        self.stream
    }
}

/// Logical and quota usage for one workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceUsage {
    /// The workspace described by this snapshot.
    pub workspace_id: WorkspaceId,
    /// Logical bytes reachable through active nodes.
    pub active_logical_bytes: u64,
    /// Logical bytes retained in recoverable trash.
    pub trashed_logical_bytes: u64,
    /// Temporary bytes held internally by incomplete streamed writes.
    ///
    /// This is a read-only accounting metric, not a caller-visible staged
    /// content identifier, upload handle, or staging API.
    pub staged_bytes: u64,
    /// The number of active nodes, including the workspace root.
    pub active_nodes: u64,
    /// The number of nodes retained in recoverable trash.
    pub trashed_nodes: u64,
    /// The configured logical-byte quota.
    pub max_logical_bytes: u64,
    /// The configured node-count quota.
    pub max_nodes: u64,
    /// The configured maximum size of one regular file.
    pub max_file_bytes: u64,
}

/// One entry in a recursive tree traversal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TreeEntry {
    /// The absolute path of the returned node.
    pub path: VirtualPath,
    /// The depth below the traversal root.
    pub depth: u32,
    /// The node metadata.
    pub node: Node,
}

/// One page of results and an optional continuation cursor.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Page<T> {
    /// Results in the backend's documented stable order.
    pub items: Vec<T>,
    /// An opaque cursor for the next page, or `None` at the end.
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Creates a page from result items and an optional continuation cursor.
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

/// Identifies a recoverable trash record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TrashId(Uuid);

impl TrashId {
    /// Creates a time-ordered UUIDv7 trash identifier.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Parses a UUID trash identifier.
    pub fn parse(input: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(input).map(Self)
    }
}

impl Default for TrashId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TrashId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Metadata for a recoverable trashed subtree.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrashEntry {
    /// The stable trash record identifier.
    pub id: TrashId,
    /// The root node of the trashed subtree.
    pub node: Node,
    /// The node's absolute path before it was trashed.
    pub original_path: VirtualPath,
    /// The Unix timestamp in milliseconds when the node was trashed.
    pub trashed_at_ms: i64,
    /// Safe actor fields captured when the node was trashed.
    pub actor_metadata: BTreeMap<String, Value>,
}

/// One literal content-search match.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SearchMatch {
    /// The matching file's metadata.
    pub node: Node,
    /// The matching file's absolute path.
    pub path: VirtualPath,
    /// The exact logical byte range occupied by the match.
    pub range: ByteRange,
    /// A bounded byte preview around the match.
    pub preview: Vec<u8>,
}

/// An opaque continuation cursor for the workspace change feed.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChangeCursor(String);

impl ChangeCursor {
    /// Wraps a backend-produced opaque cursor.
    pub fn new(cursor: impl Into<String>) -> Self {
        Self(cursor.into())
    }

    /// Returns the opaque cursor value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChangeCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The stable category of a committed filesystem change.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A node was created.
    Created,
    /// Node metadata or regular-file contents changed.
    Modified,
    /// A node was copied to a new path.
    Copied,
    /// A node moved to a new path.
    Moved,
    /// A node was permanently removed.
    Removed,
    /// A node entered recoverable trash.
    Trashed,
    /// A node was restored from trash.
    Restored,
    /// A trash record was permanently purged.
    Purged,
    /// A custom attribute was set.
    AttributeSet,
    /// A custom attribute was removed.
    AttributeRemoved,
}

/// One committed, workspace-scoped filesystem change.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Change {
    /// The strictly increasing sequence within the workspace.
    pub sequence: u64,
    /// The change category.
    pub kind: ChangeKind,
    /// The affected node when it remains identifiable.
    pub node_id: Option<NodeId>,
    /// The path before the change, when applicable.
    pub old_path: Option<VirtualPath>,
    /// The path after the change, when applicable.
    pub new_path: Option<VirtualPath>,
    /// The resulting revision, when the affected node remains active.
    pub revision: Option<Revision>,
    /// The Unix timestamp in milliseconds when the change committed.
    pub created_at_ms: i64,
    /// Safe actor fields captured for the mutation.
    pub actor_metadata: BTreeMap<String, Value>,
}

/// A metadata or namespace operation that may participate in an atomic batch.
///
/// Batches never carry file bytes, staged-content identifiers, or upload
/// handles. Content-changing file operations (`write`, `write_at`, `append`,
/// and `truncate`) remain separate atomic filesystem calls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOperation {
    /// Creates a directory.
    Mkdir {
        /// The directory path.
        path: VirtualPath,
        /// Directory creation controls.
        options: CreateOptions,
    },
    /// Touches or creates a regular file.
    Touch {
        /// The file path.
        path: VirtualPath,
        /// Touch controls.
        options: TouchOptions,
    },
    /// Copies a node.
    Copy {
        /// The source path.
        from: VirtualPath,
        /// The destination path.
        to: VirtualPath,
        /// Copy controls.
        options: CopyOptions,
    },
    /// Moves a node.
    Move {
        /// The source path.
        from: VirtualPath,
        /// The destination path.
        to: VirtualPath,
        /// Move controls.
        options: MoveOptions,
    },
    /// Permanently removes a node.
    Remove {
        /// The target path.
        path: VirtualPath,
        /// Removal controls.
        options: RemoveOptions,
    },
    /// Creates a symbolic link.
    Symlink {
        /// The normalized relative or absolute virtual target.
        target: LinkTarget,
        /// The new link path.
        link: VirtualPath,
        /// Link creation controls.
        options: CreateOptions,
    },
    /// Moves a node into recoverable trash.
    Trash {
        /// The target path.
        path: VirtualPath,
        /// Optimistic mutation controls.
        options: MutationOptions,
    },
    /// Restores a recoverable trash record.
    Restore {
        /// The trash record to restore.
        trash: TrashId,
        /// An alternate destination, or the original path when absent.
        destination: Option<VirtualPath>,
        /// Optimistic mutation controls.
        options: MutationOptions,
    },
    /// Permanently purges a trash record.
    Purge {
        /// The trash record to purge.
        trash: TrashId,
    },
    /// Sets a custom node attribute.
    SetAttribute {
        /// The target path.
        path: VirtualPath,
        /// The attribute key.
        key: String,
        /// The opaque attribute value.
        value: Vec<u8>,
        /// Optimistic mutation controls.
        options: MutationOptions,
    },
    /// Removes a custom node attribute.
    RemoveAttribute {
        /// The target path.
        path: VirtualPath,
        /// The attribute key.
        key: String,
        /// Optimistic mutation controls.
        options: MutationOptions,
    },
}

/// The result of one successful atomic batch member.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchResult {
    /// An operation returned node metadata.
    Node(Node),
    /// A trash operation returned a recoverable record.
    Trash(TrashEntry),
    /// An operation completed without a value.
    Unit,
}

/// Canonical asynchronous filesystem behavior shared by all backends.
///
/// Implementations must enforce the workspace and capabilities in each
/// [`RequestContext`] and may be used through `dyn FileSystem`.
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Returns logical and quota usage for the context's workspace.
    async fn workspace_usage(&self, ctx: &RequestContext) -> FsResult<WorkspaceUsage>;

    /// Returns metadata for a path.
    async fn stat(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<Node>;

    /// Returns whether a path resolves to a visible node.
    async fn exists(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: StatOptions,
    ) -> FsResult<bool>;

    /// Returns one cursor-paginated page of direct directory children.
    async fn read_dir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        page: PageRequest,
    ) -> FsResult<Page<Node>>;

    /// Returns one cursor-paginated page of a recursive tree traversal.
    async fn tree(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TreeOptions,
        page: PageRequest,
    ) -> FsResult<Page<TreeEntry>>;

    /// Creates a directory.
    async fn mkdir(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node>;

    /// Opens a bounded-memory byte stream for a regular file.
    async fn read(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: ReadOptions,
    ) -> FsResult<FileRead>;

    /// Atomically creates or replaces a regular file from a byte stream.
    async fn write(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node>;

    /// Atomically writes streamed bytes beginning at a logical offset.
    async fn write_at(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        offset: u64,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node>;

    /// Atomically appends streamed bytes to a regular file.
    async fn append(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        source: WriteSource,
        options: WriteOptions,
    ) -> FsResult<Node>;

    /// Changes a regular file's logical length.
    async fn truncate(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        length: u64,
        options: MutationOptions,
    ) -> FsResult<Node>;

    /// Updates timestamps or creates an empty regular file.
    async fn touch(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: TouchOptions,
    ) -> FsResult<Node>;

    /// Copies a node within the context's workspace.
    async fn copy(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: CopyOptions,
    ) -> FsResult<Node>;

    /// Moves a node within the context's workspace.
    async fn move_path(
        &self,
        ctx: &RequestContext,
        from: &VirtualPath,
        to: &VirtualPath,
        options: MoveOptions,
    ) -> FsResult<Node>;

    /// Permanently removes a node.
    async fn remove(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: RemoveOptions,
    ) -> FsResult<()>;

    /// Creates a symbolic link containing a relative or absolute virtual target.
    async fn symlink(
        &self,
        ctx: &RequestContext,
        target: &LinkTarget,
        link: &VirtualPath,
        options: CreateOptions,
    ) -> FsResult<Node>;

    /// Returns the stored target of a symbolic link without resolving it.
    async fn read_link(&self, ctx: &RequestContext, path: &VirtualPath) -> FsResult<LinkTarget>;

    /// Moves a node into recoverable trash.
    async fn trash(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        options: MutationOptions,
    ) -> FsResult<TrashEntry>;

    /// Returns one cursor-paginated page of recoverable trash records.
    async fn list_trash(
        &self,
        ctx: &RequestContext,
        page: PageRequest,
    ) -> FsResult<Page<TrashEntry>>;

    /// Restores a recoverable trash record.
    async fn restore(
        &self,
        ctx: &RequestContext,
        trash: TrashId,
        destination: Option<&VirtualPath>,
        options: MutationOptions,
    ) -> FsResult<Node>;

    /// Permanently purges a recoverable trash record.
    async fn purge(&self, ctx: &RequestContext, trash: TrashId) -> FsResult<()>;

    /// Sets an opaque custom attribute on a node.
    async fn set_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        value: &[u8],
        options: MutationOptions,
    ) -> FsResult<Node>;

    /// Removes a custom attribute from a node.
    async fn remove_attribute(
        &self,
        ctx: &RequestContext,
        path: &VirtualPath,
        key: &str,
        options: MutationOptions,
    ) -> FsResult<Node>;

    /// Returns nodes whose absolute paths match a virtual-path glob.
    async fn glob(
        &self,
        ctx: &RequestContext,
        pattern: &str,
        page: PageRequest,
    ) -> FsResult<Page<Node>>;

    /// Returns nodes matching bounded metadata predicates.
    async fn find(
        &self,
        ctx: &RequestContext,
        query: FindQuery,
        page: PageRequest,
    ) -> FsResult<Page<Node>>;

    /// Returns bounded literal byte matches from regular files.
    async fn search_content(
        &self,
        ctx: &RequestContext,
        query: ContentQuery,
        page: PageRequest,
    ) -> FsResult<Page<SearchMatch>>;

    /// Executes non-streaming operations atomically in request order.
    async fn batch(
        &self,
        ctx: &RequestContext,
        operations: Vec<BatchOperation>,
    ) -> FsResult<Vec<BatchResult>>;

    /// Returns committed changes after an optional opaque cursor.
    async fn changes(
        &self,
        ctx: &RequestContext,
        after: Option<ChangeCursor>,
        page: PageRequest,
    ) -> FsResult<Page<Change>>;
}

