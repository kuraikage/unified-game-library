use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::models::MetadataEntry;

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const GAMES_URL: &str = "https://api.igdb.com/v4/games";
const TOKEN_SAFETY_MARGIN_MS: i64 = 5 * 60 * 1000;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Default)]
struct CachedToken {
    client_id: String,
    token: String,
    expires_at: i64,
}

/// Kept in memory only — a token is cheap to re-mint and doesn't belong on disk.
static TOKEN: Mutex<Option<CachedToken>> = Mutex::new(None);

async fn access_token(client_id: &str, client_secret: &str) -> Result<String> {
    {
        let guard = TOKEN.lock().unwrap();
        if let Some(cached) = guard.as_ref() {
            if cached.client_id == client_id
                && cached.expires_at - TOKEN_SAFETY_MARGIN_MS > now_ms()
            {
                return Ok(cached.token.clone());
            }
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .post(TOKEN_URL)
        .query(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "client_credentials"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        bail!(
            "Twitch token request failed: {} — check the IGDB Client ID and Secret.",
            response.status()
        );
    }

    let body: TokenResponse = response.json().await?;
    let expires_at = now_ms() + body.expires_in * 1000;

    *TOKEN.lock().unwrap() = Some(CachedToken {
        client_id: client_id.to_string(),
        token: body.access_token.clone(),
        expires_at,
    });

    Ok(body.access_token)
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Cover {
    image_id: String,
}

#[derive(Deserialize)]
struct IgdbGame {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    themes: Vec<Named>,
    #[serde(default)]
    keywords: Vec<Named>,
    #[serde(default)]
    cover: Option<Cover>,
}

fn escape_query(title: &str) -> String {
    title.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Looks a title up on IGDB. `Ok(None)` means "searched, no match" — the caller records
/// that so we don't retry it on every launch.
pub async fn lookup_game(
    client_id: &str,
    client_secret: &str,
    title: &str,
) -> Result<Option<MetadataEntry>> {
    let token = access_token(client_id, client_secret).await?;
    let body = format!(
        "search \"{}\"; fields name,genres.name,themes.name,keywords.name,cover.image_id; limit 1;",
        escape_query(title)
    );

    let client = reqwest::Client::new();
    let response = client
        .post(GAMES_URL)
        .header("Client-ID", client_id)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "text/plain")
        .body(body)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("IGDB request failed: {}", response.status());
    }

    let results: Vec<IgdbGame> = response.json().await?;
    let Some(game) = results.into_iter().next() else {
        return Ok(None);
    };

    // Themes are broad ("Horror"), keywords are specific ("roguelike") — both are useful
    // to search on, but keywords get capped so a handful of games don't dominate the column.
    let mut tags: Vec<String> = game.themes.into_iter().map(|t| t.name).collect();
    tags.extend(game.keywords.into_iter().take(5).map(|k| k.name));

    Ok(Some(MetadataEntry {
        matched_name: game.name,
        genres: game.genres.into_iter().map(|g| g.name).collect(),
        tags,
        cover_url: game
            .cover
            .map(|c| format!("https://images.igdb.com/igdb/image/upload/t_cover_big/{}.jpg", c.image_id)),
        not_found: false,
        fetched_at: now_ms(),
    }))
}
