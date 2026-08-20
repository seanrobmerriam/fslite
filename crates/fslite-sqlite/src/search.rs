//! Bounded glob, metadata find, and literal content search.
//!
//! All three walk a subtree via [`fetch_subtree_page`], the same
//! `(depth, name, id)`-ordered, keyset-paginated recursive CTE shape used by
//! `directory::tree`, capped by [`WORK_BUDGET`] rows scanned per call.
//! Reaching the budget with results still pending yields a resumable
//! cursor; exhausting the subtree without it yields `None`.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fslite_core::{
    ByteRange, ContentQuery, FindQuery, FsError, FsResult, Node, NodeKind, Page, PageRequest,
    SearchMatch, VirtualPath, WorkspaceId,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_rusqlite::Connection as AsyncConnection;

use crate::content;
use crate::db;
use crate::mutate;
use crate::resolve::{self, DIRECTORY_KIND, FILE_KIND, RawNode, SYMLINK_KIND};

/// The maximum number of subtree rows scanned in one call, across all of
/// glob/find/search_content.
const WORK_BUDGET: i64 = 5000;
/// The maximum recursion depth of the subtree CTE (a safety bound; ordinary
/// workspaces never approach it).
const MAX_SUBTREE_DEPTH: i64 = 4096;

// --- shared subtree enumeration ---------------------------------------------

struct RawDescendant {
    node: RawNode,
    depth: i64,
    relative_path: String,
}

#[allow(clippy::too_many_arguments)]
fn fetch_subtree_page(
    conn: &Connection,
    workspace_id: &str,
    root_id: &str,
    after: (i64, &str, &str),
    limit: i64,
) -> rusqlite::Result<Vec<RawDescendant>> {
    let (after_depth, after_name, after_id) = after;
    let sql = "WITH RECURSIVE descendants(id, workspace_id, parent_id, name, kind, size, revision, \
             created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, depth, relpath) AS ( \
           SELECT id, workspace_id, parent_id, name, kind, size, revision, \
                  created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, 1, name \
           FROM nodes WHERE workspace_id = ?1 AND parent_id = ?2 AND trashed_at_ms IS NULL \
           UNION ALL \
           SELECT n.id, n.workspace_id, n.parent_id, n.name, n.kind, n.size, n.revision, \
                  n.created_at_ms, n.modified_at_ms, n.accessed_at_ms, n.content_generation_id, \
                  n.symlink_target, d.depth + 1, d.relpath || '/' || n.name \
           FROM nodes n JOIN descendants d ON n.parent_id = d.id \
           WHERE n.trashed_at_ms IS NULL AND d.depth < ?3 \
         ) \
         SELECT id, workspace_id, parent_id, name, kind, size, revision, \
                created_at_ms, modified_at_ms, accessed_at_ms, content_generation_id, symlink_target, depth, relpath \
         FROM descendants \
         WHERE depth > ?4 OR (depth = ?4 AND name > ?5) OR (depth = ?4 AND name = ?5 AND id > ?6) \
         ORDER BY depth, name, id \
         LIMIT ?7";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![
            workspace_id,
            root_id,
            MAX_SUBTREE_DEPTH,
            after_depth,
            after_name,
            after_id,
            limit
        ],
        |row| {
            Ok(RawDescendant {
                node: RawNode {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    name: row.get(3)?,
                    kind: row.get(4)?,
                    size: row.get(5)?,
                    revision: row.get(6)?,
                    created_at_ms: row.get(7)?,
                    modified_at_ms: row.get(8)?,
                    accessed_at_ms: row.get(9)?,
                    content_generation_id: row.get(10)?,
                    symlink_target: row.get(11)?,
                },
                depth: row.get(12)?,
                relative_path: row.get(13)?,
            })
        },
    )?;
    rows.collect()
}

type CursorPos = (i64, String, String);

// --- shared cursor: binds to (workspace_id, root path) plus (depth,name,id) --

