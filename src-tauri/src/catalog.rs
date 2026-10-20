//! Static game definitions shipped with the app (PLAN.md §5).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    Vorticons,
    Galaxy,
    Dreams,
}

#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub title: String,
    pub engine: Engine,
    /// Any one of these must be present.
    pub executables: Vec<String>,
    /// All of these must be present; `A|B` means either.
    pub required_files: Vec<String>,
    /// Patterns for save games.
    pub saves: Vec<String>,
    /// Patterns for other files the game writes (config, high scores).
    pub state_files: Vec<String>,
    pub versions: Vec<GameVersion>,
    /// The release most mods are made for, when the community settled on
    /// one (Keen 4-6: v1.4 EGA). Mods that don't say which version they
    /// need are assumed to need this one.
    #[serde(default)]
    pub modding_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    pub id: String,
    pub label: String,
    /// File name → lowercase hex SHA-256. At least one executable; other
    /// files tell apart releases that share it.
    pub files: BTreeMap<String, String>,
    pub shareware: bool,
    #[serde(default)]
    pub kind: VersionKind,
    /// What patch files call this release in `%version`, if they're made
    /// for it.
    #[serde(default)]
    pub patch_version: Option<String>,
    /// Program to launch instead of the episode executable.
    #[serde(default)]
    pub start: Option<String>,
    /// For releases changed by a third party (e.g. copy protection
    /// removed): the version they were made from. Mods for that version
    /// are offered, but patches may not fit.
    #[serde(default)]
    pub original: Option<String>,
    /// The game files are those of `original`, only packaged differently,
    /// so patches fit too.
    #[serde(default)]
    pub repackaged: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VersionKind {
    #[default]
    Release,
    Beta,
    /// Only part of the game.
    Demo,
}

impl GameVersion {
    /// A third-party build whose code differs from its original.
    pub fn modified(&self) -> bool {
        self.original.is_some() && !self.repackaged
    }
}

#[derive(Debug, Deserialize)]
struct Catalog {
    games: Vec<Game>,
}

static GAMES: LazyLock<Vec<Game>> = LazyLock::new(|| {
    serde_json::from_str::<Catalog>(include_str!("../catalog/games.json"))
        .expect("bundled games.json is invalid")
        .games
});

pub fn games() -> &'static [Game] {
    &GAMES
}

pub fn game(id: &str) -> Option<&'static Game> {
    games().iter().find(|g| g.id == id)
}

impl Game {
    /// Whether `name` (a bare file name) is something the game writes at runtime.
    pub fn is_state_file(&self, name: &str) -> bool {
        self.saves
            .iter()
            .chain(&self.state_files)
            .any(|p| glob_match(p, name))
    }
}

/// Case-insensitive wildcard match supporting `?` (one char) and `*` (any run).
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_ascii_uppercase().chars().collect();
    let n: Vec<char> = name.to_ascii_uppercase().chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while ni < n.len() {
        match p.get(pi) {
            Some('*') => {
                backtrack = Some((pi, ni));
                pi += 1;
            }
            Some(&c) if c == '?' || c == n[ni] => {
                pi += 1;
                ni += 1;
            }
            _ => match backtrack {
                Some((bp, bn)) => {
                    pi = bp + 1;
                    ni = bn + 1;
                    backtrack = Some((bp, bn + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let ids: Vec<_> = games().iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "keen1", "keen2", "keen3", "keen4", "keen5", "keen6", "kdreams"
            ]
        );
        for g in games() {
            assert!(!g.executables.is_empty(), "{}", g.id);
            if let Some(m) = &g.modding_version {
                assert!(
                    g.versions.iter().any(|v| v.id == *m),
                    "{} names unknown {m}",
                    g.id
                );
            }
            // The first match wins: no version may shadow a later one.
            for (i, a) in g.versions.iter().enumerate() {
                for b in &g.versions[i + 1..] {
                    assert!(
                        !a.files.iter().all(|(f, h)| b.files.get(f) == Some(h)),
                        "{} hides {}",
                        a.id,
                        b.id
                    );
                }
            }
            let mut ids = std::collections::HashSet::new();
            for v in &g.versions {
                assert!(v.id.starts_with(&g.id), "{}", v.id);
                assert!(ids.insert(&v.id), "{} twice", v.id);
                assert!(
                    v.files.keys().any(|f| g.executables.contains(f)),
                    "{} hashes no executable",
                    v.id
                );
                assert!(!v.repackaged || v.original.is_some(), "{}", v.id);
                assert!(
                    v.original.is_none() || v.patch_version.is_none(),
                    "{}: builds patch as their original",
                    v.id
                );
                if let Some(original) = &v.original {
                    assert!(
                        g.versions
                            .iter()
                            .any(|o| o.id == *original && o.original.is_none()),
                        "{} names unknown original {original}",
                        v.id
                    );
                }
                for hash in v.files.values() {
                    assert!(
                        hash.len() == 64
                            && hash
                                .bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                        "{} has a malformed hash",
                        v.id
                    );
                }
            }
        }
    }

    #[test]
    fn glob() {
        assert!(glob_match("SAVEGAM?.CK4", "savegam3.ck4"));
        assert!(!glob_match("SAVEGAM?.CK4", "SAVEGAM10.CK4"));
        assert!(glob_match("*.CK4", "CONFIG.CK4"));
        assert!(glob_match("A*B*C", "AxxBxxBxC"));
        assert!(!glob_match("A*B", "AxxBx"));
        assert!(glob_match("CONFIG.CK4", "Config.Ck4"));
    }

    #[test]
    fn state_files() {
        let k4 = game("keen4").unwrap();
        assert!(k4.is_state_file("SAVEGAM0.CK4"));
        assert!(k4.is_state_file("config.ck4"));
        assert!(!k4.is_state_file("GAMEMAPS.CK4"));
    }
}
