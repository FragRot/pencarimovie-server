//! Port of `fd_is_debug_enabled`, `fd_set_debug_enabled`, `fd_log_stderr` and
//! `fd_log` (backend.php:368-510).
//!
//! Same on-disk paths (`storage/debug_mode.txt`, `storage/debug.log`), same 5 MB
//! cap, same `[YYYY-MM-DD HH:MM:SS] [PencariMovie Server] <msg> <json>` line
//! shape. Timestamps are UTC (the PHP used the process timezone; the port has no
//! tz database, so it documents UTC instead of guessing).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;

use crate::config::{debug_from_env, MAX_LOG_SIZE};
use crate::storage::{limit_file_size, now, storage_path};

pub fn debug_toggle_path() -> PathBuf {
    storage_path("debug_mode.txt")
}

pub fn debug_log_path() -> PathBuf {
    storage_path("debug.log")
}

pub fn is_debug_enabled() -> bool {
    if debug_from_env() {
        return true;
    }
    for file in [
        storage_path("debug_mode.txt"),
        storage_path("storage/debug_mode.txt"),
    ] {
        if let Ok(v) = fs::read_to_string(&file) {
            let s = v.trim().to_ascii_lowercase();
            if s == "1" || s == "true" || s == "on" {
                return true;
            }
        }
    }
    false
}

pub fn set_debug_enabled(enabled: bool) {
    let file = debug_toggle_path();
    if enabled {
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&file, "1");
    } else {
        let _ = fs::remove_file(&file);
    }
}

/// Always-on stderr line (`fd_log_stderr`).
pub fn stderr_log(message: &str, ctx: Option<&Value>) {
    let suffix = match ctx {
        Some(v) if v.as_object().map(|o| !o.is_empty()).unwrap_or(false) => format!(" {}", v),
        _ => String::new(),
    };
    eprintln!("[PencariMovie] {}{}", message, suffix);
}

pub fn log(message: &str, ctx: &Value) {
    if !is_debug_enabled() {
        return;
    }
    let empty = ctx.as_object().map(|o| o.is_empty()).unwrap_or(true);
    let suffix = if empty {
        String::new()
    } else {
        format!(" {}", ctx)
    };
    let line = format!(
        "[{}] [PencariMovie Server] {}{}\n",
        stamp(now()),
        message,
        suffix
    );
    let path = debug_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    eprint!("[PencariMovie Server] {}{}\n", message, suffix);
    limit_file_size(&path, MAX_LOG_SIZE);
}

fn stamp(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// Howard Hinnant's `civil_from_days`, valid for any i64 day count.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_1970() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
    }

    #[test]
    fn known_timestamp() {
        // 2026-10-19T16:11:06Z
        assert_eq!(stamp(1792426266), "2026-10-19 16:11:06");
    }
}
