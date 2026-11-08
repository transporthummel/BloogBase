-- Index mods composed from their own files alone, with no game files below
-- them: every standalone mod, and source mods that need their game only to
-- be played at all (`requiresGame`, as they carry the game's own graphics
-- or sounds). Set from the index entry.
ALTER TABLE mods ADD COLUMN own_files INTEGER NOT NULL DEFAULT 0 CHECK (own_files IN (0, 1));
UPDATE mods SET own_files = standalone;
