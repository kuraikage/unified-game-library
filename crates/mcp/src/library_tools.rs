//! The tools exposed over MCP.
//!
//! Deliberately narrow: read the library, and move a game between play states. There is no
//! tool to launch or install a game — starting programs is not something an assistant
//! should be able to do on the user's behalf without them clicking it in the app.

use std::sync::Mutex;

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};

use ugly_core::library::{self, LibraryGame};
use ugly_core::models::{slugify, GameStatus};
use ugly_core::store::Store;

/// Keeps a single response from swamping the model's context. Callers page with `offset`.
const MAX_LIMIT: usize = 200;
const DEFAULT_LIMIT: usize = 50;

pub struct LibraryTools {
    store: Store,
    /// Scanning the launchers hits the filesystem, so the result is reused for the life of
    /// the process. Installs rarely change during one conversation.
    installed_cache: Mutex<Option<Vec<LibraryGame>>>,
}

// ---------- tool arguments ----------

// `deny_unknown_fields` on every argument struct: without it a caller that misspells a
// filter gets a silently *unfiltered* answer, which reads as a confident wrong result
// rather than an error.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListGamesArgs {
    #[schemars(description = "Match against title, genres and tags. Case-insensitive.")]
    pub search: Option<String>,
    #[schemars(description = "Limit to one store: 'steam' or 'epic'.")]
    pub platform: Option<String>,
    #[schemars(
        description = "Play state: 'playing', 'completed', 'dropped', or 'backlog' for games with no state set."
    )]
    pub status: Option<String>,
    #[schemars(description = "Only games with a matching genre, e.g. 'Role-playing (RPG)'.")]
    pub genre: Option<String>,
    #[schemars(description = "Only games carrying this tag, e.g. 'Roguelike'.")]
    pub tag: Option<String>,
    #[schemars(description = "Only games currently installed on this PC. Slower: scans the launchers.")]
    pub installed_only: Option<bool>,
    #[schemars(description = "Max games to return. Defaults to 50, capped at 200.")]
    pub limit: Option<usize>,
    #[schemars(description = "Skip this many matches, for paging through a large result.")]
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetGameArgs {
    #[schemars(description = "Game title or slug. Matching is case- and punctuation-insensitive.")]
    pub title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetStatusArgs {
    #[schemars(description = "Game title or slug, as returned by list_games.")]
    pub title: String,
    #[schemars(
        description = "New play state: 'playing', 'completed', 'dropped', or 'backlog' to clear it."
    )]
    pub status: String,
}

// ---------- tool results ----------

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListGamesResult {
    /// Total matches before `limit` and `offset` were applied.
    pub total: usize,
    pub returned: usize,
    pub offset: usize,
    pub games: Vec<LibraryGameView>,
}

