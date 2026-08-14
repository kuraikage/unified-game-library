//! Steam user tags — the good tag data.
//!
//! Steam's players vote games into a curated ~450-name vocabulary, which is far better at
//! "what does this game feel like" than IGDB's free-text keywords: IGDB has no `soulslike`
//! keyword on Dark Souls III at all, while `Souls-like` is Steam's top-voted tag for it.
//!
//! Both endpoints are undocumented but need no API key, the same category as the
//! `IFamilyGroupsService` call the family import already relies on. If either changes
//! shape the tags degrade to IGDB's rather than breaking the app.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use ugly_core::metadata::SteamMetadata;

const ITEMS_URL: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
const TAG_LIST_URL: &str = "https://api.steampowered.com/IStoreService/GetTagList/v1/";

/// Appids per request. 120 works and returns in well under a second, but the payload rides
/// in a percent-encoded query string, so 100 keeps it around 2KB and clear of any proxy.
pub const BATCH_SIZE: usize = 100;

/// How many tags to keep per game. Steam ranks them by votes and the tail is noise.
const TAGS_PER_GAME: u32 = 20;

// ---------- response shapes ----------
//
// Every field is optional. serde_json is all-or-nothing, so one oddly shaped item in a
// batch of 100 would otherwise discard the other 99.

#[derive(Deserialize)]
struct ItemsEnvelope {
    #[serde(default)]
    response: ItemsResponse,
}

#[derive(Deserialize, Default)]
struct ItemsResponse {
    #[serde(default)]
    store_items: Vec<StoreItem>,
}

#[derive(Deserialize, Default)]
struct StoreItem {
    #[serde(default)]
    appid: Option<i64>,
    #[serde(default)]
    name: Option<String>,
    /// 1 means Steam has a usable store entry. Anything else (delisted, playtest, a
    /// non-existent appid) comes back as 15.
    #[serde(default)]
    success: Option<i64>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    tags: Vec<ItemTag>,
    #[serde(default)]
    basic_info: Option<BasicInfo>,
    #[serde(default)]
    release: Option<ReleaseInfo>,
    #[serde(default)]
    reviews: Option<ReviewInfo>,
}

#[derive(Deserialize, Default)]
struct ItemTag {
    #[serde(default)]
    tagid: Option<i64>,
}

#[derive(Deserialize, Default)]
struct BasicInfo {
    #[serde(default)]
    short_description: Option<String>,
    #[serde(default)]
    developers: Vec<NamedEntity>,
    #[serde(default)]
    publishers: Vec<NamedEntity>,
}

#[derive(Deserialize, Default)]
struct NamedEntity {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize, Default)]
struct ReleaseInfo {
    /// Unix seconds.
    #[serde(default)]
    steam_release_date: Option<i64>,
}

#[derive(Deserialize, Default)]
struct ReviewInfo {
    #[serde(default)]
    summary_filtered: Option<ReviewSummary>,
}

#[derive(Deserialize, Default)]
struct ReviewSummary {
    #[serde(default)]
    review_count: Option<i64>,
    #[serde(default)]
    percent_positive: Option<i64>,
    #[serde(default)]
    review_score_label: Option<String>,
}

#[derive(Deserialize)]
struct TagListEnvelope {
    #[serde(default)]
    response: TagListResponse,
}

#[derive(Deserialize, Default)]
struct TagListResponse {
    #[serde(default)]
    tags: Vec<TagName>,
}

#[derive(Deserialize, Default)]
struct TagName {
    #[serde(default)]
    tagid: Option<i64>,
    #[serde(default)]
    name: Option<String>,
}

// ---------- fetching ----------

/// Maps Steam's numeric tag ids to names. Small (~450 entries) and rarely changes.
pub struct TagVocabulary {
    names: HashMap<i64, String>,
}

impl TagVocabulary {
    pub fn len(&self) -> usize {
        self.names.len()
    }

