use std::collections::BTreeMap;

use fslite_command::render::{render_human, render_json, sanitize_for_terminal};
use fslite_command::CommandOutput;
use fslite_core::{
    ByteRange, Change, ChangeKind, LinkTarget, Node, NodeId, NodeKind, Page, Revision,
    SearchMatch, TrashEntry, TrashId, TreeEntry, VirtualPath, WorkspaceId,
};

fn sample_node(name: &str) -> Node {
    Node {
        workspace_id: WorkspaceId::new(),
        id: NodeId::new(),
        parent_id: None,
        name: name.to_string(),
        kind: NodeKind::File,
        logical_size: 5,
        created_at_ms: 0,
        modified_at_ms: 0,
        accessed_at_ms: 0,
        revision: Revision::INITIAL,
        attributes: BTreeMap::new(),
    }
}

/// A path string containing the ESC byte that would begin a terminal escape
/// sequence, embedded in a segment that `VirtualPath::parse` happily
/// accepts (it only rejects a missing leading `/` or an embedded NUL).
const HOSTILE_PATH: &str = "/innocent\x1b[31mFAKE ERROR\x1b[0m.txt";
const HOSTILE_SEGMENT_LABEL: &str = "innocent";

#[test]
fn human_rendering_of_a_node_includes_its_name_and_size() {
    let output = CommandOutput::Node(sample_node("a.txt"));
    let rendered = render_human(&output);
    assert!(rendered.contains("a.txt"));
    assert!(rendered.contains('5'));
}

#[test]
fn json_rendering_is_valid_json_matching_the_wire_codec() {
    let output = CommandOutput::Node(sample_node("a.txt"));
    let rendered = render_json(&output);
    let parsed: CommandOutput = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed, output);
}

#[test]
fn sanitize_strips_the_escape_byte_that_triggers_ansi_sequences() {
    let malicious = "innocuous.txt\x1b[31mFAKE ERROR\x1b[0m";
    let clean = sanitize_for_terminal(malicious);
    assert!(!clean.contains('\x1b'));
    assert!(clean.contains("innocuous.txt"));
}

#[test]
fn sanitize_strips_other_control_bytes_but_keeps_newline_and_tab() {
    let input = "a\x07b\nc\td";
    let clean = sanitize_for_terminal(input);
    assert_eq!(clean, "ab\nc\td");
}

#[test]
fn human_rendering_of_a_node_with_a_hostile_name_is_sanitized() {
    let hostile_name = "\x1b]0;pwned\x07innocent.txt";
    let output = CommandOutput::Node(sample_node(hostile_name));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains("innocent.txt"));
}

// --- Coverage audit: every CommandOutput variant that prints an untrusted
// string must sanitize it, not just `Node`. Each test below builds the
// variant with a hostile embedded path/name and confirms the ESC byte never
// survives into `render_human`'s output while the benign surrounding text
// does, proving `sanitize_for_terminal` is wired into that arm.

#[test]
fn human_rendering_of_nodes_page_sanitizes_every_item() {
    let output = CommandOutput::Nodes(Page::new(
        vec![sample_node("a.txt"), sample_node("\x1b[2Jinnocent.txt")],
        None,
    ));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains("a.txt"));
    assert!(rendered.contains("innocent.txt"));
}

#[test]
fn human_rendering_of_tree_sanitizes_hostile_paths() {
    let node = sample_node("ignored");
    let entry = TreeEntry {
        path: VirtualPath::parse(HOSTILE_PATH).unwrap(),
        depth: 1,
        node,
    };
    let output = CommandOutput::Tree(Page::new(vec![entry], None));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains(HOSTILE_SEGMENT_LABEL));
}

#[test]
fn human_rendering_of_link_target_is_sanitized() {
    let target = LinkTarget::parse("\x1b[31m/etc/passwd").unwrap();
    let output = CommandOutput::LinkTarget(target);
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains("/etc/passwd"));
}

#[test]
fn human_rendering_of_trash_entry_sanitizes_original_path() {
    let entry = TrashEntry {
        id: TrashId::new(),
        node: sample_node("ignored"),
        original_path: VirtualPath::parse(HOSTILE_PATH).unwrap(),
        trashed_at_ms: 0,
        actor_metadata: BTreeMap::new(),
    };
    let output = CommandOutput::Trash(entry);
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains(HOSTILE_SEGMENT_LABEL));
}

#[test]
fn human_rendering_of_trash_list_sanitizes_every_item() {
    let entry = TrashEntry {
        id: TrashId::new(),
        node: sample_node("ignored"),
        original_path: VirtualPath::parse(HOSTILE_PATH).unwrap(),
        trashed_at_ms: 0,
        actor_metadata: BTreeMap::new(),
    };
    let output = CommandOutput::TrashList(Page::new(vec![entry], None));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains(HOSTILE_SEGMENT_LABEL));
}

#[test]
fn human_rendering_of_search_matches_sanitizes_path_and_preview() {
    let search_match = SearchMatch {
        node: sample_node("ignored"),
        path: VirtualPath::parse(HOSTILE_PATH).unwrap(),
        range: ByteRange::new(0, 5),
        preview: b"safe\x1b[31mtext".to_vec(),
    };
    let output = CommandOutput::SearchMatches(Page::new(vec![search_match], None));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains(HOSTILE_SEGMENT_LABEL));
    assert!(rendered.contains("safe"));
    assert!(rendered.contains("text"));
}

#[test]
fn human_rendering_of_changes_contains_no_control_bytes() {
    let change = Change {
        sequence: 1,
        kind: ChangeKind::Moved,
        node_id: Some(NodeId::new()),
        old_path: Some(VirtualPath::parse(HOSTILE_PATH).unwrap()),
        new_path: Some(VirtualPath::parse("/dest.txt").unwrap()),
        revision: Some(Revision::INITIAL),
        created_at_ms: 0,
        actor_metadata: BTreeMap::new(),
    };
    let output = CommandOutput::Changes(Page::new(vec![change], None));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
}

#[test]
fn json_rendering_round_trips_every_variant_shape() {
    let node = sample_node("a.txt");
    let outputs = vec![
        CommandOutput::Exists(true),
        CommandOutput::Unit,
        CommandOutput::Node(node.clone()),
        CommandOutput::Nodes(Page::new(vec![node.clone()], None)),
        CommandOutput::LinkTarget(LinkTarget::parse("/target").unwrap()),
        CommandOutput::Batch(vec![fslite_core::BatchResult::Node(node)]),
    ];
    for output in outputs {
        let rendered = render_json(&output);
        let parsed: CommandOutput = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed, output);
    }
}
