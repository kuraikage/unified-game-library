use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::credentials::{self, Secret};
use crate::jobs::JobSlot;
use crate::services::{epic, igdb, steam, steam_tags};
use ugly_core::metadata;
use ugly_core::models::{
    self, slugify, EnrichmentJob, EpicLibrary, Game, GameStatus, MetadataEntry, SettingsView,
    StatusEntry,
};
use ugly_core::store::Store;

pub struct AppState {
    pub store: Arc<Store>,
    /// One slot per source: they have different credentials, runtimes and failure modes.
    pub igdb_job: Arc<JobSlot>,
    pub steam_job: Arc<JobSlot>,
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

/// Both sources merged, keyed by slug. Serializes to the same JS object shape as before,
/// with the addition of `igdb`/`steam` provenance flags — see the note in `App.jsx` about
/// why the enrichment check has to read those rather than test for presence.
#[tauri::command]
pub fn get_metadata(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, ugly_core::metadata::MergedMetadata>> {
    state.store.all_metadata().map_err(to_err)
}

#[tauri::command]
pub fn get_enrichment_job(state: State<'_, AppState>) -> CmdResult<EnrichmentJob> {
    Ok(state.igdb_job.snapshot())
}

/// Every library title, deduplicated by slug, newest source winning.
///
/// Derived here rather than passed in from the webview: the caller's list was always just
/// a round trip of what the store already held, and the appid a Steam entry carries is
/// thrown away by the time it reaches JS.
fn library_titles(store: &Store) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut titles = Vec::new();
    for game in store
        .steam_games()
        .map_err(to_err)?
        .into_iter()
        .chain(store.family_games().map_err(to_err)?)
        .chain(store.epic_library().map_err(to_err)?.games)
    {
        let slug = slugify(&game.title);
        if !slug.is_empty() && seen.insert(slug) {
            titles.push(game.title);
        }
    }
    Ok(titles)
}

/// Kicks off a background IGDB pass over anything that needs one. Returns immediately;
/// progress is polled via `get_enrichment_job` and pushed via the `enrichment-progress` event.
///
/// `force` re-fetches everything, including rows already cached.
#[tauri::command]
pub fn enrich_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
) -> CmdResult<EnrichmentJob> {
    if state.igdb_job.is_running() {
        return Ok(state.igdb_job.snapshot());
    }

    let (Some(client_id), Some(client_secret)) = (
        credentials::get(Secret::IgdbClientId),
        credentials::get(Secret::IgdbClientSecret),
    ) else {
        return Err("IGDB is not configured yet.".into());
    };

    let force = force.unwrap_or(false);
    let now = igdb::now_ms();
    let refresh_from = igdb::refresh_marker(&state.store, now).map_err(to_err)?;
    let stamps = state.store.igdb_metadata_stamps().map_err(to_err)?;

    let pending: Vec<String> = library_titles(&state.store)?
        .into_iter()
        .filter(|title| metadata::needs_igdb(stamps.get(&slugify(title)), now, refresh_from, force))
        .collect();

    if pending.is_empty() {
        // Nothing left to do means the current tag schema is fully applied.
        let _ = igdb::clear_refresh_marker(&state.store);
        return Ok(state.igdb_job.snapshot());
    }

    if !state.igdb_job.try_start(pending.len()) {
        return Ok(state.igdb_job.snapshot());
    }

    let store = state.store.clone();
    let job = state.igdb_job.clone();
    tauri::async_runtime::spawn(async move {
        let mut consecutive_failures = 0;

        for title in pending {
            let entry = match igdb::lookup_game(&client_id, &client_secret, &title).await {
                Ok(Some(entry)) => {
                    consecutive_failures = 0;
                    entry
                }
                Ok(None) => {
                    consecutive_failures = 0;
                    MetadataEntry {
                        not_found: true,
                        fetched_at: igdb::now_ms(),
                        ..Default::default()
                    }
                }
                // Bad credentials will fail identically for every remaining title, so
                // stop rather than burn hundreds of requests and get the IP throttled.
                Err(igdb::LookupError::Auth(message)) => {
                    job.finish(&app, Some(message));
                    return;
                }
                // One title failing is not a reason to abandon the other hundreds — the
                // old behaviour discarded the whole pass on the first transient error.
                Err(err) => {
                    consecutive_failures += 1;
                    log::warn!("IGDB lookup failed for {title}: {err}");
                    if consecutive_failures >= 10 {
                        job.finish(&app, Some(err.to_string()));
                        return;
                    }
                    job.advance(&app, 1);
                    tokio::time::sleep(err.backoff()).await;
                    continue;
                }
            };

            if let Err(err) = store.save_metadata(&slugify(&title), &entry) {
                log::error!("Could not cache metadata for {title}: {err}");
            }
            job.advance(&app, 1);

            // IGDB's free tier allows 4 requests/second; stay comfortably under it.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        // A clean pass means every row now matches the current tag schema.
        if let Err(err) = igdb::finish_refresh(&store) {
            log::warn!("Could not clear the IGDB refresh marker: {err}");
        }
        job.finish(&app, None);
    });

    Ok(state.igdb_job.snapshot())
}

// ---------- steam tags ----------

/// Fetches Steam user tags for every game with a known appid.
///
/// Batched 100 at a time, so the whole library is a handful of requests and a couple of
/// seconds — unlike the IGDB pass, which is one request per game.
#[tauri::command]
pub fn enrich_steam_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
) -> CmdResult<EnrichmentJob> {
    if state.steam_job.is_running() {
        return Ok(state.steam_job.snapshot());
    }

    let force = force.unwrap_or(false);
    let now = igdb::now_ms();
    let stamps = state.store.steam_metadata_stamps().map_err(to_err)?;

    // slug -> appid, so the result can be stored under the same key everything else uses.
    let targets: Vec<(String, String)> = steam_appid_targets(&state.store)?
        .into_iter()
        .filter(|(slug, _)| metadata::needs_steam(stamps.get(slug), now, force))
        .collect();

    if targets.is_empty() {
        return Ok(state.steam_job.snapshot());
    }
    if !state.steam_job.try_start(targets.len()) {
        return Ok(state.steam_job.snapshot());
    }

    let store = state.store.clone();
    let job = state.steam_job.clone();
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();

        let vocabulary = match steam_tags::fetch_tag_vocabulary(&client).await {
            Ok(vocabulary) => {
                log::info!("Steam tag vocabulary: {} names", vocabulary.len());
                vocabulary
            }
            // Without the id->name map every tag would be a bare number, so there is
            // nothing useful to do with the batches.
            Err(err) => {
                log::error!("Could not load the Steam tag vocabulary: {err}");
                job.finish(&app, Some(format!("Steam tag list unavailable: {err}")));
                return;
            }
        };

        let mut consecutive_failures = 0;
        let mut matched = 0usize;

        for chunk in targets.chunks(steam_tags::BATCH_SIZE) {
            let appids: Vec<String> = chunk.iter().map(|(_, appid)| appid.clone()).collect();

            match steam_tags::fetch_batch(&client, &vocabulary, &appids, now).await {
                Ok(found) => {
                    consecutive_failures = 0;
                    // Keyed by the appid Steam echoed back, never by position in the
                    // request — a shifted response would attach the wrong game's tags.
                    let rows: Vec<(String, ugly_core::metadata::SteamMetadata)> = chunk
                        .iter()
                        .filter_map(|(slug, appid)| {
                            found.get(appid).map(|entry| (slug.clone(), entry.clone()))
                        })
                        .collect();
                    matched += rows.iter().filter(|(_, e)| !e.not_found).count();

                    if let Err(err) = store.save_steam_metadata(&rows) {
                        log::error!("Could not save Steam tags: {err}");
                    }
                }
                // One bad batch must not lose the rest, but repeated failures are an
                // outage and burning the remaining requests helps nobody.
                Err(err) => {
                    consecutive_failures += 1;
                    log::warn!("Steam tag batch failed: {err}");
                    if consecutive_failures >= 3 {
                        job.finish(&app, Some(format!("Steam tag lookup failed: {err}")));
                        return;
                    }
                }
            }

            job.advance(&app, chunk.len());
        }

        log::info!("Steam tags: {matched} of {} games matched", targets.len());
        job.finish(&app, None);
    });

    Ok(state.steam_job.snapshot())
}

