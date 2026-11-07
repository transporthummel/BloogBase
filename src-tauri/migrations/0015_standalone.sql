-- Index mods that ship their whole game (source mods such as Foray in the
-- Forest) and play without a copy of the game they're built on; their
-- profile has no install. Set from the index entry.
ALTER TABLE mods ADD COLUMN standalone INTEGER NOT NULL DEFAULT 0 CHECK (standalone IN (0, 1));
