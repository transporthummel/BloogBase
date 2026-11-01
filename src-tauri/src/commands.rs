//! Tauri commands exposed to the frontend. Keep these thin: validate input,
//! call into the core modules, map errors via `CommandError`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::backups::{self, Backup, BackupSettings, Contents, Origin, SaveVersion};
use crate::captures::{self, Kind, Screenshot, Video, VideoIndex};
use crate::catalog::{self, Game};
use crate::covers;
use crate::db::Db;
use crate::detect;
use crate::diagnostics;
use crate::dosconf::{self, ConfigView, ConfigWarning, Machine};
use crate::downloads;
use crate::error::{CommandResult, Error, Result};
use crate::exit_screen;
use crate::firing::{self, Firing};
use crate::free_games::{self, FreeGame};
use crate::jobs::{self, Job, JobKind, JobPhase, Jobs};
use crate::keenwiki::{self, UpdateStatus, WikiMod};
use crate::launcher::{self, GameLaunch, LaunchPhase, PlaySessionEnded, Playing, Running};
use crate::library::{self, Install};
use crate::mod_index::{self, IndexMod, IndexStatus, IndexUpdated, ModIndex};
use crate::mods::{self, Curated, Dropped, InstallModRequest, Mod, ModInfo};
use crate::paths::{self, AppPaths};
use crate::profiles::{self, Profile};
use crate::readme::{self, Readme};
use crate::recipe::Recipe;
use crate::runtime::{self, RuntimeStatus};
use crate::saves::{self, SaveFile};
use crate::settings::{self, IndexNews, SetupStatus};
use crate::source::{ArchiveDamaged, Source};
use crate::storage::{self, StorageUsage};
use crate::stores::{self, Store, StoreInstall};

/// Where the data folder pointer lives, and the default data folder.
pub struct DataLocation {
    pub config_dir: PathBuf,
    pub default_root: PathBuf,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub default_data_dir: String,
    pub schema_version: u32,
}

#[tauri::command]
#[specta::specta]
pub fn app_info(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    location: State<'_, DataLocation>,
    db: State<'_, Db>,
) -> CommandResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: paths.root.display().to_string(),
        default_data_dir: location.default_root.display().to_string(),
        schema_version: db.schema_version()?,
    })
}

/// The diagnostics for a bug report, as Markdown.
#[tauri::command]
#[specta::specta]
pub fn diagnostics(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    location: State<'_, DataLocation>,
    db: State<'_, Db>,
) -> CommandResult<String> {
    let info = report_info(&app, &paths, &location);
    Ok(diagnostics::report(&paths, &db, &info)?)
}

/// What reports say about the app itself.
fn report_info(app: &AppHandle, paths: &AppPaths, location: &DataLocation) -> diagnostics::App {
    diagnostics::App {
        version: app.package_info().version.to_string(),
        webview: tauri::webview_version().ok(),
        moved: paths.root != location.default_root,
    }
}

/// The report for a mod that doesn't start, as Markdown.
#[tauri::command]
#[specta::specta]
pub fn mod_report(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    location: State<'_, DataLocation>,
    db: State<'_, Db>,
    profile_id: String,
) -> CommandResult<String> {
    let info = report_info(&app, &paths, &location);
    Ok(diagnostics::mod_report(&paths, &db, &info, &profile_id)?)
}

#[tauri::command]
#[specta::specta]
pub fn list_games() -> Vec<Game> {
    catalog::games().to_vec()
}

#[tauri::command]
#[specta::specta]
pub fn list_installs(db: State<'_, Db>) -> CommandResult<Vec<Install>> {
    Ok(library::list(&db)?)
}