/// Every library game that has a Steam appid, keyed by slug.
///
/// Sources are inserted lowest-priority first so an owned copy wins — the same precedence
/// `library::merge` uses, which keeps the answer stable when a family-shared entry and an
/// Epic-resolved one disagree about which appid a title maps to.
fn steam_appid_targets(store: &Store) -> Result<HashMap<String, String>, String> {
    let mut targets: HashMap<String, String> = HashMap::new();

    for (slug, appid) in store.resolved_appids().map_err(to_err)? {
        if let Some(appid) = appid {
            targets.insert(slug, appid);
        }
    }
    for game in store
        .family_games()
        .map_err(to_err)?
        .into_iter()
        .chain(store.steam_games().map_err(to_err)?)
    {
        if let Some(appid) = models::steam_appid(&game.id) {
            targets.insert(slugify(&game.title), appid.to_string());
        }
    }

    Ok(targets)
}

// ---------- play status ----------

#[tauri::command]
pub fn get_statuses(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, StatusEntry>> {
    state.store.all_statuses().map_err(to_err)
}

/// `status` is `None` to clear (back to the implicit backlog). Unknown values are
/// rejected rather than stored, so the column can only ever hold known variants.
#[tauri::command]
pub fn set_game_status(
    state: State<'_, AppState>,
    slug: String,
    status: Option<String>,
) -> CmdResult<std::collections::HashMap<String, StatusEntry>> {
    if slug.trim().is_empty() {
        return Err("A game slug is required.".into());
    }

    let parsed = match status.as_deref() {
        None | Some("") => None,
        Some(value) => {
            Some(GameStatus::parse(value).ok_or_else(|| format!("Unknown status: {value}"))?)
        }
    };

    state
        .store
        .set_status(&slug, parsed, igdb::now_ms())
        .map_err(to_err)?;
    state.store.all_statuses().map_err(to_err)
}

// ---------- mcp server ----------

/// Where the bundled MCP server lives, plus a config snippet the user can paste straight
/// into an MCP client.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInfo {
    /// False when running from a dev build with no release binary staged yet, in which case
    /// the UI explains that instead of handing over a path that doesn't work.
    pub available: bool,
    pub path: String,
    /// Ready-to-paste JSON for `claude_desktop_config.json` and friends.
    pub config: String,
}

