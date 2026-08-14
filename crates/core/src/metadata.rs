//! Merging the two metadata sources, and deciding what still needs fetching.
//!
//! Deliberately pure: no database, no network, no clock. Everything here is a decision
//! rule, so it can be unit-tested directly rather than through a running app.
//!
//! Steam supplies user tags — a curated 446-name vocabulary, ranked by how many players
//! voted for each. IGDB supplies genres and cover art, plus its own free-text keywords.
//! Steam's tags are markedly better for "what does this game feel like": IGDB has no
//! `soulslike` keyword on Dark Souls III at all, while it is Steam's top-voted tag.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::{slugify, MetadataEntry};

/// How many Steam tags to keep per game. Steam returns them ranked by votes, and the
/// tail is noise ("Great Soundtrack" on a game with four votes).
pub const STEAM_TAG_CAP: usize = 10;

/// Ceiling on the merged list. Capped here at the merge rather than in any one view,
/// because `list_games` can return 200 rows and every tag is repeated per row.
pub const MERGED_TAG_CAP: usize = 15;

/// Re-fetch a game IGDB had no match for after this long — it may have been released,
/// or renamed, since the last attempt. Without it a `not_found` row is permanent.
pub const IGDB_NOT_FOUND_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Steam tags and review percentages drift, and a full re-pass is ~7 requests.
pub const STEAM_TTL_MS: i64 = 14 * 24 * 60 * 60 * 1000;

/// One game's Steam store data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamMetadata {
    pub appid: String,
    /// Tag names, most-voted first.
    pub tags: Vec<String>,
    pub short_description: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    /// Unix **seconds** — Steam's unit, unlike `fetched_at`.
    pub released_at: Option<i64>,
    pub review_count: Option<i64>,
    /// Percentage of reviews that are positive.
    pub review_percent: Option<i64>,
    /// Steam's own wording, e.g. "Very Positive".
    pub review_label: Option<String>,
    /// Steam answered for this appid and has no usable store entry for it.
    pub not_found: bool,
    /// Unix **milliseconds**, matching `MetadataEntry::fetched_at`.
    pub fetched_at: i64,
}

/// What one game looks like once both sources are combined. This is the shape the
/// webview and the MCP server both read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedMetadata {
    pub matched_name: Option<String>,
    /// IGDB only — Steam's endpoint returns no genre list.
    pub genres: Vec<String>,
    /// Steam tags first, then any IGDB tag not already present.
    pub tags: Vec<String>,
    pub cover_url: Option<String>,
    /// IGDB's "searched, no match".
    pub not_found: bool,
    /// IGDB's fetch time, 0 when IGDB has never run for this game.
    pub fetched_at: i64,
    /// Provenance. `igdb` is load-bearing: the webview decides what still needs
    /// enriching from it, and a merged row with Steam tags but no IGDB entry would
    /// otherwise look complete and never get genres or cover art.
    pub igdb: bool,
    pub steam: bool,
    pub short_description: Option<String>,
    pub review_percent: Option<i64>,
    pub review_count: Option<i64>,
    pub released_at: Option<i64>,
}

/// Just enough of a stored row to decide whether it needs fetching again.
#[derive(Debug, Clone, Copy)]
pub struct MetadataStamp {
    pub fetched_at: i64,
    pub not_found: bool,
}

/// Steam tags first, then IGDB tags that add something new.
///
/// Deduplicated on [`slugify`], which already collapses `Souls-like`, `Souls Like` and
/// `souls like` to one key — so the shared slug rule gives exact-match dedupe for free.
pub fn merge_tags(steam: &[String], igdb: &[String]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(MERGED_TAG_CAP);

    for tag in steam.iter().take(STEAM_TAG_CAP) {
        if out.len() >= MERGED_TAG_CAP {
            return out;
        }
        if seen.insert(slugify(tag)) {
            out.push(tag.clone());
        }
    }
    for tag in igdb {
        if out.len() >= MERGED_TAG_CAP {
            break;
        }
        if seen.insert(slugify(tag)) {
            out.push(tag.clone());
        }
    }
    out
}

