use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::models::slugify;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGame {
    pub platform: String,
    /// Steam appid, or the Epic AppName — whatever the launcher needs to start it.
    pub launch_id: String,
    pub title: String,
    pub install_dir: Option<String>,
    /// Epic needs namespace:catalogItemId:appName to launch; Steam leaves these empty.
    pub namespace: Option<String>,
    pub catalog_item_id: Option<String>,
}

// ---------- Steam ----------

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey("Software\\Valve\\Steam").ok()?;
    let path: String = key.get_value("SteamPath").ok()?;
    Some(PathBuf::from(path.replace('/', "\\")))
}

#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    None
}

/// libraryfolders.vdf is Valve's own key/value format. We only need the "path" entries,
/// so a line scan is more robust here than pulling in a full VDF parser.
fn steam_library_paths(root: &PathBuf) -> Vec<PathBuf> {
    let mut paths = vec![root.clone()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(&vdf) else {
        return paths;
    };

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("\"path\"") {
            continue;
        }
        if let Some(value) = line.split('"').nth(3) {
            let path = PathBuf::from(value.replace("\\\\", "\\"));
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Reads `"key" "value"` out of an .acf manifest.
fn acf_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(&needle))
        .and_then(|line| line.split('"').nth(3))
}

fn steam_installed() -> Vec<InstalledGame> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };

    let mut games = Vec::new();
    for library in steam_library_paths(&root) {
        let steamapps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&steamapps) else {
            continue;
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let (Some(appid), Some(title)) = (acf_value(&text, "appid"), acf_value(&text, "name"))
            else {
                continue;
            };
            let install_dir = acf_value(&text, "installdir")
                .map(|d| steamapps.join("common").join(d).to_string_lossy().to_string());

            games.push(InstalledGame {
                platform: "steam".into(),
                launch_id: appid.to_string(),
                title: title.to_string(),
                install_dir,
                namespace: None,
                catalog_item_id: None,
            });
        }
    }
    games
}

// ---------- Epic ----------

#[derive(serde::Deserialize)]
struct EpicManifest {
    #[serde(default, rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(default, rename = "AppName")]
    app_name: Option<String>,
    #[serde(default, rename = "CatalogItemId")]
    catalog_item_id: Option<String>,
    #[serde(default, rename = "CatalogNamespace")]
    catalog_namespace: Option<String>,
    #[serde(default, rename = "InstallLocation")]
    install_location: Option<String>,
}

fn epic_installed() -> Vec<InstalledGame> {
    let dir = PathBuf::from("C:\\ProgramData\\Epic\\EpicGamesLauncher\\Data\\Manifests");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut games = Vec::new();
    for entry in entries.flatten() {
        if entry.path().extension().is_none_or(|e| e != "item") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<EpicManifest>(&text) else {
            continue;
        };

        // Same "ue" signal we use when importing: Marketplace/Fab assets and the engine
        // itself show up here, and none of them are games.
        if manifest.catalog_namespace.as_deref() == Some("ue") {
            continue;
        }
        let (Some(title), Some(app_name)) = (manifest.display_name, manifest.app_name) else {
            continue;
        };

        games.push(InstalledGame {
            platform: "epic".into(),
            launch_id: app_name,
            title,
            install_dir: manifest.install_location,
            namespace: manifest.catalog_namespace,
            catalog_item_id: manifest.catalog_item_id,
        });
    }
    games
}

// ---------- matching ----------

/// Maps our library's game ids to what's installed locally.
///
/// Steam matches exactly on appid (our ids are already `steam-<appid>`). Epic manifests key
/// on CatalogItemId, which is not the offerId we store, so those fall back to a title match.
pub fn detect(library_ids: &[(String, String, String)]) -> HashMap<String, InstalledGame> {
    let mut by_steam_appid: HashMap<String, InstalledGame> = HashMap::new();
    let mut by_epic_title: HashMap<String, InstalledGame> = HashMap::new();

    for game in steam_installed() {
        by_steam_appid.insert(game.launch_id.clone(), game);
    }
    for game in epic_installed() {
        by_epic_title.insert(slugify(&game.title), game);
    }

    let mut result = HashMap::new();
    for (id, platform, title) in library_ids {
        match platform.as_str() {
            "steam" => {
                if let Some(appid) = id.strip_prefix("steam-") {
                    if let Some(found) = by_steam_appid.get(appid) {
                        result.insert(id.clone(), found.clone());
                    }
                }
            }
            "epic" => {
                if let Some(found) = by_epic_title.get(&slugify(title)) {
                    result.insert(id.clone(), found.clone());
                }
            }
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::acf_value;

    #[test]
    fn parses_acf_fields() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"1091500\"\n\t\"name\"\t\t\"Cyberpunk 2077\"\n\t\"installdir\"\t\t\"Cyberpunk 2077\"\n}";
        assert_eq!(acf_value(acf, "appid"), Some("1091500"));
        assert_eq!(acf_value(acf, "name"), Some("Cyberpunk 2077"));
        assert_eq!(acf_value(acf, "installdir"), Some("Cyberpunk 2077"));
        assert_eq!(acf_value(acf, "missing"), None);
    }
}
