//! The on-disk data layout (PLAN.md §4.3).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// In the app config dir: the data root the user chose, if not the default.
const LOCATION_FILE: &str = "data-location.txt";

/// The data root: the user's choice if one was saved, else `default`.
pub fn resolve_root(config_dir: &Path, default: PathBuf) -> PathBuf {
    match std::fs::read_to_string(config_dir.join(LOCATION_FILE)) {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s.trim()),
        _ => default,
    }
}

/// Remembers `root` as the data root for the next start; the default root
/// removes the pointer.
pub fn save_root(config_dir: &Path, default: &Path, root: &Path) -> Result<()> {
    let file = config_dir.join(LOCATION_FILE);
    if root == default {
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => return Ok(()),
        }
    }
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(file, root.display().to_string())?;
    Ok(())
}

/// The data root to use inside a folder the user picked: the folder itself
/// if it's empty or already holds BloogBase data, else a new `BloogBase`
/// subfolder. Makes sure it can be written to.
pub fn root_in(chosen: &Path) -> Result<PathBuf> {
    if !chosen.is_absolute() {
        return Err(Error::InvalidLocation(format!(
            "{} is not an absolute path",
            chosen.display()
        )));
    }
    let empty = match std::fs::read_dir(chosen) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(Error::InvalidLocation(e.to_string())),
    };
    let root = if empty || chosen.join("bloogbase.db").is_file() {
        chosen.to_path_buf()
    } else {
        chosen.join("BloogBase")
    };
    let probe = root.join(".bloogbase-write-test");
    std::fs::create_dir_all(&root)
        .and_then(|()| std::fs::write(&probe, b""))
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|e| Error::InvalidLocation(format!("{}: {e}", root.display())))?;
    Ok(root)
}

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

/// Per-profile directories (PLAN.md §4.3).
#[derive(Debug, Clone)]
pub struct ProfileDirs {
    pub config: PathBuf,
    pub override_conf: PathBuf,
    /// Written on every launch, never edited by the user.
    pub launch_conf: PathBuf,
    /// The defaults layer of a mod that needs more than its engine's.
    pub defaults_conf: PathBuf,
    pub capture: PathBuf,
    /// First frames of the videos in `capture`.
    pub video_thumbs: PathBuf,
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

    pub fn profile(&self, id: &str) -> ProfileDirs {
        let root = self.profiles.join(id);
        let config = root.join("config");
        ProfileDirs {
            override_conf: config.join("override.conf"),
            launch_conf: root.join("generated").join("launch.conf"),
            defaults_conf: root.join("generated").join("defaults.conf"),
            capture: root.join("capture"),
            video_thumbs: root.join("generated").join("video-thumbs"),
            config,
        }
    }

    /// Shipped per-engine defaults, written out so DOSBox can read them.
    pub fn default_conf(&self, engine: &str) -> PathBuf {
        self.configs.join("defaults").join(format!("{engine}.conf"))
    }

    /// A mod profile's lower layer: base files with the mod on top (§4.2).
    pub fn composed(&self, profile_id: &str) -> PathBuf {
        self.cache.join("composed").join(profile_id)
    }

    /// The options from the settings page, as a config layer.
    pub fn options_conf(&self) -> PathBuf {
        self.configs.join("options.conf")
    }

    pub fn global_override_conf(&self) -> PathBuf {
        self.configs.join("global-override.conf")
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

    #[test]
    fn location_pointer_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let default = tmp.path().join("default");
        let custom = tmp.path().join("custom");
        assert_eq!(resolve_root(&config, default.clone()), default);
        save_root(&config, &default, &custom).unwrap();
        assert_eq!(resolve_root(&config, default.clone()), custom);
        save_root(&config, &default, &default).unwrap();
        assert_eq!(resolve_root(&config, default.clone()), default);
        // Saving the default twice is fine.
        save_root(&config, &default, &default).unwrap();
    }

    #[test]
    fn root_in_uses_empty_or_existing_folders_and_nests_otherwise() {
        let tmp = tempfile::tempdir().unwrap();
        let fresh = tmp.path().join("new");
        assert_eq!(root_in(&fresh).unwrap(), fresh);
        assert!(fresh.is_dir());

        let existing = tmp.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        std::fs::write(existing.join("bloogbase.db"), b"").unwrap();
        assert_eq!(root_in(&existing).unwrap(), existing);

        let busy = tmp.path().join("Documents");
        std::fs::create_dir(&busy).unwrap();
        std::fs::write(busy.join("letter.txt"), b"").unwrap();
        assert_eq!(root_in(&busy).unwrap(), busy.join("BloogBase"));

        assert!(matches!(
            root_in(Path::new("relative")),
            Err(Error::InvalidLocation(_))
        ));
    }
}
