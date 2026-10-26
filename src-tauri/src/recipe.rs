//! Launch recipes: how a mod is started, and the heuristics that guess it from
//! the files a mod ships (PLAN.md §9.1, §9.2 step 4).
//!
//! Most mods ship a batch file that runs the episode's patcher (`CK4PATCH`,
//! `KDRPATCH`, ...) with a `.pat` file, so batch files are the strongest hint.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Recipe {
    /// Run the episode's patcher with a patch file.
    Patch {
        patcher: String,
        patch: String,
        args: Vec<String>,
    },
    /// Run a program the mod ships.
    Program { program: String, args: Vec<String> },
    /// Run a batch file the mod ships.
    Batch { file: String },
    /// Run the original episode executable on top of the mod's files.
    Episode,
}

impl Recipe {
    /// The DOS command line, or `None` for [`Recipe::Episode`], whose
    /// executable depends on the install.
    pub fn command(&self) -> Option<String> {
        let join = |first: &str, args: &[String]| {
            std::iter::once(first)
                .chain(args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Recipe::Patch {
                patcher,
                patch,
                args,
            } => Some(join(&format!("{patcher} {patch}"), args)),
            Recipe::Program { program, args } => Some(join(program, args)),
            Recipe::Batch { file } => Some(file.clone()),
            Recipe::Episode => None,
        }
    }

    /// Files the recipe runs; they have to be in the mod's root.
    pub fn files(&self) -> Vec<&str> {
        match self {
            Recipe::Patch { patcher, patch, .. } => vec![patcher, patch],
            Recipe::Program { program, .. } => vec![program],
            Recipe::Batch { file } => vec![file],
            Recipe::Episode => vec![],
        }
    }

    /// The patch file, whose `%version` lines say which game versions fit.
    pub fn patch_file(&self) -> Option<&str> {
        match self {
            Recipe::Patch { patch, .. } => Some(patch),
            _ => None,
        }
    }
}

/// Recipes for some game versions, by catalog version id, where a mod starts
/// differently on them (Norp the Yorp 3 has a patch file per version).
pub type VersionRecipes = BTreeMap<String, Recipe>;

/// The recipe for playing on `version_id` of `game_id`: that version's own,
/// or its original release's for a third-party build, else `default`.
pub fn for_version<'a>(
    default: &'a Recipe,
    by_version: &'a VersionRecipes,
    game_id: &str,
    version_id: Option<&str>,
) -> &'a Recipe {
    let Some(id) = version_id else {
        return default;
    };
    let original = crate::catalog::game(game_id)
        .and_then(|g| g.versions.iter().find(|v| v.id == id))
        .and_then(|v| v.original.as_deref());
    by_version
        .get(id)
        .or_else(|| original.and_then(|o| by_version.get(o)))
        .unwrap_or(default)
}

/// A recipe the heuristics consider, with the file that suggested it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub recipe: Recipe,
    /// e.g. `PLAY.BAT` or `FIRE.PAT`; empty for the episode fallback.
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess {
    /// Best first; never empty.
    pub candidates: Vec<Candidate>,
    /// False when several equally good candidates exist and the user should pick.
    pub confident: bool,
}

/// Programs that come with mods but don't start them.
const TOOLS: &[&str] = &[
    "UNLZEXE", "LZEXE", "TSRMUSIC", "LIST", "BROWSE", "PKUNZIP", "SETUP", "INSTALL",
];

/// Whether a batch file's command runs one of the `TOOLS`.
fn is_tool(command: &str) -> bool {
    let name = command.rsplit(['\\', ':']).next().unwrap_or(command);
    TOOLS.contains(&stem(name).to_ascii_uppercase().as_str())
}

/// Batch files with these names are tried first.
const PREFERRED_BATCH: &[&str] = &["PLAY", "START", "RUN", "GO"];

