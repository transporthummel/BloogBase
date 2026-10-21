-- What each backup holds, file by file, so earlier versions of a save can be
-- found without opening every archive (PLAN.md §11.1). Filled for older
-- backups from their archives on start.
CREATE TABLE backup_files (
    backup_id  INTEGER NOT NULL REFERENCES save_backups (id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    hash       TEXT NOT NULL,
    PRIMARY KEY (backup_id, path)
) STRICT;
