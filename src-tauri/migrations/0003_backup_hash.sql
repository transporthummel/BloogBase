-- Hash over a backup's files, so automatic backups can skip unchanged state.
ALTER TABLE save_backups ADD COLUMN content_hash TEXT;
