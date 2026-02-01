use std::collections::BTreeMap;

use fslite_core::{ErrorCode, FsError, Node, NodeId, NodeKind, Revision, WorkspaceId};

#[test]
fn identifiers_and_revisions_are_strongly_typed() {
    let workspace = WorkspaceId::new();
    assert_eq!(
        WorkspaceId::parse(&workspace.to_string()).unwrap(),
        workspace
    );
    assert_eq!(Revision::INITIAL.next(), Revision::new(2).unwrap());
}

#[test]
fn errors_have_stable_machine_codes() {
    let error = FsError::not_found("/missing");
    assert_eq!(error.code(), ErrorCode::NotFound);
    assert_eq!(error.to_string(), "not found: /missing");
}

#[test]
fn node_kind_json_is_stable() {
    assert_eq!(
        serde_json::to_string(&NodeKind::Directory).unwrap(),
        "\"directory\""
    );
}

#[test]
fn node_carries_all_filesystem_timestamps() {
    let node = Node {
        workspace_id: WorkspaceId::new(),
        id: NodeId::new(),
        parent_id: None,
        name: String::new(),
        kind: NodeKind::Directory,
        logical_size: 0,
        created_at_ms: 10,
        modified_at_ms: 20,
        accessed_at_ms: 30,
        revision: Revision::INITIAL,
        attributes: BTreeMap::new(),
    };

    assert_eq!(node.accessed_at_ms, 30);
}