/// Combines both source tables into the view every consumer reads.
///
/// Games present in only one source still appear — a Steam game IGDB has never matched,
/// and an Epic game with no Steam entry, are both legitimate.
pub fn merge_metadata(
    igdb: HashMap<String, MetadataEntry>,
    mut steam: HashMap<String, SteamMetadata>,
) -> HashMap<String, MergedMetadata> {
    let mut out: HashMap<String, MergedMetadata> = HashMap::with_capacity(igdb.len().max(steam.len()));

    for (slug, entry) in igdb {
        // Removing as we go leaves only the Steam-only slugs behind for the second pass.
        let steam_entry = steam.remove(&slug);
        out.insert(slug, combine(Some(entry), steam_entry));
    }
    for (slug, steam_entry) in steam {
        out.insert(slug, combine(None, Some(steam_entry)));
    }
    out
}

fn combine(igdb: Option<MetadataEntry>, steam: Option<SteamMetadata>) -> MergedMetadata {
    // A row that only records "Steam has nothing here" carries no tags to merge, so it
    // must not count as a Steam source — otherwise it looks like real data downstream.
    let steam = steam.filter(|s| !s.not_found);
    let steam_tags = steam.as_ref().map(|s| s.tags.as_slice()).unwrap_or(&[]);
    let igdb_tags = igdb.as_ref().map(|e| e.tags.as_slice()).unwrap_or(&[]);

    MergedMetadata {
        tags: merge_tags(steam_tags, igdb_tags),
        genres: igdb.as_ref().map(|e| e.genres.clone()).unwrap_or_default(),
        matched_name: igdb.as_ref().and_then(|e| e.matched_name.clone()),
        cover_url: igdb.as_ref().and_then(|e| e.cover_url.clone()),
        not_found: igdb.as_ref().map(|e| e.not_found).unwrap_or(false),
        fetched_at: igdb.as_ref().map(|e| e.fetched_at).unwrap_or(0),
        igdb: igdb.is_some(),
        steam: steam.is_some(),
        short_description: steam.as_ref().and_then(|s| s.short_description.clone()),
        review_percent: steam.as_ref().and_then(|s| s.review_percent),
        review_count: steam.as_ref().and_then(|s| s.review_count),
        released_at: steam.as_ref().and_then(|s| s.released_at),
    }
}

/// Whether IGDB should be asked about this game.
///
/// `refresh_from` is a timestamp marker: when the tag schema changes, everything fetched
/// before the marker is stale. Because each refreshed row's `fetched_at` moves past it,
/// an interrupted pass resumes where it stopped rather than starting over.
pub fn needs_igdb(stamp: Option<&MetadataStamp>, now: i64, refresh_from: i64, force: bool) -> bool {
    let Some(stamp) = stamp else {
        return true;
    };
    if force || stamp.fetched_at < refresh_from {
        return true;
    }
    // A game IGDB didn't know about may exist by now.
    stamp.not_found && now - stamp.fetched_at > IGDB_NOT_FOUND_TTL_MS
}

