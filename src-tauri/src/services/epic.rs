use std::collections::HashMap;

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::models::{slugify, EpicExportItem, Game};

#[derive(Deserialize)]
struct OrderWrapper {
    #[serde(default)]
    orders: Vec<Order>,
}

#[derive(Deserialize)]
struct Order {
    #[serde(default, rename = "orderType")]
    order_type: Option<String>,
    #[serde(default)]
    items: Vec<EpicExportItem>,
}

/// Accepts either the flat array the bookmarklet produces, or a raw
/// `{ "orders": [...] }` payload straight from Epic's endpoint.
pub fn parse_export(payload: &str) -> Result<Vec<Game>> {
    let value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|_| anyhow::anyhow!("That is not valid JSON. Copy the bookmarklet output exactly."))?;

    let items: Vec<EpicExportItem> = if value.is_array() {
        serde_json::from_value(value)?
    } else if value.get("games").is_some_and(|g| g.is_array()) {
        serde_json::from_value(value["games"].clone())?
    } else if value.get("orders").is_some() {
        let wrapper: OrderWrapper = serde_json::from_value(value)?;
        wrapper
            .orders
            .into_iter()
            .filter(|o| o.order_type.as_deref().is_none_or(|t| t == "PURCHASE"))
            .flat_map(|o| o.items)
            .collect()
    } else {
        bail!("Could not find a list of games in that JSON.");
    };

    let mut seen: HashMap<String, Game> = HashMap::new();
    for item in items {
        if item.is_unreal_asset() || item.is_gift_to_someone_else() {
            continue;
        }
        let Some(title) = item.best_title() else {
            continue;
        };
        let title = title.to_string();
        let key = item
            .best_id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| slugify(&title));

        seen.entry(key.clone()).or_insert_with(|| Game {
            id: format!("epic-{key}"),
            platform: "epic".into(),
            title,
            playtime_minutes: None,
            cover_url: item.best_image(),
        });
    }

    if seen.is_empty() {
        bail!("That JSON did not contain any recognizable game titles.");
    }

    let mut games: Vec<Game> = seen.into_values().collect();
    games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::parse_export;

    #[test]
    fn filters_unreal_assets_and_gifts() {
        let payload = r#"[
            {"title":"Citizen Sleeper","offerId":"a1"},
            {"title":"Some Asset Pack","offerId":"a2","namespace":"ue"},
            {"title":"Gifted Game","offerId":"a3","giftRecipient":"friend@example.com"},
            {"title":"Hades","offerId":"a4"}
        ]"#;
        let games = parse_export(payload).unwrap();
        let titles: Vec<_> = games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Citizen Sleeper", "Hades"]);
    }

    #[test]
    fn accepts_raw_order_history() {
        let payload = r#"{"orders":[
            {"orderType":"PURCHASE","items":[{"description":"Citizen Sleeper","offerId":"a1"}]},
            {"orderType":"REFUND","items":[{"description":"Refunded Thing","offerId":"a2"}]}
        ]}"#;
        let games = parse_export(payload).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Citizen Sleeper");
    }

    #[test]
    fn deduplicates_by_offer_id() {
        let payload = r#"[
            {"title":"Hades","offerId":"a1"},
            {"title":"Hades","offerId":"a1"}
        ]"#;
        assert_eq!(parse_export(payload).unwrap().len(), 1);
    }
}