/// Guesses the recipe from the files in the mod root. `names` are the bare
/// file names there; `read` returns a file's text; `episode_exes` are the
/// catalog executables of the target game; `is_patcher` says which names are
/// patchers, renamed ones included.
pub fn guess(
    names: &[String],
    read: impl Fn(&str) -> Option<String>,
    episode_exes: &[String],
    is_patcher: impl Fn(&str) -> bool,
) -> Guess {
    let by_upper: HashMap<String, &str> = names
        .iter()
        .map(|n| (n.to_ascii_uppercase(), n.as_str()))
        .collect();
    let resolve = |token: &str, exts: &[&str]| -> Option<String> {
        let token = token
            .trim_start_matches(".\\")
            .trim_start_matches("C:\\")
            .to_ascii_uppercase();
        if let Some(n) = by_upper.get(&token)
            && exts
                .iter()
                .any(|e| *e == "*" || token.ends_with(&format!(".{e}")))
        {
            return Some(n.to_string());
        }
        exts.iter()
            .find_map(|e| by_upper.get(&format!("{token}.{e}")))
            .map(|n| n.to_string())
    };
    // With or without extension, as batch files spell it.
    let is_episode_exe = |name: &str| {
        episode_exes
            .iter()
            .any(|e| e.eq_ignore_ascii_case(name) || stem(e).eq_ignore_ascii_case(name))
    };

    let mut batches: Vec<&String> = names.iter().filter(|n| has_ext(n, "BAT")).collect();
    batches.sort_by_key(|n| {
        let stem = stem(n).to_ascii_uppercase();
        (
            PREFERRED_BATCH
                .iter()
                .position(|p| *p == stem)
                .unwrap_or(PREFERRED_BATCH.len()),
            stem,
        )
    });

    // Tier 1: what the batch files run, in the order above. One we can't
    // reduce to a single command runs as it is.
    let mut from_batches = Vec::new();
    let mut seen: Vec<Vec<Vec<String>>> = Vec::new();
    for bat in batches {
        let commands = read(bat).map(|t| batch_commands(&t)).unwrap_or_default();
        // Mods often ship the same batch file under two names.
        let key: Vec<Vec<String>> = commands
            .iter()
            .map(|c| c.iter().map(|w| w.to_ascii_uppercase()).collect())
            .collect();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        // Setup steps start nothing; Patchwork Mod III's "~Install.BAT" only
        // renames the real batch file into place and deletes itself. Nor do
        // tools: Ice Planet's "Go.bat" shows the documentation with LIST.
        // Nor programs the mod doesn't have: Rorie's Birthday Mod's
        // "_Setup.BAT" renames files and runs a "virus.exe" it doesn't ship.
        let missing = |word: &str| {
            !BUILTINS.contains(&word.to_ascii_uppercase().as_str())
                && !word.starts_with(':')
                && !is_episode_exe(word)
                && !is_patcher(word)
                && !names.iter().any(|n| dos_runs(word, n))
        };
        if commands.iter().all(|c| {
            FILE_COMMANDS.contains(&c[0].to_ascii_uppercase().as_str())
                || is_tool(&c[0])
                || missing(&c[0])
        }) {
            continue;
        }
        let recipe = match commands.as_slice() {
            [] => continue,
            [cmd] => single_command(cmd, &resolve, &is_patcher, &is_episode_exe),
            _ => None,
        };
        from_batches.push(Candidate {
            recipe: recipe.unwrap_or_else(|| Recipe::Batch { file: bat.clone() }),
            origin: bat.clone(),
        });
    }

    // Tier 2: patch files next to a patcher that no batch file uses, largest first.
    let used: Vec<&str> = from_batches
        .iter()
        .filter_map(|c| c.recipe.patch_file())
        .collect();
    let patcher = names.iter().find(|n| is_patcher(n));
    let mut patches = Vec::new();
    if let Some(patcher) = patcher {
        let mut pats: Vec<(&String, usize)> = names
            .iter()
            .filter(|n| !used.contains(&n.as_str()))
            .filter_map(|n| {
                let text = read(n)?;
                (has_ext(n, "PAT") || is_patch_text(n, &text)).then_some((n, text.len()))
            })
            .collect();
        pats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (pat, _) in pats {
            patches.push(Candidate {
                recipe: Recipe::Patch {
                    patcher: patcher.clone(),
                    patch: pat.clone(),
                    args: Vec::new(),
                },
                origin: pat.clone(),
            });
        }
    }

    // Tier 3: a program of the mod's own; a menu goes before tier 2.
    let programs: Vec<Candidate> = names
        .iter()
        .filter(|n| has_ext(n, "EXE") || has_ext(n, "COM"))
        .filter(|n| !is_patcher(n) && !is_episode_exe(n))
        .filter(|n| !TOOLS.contains(&stem(n).to_ascii_uppercase().as_str()))
        .map(|n| Candidate {
            recipe: Recipe::Program {
                program: n.clone(),
                args: Vec::new(),
            },
            origin: n.clone(),
        })
        .collect();

    // Next to a patcher, a program is only worth trying without batch files:
    // then it's most likely a menu that runs the patcher itself (The
    // Prisoner's Dilemma's DILEMMA.EXE picks Keen's or Rick's patch).
    let (menus, programs) = match patcher {
        None => (Vec::new(), programs),
        Some(_) if from_batches.is_empty() => (programs, Vec::new()),
        Some(_) => (Vec::new(), Vec::new()),
    };
    let from_batches = dedup(from_batches);
    let patches = dedup(patches);
    let tiers: [Vec<Candidate>; 4] = [from_batches, menus, patches, programs];
    let confident = tiers
        .iter()
        .find(|t| !t.is_empty())
        .is_none_or(|t| t.len() == 1);

    let mut candidates: Vec<Candidate> = Vec::new();
    for c in tiers.into_iter().flatten() {
        if !candidates.iter().any(|x| x.recipe == c.recipe) {
            candidates.push(c);
        }
    }
    if (patcher.is_none() || candidates.is_empty())
        && !candidates.iter().any(|c| c.recipe == Recipe::Episode)
    {
        candidates.push(Candidate {
            recipe: Recipe::Episode,
            origin: String::new(),
        });
    }
    Guess {
        candidates,
        confident,
    }
}

