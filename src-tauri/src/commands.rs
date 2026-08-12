use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::credentials::{self, Secret};
use crate::models::{slugify, EnrichmentJob, EpicLibrary, Game, MetadataEntry, SettingsView};
use crate::services::{epic, igdb, steam};
use crate::store::Store;

pub struct AppState {
    pub store: Arc<Store>,
    pub job: Mutex<EnrichmentJob>,
    pub job_running: AtomicBool,
}

type CmdResult<T> = Result<T, String>;

fn to_err<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsView> {
    Ok(SettingsView {
        steam_id: state
            .store
            .get_state("steam_id")
            .map_err(to_err)?
            .unwrap_or_default(),
        steam_configured: credentials::has(Secret::SteamApiKey)
            && state
                .store
                .get_state("steam_id")
                .map_err(to_err)?
                .is_some_and(|v| !v.trim().is_empty()),
        igdb_configured: credentials::has(Secret::IgdbClientId)
            && credentials::has(Secret::IgdbClientSecret),
    })
}

/// Secrets are write-only from the UI's point of view: they go into the OS keychain and
/// are never returned by any command. Empty strings mean "leave unchanged".
#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    steam_api_key: String,
    steam_id: String,
    igdb_client_id: String,
    igdb_client_secret: String,
) -> CmdResult<SettingsView> {
    if !steam_api_key.trim().is_empty() {
        credentials::set(Secret::SteamApiKey, &steam_api_key).map_err(to_err)?;
    }
    if !igdb_client_id.trim().is_empty() {
        credentials::set(Secret::IgdbClientId, &igdb_client_id).map_err(to_err)?;
    }
    if !igdb_client_secret.trim().is_empty() {
        credentials::set(Secret::IgdbClientSecret, &igdb_client_secret).map_err(to_err)?;
    }
    state
        .store
        .set_state("steam_id", steam_id.trim())
        .map_err(to_err)?;

    get_settings(state)
}

#[tauri::command]
pub fn clear_credentials(state: State<'_, AppState>) -> CmdResult<SettingsView> {
    credentials::set(Secret::SteamApiKey, "").map_err(to_err)?;
    credentials::set(Secret::IgdbClientId, "").map_err(to_err)?;
    credentials::set(Secret::IgdbClientSecret, "").map_err(to_err)?;
    get_settings(state)
}

// ---------- libraries ----------

#[tauri::command]
pub fn get_steam_library(state: State<'_, AppState>) -> CmdResult<Vec<Game>> {
    state.store.steam_games().map_err(to_err)
}

#[tauri::command]
pub async fn refresh_steam_library(state: State<'_, AppState>) -> CmdResult<Vec<Game>> {
    let Some(api_key) = credentials::get(Secret::SteamApiKey) else {
        return Err("Steam is not configured yet.".into());
    };
    let steam_id = state
        .store
        .get_state("steam_id")
        .map_err(to_err)?
        .unwrap_or_default();
    if steam_id.trim().is_empty() {
        return Err("Steam is not configured yet.".into());
    }

    let games = steam::fetch_library(&api_key, &steam_id)
        .await
        .map_err(to_err)?;
    state.store.replace_steam_games(&games).map_err(to_err)?;
    Ok(games)
}

/// Family-shared games, imported separately via the Steam family bookmarklet.
#[tauri::command]
pub fn get_family_library(state: State<'_, AppState>) -> CmdResult<Vec<Game>> {
    state.store.family_games().map_err(to_err)
}

#[tauri::command]
pub fn get_epic_library(state: State<'_, AppState>) -> CmdResult<EpicLibrary> {
    state.store.epic_library().map_err(to_err)
}

#[tauri::command]
pub fn import_epic_library(state: State<'_, AppState>, data: String) -> CmdResult<EpicLibrary> {
    let games = epic::parse_export(&data).map_err(to_err)?;
    state
        .store
        .replace_epic_games(&games, igdb::now_ms())
        .map_err(to_err)?;
    state.store.epic_library().map_err(to_err)
}

// ---------- metadata ----------

#[tauri::command]
pub fn get_metadata(
    state: State<'_, AppState>,
) -> CmdResult<serde_json::Map<String, serde_json::Value>> {
    state.store.all_metadata().map_err(to_err)
}

#[tauri::command]
pub fn get_enrichment_job(state: State<'_, AppState>) -> CmdResult<EnrichmentJob> {
    Ok(state.job.lock().unwrap().clone())
}

