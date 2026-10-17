//! Screenshots the user took in DOSBox (PLAN.md §8.3). DOSBox writes them to
//! the profile's capture folder, which the launch config points at.

use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use specta::Type;

use crate::error::{Error, Result};
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    /// File name in the capture folder.
    pub file: String,
    /// Absolute path, for the asset protocol.
    pub path: String,
    /// Unix epoch seconds.
    pub taken_at: Option<u32>,
    pub size: u32,
}

fn is_image(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".png")
}

/// The profile's screenshots, newest first.
pub fn list(paths: &AppPaths, profile_id: &str) -> Result<Vec<Screenshot>> {
    let dir = paths.profile(profile_id).capture;
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut shots = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let Ok(file) = entry.file_name().into_string() else {
            continue;
        };
        if !meta.is_file() || !is_image(&file) {
            continue;
        }
        let taken_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as u32);
        shots.push(Screenshot {
            path: entry.path().display().to_string(),
            file,
            taken_at,
            size: meta.len().min(u32::MAX as u64) as u32,
        });
    }
    shots.sort_by(|a, b| b.taken_at.cmp(&a.taken_at).then(b.file.cmp(&a.file)));
    Ok(shots)
}

/// A screenshot's path; refuses names that would leave the capture folder.
fn path(paths: &AppPaths, profile_id: &str, file: &str) -> Result<PathBuf> {
    if file.is_empty() || file.contains(['/', '\\']) || file == ".." || !is_image(file) {
        return Err(Error::NotFound(format!("screenshot {file}")));
    }
    let path = paths.profile(profile_id).capture.join(file);
    if !path.is_file() {
        return Err(Error::NotFound(format!("screenshot {file}")));
    }
    Ok(path)
}

pub fn delete(paths: &AppPaths, profile_id: &str, file: &str) -> Result<()> {
    fs::remove_file(path(paths, profile_id, file)?)?;
    tracing::info!("deleted screenshot {file} of {profile_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_pngs_newest_first_and_deletes_safely() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path());
        assert!(list(&paths, "keen4-1").unwrap().is_empty());

        let dir = paths.profile("keen4-1").capture;
        fs::create_dir_all(dir.join("sub")).unwrap();
        for (name, age) in [
            ("image0001.png", 20),
            ("IMAGE0002.PNG", 10),
            ("audio0001.wav", 0),
        ] {
            let file = fs::File::create(dir.join(name)).unwrap();
            let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            file.set_modified(when).unwrap();
        }
        fs::write(dir.join("sub").join("image0003.png"), b"").unwrap();

        let shots = list(&paths, "keen4-1").unwrap();
        let names: Vec<_> = shots.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(names, ["IMAGE0002.PNG", "image0001.png"]);
        assert!(PathBuf::from(&shots[0].path).is_absolute());

        for bad in [
            "",
            "..",
            "sub/image0003.png",
            "..\\x.png",
            "audio0001.wav",
            "gone.png",
        ] {
            assert!(
                matches!(delete(&paths, "keen4-1", bad), Err(Error::NotFound(_))),
                "{bad}"
            );
        }
        delete(&paths, "keen4-1", "image0001.png").unwrap();
        assert_eq!(list(&paths, "keen4-1").unwrap().len(), 1);
    }
}