fn single_command(
    cmd: &[String],
    resolve: &impl Fn(&str, &[&str]) -> Option<String>,
    is_patcher: &impl Fn(&str) -> bool,
    is_episode_exe: &impl Fn(&str) -> bool,
) -> Option<Recipe> {
    let args = &cmd[1..];
    // The episode executable comes from the install, so it needn't be in the mod.
    if is_episode_exe(&cmd[0]) && args.is_empty() {
        return Some(Recipe::Episode);
    }
    let program = resolve(&cmd[0], &["EXE", "COM"])?;
    if is_patcher(&program) {
        // Usually `.PAT`, but any file will do (e.g. `EASTERBB.CK1`).
        let patch = resolve(args.first()?, &["PAT", "*"])?;
        return Some(Recipe::Patch {
            patcher: program,
            patch,
            args: args[1..].to_vec(),
        });
    }
    Some(Recipe::Program {
        program,
        args: args.to_vec(),
    })
}

fn dedup(list: Vec<Candidate>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for c in list {
        if !out.iter().any(|x| x.recipe == c.recipe) {
            out.push(c);
        }
    }
    out
}

/// Batch commands that only change what the screen shows.
const COSMETIC: &[&str] = &[
    "ECHO", "REM", "PAUSE", "CLS", "EXIT", "TYPE", "COLOR", "MODE",
];

/// Batch commands that only move files around.
const FILE_COMMANDS: &[&str] = &[
    "REN", "RENAME", "DEL", "ERASE", "COPY", "XCOPY", "MOVE", "MD", "MKDIR", "RD", "RMDIR", "CD",
    "CHDIR", "ATTRIB",
];

/// Whether DOS runs the program `name` for the command `word`: with or
/// without extension, cut to 8.3 as DOS does (Rescue of the Last runs
/// `ck4patcher.exe` for CK4PATCH.EXE), or by its short alias (ShadowKeen2
/// runs `dunepa~1.EXE` for dunepatch.EXE).
fn dos_runs(word: &str, name: &str) -> bool {
    let word = word.rsplit(['\\', ':']).next().unwrap_or(word);
    let (stem, ext) = word.rsplit_once('.').unwrap_or((word, ""));
    let (name_stem, name_ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let name_ext = name_ext.to_ascii_uppercase();
    let ext = ext.to_ascii_uppercase();
    let runnable = ["EXE", "COM", "BAT"].contains(&name_ext.as_str());
    let ext_fits = runnable && (ext.is_empty() || ext.chars().take(3).eq(name_ext.chars()));
    let short = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_whitespace() && *c != '.')
            .take(8)
            .collect::<String>()
            .to_ascii_uppercase()
    };
    let stem_fits = match stem.split_once('~') {
        Some((prefix, _)) => short(name_stem).starts_with(&prefix.to_ascii_uppercase()),
        None => short(stem) == short(name_stem),
    };
    ext_fits && stem_fits
}

/// Commands DOS has built in, or programs it ships, which a batch file may
/// run without the mod having them.
const BUILTINS: &[&str] = &[
    "CALL", "IF", "GOTO", "FOR", "SET", "SHIFT", "CHOICE", "LH", "LOADHIGH", "PATH", "PROMPT",
    "VER", "BREAK", "VERIFY", "CHCP", "CTTY", "DATE", "TIME", "MEM", "KEYB",
];

/// The commands of a batch file that start something, split into words.
/// Comments and screen output are left out.
pub fn batch_commands(text: &str) -> Vec<Vec<String>> {
    // DOS editors end files with Ctrl-Z.
    let text = text.split('').next().unwrap_or("");
    // NUL counts as a space: several mods' batch files end with one right
    // after the patch file's name, as The Oracle Janitor's
    // `ck1patch 1lm.pat\0`.
    let blank = |c: char| c.is_whitespace() || c == '\0';
    text.lines()
        .map(|l| {
            l.trim_matches(blank)
                .trim_start_matches('@')
                .trim_matches(blank)
        })
        .filter(|l| !l.is_empty() && !l.starts_with("::"))
        .filter_map(|l| {
            let words: Vec<String> = l
                .split(blank)
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect();
            let first = words[0].to_ascii_uppercase();
            let skip = COSMETIC.contains(&first.as_str())
                || first.starts_with("ECHO.")
                || first.starts_with("ECHO:");
            (!skip).then_some(words)
        })
        .collect()
}

/// The bytes each `%patch` of a patch file writes: `$9A` a byte, `$10C5W`
/// a word, `$0E8F0A31L` a long (`R` for relocated ones), `"text"` its
/// characters, other numbers decimal bytes. A value that can't be read
/// splits the run, so nothing found spans it.
pub fn patch_bytes(text: &str) -> Vec<Vec<u8>> {
    let mut runs: Vec<Vec<u8>> = Vec::new();
    let mut in_patch = false;
    for line in text.lines() {
        let mut parts = line.split('"');
        // Outside the quotes, everything after `#` is a comment.
        let mut words: Vec<(bool, &str)> = Vec::new();
        let mut quoted = false;
        for part in parts.by_ref() {
            if quoted {
                words.push((true, part));
            } else {
                let code = part.split('#').next().unwrap_or_default();
                words.extend(code.split_whitespace().map(|w| (false, w)));
                if code.len() < part.len() {
                    break;
                }
            }
            quoted = !quoted;
        }
        let mut words = words.into_iter();
        if let Some((false, first)) = words.clone().next()
            && first.starts_with('%')
        {
            words.next();
            in_patch = first.eq_ignore_ascii_case("%patch");
            if !in_patch {
                continue;
            }
            words.next(); // The address.
            runs.push(Vec::new());
        }
        if !in_patch {
            continue;
        }
        for (quoted, word) in words {
            let Some(run) = runs.last_mut() else { break };
            if quoted {
                run.extend(word.bytes());
                continue;
            }
            match patch_value(word) {
                Some(bytes) => run.extend(bytes),
                None => runs.push(Vec::new()),
            }
        }
    }
    runs.retain(|r| !r.is_empty());
    runs
}

