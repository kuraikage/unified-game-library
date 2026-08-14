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
///
/// Nothing here may use `skip_serializing_if` on a non-`Option` field: the schema derive
/// still marks such fields required, and a client that validates structured output against
/// it rejects the entire response. See the test at the bottom of this file.
#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGameView {
    pub title: String,
    pub slug: String,
    pub platform: String,
    pub status: Option<String>,
    pub playtime_hours: Option<f64>,
    pub installed: bool,
    pub shared: bool,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    /// Percentage of Steam reviews that are positive. Absent for games with no Steam page.
    pub review_percent: Option<i64>,
    /// Release year, from Steam.
    pub released: Option<i64>,
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
            review_percent: g.review_percent,
            // A year is all anyone reasons with here, and it costs a fraction of a
            // timestamp across hundreds of rows.
            released: g.released_at.map(release_year),
        }
    }
}

/// Unix seconds to a calendar year, without pulling in a date library for one field.
fn release_year(seconds: i64) -> i64 {
    // Days since the epoch, converted with the civil-from-days algorithm so leap years
    // and centuries are handled exactly.
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = yoe + era * 400;
    if mp >= 10 {
        year + 1
    } else {
        year
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllGamesResult {
    pub total: usize,
    pub games: Vec<CompactGame>,
}

/// Title, store and play state — nothing else. The whole library fits in one response at
/// this size, which is the point: it lets a caller reason over everything it owns instead
/// of trusting a tag filter to be complete.
#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompactGame {
    pub title: String,
    pub platform: String,
    /// Omitted for backlog games, which are most of them — `Option` fields aren't marked
    /// required by the schema derive, so skipping this one is safe where skipping a plain
    /// `bool` would not be. The compact-listing schema test guards that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub installed: bool,
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
    /// Games neither source knows anything about — no genres and no tags at all, so
    /// nothing to filter them by.
    pub missing_metadata: usize,
    /// Games with no genres. Genres come only from IGDB, so this is the count that has
    /// never been matched there, even if Steam supplied tags for it.
    pub missing_genres: usize,
    pub total_playtime_hours: f64,
    /// Most common genres across the library, largest first.
    pub top_genres: Vec<Facet>,
    /// Most common tags. These are the exact strings `list_games`'s `tag` filter matches,
    /// so pick from here rather than guessing at wording.
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

/// Reduces a label to just its letters and digits.
///
/// The two sources spell the same concept differently — Steam writes `Souls-like`, IGDB
/// writes `soulslike` — and a caller will type whichever comes to mind. Comparing on this
/// makes all three forms one tag. Without it, asking for `soulslike` returned 6 games out
/// of the 35 that qualify, which reads as a real answer rather than a near miss.
fn condense(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn contains_label(haystack: &[String], needle: &str) -> bool {
    let needle = condense(needle);
    haystack.iter().any(|v| condense(v) == needle)
}

/// Length of the longest shared opening run of two strings.
fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

/// Labels close enough to what the caller asked for to be worth suggesting.
///
/// Substring alone misses the common cases — `roguelite` for `Roguelike`, `soulsborne`
/// for `Souls-like`, or a plain typo — because a difference in the middle breaks it. A
/// shared opening run catches all three without needing edit distance.
fn nearby_labels<'a>(
    games: &'a [LibraryGame],
    pick: fn(&'a LibraryGame) -> &'a [String],
    needle: &str,
) -> Vec<String> {
    const MIN_SHARED_PREFIX: usize = 5;
    let needle = condense(needle);
    let mut found: Vec<String> = Vec::new();

    for game in games {
        for label in pick(game) {
            let c = condense(label);
            let close = c.contains(&needle)
                || needle.contains(&c)
                || common_prefix(&c, &needle) >= MIN_SHARED_PREFIX.min(needle.len());
            if close && !found.iter().any(|f| condense(f) == c) {
                found.push(label.clone());
            }
        }
    }
    found.sort();
    found.truncate(8);
    found
}

/// Applies every filter in [`ListGamesArgs`], combining them with AND.
///
/// Shared by `list_games` and `list_all_games` so the two can never disagree about what
/// matches — they differ only in how much of each game they return.
fn select<'a>(
    games: &'a [LibraryGame],
    args: &ListGamesArgs,
) -> Result<Vec<&'a LibraryGame>, ErrorData> {
    let status_filter = args.status.as_deref().map(parse_status_filter).transpose()?;
    let platform = args.platform.as_deref().map(str::to_lowercase);
    if let Some(p) = platform.as_deref() {
        if p != "steam" && p != "epic" {
            return Err(invalid(format!(
                "Unknown platform '{p}'. Use 'steam' or 'epic'."
            )));
        }
    }
    let installed_only = args.installed_only.unwrap_or(false);
    let search = args.search.as_deref().map(str::to_lowercase);

    // A tag or genre nothing in the library uses almost always means the caller guessed
    // at the wording. Saying so, with what does exist, is far more useful than an empty
    // result — which reads as "you own none of these" and gets reported as fact.
    for (label, value, pick) in [
        ("tag", args.tag.as_deref(), (|g: &LibraryGame| g.tags.as_slice()) as fn(&LibraryGame) -> &[String]),
        ("genre", args.genre.as_deref(), |g: &LibraryGame| g.genres.as_slice()),
    ] {
        let Some(value) = value else { continue };
        if games.iter().any(|g| contains_label(pick(g), value)) {
            continue;
        }
        let nearby = nearby_labels(games, pick, value);
        return Err(invalid(if nearby.is_empty() {
            format!(
                "No game is tagged '{value}', and nothing similar exists either. Tags are \
                 incomplete, so this is not evidence the user owns no such games — use \
                 list_all_games and judge by title instead."
            )
        } else {
            format!(
                "No {label} exactly '{value}'. Did you mean: {}? Note tags are incomplete \
                 either way — list_all_games and your own knowledge will find games these \
                 filters miss.",
                nearby.join(", ")
            )
        }));
    }

    Ok(games
        .iter()
        .filter(|g| {
            if let Some(p) = platform.as_deref() {
                if g.platform != p {
                    return false;
                }
            }
            // `Some(None)` is an explicit request for the backlog, so compare the inner
            // value rather than treating a missing status as "no filter".
            if let Some(want) = status_filter {
                if g.status != want {
                    return false;
                }
            }
            if installed_only && !g.installed {
                return false;
            }
            if let Some(genre) = args.genre.as_deref() {
                if !contains_label(&g.genres, genre) {
                    return false;
                }
            }
            if let Some(tag) = args.tag.as_deref() {
                if !contains_label(&g.tags, tag) {
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
        .collect())
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
            missing_genres: games.iter().filter(|g| g.genres.is_empty()).count(),
            total_playtime_hours: (total_minutes as f64 / 6.0).round() / 10.0,
            top_genres: top(genres, 25),
            top_tags: top(tags, 25),
        }))
    }

    #[tool(
        description = "Search and filter the library, returning full detail per game. Every \
                       filter is optional and they combine with AND. Reports the total number of \
                       matches alongside one page of results.\n\n\
                       Note the genre and tag filters only match what was recorded, and that \
                       data is incomplete — a game missing a tag may still fit. For \"what is \
                       this game like\" questions use list_all_games instead.",
        annotations(title = "List games", read_only_hint = true)
    )]
    fn list_games(
        &self,
        Parameters(args): Parameters<ListGamesArgs>,
    ) -> Result<Json<ListGamesResult>, ErrorData> {
        let games = self.library(args.installed_only.unwrap_or(false))?;
        let matches = select(&games, &args)?;

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
        description = "Every matching game as just a title, store, play state and whether it is \
                       installed — no genres or tags, and no limit. Unfiltered this is the whole \
                       library in one response.\n\n\
                       Use this for questions about what a game IS rather than how it is tagged \
                       — \"which of these are soulslikes?\", \"anything cosy here?\". Tags are \
                       incomplete, so filtering by them silently drops games that qualify; \
                       reading the titles and applying what you already know about these games \
                       does not.",
        annotations(title = "List every game", read_only_hint = true)
    )]
    fn list_all_games(
        &self,
        Parameters(args): Parameters<ListGamesArgs>,
    ) -> Result<Json<AllGamesResult>, ErrorData> {
        let games = self.library(args.installed_only.unwrap_or(false))?;
        let matches = select(&games, &args)?;

        Ok(Json(AllGamesResult {
            total: matches.len(),
            games: matches
                .into_iter()
                .map(|g| CompactGame {
                    title: g.title.clone(),
                    platform: g.platform.clone(),
                    status: g.status.map(|s| s.as_str().to_string()),
                    installed: g.installed,
                })
                .collect(),
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
                 the genres and tags actually present. Its topTags list is the exact vocabulary \
                 the tag filter matches, so choose from it rather than guessing at wording.\n\n\
                 TAGS ARE INCOMPLETE. They are Steam user tags where a game has a Steam page, \
                 topped up with IGDB keywords, capped per game, and absent entirely for a game \
                 neither source matched. A game not carrying a tag is NOT evidence it doesn't \
                 qualify — Dark Souls III carried no 'soulslike' tag at all until recently. \
                 So a tag filter returning few results means the tagging is thin, not that the \
                 library is.\n\n\
                 For questions about what a game IS rather than how it happens to be labelled — \
                 'what soulslike should I play', 'something cosy', 'a short one' — START with \
                 list_all_games and read EVERY title, judging each by what you know of that \
                 game. It deliberately returns no tags: the whole library arrives in one cheap \
                 response so you can classify it yourself rather than inherit the gaps in the \
                 tag data. Skimming for names you recognise finds a fraction of what qualifies \
                 and is the single most common way to get this wrong.\n\n\
                 Only afterwards, run list_games with a tag filter as a backstop, and add \
                 anything it turns up that you passed over — useful mainly for obscure titles \
                 you may not know. Answer from the union, never from the tag filter alone.\n\n\
                 A partial pass is never complete. If you have not read the full list, say \
                 'here are some' rather than implying you covered the library, and offer to go \
                 through it properly. Claiming completeness you have not earned is worse than \
                 a short answer.\n\n\
                 Games with no play status are the backlog, which is usually what to recommend \
                 from. 'installed' means it is ready to play right now on this PC. reviewPercent \
                 is the share of Steam reviews that are positive; genres come only from IGDB.\n\n\
                 Games cannot be launched or installed from here; tell the user to press play in \
                 the app.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts a serialized value carries every property its own generated schema marks as
    /// required. MCP clients validate structured output against that schema and reject the
    /// whole response when a field is missing, so a `skip_serializing_if` on a non-`Option`
    /// field silently breaks the tool for exactly the rows that trip the skip condition.
    /// A raw stdio probe does not validate, so this cannot be caught by hand-testing.
    fn assert_required_fields_present(schema: schemars::Schema, value: serde_json::Value) {
        let required = schema
            .as_value()
            .get("required")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(!required.is_empty(), "expected the schema to require something");

        let object = value.as_object().expect("expected a JSON object");
        for field in required {
            let name = field.as_str().unwrap();
            assert!(
                object.contains_key(name),
                "`{name}` is required by the schema but was omitted when serializing"
            );
        }
    }

    /// The worst case is a game with nothing set: no status, no playtime, no genres or tags,
    /// not installed and not shared.
    #[test]
    fn an_empty_game_row_still_matches_its_schema() {
        let bare = LibraryGame {
            id: "steam-1".into(),
            slug: "a-game".into(),
            title: "A Game".into(),
            platform: "steam".into(),
            shared: false,
            installed: false,
            playtime_minutes: None,
            status: None,
            completed_at: None,
            genres: Vec::new(),
            tags: Vec::new(),
            review_percent: None,
            released_at: None,
        };

        assert_required_fields_present(
            schemars::schema_for!(LibraryGameView),
            serde_json::to_value(LibraryGameView::from(&bare)).unwrap(),
        );
    }

    #[test]
    fn an_empty_compact_listing_still_matches_its_schema() {
        assert_required_fields_present(
            schemars::schema_for!(AllGamesResult),
            serde_json::to_value(AllGamesResult {
                total: 0,
                games: Vec::new(),
            })
            .unwrap(),
        );
        assert_required_fields_present(
            schemars::schema_for!(CompactGame),
            serde_json::to_value(CompactGame {
                title: "A Game".into(),
                platform: "steam".into(),
                status: None,
                installed: false,
            })
            .unwrap(),
        );
    }

    #[test]
    fn the_two_sources_spellings_of_one_tag_all_match() {
        // Steam writes "Souls-like", IGDB writes "soulslike", a caller may type either or
        // "souls like". Before this, asking for "soulslike" returned 6 of the 35 games
        // that qualify — an answer wrong enough to be reported as fact.
        let steam = vec!["Souls-like".to_string()];
        let igdb = vec!["soulslike".to_string()];
        for spelling in ["Souls-like", "soulslike", "souls like", "SOULS LIKE"] {
            assert!(contains_label(&steam, spelling), "steam vs {spelling}");
            assert!(contains_label(&igdb, spelling), "igdb vs {spelling}");
        }
        // Still distinguishes genuinely different tags.
        assert!(!contains_label(&steam, "roguelike"));
        assert!(!contains_label(&["Action RPG".to_string()], "Action"));
    }

    #[test]
    fn release_years_are_converted_exactly() {
        // Steam's own steam_release_date for Lies of P, and the epoch boundaries either
        // side of a new year in UTC.
        assert_eq!(release_year(1_695_048_142), 2023);
        assert_eq!(release_year(0), 1970);
        assert_eq!(release_year(1_704_067_199), 2023); // 2023-12-31T23:59:59Z
        assert_eq!(release_year(1_704_067_200), 2024); // 2024-01-01T00:00:00Z
        assert_eq!(release_year(951_782_400), 2000); // leap day 2000-02-29
    }

    #[test]
    fn an_empty_result_page_still_matches_its_schema() {
        assert_required_fields_present(
            schemars::schema_for!(ListGamesResult),
            serde_json::to_value(ListGamesResult {
                total: 0,
                returned: 0,
                offset: 0,
                games: Vec::new(),
            })
            .unwrap(),
        );
    }

    #[test]
    fn backlog_parses_to_a_cleared_status_and_junk_is_rejected() {
        assert_eq!(parse_status_filter("playing").unwrap(), Some(GameStatus::Playing));
        assert_eq!(parse_status_filter("  Completed ").unwrap(), Some(GameStatus::Completed));
        assert_eq!(parse_status_filter("backlog").unwrap(), None);
        assert!(parse_status_filter("banana").is_err());
    }
}
