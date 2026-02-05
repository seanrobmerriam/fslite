//! Stable, transport-independent filesystem domain primitives.
//!
//! This crate defines the canonical [`FileSystem`] trait and the domain
//! types every implementation shares: absolute, normalized [`VirtualPath`]s,
//! strongly typed identifiers and revisions, stable machine-readable
//! [`ErrorCode`]s, and the request/response shapes for every operation. It
//! contains no SQL, no HTTP, and no host filesystem paths — see the
//! `fslite-sqlite` crate for a persistent, embeddable implementation.
//!
//! ```
//! use fslite_core::VirtualPath;
//!
//! let path = VirtualPath::parse("/a/../a/b")?;
//! assert_eq!(path.as_str(), "/a/b");
//! # Ok::<(), fslite_core::FsError>(())
//! ```

mod error;
mod fs;
mod model;
mod options;
mod path;

pub use error::{ErrorCode, FsError, FsResult};
pub use fs::{
    BatchOperation, BatchResult, ByteStream, Capability, Change, ChangeCursor, ChangeKind,
    FileRead, FileSystem, Page, RequestContext, SearchMatch, TrashEntry, TrashId, TreeEntry,
    WorkspaceUsage, WriteSource,
};
pub use model::{Node, NodeId, NodeKind, Revision, WorkspaceId};
pub use options::{
    ByteRange, ContentQuery, CopyOptions, CreateOptions, DEFAULT_PAGE_LIMIT, FindQuery,
    MoveOptions, MutationOptions, PageRequest, ReadOptions, RemoveOptions, StatOptions,
    TouchOptions, TreeOptions, WriteOptions,
};
pub use path::{LinkTarget, VirtualPath};