fn patch_value(word: &str) -> Option<Vec<u8>> {
    let upper = word.to_ascii_uppercase();
    let (digits, len) = match upper.strip_suffix('W').or(upper.strip_suffix('L')) {
        Some(d) => (
            d.strip_suffix('R').unwrap_or(d),
            if upper.ends_with('W') { 2 } else { 4 },
        ),
        None => (upper.as_str(), 1),
    };
    let value = match digits.strip_prefix('$') {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    (len == 4 || value >> (8 * len) == 0).then(|| value.to_le_bytes()[..len].to_vec())
}

/// Whether a patch switches the CGA to 640x200 graphics with the color
/// burst on, which only shows colors on a composite monitor (as Keen 4
/// Composite CGA Edition does): `mov dx,3D8h`, `mov al,<mode>`,
/// `out dx,al`, in either order of the moves.
pub fn sets_composite_mode(text: &str) -> bool {
    // High resolution, graphics and the burst (black-and-white bit clear).
    let composite = |mode: u8| mode & 0b1_0110 == 0b1_0010;
    patch_bytes(text).iter().any(|run| {
        run.windows(6).any(|w| match w {
            [0xBA, 0xD8, 0x03, 0xB0, mode, 0xEE] | [0xB0, mode, 0xBA, 0xD8, 0x03, 0xEE] => {
                composite(*mode)
            }
            _ => false,
        })
    })
}

/// Whether a patch switches to VGA's 640x480 16-color mode 12h (as Keen 1
/// 640x480 Redux does): `mov ax,12h`, `int 10h`.
pub fn sets_vga_640x480(text: &str) -> bool {
    patch_bytes(text)
        .iter()
        .any(|run| run.windows(5).any(|w| w == [0xB8, 0x12, 0x00, 0xCD, 0x10]))
}

/// Whether a patch maps 128K of video memory from A0000h, which only
/// DOSBox's Paradise SVGA gives the game (Keen 1 640x480 Redux, whose README
/// asks for it): it clears the memory map bits of the graphics controller's
/// register 6, `mov dx,3CEh`, `mov al,6`, `out dx,al`, `inc dx`,
/// `in al,dx`, `and al,<mask>`.
pub fn maps_128k_video_memory(text: &str) -> bool {
    patch_bytes(text).iter().any(|run| {
        run.windows(10).any(|w| match w {
            [0xBA, 0xCE, 0x03, 0xB0, 0x06, 0xEE, 0x42, 0xEC, 0x24, mask] => mask & 0x0C == 0,
            _ => false,
        })
    })
}

/// `%version` values of a patch file, e.g. `["1.4"]`.
/// A version whose section says `%abort` is named only to refuse it, and
/// `%version all` is for every version (The Last Moon has it).
pub fn patch_versions(text: &str) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    let mut aborted: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else { continue };
        if first.eq_ignore_ascii_case("%version") {
            current = words.next().map(str::to_string);
            if let Some(v) = &current
                && !v.eq_ignore_ascii_case("ALL")
                && !named.contains(v)
            {
                named.push(v.clone());
            }
        } else if first.eq_ignore_ascii_case("%abort") {
            aborted.extend(current.clone());
        } else if first.eq_ignore_ascii_case("%end") {
            break;
        }
    }
    named.retain(|v| !aborted.contains(v));
    named
}

/// The `%ext` of a patch file, upper case, e.g. `CK4`.
pub fn patch_ext(text: &str) -> Option<String> {
    patch_directives(text, "%ext")
        .next()
        .map(|e| e.to_ascii_uppercase())
}

/// The files a patch file loads with `%patchfile`, e.g. `["lego.tli"]`.
pub fn patch_files(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            if !words.next()?.eq_ignore_ascii_case("%patchfile") {
                return None;
            }
            let name = words.nth(1)?.trim_matches('"');
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

fn patch_directives<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = String> + 'a {
    text.lines().filter_map(move |l| {
        let mut words = l.split_whitespace();
        let first = words.next()?;
        if !first.eq_ignore_ascii_case(name) {
            return None;
        }
        words.next().map(str::to_string)
    })
}

/// Documents, which may quote patches without being one.
const DOCUMENTS: &[&str] = &["TXT", "DOC", "HTM", "HTML", "NFO", "ME", "1ST", "BAT"];

/// Whether a file with another extension than `.PAT` is a patch file, as
/// Norp the Yorp 3's `.NY3` ones: text with patch directives starting lines.
pub fn is_patch_text(name: &str, text: &str) -> bool {
    if DOCUMENTS.iter().any(|e| has_ext(name, e)) || text.contains(' ') {
        return false;
    }
    let directives = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| {
            ["%ext", "%version", "%patch", "%patchfile", "%end"]
                .iter()
                .any(|d| w.eq_ignore_ascii_case(d))
        })
        .count();
    directives >= 2
}

