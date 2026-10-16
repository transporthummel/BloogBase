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
    /// All of these must be present.
    pub required_files: Vec<String>,
    /// Patterns for save games.
    pub saves: Vec<String>,
    /// Patterns for other files the game writes (config, high scores).
    pub state_files: Vec<String>,
    pub versions: Vec<GameVersion>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    pub id: String,
    pub label: String,
    /// File name → lowercase hex SHA-256.
    pub files: BTreeMap<String, String>,
    pub shareware: bool,
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
            for v in &g.versions {
                assert!(v.id.starts_with(&g.id), "{}", v.id);
                for (file, hash) in &v.files {
                    assert!(
                        g.executables.contains(file),
                        "{} hashes unknown {file}",
                        v.id
                    );
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
