use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{NodeKind, Revision, VirtualPath};

/// The default maximum number of items requested in a page.
pub const DEFAULT_PAGE_LIMIT: u32 = 100;

/// An inclusive-start, exclusive-end logical byte range.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ByteRange {
    /// The first included byte offset.
    pub start: u64,
    /// The first excluded byte offset.
    pub end: u64,
}

impl ByteRange {
    /// Creates a logical byte range.
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// Returns the range length, or zero when the range is reversed.
    pub const fn len(self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// Returns whether the range selects no bytes.
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// Cursor pagination parameters shared by listing and discovery operations.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PageRequest {
    /// An opaque continuation cursor returned by a prior page.
    pub cursor: Option<String>,
    /// The maximum number of items requested from the backend.
    pub limit: u32,
}

impl PageRequest {
    /// Sets the opaque continuation cursor.
    #[must_use]
    pub fn cursor(mut self, cursor: Option<String>) -> Self {
        self.cursor = cursor;
        self
    }

    /// Sets the requested maximum item count.
    #[must_use]
    pub const fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

/// Options controlling node metadata lookup.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatOptions {
    /// Whether to resolve the final symbolic link.
    pub follow_symlinks: bool,
}

impl StatOptions {
    /// Sets whether to resolve the final symbolic link.
    #[must_use]
    pub const fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }
}

impl Default for StatOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
        }
    }
}

/// Options controlling recursive tree traversal.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TreeOptions {
    /// The maximum depth below the requested path, subject to backend limits.
    pub max_depth: Option<u32>,
    /// Whether traversal follows symbolic links.
    pub follow_symlinks: bool,
}

impl TreeOptions {
    /// Sets the maximum traversal depth.
    #[must_use]
    pub const fn max_depth(mut self, max_depth: Option<u32>) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Sets whether traversal follows symbolic links.
    #[must_use]
    pub const fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }
}

/// Options controlling directory and symbolic-link creation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateOptions {
    /// Whether missing parent directories are created.
    pub parents: bool,
    /// Whether an existing compatible node is accepted.
    pub exist_ok: bool,
    /// An optional optimistic precondition on an existing target.
    pub expected_revision: Option<Revision>,
}

impl CreateOptions {
    /// Sets whether missing parent directories are created.
    #[must_use]
    pub const fn parents(mut self, parents: bool) -> Self {
        self.parents = parents;
        self
    }

    /// Sets whether an existing compatible node is accepted.
    #[must_use]
    pub const fn exist_ok(mut self, exist_ok: bool) -> Self {
        self.exist_ok = exist_ok;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

/// Options controlling a streamed file read.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadOptions {
    /// The logical byte range to return, or the complete file when absent.
    pub range: Option<ByteRange>,
    /// Whether to resolve the final symbolic link.
    pub follow_symlinks: bool,
}

impl ReadOptions {
    /// Sets the logical byte range to return.
    #[must_use]
    pub const fn range(mut self, range: Option<ByteRange>) -> Self {
        self.range = range;
        self
    }

    /// Sets whether to resolve the final symbolic link.
    #[must_use]
    pub const fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            range: None,
            follow_symlinks: true,
        }
    }
}

/// Options controlling a streamed file write.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WriteOptions {
    /// Whether the operation may create a missing regular file.
    pub create: bool,
    /// An optional optimistic precondition on an existing file.
    pub expected_revision: Option<Revision>,
}

impl WriteOptions {
    /// Sets whether a missing regular file may be created.
    #[must_use]
    pub const fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }

    /// Requires the target file to exist and replaces its current contents.
    pub const fn replace() -> Self {
        Self {
            create: false,
            expected_revision: None,
        }
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            create: true,
            expected_revision: None,
        }
    }
}

/// Common options for a mutation with no additional controls.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationOptions {
    /// An optional optimistic precondition on the target node.
    pub expected_revision: Option<Revision>,
}

impl MutationOptions {
    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

/// Options controlling a file touch operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TouchOptions {
    /// Whether touching a missing path creates an empty regular file.
    pub create: bool,
    /// An optional optimistic precondition on an existing file.
    pub expected_revision: Option<Revision>,
}

impl TouchOptions {
    /// Sets whether touching a missing path creates an empty regular file.
    #[must_use]
    pub const fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

impl Default for TouchOptions {
    fn default() -> Self {
        Self {
            create: true,
            expected_revision: None,
        }
    }
}

/// Options controlling a copy operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CopyOptions {
    /// Whether directory descendants are copied.
    pub recursive: bool,
    /// Whether an existing destination may be replaced.
    pub overwrite: bool,
    /// An optional optimistic precondition on the source node.
    pub expected_revision: Option<Revision>,
}

