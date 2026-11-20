//! Files of a profile kept apart while others are played: worlds (§9.6) and
//! variants (§9.7). The ones played are in the profile's `state/`, where
//! the game sees them; the others wait in a folder each (`<dir>/<key>/`).
//! A switch moves them file by file, with a journal (`<dir>/swap.json`) so
//! a switch cut short is finished at the next one.

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub(crate) const JOURNAL: &str = "swap.json";

/// A switch under way: from one slot (or none) to another (or none).
#[derive(Debug, Serialize, Deserialize)]
pub struct Swap<K> {
    pub from: Option<K>,
    pub to: Option<K>,
    /// The files of `from` are out of `state/` already.
    pub parked: bool,
}

/// What switching needs to know of the slots of one profile.
pub trait Slots {
    type Key: Serialize + DeserializeOwned + PartialEq + Debug + ToString;

    /// Where the slots not played wait, and the journal lies.
    fn dir(&self) -> &Path;
    /// The profile's `state/`.
    fn upper(&self) -> &Path;
    /// Whether a file in `state/` belongs to the slot played.
    fn belongs(&self, name: &str) -> bool;
    fn active(&self) -> Result<Option<Self::Key>>;
    fn set_active(&self, key: Option<&Self::Key>) -> Result<()>;
    /// For the files of no slot, e.g. saves from before worlds; by default
    /// they stay where they are.
    fn park_none(&self, _names: &[String]) -> Result<()> {
        Ok(())
    }

    /// The folder of slot `key`.
    fn slot_dir(&self, key: &Self::Key) -> PathBuf {
        self.dir().join(key.to_string())
    }

    /// The files of the slot played, in `state/`.
    fn in_state(&self) -> Result<Vec<String>> {
        Ok(file_names(self.upper())?
            .into_iter()
            .filter(|n| self.belongs(n))
            .collect())
    }

    /// Takes the slot played out of `state/`, into its folder.
    fn park(&self, from: Option<&Self::Key>) -> Result<()> {
        let names = self.in_state()?;
        match from {
            Some(key) => move_files(self.upper(), &self.slot_dir(key), &names),
            None => self.park_none(&names),
        }
    }

    /// Puts a slot's files into `state/`.
    fn unpark(&self, to: Option<&Self::Key>) -> Result<()> {
        let Some(key) = to else { return Ok(()) };
        let dir = self.slot_dir(key);
        move_files(&dir, self.upper(), &file_names(&dir)?)?;
        let _ = fs::remove_dir(&dir);
        Ok(())
    }

    /// Finishes a switch cut short, e.g. by a crash.
    fn recover(&self) -> Result<()> {
        let journal = self.dir().join(JOURNAL);
        let swap: Swap<Self::Key> = match fs::read(&journal) {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|e| Error::NotFound(format!("readable {}: {e}", journal.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        tracing::warn!(
            "finishing the switch of {} to {:?}",
            self.dir().display(),
            swap.to
        );
        self.finish(swap)
    }

    fn finish(&self, mut swap: Swap<Self::Key>) -> Result<()> {
        if !swap.parked {
            self.park(swap.from.as_ref())?;
            swap.parked = true;
            write_journal(self.dir(), &swap)?;
        }
        self.unpark(swap.to.as_ref())?;
        self.set_active(swap.to.as_ref())?;
        fs::remove_file(self.dir().join(JOURNAL))?;
        Ok(())
    }

    /// Makes `to` the slot played; with `None`, none.
    fn switch(&self, to: Option<Self::Key>) -> Result<()> {
        self.recover()?;
        let from = self.active()?;
        if from == to && from.is_some() {
            return Ok(());
        }
        let swap = Swap {
            from,
            to,
            parked: false,
        };
        write_journal(self.dir(), &swap)?;
        tracing::info!(
            "{} switches from {:?} to {:?}",
            self.dir().display(),
            swap.from,
            swap.to
        );
        self.finish(swap)
    }
}

pub(crate) fn write_journal<K: Serialize>(dir: &Path, swap: &Swap<K>) -> Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{JOURNAL}.part"));
    fs::write(&tmp, serde_json::to_vec(swap).expect("swap serializes"))?;
    fs::rename(tmp, dir.join(JOURNAL))?;
    Ok(())
}

/// The names of the files right in `dir`.
pub fn file_names(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            out.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

/// Moves `names` from `from` to `to`, replacing what's there.
pub fn move_files(from: &Path, to: &Path, names: &[String]) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for name in names {
        let dest = to.join(name);
        if dest.exists() {
            fs::remove_file(&dest)?;
        }
        fs::rename(from.join(name), dest)?;
    }
    Ok(())
}
