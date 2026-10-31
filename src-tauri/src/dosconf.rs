//! The DOSBox config layers (PLAN.md §6.1): shipped defaults per engine, the
//! options from the settings page, the user's global and per-profile
//! overrides, and the generated launch layer.
//! Reads and writes the layer files and shows what they add up to.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::{self, Engine};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::keymap;
use crate::paths::AppPaths;
use crate::profiles::{self, Layers};
use crate::recipe;
use crate::settings;

pub const GLOBAL_TEMPLATE: &str = "\
# Your DOSBox Staging settings for all games. They override BloogBase's
# defaults; per-game overrides override these.
# [autoexec] sections are ignored: BloogBase starts the games itself.
";

pub const PROFILE_TEMPLATE: &str = "\
# Your DOSBox Staging settings for this game only. They override BloogBase's
# defaults and your global settings.
# [autoexec] sections are ignored: BloogBase starts the game itself.
";

/// The shipped defaults for an engine: its file name stem and content.
pub fn defaults(engine: Engine) -> (&'static str, &'static str) {
    match engine {
        Engine::Vorticons => ("vorticons", include_str!("../configs/vorticons.conf")),
        Engine::Galaxy => ("galaxy", include_str!("../configs/galaxy.conf")),
        Engine::Dreams => ("dreams", include_str!("../configs/dreams.conf")),
    }
}

/// A machine a mod needs instead of the default S3 SVGA, as its patch shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Machine {
    /// It draws for a CGA composite monitor. Staging's composite emulation
    /// (`auto`, early-era for CGA) switches itself on in its graphics mode.
    CompositeCga,
    /// It uses video memory only a Paradise SVGA gives it.
    ParadiseSvga,
}

impl Machine {
    /// What it adds to the defaults.
    fn conf(self) -> &'static str {
        match self {
            Machine::CompositeCga => {
                "# Added for this mod: it draws its graphics for a CGA composite monitor.
[dosbox]
machine = cga
"
            }
            Machine::ParadiseSvga => {
                "# Added for this mod: it uses video memory only a Paradise SVGA gives it.
[dosbox]
machine = svga_paradise
"
            }
        }
    }
}

/// The machine a profile's mod needs, if not the default one.
pub fn mod_machine(layers: &Layers) -> Option<Machine> {
    let m = layers.mod_layer.as_ref()?;
    let patch = m.recipe.patch_file()?;
    // Found as DOS would, whatever the case.
    let path = fs::read_dir(&m.files)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(patch))?
        .path();
    let text = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&text);
    if recipe::sets_composite_mode(&text) {
        Some(Machine::CompositeCga)
    } else {
        recipe::maps_128k_video_memory(&text).then_some(Machine::ParadiseSvga)
    }
}

/// What a profile's mod needs on top of its engine's defaults.
pub fn mod_defaults(layers: &Layers) -> Option<&'static str> {
    mod_machine(layers).map(Machine::conf)
}

/// A profile's defaults layer: its engine's, plus what its mod needs.
fn profile_defaults(engine: Engine, extra: Option<&str>) -> String {
    let content = defaults(engine).1;
    match extra {
        Some(extra) => format!(
            "{content}
{extra}"
        ),
        None => content.to_string(),
    }
}

/// The simple options on the settings page, for all games. Written as their
/// own layer right after the defaults, so the user's own DOSBox settings
/// still win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayOptions {
    pub fullscreen: bool,
    /// Staging's adaptive CRT shader; off is its sharp shader.
    pub crt: bool,
    /// Against tearing while scrolling, at the cost of a little input lag.
    pub vsync: bool,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        DisplayOptions {
            fullscreen: false,
            crt: true,
            vsync: false,
        }
    }
}

const FULLSCREEN_KEY: &str = "dosbox.fullscreen";
const CRT_KEY: &str = "dosbox.crt";
const VSYNC_KEY: &str = "dosbox.vsync";

pub fn options(db: &Db) -> Result<DisplayOptions> {
    let d = DisplayOptions::default();
    let flag = |key, default: bool| -> Result<bool> {
        Ok(settings::get(db, key)?.map_or(default, |v| v == "1"))
    };
    Ok(DisplayOptions {
        fullscreen: flag(FULLSCREEN_KEY, d.fullscreen)?,
        crt: flag(CRT_KEY, d.crt)?,
        vsync: flag(VSYNC_KEY, d.vsync)?,
    })
}