#[derive(Serialize, Deserialize)]
struct SearchCursor {
    v: u8,
    workspace_id: String,
    root: String,
    last_depth: i64,
    last_name: String,
    last_id: String,
}

fn encode_search_cursor(workspace_id: WorkspaceId, root: &VirtualPath, pos: &CursorPos) -> String {
    let payload = SearchCursor {
        v: 1,
        workspace_id: workspace_id.to_string(),
        root: root.as_str().to_string(),
        last_depth: pos.0,
        last_name: pos.1.clone(),
        last_id: pos.2.clone(),
    };
    let json = serde_json::to_vec(&payload).expect("cursor payload is serializable");
    URL_SAFE_NO_PAD.encode(json)
}

fn decode_search_cursor(
    raw: &str,
    workspace_id: WorkspaceId,
    root: &VirtualPath,
) -> FsResult<CursorPos> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| FsError::invalid_cursor(raw))?;
    let payload: SearchCursor =
        serde_json::from_slice(&bytes).map_err(|_| FsError::invalid_cursor(raw))?;
    if payload.v != 1
        || payload.workspace_id != workspace_id.to_string()
        || payload.root != root.as_str()
    {
        return Err(FsError::invalid_cursor(raw));
    }
    Ok((payload.last_depth, payload.last_name, payload.last_id))
}

// --- glob --------------------------------------------------------------------

#[derive(Clone)]
enum PatternSegment {
    Literal(String),
    Glob(String),
    DoubleStar,
}

fn compile_pattern(pattern: &str) -> FsResult<Vec<PatternSegment>> {
    if !pattern.starts_with('/') {
        return Err(FsError::invalid_path_or_name(pattern));
    }
    Ok(pattern[1..]
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment == "**" {
                PatternSegment::DoubleStar
            } else if segment.contains('*') || segment.contains('?') {
                PatternSegment::Glob(segment.to_string())
            } else {
                PatternSegment::Literal(segment.to_string())
            }
        })
        .collect())
}

/// Splits a pattern into its leading literal segments (resolved once, up
/// front, to anchor the subtree walk) and the remaining, possibly wildcarded
/// segments (matched per candidate).
fn literal_prefix(segments: &[PatternSegment]) -> (VirtualPath, usize) {
    let mut path = VirtualPath::root();
    let mut count = 0;
    for segment in segments {
        match segment {
            PatternSegment::Literal(name) => {
                path = path.join(name).expect("a segment never contains '/'");
                count += 1;
            }
            _ => break,
        }
    }
    (path, count)
}

