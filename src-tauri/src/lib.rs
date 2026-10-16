mod catalog;
mod commands;
mod db;
mod detect;
mod error;
mod launcher;
mod library;
mod overlay_view;
mod paths;
mod profiles;
mod runtime;
mod source;

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, log::LevelFilter};
use tauri_specta::{collect_commands, collect_events};

use crate::db::Db;
use crate::launcher::Running;
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
            commands::runtime_status,
            commands::install_runtime,
            commands::set_custom_runtime,
        ])
        .events(collect_events![
            runtime::RuntimeInstallProgress,
            launcher::PlaySessionEnded,
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let paths = AppPaths::new(app.path().app_data_dir()?);
            paths.ensure()?;
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

            app.manage(paths);
            app.manage(db);
            app.manage(Running::default());
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
