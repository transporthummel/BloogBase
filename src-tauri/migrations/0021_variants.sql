-- Index mods with variants (PLAN.md §9.7): the entry's `variants` as
-- installed (JSON). Their files are in `variants/<id>/` of the mod's
-- folder instead of `files/`. NULL for other mods.
ALTER TABLE mods ADD COLUMN variants_json TEXT;

-- The variant played; its saves are in `state/`, the others' wait in
-- `variants/<id>/` of the profile's folder.
ALTER TABLE profiles ADD COLUMN active_variant TEXT;

-- The player chose the variant: until then "Play" asks.
ALTER TABLE profiles ADD COLUMN variant_chosen INTEGER NOT NULL DEFAULT 0
    CHECK (variant_chosen IN (0, 1));

-- The variant a backup was made of.
ALTER TABLE save_backups ADD COLUMN variant TEXT;
