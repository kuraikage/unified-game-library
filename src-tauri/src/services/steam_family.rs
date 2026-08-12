use anyhow::{bail, Result};
use serde::Deserialize;

use crate::models::Game;

const FAMILY_GROUP_URL: &str =
    "https://api.steampowered.com/IFamilyGroupsService/GetFamilyGroupForUser/v1/";
const SHARED_APPS_URL: &str =
    "https://api.steampowered.com/IFamilyGroupsService/GetSharedLibraryApps/v1/";

#[derive(Deserialize)]
struct Envelope<T> {
    response: T,
}

#[derive(Deserialize)]
struct FamilyGroupResponse {
    #[serde(default)]
    family_groupid: Option<String>,
}

#[derive(Deserialize)]
struct SharedAppsResponse {
    #[serde(default)]
    apps: Vec<SharedApp>,
}

#[derive(Deserialize)]
struct SharedApp {
    appid: i64,
    #[serde(default)]
    name: Option<String>,
    /// SteamIDs of family members who own it. Absent/empty means nobody in the family
    /// actually owns it, so it isn't playable.
    #[serde(default)]
    owner_steamids: Vec<String>,
    #[serde(default)]
    exclude_reason: Option<i64>,
}

/// Steam has no official endpoint for family-shared games — `GetOwnedGames` only ever returns
/// what the key's own account owns. These IFamilyGroupsService endpoints are undocumented and
/// need a short-lived `webapi_token` from a logged-in Steam web session, not a Web API key.
pub async fn fetch_family_library(access_token: &str, own_steam_id: &str) -> Result<Vec<Game>> {
    let client = reqwest::Client::new();

    let group: Envelope<FamilyGroupResponse> = client
        .get(FAMILY_GROUP_URL)
        .query(&[("access_token", access_token), ("steamid", own_steam_id)])
        .send()
        .await?
        .json()
        .await?;

    let Some(family_id) = group.response.family_groupid.filter(|id| id != "0") else {
        bail!("This Steam account isn't part of a Family, or the token has expired.");
    };

    let shared: Envelope<SharedAppsResponse> = client
        .get(SHARED_APPS_URL)
        .query(&[
            ("access_token", access_token),
            ("family_groupid", &family_id),
            ("include_own", "false"),
            ("include_excluded", "false"),
        ])
        .send()
        .await?
        .json()
        .await?;

    let mut games: Vec<Game> = shared
        .response
        .apps
        .into_iter()
        .filter(|app| app.exclude_reason.unwrap_or(0) == 0)
        // Only games somebody in the family actually owns can be played.
        .filter(|app| !app.owner_steamids.is_empty())
        // Anything the account owns itself already arrives via GetOwnedGames.
        .filter(|app| !app.owner_steamids.iter().any(|id| id == own_steam_id))
        .filter_map(|app| {
            let title = app.name.filter(|n| !n.trim().is_empty())?;
            Some(Game {
                id: format!("steam-{}", app.appid),
                platform: "steam".into(),
                title,
                // Playtime belongs to the owner, not to you.
                playtime_minutes: None,
                cover_url: Some(format!(
                    "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg",
                    app.appid
                )),
            })
        })
        .collect();

    games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(games)
}
