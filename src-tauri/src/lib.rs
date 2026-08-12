mod bookmarklet;
mod commands;
mod credentials;
mod installed;
mod launcher;
mod migrate;
mod models;
mod services;
mod store;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use commands::AppState;
use credentials::Secret;
use models::EnrichmentJob;
use store::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Logging is enabled in release builds too, written to the app's log directory.
            // Without it an installed app fails silently and there's nothing to diagnose from.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: Some("ugly".into()),
                        }),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    ])
                    .build(),
            )?;

            let data_dir = app.path().app_data_dir()?;
            let store = Arc::new(Store::open(&data_dir)?);

            // Carry over data (and credentials) from the pre-Tauri Node version, once.
            if let Some(legacy_dir) = store::legacy_store_dir() {
                match migrate::migrate_legacy_json(&store, &legacy_dir) {
                    Ok(Some(summary)) => log::info!("Migrated legacy data: {summary}"),
                    Ok(None) => {}
                    Err(err) => log::error!("Legacy migration failed: {err}"),
                }

                if let Some(config) = migrate::read_legacy_config(&legacy_dir) {
                    if !credentials::has(Secret::SteamApiKey) {
                        if let Some(key) = config.steam_api_key.filter(|k| !k.trim().is_empty()) {
                            let _ = credentials::set(Secret::SteamApiKey, &key);
                            log::info!("Moved Steam API key into the OS keychain");
                        }
                    }
                    if !credentials::has(Secret::IgdbClientId) {
                        if let Some(id) = config.igdb_client_id.filter(|k| !k.trim().is_empty()) {
                            let _ = credentials::set(Secret::IgdbClientId, &id);
                        }
                    }
                    if !credentials::has(Secret::IgdbClientSecret) {
                        if let Some(secret) =
                            config.igdb_client_secret.filter(|k| !k.trim().is_empty())
                        {
                            let _ = credentials::set(Secret::IgdbClientSecret, &secret);
                        }
                    }
                    if store.get_state("steam_id")?.is_none() {
                        if let Some(id) = config.steam_id.filter(|k| !k.trim().is_empty()) {
                            store.set_state("steam_id", &id)?;
                        }
                    }
                }
            }

            bookmarklet::spawn(app.handle().clone(), store.clone());

            app.manage(AppState {
                store,
                job: Mutex::new(EnrichmentJob::default()),
                job_running: AtomicBool::new(false),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::clear_credentials,
            commands::get_steam_library,
            commands::refresh_steam_library,
            commands::get_family_library,
            commands::get_epic_library,
            commands::import_epic_library,
            commands::get_metadata,
            commands::get_enrichment_job,
            commands::enrich_metadata,
            commands::get_installed,
            commands::launch_game,
            commands::install_game,
            commands::open_external,
            commands::bookmarklet_port,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