    fn resolve(&self, tags: &[ItemTag]) -> Vec<String> {
        // Already ranked by vote weight, so preserve the order and drop ids the
        // vocabulary doesn't cover rather than storing raw numbers.
        tags.iter()
            .filter_map(|t| t.tagid)
            .filter_map(|id| self.names.get(&id).cloned())
            .collect()
    }
}

/// English and the US store, deliberately: `client/src/search.js`'s synonym table is
/// written against English Steam tag names, so localized tags would break search rather
/// than improve it.
fn context_json() -> serde_json::Value {
    serde_json::json!({ "language": "english", "country_code": "US" })
}

pub async fn fetch_tag_vocabulary(client: &reqwest::Client) -> Result<TagVocabulary> {
    let response = client
        .get(TAG_LIST_URL)
        .query(&[("language", "english")])
        .send()
        .await
        .context("requesting the Steam tag list")?;

    if !response.status().is_success() {
        bail!("Steam tag list request failed: {}", response.status());
    }

    let body: TagListEnvelope = response.json().await.context("parsing the Steam tag list")?;
    let names: HashMap<i64, String> = body
        .response
        .tags
        .into_iter()
        .filter_map(|t| Some((t.tagid?, t.name?)))
        .collect();

    if names.is_empty() {
        bail!("Steam returned an empty tag list");
    }
    Ok(TagVocabulary { names })
}

/// Looks up one batch of appids.
///
/// Returns an entry for every appid that came back, keyed by appid. Steam echoes failed
/// lookups in place rather than omitting them, but results are matched on the returned
/// appid regardless — aligning by array position would silently attach one game's tags to
/// another, and would look entirely plausible when spot-checked.
pub async fn fetch_batch(
    client: &reqwest::Client,
    vocabulary: &TagVocabulary,
    appids: &[String],
    now: i64,
) -> Result<HashMap<String, SteamMetadata>> {
    let ids: Vec<serde_json::Value> = appids
        .iter()
        .filter_map(|a| a.parse::<i64>().ok())
        .map(|appid| serde_json::json!({ "appid": appid }))
        .collect();

    let input = serde_json::json!({
        "ids": ids,
        "context": context_json(),
        "data_request": {
            "include_tag_count": TAGS_PER_GAME,
            "include_basic_info": true,
            "include_release": true,
            "include_reviews": true,
        },
    })
    .to_string();

    let response = client
        .get(ITEMS_URL)
        .query(&[("input_json", input.as_str())])
        .send()
        .await
        .context("requesting Steam store items")?;

    if !response.status().is_success() {
        bail!("Steam store request failed: {}", response.status());
    }

    let body: ItemsEnvelope = response
        .json()
        .await
        .context("parsing the Steam store response")?;

    let mut out = HashMap::new();
    for item in body.response.store_items {
        let Some(appid) = item.appid else { continue };
        let appid = appid.to_string();

        // Steam answered and has nothing usable here. Recorded so the game isn't
        // retried on every launch — but only ever from a per-item answer, never from a
        // failed request, which would blank a whole batch on one outage.
        if item.success != Some(1) || item.visible == Some(false) {
            out.insert(
                appid.clone(),
                SteamMetadata {
                    appid,
                    not_found: true,
                    fetched_at: now,
                    ..Default::default()
                },
            );
            continue;
        }

        let basic = item.basic_info.unwrap_or_default();
        let reviews = item
            .reviews
            .and_then(|r| r.summary_filtered)
            .unwrap_or_default();

        out.insert(
            appid.clone(),
            SteamMetadata {
                appid,
                tags: vocabulary.resolve(&item.tags),
                short_description: basic.short_description.filter(|s| !s.trim().is_empty()),
                developer: basic.developers.into_iter().find_map(|d| d.name),
                publisher: basic.publishers.into_iter().find_map(|p| p.name),
                released_at: item.release.and_then(|r| r.steam_release_date),
                review_count: reviews.review_count,
                review_percent: reviews.percent_positive,
                review_label: reviews.review_score_label,
                not_found: false,
                fetched_at: now,
            },
        );
        let _ = item.name;
    }

    Ok(out)
}
