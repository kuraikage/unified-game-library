//! Where the app keeps its data.
//!
//! The desktop app gets this path from Tauri, but the MCP server runs as a plain stdio
//! process with no Tauri context, so it has to resolve the same directory independently.

use std::path::PathBuf;

use anyhow::{bail, Result};

/// Must match `identifier` in `tauri.conf.json` — Tauri derives the data directory from it.
pub const APP_IDENTIFIER: &str = "com.ugly.library";

/// Overrides the resolved location, for tests and non-standard installs.
pub const DATA_DIR_ENV: &str = "UGLY_DATA_DIR";

/// Resolves the app data directory the same way Tauri's `app_data_dir()` does:
/// the platform data directory joined with the app identifier.
pub fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }

    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library").join("Application Support"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));

    match base {
        Some(base) => Ok(base.join(APP_IDENTIFIER)),
        None => bail!(
            "Could not locate your user data directory. \
             Set {DATA_DIR_ENV} to the folder containing ugly.db."
        ),
    }
}

/// The database file itself. Never created here — the desktop app owns the schema, and a
/// missing file means the app has not been run yet rather than something to paper over.
pub fn database_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("ugly.db"))
}
