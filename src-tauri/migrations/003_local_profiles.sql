CREATE TABLE local_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    nickname TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE profile_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL,
    active_id TEXT NOT NULL REFERENCES local_profiles(id)
);