/// Kicks off a background pass over any titles with no cached lookup. Returns immediately;
/// progress is polled via `get_enrichment_job` and pushed via the `enrichment-progress` event.
#[tauri::command]
pub fn enrich_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    titles: Vec<String>,
) -> CmdResult<EnrichmentJob> {
    if state.job_running.load(Ordering::SeqCst) {
        return Ok(state.job.lock().unwrap().clone());
    }

    let (Some(client_id), Some(client_secret)) = (
        credentials::get(Secret::IgdbClientId),
        credentials::get(Secret::IgdbClientSecret),
    ) else {
        return Err("IGDB is not configured yet.".into());
    };

    let known = state.store.known_metadata_slugs().map_err(to_err)?;
    let mut seen = HashSet::new();
    let pending: Vec<String> = titles
        .into_iter()
        .filter(|t| !t.trim().is_empty())
        .filter(|t| {
            let slug = slugify(t);
            !slug.is_empty() && !known.contains(&slug) && seen.insert(slug)
        })
        .collect();

    if pending.is_empty() {
        return Ok(state.job.lock().unwrap().clone());
    }

    {
        let mut job = state.job.lock().unwrap();
        job.running = true;
        job.total = pending.len();
        job.completed = 0;
        job.error = None;
    }
    state.job_running.store(true, Ordering::SeqCst);

    let store = state.store.clone();
    tauri::async_runtime::spawn(async move {
        for title in pending {
            let result = igdb::lookup_game(&client_id, &client_secret, &title).await;
            let entry = match result {
                Ok(Some(entry)) => entry,
                Ok(None) => MetadataEntry {
                    not_found: true,
                    fetched_at: igdb::now_ms(),
                    ..Default::default()
                },
                Err(err) => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let mut job = state.job.lock().unwrap();
                        job.error = Some(err.to_string());
                        job.running = false;
                        state.job_running.store(false, Ordering::SeqCst);
                        let _ = app.emit("enrichment-progress", job.clone());
                    }
                    return;
                }
            };

            if let Err(err) = store.save_metadata(&slugify(&title), &entry) {
                log::error!("Could not cache metadata for {title}: {err}");
            }

            if let Some(state) = app.try_state::<AppState>() {
                let snapshot = {
                    let mut job = state.job.lock().unwrap();
                    job.completed += 1;
                    job.clone()
                };
                let _ = app.emit("enrichment-progress", snapshot);
            }

            // IGDB's free tier allows 4 requests/second; stay comfortably under it.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        if let Some(state) = app.try_state::<AppState>() {
            let snapshot = {
                let mut job = state.job.lock().unwrap();
                job.running = false;
                job.clone()
            };
            state.job_running.store(false, Ordering::SeqCst);
            let _ = app.emit("enrichment-progress", snapshot);
        }
    });

    Ok(state.job.lock().unwrap().clone())
}

// ---------- installed games ----------

/// Scans Steam's .acf manifests and Epic's .item manifests and reports which of our
/// library entries are present on disk, keyed by our own game id.
#[tauri::command]
pub fn get_installed(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, crate::installed::InstalledGame>> {
    let mut ids: Vec<(String, String, String)> = Vec::new();
    for game in state.store.steam_games().map_err(to_err)? {
        ids.push((game.id, game.platform, game.title));
    }
    for game in state.store.epic_library().map_err(to_err)?.games {
        ids.push((game.id, game.platform, game.title));
    }

    let found = crate::installed::detect(&ids);
    let steam = found.values().filter(|g| g.platform == "steam").count();
    let epic = found.values().filter(|g| g.platform == "epic").count();
    log::info!(
        "Installed scan: {} matched ({steam} Steam, {epic} Epic) of {} library entries",
        found.len(),
        ids.len()
    );
    Ok(found)
}

#[tauri::command]
pub async fn launch_game(state: State<'_, AppState>, game_id: String) -> CmdResult<()> {
    // Copy what we need out of the state before any await — State isn't Send.
    let entry = {
        let installed = get_installed(state)?;
        installed
            .get(&game_id)
            .cloned()
            .ok_or_else(|| "That game does not look installed.".to_string())?
    };

    match entry.platform.as_str() {
        "steam" => crate::launcher::launch_steam(&entry.launch_id).map_err(to_err),
        "epic" => {
            let (Some(namespace), Some(catalog_item_id)) =
                (entry.namespace.as_deref(), entry.catalog_item_id.as_deref())
            else {
                return Err("This Epic game is missing launch details.".into());
            };
            crate::launcher::launch_epic(namespace, catalog_item_id, &entry.launch_id)
                .await
                .map_err(to_err)
        }
        other => Err(format!("Cannot launch a {other} game.")),
    }
}

#[tauri::command]
pub async fn install_game(game_id: String, platform: String, title: String) -> CmdResult<()> {
    match platform.as_str() {
        "steam" => {
            let appid = game_id
                .strip_prefix("steam-")
                .ok_or_else(|| "Unexpected Steam game id.".to_string())?;
            crate::launcher::install_steam(appid).map_err(to_err)
        }
        // Epic has no install action for a game we don't hold identifiers for, so open its
        // store page inside the Epic launcher rather than a web browser.
        "epic" => crate::launcher::open_epic_store(&title).await.map_err(to_err),
        other => Err(format!("Cannot install a {other} game.")),
    }
}

#[tauri::command]
pub fn open_external(url: String) -> CmdResult<()> {
    crate::launcher::open_external(&url).map_err(to_err)
}

// ---------- bookmarklet ----------

#[tauri::command]
pub fn bookmarklet_port() -> u16 {
    crate::bookmarklet::PORT
}
