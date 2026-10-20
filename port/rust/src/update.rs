//! Desktop self-update: check the GitHub release feed, download the matching
//! artifact, verify its published SHA-256, and hand the archive to the shipped
//! `apply-update` script.
//!
//! Why a script applies the update instead of this process replacing itself:
//! Windows locks a running image (the replacement must happen after the process
//! exits), and every platform needs the server restarted behind the tray or
//! autostart entry. `apply-update.bat` / `apply-update.sh` wait for this process,
//! extract over the install directory (never touching `storage/`), and restart.
//!
//! Nothing here runs automatically: `/api/update/check` is read-only and
//! `/api/update/apply` is a privileged (loopback or token) action, so a LAN
//! client cannot push a binary onto the host.

use serde_json::Value;

use crate::config;

const REPO: &str = "aiskendi/pencarimovie-server";
const API_LATEST: &str = "https://api.github.com/repos/aiskendi/pencarimovie-server/releases/latest";

/// `linux-x86_64`, `windows-x86_64`, `macos-aarch64` — the artifact suffix the
/// release workflow publishes. `None` on a platform we do not build for.
pub fn platform_key() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        _ => None,
    }
}

/// `v2.8.9` → `(2, 8, 9)`; tolerates `2.8.8-5-g9fc84e1-dirty` (build stamps).
fn numeric(version: &str) -> (u64, u64, u64) {
    let head = version.trim().trim_start_matches('v');
    let head = head.split('-').next().unwrap_or(head);
    let mut it = head.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

fn is_newer(latest: &str, current: &str) -> bool {
    numeric(latest) > numeric(current)
}

fn asset_of(release: &Value, suffix: &str) -> Option<String> {
    release
        .get("assets")
        .and_then(|a| a.as_array())?
        .iter()
        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
        .find(|n| n.ends_with(suffix))
        .map(|n| n.to_string())
}

fn asset_url(release: &Value, name: &str) -> Option<String> {
    release
        .get("assets")
        .and_then(|a| a.as_array())?
        .iter()
        .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|u| u.as_str())
        .map(|u| u.to_string())
}

async fn latest_release() -> Value {
    let client = crate::httpc::client();
    let req = client
        .get(API_LATEST)
        .header("User-Agent", config::user_agent())
        .header("Accept", "application/vnd.github+json");
    match tokio::time::timeout(std::time::Duration::from_secs(15), req.send()).await {
        Ok(Ok(res)) if res.status().is_success() => res.json::<Value>().await.unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// `GET /api/update/check` — never downloads anything.
pub async fn check() -> Value {
    let current = config::app_version();
    let release = latest_release().await;
    if release.is_null() {
        return serde_json::json!({
            "ok": false,
            "current": current,
            "build": config::build_id(),
            "error": "Could not reach the release feed. Check this machine's network, then retry.",
        });
    }
    let latest = release
        .get("tag_name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let Some(key) = platform_key() else {
        return serde_json::json!({
            "ok": false,
            "current": current,
            "build": config::build_id(),
            "latest": latest,
            "error": "No update artifact is published for this platform.",
        });
    };
    let asset = asset_of(&release, &format!("pencarimovie-server-{key}.tar.gz"));
    serde_json::json!({
        "ok": true,
        "current": current,
        "build": config::build_id(),
        "latest": latest,
        "update_available": is_newer(&latest, &current),
        "asset": asset,
        "notes_url": release.get("html_url").and_then(|v| v.as_str()).unwrap_or(""),
    })
}

fn updates_dir() -> std::path::PathBuf {
    let dir = crate::storage::storage_path("updates");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn apply_script() -> std::path::PathBuf {
    let name = if cfg!(windows) {
        "apply-update.bat"
    } else {
        "apply-update.sh"
    };
    crate::storage::app_root().join(name)
}

/// `POST /api/update/apply` — download the artifact, verify its hash, then run
/// the apply script detached. Returns as soon as the script is started; the
/// server stops a moment later, so the caller may lose the connection.
pub async fn apply() -> Value {
    let info = check().await;
    if info.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return info;
    }
    if info.get("update_available").and_then(|v| v.as_bool()) != Some(true) {
        return serde_json::json!({
            "ok": true,
            "up_to_date": true,
            "current": config::app_version(),
            "message": "This is already the latest version.",
        });
    }
    let Some(asset) = info.get("asset").and_then(|v| v.as_str()).map(|s| s.to_string()) else {
        return serde_json::json!({
            "ok": false,
            "error": "The release has no artifact for this platform.",
        });
    };
    let release = latest_release().await;
    let Some(url) = asset_url(&release, &asset) else {
        return serde_json::json!({ "ok": false, "error": "Asset URL is missing from the release." });
    };
    let dest = updates_dir().join(&asset);
    if !crate::httpc::download_file(&url, &dest, 600).await {
        return serde_json::json!({ "ok": false, "error": "Download failed. Check this machine's network." });
    }
    // Integrity: the release ships a `<asset>.sha256` sidecar holding just the hash.
    if let Some(sum_url) = asset_url(&release, &format!("{asset}.sha256")) {
        let want = crate::httpc::get_contents(&sum_url, 30, Default::default())
            .await
            .unwrap_or_default();
        let want = want.split_whitespace().next().unwrap_or("").to_lowercase();
        let got = match std::fs::read(&dest) {
            Ok(bytes) => {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(&bytes);
                format!("{:x}", h.finalize())
            }
            Err(_) => String::new(),
        };
        if want.len() == 64 && got != want {
            let _ = std::fs::remove_file(&dest);
            return serde_json::json!({
                "ok": false,
                "error": "The downloaded archive failed its SHA-256 check; nothing was installed.",
            });
        }
    }
    let script = apply_script();
    if !script.is_file() {
        return serde_json::json!({
            "ok": false,
            "downloaded": dest.display().to_string(),
            "error": format!(
                "Downloaded {}, but {} is missing - extract the archive over this folder manually.",
                asset,
                script.display()
            ),
        });
    }
    let started = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", script.to_string_lossy().as_ref(), dest.to_string_lossy().as_ref()])
            .spawn()
    } else {
        std::process::Command::new("sh")
            .arg(&script)
            .arg(&dest)
            .spawn()
    };
    match started {
        Ok(_) => {
            // Let the HTTP response go out, then stop so the script can replace
            // the binary and start the new one behind the tray/autostart entry.
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                let _ = crate::cache::flush();
                std::process::exit(0);
            });
            serde_json::json!({
                "ok": true,
                "starting": true,
                "asset": asset,
                "message": "Update downloaded and verified. The server restarts now.",
            })
        }
        Err(e) => serde_json::json!({ "ok": false, "error": format!("Could not start the updater: {e}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_versions_compare_and_ignore_build_stamps() {
        assert_eq!(numeric("v2.8.9"), (2, 8, 9));
        assert_eq!(numeric("2.8.8-5-g9fc84e1-dirty"), (2, 8, 8));
        assert!(is_newer("v2.9.0", "2.8.8"));
        assert!(is_newer("v2.8.9", "2.8.8-5-gabc"));
        assert!(!is_newer("v2.8.8", "2.8.8-5-gabc"));
        assert!(!is_newer("v2.7.9", "2.8.0"));
    }

    #[test]
    fn platform_keys_are_the_published_names() {
        // The keys must match the artifact suffixes release.yml uploads.
        let key = platform_key().expect("this test runs on a supported platform");
        let published = ["linux-x86_64", "linux-aarch64", "windows-x86_64", "macos-aarch64", "macos-x86_64"];
        assert!(published.contains(&key), "{key}");
    }
}
