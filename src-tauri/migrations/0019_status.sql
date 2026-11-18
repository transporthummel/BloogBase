-- How far along a mod is, as its KeenWiki page's "Status" says: complete,
-- alpha, beta or abandoned. Index mods take it from the index instead.
ALTER TABLE mods ADD COLUMN status TEXT;
