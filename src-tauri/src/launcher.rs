use anyhow::{bail, Result};

/// Opens a URL with the shell's registered protocol handler.
///
/// Deliberately NOT routed through `cmd /c start`: Epic's launch URLs contain `%3A`, and cmd
/// expands `%3` as a batch argument, silently corrupting the URL before the launcher sees it.
/// ShellExecuteW receives the string exactly as written.
#[cfg(windows)]
fn shell_open(url: &str) -> Result<()> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = HSTRING::from("open");
    let target = HSTRING::from(url);
    let result = unsafe { ShellExecuteW(None, &operation, &target, None, None, SW_SHOWNORMAL) };

    // ShellExecuteW returns a value <= 32 to signal failure.
    if result.0 as isize <= 32 {
        bail!("Windows could not open {url} — is the launcher installed?");
    }
    Ok(())
}

#[cfg(not(windows))]
fn shell_open(_url: &str) -> Result<()> {
    bail!("Launching games is only supported on Windows.");
}

/// True if a process with this executable name is running.
#[cfg(windows)]
fn process_running(exe_name: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if name.eq_ignore_ascii_case(exe_name) {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn process_running(_exe_name: &str) -> bool {
    false
}

/// Reads the launcher path straight from the protocol handler Windows already has
/// registered, rather than assuming an install location.
#[cfg(windows)]
fn epic_launcher_exe() -> Option<String> {
    use winreg::enums::HKEY_CLASSES_ROOT;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey("com.epicgames.launcher\\shell\\open\\command")
        .ok()?;
    let command: String = key.get_value("").ok()?;
    // Value looks like: "C:\...\EpicGamesLauncher.exe" %1
    let path = command.split('"').nth(1)?;
    Some(path.to_string())
}

#[cfg(not(windows))]
fn epic_launcher_exe() -> Option<String> {
    None
}

/// Firing a `com.epicgames.launcher://` URL at a launcher that isn't running usually gets
/// dropped — it starts up but never acts on the request. So start it first and give it a
/// moment before handing over the URL.
async fn ensure_epic_launcher_running() -> Result<()> {
    const EXE: &str = "EpicGamesLauncher.exe";
    if process_running(EXE) {
        return Ok(());
    }

    let Some(exe) = epic_launcher_exe() else {
        bail!("The Epic Games Launcher doesn't appear to be installed.");
    };
    shell_open(&exe)?;

    // Wait for it to come up, then let it finish initialising before sending the URL.
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if process_running(EXE) {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            return Ok(());
        }
    }
    bail!("The Epic Games Launcher did not start in time.");
}

/// Every outbound URL is built here in Rust from an id the frontend supplies, rather than
/// letting the webview hand us a finished URL. That keeps "open something" from becoming a
/// general-purpose escape hatch if the frontend were ever compromised.
pub fn launch_steam(appid: &str) -> Result<()> {
    validate_appid(appid)?;
    shell_open(&format!("steam://rungameid/{appid}"))
}

pub fn install_steam(appid: &str) -> Result<()> {
    validate_appid(appid)?;
    shell_open(&format!("steam://install/{appid}"))
}

/// Current launcher builds require the SandboxId:CatalogId:ArtifactId triple with the colons
/// percent-encoded. The older `apps/{ArtifactId}` form was deprecated and no longer works.
pub async fn launch_epic(namespace: &str, catalog_item_id: &str, app_name: &str) -> Result<()> {
    for part in [namespace, catalog_item_id, app_name] {
        if part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            bail!("Unexpected Epic launch identifier.");
        }
    }
    ensure_epic_launcher_running().await?;
    shell_open(&format!(
        "com.epicgames.launcher://apps/{namespace}%3A{catalog_item_id}%3A{app_name}?action=launch&silent=true"
    ))
}

/// Epic exposes no install action for a game you don't own locally, and we don't have the
/// identifier triple until it's installed. Opening the launcher's own store page is the
/// closest we can get, and keeps the user inside the Epic app rather than a web browser.
pub async fn open_epic_store(title: &str) -> Result<()> {
    ensure_epic_launcher_running().await?;
    shell_open(&format!("com.epicgames.launcher://store/product/{}", store_slug(title)))
}

/// Epic store slugs are generally the kebab-cased title.
fn store_slug(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn validate_appid(appid: &str) -> Result<()> {
    if appid.is_empty() || !appid.chars().all(|c| c.is_ascii_digit()) {
        bail!("Unexpected Steam app id.");
    }
    Ok(())
}

/// Used for the handful of documentation links in the UI. Restricted to https so a
/// compromised frontend cannot use it to launch local programs.
pub fn open_external(url: &str) -> Result<()> {
    if !url.starts_with("https://") {
        bail!("Only https links can be opened.");
    }
    shell_open(url)
}

#[cfg(test)]
mod tests {
    use super::{store_slug, validate_appid};

    #[test]
    fn builds_store_slugs() {
        assert_eq!(store_slug("ABZU"), "abzu");
        assert_eq!(store_slug("A Plague Tale: Requiem"), "a-plague-tale-requiem");
        assert_eq!(store_slug("Horizon Zero Dawn™ Remastered"), "horizon-zero-dawn-remastered");
    }

    #[test]
    fn rejects_non_numeric_appids() {
        assert!(validate_appid("1091500").is_ok());
        assert!(validate_appid("1091500; calc").is_err());
        assert!(validate_appid("").is_err());
    }
}
