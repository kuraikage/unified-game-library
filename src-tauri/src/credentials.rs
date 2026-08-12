use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "com.ugly.library";

/// Secrets live in the Windows Credential Manager (encrypted at rest, per Windows user)
/// rather than a plaintext file, and are only ever read inside Rust — no Tauri command
/// returns them to the webview.
#[derive(Clone, Copy)]
pub enum Secret {
    SteamApiKey,
    IgdbClientId,
    IgdbClientSecret,
}

impl Secret {
    fn key(self) -> &'static str {
        match self {
            Secret::SteamApiKey => "steam_api_key",
            Secret::IgdbClientId => "igdb_client_id",
            Secret::IgdbClientSecret => "igdb_client_secret",
        }
    }
}

pub fn set(secret: Secret, value: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, secret.key())?;
    if value.trim().is_empty() {
        // Clearing is not an error if nothing was stored.
        let _ = entry.delete_credential();
    } else {
        entry.set_password(value.trim())?;
    }
    Ok(())
}

pub fn get(secret: Secret) -> Option<String> {
    let entry = Entry::new(SERVICE, secret.key()).ok()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

pub fn has(secret: Secret) -> bool {
    get(secret).is_some()
}
