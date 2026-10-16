//! Tauri commands exposed to the frontend. Keep these thin: validate input,
//! call into the core modules, map errors via `CommandError`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::catalog::{self, Game};
use crate::db::Db;
use crate::error::{CommandResult, Error, Result};
use crate::jobs::{self, Job, JobKind, Jobs};
use crate::launcher::{self, PlaySessionEnded, Running};
use crate::library::{self, Install};
use crate::paths::{self, AppPaths};
use crate::profiles::{self, Profile};
use crate::runtime::{self, RuntimeStatus};
use crate::settings::{self, SetupStatus};
use crate::shareware::{self, FreeEpisode};
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
        library::import(
            &app.state::<AppPaths>(),
            &app.state::<Db>(),
            Path::new(&path),
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn list_profiles(
    db: State<'_, Db>,
    running: State<'_, Running>,
) -> CommandResult<Vec<Profile>> {
    Ok(profiles::list(&db, |id| running.contains(id))?)
}

/// Starts a profile in DOSBox. Returns once the process is running; emits
/// `PlaySessionEnded` when it exits.
#[tauri::command]
#[specta::specta]
pub async fn launch_profile(app: AppHandle, profile_id: String) -> CommandResult<()> {
    let runtime = runtime::active(&app.state::<Db>())?;
    app.state::<Running>().claim(&profile_id)?;
    let a = app.clone();
    let id = profile_id.clone();
    let started = blocking(move || {
        let spec = launcher::prepare(&a.state::<AppPaths>(), &a.state::<Db>(), &id)?;
        launcher::spawn(&runtime, &spec, &a.state::<Db>(), &id)
    })
    .await;
    let (mut child, session) = match started {
        Ok(s) => s,
        Err(e) => {
            app.state::<Running>().release(&profile_id);
            return Err(e);
        }
    };

    tauri::async_runtime::spawn(async move {
        let exit_code = match child.wait().await {
            Ok(status) => status.code(),
            Err(e) => {
                tracing::error!("lost track of DOSBox for {profile_id}: {e}");
                None
            }
        };
        let seconds =
            launcher::finish(&app.state::<Db>(), session, exit_code).unwrap_or_else(|e| {
                tracing::error!("could not record end of session {session}: {e}");
                0
            });
        app.state::<Running>().release(&profile_id);
        tracing::info!("{profile_id} exited with {exit_code:?} after {seconds}s");
        let _ = PlaySessionEnded {
            profile_id,
            exit_code,
            seconds,
        }
        .emit(&app);
    });
    Ok(())
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
pub fn free_episodes(db: State<'_, Db>) -> CommandResult<Vec<FreeEpisode>> {
    Ok(shareware::list(&db)?)
}

/// Starts downloading a free shareware episode into the library.
#[tauri::command]
#[specta::specta]
pub fn install_free_episode(app: AppHandle, game_id: String) -> Job {
    let a = app.clone();
    let id = game_id.clone();
    jobs::start(
        &app,
        JobKind::Shareware,
        Some(game_id),
        |report| async move {
            shareware::install(
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
    blocking(move || stores::scan(&app.state::<Db>())).await
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
    Ok(settings::set(&db, settings::SETUP_COMPLETED, "1")?)
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
    paths::save_root(&location.config_dir, &location.default_root, &root)?;
    tracing::info!("data folder will be {} after a restart", root.display());
    Ok(root.display().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// Runs file system heavy work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> CommandResult<T> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(res) => Ok(res?),
        Err(err) => Err(Error::from(err).into()),
    }
}