/// The game a CKPatch build is for, from the usage line it prints, so a
/// renamed one counts too (Norp the Yorp 3 ships CK3PATCH as NORPER.EXE).
pub fn patcher_game_of(exe: &[u8]) -> Option<&'static str> {
    let image = keen_graphics::load_image(exe).ok()?;
    let at = image.windows(7).position(|w| w == b"Usage: ")? + 7;
    let name: String = image[at..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .map(|&b| b as char)
        .collect();
    image[at + name.len()..]
        .starts_with(b" {patchfile")
        .then(|| patcher_game(&format!("{name}.EXE")))?
}

/// Whether the program at `path` is a patcher: by its name, or by the
/// usage line of a renamed one.
pub fn is_patcher_at(path: &std::path::Path) -> bool {
    path.file_name()
        .is_some_and(|n| is_patcher(&n.to_string_lossy()))
        || std::fs::read(path)
            .ok()
            .and_then(|exe| patcher_game_of(&exe))
            .is_some()
}

/// `CK1PATCH.EXE` ... `CK6PATCH.EXE`, `KDRPATCH.EXE`.
pub fn is_patcher(name: &str) -> bool {
    patcher_game(name).is_some()
}

/// The game a patcher belongs to.
pub fn patcher_game(name: &str) -> Option<&'static str> {
    if !(has_ext(name, "EXE") || has_ext(name, "COM")) {
        return None;
    }
    Some(match stem(name).to_ascii_uppercase().as_str() {
        "CK1PATCH" => "keen1",
        "CK2PATCH" => "keen2",
        "CK3PATCH" => "keen3",
        "CK4PATCH" => "keen4",
        "CK5PATCH" => "keen5",
        "CK6PATCH" => "keen6",
        "KDRPATCH" => "kdreams",
        _ => return None,
    })
}

