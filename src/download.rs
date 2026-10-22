//! Port of the download path: backend.php:1884 (payload decode), 11107-11126
//! (payload-from-path) and 19699-20360 (the `/api/download` handler), with
//! `downloadToBrowser()` replaced by grammers' `iter_download()`.
//!
//! The RETRY LADDER is the PHP's, not a simplification:
//! * `$maxDownloadAttempts = ($shortCode !== '') ? 4 : 1`;
//! * every reference failure re-resolves the short_code with the cache bypassed
//!   (`force=1&nocache=1`);
//! * when the fresh resolve returns the SAME id, walk the pool and try an
//!   alternate bot (`file reference expired, trying alternate bot from pool`);
//! * any of it only happens before the response body starts. Unlike the PHP we
//!   go one step further and only commit headers after the FIRST chunk arrives,
//!   so a failed first fetch can still answer a JSON error instead of leaving
//!   the client waiting on a `Content-Length` that never completes.
//!
//! Range handling: a Range start is rounded DOWN to 4096 and the leading bytes
//! are discarded; the chunk size is the largest `4096 * 2^k` (<= 512 KB) that
//! divides that offset and does not cross a 1 MB boundary, so every
//! `upload.getFile` call satisfies Telegram's alignment rules.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::{ReqCtx, is_authenticated};
use crate::log::log;
use crate::respond::json_response;
use crate::resolve::{
    invalidate_resolve_cache, resolve_shortcode, resolve_shortcode_cached,
    resolve_shortcode_concurrent, resolve_shortcode_fresh, save_resolve_cache,
};
use crate::session::{
    auto_provision_guest, get_bot_id, get_bot_pool, has_local_session,
    is_guest_provision_in_progress, pick_pool_bot,
};
use crate::storage::now;
use std::sync::OnceLock;

/// Live transfer counters, read by `/api/diagnostics` and used by the Android
/// app to drive its notification indicator and battery saver from real activity
/// (the TDLib app's `markActive()` / `ACTIVE_LOCK_TIMEOUT_MS` design) instead of
/// holding a wake lock forever.
static ACTIVE_STREAMS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ACTIVE_PREFLIGHTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn active_streams() -> usize {
    ACTIVE_STREAMS.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn active_preflights() -> usize {
    ACTIVE_PREFLIGHTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Decrements its counter when dropped, including on every early `return`
/// (client gone, Telegram error, task cancelled).
struct ActivityGuard(&'static std::sync::atomic::AtomicUsize);
impl ActivityGuard {
    fn new(c: &'static std::sync::atomic::AtomicUsize) -> Self {
        c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ActivityGuard(c)
    }
}
impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Cap on the pre-flight (resolve + retries + priming chunk) in seconds.
///
/// Nothing in this phase has been written to the client yet, so a player that
/// gives up leaves a request doing work for the whole retry chain: with the
/// backoff below that reached ~60 s per abandoned attempt, and a looping player
/// then had several of them running at once. An abandoned pre-flight is capped
/// so the work ends quickly; a real client is unaffected because a working
/// resolve finishes in ~1-3 s.
const PREFLIGHT_BUDGET_SECS: u64 = 20;

/// `file_id_mt` (preferred) or `file_id` from a resolve result, or `""`.
fn id_of(v: &Value) -> String {
    let a = jstr(v, "file_id_mt");
    if a.is_empty() {
        jstr(v, "file_id")
    } else {
        a
    }
}

/// How many bytes the FIRST response writes before it starts streaming.
///
/// A whole 1 MB window means the player waits for a megabyte before byte one
/// (~2.5 s at the throughput a phone sees), which reads as buffering after a
/// seek. 512 KB halves that wait and still costs exactly the SAME two parts for
/// window 0 (512 KB prime + 512 KB remainder), so steady-state throughput is
/// untouched — 256 KB was one part more per window 0 for no extra benefit.
const PRIME_BYTES: u64 = 512 * 1024;

/// How many 1 MB windows to keep in flight per stream.
///
/// Default **8**: that is the depth the PHP downloader measured at 7.5-9.7 MiB/s
/// (see the spool notes in the root `AGENTS.md`), and 4 was clearly too shallow —
/// a phone streaming over its own loopback managed only 0.3-0.7 MiB/s with 4.
/// `PM_PIPELINE=<n>` overrides it, clamped to 1..=32; the `--inflight` gate is a
/// separate bound on the real parts in flight, so a too-large value cannot open
/// an unbounded number of `upload.getFile` calls.
fn pipeline_depth() -> usize {
    static DEPTH: OnceLock<usize> = OnceLock::new();
    *DEPTH.get_or_init(|| {
        crate::storage::env("PM_PIPELINE")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map(|v| v.clamp(1, 32))
            .unwrap_or(8)
    })
}
use crate::telegram::{DocTarget, prepare_file_target};
use crate::AppState;

/* ── payload ─────────────────────────────────────────────────────────────── */

pub fn decode_download_payload(payload: &str) -> Value {
    let mut s = payload.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    match STANDARD.decode(s) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(|v| v.is_object())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        Err(_) => Value::Object(serde_json::Map::new()),
    }
}

/// `fd_extract_download_payload_from_path()`: skip a segment that is a media name.
pub fn extract_payload_from_path(path: &str) -> String {
    let rest = match path.strip_prefix("/api/download/") {
        Some(r) => r,
        None => return String::new(),
    };
    let segment = rest.split('/').next().unwrap_or("");
    if segment.is_empty() {
        return String::new();
    }
    let decoded = percent_decode(segment);
    let ext = decoded
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    const MEDIA: [&str; 19] = [
        "mp4", "m4v", "mkv", "webm", "avi", "mov", "ts", "m2ts", "flv", "wmv", "3gp", "mpg", "mpeg",
        "mp3", "m4a", "flac", "wav", "ogg", "opus",
    ];
    if MEDIA.contains(&ext.as_str()) {
        return String::new();
    }
    decoded
}

pub fn is_public_download_path(path: &str) -> bool {
    path == "/api/download" || path.starts_with("/api/download/")
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hexval(bytes[i + 1]), hexval(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/* ── Range ───────────────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

pub fn parse_range_header(header: &str, size: u64) -> Option<ByteRange> {
    if size == 0 {
        return None;
    }
    let spec = header.trim().strip_prefix("bytes=")?;
    let (raw_start, raw_end) = spec.split_once('-')?;
    let raw_start = raw_start.trim();
    let raw_end = raw_end.trim();
    if raw_start.is_empty() && raw_end.is_empty() {
        return None;
    }
    if raw_start.is_empty() {
        let n: u64 = raw_end.parse().ok()?;
        if n == 0 {
            return None;
        }
        let start = size.saturating_sub(n);
        return Some(ByteRange {
            start,
            end: size - 1,
        });
    }
    let start: u64 = raw_start.parse().ok()?;
    if start >= size {
        return None;
    }
    let mut end: u64 = if raw_end.is_empty() {
        size - 1
    } else {
        raw_end.parse().unwrap_or(size - 1)
    };
    if end >= size {
        end = size - 1;
    }
    if end < start {
        return None;
    }
    Some(ByteRange { start, end })
}

/* ── mime / filename helpers ─────────────────────────────────────────────── */

pub fn mime_by_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "ts" | "m2ts" => "video/mp2t",
        "flv" => "video/x-flv",
        "wmv" => "video/x-ms-wmv",
        "3gp" => "video/3gpp",
        "mpg" | "mpeg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        _ => return None,
    })
}

pub fn ext_by_mime(mime: &str) -> &'static str {
    match mime {
        "video/mp4" => "mp4",
        "video/x-matroska" => "mkv",
        "video/webm" => "webm",
        "video/x-msvideo" => "avi",
        "video/quicktime" => "mov",
        "video/mp2t" => "ts",
        "video/x-flv" => "flv",
        "video/mpeg" => "mpg",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/flac" => "flac",
        "audio/ogg" => "ogg",
        _ => "mp4",
    }
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// `fd_guess_video_mime()`.
pub fn guess_video_mime(file_name: &str, mime: &str) -> String {
    let m = mime.trim().to_ascii_lowercase();
    if !m.is_empty() && m.contains('/') && !m.contains(' ') {
        if m == "application/octet-stream" {
            return mime_by_ext(&ext_of(file_name))
                .unwrap_or("application/octet-stream")
                .to_string();
        }
        return m;
    }
    mime_by_ext(&ext_of(file_name))
        .unwrap_or("application/octet-stream")
        .to_string()
}

/// `fd_stremio_stream_filename()`.
pub fn stream_filename(file_name: &str, mime: &str) -> String {
    let mut name: String = file_name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\r' | '\n' => '_',
            other => other,
        })
        .collect();
    name = name.trim().to_string();
    if name.is_empty() {
        name = "video".to_string();
    }
    let ext = ext_of(&name);
    if mime_by_ext(&ext).is_none() {
        let want = ext_by_mime(&guess_video_mime(&name, mime));
        return format!("{}.{}", name, want);
    }
    name
}

