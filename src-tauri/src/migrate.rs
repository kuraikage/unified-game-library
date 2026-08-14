use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use ugly_core::models::{Game, MetadataEntry};
use ugly_core::store::Store;

const MIGRATION_FLAG: &str = "legacy_json_migrated";

#[derive(Deserialize)]
struct LegacyGame {
    id: String,
    title: String,
    #[serde(default, rename = "playtimeMinutes")]
    playtime_minutes: Option<i64>,
    #[serde(default, rename = "coverUrl")]
    cover_url: Option<String>,
}

#[derive(Deserialize)]
struct LegacyEpic {
    #[serde(default)]
    games: Vec<LegacyGame>,
    #[serde(default, rename = "importedAt")]
    imported_at: Option<i64>,
}

#[derive(Deserialize)]
struct LegacySteamCache {
    #[serde(default)]
    games: Vec<LegacyGame>,
}

#[derive(Deserialize)]
struct LegacyMetadata {
    #[serde(default, rename = "matchedName")]
    matched_name: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "coverUrl")]
    cover_url: Option<String>,
    #[serde(default, rename = "notFound")]
    not_found: bool,
    #[serde(default, rename = "fetchedAt")]
    fetched_at: i64,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Imports the pre-Tauri JSON store once, so upgrading doesn't force a re-import of the
/// Epic library or a re-fetch of every IGDB lookup. Runs at most once per install.
pub fn migrate_legacy_json(store: &Store, legacy_dir: &Path) -> Result<Option<String>> {
    if store.get_state(MIGRATION_FLAG)?.is_some() {
        return Ok(None);
    }

    let mut summary = Vec::new();

    if let Some(epic) = read_json::<LegacyEpic>(&legacy_dir.join("epic-library.json")) {
        if !epic.games.is_empty() {
            let games: Vec<Game> = epic
                .games
                .into_iter()
                .map(|g| Game {
                    id: g.id,
                    platform: "epic".into(),
                    title: g.title,
                    playtime_minutes: None,
                    cover_url: g.cover_url,
                })
                .collect();
            let count = games.len();
            store.replace_epic_games(&games, epic.imported_at.unwrap_or(0))?;
            summary.push(format!("{count} Epic games"));
        }
    }

    if let Some(steam) = read_json::<LegacySteamCache>(&legacy_dir.join("steam-cache.json")) {
        if !steam.games.is_empty() {
            let games: Vec<Game> = steam
                .games
                .into_iter()
                .map(|g| Game {
                    id: g.id,
                    platform: "steam".into(),
                    title: g.title,
                    playtime_minutes: g.playtime_minutes,
                    cover_url: g.cover_url,
                })
                .collect();
            let count = games.len();
            store.replace_steam_games(&games)?;
            summary.push(format!("{count} Steam games"));
        }
    }

    if let Some(metadata) =
        read_json::<std::collections::HashMap<String, LegacyMetadata>>(&legacy_dir.join("game-metadata.json"))
    {
        let count = metadata.len();
        for (slug, entry) in metadata {
            store.save_metadata(
                &slug,
                &MetadataEntry {
                    matched_name: entry.matched_name,
                    genres: entry.genres,
                    tags: entry.tags,
                    cover_url: entry.cover_url,
                    not_found: entry.not_found,
                    fetched_at: entry.fetched_at,
                },
            )?;
        }
        if count > 0 {
            summary.push(format!("{count} cached IGDB lookups"));
        }
    }

    store.set_state(MIGRATION_FLAG, "1")?;

    Ok(if summary.is_empty() {
        None
    } else {
        Some(summary.join(", "))
    })
}

/// Steam credentials used to live in a plaintext config.json. Returns them so they can be
/// moved into the OS keychain, then the caller deletes the file.
#[derive(Deserialize)]
pub struct LegacyConfig {
    #[serde(default, rename = "steamApiKey")]
    pub steam_api_key: Option<String>,
    #[serde(default, rename = "steamId")]
    pub steam_id: Option<String>,
    #[serde(default, rename = "igdbClientId")]
    pub igdb_client_id: Option<String>,
    #[serde(default, rename = "igdbClientSecret")]
    pub igdb_client_secret: Option<String>,
}

pub fn read_legacy_config(legacy_dir: &Path) -> Option<LegacyConfig> {
    read_json::<LegacyConfig>(&legacy_dir.join("config.json"))
}
