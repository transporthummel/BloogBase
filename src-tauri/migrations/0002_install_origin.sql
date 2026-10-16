-- Where an install came from: the imported folder or ZIP, or the download URL.
-- Lets the setup wizard tell what's already in the library.
ALTER TABLE installs ADD COLUMN origin TEXT;