pub fn set_options(db: &Db, o: &DisplayOptions) -> Result<()> {
    for (key, on) in [
        (FULLSCREEN_KEY, o.fullscreen),
        (CRT_KEY, o.crt),
        (VSYNC_KEY, o.vsync),
    ] {
        settings::set(db, key, if on { "1" } else { "0" })?;
    }
    Ok(())
}

/// Each option's name and the DOSBox setting behind it.
const OPTION_SETTINGS: [(&str, &str, &str); 3] = [
    ("fullscreen", "sdl", "fullscreen"),
    ("crt", "render", "shader"),
    ("vsync", "sdl", "vsync"),
];

/// `with_keymap`: whether the keyboard layout's mapper file is next to this layer
/// (see [`keymap`]).
pub fn options_conf(o: &DisplayOptions, with_keymap: bool) -> String {
    let on_off = |on: bool| if on { "on" } else { "off" };
    let mapper = if with_keymap {
        format!("mapperfile = {}\n", keymap::MAPPERFILE)
    } else {
        String::new()
    };
    format!(
        "# Generated by BloogBase from its settings page. Do not edit; your own\n\
         # DOSBox settings for all games or one game override these.\n\
         [sdl]\n\
         fullscreen = {}\n\
         vsync = {}\n\
         {mapper}\
         \n\
         [render]\n\
         shader = {}\n",
        on_off(o.fullscreen),
        on_off(o.vsync),
        if o.crt { "crt-auto" } else { "sharp" },
    )
}

/// Whether the last launch wrote a mapper file for the keyboard layout.
fn has_keymap(paths: &AppPaths) -> bool {
    paths
        .options_conf()
        .with_file_name(keymap::MAPPERFILE)
        .exists()
}

/// The options whose setting the user's global override sets as well, so
/// they have no effect.
pub fn overridden_options(paths: &AppPaths) -> Result<Vec<String>> {
    let global = read_or(&paths.global_override_conf(), GLOBAL_TEMPLATE)?;
    let (set, _) = parse(&global, ConfigLayer::Global);
    Ok(OPTION_SETTINGS
        .iter()
        .filter(|(_, section, key)| set.iter().any(|s| s.section == *section && s.key == *key))
        .map(|(name, ..)| name.to_string())
        .collect())
}

/// The capture folder as DOSBox needs it in the launch layer: DOSBox resolves
/// a relative `capture_dir` against the folder of the config file that sets
/// it (`profiles/<id>/generated/`), not against its working directory.
/// Relative, so non-ASCII user names can't break it either.
fn capture_dir() -> String {
    Path::new("..").join("capture").display().to_string()
}

/// What the DOSBox window is named after: the mod's name for a mod, else
/// the game's title.
pub fn window_title<'a>(profile: &'a profiles::Profile, game: &'a catalog::Game) -> &'a str {
    match profile.mod_id {
        Some(_) => &profile.label,
        None => &game.title,
    }
}

/// Settings that belong to the launch itself; loaded last so users can't
/// break them by accident. `game` names the window (see [`window_title`]).
pub fn launch_conf(game: &str) -> String {
    let capture = capture_dir();
    // DOSBox rejects the whole setting over a quote or control character.
    let game: String = game
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '\''))
        .collect();
    format!(
        "# Generated by BloogBase on every launch. Do not edit.\n\
         [dosbox]\n\
         automount = off\n\
         startup_verbosity = quiet\n\
         \n\
         [sdl]\n\
         window_titlebar = program=\"{game} – BloogBase\" cycles=off mouse=off\n\
         \n\
         [capture]\n\
         capture_dir = {capture}\n"
    )
}

/// The config files of a launch, in the order DOSBox loads them.
pub struct LayerFiles {
    pub defaults: PathBuf,
    pub options: PathBuf,
    pub global: PathBuf,
    pub profile: PathBuf,
    pub launch: PathBuf,
}