pub fn has_ext(name: &str, ext: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// The names DOS sees for `names`, the files of one directory: one that
/// fits 8.3 as it is, a longer one as the alias DOSBox gives it, as
/// `dunepatch.EXE` becomes `DUNEPA~1.EXE`. Aliases sharing a start are
/// numbered in the order of the long names.
pub fn short_names(names: &[&str]) -> Vec<String> {
    let clean = |s: &str, n: usize| -> String {
        s.chars()
            .filter(|c| !matches!(c, ' ' | '.'))
            .take(n)
            .collect::<String>()
            .to_ascii_uppercase()
    };
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by_key(|&i| names[i].to_ascii_uppercase());
    let mut out = vec![String::new(); names.len()];
    let mut taken: HashMap<(String, String), usize> = HashMap::new();
    for i in order {
        let name = names[i];
        let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
        let fits = (1..=8).contains(&stem.len())
            && ext.len() <= 3
            && !stem.contains('.')
            && name.chars().all(|c| c.is_ascii_graphic());
        if fits {
            out[i] = name.to_ascii_uppercase();
            continue;
        }
        let ext = clean(ext, 3);
        let n = taken.entry((clean(stem, 6), ext.clone())).or_insert(0);
        *n += 1;
        let tail = format!("~{n}");
        let short = format!("{}{tail}", clean(stem, 8 - tail.len()));
        out[i] = if ext.is_empty() {
            short
        } else {
            format!("{short}.{ext}")
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Guess {
        let names: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
        let read = |n: &str| {
            files
                .iter()
                .find(|(f, _)| *f == n)
                .map(|(_, t)| t.to_string())
        };
        guess(
            &names,
            read,
            &["KEEN4E.EXE".into(), "KEEN4C.EXE".into()],
            is_patcher,
        )
    }

    fn patch(patcher: &str, patch: &str, args: &[&str]) -> Recipe {
        Recipe::Patch {
            patcher: patcher.into(),
            patch: patch.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn batch_with_patcher_wins_and_identical_batches_merge() {
        let g = run(&[
            (
                "AURORA.bat",
                "@echo off\r\nREM Abiathar\r\nCK4PATCH.EXE abiathar.pat\r\n",
            ),
            ("play.bat", "@echo off\nck4patch abiathar.pat\npause\n"),
            ("CK4PATCH.EXE", ""),
            ("abiathar.pat", "%ext ck4\n%version 1.4\n"),
            ("KEEN4E.EXE", ""),
        ]);
        assert!(g.confident);
        assert_eq!(g.candidates.len(), 1);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "abiathar.pat", &[])
        );
        assert_eq!(g.candidates[0].origin, "play.bat");
    }

    #[test]
    fn batches_that_only_run_tools_start_nothing() {
        // As Ice Planet: shareware Keen 1's extras without its executable.
        let g = run(&[
            ("Go.bat", "cls\r\nlist readall.txt"),
            (
                "Read.bat",
                "ECHO OFF\r\nCLS\r\nECHO COMMANDER KEEN\r\nPAUSE\r\n",
            ),
            (
                "Cop.bat",
                "MKDIR C:\\KEEN\r\nCOPY *.* C:\\KEEN\r\nPAUSE\r\n",
            ),
            ("Print.bat", "copy readall.txt prn:\r\n"),
            ("List.exe", ""),
            ("Browse.com", ""),
            ("Readall.txt", ""),
        ]);
        assert!(g.confident);
        let recipes: Vec<_> = g.candidates.iter().map(|c| c.recipe.clone()).collect();
        assert_eq!(recipes, [Recipe::Episode]);
    }

    #[test]
    fn batches_that_only_run_missing_programs_start_nothing() {
        // As Rorie's Birthday Mod: the real batch files hide as `1.CK1` and
        // `2.CK1`, so it's the two patches, and the player picks.
        let g = run(&[
            (
                "_Setup.BAT",
                "echo off\r\npause\r\nrename 1.CK1 _Bush.BAT\r\nrename 2.CK1 _Kerry.BAT\r\n\
                 virus.exe\r\ndel virus.exe\r\ndel _Setup.BAT\r\n",
            ),
            ("CK4PATCH.EXE", ""),
            ("bush.PAT", "%ext ck4\n%patch $1 $2\n"),
            ("Kerry.PAT", "%ext ck4\n%patch $1 $2\n%patch $3 $4\n"),
        ]);
        assert!(!g.confident);
        let recipes: Vec<_> = g.candidates.iter().map(|c| c.recipe.clone()).collect();
        assert_eq!(
            recipes,
            [
                patch("CK4PATCH.EXE", "Kerry.PAT", &[]),
                patch("CK4PATCH.EXE", "bush.PAT", &[])
            ]
        );

        // DOS's own commands, the episode, other batch files and the
        // patcher count.
        for start in ["choice /c:yn", "call PLAY.BAT", "KEEN4E", "ck4patch x.pat"] {
            let g = run(&[
                ("RUN.BAT", &format!("ren a b\r\n{start}\r\n")),
                ("PLAY.BAT", "KEEN4E"),
                ("CK4PATCH.EXE", ""),
            ]);
            assert!(
                g.candidates.iter().any(|c| c.origin == "RUN.BAT"),
                "{start}"
            );
        }
    }

    #[test]
    fn nul_in_batch_files_is_a_space() {
        // As The Oracle Janitor's oj1.bat.
        assert_eq!(
            batch_commands("ck1patch 1lm.pat\0"),
            [vec!["ck1patch".to_string(), "1lm.pat".to_string()]]
        );
        let g = run(&[
            ("oj4.bat", "ck4patch 1lm.pat\0"),
            ("CK4PATCH.EXE", ""),
            ("1lm.pat", "%ext ck4\n%patch $1 $2\n"),
        ]);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "1lm.pat", &[])
        );
    }

    #[test]
    fn dos_runs_short_names() {
        assert!(dos_runs("ck4patcher.exe", "CK4PATCH.EXE"));
        assert!(dos_runs("dunepa~1.EXE", "dunepatch.EXE"));
        assert!(dos_runs("C:\\KEEN\\keen4e", "KEEN4E.EXE"));
        assert!(dos_runs("play", "PLAY.BAT"));
        assert!(!dos_runs("virus.exe", "VIRUS.CK1"));
        assert!(!dos_runs("1.CK1", "1.CK1"));
        assert!(!dos_runs("ck4patch", "CK4PATCH.PAT"));
    }

    #[test]
    fn disagreeing_batches_ask_the_user() {
        let g = run(&[
            ("BTOWER.BAT", "tsrmusic.com\nck3patch btower.pat -nowait"),
            ("BTRNOMUS.bat", "ck3patch btrnomus.pat -nowait"),
            ("CK3PATCH.exe", ""),
            ("TSRMUSIC.com", ""),
            ("btower.pat", "%ext ck3"),
            ("btrnomus.pat", "%ext ck3"),
        ]);
        assert!(!g.confident);
        let recipes: Vec<_> = g.candidates.iter().map(|c| c.recipe.clone()).collect();
        assert_eq!(
            recipes,
            [
                Recipe::Batch {
                    file: "BTOWER.BAT".into()
                },
                patch("CK3PATCH.exe", "btrnomus.pat", &["-nowait"]),
                patch("CK3PATCH.exe", "btower.pat", &[]),
            ]
        );
        assert_eq!(
            recipes[1].command().unwrap(),
            "CK3PATCH.exe btrnomus.pat -nowait"
        );
    }

    #[test]
    fn patch_without_batch_file() {
        let g = run(&[
            ("CK4PATCH.EXE", ""),
            ("MOD.PAT", "%ext ck4\n%patch $1 $2\n%patch $3 $4"),
            ("tiles.pat", "%ext ck4"),
        ]);
        assert!(!g.confident);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "MOD.PAT", &[])
        );
        assert_eq!(g.candidates.len(), 2);
    }

    #[test]
    fn menu_program_next_to_a_patcher_goes_first() {
        // The Prisoner's Dilemma: DILEMMA.EXE runs CK6PATCH with patch1 or patch2.
        let g = run(&[
            ("CK6PATCH.EXE", ""),
            ("DILEMMA.EXE", ""),
            (
                "patch1.ck6",
                "%ext ck6
%patch $1 $2",
            ),
            (
                "patch2.ck6",
                "%ext ck6
%patch $1 $2
%patch $3 $4",
            ),
            ("readme.txt", "Run DILEMMA.EXE"),
        ]);
        assert!(g.confident);
        let recipes: Vec<_> = g.candidates.iter().map(|c| c.recipe.clone()).collect();
        assert_eq!(
            recipes,
            [
                Recipe::Program {
                    program: "DILEMMA.EXE".into(),
                    args: vec![]
                },
                patch("CK6PATCH.EXE", "patch2.ck6", &[]),
                patch("CK6PATCH.EXE", "patch1.ck6", &[]),
            ]
        );

        // With a batch file, other programs are helpers (Simulator Keen's RANDOM.EXE).
        let g = run(&[
            (
                "play.bat",
                "random
ck4patch p.pat",
            ),
            ("RANDOM.EXE", ""),
            ("CK4PATCH.EXE", ""),
            ("p.pat", "%ext ck4"),
        ]);
        assert!(g.candidates.iter().all(|c| !matches!(
            &c.recipe,
            Recipe::Program { program, .. } if program == "RANDOM.EXE"
        )));
    }

    #[test]
    fn file_replacement_runs_the_episode() {
        let g = run(&[("LEVEL01.CK4", ""), ("README.TXT", "")]);
        assert!(g.confident);
        assert_eq!(g.candidates[0].recipe, Recipe::Episode);
        assert_eq!(g.candidates[0].recipe.command(), None);

        let g = run(&[("GO.BAT", "keen4e"), ("GAMEMAPS.CK4", "")]);
        assert!(g.confident);
        assert_eq!(
            g.candidates,
            [Candidate {
                recipe: Recipe::Episode,
                origin: "GO.BAT".into()
            }]
        );
    }

    #[test]
    fn standalone_program() {
        let g = run(&[("KEEN7.EXE", ""), ("UNLZEXE.EXE", ""), ("DATA.CK4", "")]);
        assert!(g.confident);
        assert_eq!(
            g.candidates[0].recipe,
            Recipe::Program {
                program: "KEEN7.EXE".into(),
                args: vec![]
            }
        );
        assert_eq!(g.candidates[1].recipe, Recipe::Episode);
    }

    #[test]
    fn real_world_batch_quirks() {
        // Ctrl-Z at the end, screen output, a patch file without .PAT.
        let g = run(&[
            (
                "EASTER.BAT",
                "echo off
cls
type INTRO.CK4
ck4patch easter.ck4
",
            ),
            ("CK4PATCH.EXE", ""),
            ("EASTER.CK4", "%ext ck4"),
        ]);
        assert!(g.confident);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "EASTER.CK4", &[])
        );

        // The same batch file under two names counts once.
        let g = run(&[
            (
                "play.bat",
                "random
ck4patch p.pat",
            ),
            (
                "start.Bat",
                "RANDOM
CK4PATCH P.PAT
",
            ),
            ("RANDOM.EXE", ""),
            ("CK4PATCH.EXE", ""),
            ("p.pat", "%ext ck4"),
        ]);
        assert!(g.confident);
        assert_eq!(
            g.candidates[0].recipe,
            Recipe::Batch {
                file: "play.bat".into()
            }
        );
    }

    #[test]
    fn batch_parsing() {
        assert_eq!(
            batch_commands(
                "@ECHO OFF\r\n:: x\r\nrem y\r\necho. \r\n  cls\r\nKDRPATCH MartGold.pat\r\nPause\r\n"
            ),
            [vec!["KDRPATCH".to_string(), "MartGold.pat".into()]]
        );
    }

    #[test]
    fn setup_batch_files_are_passed_over() {
        // As Patchwork Mod III: the batch file it starts with only renames
        // the real one, which has another extension until then.
        let g = run(&[
            (
                "~Install.BAT",
                "echo:off\r\ncls\r\n@echo Welcome\r\npause\r\nrename ~Install.DAT ~PMIII.BAT\r\ndel ~Install.BAT",
            ),
            ("~Install.DAT", "echo off\r\nCK4PATCH patches.PAT\r\n"),
            ("CK4PATCH.EXE", ""),
            ("patches.PAT", "%ext ck4"),
        ]);
        assert!(g.confident);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "patches.PAT", &[])
        );
    }

    #[test]
    fn patch_file_directives() {
        let text = "#Tile info\n%ext ck2\n%version 1.0\n%patchfile 0x1 a.tli\n%VERSION 1.31\n%version 1.0\n";
        assert_eq!(patch_versions(text), ["1.0", "1.31"]);
        assert_eq!(patch_ext(text).as_deref(), Some("CK2"));
        assert_eq!(patch_ext("# nothing"), None);
        assert_eq!(patch_files(text), ["a.tli"]);
        assert_eq!(
            patch_files("%PATCHFILE $1EE40 \"STORY.CK4\"\n"),
            ["STORY.CK4"]
        );

        let refusing = "%ext ck6
%version 1.0
%abort
%version 1.4
%patch $1 $2
%end
%version 1.5
";
        assert_eq!(patch_versions(refusing), ["1.4"]);
        assert!(
            patch_versions(
                "%ext ck1
%version all
%patch $1 $2
"
            )
            .is_empty()
        );
        assert_eq!(
            patch_versions(
                "%version ALL
%patch $1 $2
%version 1.31
%patch $3 $4
"
            ),
            ["1.31"]
        );
    }

    #[test]
    fn patch_bytes_of_all_value_kinds() {
        let text = "%ext ck4
%patch $1F340 \"Hi! #1\" $0A 13 # comment $FF
  $10C5RW $0B89W
%patchfile $20DF0 AUDIOHED.CK4
$01
%patch $F24C $9A $0E8F0A31RL junk $02
";
        assert_eq!(
            patch_bytes(text),
            [
                vec![
                    b'H', b'i', b'!', b' ', b'#', b'1', 0x0A, 13, 0xC5, 0x10, 0x89, 0x0B
                ],
                vec![0x9A, 0x31, 0x0A, 0x8F, 0x0E],
                vec![0x02],
            ]
        );
    }

    #[test]
    fn composite_mode_from_the_cga_mode_register() {
        // Keen 4 Composite CGA Edition: mode 6, then 1Ah to port 3D8h.
        let composite = "%patch $11FDD $B8 $06 $00 $CD $10 $BA $D8 $03 $B0 $1A $EE $CB";
        assert!(sets_composite_mode(composite));
        assert!(sets_composite_mode(
            "%patch $1 $B0 $1A
$BA $D8 $03 $EE"
        ));
        // Black and white (1Eh), or 320x200 (0Ah): no composite colors.
        assert!(!sets_composite_mode("%patch $1 $BA $D8 $03 $B0 $1E $EE"));
        assert!(!sets_composite_mode("%patch $1 $BA $D8 $03 $B0 $0A $EE"));
        // Not across patches.
        assert!(!sets_composite_mode(
            "%patch $1 $BA $D8 $03
%patch $9 $B0 $1A $EE"
        ));
    }

    #[test]
    fn vga_modes_of_keen1_640x480_redux() {
        let text = "%patch $6E51 $BA $CE $03 $B0 $06 $EE $42 $EC $24 $F3 $EE $EB $15
%patch $6CB0 $E8 $C4 $FA # palette fading
             $B8 $12 $00 $CD $10 #set mode 12h";
        assert!(sets_vga_640x480(text));
        assert!(maps_128k_video_memory(text));
        // Mode 0Dh; the memory map left at 64K (bits 2-3 kept).
        assert!(!sets_vga_640x480("%patch $1 $B8 $0D $00 $CD $10"));
        assert!(!maps_128k_video_memory(
            "%patch $1 $BA $CE $03 $B0 $06 $EE $42 $EC $24 $FF $EE"
        ));
    }

    #[test]
    fn patchers() {
        assert_eq!(patcher_game("ck1patch.exe"), Some("keen1"));
        assert_eq!(patcher_game("KDRPATCH.EXE"), Some("kdreams"));
        assert_eq!(patcher_game("CK4PATCH.PAT"), None);
        assert_eq!(patcher_game("CKPATCH.EXE"), None);
    }

    #[test]
    fn renamed_patchers_by_usage_line() {
        let exe = |text: &[u8]| [b"MZ".as_slice(), &[0; 0x1E], text].concat();
        let usage = b"Usage: CK3PATCH {patchfile|patchdir} [game arguments]";
        assert_eq!(patcher_game_of(&exe(usage)), Some("keen3"));
        assert_eq!(
            patcher_game_of(&exe(b"Usage: KDRPATCH {patchfile}")),
            Some("kdreams")
        );
        assert_eq!(patcher_game_of(&exe(b"Usage: FOO {patchfile}")), None);
        assert_eq!(patcher_game_of(&exe(b"Usage: CK3PATCH file")), None);
        assert_eq!(patcher_game_of(usage), None);
    }

    #[test]
    fn patch_files_by_content() {
        let text = "#Text Files
%patchfile 0x17b00 DRAGON.NY3
%patch 0x1ba4c \"N3MAP\"
";
        assert!(is_patch_text("MAGMA.NY3", text));
        assert!(!is_patch_text("README.TXT", text));
        assert!(!is_patch_text(
            "LEVEL01.NY3",
            "%patch 1
 %end"
        ));
        assert!(!is_patch_text(
            "NOTES.NY3",
            "%patch 0x1 2
"
        ));
        // Next to a patcher, without a batch file.
        let g = run(&[
            ("CK4PATCH.EXE", ""),
            (
                "MOD.NY4",
                "%ext ck4
%patch 0x1 2
%end
",
            ),
        ]);
        assert_eq!(
            g.candidates[0].recipe,
            patch("CK4PATCH.EXE", "MOD.NY4", &[])
        );
    }

    #[test]
    fn recipe_serializes_with_type_tag() {
        let json = serde_json::to_string(&patch("A.EXE", "B.PAT", &[])).unwrap();
        assert_eq!(
            json,
            r#"{"type":"patch","patcher":"A.EXE","patch":"B.PAT","args":[]}"#
        );
        let back: Recipe = serde_json::from_str(r#"{"type":"episode"}"#).unwrap();
        assert_eq!(back, Recipe::Episode);
    }

    #[test]
    fn long_names_get_dos_aliases() {
        let names = [
            "dunepatch.EXE",
            "keen2.exe",
            "Dunepatch2.exe",
            "~Halloween.BAT",
            "my game.ck2",
            "README.html",
            "x.tar.gz",
        ];
        assert_eq!(
            short_names(&names),
            [
                "DUNEPA~1.EXE",
                "KEEN2.EXE",
                "DUNEPA~2.EXE",
                "~HALLO~1.BAT",
                "MYGAME~1.CK2",
                "README~1.HTM",
                "XTAR~1.GZ",
            ]
        );
    }
}
