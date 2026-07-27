//! The typed operation codec: one [`Command`] variant per
//! `fslite_core::FileSystem` operation. Byte payloads are bounded, in-memory
//! `Vec<u8>` (base64 on the wire, via [`crate::bytes_b64`]) — this codec is
//! sized for CLI use, not for streaming arbitrarily large files.

use fslite_core::{
    BatchOperation, ChangeCursor, ContentQuery, CopyOptions, CreateOptions, FindQuery, LinkTarget,
    MoveOptions, MutationOptions, PageRequest, ReadOptions, RemoveOptions, StatOptions,
    TouchOptions, TrashId, TreeOptions, VirtualPath, WriteOptions,
};
use serde::{Deserialize, Serialize};

/// One typed filesystem operation, serializable for local execution,
/// remote transport, or storage as a `batch --file` script.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    WorkspaceUsage,
    Stat {
        path: VirtualPath,
        options: StatOptions,
    },
    Exists {
        path: VirtualPath,
        options: StatOptions,
    },
    ReadDir {
        path: VirtualPath,
        page: PageRequest,
    },
    Tree {
        path: VirtualPath,
        options: TreeOptions,
        page: PageRequest,
    },
    Mkdir {
        path: VirtualPath,
        options: CreateOptions,
    },
    Read {
        path: VirtualPath,
        options: ReadOptions,
    },
    Write {
        path: VirtualPath,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    WriteAt {
        path: VirtualPath,
        offset: u64,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    Append {
        path: VirtualPath,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    Truncate {
        path: VirtualPath,
        length: u64,
        options: MutationOptions,
    },
    Touch {
        path: VirtualPath,
        options: TouchOptions,
    },
    Copy {
        from: VirtualPath,
        to: VirtualPath,
        options: CopyOptions,
    },
    Move {
        from: VirtualPath,
        to: VirtualPath,
        options: MoveOptions,
    },
    Remove {
        path: VirtualPath,
        options: RemoveOptions,
    },
    Symlink {
        target: LinkTarget,
        link: VirtualPath,
        options: CreateOptions,
    },
    ReadLink {
        path: VirtualPath,
    },
    Trash {
        path: VirtualPath,
        options: MutationOptions,
    },
    ListTrash {
        page: PageRequest,
    },
    Restore {
        trash: TrashId,
        destination: Option<VirtualPath>,
        options: MutationOptions,
    },
    Purge {
        trash: TrashId,
    },
    SetAttribute {
        path: VirtualPath,
        key: String,
        #[serde(with = "crate::bytes_b64")]
        value: Vec<u8>,
        options: MutationOptions,
    },
    RemoveAttribute {
        path: VirtualPath,
        key: String,
        options: MutationOptions,
    },
    Glob {
        pattern: String,
        page: PageRequest,
    },
    Find {
        query: FindQuery,
        page: PageRequest,
    },
    SearchContent {
        query: ContentQuery,
        page: PageRequest,
    },
    Changes {
        after: Option<ChangeCursor>,
        page: PageRequest,
    },
    Batch(Vec<BatchOperation>),
}