impl CopyOptions {
    /// Sets whether directory descendants are copied.
    #[must_use]
    pub const fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Sets whether an existing destination may be replaced.
    #[must_use]
    pub const fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

/// Options controlling a move operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MoveOptions {
    /// Whether an existing destination may be replaced.
    pub overwrite: bool,
    /// An optional optimistic precondition on the source node.
    pub expected_revision: Option<Revision>,
}

impl MoveOptions {
    /// Sets whether an existing destination may be replaced.
    #[must_use]
    pub const fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

/// Options controlling permanent removal.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoveOptions {
    /// Whether a directory and all descendants may be removed.
    pub recursive: bool,
    /// An optional optimistic precondition on the target node.
    pub expected_revision: Option<Revision>,
}

impl RemoveOptions {
    /// Sets whether a directory and all descendants may be removed.
    #[must_use]
    pub const fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Sets the optimistic revision precondition.
    #[must_use]
    pub const fn expected_revision(mut self, expected_revision: Option<Revision>) -> Self {
        self.expected_revision = expected_revision;
        self
    }
}

/// Metadata predicates for workspace-scoped node discovery.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FindQuery {
    /// The root beneath which matching nodes are considered.
    pub root: VirtualPath,
    /// An optional case-sensitive substring required in the node name.
    pub name_contains: Option<String>,
    /// An optional required node kind.
    pub kind: Option<NodeKind>,
    /// An optional inclusive lower bound on logical size.
    pub min_logical_size: Option<u64>,
    /// An optional inclusive upper bound on logical size.
    pub max_logical_size: Option<u64>,
    /// An optional exclusive lower bound on modification time.
    pub modified_after_ms: Option<i64>,
    /// An optional exclusive upper bound on modification time.
    pub modified_before_ms: Option<i64>,
    /// Attribute key/value pairs that every match must contain.
    pub attributes: BTreeMap<String, Value>,
}

impl FindQuery {
    /// Sets the root beneath which matching nodes are considered.
    #[must_use]
    pub fn root(mut self, root: VirtualPath) -> Self {
        self.root = root;
        self
    }

    /// Sets the optional case-sensitive node-name substring.
    #[must_use]
    pub fn name_contains(mut self, name_contains: Option<String>) -> Self {
        self.name_contains = name_contains;
        self
    }

    /// Sets the optional required node kind.
    #[must_use]
    pub const fn kind(mut self, kind: Option<NodeKind>) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the optional inclusive lower bound on logical size.
    #[must_use]
    pub const fn min_logical_size(mut self, min_logical_size: Option<u64>) -> Self {
        self.min_logical_size = min_logical_size;
        self
    }

    /// Sets the optional inclusive upper bound on logical size.
    #[must_use]
    pub const fn max_logical_size(mut self, max_logical_size: Option<u64>) -> Self {
        self.max_logical_size = max_logical_size;
        self
    }

    /// Sets the optional exclusive lower modification-time bound.
    #[must_use]
    pub const fn modified_after_ms(mut self, modified_after_ms: Option<i64>) -> Self {
        self.modified_after_ms = modified_after_ms;
        self
    }

    /// Sets the optional exclusive upper modification-time bound.
    #[must_use]
    pub const fn modified_before_ms(mut self, modified_before_ms: Option<i64>) -> Self {
        self.modified_before_ms = modified_before_ms;
        self
    }

    /// Sets the required attribute key/value pairs.
    #[must_use]
    pub fn attributes(mut self, attributes: BTreeMap<String, Value>) -> Self {
        self.attributes = attributes;
        self
    }
}

impl Default for FindQuery {
    fn default() -> Self {
        Self {
            root: VirtualPath::root(),
            name_contains: None,
            kind: None,
            min_logical_size: None,
            max_logical_size: None,
            modified_after_ms: None,
            modified_before_ms: None,
            attributes: BTreeMap::new(),
        }
    }
}

/// A bounded literal content-search request.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentQuery {
    /// The root beneath which regular files are searched.
    pub root: VirtualPath,
    /// The literal byte sequence to find.
    pub needle: Vec<u8>,
}

impl ContentQuery {
    /// Sets the root beneath which regular files are searched.
    #[must_use]
    pub fn root(mut self, root: VirtualPath) -> Self {
        self.root = root;
        self
    }

    /// Sets the literal byte sequence to find.
    #[must_use]
    pub fn needle(mut self, needle: Vec<u8>) -> Self {
        self.needle = needle;
        self
    }
}

impl Default for ContentQuery {
    fn default() -> Self {
        Self {
            root: VirtualPath::root(),
            needle: Vec::new(),
        }
    }
}

