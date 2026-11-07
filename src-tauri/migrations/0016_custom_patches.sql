-- Patch file lines the player added for a game copy; they apply to the
-- game and to every mod played on it. NULL when there are none.
ALTER TABLE installs ADD COLUMN custom_patches TEXT;
