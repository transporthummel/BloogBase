-- Initial schema (PLAN.md §12). Timestamps are Unix epoch seconds.

CREATE TABLE installs (
    id          TEXT PRIMARY KEY,
    game_id     TEXT NOT NULL,
    version_id  TEXT,
    label       TEXT NOT NULL,
    source      TEXT NOT NULL CHECK (source IN ('folder', 'zip', 'download', 'steam', 'gog')),
    path        TEXT NOT NULL,
    added_at    INTEGER NOT NULL,
    cover_path  TEXT
) STRICT;

CREATE TABLE mods (
    id              TEXT PRIMARY KEY,
    game_id         TEXT NOT NULL,
    index_id        TEXT,
    keenwiki_url    TEXT,
    name            TEXT NOT NULL,
    authors         TEXT NOT NULL,  -- JSON array of names
    version         TEXT,
    recipe_json     TEXT NOT NULL,
    path            TEXT NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('index', 'keenwiki', 'manual')),
    source_url      TEXT,
    archive_sha256  TEXT,
    added_at        INTEGER NOT NULL
) STRICT;

CREATE TABLE profiles (
    id             TEXT PRIMARY KEY,
    install_id     TEXT NOT NULL REFERENCES installs (id),
    mod_id         TEXT REFERENCES mods (id),
    state_path     TEXT NOT NULL,
    composed_hash  TEXT,
    created_at     INTEGER NOT NULL
) STRICT;

CREATE INDEX profiles_install_id ON profiles (install_id);
CREATE INDEX profiles_mod_id ON profiles (mod_id);

CREATE TABLE play_sessions (
    id          INTEGER PRIMARY KEY,
    profile_id  TEXT NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,
    exit_code   INTEGER
) STRICT;

CREATE INDEX play_sessions_profile_id ON play_sessions (profile_id, started_at);

-- No foreign key on profile_id: backups must outlive a deleted profile (§9.5).
CREATE TABLE save_backups (
    id            INTEGER PRIMARY KEY,
    profile_id    TEXT NOT NULL,
    label         TEXT,
    archive_path  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    files_json    TEXT NOT NULL,
    auto          INTEGER NOT NULL DEFAULT 0 CHECK (auto IN (0, 1))
) STRICT;

CREATE INDEX save_backups_profile_id ON save_backups (profile_id, created_at);

CREATE TABLE runtimes (
    id       TEXT PRIMARY KEY,
    kind     TEXT NOT NULL,
    version  TEXT NOT NULL,
    path     TEXT NOT NULL,
    managed  INTEGER NOT NULL CHECK (managed IN (0, 1))
) STRICT;

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
) STRICT;

-- Aggregates per profile; unfinished sessions don't count towards play time.
CREATE VIEW profile_stats AS
SELECT
    p.id                                           AS profile_id,
    COUNT(s.id)                                    AS session_count,
    COALESCE(SUM(s.ended_at - s.started_at), 0)    AS play_seconds,
    MAX(s.started_at)                              AS last_played_at
FROM profiles p
LEFT JOIN play_sessions s ON s.profile_id = p.id
GROUP BY p.id;
