-- Index mods whose entry names the folder of the archive they're installed
-- from (`root`, "" for the archive's root): that folder. NULL when the app
-- found it, as for every mod installed before.
ALTER TABLE mods ADD COLUMN root TEXT;

-- The folders right in such a mod's root the game keeps saves and settings
-- in, besides the root (the entry's `saveDirs`, JSON). NULL for other mods.
ALTER TABLE mods ADD COLUMN save_dirs_json TEXT;

-- Where its cover comes from when the root only holds a launcher (the
-- entry's `coverFrom`, JSON).
ALTER TABLE mods ADD COLUMN cover_from_json TEXT;