/// Writes all layers of a profile: defaults and options refreshed, user
/// overrides only created when missing, the launch layer regenerated.
/// A mod that needs more than its engine's defaults gets a defaults layer of
/// its own.
pub fn ensure(
    paths: &AppPaths,
    engine: Engine,
    mod_defaults: Option<&str>,
    options: &DisplayOptions,
    profile_id: &str,
    title: &str,
) -> Result<LayerFiles> {
    let (stem, content) = defaults(engine);
    let dirs = paths.profile(profile_id);
    let files = LayerFiles {
        defaults: match mod_defaults {
            Some(_) => dirs.defaults_conf.clone(),
            None => paths.default_conf(stem),
        },
        options: paths.options_conf(),
        global: paths.global_override_conf(),
        profile: dirs.override_conf,
        launch: dirs.launch_conf,
    };
    match mod_defaults {
        Some(_) => write_if_changed(&files.defaults, profile_defaults(engine, mod_defaults))?,
        None => write_if_changed(&files.defaults, content)?,
    }
    let with_keymap = keymap::ensure(&paths.configs)?;
    write_if_changed(&files.options, options_conf(options, with_keymap))?;
    write_if_missing(&files.global, GLOBAL_TEMPLATE)?;
    write_if_missing(&files.profile, PROFILE_TEMPLATE)?;
    write_if_changed(&files.launch, launch_conf(title))?;
    Ok(files)
}

fn write_if_missing(path: &Path, content: &str) -> Result<()> {
    if !path.exists() {
        write_if_changed(path, content)?;
    }
    Ok(())
}

pub fn write_if_changed(path: &Path, content: impl AsRef<[u8]>) -> Result<()> {
    let content = content.as_ref();
    if fs::read(path).is_ok_and(|c| c == content) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content)?;
    Ok(())
}

/// A user layer's text, or its template if the user never saved one.
pub fn read_or(path: &Path, template: &str) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(template.to_string()),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConfigLayer {
    Defaults,
    Options,
    Global,
    Profile,
    Launch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// An `[autoexec]` section: BloogBase never runs it.
    Autoexec,
    /// Neither a section header, a setting nor a comment.
    Syntax,
    /// A setting before the first section header.
    NoSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWarning {
    /// 1-based.
    pub line: u32,
    pub kind: WarningKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    pub section: String,
    pub key: String,
    pub value: String,
    pub layer: ConfigLayer,
}

/// The settings in one config file, plus what looks wrong in it. Section
/// and key names are lowercased, as DOSBox treats them case-insensitively.
pub fn parse(text: &str, layer: ConfigLayer) -> (Vec<Setting>, Vec<ConfigWarning>) {
    let mut settings = Vec::new();
    let mut warnings = Vec::new();
    let mut section: Option<String> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        let warn = |kind| ConfigWarning {
            line: i as u32 + 1,
            kind,
        };
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_ascii_lowercase();
            if name == "autoexec" {
                warnings.push(warn(WarningKind::Autoexec));
            }
            section = Some(name);
            continue;
        }
        // Everything in [autoexec] is DOS commands, warned about once above.
        if section.as_deref() == Some("autoexec") {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warnings.push(warn(WarningKind::Syntax));
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            warnings.push(warn(WarningKind::Syntax));
            continue;
        }
        let Some(section) = &section else {
            warnings.push(warn(WarningKind::NoSection));
            continue;
        };
        settings.push(Setting {
            section: section.clone(),
            key: key.to_ascii_lowercase(),
            value: value.trim().to_string(),
            layer,
        });
    }
    (settings, warnings)
}

