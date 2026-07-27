//! The typed operation codec's response half: one [`CommandOutput`] variant
//! per distinct `FileSystem` return shape.

use fslite_core::{
    ByteRange, Change, LinkTarget, Node, Page, Revision, SearchMatch, TrashEntry, TreeEntry,
    WorkspaceUsage,
};
use serde::{Deserialize, Serialize};

/// The typed result of executing a [`crate::Command`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutput {
    Usage(WorkspaceUsage),
    Node(Node),
    Exists(bool),
    Nodes(Page<Node>),
    Tree(Page<TreeEntry>),
    Content {
        logical_length: u64,
        revision: Revision,
        range: ByteRange,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
    },
    Unit,
    LinkTarget(LinkTarget),
    Trash(TrashEntry),
    TrashList(Page<TrashEntry>),
    SearchMatches(Page<SearchMatch>),
    Changes(Page<Change>),
    Batch(Vec<fslite_core::BatchResult>),
}
