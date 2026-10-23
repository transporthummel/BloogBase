-- Index mods may start differently on some game versions: JSON object of
-- catalog version id → recipe, used instead of recipe_json there.
ALTER TABLE mods ADD COLUMN version_recipes_json TEXT;
