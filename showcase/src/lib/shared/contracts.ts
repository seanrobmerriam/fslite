/** JSON values accepted by fslite's application-defined metadata fields. */
export type JsonValue =
  boolean | number | string | null | JsonValue[] | { [key: string]: JsonValue };

import type { VirtualPath } from "./path";

export type NodeKind = "directory" | "file" | "symlink";

/** Browser representation of fslite_core::Node's serialized fields. */
export interface Node {
  workspace_id: string;
  id: string;
  parent_id: string | null;
  name: string;
  kind: NodeKind;
  logical_size: number;
  created_at_ms: number;
  modified_at_ms: number;
  accessed_at_ms: number;
  revision: number;
  attributes: Record<string, JsonValue>;
}

/** Browser representation of fslite_core::TreeEntry's serialized fields. */
export interface TreeEntry {
  path: VirtualPath;
  depth: number;
  node: Node;
}

/** Browser representation of fslite_core::TrashEntry's serialized fields. */
export interface TrashEntry {
  id: string;
  node: Node;
  original_path: VirtualPath;
  trashed_at_ms: number;
  actor_metadata: Record<string, JsonValue>;
}

export type ChangeKind =
  | "created"
  | "modified"
  | "copied"
  | "moved"
  | "removed"
  | "trashed"
  | "restored"
  | "purged"
  | "attribute_set"
  | "attribute_removed";

/** Browser representation of fslite_core::Change's serialized fields. */
export interface Change {
  sequence: number;
  kind: ChangeKind;
  node_id: string | null;
  old_path: VirtualPath | null;
  new_path: VirtualPath | null;
  revision: number | null;
  created_at_ms: number;
  actor_metadata: Record<string, JsonValue>;
}

/** Browser representation of fslite_core::WorkspaceUsage's serialized fields. */
export interface WorkspaceUsage {
  workspace_id: string;
  active_logical_bytes: number;
  trashed_logical_bytes: number;
  staged_bytes: number;
  active_nodes: number;
  trashed_nodes: number;
  max_logical_bytes: number;
  max_nodes: number;
  max_file_bytes: number;
}

/**
 * A redacted request/response record shown by the browser activity feed.
 * Headers are intentionally excluded because they can contain credentials.
 */
export interface ActivityRecord {
  id: string;
  timestamp: string;
  method: string;
  path: string;
  status: number;
  durationMs: number;
  requestId: string;
  request: JsonValue | null;
  response: JsonValue | null;
  curl: string;
}

export interface GatewayResult<T> {
  data: T;
  activity: ActivityRecord;
}