/// The wire shape of a game. Mirrors [`LibraryGame`] minus the fields an assistant has no
/// use for, because every field is repeated for hundreds of rows.
#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGameView {
    pub title: String,
    pub slug: String,
    pub platform: String,
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime_hours: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub installed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shared: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl From<&LibraryGame> for LibraryGameView {
    fn from(g: &LibraryGame) -> Self {
        Self {
            title: g.title.clone(),
            slug: g.slug.clone(),
            platform: g.platform.clone(),
            status: g.status.map(|s| s.as_str().to_string()),
            // Minutes are Steam's unit but hours are what anyone reasons in.
            playtime_hours: g
                .playtime_minutes
                .filter(|m| *m > 0)
                .map(|m| (m as f64 / 6.0).round() / 10.0),
            installed: g.installed,
            shared: g.shared,
            genres: g.genres.clone(),
            tags: g.tags.clone(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatsResult {
    pub total_games: usize,
    pub steam: usize,
    pub epic: usize,
    pub family_shared: usize,
    pub installed: usize,
    pub playing: usize,
    pub completed: usize,
    pub dropped: usize,
    /// Everything with no play state set — the implicit backlog.
    pub backlog: usize,
    /// Games with no cached IGDB lookup, so no genres or tags to filter on.
    pub missing_metadata: usize,
    pub total_playtime_hours: f64,
    /// Most common genres across the library, largest first.
    pub top_genres: Vec<Facet>,
    pub top_tags: Vec<Facet>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Facet {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusResult {
    pub title: String,
    pub slug: String,
    pub status: Option<String>,
    pub message: String,
}

// ---------- helpers ----------

fn internal(err: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}

fn invalid(err: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(err.to_string(), None)
}

/// `backlog` is the absence of a status rather than a stored value, so it round-trips as
/// `None` in both directions.
fn parse_status_filter(value: &str) -> Result<Option<GameStatus>, ErrorData> {
    let value = value.trim().to_lowercase();
    if matches!(value.as_str(), "backlog" | "none" | "unset") {
        return Ok(None);
    }
    GameStatus::parse(&value).map(Some).ok_or_else(|| {
        invalid(format!(
            "Unknown status '{value}'. Use playing, completed, dropped or backlog."
        ))
    })
}

fn contains_ignore_case(haystack: &[String], needle: &str) -> bool {
    haystack.iter().any(|v| v.eq_ignore_ascii_case(needle))
}

#[tool_router]
impl LibraryTools {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            installed_cache: Mutex::new(None),
        }
    }

    /// Loads the library, scanning for installed games only when a caller needs it.
    fn library(&self, need_installed: bool) -> Result<Vec<LibraryGame>, ErrorData> {
        if !need_installed {
            return library::load(&self.store, false).map_err(internal);
        }

        let mut cache = self.installed_cache.lock().unwrap();
        if let Some(games) = cache.as_ref() {
            return Ok(games.clone());
        }
        let games = library::load(&self.store, true).map_err(internal)?;
        *cache = Some(games.clone());
        Ok(games)
    }

    /// Status writes go straight to the database the app reads, so any cached copy of the
    /// library is stale the moment one lands.
    fn invalidate_cache(&self) {
        *self.installed_cache.lock().unwrap() = None;
    }

    #[tool(
        description = "Counts and totals across the whole library: games per store, play states, \
                       install count, total playtime and the most common genres and tags. \
                       Start here to get your bearings before listing games.",
        annotations(title = "Library overview", read_only_hint = true)
    )]
    fn get_library_stats(&self) -> Result<Json<StatsResult>, ErrorData> {
        let games = self.library(true)?;

        let mut genres: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut tags: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for game in &games {
            for g in &game.genres {
                *genres.entry(g.as_str()).or_default() += 1;
            }
            for t in &game.tags {
                *tags.entry(t.as_str()).or_default() += 1;
            }
        }

        let top = |map: std::collections::HashMap<&str, usize>, n: usize| {
            let mut items: Vec<Facet> = map
                .into_iter()
                .map(|(name, count)| Facet {
                    name: name.to_string(),
                    count,
                })
                .collect();
            // Name breaks ties so the ordering is stable between calls.
            items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
            items.truncate(n);
            items
        };

        let count_status = |want: GameStatus| {
            games
                .iter()
                .filter(|g| g.status == Some(want))
                .count()
        };
        let total_minutes: i64 = games.iter().filter_map(|g| g.playtime_minutes).sum();

        Ok(Json(StatsResult {
            total_games: games.len(),
            steam: games.iter().filter(|g| g.platform == "steam").count(),
            epic: games.iter().filter(|g| g.platform == "epic").count(),
            family_shared: games.iter().filter(|g| g.shared).count(),
            installed: games.iter().filter(|g| g.installed).count(),
            playing: count_status(GameStatus::Playing),
            completed: count_status(GameStatus::Completed),
            dropped: count_status(GameStatus::Dropped),
            backlog: games.iter().filter(|g| g.status.is_none()).count(),
            missing_metadata: games
                .iter()
                .filter(|g| g.genres.is_empty() && g.tags.is_empty())
                .count(),
            total_playtime_hours: (total_minutes as f64 / 6.0).round() / 10.0,
            top_genres: top(genres, 25),
            top_tags: top(tags, 25),
        }))
    }

    #[tool(
        description = "Search and filter the library. Every filter is optional and they combine \
                       with AND. Returns the total number of matches alongside one page of \
                       results, so a broad search reports its size without dumping every row.",
        annotations(title = "List games", read_only_hint = true)
    )]
    fn list_games(
        &self,
        Parameters(args): Parameters<ListGamesArgs>,
    ) -> Result<Json<ListGamesResult>, ErrorData> {
        let installed_only = args.installed_only.unwrap_or(false);
        let games = self.library(installed_only)?;

        let status_filter = args.status.as_deref().map(parse_status_filter).transpose()?;
        let platform = args.platform.as_deref().map(str::to_lowercase);
        if let Some(p) = platform.as_deref() {
            if p != "steam" && p != "epic" {
                return Err(invalid(format!(
                    "Unknown platform '{p}'. Use 'steam' or 'epic'."
                )));
            }
        }
        let search = args.search.as_deref().map(str::to_lowercase);

        let matches: Vec<&LibraryGame> = games
            .iter()
            .filter(|g| {
                if let Some(p) = platform.as_deref() {
                    if g.platform != p {
                        return false;
                    }
                }
                // `Some(None)` is an explicit request for the backlog, so compare the
                // inner value rather than treating a missing status as "no filter".
                if let Some(want) = status_filter {
                    if g.status != want {
                        return false;
                    }
                }
                if installed_only && !g.installed {
                    return false;
                }
                if let Some(genre) = args.genre.as_deref() {
                    if !contains_ignore_case(&g.genres, genre) {
                        return false;
                    }
                }
                if let Some(tag) = args.tag.as_deref() {
                    if !contains_ignore_case(&g.tags, tag) {
                        return false;
                    }
                }
                if let Some(term) = search.as_deref() {
                    let hit = g.title.to_lowercase().contains(term)
                        || g.genres.iter().any(|v| v.to_lowercase().contains(term))
                        || g.tags.iter().any(|v| v.to_lowercase().contains(term));
                    if !hit {
                        return false;
                    }
                }
                true
            })
            .collect();

        let offset = args.offset.unwrap_or(0);
        let limit = args.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let page: Vec<LibraryGameView> = matches
            .iter()
            .skip(offset)
            .take(limit)
            .map(|g| LibraryGameView::from(*g))
            .collect();

        Ok(Json(ListGamesResult {
            total: matches.len(),
            returned: page.len(),
            offset,
            games: page,
        }))
    }

    #[tool(
        description = "Full detail for one game, looked up by title or slug.",
        annotations(title = "Get game", read_only_hint = true)
    )]
    fn get_game(
        &self,
        Parameters(args): Parameters<GetGameArgs>,
    ) -> Result<Json<LibraryGameView>, ErrorData> {
        let games = self.library(true)?;
        let slug = slugify(&args.title);
        let found = games
            .iter()
            .find(|g| g.slug == slug)
            .ok_or_else(|| invalid(format!("No game in the library matches '{}'.", args.title)))?;
        Ok(Json(LibraryGameView::from(found)))
    }

    #[tool(
        description = "Mark a game as playing, completed or dropped, or move it back to the \
                       backlog. The change appears in the UGLy app immediately.",
        annotations(title = "Set play status", read_only_hint = false, idempotent_hint = true)
    )]
    fn set_game_status(
        &self,
        Parameters(args): Parameters<SetStatusArgs>,
    ) -> Result<Json<SetStatusResult>, ErrorData> {
        let status = parse_status_filter(&args.status)?;

        // Resolve against the real library so a typo doesn't silently write a status for a
        // game that isn't there — the row is keyed by slug and would never be seen again.
        let games = self.library(false)?;
        let slug = slugify(&args.title);
        let found = games
            .iter()
            .find(|g| g.slug == slug)
            .ok_or_else(|| invalid(format!("No game in the library matches '{}'.", args.title)))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.store
            .set_status(&found.slug, status, now)
            .map_err(internal)?;
        self.invalidate_cache();

        let message = match status {
            Some(s) => format!("Marked {} as {}.", found.title, s.as_str()),
            None => format!("Moved {} back to the backlog.", found.title),
        };
        Ok(Json(SetStatusResult {
            title: found.title.clone(),
            slug: found.slug.clone(),
            status: status.map(|s| s.as_str().to_string()),
            message,
        }))
    }
}

#[tool_handler]
impl ServerHandler for LibraryTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                rmcp::model::Implementation::new("ugly", env!("CARGO_PKG_VERSION"))
                    .with_title("UGLy — Unified Game Library"),
            )
            .with_instructions(
                "Read-access to the user's own Steam and Epic game library, as collected by the \
                 UGLy desktop app, plus the ability to change a game's play status.\n\n\
                 Call get_library_stats first: it reports the size and shape of the library and \
                 the genres and tags actually present, which are the values list_games filters \
                 on. Use list_games for everything else — it returns a total match count, so \
                 prefer narrowing filters over paging through hundreds of rows.\n\n\
                 Games with no play status are the backlog, which is usually what to recommend \
                 from. 'installed' means it is ready to play right now on this PC. Genres and \
                 tags come from IGDB and are missing for games that were never enriched.\n\n\
                 Games cannot be launched or installed from here; tell the user to press play in \
                 the app.",
            )
    }
}
