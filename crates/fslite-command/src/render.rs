//! Turns a [`CommandOutput`] into text: either a human-readable summary
//! (used by default in `fslite-cli`) or pretty-printed JSON matching the
//! wire codec exactly (`--json`). Every untrusted string field (node names,
//! link targets, and paths — which `fslite-core` normalizes but does not
//! strip control bytes from) is passed through [`sanitize_for_terminal`] or
//! [`sanitize_name`] before it reaches a human-readable line, since a
//! malicious filename or path segment is attacker-controlled input reaching
//! a real terminal.
//!
//! Two sanitizers exist because `\n`/`\t` are not uniformly safe to keep:
//! [`sanitize_for_terminal`] preserves them for genuinely free-text fields
//! (search match previews), where a literal newline can be legitimate file
//! content. [`sanitize_name`] additionally strips `\n`/`\t` and is used for
//! every *structured* field — node names, paths, and link targets — where a
//! newline is never legitimate and would otherwise let an attacker forge
//! extra rows in table-shaped output (e.g. a node named
//! `a.txt\nfile 999 IMPORTANT.txt` injecting a fake `ls` row).

use fslite_core::Node;

use crate::CommandOutput;

/// Strips ASCII control bytes (except `\n`/`\t`) — including the ESC byte
/// that begins every ANSI escape sequence — from untrusted text before it
/// is written to a terminal. This removes the trigger byte outright rather
/// than substituting a visible placeholder, since the goal is preventing
/// the escape sequence from being interpreted at all.
///
/// Use this only for genuinely free-text fields (e.g. search match
/// previews) where a literal `\n`/`\t` can be legitimate content. For
/// structured fields (node names, paths, link targets), use
/// [`sanitize_name`] instead — those must never contain a newline, since one
/// would let an attacker forge extra rows in table-shaped output.
pub fn sanitize_for_terminal(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| ch == '\n' || ch == '\t' || !ch.is_control())
        .collect()
}

/// Stricter sibling of [`sanitize_for_terminal`] for structured fields
/// (node names, paths, link targets) where a newline is never legitimate
/// content. Strips every ASCII control byte, including `\n`/`\t`, so a
/// hostile name/path cannot inject extra lines that masquerade as separate
/// output rows.
pub fn sanitize_name(raw: &str) -> String {
    raw.chars().filter(|ch| !ch.is_control()).collect()
}

fn render_node_line(node: &Node) -> String {
    format!(
        "{:<10} {:>10} {}",
        format!("{:?}", node.kind).to_lowercase(),
        node.logical_size,
        sanitize_name(&node.name)
    )
}

/// Renders a [`CommandOutput`] as human-readable text.
pub fn render_human(output: &CommandOutput) -> String {
    match output {
        CommandOutput::Usage(usage) => format!(
            "active: {} bytes / {} nodes\ntrashed: {} bytes / {} nodes\nquota: {} bytes / {} nodes",
            usage.active_logical_bytes,
            usage.active_nodes,
            usage.trashed_logical_bytes,
            usage.trashed_nodes,
            usage.max_logical_bytes,
            usage.max_nodes,
        ),
        CommandOutput::Node(node) => render_node_line(node),
        CommandOutput::Exists(found) => found.to_string(),
        CommandOutput::Nodes(page) => page
            .items
            .iter()
            .map(render_node_line)
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Tree(page) => page
            .items
            .iter()
            .map(|entry| {
                format!(
                    "{}{}",
                    "  ".repeat(entry.depth as usize),
                    sanitize_name(entry.path.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Content { bytes, .. } => String::from_utf8_lossy(bytes).into_owned(),
        CommandOutput::Unit => "ok".to_string(),
        CommandOutput::LinkTarget(target) => sanitize_name(target.as_str()),
        CommandOutput::Trash(entry) => format!(
            "{} (was {})",
            entry.id,
            sanitize_name(entry.original_path.as_str())
        ),
        CommandOutput::TrashList(page) => page
            .items
            .iter()
            .map(|entry| {
                format!(
                    "{} {}",
                    entry.id,
                    sanitize_name(entry.original_path.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::SearchMatches(page) => page
            .items
            .iter()
            .map(|m| {
                format!(
                    "{}: {}",
                    sanitize_name(m.path.as_str()),
                    sanitize_for_terminal(&String::from_utf8_lossy(&m.preview))
                )
            })
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
