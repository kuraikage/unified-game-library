//! Finding the Steam appid for a game we only own on Epic.
//!
//! Epic titles carry no appid, but most of those games also exist on Steam — and once the
//! appid is known they get Steam's tags like any other game. Resolved once and cached,
//! including the misses, so this never runs twice for the same title.

use anyhow::{Context, Result};
use serde::Deserialize;

use ugly_core::models::slugify;

const SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";

/// Suffixes that mark the same game rather than a different one. Stripped from both sides
/// before comparing, so "Death Stranding" can match "DEATH STRANDING DIRECTOR'S CUT".
///
/// Deliberately does not include anything numeric: "Death Stranding" must never match
/// "Death Stranding 2".
const EDITION_SUFFIXES: &[&str] = &[
    "-goty-edition",
    "-game-of-the-year-edition",
    "-ultimate-edition",
    "-definitive-edition",
    "-complete-edition",
    "-deluxe-edition",
    "-enhanced-edition",
    "-special-edition",
    "-director-s-cut",
    "-directors-cut",
    "-remastered",
    "-the-complete-edition",
];

#[derive(Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize, Default)]
struct SearchItem {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    name: Option<String>,
    /// "app" for games; DLC and bundles report something else.
    #[serde(default)]
    r#type: Option<String>,
}

/// Strips a trailing edition marker, if any. Applied repeatedly so
/// "Game: Definitive Edition Remastered" reduces fully.
fn trim_edition(slug: &str) -> &str {
    let mut out = slug;
    loop {
        let before = out;
        for suffix in EDITION_SUFFIXES {
            out = out.strip_suffix(suffix).unwrap_or(out);
        }
        if out == before {
            return out;
        }
    }
}

/// True when two titles name the same game.
///
/// Strict on purpose. A wrong appid is worse than none: it silently attaches another
/// game's tags with nothing to signal the mistake, and Steam's top result for a title it
/// doesn't carry is routinely the sequel.
fn is_same_game(query_slug: &str, candidate: &str) -> bool {
    let candidate = slugify(candidate);
    candidate == query_slug || trim_edition(&candidate) == trim_edition(query_slug)
}

/// Searching stopped because Steam asked us to back off. The caller should end the pass
/// and resume next launch — progress is already cached, so nothing is lost.
#[derive(Debug)]
pub struct RateLimited;

/// Looks up one title. `Ok(None)` means "searched, and it isn't on Steam under this name",
/// which the caller caches so it is never searched again.
pub async fn resolve_appid(
    client: &reqwest::Client,
    title: &str,
) -> Result<Result<Option<String>, RateLimited>> {
    let response = client
        .get(SEARCH_URL)
        .query(&[("term", title), ("cc", "US"), ("l", "english")])
        .send()
        .await
        .context("searching the Steam store")?;

    if response.status().as_u16() == 429 {
        return Ok(Err(RateLimited));
    }
    if !response.status().is_success() {
        anyhow::bail!("Steam store search failed: {}", response.status());
    }

    let body: SearchResponse = response
        .json()
        .await
        .context("parsing the Steam store search response")?;

    let query_slug = slugify(title);
    for item in body.items {
        let (Some(id), Some(name)) = (item.id, item.name.as_deref()) else {
            continue;
        };
        // Anything that isn't a game would attach a DLC's or bundle's tags.
        if item.r#type.as_deref().is_some_and(|t| t != "app") {
            continue;
        }
        if is_same_game(&query_slug, name) {
            return Ok(Ok(Some(id.to_string())));
        }
    }
    Ok(Ok(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_same_game_across_naming_differences() {
        // Real cases from the library: casing, punctuation and edition markers.
        assert!(is_same_game(&slugify("Control Ultimate Edition"), "CONTROL Ultimate Edition"));
        assert!(is_same_game(&slugify("Kingdom New Lands"), "Kingdom: New Lands"));
        assert!(is_same_game(&slugify("Death Stranding"), "DEATH STRANDING DIRECTOR'S CUT"));
        assert!(is_same_game(&slugify("Sludge Life"), "SLUDGE LIFE"));
    }

    #[test]
    fn rejects_a_sequel_or_a_different_game() {
        // Steam's top hit for a title it doesn't carry is often the sequel, and taking it
        // would silently attach the wrong game's tags.
        assert!(!is_same_game(&slugify("Death Stranding"), "DEATH STRANDING 2: ON THE BEACH"));
        assert!(!is_same_game(&slugify("Hades"), "Hades II"));
        assert!(!is_same_game(&slugify("Portal"), "Portal 2"));
        assert!(!is_same_game(&slugify("Fall Guys"), "Fall Guys Season Pass"));
    }

    #[test]
    fn edition_markers_are_stripped_repeatedly_but_never_numbers() {
        assert_eq!(trim_edition("control-ultimate-edition"), "control");
        assert_eq!(trim_edition("nioh-complete-edition"), "nioh");
        assert_eq!(trim_edition("death-stranding-2-on-the-beach"), "death-stranding-2-on-the-beach");
    }
}
