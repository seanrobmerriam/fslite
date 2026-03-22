CREATE TABLE schema_migrations(
    version INTEGER PRIMARY KEY,
    applied_at_ms INTEGER NOT NULL
);

CREATE TABLE workspaces(
    id BLOB PRIMARY KEY,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    change_seq INTEGER NOT NULL DEFAULT 0,
    max_bytes INTEGER NOT NULL,
    max_nodes INTEGER NOT NULL,
    max_file_bytes INTEGER NOT NULL
);

CREATE TABLE content_generations(
    id BLOB PRIMARY KEY,
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    complete INTEGER NOT NULL DEFAULT 0,
    length INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE nodes(
    id BLOB PRIMARY KEY,
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    parent_id BLOB REFERENCES nodes(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind INTEGER NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    modified_at_ms INTEGER NOT NULL,
    accessed_at_ms INTEGER NOT NULL,
    content_generation_id BLOB REFERENCES content_generations(id),
    symlink_target TEXT,
    trashed_at_ms INTEGER
);

CREATE TABLE content_chunks(
    generation_id BLOB NOT NULL REFERENCES content_generations(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY(generation_id, chunk_index)
);

CREATE TABLE attributes(
    node_id BLOB NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value BLOB NOT NULL,
    PRIMARY KEY(node_id, key)
);

CREATE TABLE trash(
    id BLOB PRIMARY KEY,
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    node_id BLOB NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    original_parent_id BLOB NOT NULL,
    original_name TEXT NOT NULL,
    deleted_at_ms INTEGER NOT NULL
);

CREATE TABLE changes(
    workspace_id BLOB NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    kind TEXT NOT NULL,
    node_id BLOB,
    old_path TEXT,
    new_path TEXT,
    revision INTEGER,
    created_at_ms INTEGER NOT NULL,
    actor_json TEXT NOT NULL,
    PRIMARY KEY(workspace_id, sequence)
);

CREATE UNIQUE INDEX nodes_parent_active ON nodes(workspace_id, parent_id, name) WHERE trashed_at_ms IS NULL;
CREATE INDEX changes_feed ON changes(workspace_id, sequence);