/// Whether Steam should be asked about this game. A whole re-pass is a handful of
/// batched requests, so this is far more willing to refresh than the IGDB rule.
pub fn needs_steam(stamp: Option<&MetadataStamp>, now: i64, force: bool) -> bool {
    let Some(stamp) = stamp else {
        return true;
    };
    force || now - stamp.fetched_at > STEAM_TTL_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn steam_tags_lead_and_igdb_fills_the_rest() {
        let merged = merge_tags(
            &names(&["Souls-like", "Dark Fantasy"]),
            &names(&["Action", "medieval"]),
        );
        assert_eq!(merged, names(&["Souls-like", "Dark Fantasy", "Action", "medieval"]));
    }

    #[test]
    fn the_same_tag_from_both_sources_appears_once_in_steams_wording() {
        // IGDB writes "soulslike", Steam writes "Souls-like"; slugify collapses both.
        let merged = merge_tags(&names(&["Souls-like"]), &names(&["souls like", "Souls Like"]));
        assert_eq!(merged, names(&["Souls-like"]));
    }

    #[test]
    fn caps_are_enforced_on_both_sides() {
        let steam: Vec<String> = (0..20).map(|i| format!("s{i}")).collect();
        let igdb: Vec<String> = (0..20).map(|i| format!("i{i}")).collect();
        let merged = merge_tags(&steam, &igdb);

        assert_eq!(merged.len(), MERGED_TAG_CAP);
        // Only STEAM_TAG_CAP Steam tags are taken, so IGDB still gets the remainder.
        assert_eq!(merged.iter().filter(|t| t.starts_with('s')).count(), STEAM_TAG_CAP);
        assert_eq!(
            merged.iter().filter(|t| t.starts_with('i')).count(),
            MERGED_TAG_CAP - STEAM_TAG_CAP
        );
    }

    #[test]
    fn a_game_only_steam_knows_about_still_appears_and_is_marked_unenriched() {
        let mut steam = HashMap::new();
        steam.insert(
            "lies-of-p".to_string(),
            SteamMetadata {
                appid: "1627720".into(),
                tags: names(&["Souls-like"]),
                review_percent: Some(91),
                ..Default::default()
            },
        );

        let merged = merge_metadata(HashMap::new(), steam);
        let entry = &merged["lies-of-p"];

        assert_eq!(entry.tags, names(&["Souls-like"]));
        assert!(entry.steam);
        // The flag the webview uses to decide what still needs IGDB. If this were true,
        // the game would silently never get genres or cover art.
        assert!(!entry.igdb, "a Steam-only row must not look IGDB-enriched");
        assert!(entry.genres.is_empty());
        assert_eq!(entry.review_percent, Some(91));
    }

    #[test]
    fn a_steam_not_found_row_contributes_nothing() {
        let mut steam = HashMap::new();
        steam.insert(
            "some-playtest".to_string(),
            SteamMetadata {
                appid: "1".into(),
                not_found: true,
                fetched_at: 500,
                ..Default::default()
            },
        );
        let mut igdb = HashMap::new();
        igdb.insert(
            "some-playtest".to_string(),
            MetadataEntry {
                tags: names(&["Action"]),
                ..Default::default()
            },
        );

        let merged = merge_metadata(igdb, steam);
        let entry = &merged["some-playtest"];
        assert!(!entry.steam, "not_found is an absence, not a source");
        assert_eq!(entry.tags, names(&["Action"]));
    }

    #[test]
    fn igdb_is_refetched_when_stale_forced_or_long_unmatched() {
        let now = 1_000_000_000;
        let fresh = MetadataStamp { fetched_at: now - 1000, not_found: false };

        assert!(needs_igdb(None, now, 0, false), "never fetched");
        assert!(!needs_igdb(Some(&fresh), now, 0, false), "already current");
        assert!(needs_igdb(Some(&fresh), now, 0, true), "forced");
        assert!(
            needs_igdb(Some(&fresh), now, now, false),
            "fetched before the schema-change marker"
        );

        let recent_miss = MetadataStamp { fetched_at: now - 1000, not_found: true };
        assert!(!needs_igdb(Some(&recent_miss), now, 0, false), "too soon to retry");

        let old_miss = MetadataStamp {
            fetched_at: now - IGDB_NOT_FOUND_TTL_MS - 1,
            not_found: true,
        };
        assert!(needs_igdb(Some(&old_miss), now, 0, false), "worth another try");
    }

    #[test]
    fn steam_is_refetched_once_the_ttl_lapses() {
        let now = 1_000_000_000;
        let fresh = MetadataStamp { fetched_at: now - 1000, not_found: false };
        let stale = MetadataStamp { fetched_at: now - STEAM_TTL_MS - 1, not_found: false };

        assert!(needs_steam(None, now, false));
        assert!(!needs_steam(Some(&fresh), now, false));
        assert!(needs_steam(Some(&fresh), now, true));
        assert!(needs_steam(Some(&stale), now, false));
    }
}
