use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub platform: String,
    pub title: String,
    pub playtime_minutes: Option<i64>,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataEntry {
    #[serde(default)]
    pub matched_name: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub not_found: bool,
    #[serde(default)]
    pub fetched_at: i64,
}

/// How far along you are with a game. "Backlog" is the absence of a row rather than a
/// variant, so untouched games cost nothing to store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameStatus {
    Playing,
    Completed,
    Dropped,
}

impl GameStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GameStatus::Playing => "playing",
            GameStatus::Completed => "completed",
            GameStatus::Dropped => "dropped",
        }
    }

    /// Parsed at the boundary so an unexpected value from the webview is rejected rather
    /// than written to the database as free text.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "playing" => Some(GameStatus::Playing),
            "completed" => Some(GameStatus::Completed),
            "dropped" => Some(GameStatus::Dropped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub status: GameStatus,
    pub updated_at: i64,
    /// Only set while the game is marked completed.
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentJob {
    pub running: bool,
    pub total: usize,
    pub completed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// Non-secret, so it's safe to send to the webview for display.
    pub steam_id: String,
    /// The Steam API key and IGDB secret deliberately never leave Rust —
    /// the UI only learns whether they are present.
    pub steam_configured: bool,
    pub igdb_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicLibrary {
    pub games: Vec<Game>,
    pub imported_at: Option<i64>,
}

/// One item as it appears in Epic's order history export.
#[derive(Debug, Deserialize)]
pub struct EpicExportItem {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "offerTitle")]
    pub offer_title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "offerId")]
    pub offer_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "catalogItemId")]
    pub catalog_item_id: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default, rename = "coverUrl")]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default, rename = "giftRecipient")]
    pub gift_recipient: Option<String>,
}

impl EpicExportItem {
    pub fn best_title(&self) -> Option<&str> {
        self.title
            .as_deref()
            .or(self.description.as_deref())
            .or(self.offer_title.as_deref())
            .or(self.name.as_deref())
            .filter(|t| !t.trim().is_empty())
    }

    pub fn best_id(&self) -> Option<&str> {
        self.offer_id
            .as_deref()
            .or(self.id.as_deref())
            .or(self.catalog_item_id.as_deref())
    }

    pub fn best_image(&self) -> Option<String> {
        self.image.clone().or_else(|| self.cover_url.clone())
    }

    /// Unreal Engine Marketplace / Fab assets share the "ue" namespace — the same
    /// signal `legendary` uses to keep them out of a games list.
    pub fn is_unreal_asset(&self) -> bool {
        self.namespace.as_deref() == Some("ue")
    }

    pub fn is_gift_to_someone_else(&self) -> bool {
        self.gift_recipient
            .as_deref()
            .is_some_and(|r| !r.trim().is_empty())
    }
}

/// Titles are matched against IGDB results by a normalized slug, so the same game
/// owned on two stores shares one metadata row.
/// Mirrors client/src/slugify.js exactly: lowercase, drop ™®©, collapse every run of
/// non-[a-z0-9] into a single dash, then trim dashes. Non-ASCII letters become dashes,
/// which is why "ABZÛ" and "ABZU" do NOT collide — matching the JS behaviour is what
/// keeps the migrated cache keys valid.
pub fn slugify(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        if matches!(ch, '™' | '®' | '©') {
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_matches_js_behaviour() {
        assert_eq!(slugify("Balatro"), "balatro");
        assert_eq!(slugify("DARK SOULS™: REMASTERED"), "dark-souls-remastered");
        assert_eq!(slugify("Assassin's Creed IV Black Flag"), "assassin-s-creed-iv-black-flag");
        assert_eq!(slugify(">observer_"), "observer");
        assert_eq!(slugify("A Plague Tale: Requiem"), "a-plague-tale-requiem");
        // Non-ASCII letters collapse to a dash and then get trimmed, same as the JS regex.
        assert_eq!(slugify("ABZÛ"), "abz");
    }

    #[test]
    fn game_status_round_trips_and_rejects_unknown() {
        use super::GameStatus;
        for status in [GameStatus::Playing, GameStatus::Completed, GameStatus::Dropped] {
            assert_eq!(GameStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(GameStatus::parse("finished"), None);
        assert_eq!(GameStatus::parse(""), None);
        assert_eq!(GameStatus::parse("PLAYING"), None);
    }
}
