CREATE TABLE microsoft_accounts (
    profile_id TEXT PRIMARY KEY REFERENCES local_profiles(id) ON DELETE CASCADE,
    minecraft_uuid TEXT UNIQUE NOT NULL,
    refresh_secret BLOB NOT NULL
);