/// Resolves the sidecar next to the running executable, which is where Tauri puts external
/// binaries in both a dev run and an installed build. Computed rather than assumed, because
/// the installer lets the user choose where the app goes.
fn mcp_binary_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let beside = exe
        .parent()?
        .join(format!("ugly-mcp{}", std::env::consts::EXE_SUFFIX));
    beside.exists().then_some(beside)
}

#[tauri::command]
pub fn get_mcp_info() -> McpInfo {
    let Some(path) = mcp_binary_path() else {
        return McpInfo {
            available: false,
            path: String::new(),
            config: String::new(),
        };
    };

    let path = path.to_string_lossy().to_string();
    let config = serde_json::json!({
        "mcpServers": {
            "ugly": { "command": path, "args": [] }
        }
    });

    McpInfo {
        available: true,
        path,
        // Pretty-printed because it is shown in a code block and pasted into a config file.
        config: serde_json::to_string_pretty(&config).unwrap_or_default(),
    }
}

// ---------- installed games ----------

/// Scans Steam's .acf manifests and Epic's .item manifests and reports which of our
/// library entries are present on disk, keyed by our own game id.
#[tauri::command]
pub fn get_installed(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, ugly_core::installed::InstalledGame>> {
    // Merged rather than per-table, so a family-shared game that is installed is reported
    // too — the same list the UI renders, from the same helper the MCP server uses.
    let games = ugly_core::library::merge(
        state.store.steam_games().map_err(to_err)?,
        state.store.family_games().map_err(to_err)?,
        state.store.epic_library().map_err(to_err)?.games,
    );

    let found = ugly_core::library::installed_map(&games);
    let steam = found.values().filter(|g| g.platform == "steam").count();
    let epic = found.values().filter(|g| g.platform == "epic").count();
    log::info!(
        "Installed scan: {} matched ({steam} Steam, {epic} Epic) of {} library entries",
        found.len(),
        games.len()
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
