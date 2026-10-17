//! How much disk space BloogBase's data takes, per area of the data folder
//! (PLAN.md §4.3).

use std::path::Path;

use serde::Serialize;
use specta::Type;
use walkdir::WalkDir;

use crate::paths::AppPaths;

/// Sizes in bytes. Floats, as JavaScript numbers can't hold every u64.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    /// The games' own files.
    pub games: f64,
    pub mods: f64,
    /// Saves, configs and screenshots of all profiles.
    pub profiles: f64,
    pub backups: f64,
    /// DOSBox.
    pub runtimes: f64,
    /// Composed mod folders and covers; rebuilt when needed.
    pub cache: f64,
    /// Everything in the data folder, including the database and logs.
    pub total: f64,
}

/// Bytes of all files below `dir`; unreadable entries are skipped.
fn size(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn usage(paths: &AppPaths) -> StorageUsage {
    StorageUsage {
        games: size(&paths.library) as f64,
        mods: size(&paths.mods) as f64,
        profiles: size(&paths.profiles) as f64,
        backups: size(&paths.backups) as f64,
        runtimes: size(&paths.runtimes) as f64,
        cache: size(&paths.cache) as f64,
        total: size(&paths.root) as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_each_area_and_the_whole_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path());
        paths.ensure().unwrap();
        std::fs::create_dir_all(paths.library.join("keen4-1/base")).unwrap();
        std::fs::write(paths.library.join("keen4-1/base/KEEN4E.EXE"), [0; 100]).unwrap();
        std::fs::write(paths.mods.join("x.zip"), [0; 20]).unwrap();
        std::fs::write(&paths.database, [0; 3]).unwrap();

        let u = usage(&paths);
        assert_eq!((u.games, u.mods, u.profiles), (100.0, 20.0, 0.0));
        assert_eq!(u.total, 123.0);
    }
}
