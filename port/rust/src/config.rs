//! Constants lifted from backend.php:172-260 so every module uses the same
//! values as the PHP runtime (same defaults, same paths, same strings).

use crate::storage::{app_root, env, env_or};

pub fn wp_api_base() -> String {
    env_or(
        "FD_WP_API_BASE",
        "https://pencarimovie.com/wp-json/pencarimovie-server/v1",
    )
}

/// DPI-fallback host: a Cloudflare Worker that reverse-proxies /wp-json back
/// to the origin (backend.php `FD_WP_FALLBACK_HOST`).
pub const WP_FALLBACK_HOST: &str = "telegra.my";

pub fn wp_api_base_fallback() -> String {
    format!("https://{}/wp-json/pencarimovie-server/v1", WP_FALLBACK_HOST)
}

pub fn wp_version_url() -> String {
    format!("{}/version", wp_api_base())
}

// There is deliberately NO `admin-ajax.php` URL here. The PHP runtime proxied
// the dashboard's `stream_*` AJAX actions; this port reads the same data from
// the plugin's REST namespace (`wp_api_base()`), which is what every route
// already uses.

pub const AUTH_DEFAULT_PASSWORD: &str = "123456";
pub const AUTH_COOKIE: &str = "pm_auth";

/// The version this binary was BUILT from — `build.rs` stamps `git describe
/// --tags --always --dirty` into it, so every artifact is traceable even when
/// nothing was tagged. `PM_BUILD_VERSION` at build time overrides that.
pub fn build_id() -> &'static str {
    option_env!("PM_BUILD_VERSION").unwrap_or("0.0.0-dev")
}

/// The version of the APP SHELL that started us (the Android APK's versionName),
/// passed as `--app-version` / `PM_APP_VERSION`. `/api/version` reports it so the
/// dashboard can show what the user actually installed; without it the page only
/// ever showed the engine version, which never changes between APK releases.
static CLIENT_VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn set_client_version(v: &str) {
    let v = v.trim().trim_start_matches('v');
    if !v.is_empty() {
        let _ = CLIENT_VERSION.set(v.to_string());
    }
}

pub fn client_version() -> String {
    CLIENT_VERSION.get().cloned().unwrap_or_default()
}

/// `FD_APP_VERSION`: `<appRoot>/.release-tag` (v-prefix stripped) when the
/// install was stamped by the OTA updater, otherwise the version the binary was
/// BUILT from (`git describe`). The old hardcoded "2.7.9" made every native
/// build — whatever commit it came from — report the same number.
pub fn app_version() -> String {
    use std::fs;
    if let Ok(tag) = fs::read_to_string(app_root().join(".release-tag")) {
        let t = tag.trim();
        if !t.is_empty() {
            return t.trim_start_matches('v').to_string();
        }
    }
    build_id().to_string()
}

/// The version in `X.Y.Z` form, for the places that REQUIRE a plain version —
/// the addon descriptor's `version`, which Stremio/Nuvio parse and display.
///
/// A `git describe` stamp (`2.8.8-6-ge36d439-dirty`) is not a descriptor
/// version, and the manifest used to hardcode `1.0.0`, so every client showed
/// the addon as the first release forever no matter which build was installed.
pub fn semver() -> String {
    let raw = app_version();
    let head = raw.trim().trim_start_matches('v');
    let head = head.split(['-', '+']).next().unwrap_or(head);
    let mut parts: Vec<String> = head
        .split('.')
        .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .collect();
    // A bare commit sha (`--always` when no tag exists) carries no version.
    if parts.len() < 2 || parts[0].is_empty() {
        return env!("CARGO_PKG_VERSION").to_string();
    }
    for p in parts.iter_mut() {
        if p.is_empty() {
            p.push('0');
        }
    }
    while parts.len() < 3 {
        parts.push("0".to_string());
    }
    parts.truncate(3);
    parts.join(".")
}

/// The port reports its own identity in the user agent; the PHP uses
/// `pencarimovie-server/<version>` and the WordPress logs show it.
pub fn user_agent() -> String {
    format!("pencarimovie-server/{} (rust)", app_version())
}

pub fn imdb_aka_api_base() -> String {
    env_or("FD_IMDB_AKA_API_BASE", "https://pencarimovie.com/imdb-api")
}

pub fn debug_from_env() -> bool {
    for k in ["DEBUG", "DEBUG_MODE", "PM_DEBUG"] {
        if let Some(v) = env(k) {
            let s = v.trim().to_ascii_lowercase();
            if s == "1" || s == "true" || s == "on" || s == "yes" {
                return true;
            }
        }
    }
    false
}

pub const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024;
