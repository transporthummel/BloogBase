-- What the index or the KeenWiki page says about a mod, for its detail page.
ALTER TABLE mods ADD COLUMN description TEXT;
ALTER TABLE mods ADD COLUMN released TEXT;