/// What the layers add up to: the last layer setting a key wins. Sorted by
/// section, then key.
pub fn merge(layers: &[(&str, ConfigLayer)]) -> Vec<Setting> {
    let mut out: Vec<Setting> = Vec::new();
    for (text, layer) in layers {
        for s in parse(text, *layer).0 {
            match out
                .iter_mut()
                .find(|o| o.section == s.section && o.key == s.key)
            {
                Some(o) => *o = s,
                None => out.push(s),
            }
        }
    }
    out.sort_by(|a, b| (&a.section, &a.key).cmp(&(&b.section, &b.key)));
    out
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    /// The user's global override.
    pub global: String,
    pub global_warnings: Vec<ConfigWarning>,
    /// Asked for a profile: its layers and what they add up to.
    pub profile: Option<ProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileConfig {
    /// BloogBase's defaults for the profile's engine, read-only.
    pub defaults: String,
    pub text: String,
    pub warnings: Vec<ConfigWarning>,
    pub effective: Vec<Setting>,
}

pub fn view(paths: &AppPaths, db: &Db, profile_id: Option<&str>) -> Result<ConfigView> {
    let global = read_or(&paths.global_override_conf(), GLOBAL_TEMPLATE)?;
    let profile = match profile_id {
        None => None,
        Some(id) => {
            let (profile, layers) = profiles::get(db, id)?;
            let game = catalog::game(&profile.game_id)
                .ok_or_else(|| Error::NotFound(format!("game {}", profile.game_id)))?;
            let engine = game.engine;
            let defaults = &profile_defaults(engine, mod_defaults(&layers));
            let dirs = paths.profile(id);
            let text = read_or(&dirs.override_conf, PROFILE_TEMPLATE)?;
            let launch = launch_conf(window_title(&profile, game));
            let options = options_conf(&options(db)?, has_keymap(paths));
            let effective = merge(&[
                (defaults, ConfigLayer::Defaults),
                (&options, ConfigLayer::Options),
                (&global, ConfigLayer::Global),
                (&text, ConfigLayer::Profile),
                (&launch, ConfigLayer::Launch),
            ]);
            Some(ProfileConfig {
                defaults: defaults.to_string(),
                warnings: parse(&text, ConfigLayer::Profile).1,
                text,
                effective,
            })
        }
    };
    Ok(ConfigView {
        global_warnings: parse(&global, ConfigLayer::Global).1,
        global,
        profile,
    })
}

/// The file of the global override, or of a profile's.
fn user_layer(
    paths: &AppPaths,
    db: &Db,
    profile_id: Option<&str>,
) -> Result<(PathBuf, &'static str)> {
    Ok(match profile_id {
        None => (paths.global_override_conf(), GLOBAL_TEMPLATE),
        Some(id) => {
            // Also makes sure the id names a real profile.
            profiles::get(db, id)?;
            (paths.profile(id).override_conf, PROFILE_TEMPLATE)
        }
    })
}

/// Saves a user layer as written; the warnings say what DOSBox will skip.
pub fn save(
    paths: &AppPaths,
    db: &Db,
    profile_id: Option<&str>,
    text: &str,
) -> Result<Vec<ConfigWarning>> {
    let (path, _) = user_layer(paths, db, profile_id)?;
    write_if_changed(&path, text)?;
    Ok(parse(text, ConfigLayer::Profile).1)
}

/// Puts a user layer back to its empty template.
pub fn reset(paths: &AppPaths, db: &Db, profile_id: Option<&str>) -> Result<()> {
    let (path, template) = user_layer(paths, db, profile_id)?;
    write_if_changed(&path, template)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_and_flags_problems() {
        let text = "# comment\nstray = 1\n[CPU]\n  CPU_Cycles = 3000 \n; other\nnonsense\n\
                    [autoexec]\nmount c .\n[sdl]\nfull screen = on\n";
        let (settings, warnings) = parse(text, ConfigLayer::Global);
        assert_eq!(
            settings,
            vec![Setting {
                section: "cpu".into(),
                key: "cpu_cycles".into(),
                value: "3000".into(),
                layer: ConfigLayer::Global,
            }]
        );
        let kinds: Vec<(u32, WarningKind)> = warnings.iter().map(|w| (w.line, w.kind)).collect();
        assert_eq!(
            kinds,
            [
                (2, WarningKind::NoSection),
                (6, WarningKind::Syntax),
                (7, WarningKind::Autoexec),
                (10, WarningKind::Syntax),
            ]
        );
    }

    #[test]
    fn templates_and_defaults_are_clean() {
        for engine in [Engine::Vorticons, Engine::Galaxy, Engine::Dreams] {
            assert!(
                parse(defaults(engine).1, ConfigLayer::Defaults)
                    .1
                    .is_empty()
            );
        }
        for t in [GLOBAL_TEMPLATE, PROFILE_TEMPLATE] {
            assert_eq!(parse(t, ConfigLayer::Global), (vec![], vec![]));
        }
        let launch = launch_conf("Keen \"4\"\n");
        assert!(parse(&launch, ConfigLayer::Launch).1.is_empty());
        assert!(launch.contains("program=\"Keen 4 – BloogBase\" "));
        assert!(
            parse(
                &options_conf(&DisplayOptions::default(), true),
                ConfigLayer::Options
            )
            .1
            .is_empty()
        );
    }

    #[test]
    fn later_layers_win() {
        let merged = merge(&[
            (
                "[cpu]\ncpu_cycles = 1\n[sdl]\nfullscreen = off\n",
                ConfigLayer::Defaults,
            ),
            ("[CPU]\ncpu_cycles = 2\n", ConfigLayer::Global),
            ("[sdl]\nfullscreen = on\n", ConfigLayer::Profile),
        ]);
        let got: Vec<(&str, &str, ConfigLayer)> = merged
            .iter()
            .map(|s| (s.key.as_str(), s.value.as_str(), s.layer))
            .collect();
        assert_eq!(
            got,
            [
                ("cpu_cycles", "2", ConfigLayer::Global),
                ("fullscreen", "on", ConfigLayer::Profile),
            ]
        );
    }

    #[test]
    fn view_save_and_reset_layers() {
        let (_tmp, paths, db) = crate::saves::tests::setup();
        let v = view(&paths, &db, Some("keen4-1")).unwrap();
        assert_eq!(v.global, GLOBAL_TEMPLATE);
        let p = v.profile.unwrap();
        assert_eq!(p.text, PROFILE_TEMPLATE);
        assert!(p.defaults.contains("[cpu]"));
        let capture = p.effective.iter().find(|s| s.key == "capture_dir").unwrap();
        assert_eq!(capture.layer, ConfigLayer::Launch);
        let dirs = paths.profile("keen4-1");
        fs::create_dir_all(&dirs.capture).unwrap();
        // Linux resolves `generated/..` only if `generated` exists.
        fs::create_dir_all(dirs.launch_conf.parent().unwrap()).unwrap();
        let resolved = dirs.launch_conf.parent().unwrap().join(&capture.value);
        assert_eq!(
            fs::canonicalize(resolved).unwrap(),
            fs::canonicalize(&dirs.capture).unwrap()
        );

        save(&paths, &db, None, "[cpu]\ncpu_cycles = 5\n").unwrap();
        let warnings = save(
            &paths,
            &db,
            Some("keen4-1"),
            "[cpu]\ncpu_cycles = 7\n[autoexec]\n",
        )
        .unwrap();
        assert_eq!(warnings[0].kind, WarningKind::Autoexec);
        let v = view(&paths, &db, Some("keen4-1")).unwrap();
        let cycles = v
            .profile
            .unwrap()
            .effective
            .into_iter()
            .find(|s| s.key == "cpu_cycles")
            .unwrap();
        assert_eq!(
            (cycles.value.as_str(), cycles.layer),
            ("7", ConfigLayer::Profile)
        );

        reset(&paths, &db, Some("keen4-1")).unwrap();
        let v = view(&paths, &db, Some("keen4-1")).unwrap();
        assert_eq!(v.profile.unwrap().text, PROFILE_TEMPLATE);
        assert_eq!(v.global, "[cpu]\ncpu_cycles = 5\n");
        assert!(matches!(
            save(&paths, &db, Some("../x"), ""),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn options_are_stored_written_and_can_be_overridden() {
        let (_tmp, paths, db) = crate::saves::tests::setup();
        assert_eq!(options(&db).unwrap(), DisplayOptions::default());
        let mine = DisplayOptions {
            fullscreen: true,
            crt: false,
            vsync: true,
        };
        set_options(&db, &mine).unwrap();
        assert_eq!(options(&db).unwrap(), mine);

        let (set, _) = parse(&options_conf(&mine, false), ConfigLayer::Options);
        let value = |key: &str| set.iter().find(|s| s.key == key).map(|s| s.value.as_str());
        assert_eq!(
            (value("fullscreen"), value("vsync"), value("shader")),
            (Some("on"), Some("on"), Some("sharp"))
        );

        // Their own layer in the result; the user's global override wins.
        assert!(overridden_options(&paths).unwrap().is_empty());
        save(&paths, &db, None, "[SDL]\nFullscreen = off\n").unwrap();
        assert_eq!(overridden_options(&paths).unwrap(), ["fullscreen"]);
        let effective = view(&paths, &db, Some("keen4-1"))
            .unwrap()
            .profile
            .unwrap()
            .effective;
        let layer = |key: &str| effective.iter().find(|s| s.key == key).unwrap().layer;
        assert_eq!(layer("fullscreen"), ConfigLayer::Global);
        assert_eq!(layer("shader"), ConfigLayer::Options);
    }

    #[test]
    fn missing_user_layer_reads_as_template() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.conf");
        assert_eq!(read_or(&path, "T").unwrap(), "T");
        fs::write(&path, "mine").unwrap();
        assert_eq!(read_or(&path, "T").unwrap(), "mine");
    }
}
