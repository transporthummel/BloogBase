//! Tauri commands exposed to the frontend. Keep these thin: validate input,
//! call into the core modules, map errors via `CommandError`.

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use crate::db::Db;
use crate::error::CommandResult;
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
pub fn app_info(app: AppHandle, paths: State<'_, AppPaths>, db: State<'_, Db>) -> CommandResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: paths.root.display().to_string(),
        schema_version: db.schema_version()?,
    })
}
