-- The mod's thread on the Keen: Modding forum, as its KeenWiki page links it.
ALTER TABLE mods ADD COLUMN keenmodding TEXT;
-- Its first discussion link was kept as the PCKF one, whatever the forum.
UPDATE mods SET keenmodding = pckf, pckf = NULL WHERE pckf LIKE '%keenmodding.org/%';
