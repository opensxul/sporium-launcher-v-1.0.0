CREATE TABLE collections (
    id TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload))
);

CREATE TABLE instances (
    id TEXT PRIMARY KEY NOT NULL,
    collection_id TEXT REFERENCES collections(id) ON DELETE RESTRICT,
    payload TEXT NOT NULL CHECK (json_valid(payload))
);
CREATE INDEX instances_by_collection ON instances(collection_id);

-- Durable intentions bridge SQLite transactions and atomic directory renames.
CREATE TABLE instance_operations (
    id TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload))
);