/// The classic greedy two-pointer `*`/`?` wildcard match.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut match_from = 0usize;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            match_from = ti;
            pi += 1;
        } else if let Some(star_at) = star {
            pi = star_at + 1;
            match_from += 1;
            ti = match_from;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

fn segment_matches(segment: &PatternSegment, name: &str) -> bool {
    match segment {
        PatternSegment::Literal(expected) => expected == name,
        PatternSegment::Glob(pattern) => wildcard_match(pattern, name),
        PatternSegment::DoubleStar => unreachable!("DoubleStar is handled by glob_match's DP"),
    }
}

/// Matches a (possibly `**`-containing) pattern against a candidate's path
/// segments via dynamic programming, so `**` may consume zero or more
/// segments without combinatorial backtracking.
fn glob_match(pattern: &[PatternSegment], candidate: &[&str]) -> bool {
    let (plen, clen) = (pattern.len(), candidate.len());
    let mut dp = vec![vec![false; clen + 1]; plen + 1];
    dp[0][0] = true;
    for i in 1..=plen {
        if matches!(pattern[i - 1], PatternSegment::DoubleStar) {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=plen {
        for j in 1..=clen {
            dp[i][j] = match &pattern[i - 1] {
                PatternSegment::DoubleStar => dp[i - 1][j] || dp[i][j - 1],
                other => dp[i - 1][j - 1] && segment_matches(other, candidate[j - 1]),
            };
        }
    }
    dp[plen][clen]
}

enum GlobRaw {
    RootNotFound,
    Found {
        matches: Vec<RawNode>,
        next_cursor_pos: Option<CursorPos>,
    },
}

fn glob_tx(
    conn: &Connection,
    workspace_id: &str,
    prefix_path: &VirtualPath,
    remaining: &[PatternSegment],
    after: Option<CursorPos>,
    limit: i64,
) -> rusqlite::Result<GlobRaw> {
    let Some(root) = resolve::resolve(conn, workspace_id, prefix_path)? else {
        return Ok(GlobRaw::RootNotFound);
    };

    let mut matches = Vec::new();

    if after.is_none() && glob_match(remaining, &[]) {
        let pos = (0, String::new(), root.id.clone());
        matches.push(root.clone());
        if matches.len() as i64 >= limit {
            return Ok(GlobRaw::Found {
                matches,
                next_cursor_pos: Some(pos),
            });
        }
    }

    let cursor_pos = after.unwrap_or_default();
    let rows = fetch_subtree_page(
        conn,
        workspace_id,
        &root.id,
        (cursor_pos.0, &cursor_pos.1, &cursor_pos.2),
        WORK_BUDGET + 1,
    )?;
    let budget_exhausted = rows.len() as i64 > WORK_BUDGET;
    let scan_count = if budget_exhausted {
        WORK_BUDGET as usize
    } else {
        rows.len()
    };

    let mut stopped_early = false;
    let mut last_pos: Option<CursorPos> = None;
    for row in rows.into_iter().take(scan_count) {
        let pos = (row.depth, row.node.name.clone(), row.node.id.clone());
        let segments: Vec<&str> = row.relative_path.split('/').collect();
        if glob_match(remaining, &segments) {
            matches.push(row.node);
            if matches.len() as i64 >= limit {
                last_pos = Some(pos);
                stopped_early = true;
                break;
            }
        }
        last_pos = Some(pos);
    }

    let next_cursor_pos = if stopped_early || budget_exhausted {
        last_pos
    } else {
        None
    };

    Ok(GlobRaw::Found {
        matches,
        next_cursor_pos,
    })
}

pub(crate) async fn glob(
    conn: &AsyncConnection,
    workspace_id: WorkspaceId,
    pattern: String,
    page: PageRequest,
) -> FsResult<Page<Node>> {
    let compiled = compile_pattern(&pattern)?;
    let (prefix_path, prefix_len) = literal_prefix(&compiled);
    let remaining = compiled[prefix_len..].to_vec();
    let limit = i64::from(page.limit.max(1));
    let workspace_id_str = workspace_id.to_string();

    let after = match page.cursor.as_deref() {
        Some(raw) => Some(decode_search_cursor(raw, workspace_id, &prefix_path)?),
        None => None,
    };

    let prefix_for_tx = prefix_path.clone();
    let raw = conn
        .call(move |conn| {
            Ok(glob_tx(
                conn,
                &workspace_id_str,
                &prefix_for_tx,
                &remaining,
                after,
                limit,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    match raw {
        GlobRaw::RootNotFound => Ok(Page::new(Vec::new(), None)),
        GlobRaw::Found {
            matches,
            next_cursor_pos,
        } => {
            let items = matches
                .into_iter()
                .map(RawNode::into_node)
                .collect::<FsResult<Vec<_>>>()?;
            let next_cursor =
                next_cursor_pos.map(|pos| encode_search_cursor(workspace_id, &prefix_path, &pos));
            Ok(Page::new(items, next_cursor))
        }
    }
}

// --- find --------------------------------------------------------------------

fn kind_code(kind: NodeKind) -> i64 {
    match kind {
        NodeKind::Directory => DIRECTORY_KIND,
        NodeKind::File => FILE_KIND,
        NodeKind::Symlink => SYMLINK_KIND,
    }
}

fn predicate_matches(node: &RawNode, query: &FindQuery) -> bool {
    if let Some(kind) = query.kind {
        if node.kind != kind_code(kind) {
            return false;
        }
    }
    if let Some(min) = query.min_logical_size {
        if (node.size as u64) < min {
            return false;
        }
    }
    if let Some(max) = query.max_logical_size {
        if (node.size as u64) > max {
            return false;
        }
    }
    if let Some(after) = query.modified_after_ms {
        if node.modified_at_ms <= after {
            return false;
        }
    }
    if let Some(before) = query.modified_before_ms {
        if node.modified_at_ms >= before {
            return false;
        }
    }
    if let Some(substring) = &query.name_contains {
        if !node.name.contains(substring.as_str()) {
            return false;
        }
    }
    true
}

fn attributes_match(
    conn: &Connection,
    node_id: &str,
    required: &BTreeMap<String, Value>,
) -> rusqlite::Result<bool> {
    if required.is_empty() {
        return Ok(true);
    }
    let attributes = mutate::build_attributes_map(conn, node_id)?;
    Ok(required
        .iter()
        .all(|(key, value)| attributes.get(key) == Some(value)))
}

enum FindRaw {
    RootNotFound,
    Found {
        matches: Vec<RawNode>,
        next_cursor_pos: Option<CursorPos>,
    },
}

fn find_tx(
    conn: &Connection,
    workspace_id: &str,
    root_path: &VirtualPath,
    query: &FindQuery,
    after: Option<CursorPos>,
    limit: i64,
) -> rusqlite::Result<FindRaw> {
    let Some(root) = resolve::resolve(conn, workspace_id, root_path)? else {
        return Ok(FindRaw::RootNotFound);
    };

    let mut matches = Vec::new();

    if after.is_none()
        && predicate_matches(&root, query)
        && attributes_match(conn, &root.id, &query.attributes)?
    {
        let pos = (0, String::new(), root.id.clone());
        matches.push(root.clone());
        if matches.len() as i64 >= limit {
            return Ok(FindRaw::Found {
                matches,
                next_cursor_pos: Some(pos),
            });
        }
    }

    let cursor_pos = after.unwrap_or_default();
    let rows = fetch_subtree_page(
        conn,
        workspace_id,
        &root.id,
        (cursor_pos.0, &cursor_pos.1, &cursor_pos.2),
        WORK_BUDGET + 1,
    )?;
    let budget_exhausted = rows.len() as i64 > WORK_BUDGET;
    let scan_count = if budget_exhausted {
        WORK_BUDGET as usize
    } else {
        rows.len()
    };

    let mut stopped_early = false;
    let mut last_pos: Option<CursorPos> = None;
    for row in rows.into_iter().take(scan_count) {
        let pos = (row.depth, row.node.name.clone(), row.node.id.clone());
        let is_match = predicate_matches(&row.node, query)
            && attributes_match(conn, &row.node.id, &query.attributes)?;
        if is_match {
            matches.push(row.node);
            if matches.len() as i64 >= limit {
                last_pos = Some(pos);
                stopped_early = true;
                break;
            }
        }
        last_pos = Some(pos);
    }

    let next_cursor_pos = if stopped_early || budget_exhausted {
        last_pos
    } else {
        None
    };

    Ok(FindRaw::Found {
        matches,
        next_cursor_pos,
    })
}

pub(crate) async fn find(
    conn: &AsyncConnection,
    workspace_id: WorkspaceId,
    query: FindQuery,
    page: PageRequest,
) -> FsResult<Page<Node>> {
    let limit = i64::from(page.limit.max(1));
    let workspace_id_str = workspace_id.to_string();
    let root = query.root.clone();

    let after = match page.cursor.as_deref() {
        Some(raw) => Some(decode_search_cursor(raw, workspace_id, &root)?),
        None => None,
    };

    let root_for_tx = root.clone();
    let raw = conn
        .call(move |conn| {
            Ok(find_tx(
                conn,
                &workspace_id_str,
                &root_for_tx,
                &query,
                after,
                limit,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    match raw {
        FindRaw::RootNotFound => Err(FsError::not_found(root)),
        FindRaw::Found {
            matches,
            next_cursor_pos,
        } => {
            let items = matches
                .into_iter()
                .map(RawNode::into_node)
                .collect::<FsResult<Vec<_>>>()?;
            let next_cursor =
                next_cursor_pos.map(|pos| encode_search_cursor(workspace_id, &root, &pos));
            Ok(Page::new(items, next_cursor))
        }
    }
}

// --- search_content ------------------------------------------------------------

/// The maximum total file bytes streamed in one call, across every file
/// considered. If exhausted mid-file, the next call resumes at that same
/// file's start rather than a persisted byte offset — a deliberate
/// simplification; see the crate's development notes.
const SEARCH_BYTE_BUDGET: u64 = 16 * 1024 * 1024;
/// Bytes of surrounding context captured on each side of a match. Clipped to
/// the chunk(s) currently in the search buffer, so context at the very edge
/// of a 1 MiB chunk boundary may occasionally be shorter than requested.
const PREVIEW_CONTEXT_BYTES: usize = 32;

fn find_all_occurrences(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    (0..=haystack.len() - needle.len())
        .filter(|&start| &haystack[start..start + needle.len()] == needle)
        .collect()
}

struct RawSearchMatch {
    node: RawNode,
    relative_path: String,
    start: u64,
    end: u64,
    preview: Vec<u8>,
}

impl RawSearchMatch {
    fn into_search_match(self, root_path: &VirtualPath) -> FsResult<SearchMatch> {
        let path = if self.relative_path.is_empty() {
            root_path.clone()
        } else {
            root_path.join(&self.relative_path)?
        };
        Ok(SearchMatch {
            node: self.node.into_node()?,
            path,
            range: ByteRange::new(self.start, self.end),
            preview: self.preview,
        })
    }
}

/// Streams one file's chunks searching for `needle`, carrying
/// `needle.len() - 1` overlap bytes between chunks so a match spanning a
/// chunk boundary is never missed, and never holding more than one chunk
/// (plus the small overlap) in memory at a time.
fn search_file(
    conn: &Connection,
    node: &RawNode,
    relative_path: &str,
    needle: &[u8],
    bytes_scanned: &mut u64,
) -> rusqlite::Result<Vec<RawSearchMatch>> {
    let mut results = Vec::new();
    let Some(generation_id) = &node.content_generation_id else {
        return Ok(results);
    };

    let mut overlap: Vec<u8> = Vec::new();
    let mut buffer_base: u64 = 0;
    let mut chunk_index = 0i64;
    let keep = needle.len().saturating_sub(1);

    loop {
        if *bytes_scanned >= SEARCH_BYTE_BUDGET {
            break;
        }
        let chunk = content::fetch_chunk_bytes(conn, generation_id, chunk_index)?;
        if chunk.is_empty() {
            break;
        }
        *bytes_scanned += chunk.len() as u64;

        let mut buffer = std::mem::take(&mut overlap);
        buffer.extend_from_slice(&chunk);

        for start in find_all_occurrences(&buffer, needle) {
            let match_start = buffer_base + start as u64;
            let match_end = match_start + needle.len() as u64;
            let preview_start = start.saturating_sub(PREVIEW_CONTEXT_BYTES);
            let preview_end = (start + needle.len() + PREVIEW_CONTEXT_BYTES).min(buffer.len());
            results.push(RawSearchMatch {
                node: node.clone(),
                relative_path: relative_path.to_string(),
                start: match_start,
                end: match_end,
                preview: buffer[preview_start..preview_end].to_vec(),
            });
        }

        let next_overlap_start = buffer.len().saturating_sub(keep);
        buffer_base += next_overlap_start as u64;
        overlap = buffer[next_overlap_start..].to_vec();
        chunk_index += 1;
    }

    Ok(results)
}

enum SearchRaw {
    RootNotFound,
    Found {
        matches: Vec<RawSearchMatch>,
        next_cursor_pos: Option<CursorPos>,
    },
}

fn search_content_tx(
    conn: &Connection,
    workspace_id: &str,
    root_path: &VirtualPath,
    needle: &[u8],
    after: Option<CursorPos>,
    limit: i64,
) -> rusqlite::Result<SearchRaw> {
    let Some(root) = resolve::resolve(conn, workspace_id, root_path)? else {
        return Ok(SearchRaw::RootNotFound);
    };

    let mut matches = Vec::new();
    let mut bytes_scanned = 0u64;

    if after.is_none() && root.kind == FILE_KIND {
        let file_matches = search_file(conn, &root, "", needle, &mut bytes_scanned)?;
        for found in file_matches {
            matches.push(found);
            if matches.len() as i64 >= limit {
                return Ok(SearchRaw::Found {
                    matches,
                    next_cursor_pos: Some((0, String::new(), root.id.clone())),
                });
            }
        }
        if bytes_scanned >= SEARCH_BYTE_BUDGET {
            return Ok(SearchRaw::Found {
                matches,
                next_cursor_pos: Some((0, String::new(), root.id.clone())),
            });
        }
    }

    let cursor_pos = after.unwrap_or_default();
    let rows = fetch_subtree_page(
        conn,
        workspace_id,
        &root.id,
        (cursor_pos.0, &cursor_pos.1, &cursor_pos.2),
        WORK_BUDGET + 1,
    )?;
    let budget_exhausted_nodes = rows.len() as i64 > WORK_BUDGET;
    let scan_count = if budget_exhausted_nodes {
        WORK_BUDGET as usize
    } else {
        rows.len()
    };

    let mut stopped_early = false;
    let mut last_pos: Option<CursorPos> = None;
    for row in rows.into_iter().take(scan_count) {
        let pos = (row.depth, row.node.name.clone(), row.node.id.clone());

        if row.node.kind == FILE_KIND {
            let file_matches = search_file(
                conn,
                &row.node,
                &row.relative_path,
                needle,
                &mut bytes_scanned,
            )?;
            for found in file_matches {
                matches.push(found);
                if matches.len() as i64 >= limit {
                    stopped_early = true;
                    break;
                }
            }
        }

        last_pos = Some(pos);
        if stopped_early || bytes_scanned >= SEARCH_BYTE_BUDGET {
            stopped_early = true;
            break;
        }
    }

    let next_cursor_pos = if stopped_early || budget_exhausted_nodes {
        last_pos
    } else {
        None
    };

    Ok(SearchRaw::Found {
        matches,
        next_cursor_pos,
    })
}

pub(crate) async fn search_content(
    conn: &AsyncConnection,
    workspace_id: WorkspaceId,
    query: ContentQuery,
    page: PageRequest,
) -> FsResult<Page<SearchMatch>> {
    if query.needle.is_empty() {
        return Err(FsError::invalid_range(
            "content search needle must not be empty",
        ));
    }

    let limit = i64::from(page.limit.max(1));
    let workspace_id_str = workspace_id.to_string();
    let root = query.root.clone();
    let needle = query.needle.clone();

    let after = match page.cursor.as_deref() {
        Some(raw) => Some(decode_search_cursor(raw, workspace_id, &root)?),
        None => None,
    };

    let root_for_tx = root.clone();
    let raw = conn
        .call(move |conn| {
            Ok(search_content_tx(
                conn,
                &workspace_id_str,
                &root_for_tx,
                &needle,
                after,
                limit,
            )?)
        })
        .await
        .map_err(db::map_call_error)?;

    match raw {
        SearchRaw::RootNotFound => Err(FsError::not_found(root)),
        SearchRaw::Found {
            matches,
            next_cursor_pos,
        } => {
            let items = matches
                .into_iter()
                .map(|found| found.into_search_match(&root))
                .collect::<FsResult<Vec<_>>>()?;
            let next_cursor =
                next_cursor_pos.map(|pos| encode_search_cursor(workspace_id, &root, &pos));
            Ok(Page::new(items, next_cursor))
        }
    }
}
