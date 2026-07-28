use std::collections::BTreeMap;

use fslite_command::CommandOutput;
use fslite_command::render::{
    render_human, render_json, sanitize_for_terminal, sanitize_name, sanitize_preview,
};
use fslite_core::{
    ByteRange, Change, ChangeKind, LinkTarget, Node, NodeId, NodeKind, Page, Revision, SearchMatch,
    TrashEntry, TrashId, TreeEntry, VirtualPath, WorkspaceId,
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

/// Regression test: `sanitize_for_terminal` deliberately keeps `\n`/`\t` for
/// genuinely free-text fields, but that is the wrong sanitizer for
/// structured fields like a node name — a newline there is never legitimate
/// and lets an attacker forge extra rows in table-shaped output.
/// `sanitize_name` is the stricter sibling that also strips `\n`/`\t`.
#[test]
fn sanitize_name_strips_newline_and_tab_in_addition_to_other_control_bytes() {
    let input = "a\x07b\nc\td\x1be";
    let clean = sanitize_name(input);
    assert_eq!(clean, "abcde");
}

/// Live-verified repro: writing a node named
/// `a.txt\nfile          999 IMPORTANT-SYSTEM-FILE.txt` and rendering a
/// directory listing must produce exactly one line for that entry, not a
/// second fabricated line that looks like a real, unrelated file.
#[test]
fn human_rendering_of_a_node_with_an_embedded_newline_does_not_forge_an_extra_row() {
    let hostile_name = "a.txt\nfile          999 IMPORTANT-SYSTEM-FILE.txt";
    let output = CommandOutput::Nodes(Page::new(
        vec![sample_node("legit.txt"), sample_node(hostile_name)],
        None,
    ));
    let rendered = render_human(&output);
    let lines: Vec<&str> = rendered.lines().collect();
    // Exactly one line per node (two nodes in the page), not three — proves
    // the embedded `\n` did not split the hostile node's row into a second,
    // forged line. (The fabricated text itself is still present —
    // `sanitize_name` strips the newline, not the ordinary characters
    // around it — but it must stay glued to its own node's single line
    // rather than becoming a standalone row a user could mistake for real
    // output.)
    assert_eq!(
        lines.len(),
        2,
        "rendered output had a forged extra line: {rendered:?}"
    );
    let matches_naming_the_fake_file = lines
        .iter()
        .filter(|line| line.contains("IMPORTANT-SYSTEM-FILE"))
        .count();
    assert_eq!(
        matches_naming_the_fake_file, 1,
        "the fabricated text should appear on exactly one (merged) line: {rendered:?}"
    );
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

/// Regression test: `sanitize_name` is `fslite-command`'s stricter, more
/// generally-applicable sanitizer, but was missing from the crate-root
/// re-export line in `lib.rs` — reachable only via the longer
/// `fslite_command::render::sanitize_name` path, unlike its sibling
/// `sanitize_for_terminal`. This is a compile-time proof: if the
/// crate-root path doesn't resolve, this test fails to compile at all.
#[test]
fn sanitize_name_is_reachable_from_the_crate_root() {
    let clean = fslite_command::sanitize_name("a\nb");
    assert_eq!(clean, "ab");
}

/// Regression test: `sanitize_preview` is on the same crate-root re-export
/// line in `lib.rs` as `sanitize_name`, but had no equivalent compile-time
/// guard proving it's reachable via `fslite_command::sanitize_preview` (not
/// just `fslite_command::render::sanitize_preview`).
#[test]
fn sanitize_preview_is_reachable_from_the_crate_root() {
    let escaped = fslite_command::sanitize_preview("a\nb");
    assert_eq!(escaped, "a\\nb");
}

/// Regression test: search-match previews are free-text file content,
/// where a literal newline can be legitimate — but printing it raw inside
/// `"{path}: {preview}"` still let a hostile file forge a fake extra
/// search-result row that looked like a real, unrelated match. Escaping
/// the newline into a visible two-character `\n` sequence keeps the
/// content visible without letting it masquerade as a row boundary.
#[test]
fn human_rendering_of_search_matches_does_not_forge_an_extra_row_from_a_newline_in_the_preview() {
    let search_match = SearchMatch {
        node: sample_node("ignored"),
        path: VirtualPath::parse("/real.txt").unwrap(),
        range: ByteRange::new(0, 5),
        preview: b"needle\n/etc/shadow: root:x:0:0".to_vec(),
    };
    let output = CommandOutput::SearchMatches(Page::new(vec![search_match], None));
    let rendered = render_human(&output);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "rendered output had a forged extra line: {rendered:?}"
    );
    assert!(
        rendered.contains("needle\\n/etc/shadow"),
        "expected the embedded newline to survive as a visible escape sequence, got: {rendered:?}"
    );
}

/// Regression test: a Unicode right-to-left-override character can make a
/// name *display* with a spoofed extension (e.g. a name ending
/// `\u{202E}gpj.exe` can render as if it ends `.jpg`) without changing the
/// underlying bytes — `char::is_control()` doesn't catch it, since RLO is
/// Unicode category Cf (format), not Cc (control).
#[test]
fn sanitize_name_strips_bidi_override_characters() {
    let hostile_name = "harmless\u{202E}gpj.exe";
    let clean = sanitize_name(hostile_name);
    assert!(
        !clean.contains('\u{202E}'),
        "RLO character survived: {clean:?}"
    );
    assert!(clean.contains("harmless"));
    assert_eq!(clean, "harmlessgpj.exe");
}

/// Regression test: LRM/RLM/ALM are weaker bidi marks than the explicit
/// overrides/embeddings/isolates — they only flip the resolved direction of
/// adjacent neutral characters (punctuation, digits, spaces) rather than
/// reversing a whole run — but a filename's extension dot and digits are
/// exactly such neutrals, so `is_bidi_override` must catch these too, not
/// just RLO/LRO/isolates.
#[test]
fn sanitize_name_strips_bidi_marks_not_just_overrides() {
    let hostile_name = "harmless\u{200E}file.txt";
    let clean = sanitize_name(hostile_name);
    assert!(
        !clean.contains('\u{200E}'),
        "LRM character survived: {clean:?}"
    );
    assert_eq!(clean, "harmlessfile.txt");
}

/// Regression test: the Unicode line/paragraph separators U+2028/U+2029
/// render as line breaks in many terminals — the same row-forging risk as
/// `\n` — but aren't caught by `char::is_control()` (categories Zl/Zp, not
/// Cc). `sanitize_name` must strip them like it strips `\n`.
#[test]
fn sanitize_name_strips_unicode_line_and_paragraph_separators() {
    let hostile_name = "a.txt\u{2028}file 999 IMPORTANT.txt";
    let clean = sanitize_name(hostile_name);
    assert_eq!(clean, "a.txtfile 999 IMPORTANT.txt");
}

/// `sanitize_for_terminal` must strip bidi overrides (they're never
/// legitimate in any context) while still preserving the Unicode
/// line/paragraph separators for genuinely free-text content, exactly
/// like it already preserves `\n`/`\t`.
#[test]
fn sanitize_for_terminal_strips_bidi_overrides_but_preserves_unicode_linebreaks() {
    let input = "safe\u{202E}text\u{2028}more";
    let clean = sanitize_for_terminal(input);
    assert_eq!(clean, "safetext\u{2028}more");
}

/// `sanitize_preview` must escape the Unicode line separator into a
/// visible sequence exactly like it already escapes `\n`, for the same
/// row-forging reason.
#[test]
fn sanitize_preview_escapes_unicode_line_separator_too() {
    let input = "needle\u{2028}more content";
    let escaped = sanitize_preview(input);
    assert!(!escaped.contains('\u{2028}'));
    assert!(escaped.contains("needle\\u{2028}more"));
}
