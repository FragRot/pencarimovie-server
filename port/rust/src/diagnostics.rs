//! Port of the `diagnostics` group, backend.php:11293-11654:
//! `fd_diagnostics_settings_path/settings`, `fd_diagnostics_auto_report_enabled`,
//! `fd_diagnostics_set_auto_report`, `fd_reports_dir`, `fd_device_info`,
//! `fd_diag_prune`, `fd_diag_record`, `fd_diag_list`, `fd_diag_counts`,
//! `fd_diag_clear`.
//!
//! Same storage paths and file naming (`<type>_<unix_ms>.json`, 10 per type),
//! same category names. The uploader (`fd_diag_report_send` / flush) is NOT
//! ported — the port writes locally and reports `ok: 0` for a send, which is
//! listed in PARITY.md as a gap rather than faked.

use std::fs;
use std::path::PathBuf;

use rand::RngCore;
use serde_json::Value;

use crate::config::app_version;
use crate::storage::{now, storage_path};

pub const REPORT_TYPES: [&str; 5] = [
    "crash",
    "anr",
    "stream_incident",
    "playback_incident",
    "self_test",
];

pub fn settings_path() -> PathBuf {
    storage_path("diagnostics.json")
}

pub fn settings() -> Value {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

pub fn auto_report_enabled() -> bool {
    match settings().get("auto_report") {
        Some(v) => v.as_bool().unwrap_or(true),
        None => true,
    }
}

pub fn set_auto_report(enabled: bool) -> Value {
    let mut s = settings();
    if !s.is_object() {
        s = serde_json::json!({});
    }
    s["auto_report"] = serde_json::json!(enabled);
    let _ = fs::write(settings_path(), s.to_string());
    s
}

pub fn reports_dir() -> PathBuf {
    let dir = storage_path("reports");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn device_id_path() -> PathBuf {
    storage_path("device_id.txt")
}

pub fn get_device_id() -> String {
    if let Ok(v) = fs::read_to_string(device_id_path()) {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return v;
        }
    }
    let mut buf = [0u8; 6];
    rand::rng().fill_bytes(&mut buf);
    let id = hex::encode(buf);
    let _ = fs::write(device_id_path(), &id);
    id
}

fn read_first_line(path: &str) -> String {
    // `read_to_string` fails outright on a file with one invalid byte, and some
    // `/proc` entries (Android's `/proc/version`) are not guaranteed UTF-8.
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default()
}

/// `ro.product.model=…` out of an Android `build.prop` (pure, testable).
///
/// Only the first `=` splits, values may contain spaces, and comments/blank
/// lines are skipped — the real file has all three.
pub fn build_prop_lookup(text: &str, key: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

/// Android's `/proc` is heavily restricted and `uname` fields are useless for a
/// support report, so read the same `build.prop` the PHP/Kotlin apps read.
/// Every step degrades to an empty string: this must never fail a request.
fn android_build_prop() -> String {
    for path in ["/system/build.prop", "/vendor/build.prop", "/product/build.prop"] {
        if let Ok(bytes) = fs::read(path) {
            return String::from_utf8_lossy(&bytes).to_string();
        }
    }
    String::new()
}

fn android_model_and_version() -> (String, String) {
    let prop = android_build_prop();
    if prop.is_empty() {
        return (String::new(), String::new());
    }
    let model = build_prop_lookup(&prop, "ro.product.model");
    let release = build_prop_lookup(&prop, "ro.build.version.release");
    let sdk = build_prop_lookup(&prop, "ro.build.version.sdk");
    let version = if release.is_empty() {
        String::new()
    } else if sdk.is_empty() {
        format!("Android {}", release)
    } else {
        format!("Android {} (API {})", release, sdk)
    };
    (model, version)
}

fn proc_value(path: &str, key: &str) -> u64 {
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            let num: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            return num.parse().unwrap_or(0);
        }
    }
    0
}

