mod backups;
mod catalog;
mod commands;
mod compose;
mod covers;
mod db;
mod detect;
mod dosconf;
mod downloads;
mod error;
mod jobs;
mod keenwiki;
mod launcher;
mod library;
mod mod_index;
mod mods;
mod overlay_view;
mod paths;
mod profiles;
mod recipe;
mod runtime;
mod saves;
mod screenshots;
mod settings;
mod shareware;
mod source;
mod stores;

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, log::LevelFilter};
use tauri_specta::Event;
use tauri_specta::{collect_commands, collect_events};

use crate::commands::DataLocation;
use crate::db::Db;
use crate::jobs::Jobs;
use crate::launcher::Running;
use crate::mod_index::{IndexUpdated, ModIndex};
use crate::paths::AppPaths;

/// Where the generated TypeScript bindings for commands and events live.
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app_info,
            commands::list_games,
            commands::list_installs,
            commands::import_games,
            commands::list_profiles,
            commands::launch_profile,
            commands::list_saves,
            commands::delete_save,
            commands::list_screenshots,
            commands::profile_cover,
            commands::delete_screenshot,
            commands::open_capture_folder,
            commands::list_backups,
            commands::create_backup,
            commands::backup_saves,
            commands::restore_backup,
            commands::delete_backup,
            commands::export_backup,
            commands::import_backup,
            commands::backup_settings,
            commands::set_backup_settings,
            commands::display_options,
            commands::set_display_options,
            commands::config_view,
            commands::save_config,
            commands::reset_config,
            commands::inspect_mod,
            commands::install_mod,
            commands::list_mods,
            commands::delete_mod,
            commands::index_status,
            commands::refresh_index,
            commands::index_mods,
            commands::install_index_mod,
            commands::lookup_keenwiki,
            commands::download_keenwiki_mod,
            commands::index_suggestion_url,
            commands::runtime_status,
            commands::install_runtime,
            commands::set_custom_runtime,
            commands::list_jobs,
            commands::clear_finished_jobs,
            commands::free_episodes,
            commands::install_free_episode,
            commands::scan_stores,
            commands::import_store_games,
            commands::setup_status,
            commands::complete_setup,
            commands::set_data_location,
            commands::restart_app,
        ])
        .events(collect_events![
            jobs::JobUpdated,
            launcher::PlaySessionEnded,
            mod_index::IndexUpdated
        ])
}

fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */"),
            BINDINGS_PATH,
        )
        .expect("failed to export TypeScript bindings");
}

fn log_plugin(paths: &AppPaths) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Folder {
                path: paths.logs.clone(),
                file_name: Some("bloogbase".into()),
            }),
        ])
        .level(level)
        .max_file_size(5 * 1024 * 1024)
        .rotation_strategy(RotationStrategy::KeepSome(5))
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let location = DataLocation {
                config_dir: app.path().app_config_dir()?,
                default_root: app.path().app_data_dir()?,
            };
            // `BLOOGBASE_DATA_DIR` keeps tests away from the real data.
            let root = match std::env::var_os("BLOOGBASE_DATA_DIR") {
                Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
                _ => paths::resolve_root(&location.config_dir, location.default_root.clone()),
            };
            let mut paths = AppPaths::new(root);
            if let Err(err) = paths.ensure() {
                // E.g. on a removed USB drive: better to start with the
                // default folder than not at all. Logging isn't up yet.
                eprintln!("data folder {} unusable: {err}", paths.root.display());
                paths = AppPaths::new(location.default_root.clone());
                paths.ensure()?;
            }
            // Registered here rather than on the builder because the log
            // directory lives inside the app data dir, known only at runtime.
            app.handle().plugin(log_plugin(&paths))?;
            tracing::info!(
                "BloogBase {} starting, data dir {}",
                app.package_info().version,
                paths.root.display()
            );

            let db = Db::open(&paths.database)?;
            tracing::info!("database ready, schema version {}", db.schema_version()?);

            let index = ModIndex::open(&paths, mod_index::configured_url());
            tracing::info!("mod index source: {}", index.status().url);
            // Screenshots are shown straight from the capture folders.
            app.asset_protocol_scope()
                .allow_directory(&paths.profiles, true)?;
            std::fs::create_dir_all(covers::dir(&paths))?;
            app.asset_protocol_scope()
                .allow_directory(covers::dir(&paths), false)?;
            app.manage(index);
            app.manage(paths);
            app.manage(db);
            app.manage(Running::default());
            app.manage(Jobs::default());
            app.manage(location);

            // On start and every few hours (§10.2); usually a 304.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let status = handle.state::<ModIndex>().refresh(false).await;
                    let _ = IndexUpdated(status).emit(&handle);
                    tokio::time::sleep(mod_index::REFRESH_EVERY).await;
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Regenerates `src/bindings.ts`; CI fails if the committed file is stale.
    #[test]
    fn export_bindings() {
        super::export_bindings(&super::specta_builder());
    }
}
