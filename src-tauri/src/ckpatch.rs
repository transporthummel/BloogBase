//! CKPatch for mods that run it but don't ship it: their README says to put
//! it next to the game first (Robot Rebellion's ROBOKEEN.EXE runs `ck1patch
//! ckrr.pat`). Unmodified: CKPatch 0.10 for Keen 1-6, and MultiMania's
//! KDrPatch 0.9.0, built on CKPatch 0.9, for Keen Dreams. GPL 2; licenses
//! and sources: `assets/ckpatch/`, `assets/kdrpatch/`.

use crate::recipe::{self, has_ext};

const PATCHERS: [(&str, &str, &[u8]); 7] = [
    (
        "kdreams",
        "KDRPATCH.EXE",
        include_bytes!("../assets/kdrpatch/KDRPATCH.EXE"),
    ),
    (
        "keen1",
        "CK1PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK1PATCH.EXE"),
    ),
    (
        "keen2",
        "CK2PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK2PATCH.EXE"),
    ),
    (
        "keen3",
        "CK3PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK3PATCH.EXE"),
    ),
    (
        "keen4",
        "CK4PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK4PATCH.EXE"),
    ),
    (
        "keen5",
        "CK5PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK5PATCH.EXE"),
    ),
    (
        "keen6",
        "CK6PATCH.EXE",
        include_bytes!("../assets/ckpatch/CK6PATCH.EXE"),
    ),
];

/// The patcher BloogBase has for the game: its name and the program.
pub fn bundled(game_id: &str) -> Option<(&'static str, &'static [u8])> {
    PATCHERS
        .iter()
        .find(|(g, ..)| *g == game_id)
        .map(|(_, name, exe)| (*name, *exe))
}

/// Whether a mod of `game_id` needs the bundled patcher: it has no patcher
/// of its own and a patch file for the game. `files` are the bare names in
/// its root with their text, if readable.
pub fn needed<'a>(
    game_id: &str,
    mut files: impl Iterator<Item = (&'a str, Option<String>)>,
    is_patcher: impl Fn(&str) -> bool,
) -> bool {
    let Some((patcher, _)) = bundled(game_id) else {
        return false;
    };
    // "CK1" or "KDR"; `%ext` names the episode's data files.
    let ext = &patcher[..3];
    let mut patch = false;
    for (name, text) in files.by_ref() {
        if is_patcher(name) {
            return false;
        }
        patch |= text.is_some_and(|t| {
            (has_ext(name, "PAT") || recipe::is_patch_text(name, &t))
                && recipe::patch_ext(&t).as_deref() == Some(ext)
        });
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needs(game: &str, files: &[(&str, &str)]) -> bool {
        needed(
            game,
            files.iter().map(|(n, t)| (*n, Some(t.to_string()))),
            recipe::is_patcher,
        )
    }

    #[test]
    fn patch_files_without_a_patcher_need_one() {
        let pat = "%ext ck1\n%patch $10 $01\n%end\n";
        assert!(needs("keen1", &[("ROBOKEEN.EXE", ""), ("ckrr.pat", pat)]));
        assert!(!needs("keen1", &[("CK1PATCH.EXE", ""), ("ckrr.pat", pat)]));
        // For another episode, or no patch at all.
        assert!(!needs("keen2", &[("ckrr.pat", pat)]));
        assert!(!needs("keen1", &[("LEVEL01.CK1", "")]));
        assert!(needs("keen4", &[("m.pat", "%ext ck4\n%end\n")]));
        assert!(needs("kdreams", &[("m.pat", "%ext kdr\n%end\n")]));
        assert!(!needs(
            "kdreams",
            &[("m.pat", "%ext kdr\n"), ("KDRPATCH.EXE", "")]
        ));
        assert_eq!(bundled("keen6").map(|(n, _)| n), Some("CK6PATCH.EXE"));
        assert_eq!(bundled("keen7"), None);
    }
}