/// `fd_device_info()` (backend.php:11339) with `/proc` metrics.
///
/// **Android**: every `/proc` read here degrades to `0`/`""` because a
/// non-rooted app sees a stripped `/proc` (`/proc/sys/kernel/hostname` is not
/// readable, `/proc/self/status` and `/proc/self/fd` are). That is deliberate —
/// a diagnostics report must never fail because a counter is missing — and the
/// model/OS fields come from `build.prop` instead, which IS readable.
pub fn device_info() -> Value {
    let mem_total_kb = proc_value("/proc/meminfo", "MemTotal");
    let mem_avail_kb = proc_value("/proc/meminfo", "MemAvailable");
    let rss_kb = proc_value("/proc/self/status", "VmRSS");
    let uptime_s = {
        let raw = read_first_line("/proc/uptime");
        raw.split('.').next().unwrap_or("0").parse::<u64>().unwrap_or(0)
    };
    let fd_count = fs::read_dir("/proc/self/fd").map(|d| d.count()).unwrap_or(0);
    let (model, os_version) = if cfg!(target_os = "android") {
        android_model_and_version()
    } else {
        (
            read_first_line("/proc/sys/kernel/osrelease"),
            read_first_line("/proc/version"),
        )
    };
    serde_json::json!({
        "device_id": get_device_id(),
        "platform": std::env::consts::OS,
        "model": model,
        "brand": std::env::consts::ARCH,
        "os_version": os_version,
        "app_version": app_version(),
        "hostname": read_first_line("/proc/sys/kernel/hostname"),
        "total_memory_mb": mem_total_kb / 1024,
        "free_memory_mb": mem_avail_kb / 1024,
        "process_rss_mb": rss_kb / 1024,
        "open_fds": fd_count,
        "uptime_s": uptime_s,
        "runtime": "rust",
    })
}

/// `fd_diag_prune()`: keep at most 10 files per type.
pub fn prune() {
    let dir = reports_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut by_type: std::collections::HashMap<String, Vec<(i64, PathBuf)>> = std::collections::HashMap::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let base = name.trim_end_matches(".json").to_string();
        let ty = match base.rsplit_once('_') {
            Some((t, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => t.to_string(),
            _ => continue,
        };
        let m = crate::storage::mtime_secs(&e.path()).unwrap_or(0);
        by_type.entry(ty).or_default().push((m, e.path()));
    }
    for (_, mut files) in by_type {
        files.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, path) in files.into_iter().skip(10) {
            let _ = fs::remove_file(path);
        }
    }
}

/// `fd_diag_record()`: write one incident locally.
pub fn record(
    report_type: &str,
    title: &str,
    message: &str,
    stack: &str,
    reason: &str,
    short_code: &str,
    item_id: &str,
    extra: Value,
) -> PathBuf {
    prune();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file = reports_dir().join(format!("{}_{}.json", report_type, ms));
    let mut payload = serde_json::json!({
        "report_type": report_type,
        "platform": "rust-port",
        "device_id": get_device_id(),
        "app_version": app_version(),
        "created_at": now(),
        "error_title": title,
        "error_message": message,
        "stack_trace": stack,
        "failure_reason": reason,
        "short_code": short_code,
        "item_id": item_id,
    });
    if let (Some(dst), Some(src)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    let _ = fs::write(&file, serde_json::to_string_pretty(&payload).unwrap_or_default());
    file
}

pub fn list() -> Vec<Value> {
    let dir = reports_dir();
    let mut out: Vec<Value> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                if let Ok(text) = fs::read_to_string(e.path()) {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        out.push(v);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let av = a.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);
        let bv = b.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);
        bv.cmp(&av)
    });
    out
}

/// `fd_diag_counts()`: `<type>_<digits>.json` -> `{ type: count }`.
/// NOTE: strip only the trailing `_<digits>.json`, never split on `_`.
pub fn counts() -> Value {
    let mut map = serde_json::Map::new();
    for t in REPORT_TYPES {
        map.insert(t.to_string(), serde_json::json!(0));
    }
    if let Ok(entries) = fs::read_dir(reports_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let base = match name.strip_suffix(".json") {
                Some(b) => b.to_string(),
                None => continue,
            };
            let ty = match base.rsplit_once('_') {
                Some((t, tail)) if !t.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => t.to_string(),
                _ => continue,
            };
            let cur = map.get(&ty).and_then(|v| v.as_i64()).unwrap_or(0);
            map.insert(ty, serde_json::json!(cur + 1));
        }
    }
    Value::Object(map)
}

