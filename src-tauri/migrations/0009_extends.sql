-- Index mods that are played over another index mod (e.g. a level pack for
-- a mod): that mod's index id. Its files go between the game and these.
ALTER TABLE mods ADD COLUMN extends TEXT;