/* ── handler ─────────────────────────────────────────────────────────────── */

fn jstr(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim().to_string()
}

fn jnum(v: &Value, key: &str) -> u64 {
    v.get(key)
        .and_then(|x| match x {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

/* ── diagnostics ─────────────────────────────────────────────────────────── */

/// Record a request the client is about to see fail.
///
/// `http_status` is the number the PLAYER renders verbatim — Nuvio shows
/// `HTTP 400 Bad Request [ERROR_CODE_IO_BAD_HTTP_STATUS]` and nothing else — so
/// it is what turns a screenshot back into a request. It is also the field the
/// old incidents never carried: the two **400** payload refusals below left no
/// trace at all, which is why "the stream source is blocked or restricted" was
/// unanswerable. The token is never stored; `token_url` is only a bool.
fn record_refusal(
    ctx: &ReqCtx,
    started: std::time::Instant,
    status: u16,
    stage: &str,
    message: &str,
    short_code: &str,
    extra: Value,
) {
    let mut info = serde_json::json!({
        "http_status": status,
        "stage": stage,
        "range": ctx.hdr("range"),
        "user_agent": ctx.hdr("user-agent"),
        "client_platform": ctx.hdr("x-client-platform"),
        "token_url": !crate::auth::auth_token_from_request(ctx).is_empty(),
        "tunneled": !ctx.hdr("cf-ray").is_empty(),
        "elapsed_ms": started.elapsed().as_millis() as u64,
    });
    if let (Some(dst), Some(src)) = (info.as_object_mut(), extra.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    crate::diagnostics::record_throttled(
        "playback_incident",
        &format!("download refused (HTTP {status})"),
        message,
        stage,
        short_code,
        info,
    );
    log(
        "download refused",
        &serde_json::json!({
            "http_status": status,
            "stage": stage,
            "short_code": short_code,
            "range": ctx.hdr("range"),
            "user_agent": ctx.hdr("user-agent"),
        }),
    );
}

pub async fn handle_download(state: &Arc<AppState>, ctx: &ReqCtx) -> Response {
    let started = std::time::Instant::now();
    if !is_authenticated(ctx) {
        record_refusal(ctx, started, 401, "AUTH_REQUIRED", "Server password or token required", "", serde_json::json!({}));
        return json_response(
            401,
            &serde_json::json!({"ok": 0, "message": "Password required", "auth_required": true}),
        );
    }

    let encoded_qs = ctx.qs("d");
    let mut encoded = if encoded_qs.is_empty() {
        extract_payload_from_path(&ctx.path)
    } else {
        encoded_qs
    };
    if encoded.is_empty() {
        encoded = extract_payload_from_path(&ctx.raw_path);
    }
    let payload = if encoded.is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        decode_download_payload(&encoded)
    };

    let mut file_id = {
        let a = jstr(&payload, "file_id_mt");
        if a.is_empty() { jstr(&payload, "file_id") } else { a }
    };
    if file_id.is_empty() {
        file_id = ctx.first_param(&["file_id_mt", "file_id"]);
    }
    let mut short_code = jstr(&payload, "short_code");
    if short_code.is_empty() {
        short_code = ctx.first_param(&["short_code", "sc"]);
    }
    if short_code.is_empty()
        && file_id.is_empty()
        && !encoded.is_empty()
        && payload.as_object().map(|o| o.is_empty()).unwrap_or(true)
    {
        short_code = encoded.clone();
    }
    let mut file_size = jnum(&payload, "file_size");
    if file_size == 0 {
        file_size = ctx.first_param(&["file_size", "size"]).parse().unwrap_or(0);
    }
    let mut file_name = {
        let a = jstr(&payload, "file_name");
        if a.is_empty() { ctx.first_param(&["file_name", "name"]) } else { a }
    };
    let mut file_mime = {
        let a = jstr(&payload, "mime");
        if a.is_empty() { ctx.first_param(&["mime", "mime_type"]) } else { a }
    };
    let mut bot_id = {
        let a = jstr(&payload, "bot_id");
        if a.is_empty() { ctx.first_param(&["bot_id"]) } else { a }
    };

    // Prefer the id an earlier request PROVED works, and take it EXACTLY as cached.
    //
    // This used to run only when the payload carried an id (`!file_id.is_empty()`).
    // Stream URLs built by this addon carry only the short_code, so the condition
    // was false for every real Play and each Range request resolved again — the
    // player then re-asks on every seek, so a single playback fired a resolve per
    // range. That is what the user saw as "while playing it keeps requesting a new
    // file_id", and against a busy origin (HTTP 503 on `/resolve-file`) it is what
    // stretched the first byte to 11.7 s.
    //
    // A cached id is used whether or not it has been verified: it came from a
    // successful resolve, and the FILE_REFERENCE_EXPIRED path below re-resolves
    // exactly once when Telegram disagrees. 6 h is shorter than the 2 h cache TTL
    // only for the *verified* preference; an unverified hit is still better than a
    // network round-trip.
    let eff_bot = if bot_id.is_empty() { get_bot_id() } else { bot_id.clone() };
    if !short_code.is_empty() && !eff_bot.is_empty() {
        if let Some(entry) = resolve_shortcode_cached(&short_code, &eff_bot) {
            let cached_id = {
                let a = jstr(&entry, "file_id_mt");
                if a.is_empty() { jstr(&entry, "file_id") } else { a }
            };
            // A Bot API file_id is minted FOR ONE BOT: another bot cannot download
            // with it (and the reference inside it is not interchangeable either).
            // The cache is already keyed `short_code:bot_id`, and this second check
            // rejects an entry that names a different bot than the one we are about
            // to use — belt and braces, because using a foreign id can only fail
            // later (FILE_REFERENCE_EXPIRED) after a wasted round-trip.
            let entry_bot = jstr(&entry, "bot_id");
            let same_bot = entry_bot.is_empty() || entry_bot == eff_bot;
            let verified = entry.get("verified_at").and_then(|v| v.as_i64()).unwrap_or(0);
            let fresh_enough = verified == 0 || now() - verified < 21600;
            if !cached_id.is_empty() && fresh_enough && same_bot {
                if cached_id != file_id {
                    log(
                        "download using the cached file_id (no re-resolve)",
                        &serde_json::json!({"short_code": short_code, "verified": verified > 0}),
                    );
                }
                file_id = cached_id;
                let fs = entry.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
                if file_size == 0 && fs > 0 {
                    file_size = fs;
                }
                if file_name.is_empty() {
                    let t = jstr(&entry, "title");
                    file_name = if t.is_empty() { jstr(&entry, "file_name") } else { t };
                }
                if file_mime.is_empty() {
                    let m = jstr(&entry, "mime");
                    file_mime = if m.is_empty() { jstr(&entry, "file_type") } else { m };
                }
            }
        }
    }

    log(
        "download request received",
        &serde_json::json!({
            "file_id_present": !file_id.is_empty(),
            "short_code_present": !short_code.is_empty(),
            "encoded_present": !encoded.is_empty(),
            "file_size": file_size,
            "file_name": file_name,
            "mime": file_mime,
            // PHP logs the UA, and without it a retry loop cannot be attributed:
            // a browser/WebView cannot demux Matroska and aborts on the container,
            // while ExoPlayer plays it. `range` tells an initial `bytes=0-` probe
            // from a seek, which is how a looping player is told apart from one
            // that is merely slow to start.
            "ua": ctx.hdr("user-agent"),
            "range": ctx.hdr("range"),
        }),
    );

    let mut active_bot = get_bot_id();
    if !has_local_session("") {
        match auto_provision_guest(&state.telegram).await {
            Ok(prov) => {
                active_bot = prov.bot_id.clone();
                if bot_id.is_empty() || !has_local_session(&bot_id) {
                    bot_id = active_bot.clone();
                }
            }
            Err(prov) => {
                let connecting = is_guest_provision_in_progress();
                return json_response(
                    if connecting { 503 } else { 403 },
                    &serde_json::json!({
                        "ok": 0,
                        "message": if connecting {
                            "Connecting guest bot in progress. Please refresh or try again in a few seconds.".to_string()
                        } else {
                            prov.error.unwrap_or_else(|| "Telegram Bot is not connected. Please connect your bot token in Settings to stream.".to_string())
                        },
                        "hint": if connecting {
                            "Guest bot session is initializing. Please refresh playback shortly."
                        } else {
                            "Open Settings and connect your bot token."
                        },
                        "short_code": short_code,
                        "bot_id": bot_id,
                    }),
                );
            }
        }
    }

    // A bot is usable when this instance already holds its session file, OR when
    // the environment supplies its token (`BOT_TOKENS`) and it can be logged in
    // here on demand. Without the second half a scaled-out deploy only ever uses
    // the bots whose session happens to live on the instance the request landed
    // on — measured on Heroku: 5 dynos, 5 independent pools, the added bot used
    // by one dyno in five.
    let usable = |id: &str| has_local_session(id) || crate::session::env_token_for(id).is_some();

    // A Bot API `file_id` belongs to the bot that MINTED it — its embedded
    // `file_reference` is not interchangeable — so rotating bots for a request
    // that already carries a file_id is guaranteed to fail. Measured on a pool
    // burst against one stream URL: 5 of 12 ranges returned `410
    // FILE_REFERENCE_EXPIRED`, and every 410 body named a DIFFERENT bot than the
    // one the id had been resolved for. Rotation therefore only happens when we
    // still have to resolve (no `bot_id` in the payload), where `pick_pool_bot()`
    // is exactly what spreads the *resolves* over the pool, and the id it returns
    // is then bound to the bot that will use it.
    let bot_bound = !bot_id.is_empty();
    let mut candidates: Vec<String> = Vec::new();
    if !bot_bound && !short_code.is_empty() {
        if let Some(picked) = pick_pool_bot() {
            let id = jstr(&picked, "bot_id");
            if !id.is_empty() {
                candidates.push(id);
            }
        }
    }
    if !active_bot.is_empty() && !candidates.contains(&active_bot) && usable(&active_bot) {
        candidates.push(active_bot.clone());
    }
    if !bot_id.is_empty() && !candidates.contains(&bot_id) && usable(&bot_id) {
        candidates.push(bot_id.clone());
    }
    // Pool bots are only useful when the payload does not name one; with a
    // bound bot they would download with a foreign file_id and 410.
    if !bot_bound {
        for b in get_bot_pool() {
            let id = jstr(&b, "bot_id");
            if !id.is_empty() && !candidates.contains(&id) && usable(&id) {
                candidates.push(id);
            }
        }
    }
    if candidates.is_empty() {
        let fallback = if active_bot.is_empty() { bot_id.clone() } else { active_bot.clone() };
        candidates.push(fallback);
    }
    if bot_id.is_empty() || !has_local_session(&bot_id) {
        bot_id = candidates.first().cloned().unwrap_or_default();
    }

    if file_id.is_empty() && !short_code.is_empty() {
        let resolved = resolve_shortcode_concurrent(&short_code, &candidates).await;
        let candidate_id = {
            let a = jstr(&resolved, "file_id_mt");
            if a.is_empty() { jstr(&resolved, "file_id") } else { a }
        };
        let message = jstr(&resolved, "message");
        if candidate_id.is_empty() && message.to_ascii_lowercase().contains("not found for this request") {
            log(
                "download refused - WordPress does not have this short_code",
                &serde_json::json!({"short_code": short_code}),
            );
            return json_response(
                404,
                &serde_json::json!({
                    "ok": 0,
                    "message": message,
                    "short_code": short_code,
                    "bot_id": bot_id,
                    "retried_bots": candidates.len(),
                }),
            );
        }
        if !candidate_id.is_empty() {
            file_id = candidate_id;
            let rs = jnum(&resolved, "file_size");
            if rs > 0 {
                file_size = rs;
            }
            let name = {
                let t = jstr(&resolved, "title");
                if t.is_empty() { jstr(&resolved, "file_name") } else { t }
            };
            if !name.is_empty() {
                file_name = name;
            }
            let mime = {
                let m = jstr(&resolved, "mime");
                if m.is_empty() {
                    let ft = jstr(&resolved, "file_type");
                    if ft.is_empty() { file_mime.clone() } else { ft }
                } else {
                    m
                }
            };
            file_mime = guess_video_mime(&file_name, &mime);
            let rb = jstr(&resolved, "bot_id");
            if !rb.is_empty() {
                bot_id = rb;
            }
        }
    }

    if file_id.is_empty() {
        record_refusal(
            ctx, started, 400, "MISSING_FILE_ID",
            "file_id or valid short_code is required for local browser download.",
            &short_code,
            serde_json::json!({
                "payload_len": encoded.len(),
                "bot_id": bot_id,
                "pool_bots": candidates.len(),
            }),
        );
        return json_response(
            400,
            &serde_json::json!({
                "ok": 0,
                "message": "file_id or valid short_code is required for local browser download.",
                "hint": "Pass a Bot API file id or shortcode to resolve the file.",
            }),
        );
    }

    file_name = stream_filename(&file_name, &file_mime);
    file_mime = guess_video_mime(&file_name, &file_mime);

    if file_size == 0 || file_name.is_empty() || file_mime.is_empty() {
        record_refusal(
            ctx, started, 400, "MISSING_FILE_META",
            "For Bot API file_id download, file_size, file_name, and mime are required.",
            &short_code,
            serde_json::json!({
                "has_file_size": file_size > 0,
                "has_file_name": !file_name.is_empty(),
                "has_mime": !file_mime.is_empty(),
                "file_id_len": file_id.len(),
                "bot_id": bot_id,
            }),
        );
        return json_response(
            400,
            &serde_json::json!({
                "ok": 0,
                "message": "For Bot API file_id download, file_size, file_name, and mime are required.",
                "hint": "Include file_size, file_name, and mime from your PencariMovie metadata response.",
            }),
        );
    }

    /* ── global stream cap ──────────────────────────────────────────────── */
    // At most `--max-streams` (default 256) downloads run at once; the rest
    // QUEUE for up to 5 s and then answer 503 + Retry-After, so 400+ viewers
    // queue deterministically instead of all opening a Telegram part at once.
    //
    // `--max-streams 0` / `PM_MAX_STREAMS=0` disables the cap: the gate is sized
    // to `Semaphore::MAX_PERMITS`, so this acquire is free and never 503s.
    let stream_permit = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.download_gate.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => Some(permit),
        Ok(Err(_)) => None,
        Err(_) => {
            log(
                "download refused - max-streams gate saturated",
                &serde_json::json!({"max_streams": state.max_streams, "short_code": short_code}),
            );
            record_refusal(
                ctx, started, 503, "STREAM_GATE_SATURATED",
                "Server busy streaming other viewers. Please retry shortly.",
                &short_code,
                serde_json::json!({"max_streams": state.max_streams, "retry_after": 1}),
            );
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::RETRY_AFTER, "1")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "ok": 0,
                        "message": "Server busy streaming other viewers. Please retry shortly.",
                        "retry_after": 1,
                    }))
                    .unwrap_or_default(),
                ))
                .unwrap_or_else(|_| {
                    json_response(503, &serde_json::json!({"ok": 0, "retry_after": 1}))
                });
        }
    };

    /* ── pick a bot ─────────────────────────────────────────────────────── */
    let mut boot_err: Option<String> = None;
    let mut entry = None;
    let mut tried: Vec<String> = Vec::new();
    for candidate in std::iter::once(bot_id.clone())
        .chain(std::iter::once(active_bot.clone()))
        .chain(candidates.iter().cloned())
    {
        // `has_local_session` alone would skip an env-supplied bot on an instance
        // that has not logged it in yet, so accept it and pass its token through.
        let env_token = crate::session::env_token_for(&candidate);
        if candidate.is_empty()
            || (!has_local_session(&candidate) && env_token.is_none())
            || tried.contains(&candidate)
        {
            continue;
        }
        tried.push(candidate.clone());
        match state.telegram.boot(&candidate, env_token.as_deref()).await {
            Ok(e) => {
                bot_id = candidate;
                entry = Some(e);
                break;
            }
            Err(e) => {
                // A FLOOD_WAIT here must stop every other request from retrying the
                // login too — see `session::note_flood()`.
                crate::session::note_flood(&e);
                log("download boot failed for bot", &serde_json::json!({"bot_id": candidate, "error": e}));
                boot_err = Some(e);
            }
        }
    }
    if entry.is_none() {
        match auto_provision_guest(&state.telegram).await {
            Ok(prov) => {
                bot_id = prov.bot_id.clone();
                match state.telegram.boot(&prov.bot_id, None).await {
                    Ok(e) => entry = Some(e),
                    Err(e) => boot_err = Some(e),
                }
            }
            Err(prov) => boot_err = prov.error,
        }
    }
    let Some(entry) = entry else {
        // A Telegram login flood is retryable and must NOT look like "your bot is
        // gone": answer 429 + retry_after so a player backs off instead of
        // hammering the login (each retry re-arms FLOOD_WAIT).
        let flood = crate::session::flood_remaining();
        let status = if flood > 0 { 429 } else { 403 };
        let reason = if flood > 0 { "RATE_LIMITED" } else { "NO_SESSION" };
        record_refusal(
            ctx, started, status, reason,
            &boot_err.clone().unwrap_or_else(|| "No valid MadelineProto session.".to_string()),
            &short_code,
            serde_json::json!({
                "bots_tried": tried.len(),
                "boot_error": boot_err.clone().unwrap_or_default(),
                "flood_remaining": flood,
            }),
        );
        let mut body = serde_json::json!({
            "ok": 0,
            "message": boot_err.unwrap_or_else(|| "No valid MadelineProto session.".to_string()),
            "hint": if flood > 0 {
                "Telegram rate limited the bot login. This clears by itself; retry shortly."
            } else {
                "Call /api/botlogin first to authenticate."
            },
        });
        if flood > 0 {
            body["retry_after"] = serde_json::json!(flood);
        }
        return json_response(status, &body);
    };

    // Which bot actually serves this request. `pick_pool_bot()` round-robins over
    // the pool, so a burst must show several bot ids here — that is how streaming
    // load balancing is verified (group the lines by bot_id in the logs).
    log(
        "download served by bot",
        &serde_json::json!({
            "bot_id": bot_id,
            "short_code": short_code,
            "pool_size": candidates.len(),
            "range": ctx.hdr("range"),
        }),
    );

    file_name = stream_filename(&file_name, &file_mime);
    file_mime = guess_video_mime(&file_name, &file_mime);

    /* ── stream ─────────────────────────────────────────────────────────── */
    let range_header = ctx.hdr("range");
    let range = if range_header.is_empty() {
        None
    } else {
        parse_range_header(&range_header, file_size)
    };
    if !range_header.is_empty() && range.is_none() {
        record_refusal(
            ctx, started, 416, "BAD_RANGE",
            "The client sent a Range this file cannot satisfy.",
            &short_code,
            serde_json::json!({"range_header": range_header, "file_size": file_size}),
        );
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{}", file_size))
            .body(Body::empty())
            .unwrap_or_else(|_| json_response(500, &serde_json::json!({"ok": 0})));
    }
    let (start, end) = match range {
        Some(r) => (r.start, r.end),
        None => (0, file_size.saturating_sub(1)),
    };
    let total = end.saturating_sub(start) + 1;
    // Align the fetch base to **1 MB**, not 4096.
    //
    // `plan_limit(pos, MB)` picks the largest `4096 << k` that divides `pos` and
    // does not cross a 1 MB boundary. For a 4096-only-aligned `pos` that can
    // collapse to 4 KB, and `fetch_range_once()` then has to SKIP `pos / 4 KB`
    // chunks — for a seek to ~900 MB that is ~220,000 skipped parts, i.e. the
    // request hangs (measured: `bytes=900000000-` returned 0 bytes in 15 s).
    // 1 MB alignment makes every window start a legal full-size part, so a seek
    // costs at most one discarded chunk instead of a 220k-step walk.
    let aligned = (start / crate::telegram::MB) * crate::telegram::MB;

    let mut target: DocTarget = match prepare_file_target(&file_id, file_size) {
        Ok(t) => t,
        Err(e) => {
            record_refusal(
                ctx, started, 500, "BAD_FILE_ID", &e, &short_code,
                serde_json::json!({"file_id_len": file_id.len(), "file_size": file_size, "bot_id": bot_id}),
            );
            return json_response(500, &serde_json::json!({"ok": 0, "message": e}));
        }
    };

    // ALAC cannot be decoded by Android/Eclipse, so it is decoded here and
    // served as WAV — BEFORE the byte-for-byte path below (the PHP port
    // transcoded it to FLAC on disk instead; see `serve_alac`). `None` means
    // "not decodable after all", and the normal download runs unchanged.
    if is_alac_target(&file_name, &file_mime) {
        let (alac_client, alac_gate) = entry.pick_slot();
        if let Some(resp) = serve_alac(
            &alac_client,
            alac_gate.as_ref(),
            &target,
            file_size,
            &file_name,
            &range_header,
            &short_code,
            ctx.method.eq_ignore_ascii_case("HEAD"),
        )
        .await
        {
            return resp;
        }
    }

    // The in-flight gate belongs to the same connection as the client.
    let (mut client, mut client_gate) = entry.pick_slot();

    let max_attempts = if short_code.is_empty() { 1 } else { 4 };
    let mut first: Option<Vec<u8>> = None;
    let mut last_err: Option<crate::telegram::GramErr> = None;
    // Counts as activity for the app's indicator/battery saver while we resolve
    // and pull the priming chunk; dropped when this function returns (either the
    // stream takes over or we give up).
    let _preflight_activity = ActivityGuard::new(&ACTIVE_PREFLIGHTS);
    let preflight_deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(PREFLIGHT_BUDGET_SECS);

    // Prime with ONE chunk, never with `total`. `total` is the whole remaining
    // range, so a plain GET of a 4 GB file (which is what `<video>` sends on
    // startup — no Range header) tried to download 4 GB into memory before the
    // response headers were written: the player sat at `readyState 0 /
    // networkState 2` forever and Nuvio logged "still downloading data".
    // A Range request looked fine only because there `total` was already small.
    let prime = total.min(PRIME_BYTES);
    for attempt in 1..=max_attempts {
        if std::time::Instant::now() >= preflight_deadline {
            log(
                "pre-flight budget exhausted - giving up before the client has data",
                &serde_json::json!({"short_code": short_code, "attempt": attempt, "budget_s": PREFLIGHT_BUDGET_SECS}),
            );
            break;
        }
        match crate::telegram::fetch_range_gated(&client, client_gate.as_ref(), &target, aligned, prime)
            .await
        {
            Ok(buf) => {
                first = Some(buf);
                break;
            }
            Err(e) => {
                log(
                    "download attempt failed",
                    &serde_json::json!({
                        "attempt": attempt,
                        "max_attempts": max_attempts,
                        "error": e.message,
                        "short_code": short_code,
                    }),
                );
                let file_ref = e.file_reference;
                last_err = Some(e);
                if file_ref && !short_code.is_empty() {
                    invalidate_resolve_cache(&short_code, &bot_id);
                    log(
                        "file reference expired, re-resolving short_code with nocache",
                        &serde_json::json!({"short_code": short_code, "attempt": attempt}),
                    );
                    let mut fresh = resolve_shortcode_fresh(&short_code, &bot_id).await;
                    let mut new_id = id_of(&fresh);
                    // Keep any `retry_after` seen along the way: the FORCED resolve
                    // is the one that answers "busy" and carries it, and the plain
                    // fallback below overwrites `fresh` with a 200 that has none.
                    let mut retry_after = jnum(&fresh, "retry_after");
                    // The FORCED resolve can fail upstream while the plain one still
                    // works. Measured on the phone: every
                    // `resolve-file?…&force=1&nocache=1` answered **HTTP 503** while
                    // `/api/resolve-shortcode` for the same short_code returned a
                    // valid file_id, so all four attempts re-tried the same expired
                    // reference and playback died with
                    // `410 FILE_REFERENCE_EXPIRED`. A stale reference must never be
                    // retried while a working resolve exists.
                    if new_id.is_empty() || new_id == file_id {
                        log(
                            "forced resolve gave no new id - falling back to the plain resolve",
                            &serde_json::json!({"short_code": short_code, "bot_id": bot_id}),
                        );
                        fresh = resolve_shortcode(&short_code, &bot_id, false).await;
                        new_id = id_of(&fresh);
                        retry_after = retry_after.max(jnum(&fresh, "retry_after"));
                    }
                    // Last resort: the payload may name a bot whose session no longer
                    // exists (a reinstall wipes it). The ACTIVE bot can still resolve.
                    if new_id.is_empty() || new_id == file_id {
                        let active = get_bot_id();
                        if !active.is_empty() && active != bot_id {
                            log(
                                "resolve still stale - trying the active bot",
                                &serde_json::json!({"short_code": short_code, "active_bot_id": active}),
                            );
                            fresh = resolve_shortcode(&short_code, &active, false).await;
                            new_id = id_of(&fresh);
                            retry_after = retry_after.max(jnum(&fresh, "retry_after"));
                        }
                    }
                    if !new_id.is_empty() && new_id != file_id {
                        file_id = new_id;
                        let fs = jnum(&fresh, "file_size");
                        if fs > 0 {
                            file_size = fs;
                        }
                        match prepare_file_target(&file_id, file_size) {
                            Ok(t) => target = t,
                            Err(err) => {
                                record_refusal(
                                    ctx, started, 500, "BAD_FILE_ID_AFTER_RESOLVE", &err, &short_code,
                                    serde_json::json!({"file_size": file_size, "bot_id": bot_id, "attempt": attempt}),
                                );
                                return json_response(500, &serde_json::json!({"ok": 0, "message": err}));
                            }
                        }
                        let fb = jstr(&fresh, "bot_id");
                        if !fb.is_empty() && fb != bot_id && has_local_session(&fb) {
                            bot_id = fb;
                            if let Ok(e) = state.telegram.boot(&bot_id, None).await {
                                let (c, g) = e.pick_slot();
                                client = c;
                                client_gate = g;
                            }
                        }
                        continue;
                    }
                    // Same id back: walk the pool for a bot whose reference is valid.
                    let mut switched = false;
                    for alt_bot in candidates.clone() {
                        if alt_bot.is_empty() || alt_bot == bot_id || !has_local_session(&alt_bot) {
                            continue;
                        }
                        let alt = resolve_shortcode(&short_code, &alt_bot, true).await;
                        let alt_id = {
                            let a = jstr(&alt, "file_id_mt");
                            if a.is_empty() { jstr(&alt, "file_id") } else { a }
                        };
                        if alt_id.is_empty() || alt_id == file_id {
                            continue;
                        }
                        let Ok(alt_entry) = state.telegram.boot(&alt_bot, None).await else {
                            continue;
                        };
                        bot_id = alt_bot.clone();
                        file_id = alt_id;
                        let fs = jnum(&alt, "file_size");
                        if fs > 0 {
                            file_size = fs;
                        }
                        if let Ok(t) = prepare_file_target(&file_id, file_size) {
                            target = t;
                        }
                        let (c, g) = alt_entry.pick_slot();
                        client = c;
                        client_gate = g;
                        log(
                            "file reference expired, trying alternate bot from pool",
                            &serde_json::json!({"short_code": short_code, "alt_bot_id": alt_bot}),
                        );
                        switched = true;
                        break;
                    }
                    let _ = switched;
                    // The plugin refuses a refresh with
                    // `503 {"ok":0,"message":"Server busy resolving file, please
                    // retry shortly.","retry_after":5}` when its relay is busy.
                    // Four attempts fired back-to-back all land inside that same
                    // window (measured: 4 × 503 in ~20 s, then
                    // `410 FILE_REFERENCE_EXPIRED`), so the wait it asks for is
                    // the difference between "plays" and "this title is broken".
                    if attempt < max_attempts {
                        // http_json() drops the body of a non-2xx, so the plugin's
                        // own `retry_after` (5 s) is usually lost — fall back to a
                        // short linear backoff. Without it the four attempts run
                        // back-to-back (~2 s apart, measured) inside one "Server
                        // busy resolving file" window and the stream dies with 410.
                        let secs = if retry_after > 0 {
                            retry_after.clamp(1, 10)
                        } else {
                            (attempt as u64).clamp(1, 4)
                        };
                        log(
                            "resolve busy upstream - backing off before the next attempt",
                            &serde_json::json!({"short_code": short_code, "wait_s": secs, "attempt": attempt}),
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    }
                    continue;
                }
                // Not a reference error: re-boot the session and retry.
                if let Ok(e) = state.telegram.boot(&bot_id, None).await {
                    let (c, g) = e.pick_slot();
                    client = c;
                    client_gate = g;
                }
            }
        }
    }

    let Some(first_buf) = first else {
        let err = last_err.unwrap_or(crate::telegram::GramErr {
            message: "download failed".to_string(),
            ..Default::default()
        });
        log(
            "download failed",
            &serde_json::json!({
                "error": err.message,
                "short_code": short_code,
                "bot_id": bot_id,
                "flood_wait": err.flood_wait,
            }),
        );
        // A rate limit is a 429, never a 5xx/404: a client reads those as fatal
        // and never retries (root AGENTS.md: "A rate limit must answer 429").
        if let Some(secs) = err.flood_wait {
            let retry_after = secs.max(1);
            record_refusal(
                ctx,
                started,
                429,
                "RATE_LIMITED",
                &err.message,
                &short_code,
                serde_json::json!({"bot_id": bot_id, "retry_after": retry_after}),
            );
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::RETRY_AFTER, retry_after.to_string())
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "ok": 0,
                        "message": "Upstream Telegram rate limit, please retry shortly.",
                        "retry_after": retry_after,
                        "short_code": short_code,
                        "bot_id": bot_id,
                    }))
                    .unwrap_or_default(),
                ))
                .unwrap_or_else(|_| {
                    json_response(
                        429,
                        &serde_json::json!({
                            "ok": 0,
                            "message": "Upstream Telegram rate limit, please retry shortly.",
                            "retry_after": retry_after,
                        }),
                    )
                });
        }
        let status = if err.file_reference { 410 } else { 500 };
        let reason = if err.file_reference { "FILE_REFERENCE_EXPIRED" } else { "DOWNLOAD_FAILED" };
        record_refusal(
            ctx,
            started,
            status,
            reason,
            &err.message,
            &short_code,
            serde_json::json!({"bot_id": bot_id, "flood_wait": err.flood_wait}),
        );
        return json_response(
            status,
            &serde_json::json!({
                "ok": 0,
                "message": err.message,
                "short_code": short_code,
                "bot_id": bot_id,
            }),
        );
    };

    /* ── commit headers, then stream ────────────────────────────────────── */
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    let target_for_task = target.clone();
    let client_for_task = client.clone();
    let gate_for_task = client_gate.clone();
    let short_code_for_task = short_code.clone();
    let bot_for_task = bot_id.clone();
    let file_size_for_task = file_size;
    let persist_id = file_id.clone();
    // Captured BEFORE the spawn: the streaming task cannot borrow `ctx`, and a
    // mid-body failure is only diagnosable with the Range it was serving.
    let status_code: u16 = if range.is_some() { 206 } else { 200 };
    let client_ctx = serde_json::json!({
        "http_status": status_code,
        "range": range_header,
        "user_agent": ctx.hdr("user-agent"),
        "client_platform": ctx.hdr("x-client-platform"),
        "file_size": file_size,
        "mime": file_mime,
    });
    tokio::spawn(async move {
        // Held for the whole body, so `--max-streams` covers the live stream and
        // is released when this task ends (completed, aborted, or client gone).
        let _stream_permit = stream_permit;
        // Reported to the app as "streaming" activity for the whole body.
        let _activity = ActivityGuard::new(&ACTIVE_STREAMS);
        // PIPELINE the chunks: fetch up to `pipeline_depth()` 1 MB windows
        // concurrently and emit them IN ORDER (`StreamExt::buffered`). Fetching one
        // chunk at a time made every megabyte pay a full Telegram round-trip —
        // measured ~870 KiB/s, below what a 1080p stream needs, so the player sat at
        // `readyState 2 / networkState 2` and reported a playback stall.
        //
        // The DEPTH is the throughput knob, and it is per-stream: measured on a
        // phone (Android 16, loopback so the Wi-Fi is out of the picture) a single
        // 8 MB range ran at 0.3-0.7 MiB/s with 4 windows, while the PHP port
        // measured 7.5-9.7 MiB/s with 8 parallel chunks. The per-connection
        // `--inflight` gate still bounds the real parts in flight, so raising this
        // fills the pipe rather than raising load. `PM_PIPELINE` overrides it
        // (1..=32) for tuning without a rebuild.
        let pipeline = pipeline_depth();
        use futures_util::StreamExt as _;

        let mut skip = start.saturating_sub(aligned) as usize;
        let mut remaining = total;
        let primed = first_buf;

        // Emit the primed head BEFORE planning: it is the bytes that were pulled
        // while the headers were being written, and the player can start on them
        // immediately instead of waiting for a whole window.
        let plen = primed.len() as u64;
        if plen > 0 && remaining > 0 {
            if (skip as u64) < plen {
                let at = skip;
                let take = ((plen - at as u64) as usize).min(remaining as usize);
                if take > 0 {
                    let out = Bytes::copy_from_slice(&primed[at..at + take]);
                    if tx.send(Ok(out)).await.is_err() {
                        return;
                    }
                    remaining -= take as u64;
                }
                skip = 0;
            } else {
                skip -= plen as usize;
            }
        }

        // Windows continue from the end of the primed piece. When that piece is
        // shorter than a 1 MB window the REST OF THAT WINDOW is fetched first, so
        // every later window starts on an exact 1 MB boundary (full-size legal
        // parts, unchanged throughput); only the head of the body is finer.
        let mut plan: Vec<(u64, u64)> = Vec::new();
        {
            let mut p = aligned + plen;
            // `+ skip`: the windows start at an ALIGNED base, so they must cover
            // the bytes discarded before `start` as well. Without it every seek
            // answered `total - skip` bytes — up to 1 MB SHORT of the
            // Content-Length we just sent, and a player reads a short body as a
            // truncated stream (the "buffers after fast-forward" symptom).
            let mut rem = remaining + skip as u64;
            let mb = crate::telegram::MB;
            if rem > 0 && plen > 0 && plen < mb {
                let want = (mb - plen).min(rem);
                plan.push((p, want));
                p += want;
                rem -= want;
            }
            while rem > 0 {
                let want = rem.min(mb);
                plan.push((p, want));
                p += want;
                rem -= want;
            }
        }

        let stream = futures_util::stream::iter(plan.into_iter().enumerate().map(|(i, (p, w))| {
            let client = client_for_task.clone();
            let gate = gate_for_task.clone();
            let target = target_for_task.clone();
            async move {
                let res =
                    crate::telegram::fetch_range_gated(&client, gate.as_ref(), &target, p, w).await;
                (i, res)
            }
        }))
        .buffered(pipeline);

        futures_util::pin_mut!(stream);
        loop {
            // A seek or a closed tab drops the receiver. Detecting it only when
            // the next WINDOW is ready to send means up to `pipeline_depth`
            // megabytes of Telegram parts keep holding their `--inflight`
            // permits while the request the player just opened queues behind
            // them (seen on a phone as ttfb 1.0-5.2 s with a second stream
            // open). `is_closed()` is a plain non-blocking check — deliberately
            // NOT a `select!`, which could end a body while the client is still
            // reading and make playback look slow instead.
            if tx.is_closed() {
                return;
            }
            let Some((index, result)) = stream.next().await else {
                break;
            };
            let data = match result {
                Ok(d) => d,
                Err(e) => {
                        let mut info = client_ctx.clone();
                        info["bytes_sent"] = serde_json::json!(total.saturating_sub(remaining));
                        info["bytes_total"] = serde_json::json!(total);
                        info["bot_id"] = serde_json::json!(bot_for_task);
                        log(
                            "download stream aborted mid-body",
                            &serde_json::json!({
                                "error": e.message,
                                "short_code": short_code_for_task,
                                "bytes_sent": total.saturating_sub(remaining),
                            }),
                        );
                        crate::diagnostics::record_throttled(
                            "playback_incident",
                            "stream aborted",
                            &e.message,
                            "DOWNLOAD_FAILED",
                            &short_code_for_task,
                            info,
                        );
                        return;
                    }
            };
            let _ = index;
            let slice_start = skip.min(data.len());
            let take = (data.len() - slice_start).min(remaining as usize);
            if take > 0 {
                let out = Bytes::copy_from_slice(&data[slice_start..slice_start + take]);
                if tx.send(Ok(out)).await.is_err() {
                    // Client disconnected: stop pulling from Telegram.
                    return;
                }
                remaining -= take as u64;
            }
            skip = 0;
            if remaining == 0 {
                break;
            }
        }
        // Persist the id that worked so the next Range starts from a live
        // reference (backend.php writes `verified_at` for the same reason).
        if !short_code_for_task.is_empty() && !bot_for_task.is_empty() {
            let mut merged = resolve_shortcode_cached(&short_code_for_task, &bot_for_task)
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            if !merged.is_object() {
                merged = Value::Object(serde_json::Map::new());
            }
            merged["file_id_mt"] = Value::String(persist_id);
            merged["file_size"] = serde_json::json!(file_size_for_task);
            merged["verified_at"] = serde_json::json!(now());
            save_resolve_cache(&short_code_for_task, &bot_for_task, &merged);
        }
    });

    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK))
        .header(header::CONTENT_TYPE, if file_mime.is_empty() { "application/octet-stream" } else { &file_mime })
        .header(header::CONTENT_LENGTH, total.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename*=UTF-8''{}", percent_encode(&file_name)),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            "Access-Control-Expose-Headers",
            "Accept-Ranges, Content-Range, Content-Length, Content-Type",
        )
        .header("X-Accel-Buffering", "no");
    if range.is_some() {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, file_size));
    }
    let stream = ReceiverStream::new(rx).map(|item| match item {
        Ok(b) => Ok::<Bytes, std::io::Error>(b),
        Err(e) => Err(e),
    });
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| json_response(500, &serde_json::json!({"ok": 0, "message": "response build failed"})))
}

