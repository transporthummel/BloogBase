-- What a mod session ran with (base game, mod files, recipe), so the first
-- session with a new setup can be told apart from the ones after it.
ALTER TABLE play_sessions ADD COLUMN setup TEXT;
