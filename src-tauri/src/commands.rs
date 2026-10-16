//! Tauri commands exposed to the frontend. Keep these thin: validate input,
//! call into the core modules, map errors via `CommandError`.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

use crate::catalog::{self, Game};
use crate::db::Db;
use crate::error::{CommandResult, Error, Result};
use crate::library::{self, Install};
use crate::paths::AppPaths;

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

/// Runs file system heavy work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> CommandResult<T> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(res) => Ok(res?),
        Err(err) => Err(Error::from(err).into()),
    }
}