/* ── ALAC → WAV PCM ─────────────────────────────────────────────────────── */

/// Is this file an ALAC/`.m4a` music track (as opposed to a video)?
///
/// A **video** MP4 must never be routed into the audio decoder, so a `video/*`
/// mime only qualifies when the file is explicitly named `.m4a`/`.alac`; the
/// `audio/` family check is what keeps `<video>` playback on the byte-for-byte
/// path. `audio::probe()` still has the final say: an AAC `.m4a` has no decoder
/// in this build, so it falls through and streams raw exactly as before.
pub fn is_alac_target(file_name: &str, mime: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    if name.ends_with(".m4a") || name.ends_with(".alac") {
        return true;
    }
    let mime = mime.to_ascii_lowercase();
    mime.starts_with("audio/") && (mime.contains("mp4") || mime.contains("m4a") || mime.contains("alac"))
}

fn replace_ext(name: &str, ext: &str) -> String {
    let base = name.rsplit_once('.').map(|(a, _)| a).unwrap_or(name);
    format!("{}.{}", base, ext)
}

/// Serve an ALAC track as decoded 16-bit PCM behind a WAV header.
///
/// The whole `.m4a` is pulled from Telegram into memory (1 MB per batch; music
/// is typically < 20 MB) and decoded from a `Cursor`, so nothing is written to
/// `storage/` — no cache file, no conversion lock, no second decoder, which is
/// what removes both traps of the PHP ALAC→FLAC pipeline.
///
/// Returns `None` when the file turns out not to be decodable (probe or fetch
/// failure) so the caller can run the ordinary download unchanged.
async fn serve_alac(
    client: &grammers_client::Client,
    gate: &tokio::sync::Semaphore,
    target: &DocTarget,
    file_size: u64,
    file_name: &str,
    range_header: &str,
    short_code: &str,
    head_only: bool,
) -> Option<Response> {
    let mut data: Vec<u8> = Vec::with_capacity(file_size.min(64 * crate::telegram::MB) as usize);
    let mut pos = 0u64;
    while pos < file_size {
        let want = (file_size - pos).min(crate::telegram::MB);
        match crate::telegram::fetch_range_gated(client, gate, target, pos, want).await {
            Ok(chunk) if !chunk.is_empty() => {
                pos += chunk.len() as u64;
                data.extend_from_slice(&chunk);
            }
            Ok(_) => break,
            Err(e) => {
                log(
                    "alac fetch failed, falling back to the raw download",
                    &serde_json::json!({
                        "error": e.message,
                        "pos": pos,
                        "file_size": file_size,
                        "short_code": short_code,
                    }),
                );
                return None;
            }
        }
    }
    if pos < file_size {
        log(
            "alac fetch incomplete, falling back to the raw download",
            &serde_json::json!({"have": pos, "file_size": file_size, "short_code": short_code}),
        );
        return None;
    }
    let info = match crate::audio::probe_why(&data) {
        Ok(info) => info,
        Err(why) => {
            log(
                "alac probe rejected the file, falling back to the raw download",
                &serde_json::json!({
                    "reason": why,
                    "file_name": file_name,
                    "file_size": file_size,
                    "short_code": short_code,
                }),
            );
            return None;
        }
    };

    let pcm_total = info.pcm_bytes();
    let total = info.total_bytes();
    let wav = crate::audio::wav_header(pcm_total, info.sample_rate, info.channels);

    // A Range addresses the WAV stream, header included: bytes [44, 44 + pcm)
    // are samples, so the first sample frame starts at 44.
    let range = if range_header.trim().is_empty() {
        None
    } else {
        match parse_range_header(range_header, total) {
            Some(r) => Some(r),
            None => {
                return Some(
                    Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header(header::CONTENT_RANGE, format!("bytes */{}", total))
                        .body(Body::empty())
                        .unwrap_or_else(|_| {
                            json_response(500, &serde_json::json!({"ok": 0}))
                        }),
                )
            }
        }
    };
    let (start, end) = match range {
        Some(r) => (r.start, r.end),
        None => (0, total.saturating_sub(1)),
    };

    // A HEAD has no body, so it is answered from the header alone: decoding the
    // whole track just to discard it would cost a full Telegram pull.
    let mut body: Vec<u8> = Vec::new();
    if !head_only {
        body.reserve((end.saturating_sub(start) + 1).min(32 * crate::telegram::MB) as usize);
        let mut cursor = start;
        // A range that starts inside the header emits the header prefix first.
        if cursor < crate::audio::WAV_HEADER_LEN {
            let stop = crate::audio::WAV_HEADER_LEN.min(end + 1);
            body.extend_from_slice(&wav[cursor as usize..stop as usize]);
            cursor = stop;
        }
        if cursor <= end {
            let from_pcm = cursor - crate::audio::WAV_HEADER_LEN;
            let want = (end - cursor + 1) as usize;
            let pcm = crate::audio::decode_range(&data, from_pcm, want);
            if pcm.is_empty() {
                log(
                    "alac decode produced no samples",
                    &serde_json::json!({
                        "short_code": short_code,
                        "from_pcm_byte": from_pcm,
                        "want": want,
                    }),
                );
            }
            body.extend_from_slice(&pcm);
        }
        if body.is_empty() {
            return Some(
                Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{}", total))
                    .body(Body::empty())
                    .unwrap_or_else(|_| json_response(500, &serde_json::json!({"ok": 0}))),
            );
        }
    }

    let status = if range.is_some() { 206 } else { 200 };
    let sent = if head_only {
        end.saturating_sub(start) + 1
    } else {
        body.len() as u64
    };
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
        .header(header::CONTENT_TYPE, "audio/wav")
        .header(header::CONTENT_LENGTH, sent.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "inline; filename*=UTF-8''{}",
                percent_encode(&replace_ext(file_name, "wav"))
            ),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            "Access-Control-Expose-Headers",
            "Accept-Ranges, Content-Range, Content-Length, Content-Type",
        );
    if status == 206 {
        // The body is what the decoder actually produced, so the advertised end
        // is derived from it rather than from the requested range: a short
        // decode must not promise bytes that were never written.
        let last = start + sent - 1;
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {}-{}/{}", start, last, total));
    }
    log(
        "alac served as wav",
        &serde_json::json!({
            "short_code": short_code,
            "sample_rate": info.sample_rate,
            "channels": info.channels,
            "n_frames": info.n_frames,
            "pcm_bytes": pcm_total,
            "range": range_header,
            "head_only": head_only,
            "sent": sent,
        }),
    );
    Some(builder.body(Body::from(body)).unwrap_or_else(|_| {
        json_response(500, &serde_json::json!({"ok": 0, "message": "response build failed"}))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_video_mime_when_omitted() {
        let name = "Sengkolo.Petaka.Satu.Suro.2026.1080p.MalaySub.WEB.MSM.mp4";
        let mime = guess_video_mime(name, "");
        assert_eq!(mime, "video/mp4");

        let mkv = "Movie.2026.mkv";
        assert_eq!(guess_video_mime(mkv, ""), "video/x-matroska");
    }

    #[test]
    fn stream_filename_and_mime_fallbacks() {
        let name = stream_filename("video", "video/mp4");
        assert_eq!(name, "video.mp4");
        assert_eq!(guess_video_mime(&name, ""), "video/mp4");
    }
}
