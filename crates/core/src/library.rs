//! The unified library view: one row per game, merged across stores and joined with
//! cached IGDB metadata, play status and what's installed on this PC.
//!
//! The desktop app and the MCP server must agree on what "the library" is, so the merge
//! rules live here rather than in either front end.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;

use crate::installed::{self, InstalledGame};
use crate::models::{slugify, Game, GameStatus, MetadataEntry};
use crate::store::Store;

/// One row of the unified library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGame {
    pub id: String,
    /// Shared key with metadata and status, and with the JS side's `slugify`.
    pub slug: String,
    pub title: String,
    /// `steam` or `epic`.
    pub platform: String,
    /// Steam family-shared rather than owned outright.
    pub shared: bool,
    pub installed: bool,
    pub playtime_minutes: Option<i64>,
    pub status: Option<GameStatus>,
    pub completed_at: Option<i64>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

/// Merges the per-store tables into the single list the library shows.
///
/// A family-shared game you also own arrives from both Steam sources; the owned copy wins,
/// so it keeps its playtime and doesn't appear twice. Returned in the same order the UI
/// builds it: owned Steam, then shared, then Epic.
pub fn merge(steam: Vec<Game>, family: Vec<Game>, epic: Vec<Game>) -> Vec<(Game, bool)> {
    let owned: HashSet<&str> = steam.iter().map(|g| g.id.as_str()).collect();
    let shared: Vec<Game> = family
        .iter()
        .filter(|g| !owned.contains(g.id.as_str()))
        .cloned()
        .collect();

    let mut out: Vec<(Game, bool)> = Vec::with_capacity(steam.len() + shared.len() + epic.len());
    out.extend(steam.into_iter().map(|g| (g, false)));
    out.extend(shared.into_iter().map(|g| (g, true)));
    out.extend(epic.into_iter().map(|g| (g, false)));
    out
}

/// Scans the launchers for games matching this library, keyed by library game id.
pub fn installed_map(games: &[(Game, bool)]) -> HashMap<String, InstalledGame> {
    let ids: Vec<(String, String, String)> = games
        .iter()
        .map(|(g, _)| (g.id.clone(), g.platform.clone(), g.title.clone()))
        .collect();
    installed::detect(&ids)
}

/// Reads the whole library, joined and ready to filter.
///
/// `scan_installed` is skipped when the caller doesn't need it: the scan walks Steam's
/// library folders and every Epic manifest, which is far more expensive than the queries.
pub fn load(store: &Store, scan_installed: bool) -> Result<Vec<LibraryGame>> {
    let merged = merge(
        store.steam_games()?,
        store.family_games()?,
        store.epic_library()?.games,
    );

    let metadata = store.all_metadata()?;
    let statuses = store.all_statuses()?;
    let installed = if scan_installed {
        installed_map(&merged)
    } else {
        HashMap::new()
    };

    Ok(merged
        .into_iter()
        .map(|(game, shared)| {
            let slug = slugify(&game.title);
            let entry: Option<MetadataEntry> = metadata
                .get(&slug)
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let status = statuses.get(&slug);

            LibraryGame {
                installed: installed.contains_key(&game.id),
                id: game.id,
                slug,
                title: game.title,
                platform: game.platform,
                shared,
                playtime_minutes: game.playtime_minutes,
                status: status.map(|s| s.status),
                completed_at: status.and_then(|s| s.completed_at),
                genres: entry.as_ref().map(|e| e.genres.clone()).unwrap_or_default(),
                tags: entry.map(|e| e.tags).unwrap_or_default(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, platform: &str, title: &str) -> Game {
        Game {
            id: id.into(),
            platform: platform.into(),
            title: title.into(),
            playtime_minutes: None,
            cover_url: None,
        }
    }

    #[test]
    fn owned_steam_copy_wins_over_the_family_shared_one() {
        let merged = merge(
            vec![game("steam-1", "steam", "Hades")],
            vec![
                game("steam-1", "steam", "Hades"),
                game("steam-2", "steam", "Celeste"),
            ],
            vec![game("epic-a", "epic", "Control")],
        );

        let rows: Vec<(&str, bool)> = merged
            .iter()
            .map(|(g, shared)| (g.id.as_str(), *shared))
            .collect();
        assert_eq!(
            rows,
            vec![("steam-1", false), ("steam-2", true), ("epic-a", false)]
        );
    }
}
