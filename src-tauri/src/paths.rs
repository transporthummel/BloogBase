//! The on-disk data layout (PLAN.md §4.3).

use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub runtimes: PathBuf,
    pub library: PathBuf,
    pub mods: PathBuf,
    pub profiles: PathBuf,
    pub backups: PathBuf,
    pub cache: PathBuf,
    pub configs: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let at = |name: &str| root.join(name);
        AppPaths {
            database: at("bloogbase.db"),
            runtimes: at("runtimes"),
            library: at("library"),
            mods: at("mods"),
            profiles: at("profiles"),
            backups: at("backups"),
            cache: at("cache"),
            configs: at("configs"),
            logs: at("logs"),
            root,
        }
    }

    fn dirs(&self) -> [&Path; 9] {
        [
            &self.root,
            &self.runtimes,
            &self.library,
            &self.mods,
            &self.profiles,
            &self.backups,
            &self.cache,
            &self.configs,
            &self.logs,
        ]
    }

    /// Creates all top-level directories. Safe to call on every start.
    pub fn ensure(&self) -> Result<()> {
        for dir in self.dirs() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_creates_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        for dir in paths.dirs() {
            assert!(dir.is_dir(), "{} missing", dir.display());
        }
        assert_eq!(paths.database, tmp.path().join("data").join("bloogbase.db"));
        // Idempotent.
        paths.ensure().unwrap();
    }
}