/// `fd_diag_report_send()`: POST one report to WordPress and write the
/// `<file>.sent` sidecar on success.
///
/// **Idempotent on purpose** — re-uploading the same file inserts a second row
/// (the document id is `(time << 20) | rand`), which is the exact duplicate bug
/// documented for the Android `sendNow()`. A `.sent` sidecar short-circuits it.
pub async fn report_send(path: &std::path::Path) -> bool {
    let marker = std::path::PathBuf::from(format!("{}.sent", path.display()));
    if marker.is_file() {
        return true;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let url = format!(
        "{}/report-diagnostic",
        crate::config::wp_api_base().trim_end_matches('/')
    );
    let mut req = crate::httpc::client()
        .post(&url)
        .timeout(std::time::Duration::from_secs(12))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(text);
    let secret = crate::credentials::get_api_secret();
    if !secret.is_empty() {
        req = req.header("X-API-Secret", secret);
    }
    match req.send().await {
        Ok(r) if r.status().is_success() => {
            let _ = fs::write(&marker, b"");
            true
        }
        Ok(r) => {
            crate::log::log(
                "diag upload rejected",
                &serde_json::json!({"status": r.status().as_u16(), "file": path.display().to_string()}),
            );
            false
        }
        Err(e) => {
            crate::log::log(
                "diag upload failed",
                &serde_json::json!({"error": e.to_string(), "file": path.display().to_string()}),
            );
            false
        }
    }
}

/// `fd_diag_flush_pending()`: send every report that has no `.sent` sidecar.
/// Returns `(sent, failed)`.
pub async fn flush_pending() -> (usize, usize) {
    let dir = reports_dir();
    let mut sent = 0usize;
    let mut failed = 0usize;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if report_send(&path).await {
                sent += 1;
            } else {
                failed += 1;
            }
        }
    }
    if sent + failed > 0 {
        crate::log::log(
            "diag flush",
            &serde_json::json!({"sent": sent, "failed": failed}),
        );
    }
    (sent, failed)
}

pub fn clear() {
    if let Ok(entries) = fs::read_dir(reports_dir()) {
        for e in entries.flatten() {
            if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed real `build.prop` (Android 5.1 / API 22 emulator shape).
    const PROP: &str = "\
# begin build properties
ro.product.model=SM-A805N
ro.product.brand=samsung
ro.build.version.release=5.1.1
ro.build.version.sdk=22
ro.build.description=generic_x86-user 5.1.1 LMY48Z 30 test-keys
";

    #[test]
    fn reads_a_plain_build_prop_value() {
        assert_eq!(build_prop_lookup(PROP, "ro.product.model"), "SM-A805N");
        assert_eq!(build_prop_lookup(PROP, "ro.build.version.sdk"), "22");
    }

    #[test]
    fn build_prop_lookup_ignores_comments_and_absent_keys() {
        assert_eq!(build_prop_lookup(PROP, "# begin build properties"), "");
        assert_eq!(build_prop_lookup(PROP, "ro.not.there"), "");
        assert_eq!(build_prop_lookup("", "ro.product.model"), "");
    }

    #[test]
    fn build_prop_values_keep_their_spaces() {
        // `ro.build.description` contains spaces; only the FIRST `=` splits.
        let v = build_prop_lookup(PROP, "ro.build.description");
        assert_eq!(v, "generic_x86-user 5.1.1 LMY48Z 30 test-keys");
    }

    #[test]
    fn build_prop_handles_crlf_and_padding() {
        let crlf = "  ro.product.model = Pixel 7  \r\nro.build.version.release=14\r\n";
        assert_eq!(build_prop_lookup(crlf, "ro.product.model"), "Pixel 7");
        assert_eq!(build_prop_lookup(crlf, "ro.build.version.release"), "14");
    }

    #[test]
    fn device_info_never_panics_without_proc() {
        // Everything must degrade to a placeholder rather than fail - this is how
        // a diagnostics report survives Android's restricted /proc.
        let info = device_info();
        assert_eq!(info["runtime"], "rust");
        assert_eq!(info["platform"], std::env::consts::OS);
        assert!(info["device_id"].as_str().unwrap_or("").len() >= 6);
    }
}