/// Imports all games found in a folder or ZIP file.
#[tauri::command]
#[specta::specta]
pub async fn import_games(app: AppHandle, path: String) -> CommandResult<Vec<Install>> {
    blocking(move || {
        let path = Path::new(&path);
        let installs = library::import(&app.state::<AppPaths>(), &app.state::<Db>(), path)?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        warn_if_damaged(&app, path, &name);
        Ok(installs)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_profiles(app: AppHandle) -> CommandResult<Vec<Profile>> {
    // Counting saves reads every profile's folders: not on the main thread.
    blocking(move || {
        let running = app.state::<Running>();
        profiles::list(&app.state::<Db>(), |id| running.contains(id))
    })
    .await
}

/// Disk space per area of the data folder; walks it, so it takes a moment.
#[tauri::command]
#[specta::specta]
pub async fn storage_usage(app: AppHandle) -> CommandResult<StorageUsage> {
    blocking(move || Ok(storage::usage(&app.state::<AppPaths>()))).await
}

/// Starts a profile in DOSBox. Returns once the process is running; emits
/// `GameLaunch` as it gets going and `PlaySessionEnded` when it exits.
#[tauri::command]
#[specta::specta]
pub async fn launch_profile(app: AppHandle, profile_id: String) -> CommandResult<()> {
    let runtime = runtime::active(&app.state::<Db>())?;
    app.state::<Running>().claim(&profile_id)?;
    let launch = |phase| {
        let _ = GameLaunch {
            profile_id: profile_id.clone(),
            phase,
        }
        .emit(&app);
    };
    launch(LaunchPhase::Starting);
    let a = app.clone();
    let id = profile_id.clone();
    let started = blocking(move || {
        let backup = backups::before_launch(&a.state::<AppPaths>(), &a.state::<Db>(), &id);
        let spec = launcher::prepare(&a.state::<AppPaths>(), &a.state::<Db>(), &id)?;
        let shown = {
            let (app, profile_id) = (a.clone(), id.clone());
            move || {
                let _ = GameLaunch {
                    profile_id,
                    phase: LaunchPhase::Shown,
                }
                .emit(&app);
            }
        };
        let (child, session) = launcher::spawn(&runtime, &spec, &a.state::<Db>(), &id, shown)?;
        Ok((child, session, backup))
    })
    .await;
    let (mut child, session, backup) = match started {
        Ok(s) => s,
        Err(e) => {
            launch(LaunchPhase::Failed);
            app.state::<Running>().release(&profile_id);
            return Err(e);
        }
    };

    let mut stop = app.state::<Playing>().add(&profile_id, session);
    tauri::async_runtime::spawn(async move {
        let (status, quitting) = tokio::select! {
            status = child.wait() => (status, false),
            // Quitting BloogBase with the games: the session ends as usual.
            Ok(()) = &mut stop => {
                tracing::info!("stopping {profile_id} as BloogBase quits");
                let _ = child.start_kill();
                (child.wait().await, true)
            }
        };
        let exit_code = match status {
            Ok(status) => status.code(),
            Err(e) => {
                tracing::error!("lost track of DOSBox for {profile_id}: {e}");
                None
            }
        };
        app.state::<Playing>().remove(&profile_id);
        let seconds =
            launcher::finish(&app.state::<Db>(), session, exit_code).unwrap_or_else(|e| {
                tracing::error!("could not record end of session {session}: {e}");
                0
            });
        // Still locked, so nothing changes the saves while they're compared.
        if let Some(backup) = backup
            && let Err(e) = backups::after_session(&app.state::<Db>(), &profile_id, backup)
        {
            tracing::warn!("could not check backup {backup} after the session: {e}");
        }
        app.state::<Running>().release(&profile_id);
        tracing::info!("{profile_id} exited with {exit_code:?} after {seconds}s");
        let message =
            exit_screen::message(&app.state::<AppPaths>().profile(&profile_id).exit_screen);
        if let Some(m) = &message {
            tracing::info!("{profile_id} ended with: {m}");
        }
        let start_problem = !quitting
            && (message.is_some() || seconds < launcher::SHORT_SESSION)
            && launcher::first_with_setup(&app.state::<Db>(), session).unwrap_or_else(|e| {
                tracing::warn!("could not tell if session {session} was the first: {e}");
                false
            });
        let _ = PlaySessionEnded {
            profile_id,
            exit_code,
            seconds,
            message,
            start_problem,
        }
        .emit(&app);
    });
    Ok(())
}

/// Runs `f` with the profile marked as running, so the game can't start
/// while BloogBase changes its files (§11: locked while running).
async fn with_profile_locked<T: Send + 'static>(
    app: &AppHandle,
    profile_id: &str,
    f: impl FnOnce(&AppPaths, &Db) -> Result<T> + Send + 'static,
) -> CommandResult<T> {
    app.state::<Running>().claim(profile_id)?;
    let a = app.clone();
    let result = blocking(move || f(&a.state::<AppPaths>(), &a.state::<Db>())).await;
    app.state::<Running>().release(profile_id);
    result
}

#[tauri::command]
#[specta::specta]
pub async fn list_saves(app: AppHandle, profile_id: String) -> CommandResult<Vec<SaveFile>> {
    blocking(move || saves::list(&app.state::<Db>(), &profile_id)).await
}

/// The earlier versions of a save that backups hold, newest first.
#[tauri::command]
#[specta::specta]
pub async fn save_versions(
    app: AppHandle,
    profile_id: String,
    file: String,
) -> CommandResult<Vec<SaveVersion>> {
    blocking(move || backups::save_versions(&app.state::<Db>(), &profile_id, &file)).await
}

/// Deletes a save; it's backed up first. Returns that backup, to undo from.
#[tauri::command]
#[specta::specta]
pub async fn delete_save(app: AppHandle, profile_id: String, file: String) -> CommandResult<u32> {
    let id = profile_id.clone();
    with_profile_locked(&app, &profile_id, move |paths, db| {
        saves::delete(paths, db, &id, &file)
    })
    .await
}

/// Screenshots taken in DOSBox, newest first.
#[tauri::command]
#[specta::specta]
pub fn list_screenshots(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: String,
) -> CommandResult<Vec<Screenshot>> {
    profiles::get(&db, &profile_id)?;
    Ok(captures::screenshots(&paths, &profile_id)?)
}

#[tauri::command]
#[specta::specta]
pub fn delete_screenshot(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: String,
    file: String,
) -> CommandResult<()> {
    profiles::get(&db, &profile_id)?;
    Ok(captures::delete(
        &paths,
        &profile_id,
        Kind::Screenshot,
        &file,
    )?)
}

/// Videos recorded in DOSBox, newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_videos(app: AppHandle, profile_id: String) -> CommandResult<Vec<Video>> {
    blocking(move || {
        profiles::get(&app.state::<Db>(), &profile_id)?;
        captures::videos(&app.state::<AppPaths>(), &profile_id)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn delete_video(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: String,
    file: String,
) -> CommandResult<()> {
    profiles::get(&db, &profile_id)?;
    Ok(captures::delete(&paths, &profile_id, Kind::Video, &file)?)
}

/// Where a video's frames and sound are, for playing it in the app.
#[tauri::command]
#[specta::specta]
pub async fn video_index(
    app: AppHandle,
    profile_id: String,
    file: String,
) -> CommandResult<VideoIndex> {
    blocking(move || {
        profiles::get(&app.state::<Db>(), &profile_id)?;
        captures::video_index(&app.state::<AppPaths>(), &profile_id, &file)
    })
    .await
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCover {
    pub path: String,
    /// The graphics files don't fit together, so the game shows scrambled
    /// graphics (as Keen Static does on purpose).
    pub scrambled: bool,
}

/// The profile's title screen as a PNG path, extracted on first use; `None`
/// keeps the placeholder.
#[tauri::command]
#[specta::specta]
pub async fn profile_cover(
    app: AppHandle,
    profile_id: String,
) -> CommandResult<Option<ProfileCover>> {
    blocking(move || {
        Ok(
            covers::cover(&app.state::<AppPaths>(), &app.state::<Db>(), &profile_id)?.map(|c| {
                ProfileCover {
                    path: c.png.display().to_string(),
                    scrambled: c.scrambled,
                }
            }),
        )
    })
    .await
}

/// Opens the folder DOSBox writes screenshots and recordings to.
#[tauri::command]
#[specta::specta]
pub fn open_capture_folder(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: String,
) -> CommandResult<()> {
    profiles::get(&db, &profile_id)?;
    let dir = paths.profile(&profile_id).capture;
    std::fs::create_dir_all(&dir).map_err(Error::from)?;
    open_path(&app, &dir)
}

/// Opens the folder with a mod's installed files.
#[tauri::command]
#[specta::specta]
pub fn open_mod_folder(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    mod_id: String,
) -> CommandResult<()> {
    if !mods::list(&db)?.iter().any(|m| m.id == mod_id) {
        return Err(Error::NotFound(format!("mod {mod_id}")).into());
    }
    open_path(&app, &paths.mods.join(&mod_id).join("files"))
}

/// Opens the folder with a game's files in the library, for a full game,
/// shareware episode or demo alike.
#[tauri::command]
#[specta::specta]
pub fn open_game_folder(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    install_id: String,
) -> CommandResult<()> {
    if !library::list(&db)?.iter().any(|i| i.id == install_id) {
        return Err(Error::NotFound(format!("install {install_id}")).into());
    }
    open_path(&app, &paths.library.join(&install_id).join("base"))
}

/// Opens a web page or Word document shipped with a mod in the browser or
/// its app; the app never shows HTML itself.
#[tauri::command]
#[specta::specta]
pub async fn open_mod_readme(app: AppHandle, mod_id: String, file: String) -> CommandResult<()> {
    let path = {
        let app = app.clone();
        blocking(move || {
            readme::file_path(&app.state::<AppPaths>(), &app.state::<Db>(), &mod_id, &file)
        })
        .await?
    };
    open_path(&app, &path)
}

/// Opens a web page or Word document of a game in the library, as
/// [`open_mod_readme`].
#[tauri::command]
#[specta::specta]
pub async fn open_game_readme(
    app: AppHandle,
    install_id: String,
    file: String,
) -> CommandResult<()> {
    let path = {
        let app = app.clone();
        blocking(move || {
            readme::game_file_path(
                &app.state::<AppPaths>(),
                &app.state::<Db>(),
                &install_id,
                &file,
            )
        })
        .await?
    };
    open_path(&app, &path)
}

/// The documents in a folder or ZIP not installed yet, the README first.
#[tauri::command]
#[specta::specta]
pub async fn readmes_at(path: String) -> CommandResult<Vec<Readme>> {
    blocking(move || readme::at(Path::new(&path))).await
}

/// Opens a web page or Word document in a folder or ZIP not installed yet,
/// as [`open_mod_readme`].
#[tauri::command]
#[specta::specta]
pub async fn open_readme_at(app: AppHandle, path: String, file: String) -> CommandResult<()> {
    let page = {
        let app = app.clone();
        blocking(move || readme::file_path_at(&app.state::<AppPaths>(), Path::new(&path), &file))
            .await?
    };
    open_path(&app, &page)
}

/// Shows a folder in the system's file manager, or a file in its app.
fn open_path(app: &AppHandle, path: &Path) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| Error::from(std::io::Error::other(e.to_string())))?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn list_backups(db: State<'_, Db>, profile_id: String) -> CommandResult<Vec<Backup>> {
    Ok(backups::list(&db, &profile_id)?)
}

/// Backs up one save, or with `file` unset everything the game wrote.
/// `None` if there was nothing to back up.
#[tauri::command]
#[specta::specta]
pub async fn create_backup(
    app: AppHandle,
    profile_id: String,
    label: Option<String>,
    file: Option<String>,
) -> CommandResult<Option<Backup>> {
    let id = profile_id.clone();
    with_profile_locked(&app, &profile_id, move |paths, db| {
        let files: Vec<String> = file.into_iter().collect();
        let contents = if files.is_empty() {
            Contents::State
        } else {
            Contents::Files(&files)
        };
        backups::create(paths, db, &id, contents, Origin::User(label.as_deref()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn backup_saves(app: AppHandle, backup_id: u32) -> CommandResult<Vec<SaveFile>> {
    blocking(move || backups::saves_in(&app.state::<Db>(), backup_id)).await
}

/// The backup made when this mod or game was removed before, to offer
/// when it's added again.
#[tauri::command]
#[specta::specta]
pub async fn earlier_backup(
    app: AppHandle,
    profile_id: String,
) -> CommandResult<Option<backups::EarlierBackup>> {
    blocking(move || backups::earlier(&app.state::<Db>(), &profile_id)).await
}

/// Restores or declines the backup [`earlier_backup`] offered.
#[tauri::command]
#[specta::specta]
pub async fn answer_earlier_backup(
    app: AppHandle,
    profile_id: String,
    backup_id: u32,
    restore: bool,
) -> CommandResult<()> {
    let id = profile_id.clone();
    with_profile_locked(&app, &id, move |paths, db| {
        backups::answer_earlier(paths, db, &profile_id, backup_id, restore)
    })
    .await
}

/// Restores a backup's saves (all its files with `include_other`), or only
/// its `file`, optionally into the slot file `target`. What's there now is
/// backed up first.
#[tauri::command]
#[specta::specta]
pub async fn restore_backup(
    app: AppHandle,
    backup_id: u32,
    file: Option<String>,
    target: Option<String>,
    include_other: bool,
) -> CommandResult<()> {
    let profile_id = backups::get(&app.state::<Db>(), backup_id)?.profile_id;
    with_profile_locked(&app, &profile_id, move |paths, db| {
        backups::restore(
            paths,
            db,
            backup_id,
            file.as_deref(),
            target.as_deref(),
            include_other,
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn delete_backup(db: State<'_, Db>, backup_id: u32) -> CommandResult<()> {
    Ok(backups::delete(&db, backup_id)?)
}

#[tauri::command]
#[specta::specta]
pub async fn export_backup(app: AppHandle, backup_id: u32, path: String) -> CommandResult<()> {
    blocking(move || backups::export(&app.state::<Db>(), backup_id, Path::new(&path))).await
}

/// Adds a ZIP of saves as a new backup of the profile.
#[tauri::command]
#[specta::specta]
pub async fn import_backup(
    app: AppHandle,
    profile_id: String,
    path: String,
) -> CommandResult<Backup> {
    blocking(move || {
        backups::import(
            &app.state::<AppPaths>(),
            &app.state::<Db>(),
            &profile_id,
            Path::new(&path),
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn backup_settings(db: State<'_, Db>) -> CommandResult<BackupSettings> {
    Ok(backups::get_settings(&db)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_backup_settings(db: State<'_, Db>, settings: BackupSettings) -> CommandResult<()> {
    Ok(backups::set_settings(&db, &settings)?)
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayOptionsView {
    pub options: dosconf::DisplayOptions,
    /// Options the user's own DOSBox settings for all games override.
    pub overridden: Vec<String>,
}

/// The simple display options on the settings page.
#[tauri::command]
#[specta::specta]
pub fn display_options(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
) -> CommandResult<DisplayOptionsView> {
    Ok(DisplayOptionsView {
        options: dosconf::options(&db)?,
        overridden: dosconf::overridden_options(&paths)?,
    })
}

#[tauri::command]
#[specta::specta]
pub fn set_display_options(
    db: State<'_, Db>,
    options: dosconf::DisplayOptions,
) -> CommandResult<()> {
    Ok(dosconf::set_options(&db, &options)?)
}

/// Whether Keen 1-3 fire with Space, from the settings page.
#[tauri::command]
#[specta::specta]
pub fn one_button_firing(db: State<'_, Db>) -> CommandResult<bool> {
    Ok(firing::enabled(&db)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_one_button_firing(db: State<'_, Db>, on: bool) -> CommandResult<()> {
    Ok(firing::set_enabled(&db, on)?)
}

/// Where a profile stands with one-button firing, whether it's on or not;
/// `None` for games without it.
#[tauri::command]
#[specta::specta]
pub fn profile_firing(db: State<'_, Db>, profile_id: String) -> CommandResult<Option<Firing>> {
    let (profile, layers) = profiles::get(&db, &profile_id)?;
    let Some(game) = catalog::game(&profile.game_id) else {
        return Ok(None);
    };
    let recipe = layers.mod_layer.as_ref().map(|m| &m.recipe);
    Ok(firing::profile_status(
        game,
        recipe.unwrap_or(&Recipe::Episode),
        &layers.sources(),
        profile.version_id.as_deref(),
    ))
}

/// The machine a profile's mod needs instead of the default one, if any.
#[tauri::command]
#[specta::specta]
pub fn profile_machine(db: State<'_, Db>, profile_id: String) -> CommandResult<Option<Machine>> {
    let (_, layers) = profiles::get(&db, &profile_id)?;
    Ok(dosconf::mod_machine(&layers))
}

/// The DOSBox config layers: the global override, and with `profile_id`
/// that profile's layers and the effective result.
#[tauri::command]
#[specta::specta]
pub fn config_view(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: Option<String>,
) -> CommandResult<ConfigView> {
    Ok(dosconf::view(&paths, &db, profile_id.as_deref())?)
}

/// Saves the global override, or a profile's with `profile_id`.
#[tauri::command]
#[specta::specta]
pub fn save_config(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: Option<String>,
    text: String,
) -> CommandResult<Vec<ConfigWarning>> {
    Ok(dosconf::save(&paths, &db, profile_id.as_deref(), &text)?)
}

#[tauri::command]
#[specta::specta]
pub fn reset_config(
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
    profile_id: Option<String>,
) -> CommandResult<()> {
    Ok(dosconf::reset(&paths, &db, profile_id.as_deref())?)
}

/// Looks at a folder or ZIP and reports the mod found in it, for the user to confirm.
#[tauri::command]
#[specta::specta]
pub async fn inspect_mod(app: AppHandle, path: String) -> CommandResult<ModInfo> {
    blocking(move || {
        let mut info = mods::inspect(Path::new(&path), None)?;
        info.index_id = index_match(&app, &path, &info);
        find_base_mods(&app, &mut info);
        Ok(info)
    })
    .await
}

/// The index entry whose download `path` is, for `info` found in it.
fn index_match(app: &AppHandle, path: &str, info: &ModInfo) -> Option<String> {
    app.state::<ModIndex>()
        .entry_for_archive(Path::new(path), &info.game_id)
        .map(|e| e.id)
}

/// For each way to start `info` that lacks files, the mod it's made to be
/// copied over (see [`mods::needed_mod`]); for levels alone, the one their
/// archive names ([`mods::named_mod`]). Among the index's and the library's
/// for the same game; mods played over another one never are.
fn find_base_mods(app: &AppHandle, info: &mut ModInfo) {
    let levels_for =
        |o: &mods::RecipeOption| o.missing_files.is_empty() && o.recipe == Recipe::Episode;
    let named = info
        .made_for
        .clone()
        .filter(|_| info.recipes.iter().any(levels_for));
    if named.is_none() && info.recipes.iter().all(|o| o.missing_files.is_empty()) {
        return;
    }
    let paths = app.state::<AppPaths>();
    let db = app.state::<Db>();
    let index = app.state::<ModIndex>().entries();
    let installed = mods::list(&db).unwrap_or_default();
    let candidates: Vec<mods::BaseCandidate> = index
        .into_iter()
        .filter(|e| e.game == info.game_id && e.extends.is_none())
        .map(|e| mods::BaseCandidate {
            mod_id: mods::find_by_index(&db, &e.id).ok().flatten(),
            name: e.name,
            index_id: Some(e.id),
            files: None,
            provides: e.provides,
        })
        .chain(
            installed
                .into_iter()
                .filter(|m| m.game_id == info.game_id && m.extends.is_none())
                .map(|m| mods::BaseCandidate {
                    files: Some(paths.mods.join(&m.id).join("files")),
                    name: m.name,
                    mod_id: Some(m.id),
                    index_id: m.index_id,
                    provides: Vec::new(),
                }),
        )
        .collect();
    for option in &mut info.recipes {
        option.made_for = named.clone().filter(|_| levels_for(option));
        option.base_mod = match &named {
            Some(name) if levels_for(option) => mods::named_mod(name, &candidates),
            _ => mods::needed_mod(&option.missing_files, &candidates),
        };
    }
}

/// Whether a folder or ZIP dropped on the window holds games or a mod.
#[tauri::command]
#[specta::specta]
pub async fn inspect_dropped(app: AppHandle, path: String) -> CommandResult<Dropped> {
    blocking(move || {
        let mut dropped = mods::classify(Path::new(&path))?;
        if let Dropped::Mod { info } = &mut dropped {
            info.index_id = index_match(&app, &path, info);
            find_base_mods(&app, info);
        }
        Ok(dropped)
    })
    .await
}

/// Installs a mod inspected with `inspect_mod` and creates its play profile.
#[tauri::command]
#[specta::specta]
pub async fn install_mod(app: AppHandle, request: InstallModRequest) -> CommandResult<Mod> {
    blocking(move || {
        let paths = app.state::<AppPaths>();
        // Replacing must not swap files under a running game.
        let running = app.state::<Running>();
        if let Some(id) = &request.replace {
            running.claim(id)?;
        }
        let m = mods::install(&paths, &app.state::<Db>(), &request);
        if let Some(id) = &request.replace {
            running.release(id);
        }
        let m = m?;
        warn_if_damaged(&app, Path::new(&request.path), &m.name);
        // Installed by hand from an index download.
        link_installed(&app);
        // The mod keeps its own copy of the archive.
        let downloads = keenwiki_downloads(&paths);
        if Path::new(&request.path).starts_with(&downloads) {
            let _ = std::fs::remove_dir_all(&downloads);
        }
        Ok(m)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn list_mods(db: State<'_, Db>) -> CommandResult<Vec<Mod>> {
    Ok(mods::list(&db)?)
}

/// The documents shipped with a mod, the README first; empty
/// when it has none.
#[tauri::command]
#[specta::specta]
pub async fn mod_readmes(app: AppHandle, mod_id: String) -> CommandResult<Vec<Readme>> {
    blocking(move || readme::for_mod(&app.state::<AppPaths>(), &app.state::<Db>(), &mod_id)).await
}

/// The documents of a game in the library, the README first; empty when it
/// has none.
#[tauri::command]
#[specta::specta]
pub async fn game_readmes(app: AppHandle, install_id: String) -> CommandResult<Vec<Readme>> {
    blocking(move || readme::for_game(&app.state::<AppPaths>(), &app.state::<Db>(), &install_id))
        .await
}

/// The installs a mod can be played on.
#[tauri::command]
#[specta::specta]
pub async fn mod_installs(app: AppHandle, mod_id: String) -> CommandResult<Vec<Install>> {
    blocking(move || mods::fitting_installs(&app.state::<Db>(), &mod_id)).await
}

/// Plays a mod on another install it fits.
#[tauri::command]
#[specta::specta]
pub async fn set_mod_install(
    app: AppHandle,
    mod_id: String,
    install_id: String,
) -> CommandResult<()> {
    // The profile id equals the mod id.
    let id = mod_id.clone();
    with_profile_locked(&app, &id, move |_, db| {
        mods::set_install(db, &mod_id, &install_id)
    })
    .await
}

/// Removes a mod and its profile; `keep_saves` archives the saves first.
#[tauri::command]
#[specta::specta]
pub async fn delete_mod(app: AppHandle, mod_id: String, keep_saves: bool) -> CommandResult<()> {
    // Profile ids equal mod ids; claiming them keeps a launch from racing the
    // removal, of the mods played over this one too.
    let mut claimed = vec![mod_id.clone()];
    claimed.extend(mods::extensions(&app.state::<Db>(), &mod_id)?);
    let running = app.state::<Running>();
    for (n, id) in claimed.iter().enumerate() {
        if let Err(e) = running.claim(id) {
            claimed[..n].iter().for_each(|c| running.release(c));
            return Err(e.into());
        }
    }
    let a = app.clone();
    let id = mod_id.clone();
    let result =
        blocking(move || mods::delete(&a.state::<AppPaths>(), &a.state::<Db>(), &id, keep_saves))
            .await;
    claimed.iter().for_each(|c| running.release(c));
    result
}

/// Uninstalls a game; `with_mods` removes the mods played on it too, else
/// they move to another fitting game or wait for one.
#[tauri::command]
#[specta::specta]
pub async fn remove_game(
    app: AppHandle,
    install_id: String,
    keep_saves: bool,
    with_mods: bool,
) -> CommandResult<()> {
    // None of the profiles on it may start or run meanwhile.
    let affected: Vec<String> = profiles::list(&app.state::<Db>(), |_| false)?
        .into_iter()
        .filter(|p| p.install_id.as_deref() == Some(install_id.as_str()))
        .map(|p| p.id)
        .collect();
    let running = app.state::<Running>();
    for (n, id) in affected.iter().enumerate() {
        if let Err(e) = running.claim(id) {
            affected[..n].iter().for_each(|id| running.release(id));
            return Err(e.into());
        }
    }
    let a = app.clone();
    let result = blocking(move || {
        library::remove(
            &a.state::<AppPaths>(),
            &a.state::<Db>(),
            &install_id,
            keep_saves,
            with_mods,
        )
    })
    .await;
    affected.iter().for_each(|id| running.release(id));
    result
}

/// What a pasted KeenWiki link leads to: the curated index entry for that
/// page if there is one (§9.3 step 2), otherwise what the page says.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeenwikiLookup {
    pub page_url: String,
    pub index_id: Option<String>,
    pub page: Option<WikiMod>,
}

#[tauri::command]
#[specta::specta]
pub async fn lookup_keenwiki(app: AppHandle, url: String) -> CommandResult<KeenwikiLookup> {
    let page_url = keenwiki::page_url(&url)?;
    let in_index = |page: &str| {
        app.state::<ModIndex>()
            .entries()
            .into_iter()
            .find(|e| {
                e.links
                    .keenwiki
                    .as_deref()
                    .is_some_and(|k| keenwiki::same_page(k, page))
            })
            .map(|e| e.id)
    };
    if let Some(id) = in_index(&page_url) {
        return Ok(KeenwikiLookup {
            page_url,
            index_id: Some(id),
            page: None,
        });
    }
    let cache = app.state::<AppPaths>().cache.join("keenwiki");
    let html = keenwiki::fetch(&cache, &page_url).await?;
    let page = keenwiki::parse(&html, &page_url)?;
    Ok(KeenwikiLookup {
        page_url,
        // The page may have been reached under another name.
        index_id: in_index(&page.page_url),
        page: Some(page),
    })
}

/// Empties the cache of KeenWiki pages, e.g. after a page was fixed.
#[tauri::command]
#[specta::specta]
pub fn clear_keenwiki_cache(paths: State<'_, AppPaths>) -> CommandResult<()> {
    Ok(keenwiki::clear_cache(&paths.cache.join("keenwiki"))?)
}

/// What a mod's KeenWiki page offers compared to the installed version.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdate {
    pub status: UpdateStatus,
    pub page: WikiMod,
    /// What was taken over from the page because it changed there.
    pub refreshed: Vec<mods::PageDetail>,
}

/// Checks a mod's KeenWiki page for a newer download and takes over what the
/// page now says about the mod. The page comes from the cache when it was
/// read today.
#[tauri::command]
#[specta::specta]
pub async fn check_mod_update(app: AppHandle, mod_id: String) -> CommandResult<ModUpdate> {
    let (page_url, url, version) = mods::origin(&app.state::<Db>(), &mod_id)?;
    let page_url = page_url.ok_or_else(|| Error::NotFound(format!("KeenWiki page of {mod_id}")))?;
    let cache = app.state::<AppPaths>().cache.join("keenwiki");
    let html = keenwiki::fetch(&cache, &page_url).await?;
    let page = keenwiki::parse(&html, &page_url)?;
    let refreshed = mods::refresh_details(&app.state::<Db>(), &mod_id, &page)?;
    Ok(ModUpdate {
        status: keenwiki::update_status(&page, &url, version.as_deref()),
        page,
        refreshed,
    })
}

/// A downloaded KeenWiki mod, ready for `install_mod`.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeenwikiDownload {
    pub path: String,
    pub info: ModInfo,
}

/// Downloads one of a KeenWiki page's links and inspects it. `game_id` is
/// the page's episode, used when the files don't tell.
#[tauri::command]
#[specta::specta]
pub async fn download_keenwiki_mod(
    app: AppHandle,
    url: String,
    game_id: Option<String>,
) -> CommandResult<KeenwikiDownload> {
    let dir = keenwiki_downloads(&app.state::<AppPaths>());
    let path = keenwiki::download(&dir, &url, &|received, total| {
        let _ = keenwiki::KeenwikiProgress {
            url: url.clone(),
            // Downloads stop at 256 MB.
            received: received as u32,
            total: total.min(u64::from(u32::MAX)) as u32,
        }
        .emit(&app);
    })
    .await?;
    let p = path.clone();
    let a = app.clone();
    let info = blocking(move || {
        let mut info = mods::inspect(&p, game_id.as_deref())?;
        // The page may not be the one the index links, the download may be.
        info.index_id = index_match(&a, &p.display().to_string(), &info);
        find_base_mods(&a, &mut info);
        Ok(info)
    })
    .await?;
    Ok(KeenwikiDownload {
        path: path.display().to_string(),
        info,
    })
}

fn keenwiki_downloads(paths: &AppPaths) -> PathBuf {
    paths.cache.join("downloads").join("keenwiki")
}

/// A GitHub link suggesting a KeenWiki mod for the index, with a draft entry.
#[tauri::command]
#[specta::specta]
pub fn index_suggestion_url(db: State<'_, Db>, mod_id: String) -> CommandResult<String> {
    Ok(mods::index_suggestion(&db, &mod_id)?)
}

#[tauri::command]
#[specta::specta]
pub fn index_status(index: State<'_, ModIndex>) -> IndexStatus {
    index.status()
}

/// Checks for a new mod index; `force` even if the last check was recent.
#[tauri::command]
#[specta::specta]
pub async fn refresh_index(app: AppHandle, force: bool) -> IndexStatus {
    let status = app.state::<ModIndex>().refresh(force).await;
    link_installed(&app);
    let _ = IndexUpdated(status.clone()).emit(&app);
    status
}

/// See [`mod_index::link_installed`]; a failure only leaves mods untied.
pub fn link_installed(app: &AppHandle) {
    if let Err(e) = mod_index::link_installed(&app.state::<ModIndex>(), &app.state::<Db>()) {
        tracing::warn!("could not tie installed mods to the index: {e}");
    }
}

/// The mods in the index, with what's installed already.
#[tauri::command]
#[specta::specta]
pub fn index_mods(index: State<'_, ModIndex>, db: State<'_, Db>) -> CommandResult<Vec<IndexMod>> {
    Ok(mod_index::list(&index, &db)?)
}

/// Downloads a mod from the index and installs it, or updates it when it's
/// installed already. Without `install_id` it goes on the best fitting game,
/// once a free episode for it has finished downloading; with none,
/// `without_game` lets it wait for one. `archive` is a copy of the download
/// the player has already, used instead of downloading it.
#[tauri::command]
#[specta::specta]
pub fn install_index_mod(
    app: AppHandle,
    index_id: String,
    install_id: Option<String>,
    without_game: bool,
    archive: Option<String>,
) -> Job {
    let a = app.clone();
    let id = index_id.clone();
    jobs::start(&app, JobKind::Mod, Some(index_id), |report| async move {
        let index = a.state::<ModIndex>();
        let entry = index
            .entry(&id)
            .ok_or_else(|| Error::NotFound(format!("{id} in the mod index")))?;
        // A mod played over another one needs that one first.
        let base = match &entry.extends {
            Some(base) if mods::find_by_index(&a.state::<Db>(), base)?.is_none() => Some(
                index
                    .entry(base)
                    .ok_or_else(|| Error::NotFound(format!("{base} in the mod index")))?,
            ),
            _ => None,
        };
        if let Some(base) = base {
            install_entry(&a, &report, base, None, install_id.clone(), without_game).await?;
        }
        install_entry(&a, &report, entry, archive, install_id, without_game).await
    })
}

/// Downloads (unless `archive` is the player's copy) and installs one index
/// mod for [`install_index_mod`].
async fn install_entry(
    a: &AppHandle,
    report: &jobs::Reporter,
    entry: mod_index::IndexEntry,
    archive: Option<String>,
    install_id: Option<String>,
    without_game: bool,
) -> Result<()> {
    let id = entry.id.clone();
    let download = entry.downloads[0].clone();
    let size = u64::from(download.size);
    let local = archive.map(PathBuf::from);
    let archive = match &local {
        Some(path) => {
            let hash = detect::sha256_hex(&tokio::fs::read(path).await?);
            if !hash.eq_ignore_ascii_case(&download.sha256) {
                return Err(Error::UnexpectedContent(format!(
                    "{} is not the download of {id}",
                    path.display()
                )));
            }
            path.clone()
        }
        None => {
            let archive = a
                .state::<AppPaths>()
                .cache
                .join("downloads")
                .join(format!("index-{id}.zip"));
            downloads::fetch(&download.url, &download.sha256, size, &archive, &|r, t| {
                report.progress(JobPhase::Downloading, r, t)
            })
            .await?;
            archive
        }
    };
    report.progress(JobPhase::Installing, size, size);
    // Right after the setup wizard the game may still be on its way.
    while a.state::<Jobs>().is_running(JobKind::FreeGame, |v| {
        free_games::game_of(v) == Some(entry.game.as_str())
    }) {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let curated = Curated::new(&entry, 0);
    let b = a.clone();
    let file = archive.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (paths, db, running) = (b.state::<AppPaths>(), b.state::<Db>(), b.state::<Running>());
        // An update must not swap files under a running game.
        let existing = mods::find_by_index(&db, &curated.index_id)?;
        if let Some(m) = &existing {
            running.claim(m)?;
        }
        let result = mods::install_curated(
            &paths,
            &db,
            &file,
            &curated,
            install_id.as_deref(),
            without_game,
        );
        if let Some(m) = &existing {
            running.release(m);
        }
        result
    })
    .await
    .map_err(Error::from)
    .and_then(|r| r);
    if let Ok(m) = &result {
        warn_if_damaged(a, &archive, &m.name);
    }
    // The player's own copy stays.
    if local.is_none() {
        let _ = std::fs::remove_file(&archive);
    }
    result.map(drop)
}

#[tauri::command]
#[specta::specta]
pub fn runtime_status(db: State<'_, Db>) -> CommandResult<RuntimeStatus> {
    Ok(runtime::status(&db)?)
}

/// Starts downloading and installing the pinned DOSBox Staging build.
#[tauri::command]
#[specta::specta]
pub fn install_runtime(app: AppHandle) -> Job {
    let a = app.clone();
    jobs::start(&app, JobKind::Runtime, None, |report| async move {
        let paths = a.state::<AppPaths>().inner().clone();
        runtime::install(paths, &a.state::<Db>(), |phase, received, total| {
            report.progress(phase, received, total)
        })
        .await
        .map(drop)
    })
}

/// Uses the given DOSBox Staging executable instead of the managed one;
/// `None` switches back.
#[tauri::command]
#[specta::specta]
pub async fn set_custom_runtime(
    app: AppHandle,
    path: Option<String>,
) -> CommandResult<RuntimeStatus> {
    blocking(move || runtime::set_custom(&app.state::<Db>(), path.as_deref().map(Path::new))).await
}

#[tauri::command]
#[specta::specta]
pub fn list_jobs(jobs: State<'_, Jobs>) -> Vec<Job> {
    jobs.list()
}

#[tauri::command]
#[specta::specta]
pub fn clear_finished_jobs(jobs: State<'_, Jobs>) {
    jobs.clear_finished();
}

#[tauri::command]
#[specta::specta]
pub fn free_games(db: State<'_, Db>) -> CommandResult<Vec<FreeGame>> {
    Ok(free_games::list(&db)?)
}

/// Starts downloading a free game version into the library.
#[tauri::command]
#[specta::specta]
pub fn install_free_game(app: AppHandle, version_id: String) -> Job {
    let a = app.clone();
    let id = version_id.clone();
    jobs::start(
        &app,
        JobKind::FreeGame,
        Some(version_id),
        |report| async move {
            free_games::install(
                &a.state::<AppPaths>(),
                &a.state::<Db>(),
                &id,
                |phase, received, total| report.progress(phase, received, total),
            )
            .await
        },
    )
}

/// Looks for Commander Keen in the Steam and GOG library folders.
#[tauri::command]
#[specta::specta]
pub async fn scan_stores(app: AppHandle) -> CommandResult<Vec<StoreInstall>> {
    blocking(move || stores::scan(&app.state::<AppPaths>(), &app.state::<Db>())).await
}

/// Imports the chosen episodes from a folder found by `scan_stores`.
#[tauri::command]
#[specta::specta]
pub async fn import_store_games(
    app: AppHandle,
    store: Store,
    path: String,
    game_ids: Vec<String>,
) -> CommandResult<Vec<Install>> {
    blocking(move || {
        library::import_from(
            &app.state::<AppPaths>(),
            &app.state::<Db>(),
            Path::new(&path),
            store.install_source(),
            &path,
            Some(&game_ids),
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn setup_status(db: State<'_, Db>) -> CommandResult<SetupStatus> {
    Ok(settings::setup_status(&db)?)
}

#[tauri::command]
#[specta::specta]
pub fn complete_setup(db: State<'_, Db>) -> CommandResult<()> {
    settings::set(&db, settings::SETUP_COMPLETED, "1")?;
    Ok(settings::set_setup_step(&db, None)?)
}

/// The wizard step to resume at; `None` once it is finished.
#[tauri::command]
#[specta::specta]
pub fn set_setup_step(db: State<'_, Db>, step: Option<String>) -> CommandResult<()> {
    Ok(settings::set_setup_step(&db, step.as_deref())?)
}

/// What the news dialog on start knows already about the mod list.
#[tauri::command]
#[specta::specta]
pub fn index_news(db: State<'_, Db>) -> CommandResult<IndexNews> {
    Ok(settings::index_news(&db)?)
}

#[tauri::command]
#[specta::specta]
pub fn set_index_news(db: State<'_, Db>, news: IndexNews) -> CommandResult<()> {
    Ok(settings::set_index_news(&db, &news)?)
}

/// Keeps BloogBase's data in (or below) `path` from the next start on;
/// `None` goes back to the default folder. Only while the library is empty,
/// so nothing has to be moved. Returns the new data folder.
#[tauri::command]
#[specta::specta]
pub fn set_data_location(
    location: State<'_, DataLocation>,
    db: State<'_, Db>,
    path: Option<String>,
) -> CommandResult<String> {
    if settings::setup_status(&db)?.has_installs {
        return Err(Error::InvalidLocation("the library already has games".into()).into());
    }
    let root = match path {
        Some(p) => paths::root_in(Path::new(&p))?,
        None => location.default_root.clone(),
    };
    // The wizard goes on there after the restart.
    std::fs::create_dir_all(&root).map_err(Error::from)?;
    let target = Db::open(&AppPaths::new(&root).database)?;
    settings::copy_setup_step(&db, &target)?;
    paths::save_root(&location.config_dir, &location.default_root, &root)?;
    tracing::info!("data folder will be {} after a restart", root.display());
    Ok(root.display().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// Quits after [`QuitRequested`](launcher::QuitRequested): with
/// `stop_games` the running games end first, so their play time counts in
/// full; otherwise they run on, and only the time played so far counts.
#[tauri::command]
#[specta::specta]
pub async fn quit_app(app: AppHandle, stop_games: bool) {
    let playing = app.state::<Playing>();
    if stop_games {
        let ids = playing.profile_ids();
        playing.stop_all();
        // Each session is recorded once its DOSBox has exited.
        let running = app.state::<Running>();
        for _ in 0..100 {
            if !ids.iter().any(|id| running.contains(id)) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    } else {
        playing.abandon_all(&app.state::<Db>());
    }
    app.exit(0);
}

/// Runs file system heavy work off the async runtime.
/// Tells the frontend when files imported from `path` failed their checksum.
fn warn_if_damaged(app: &AppHandle, path: &Path, name: &str) {
    match Source::open(path).and_then(|s| s.damaged()) {
        Ok(files) if files.is_empty() => {}
        Ok(files) => {
            tracing::warn!("damaged in {}: {}", path.display(), files.join(", "));
            let _ = ArchiveDamaged {
                name: name.to_string(),
                files,
            }
            .emit(app);
        }
        Err(e) => tracing::warn!("could not check {}: {e}", path.display()),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> CommandResult<T> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(res) => Ok(res?),
        Err(err) => Err(Error::from(err).into()),
    }
}
