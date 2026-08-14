use anyhow::{bail, Result};
use serde::Deserialize;

use ugly_core::models::Game;

const OWNED_GAMES_URL: &str =
    "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/";

#[derive(Deserialize)]
struct OwnedGamesResponse {
    response: OwnedGamesBody,
}

#[derive(Deserialize)]
struct OwnedGamesBody {
    #[serde(default)]
    games: Vec<SteamGame>,
}

#[derive(Deserialize)]
struct SteamGame {
    appid: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    playtime_forever: i64,
}

pub async fn fetch_library(api_key: &str, steam_id: &str) -> Result<Vec<Game>> {
    let client = reqwest::Client::new();
    let response = client
        .get(OWNED_GAMES_URL)
        .query(&[
            ("key", api_key),
            ("steamid", steam_id),
            ("format", "json"),
            ("include_appinfo", "true"),
            ("include_played_free_games", "true"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        bail!(
            "Steam API request failed: {} — check your API key and SteamID.",
            response.status()
        );
    }

    let body: OwnedGamesResponse = response.json().await?;

    let mut games: Vec<Game> = body
        .response
        .games
        .into_iter()
        .filter_map(|g| {
            let title = g.name.filter(|n| !n.trim().is_empty())?;
            Some(Game {
                id: format!("steam-{}", g.appid),
                platform: "steam".into(),
                title,
                playtime_minutes: Some(g.playtime_forever),
                cover_url: Some(format!(
                    "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg",
                    g.appid
                )),
            })
        })
        .collect();

    games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(games)
}
