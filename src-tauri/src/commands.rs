//! Tauri commands exposed to the frontend. Keep these thin: validate input,
//! call into the core modules, map errors via `CommandError`.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::catalog::{self, Game};
use crate::db::Db;
use crate::error::{CommandResult, Error, Result};
use crate::launcher::{self, PlaySessionEnded, Running};
use crate::library::{self, Install};
use crate::paths::AppPaths;
use crate::profiles::{self, Profile};
use crate::runtime::{self, RuntimeInstallProgress, RuntimeStatus};

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub schema_version: u32,
}

#[tauri::command]
#[specta::specta]
pub fn app_info(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    db: State<'_, Db>,
) -> CommandResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: paths.root.display().to_string(),
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

/// Downloads and installs the pinned DOSBox Staging build, emitting
/// `RuntimeInstallProgress` along the way.
#[tauri::command]
#[specta::specta]
pub async fn install_runtime(app: AppHandle) -> CommandResult<RuntimeStatus> {
    let paths = app.state::<AppPaths>().inner().clone();
    let a = app.clone();
    let progress = move |p: RuntimeInstallProgress| {
        let _ = p.emit(&a);
    };
    Ok(runtime::install(paths, &app.state::<Db>(), progress).await?)
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

/// Runs file system heavy work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> CommandResult<T> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(res) => Ok(res?),
        Err(err) => Err(Error::from(err).into()),
    }
}
