-- Index mods that generate their levels at every start: how their sets are
-- kept as worlds (the index entry's `worlds`, JSON). NULL for other mods.
ALTER TABLE mods ADD COLUMN worlds_json TEXT;

-- The worlds of a profile (PLAN.md §9.6). `id` counts up per profile and
-- names the world's folder, `profiles/<profile>/worlds/<id>/`, which holds
-- its files and saves while another one is played. `name` is the player's;
-- without one the app names it after its seed or number.
CREATE TABLE worlds (
    profile_id      TEXT NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    id              INTEGER NOT NULL,
    name            TEXT,
    seed            INTEGER,
    options_json    TEXT NOT NULL DEFAULT '{}',
    created_at      INTEGER NOT NULL,
    last_played_at  INTEGER NOT NULL,
    favourite       INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0, 1)),
    PRIMARY KEY (profile_id, id)
) STRICT;

-- The world whose files and saves are in `state/`.
ALTER TABLE profiles ADD COLUMN active_world INTEGER;

-- The world a backup was made of.
ALTER TABLE save_backups ADD COLUMN world_id INTEGER;
