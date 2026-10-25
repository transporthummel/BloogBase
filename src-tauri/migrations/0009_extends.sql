-- Mods that are played over another installed mod (e.g. a level pack for a
-- mod): that mod's id. Its files go between the game and these.
ALTER TABLE mods ADD COLUMN extends TEXT;
