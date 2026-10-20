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

pub const WP_AJAX_URL: &str = "https://pencarimovie.com/wp-admin/admin-ajax.php";

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
