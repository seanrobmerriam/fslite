//! Turns a [`CommandOutput`] into text: either a human-readable summary
//! (used by default in `fslite-cli`) or pretty-printed JSON matching the
//! wire codec exactly (`--json`). Every untrusted string field (node names,
//! link targets, and paths — which `fslite-core` normalizes but does not
//! strip control bytes from) is passed through [`sanitize_for_terminal`]
//! before it reaches a human-readable line, since a malicious filename or
//! path segment is attacker-controlled input reaching a real terminal.

use fslite_core::Node;

use crate::CommandOutput;

/// Strips ASCII control bytes (except `\n`/`\t`) — including the ESC byte
/// that begins every ANSI escape sequence — from untrusted text before it
/// is written to a terminal. This removes the trigger byte outright rather
/// than substituting a visible placeholder, since the goal is preventing
/// the escape sequence from being interpreted at all.
pub fn sanitize_for_terminal(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| ch == '\n' || ch == '\t' || !ch.is_control())
        .collect()
}

fn render_node_line(node: &Node) -> String {
    format!(
        "{:<10} {:>10} {}",
        format!("{:?}", node.kind).to_lowercase(),
        node.logical_size,
        sanitize_for_terminal(&node.name)
    )
}

/// Renders a [`CommandOutput`] as human-readable text.
pub fn render_human(output: &CommandOutput) -> String {
    match output {
        CommandOutput::Usage(usage) => format!(
            "active: {} bytes / {} nodes\ntrashed: {} bytes / {} nodes\nquota: {} bytes / {} nodes",
            usage.active_logical_bytes, usage.active_nodes,
            usage.trashed_logical_bytes, usage.trashed_nodes,
            usage.max_logical_bytes, usage.max_nodes,
        ),
        CommandOutput::Node(node) => render_node_line(node),
        CommandOutput::Exists(found) => found.to_string(),
        CommandOutput::Nodes(page) => page.items.iter().map(render_node_line).collect::<Vec<_>>().join("\n"),
        CommandOutput::Tree(page) => page
            .items
            .iter()
            .map(|entry| format!("{}{}", "  ".repeat(entry.depth as usize), sanitize_for_terminal(entry.path.as_str())))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Content { bytes, .. } => String::from_utf8_lossy(bytes).into_owned(),
        CommandOutput::Unit => "ok".to_string(),
        CommandOutput::LinkTarget(target) => sanitize_for_terminal(target.as_str()),
        CommandOutput::Trash(entry) => format!(
            "{} (was {})",
            entry.id,
            sanitize_for_terminal(entry.original_path.as_str())
        ),
        CommandOutput::TrashList(page) => page
            .items
            .iter()
            .map(|entry| format!("{} {}", entry.id, sanitize_for_terminal(entry.original_path.as_str())))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::SearchMatches(page) => page
            .items
            .iter()
            .map(|m| format!("{}: {}", sanitize_for_terminal(m.path.as_str()), sanitize_for_terminal(&String::from_utf8_lossy(&m.preview))))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Changes(page) => page
            .items
            .iter()
            .map(|change| format!("{} {:?}", change.sequence, change.kind))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Batch(results) => format!("{} operations completed", results.len()),
    }
}

/// Renders a [`CommandOutput`] as pretty-printed JSON, exactly matching the
/// serde wire codec (round-trippable back into a `CommandOutput`).
pub fn render_json(output: &CommandOutput) -> String {
    serde_json::to_string_pretty(output).expect("CommandOutput always serializes")
}
