-- Mods can wait for their game: a mod profile may have no install until a
-- fitting one is added (PLAN.md §2.8). Runs with foreign keys off, so
-- dropping the old table keeps the play sessions that point at it.

DROP VIEW profile_stats;

CREATE TABLE profiles_new (
    id             TEXT PRIMARY KEY,
    install_id     TEXT REFERENCES installs (id),
    mod_id         TEXT REFERENCES mods (id),
    state_path     TEXT NOT NULL,
    composed_hash  TEXT,
    created_at     INTEGER NOT NULL,
    CHECK (install_id IS NOT NULL OR mod_id IS NOT NULL)
) STRICT;

INSERT INTO profiles_new (id, install_id, mod_id, state_path, composed_hash, created_at)
SELECT id, install_id, mod_id, state_path, composed_hash, created_at FROM profiles;

DROP TABLE profiles;
ALTER TABLE profiles_new RENAME TO profiles;

CREATE INDEX profiles_install_id ON profiles (install_id);
CREATE INDEX profiles_mod_id ON profiles (mod_id);

CREATE VIEW profile_stats AS
SELECT
    p.id                                           AS profile_id,
    COUNT(s.id)                                    AS session_count,
    COALESCE(SUM(s.ended_at - s.started_at), 0)    AS play_seconds,
    MAX(s.started_at)                              AS last_played_at
FROM profiles p
LEFT JOIN play_sessions s ON s.profile_id = p.id
GROUP BY p.id;

-- The game versions a mod plays on, to find an install for it later: a JSON
-- array of catalog version ids, NULL for any version. Installs of an unknown
-- version always qualify.
ALTER TABLE mods ADD COLUMN requires_json TEXT;
