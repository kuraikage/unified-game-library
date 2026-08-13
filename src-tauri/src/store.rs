use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{EpicLibrary, Game, GameStatus, MetadataEntry, StatusEntry};

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).context("creating app data directory")?;
        let conn = Connection::open(dir.join("ugly.db")).context("opening database")?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS epic_games (
                 id           TEXT PRIMARY KEY,
                 title        TEXT NOT NULL,
                 cover_url    TEXT
             );

             CREATE TABLE IF NOT EXISTS steam_games (
                 id               TEXT PRIMARY KEY,
                 title            TEXT NOT NULL,
                 playtime_minutes INTEGER,
                 cover_url        TEXT
             );

             CREATE TABLE IF NOT EXISTS steam_family_games (
                 id        TEXT PRIMARY KEY,
                 title     TEXT NOT NULL,
                 cover_url TEXT
             );

             CREATE TABLE IF NOT EXISTS game_metadata (
                 slug         TEXT PRIMARY KEY,
                 matched_name TEXT,
                 genres       TEXT NOT NULL DEFAULT '[]',
                 tags         TEXT NOT NULL DEFAULT '[]',
                 cover_url    TEXT,
                 not_found    INTEGER NOT NULL DEFAULT 0,
                 fetched_at   INTEGER NOT NULL
             );

             -- Keyed by slug and intentionally NOT tied to the game tables: those are
             -- emptied and rebuilt on every import, and progress must survive that.
             -- Slug also means a game owned on two stores shares one status.
             CREATE TABLE IF NOT EXISTS game_status (
                 slug         TEXT PRIMARY KEY,
                 status       TEXT NOT NULL,
                 updated_at   INTEGER NOT NULL,
                 completed_at INTEGER
             );

             CREATE TABLE IF NOT EXISTS app_state (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .context("creating schema")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ---------- generic key/value ----------

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let value = conn
            .query_row("SELECT value FROM app_state WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        Ok(value)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------- steam ----------

    pub fn replace_steam_games(&self, games: &[Game]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM steam_games", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO steam_games (id, title, playtime_minutes, cover_url)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for game in games {
                stmt.execute(params![
                    game.id,
                    game.title,
                    game.playtime_minutes,
                    game.cover_url
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn steam_games(&self) -> Result<Vec<Game>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, playtime_minutes, cover_url FROM steam_games ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Game {
                    id: row.get(0)?,
                    platform: "steam".into(),
                    title: row.get(1)?,
                    playtime_minutes: row.get(2)?,
                    cover_url: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---------- steam family ----------

    pub fn replace_family_games(&self, games: &[Game], fetched_at: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM steam_family_games", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO steam_family_games (id, title, cover_url) VALUES (?1, ?2, ?3)",
            )?;
            for game in games {
                stmt.execute(params![game.id, game.title, game.cover_url])?;
            }
        }
        tx.execute(
            "INSERT INTO app_state (key, value) VALUES ('family_fetched_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![fetched_at.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn family_games(&self) -> Result<Vec<Game>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, cover_url FROM steam_family_games ORDER BY title COLLATE NOCASE",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Game {
                    id: row.get(0)?,
                    platform: "steam".into(),
                    title: row.get(1)?,
                    playtime_minutes: None,
                    cover_url: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---------- epic ----------

    pub fn replace_epic_games(&self, games: &[Game], imported_at: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM epic_games", [])?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO epic_games (id, title, cover_url) VALUES (?1, ?2, ?3)")?;
            for game in games {
                stmt.execute(params![game.id, game.title, game.cover_url])?;
            }
        }
        tx.execute(
            "INSERT INTO app_state (key, value) VALUES ('epic_imported_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![imported_at.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn epic_library(&self) -> Result<EpicLibrary> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, cover_url FROM epic_games ORDER BY title COLLATE NOCASE",
        )?;
        let games = stmt
            .query_map([], |row| {
                Ok(Game {
                    id: row.get(0)?,
                    platform: "epic".into(),
                    title: row.get(1)?,
                    playtime_minutes: None,
                    cover_url: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let imported_at = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'epic_imported_at'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|v| v.parse::<i64>().ok());

        Ok(EpicLibrary { games, imported_at })
    }

    // ---------- metadata ----------

    /// A single indexed upsert. The old JSON store rewrote the entire cache file for
    /// every game, which meant ~700 full file rewrites during one enrichment pass.
    pub fn save_metadata(&self, slug: &str, entry: &MetadataEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO game_metadata (slug, matched_name, genres, tags, cover_url, not_found, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(slug) DO UPDATE SET
                 matched_name = excluded.matched_name,
                 genres       = excluded.genres,
                 tags         = excluded.tags,
                 cover_url    = excluded.cover_url,
                 not_found    = excluded.not_found,
                 fetched_at   = excluded.fetched_at",
            params![
                slug,
                entry.matched_name,
                serde_json::to_string(&entry.genres)?,
                serde_json::to_string(&entry.tags)?,
                entry.cover_url,
                entry.not_found as i32,
                entry.fetched_at,
            ],
        )?;
        Ok(())
    }

    pub fn all_metadata(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT slug, matched_name, genres, tags, cover_url, not_found, fetched_at FROM game_metadata",
        )?;
        let mut map = serde_json::Map::new();
        let rows = stmt.query_map([], |row| {
            let slug: String = row.get(0)?;
            let genres: String = row.get(2)?;
            let tags: String = row.get(3)?;
            Ok((
                slug,
                MetadataEntry {
                    matched_name: row.get(1)?,
                    genres: serde_json::from_str(&genres).unwrap_or_default(),
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    cover_url: row.get(4)?,
                    not_found: row.get::<_, i32>(5)? != 0,
                    fetched_at: row.get(6)?,
                },
            ))
        })?;
        for row in rows {
            let (slug, entry) = row?;
            map.insert(slug, serde_json::to_value(entry)?);
        }
        Ok(map)
    }

    // ---------- play status ----------

    /// Passing `None` clears the status, returning the game to the implicit backlog.
    pub fn set_status(&self, slug: &str, status: Option<GameStatus>, now: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let Some(status) = status else {
            conn.execute("DELETE FROM game_status WHERE slug = ?1", [slug])?;
            return Ok(());
        };

        let completed_at = (status == GameStatus::Completed).then_some(now);
        conn.execute(
            "INSERT INTO game_status (slug, status, updated_at, completed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(slug) DO UPDATE SET
                 status     = excluded.status,
                 updated_at = excluded.updated_at,
                 -- Keep the original completion date if it's still marked completed.
                 completed_at = CASE
                     WHEN excluded.status = 'completed'
                     THEN COALESCE(game_status.completed_at, excluded.completed_at)
                     ELSE NULL
                 END",
            params![slug, status.as_str(), now, completed_at],
        )?;
        Ok(())
    }

    pub fn all_statuses(&self) -> Result<HashMap<String, StatusEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT slug, status, updated_at, completed_at FROM game_status")?;
        let mut out = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?;
        for row in rows {
            let (slug, status, updated_at, completed_at) = row?;
            // A row written by a newer version with an unknown status is skipped rather
            // than failing the whole read.
            if let Some(status) = GameStatus::parse(&status) {
                out.insert(
                    slug,
                    StatusEntry {
                        status,
                        updated_at,
                        completed_at,
                    },
                );
            }
        }
        Ok(out)
    }

    pub fn known_metadata_slugs(&self) -> Result<std::collections::HashSet<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT slug FROM game_metadata")?;
        let slugs = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
        Ok(slugs)
    }
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::models::GameStatus;

    fn temp_store() -> (Store, tempdir::TempDir) {
        let dir = tempdir::TempDir::new("ugly-test").unwrap();
        let store = Store::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn status_is_set_cleared_and_survives_library_rebuild() {
        let (store, _dir) = temp_store();

        store.set_status("hades", Some(GameStatus::Playing), 100).unwrap();
        assert_eq!(store.all_statuses().unwrap()["hades"].status, GameStatus::Playing);

        // Imports wipe and rebuild the game tables; progress must not go with them.
        store.replace_steam_games(&[]).unwrap();
        store.replace_epic_games(&[], 1).unwrap();
        assert!(store.all_statuses().unwrap().contains_key("hades"));

        store.set_status("hades", None, 300).unwrap();
        assert!(store.all_statuses().unwrap().is_empty());
    }

    #[test]
    fn completed_at_is_recorded_once_and_cleared_when_unset() {
        let (store, _dir) = temp_store();

        store.set_status("celeste", Some(GameStatus::Completed), 500).unwrap();
        assert_eq!(store.all_statuses().unwrap()["celeste"].completed_at, Some(500));

        // Re-marking completed keeps the original date rather than bumping it.
        store.set_status("celeste", Some(GameStatus::Completed), 900).unwrap();
        assert_eq!(store.all_statuses().unwrap()["celeste"].completed_at, Some(500));

        // Moving away from completed drops the date.
        store.set_status("celeste", Some(GameStatus::Playing), 950).unwrap();
        assert_eq!(store.all_statuses().unwrap()["celeste"].completed_at, None);
    }
}

/// Location of the pre-Tauri JSON store, so an existing install keeps its data.
pub fn legacy_store_dir() -> Option<PathBuf> {
    let dir = std::env::current_dir().ok()?;
    // In dev the CWD is src-tauri; in a packaged build this simply won't exist.
    for candidate in [
        dir.join("../server/src/store"),
        dir.join("server/src/store"),
    ] {
        if candidate.join("epic-library.json").exists()
            || candidate.join("game-metadata.json").exists()
        {
            return Some(candidate);
        }
    }
    None
}
