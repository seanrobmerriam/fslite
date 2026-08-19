//! Turns a [`CommandOutput`] into text: either a human-readable summary
//! (used by default in `fslite`) or pretty-printed JSON matching the
//! wire codec exactly (`--json`). Every untrusted string field (node names,
//! link targets, and paths — which `fslite-core` normalizes but does not
//! strip control bytes from) is passed through one of three sanitizers
//! before it reaches a human-readable line, since a malicious filename or
//! path segment is attacker-controlled input reaching a real terminal.
//!
//! Three sanitizers exist because `\n`/`\t` are not uniformly safe to keep:
//! [`sanitize_name`] strips all control bytes (including `\n`/`\t`), Unicode
//! bidirectional-override characters, and the Unicode line/paragraph
//! separators, and is used for *structured* fields (node names, paths, link
//! targets) where a newline is never legitimate and would otherwise let an
//! attacker forge extra rows in table-shaped output (e.g. a node named
//! `a.txt\nfile 999 IMPORTANT.txt` injecting a fake `ls` row), and where a
//! bidi override could visually spoof the name (e.g. an extension made to
//! *display* reversed). [`sanitize_for_terminal`] preserves `\n`/`\t` and
//! the Unicode line/paragraph separators but strips other control bytes and
//! bidi overrides (which are never legitimate in any context) — used for
//! free-text fields where raw newlines can be legitimate content.
//! [`sanitize_preview`] is a stricter tier: it wraps [`sanitize_for_terminal`]
//! but further escapes `\n`/`\t` and the Unicode line/paragraph separators
//! into visible escape sequences, for free-text content rendered
//! *inline* within a single row (currently only search-match previews),
//! keeping the content visible without letting it masquerade as a row
//! boundary.

use fslite_core::Node;

use crate::CommandOutput;

/// Unicode bidirectional-control characters that can silently reorder how
/// surrounding text *displays* without changing its underlying bytes —
/// e.g. a name ending `\u{202E}gpj.exe` can display as if it ends `.jpg`
/// reversed. Covers the explicit embeddings/overrides (U+202A-U+202E), the
/// isolates (U+2066-U+2069), and the weaker marks LRM/RLM/ALM (U+200E,
/// U+200F, U+061C), which only flip the resolved direction of adjacent
/// neutral characters (e.g. a filename's extension dot) rather than
/// reversing a whole run. `char::is_control()` does not catch these; they
/// are Unicode general category Cf (format), not Cc (control).
fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

/// Unicode line/paragraph separators that render as line breaks in many
/// terminals but aren't caught by `char::is_control()` either (categories
/// Zl/Zp, not Cc).
fn is_unicode_linebreak(ch: char) -> bool {
    matches!(ch, '\u{2028}' | '\u{2029}')
}

/// Strips ASCII control bytes (except `\n`/`\t`) — including the ESC byte
/// that begins every ANSI escape sequence — and Unicode bidirectional-override
/// characters from untrusted text before it is written to a terminal. This
/// removes the trigger byte outright rather than substituting a visible
/// placeholder, since the goal is preventing the escape sequence (or a
/// spoofed display order) from being interpreted at all. `\n`/`\t` and the
/// Unicode line/paragraph separators (U+2028/U+2029) are preserved, since
/// they can be legitimate content in free text; bidi overrides are never
/// legitimate in any context, so they are always stripped.
///
/// Use this only for genuinely free-text fields where a literal `\n`/`\t`
/// can be legitimate content. For structured fields (node names, paths, link
/// targets), use [`sanitize_name`] instead — those must never contain a
/// newline, since one would let an attacker forge extra rows in
/// table-shaped output. For free-text fields rendered inline within a single
/// row, use [`sanitize_preview`] instead — it wraps this function and
/// further escapes `\n`/`\t` and the Unicode line/paragraph separators to
/// prevent them from being mistaken for separators.
pub fn sanitize_for_terminal(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| {
            ch == '\n'
                || ch == '\t'
                || is_unicode_linebreak(ch)
                || (!ch.is_control() && !is_bidi_override(ch))
        })
        .collect()
}

/// Stricter sibling of [`sanitize_for_terminal`] for structured fields
/// (node names, paths, link targets) where a newline is never legitimate
/// content. Strips every ASCII control byte (including `\n`/`\t`), every
/// Unicode bidirectional-override character, and the Unicode line/paragraph
/// separators, so a hostile name/path can neither inject extra lines that
/// masquerade as separate output rows nor visually spoof its own content
/// (e.g. a bidi override making an extension display reversed).
pub fn sanitize_name(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| !ch.is_control() && !is_bidi_override(ch) && !is_unicode_linebreak(ch))
        .collect()
}

/// [`sanitize_for_terminal`], with `\n`/`\t` and the Unicode line/paragraph
/// separators (U+2028/U+2029) then escaped into visible escape sequences
/// instead of passed through raw. Use this for free-text content
/// rendered *inline* within a single table-shaped output row (currently
/// only search-match previews): a real newline (ASCII or Unicode) in the
/// underlying file content stays visible to the user, but can never be
/// mistaken for a row boundary the way a raw line break could.
pub fn sanitize_preview(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in sanitize_for_terminal(raw).chars() {
        match ch {
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\u{2028}' => escaped.push_str("\\u{2028}"),
            '\u{2029}' => escaped.push_str("\\u{2029}"),
            other => escaped.push(other),
        }
    }
    escaped
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
                    sanitize_preview(&String::from_utf8_lossy(&m.preview))
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
