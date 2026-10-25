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
    let mut patch = false;
    for (name, text) in files.by_ref() {
        if is_patcher(name) {
            return false;
        }
        patch |= text.is_some_and(|t| is_patch_for(game_id, name, &t));
    }
    patch
}

/// Whether `name` is a patch file for the game's bundled patcher.
pub fn is_patch_for(game_id: &str, name: &str, text: &str) -> bool {
    let Some((patcher, _)) = bundled(game_id) else {
        return false;
    };
    // "CK1" or "KDR"; `%ext` names the episode's data files.
    let ext = &patcher[..3];
    (has_ext(name, "PAT") || recipe::is_patch_text(name, text))
        && recipe::patch_ext(text).as_deref() == Some(ext)
}

/// A patch file as CKPatch 0.10 reads it the way 0.9 and earlier did, if
/// that differs: those wrote just the low byte of a `%patch` value over
/// $FF, where 0.10 stops with "Invalid numeric format" unless it has a
/// `W` or `L` for a word or long. Robot Rebellion (2004) has
/// `%patch $81D6 $1000`.
pub fn as_older_versions(text: &str) -> Option<String> {
    let mut values = false;
    let mut changed = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (code, comment) = line.split_at(line.find('#').unwrap_or(line.len()));
        let mut words = code.split_whitespace();
        let first = words.next();
        if let Some(directive) = first.filter(|w| w.starts_with('%')) {
            values = directive.eq_ignore_ascii_case("%patch");
        }
        // Quoted strings are left as they are, and so is their line.
        if !values || code.contains('"') {
            out.push_str(line);
            continue;
        }
        // The offset follows `%patch` on its line.
        let skip = if first.is_some_and(|w| w.starts_with('%')) {
            2
        } else {
            0
        };
        let mut rest = code;
        let mut index = 0;
        while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
            let end = rest[start..]
                .find(char::is_whitespace)
                .map_or(rest.len(), |e| start + e);
            out.push_str(&rest[..start]);
            let word = &rest[start..end];
            match (index >= skip).then(|| low_byte(word)).flatten() {
                Some(byte) => {
                    out.push_str(&byte);
                    changed = true;
                }
                None => out.push_str(word),
            }
            index += 1;
            rest = &rest[end..];
        }
        out.push_str(rest);
        out.push_str(comment);
    }
    changed.then_some(out)
}

/// `$XX` for a plain value over $FF.
fn low_byte(word: &str) -> Option<String> {
    let n = if let Some(hex) = word
        .strip_prefix('$')
        .or_else(|| word.strip_prefix("0x"))
        .or_else(|| word.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        word.parse::<u32>().ok()?
    };
    (n > 0xFF).then(|| format!("${:02X}", n & 0xFF))
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

    #[test]
    fn values_over_a_byte_become_their_low_byte() {
        let text = "%ext ck1\r\n\
            %patchfile 0x12087 CKSCREEN.CK1\r\n\
            # Map coords (multiplied by $1000)\r\n\
            %patch $81D6 $1000 # This is x coord: $00003000\r\n\
            %patch $81E8 $0002\r\n\
            %patch $8919  $89 $F0\r\n   $1F8 4096 $D1\r\n\
            %patch $9000 $1000W $12345678L \"$1000\"\r\n\
            %patch $10000 $01\r\n\
            %end\r\n";
        assert_eq!(
            as_older_versions(text).unwrap(),
            "%ext ck1\r\n\
            %patchfile 0x12087 CKSCREEN.CK1\r\n\
            # Map coords (multiplied by $1000)\r\n\
            %patch $81D6 $00 # This is x coord: $00003000\r\n\
            %patch $81E8 $0002\r\n\
            %patch $8919  $89 $F0\r\n   $F8 $00 $D1\r\n\
            %patch $9000 $1000W $12345678L \"$1000\"\r\n\
            %patch $10000 $01\r\n\
            %end\r\n"
        );
        assert_eq!(as_older_versions("%patch $81D6 $10 $0002\n"), None);
    }
}
