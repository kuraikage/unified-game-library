use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::metadata::{self, MergedMetadata, MetadataStamp, SteamMetadata};
use crate::models::{EpicLibrary, Game, GameStatus, MetadataEntry, StatusEntry};

pub struct Store {
    conn: Mutex<Connection>,
}

/// Schema changes applied in order, tracked by SQLite's `user_version`.
///
/// Append only — never edit or reorder an entry, because a database that has already
/// applied one will never run it again. The baseline tables in `open` stay as
/// `CREATE TABLE IF NOT EXISTS` so a fresh database and an upgraded one converge.
const MIGRATIONS: &[&str] = &[
    // 1 — Steam user tags as a second metadata source alongside IGDB.
    //
    // Both tables are keyed by slug rather than appid so a game owned on Epic that also
    // exists on Steam shares the row, the same way game_status and game_metadata do.
    "CREATE TABLE IF NOT EXISTS steam_metadata (
         slug              TEXT PRIMARY KEY,
         appid             TEXT NOT NULL,
         -- JSON array of tag names, most-voted first. Weights aren't kept: the order
         -- already encodes them and nothing reads the numbers.
         tags              TEXT NOT NULL DEFAULT '[]',
         short_description TEXT,
         developer         TEXT,
         publisher         TEXT,
         -- Unix SECONDS — Steam's unit. Deliberately named apart from fetched_at, which
         -- is milliseconds like game_metadata, so the two can't be confused.
         released_at       INTEGER,
         review_count      INTEGER,
         review_percent    INTEGER,
         review_label      TEXT,
         -- Set only when Steam answered for this appid and said it has no usable store
         -- entry. Never set from a failed request, or one outage blanks a whole batch.
         not_found         INTEGER NOT NULL DEFAULT 0,
         fetched_at        INTEGER NOT NULL
     );

     -- Epic games have no appid, so their titles are searched against the Steam store
     -- once and the answer cached. A NULL appid means 'searched, genuinely not on Steam'
     -- and must not be retried.
     CREATE TABLE IF NOT EXISTS steam_appid_lookup (
         slug        TEXT PRIMARY KEY,
         appid       TEXT,
         resolved_at INTEGER NOT NULL
     );",
];

/// Brings an existing database up to the current schema.
fn migrate(conn: &mut Connection) -> Result<()> {
    let applied = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as usize;

    // A database written by a newer build knows more than this binary does. Leave it
    // alone rather than failing — an older MCP sidecar still has to be able to read the
    // library after the desktop app upgrades.
    if applied >= MIGRATIONS.len() {
        return Ok(());
    }

    let tx = conn.transaction()?;
    for (index, statement) in MIGRATIONS.iter().enumerate().skip(applied) {
        tx.execute_batch(statement)
            .with_context(|| format!("applying migration {}", index + 1))?;
    }
    // PRAGMA values can't be bound as parameters.
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
    Ok(())
}

impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).context("creating app data directory")?;
        let mut conn = Connection::open(dir.join("ugly.db")).context("opening database")?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             -- The MCP server opens this same database while the app is running, so a
             -- write can find the other process mid-transaction. Wait rather than fail.
             PRAGMA busy_timeout = 5000;

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

        migrate(&mut conn).context("migrating schema")?;

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

    /// Everything both sources know, combined. This is what the webview and the MCP
    /// server read; neither should query a single source directly.
    pub fn all_metadata(&self) -> Result<HashMap<String, MergedMetadata>> {
        // Two full scans of a few hundred rows each, merged in memory. Not a JOIN: a
        // LEFT JOIN would drop games only Steam knows about, and a FULL OUTER JOIN needs
        // SQLite 3.39 to buy nothing at this size.
        Ok(metadata::merge_metadata(
            self.igdb_metadata()?,
            self.all_steam_metadata()?,
        ))
    }

    /// The raw IGDB table. Used by the enrichment job, which must reason about what
    /// *IGDB* has rather than what the merged view shows.
    fn igdb_metadata(&self) -> Result<HashMap<String, MetadataEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT slug, matched_name, genres, tags, cover_url, not_found, fetched_at FROM game_metadata",
        )?;
        let mut map = HashMap::new();
        // Every column is read leniently. A single malformed row must not fail the whole
        // read: this powers the entire library view, so one bad value would otherwise
        // leave the user staring at an empty grid with no way to recover.
        let rows = stmt.query_map([], |row| {
            let slug: String = row.get(0)?;
            let genres: Option<String> = row.get(2).unwrap_or_default();
            let tags: Option<String> = row.get(3).unwrap_or_default();
            Ok((
                slug,
                MetadataEntry {
                    matched_name: row.get(1).unwrap_or_default(),
                    genres: genres
                        .and_then(|g| serde_json::from_str(&g).ok())
                        .unwrap_or_default(),
                    tags: tags
                        .and_then(|t| serde_json::from_str(&t).ok())
                        .unwrap_or_default(),
                    cover_url: row.get(4).unwrap_or_default(),
                    not_found: row.get::<_, i32>(5).unwrap_or(0) != 0,
                    fetched_at: row.get(6).unwrap_or(0),
                },
            ))
        })?;
        for row in rows {
            let (slug, entry) = row?;
            map.insert(slug, entry);
        }
        Ok(map)
    }

    // ---------- steam store metadata ----------

    pub fn all_steam_metadata(&self) -> Result<HashMap<String, SteamMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT slug, appid, tags, short_description, developer, publisher,
                    released_at, review_count, review_percent, review_label,
                    not_found, fetched_at
             FROM steam_metadata",
        )?;
        // Read leniently, for the same reason as the IGDB table: one bad row must not
        // blank the whole library.
        let rows = stmt.query_map([], |row| {
            let slug: String = row.get(0)?;
            let tags: Option<String> = row.get(2).unwrap_or_default();
            Ok((
                slug,
                SteamMetadata {
                    appid: row.get(1).unwrap_or_default(),
                    tags: tags
                        .and_then(|t| serde_json::from_str(&t).ok())
                        .unwrap_or_default(),
                    short_description: row.get(3).unwrap_or_default(),
                    developer: row.get(4).unwrap_or_default(),
                    publisher: row.get(5).unwrap_or_default(),
                    released_at: row.get(6).unwrap_or_default(),
                    review_count: row.get(7).unwrap_or_default(),
                    review_percent: row.get(8).unwrap_or_default(),
                    review_label: row.get(9).unwrap_or_default(),
                    not_found: row.get::<_, i32>(10).unwrap_or(0) != 0,
                    fetched_at: row.get(11).unwrap_or(0),
                },
            ))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (slug, entry) = row?;
            map.insert(slug, entry);
        }
        Ok(map)
    }

    /// Writes a whole batch in one transaction. Per-row commits would take and release
    /// the write lock hundreds of times against a database the MCP server also has open.
    pub fn save_steam_metadata(&self, rows: &[(String, SteamMetadata)]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO steam_metadata (slug, appid, tags, short_description, developer,
                     publisher, released_at, review_count, review_percent, review_label,
                     not_found, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(slug) DO UPDATE SET
                     appid = excluded.appid, tags = excluded.tags,
                     short_description = excluded.short_description,
                     developer = excluded.developer, publisher = excluded.publisher,
                     released_at = excluded.released_at,
                     review_count = excluded.review_count,
                     review_percent = excluded.review_percent,
                     review_label = excluded.review_label,
                     not_found = excluded.not_found, fetched_at = excluded.fetched_at",
            )?;
            for (slug, entry) in rows {
                stmt.execute(params![
                    slug,
                    entry.appid,
                    serde_json::to_string(&entry.tags)?,
                    entry.short_description,
                    entry.developer,
                    entry.publisher,
                    entry.released_at,
                    entry.review_count,
                    entry.review_percent,
                    entry.review_label,
                    entry.not_found as i32,
                    entry.fetched_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---------- fetch bookkeeping ----------

    pub fn igdb_metadata_stamps(&self) -> Result<HashMap<String, MetadataStamp>> {
        self.stamps("SELECT slug, fetched_at, not_found FROM game_metadata")
    }

    pub fn steam_metadata_stamps(&self) -> Result<HashMap<String, MetadataStamp>> {
        self.stamps("SELECT slug, fetched_at, not_found FROM steam_metadata")
    }

    fn stamps(&self, sql: &str) -> Result<HashMap<String, MetadataStamp>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                MetadataStamp {
                    fetched_at: row.get(1).unwrap_or(0),
                    not_found: row.get::<_, i32>(2).unwrap_or(0) != 0,
                },
            ))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (slug, stamp) = row?;
            map.insert(slug, stamp);
        }
        Ok(map)
    }

    // ---------- epic title -> steam appid ----------

    /// A present key means the title has been searched. A `None` value means it was
    /// searched and is genuinely not on Steam, so it must not be looked up again.
    pub fn resolved_appids(&self) -> Result<HashMap<String, Option<String>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT slug, appid FROM steam_appid_lookup")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (slug, appid) = row?;
            map.insert(slug, appid);
        }
        Ok(map)
    }

    pub fn save_appid_lookup(&self, rows: &[(String, Option<String>)], now: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO steam_appid_lookup (slug, appid, resolved_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(slug) DO UPDATE SET
                     appid = excluded.appid, resolved_at = excluded.resolved_at",
            )?;
            for (slug, appid) in rows {
                stmt.execute(params![slug, appid, now])?;
            }
        }
        tx.commit()?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::{migrate, Store, MIGRATIONS};
    use crate::models::GameStatus;
    use rusqlite::Connection;

    /// Builds a database shaped like one written before migrations existed: the baseline
    /// tables, real rows, and `user_version` still at 0.
    fn legacy_database(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE game_metadata (
                 slug TEXT PRIMARY KEY, matched_name TEXT,
                 genres TEXT NOT NULL DEFAULT '[]', tags TEXT NOT NULL DEFAULT '[]',
                 cover_url TEXT, not_found INTEGER NOT NULL DEFAULT 0,
                 fetched_at INTEGER NOT NULL);
             CREATE TABLE game_status (
                 slug TEXT PRIMARY KEY, status TEXT NOT NULL,
                 updated_at INTEGER NOT NULL, completed_at INTEGER);
             INSERT INTO game_metadata (slug, genres, tags, fetched_at)
                 VALUES ('hades', '[\"Indie\"]', '[\"roguelike\"]', 42);
             INSERT INTO game_status VALUES ('hades', 'playing', 99, NULL);",
        )
        .unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn migrating_an_existing_database_adds_tables_and_keeps_rows() {
        let dir = tempdir::TempDir::new("ugly-migrate").unwrap();
        legacy_database(&dir.path().join("ugly.db"));

        let store = Store::open(dir.path()).unwrap();

        // The pre-existing rows survive untouched.
        assert_eq!(store.all_statuses().unwrap()["hades"].status, GameStatus::Playing);
        assert!(store.all_metadata().unwrap().contains_key("hades"));

        let conn = store.conn.lock().unwrap();
        for table in ["steam_metadata", "steam_appid_lookup"] {
            let found: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(found, 1, "{table} should exist after migrating");
        }
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            MIGRATIONS.len() as i64
        );
    }

    #[test]
    fn a_database_from_a_newer_build_is_left_alone() {
        // An older MCP sidecar opening a database the desktop app has already upgraded
        // must not fail, and must not try to re-apply anything.
        let dir = tempdir::TempDir::new("ugly-newer").unwrap();
        let path = dir.path().join("ugly.db");
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 5)
            .unwrap();

        migrate(&mut conn).unwrap();

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            MIGRATIONS.len() as i64 + 5,
            "the newer version must be preserved"
        );
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let dir = tempdir::TempDir::new("ugly-twice").unwrap();
        legacy_database(&dir.path().join("ugly.db"));
        Store::open(dir.path()).unwrap();
        // Re-opening runs migrate again; CREATE TABLE IF NOT EXISTS plus the version
        // check must make that harmless.
        Store::open(dir.path()).unwrap();
    }

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
