//! Route dispatch — the port of backend.php:14112-20527.
//!
//! Route bodies, response shapes, status codes and message strings follow the
//! PHP one-for-one, and the ORDER is the PHP's:
//! `/api/sub-proxy` -> `/api/auth/*` -> `/api/version` -> `/api/lan-ip` ->
//! `/api/tunnel/*` -> `/api/catalog-settings` -> `/api/media-ids` ->
//! `/api/validate-manifest` -> `/api/country` -> `/api/session` ->
//! `/api/clock-check` -> `/api/provision` -> `/api/bots*` -> [version gate] ->
//! `/api/diagnostics*` -> `/api/reports` -> `/api/debug-mode` -> `/api/logs` ->
//! `/api/botlogin` -> `/api/botlogout` -> `/api/proxy-stream` -> `/api/download`
//! -> `/api/resolve-shortcode` -> `/api/warmup-resolve` -> manifest/catalog/meta/
//! stream/eclipse.
//!
//! Auth gates exactly as the PHP: public = `/manifest.json` (+ `/nuvio`,
//! `/stremio`), `/catalog/*`, `/meta/*`; token-gated = `/stream/*`,
//! `/eclipse/stream/*`, `/api/download*`, `/api/media-ids`, `/api/logs`,
//! `/api/debug-mode`; privileged = `/api/auth/token/rotate`, `/api/auth/reset`,
//! `/api/tunnel/*`, `/api/diagnostics/settings` POST, `/api/reports?clear|send|flush`.
//! A locked `/stream` answer is a STREAM ITEM, never a 401.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use serde_json::Value;

use crate::auth::{
    ReqCtx, auth_client_ip, auth_enabled, auth_lockout_check, auth_privileged,
    auth_record_failure, auth_record_success, auth_rotate_token, auth_set_password,
    auth_token, auth_verify_password, is_authenticated, is_https_request, parse_cookies,
    require_privileged, strip_token_prefix, url_host_is_public,
};
use crate::config::{AUTH_DEFAULT_PASSWORD, app_version, wp_api_base, wp_version_url};
use crate::credentials::{clear_api_secret, get_api_secret};
use crate::diagnostics::{
    auto_report_enabled, clear as diag_clear, counts as diag_counts, device_info,
    get_device_id, list as diag_list, set_auto_report,
};
use crate::download::{
    guess_video_mime, handle_download, is_public_download_path, stream_filename,
};
use crate::httpc::{cached_wp_json, get_contents, http_json};
use crate::log::{is_debug_enabled, log, set_debug_enabled};
use crate::resolve::{
    resolve_shortcode, resolve_shortcode_concurrent, warmup_resolve_batch,
};
use crate::respond::{
    eclipse_locked_response, json_response, not_found_json, redirect, rtrim, stremio_json,
    stremio_locked_stream, text_response,
};
use crate::session::{
    add_pool_bot, auto_provision_guest, clear_session, clock_skew_message, get_bot_id, get_bot_pool,
    has_local_session, is_guest_provision_in_progress, load_session_meta, measure_clock_offset,
    remove_pool_bot, save_session_meta,
};
use crate::storage::{now, storage_path};
use crate::tunnel;
use crate::upstream;
use crate::AppState;

const MAX_BODY: usize = 4 * 1024 * 1024;

fn jstr(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim().to_string()
}

fn header_json(req: &Request<Body>) -> HashMap<String, String> {
    let _ = req;
    HashMap::new()
}

pub async fn handle(state: Arc<AppState>, req: Request<Body>) -> Response {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => bytes::Bytes::new(),
    };
    let body_text = String::from_utf8_lossy(&body_bytes).to_string();

    let raw_uri = parts.uri.clone();
    let raw_path = if raw_uri.path().is_empty() {
        "/".to_string()
    } else {
        raw_uri.path().to_string()
    };
    let query: Vec<(String, String)> = raw_uri
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    let cookies = parse_cookies(
        parts
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    );
    let remote = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_default();
    let method = parts.method.as_str().to_uppercase();

    let mut ctx = ReqCtx {
        method: method.clone(),
        path: raw_path.clone(),
        raw_path: raw_path.clone(),
        query,
        headers: parts.headers,
        cookies,
        remote,
        local: String::new(),
        base_url: String::new(),
    };
    ctx.base_url = base_url_of(&ctx, state.port);

    if method == "OPTIONS" {
        // Echo the headers the browser asks for. A fixed list made the Eclipse
        // Music web client (https://eclipsemusic.app -> here) fail every
        // preflight: it sends its own `X-Client-Platform` / `X-Client-Version`,
        // and Chrome refuses the real request when one of them is not allowed.
        // `Allow-Origin: *` means no credentials are involved, so echoing is safe.
        let requested = ctx
            .headers
            .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                "Content-Type, Authorization, X-API-Secret, X-Auth-Token, Range, X-Requested-With"
                    .to_string()
            });
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, OPTIONS, DELETE, HEAD",
            )
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, requested)
            .header(
                "Access-Control-Expose-Headers",
                "Accept-Ranges, Content-Range, Content-Length, Content-Type",
            )
            .header("Access-Control-Allow-Private-Network", "true")
            .header(header::ACCESS_CONTROL_MAX_AGE, "86400")
            .body(Body::empty())
            .unwrap_or_else(|_| json_response(500, &serde_json::json!({"ok": 0})));
    }

    ctx.path = strip_token_prefix(&ctx.path);
    crate::storage::prune_cache_files(false);
    let p = ctx.path.clone();
    let m = method.clone();

    /* ── /api/sub-proxy — subtitle fetch + SRT→VTT (backend.php:18321) ────── */
    if p == "/api/sub-proxy" && m == "GET" {
        let sub_url = ctx.qs("url");
        if sub_url.is_empty() || !(sub_url.starts_with("http://") || sub_url.starts_with("https://")) {
            return text_response(400, "text/plain; charset=utf-8", "URL is required".to_string());
        }
        if !url_host_is_public(&sub_url).await {
            return text_response(400, "text/plain; charset=utf-8", "URL is not permitted".to_string());
        }
        // 24 h cache, keyed on the subtitle url itself. The converted VTT is a
        // string, so it is stored as `{"vtt": "…"}` and served straight from the
        // cache — the old version wrote a `.vtt` file and re-read it every time.
        if let Some(hit) = crate::cache::get("sub_vtt", &sub_url) {
            if let Some(vtt) = hit.get("vtt").and_then(|v| v.as_str()) {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
                    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(Body::from(vtt.to_string()))
                    .unwrap_or_else(|_| json_response(500, &serde_json::json!({"ok": 0})));
            }
        }
        let raw = get_contents(&sub_url, 10, header_json(&req_for_headers())).await;
        let Some(raw) = raw else {
            return text_response(502, "text/plain; charset=utf-8", "Failed to fetch subtitle".to_string());
        };
        let mut vtt = raw.clone();
        if !raw.trim_start().starts_with("WEBVTT") {
            let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
            let converted = srt_timestamps_to_vtt(&normalized);
            vtt = format!("WEBVTT\n\n{}", converted.trim_start());
        }
        crate::cache::put(
            "sub_vtt",
            &sub_url,
            &serde_json::json!({ "vtt": vtt }),
            86_400,
            0,
        );
        return text_response(200, "text/vtt; charset=utf-8", vtt);
    }

    /* ── cache observability (local/privileged) ───────────────────────────── */
    // The cache is now one redb file plus an in-process map, i.e. far less
    // inspectable than a folder of JSON — these two routes replace that.
    if p == "/api/cache/stats" && m == "GET" {
        if let Some(resp) = crate::auth::require_privileged(&ctx) {
            return resp;
        }
        return json_response(200, &crate::cache::stats());
    }
    if p == "/api/cache/clear" && (m == "POST" || m == "GET") {
        if let Some(resp) = crate::auth::require_privileged(&ctx) {
            return resp;
        }
        let prefix = ctx.qs("prefix");
        let prefix = if prefix.is_empty() {
            None
        } else {
            Some(prefix.as_str())
        };
        let removed = crate::cache::clear(prefix);
        log(
            "cache cleared",
            &serde_json::json!({"prefix": prefix.unwrap_or("*"), "l1_removed": removed}),
        );
        return json_response(
            200,
            &serde_json::json!({"ok": 1, "prefix": prefix.unwrap_or("*"), "l1_removed": removed}),
        );
    }

    /* ── auth (backend.php:18369-18460) ───────────────────────────────────── */
    if p == "/api/auth/status" && m == "GET" {
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "enabled": auth_enabled(),
                "authenticated": is_authenticated(&ctx),
                "token": if auth_privileged(&ctx) { auth_token() } else { String::new() },
            }),
        );
    }

    if p == "/api/auth/login" && m == "POST" {
        let ip = auth_client_ip(&ctx);
        if let Some(wait) = auth_lockout_check(&ip) {
            if wait > 0 {
                let mut resp = json_response(
                    429,
                    &serde_json::json!({
                        "ok": 0,
                        "message": format!("Too many failed attempts. Try again in {}s. (Or reset via: pms reset-password)", wait),
                        "retryAfter": wait,
                        "locked": true
                    }),
                );
                resp.headers_mut()
                    .insert(header::RETRY_AFTER, header::HeaderValue::from_str(&wait.to_string()).unwrap());
                return resp;
            }
        }
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        let pw = input.get("password").and_then(|v| v.as_str()).unwrap_or("");
        if !auth_verify_password(pw) {
            let fail = auth_record_failure(&ip);
            if fail.locked {
                let mut resp = json_response(
                    429,
                    &serde_json::json!({
                        "ok": 0,
                        "message": format!("Too many failed attempts. Try again in {}s. (Or reset via: pms reset-password)", fail.retry_after),
                        "retryAfter": fail.retry_after,
                        "locked": true
                    }),
                );
                resp.headers_mut().insert(
                    header::RETRY_AFTER,
                    header::HeaderValue::from_str(&fail.retry_after.to_string()).unwrap(),
                );
                return resp;
            }
            return json_response(
                401,
                &serde_json::json!({
                    "ok": 0,
                    "message": format!("Invalid password. {} attempt(s) left before lockout.", fail.remaining),
                    "remainingBeforeLock": fail.remaining
                }),
            );
        }
        auth_record_success(&ip);
        let token = auth_token();
        let secure = if is_https_request(&ctx) { "; Secure" } else { "" };
        let cookie = format!(
            "pm_auth={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000{}",
            token, secure
        );
        let mut resp = json_response(200, &serde_json::json!({"ok": 1, "token": token}));
        if let Ok(v) = header::HeaderValue::from_str(&cookie) {
            resp.headers_mut().insert(header::SET_COOKIE, v);
        }
        return resp;
    }

    if p == "/api/auth/logout" && m == "POST" {
        let mut resp = json_response(200, &serde_json::json!({"ok": 1}));
        resp.headers_mut().insert(
            header::SET_COOKIE,
            header::HeaderValue::from_static("pm_auth=; Path=/; Max-Age=0"),
        );
        return resp;
    }

    if p == "/api/auth/password" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        let current = input.get("current").and_then(|v| v.as_str()).unwrap_or("");
        if !auth_verify_password(current) {
            return json_response(401, &serde_json::json!({"ok": 0, "message": "Current password is wrong"}));
        }
        let next = input.get("next").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if next.len() < 4 {
            return json_response(
                400,
                &serde_json::json!({"ok": 0, "message": "Password must be at least 4 characters"}),
            );
        }
        auth_set_password(&next);
        return json_response(200, &serde_json::json!({"ok": 1, "token": auth_token()}));
    }

    if p == "/api/auth/token/rotate" && m == "POST" {
        if let Some(resp) = require_privileged(&ctx) {
            return resp;
        }
        return json_response(200, &serde_json::json!({"ok": 1, "token": auth_rotate_token()}));
    }

    if p == "/api/auth/reset" && m == "POST" {
        if let Some(resp) = require_privileged(&ctx) {
            return resp;
        }
        auth_set_password(AUTH_DEFAULT_PASSWORD);
        let _ = std::fs::remove_file(storage_path("cache/auth_lockout.json"));
        return json_response(200, &serde_json::json!({"ok": 1, "token": auth_token()}));
    }

    /* ── lightweight public routes (no version gate) ──────────────────────── */
    if p == "/api/version" {
        return json_response(200, &check_version().await);
    }

    if p == "/api/lan-ip" {
        return json_response(
            200,
            &serde_json::json!({"ok": 1, "lan_ip": lan_ip(), "port": state.port}),
        );
    }

    // backend.php:18477-18493 — status also runs the watchdog, enable spawns
    // cloudflared, disable kills it and clears the state (keeping the token).
    if p == "/api/tunnel/status" && m == "GET" {
        let mut status = tunnel::get_tunnel_status(state.port).await;
        let starting = tunnel::is_starting();
        let enabled = status.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        // `starting` lets the dashboard show "Starting…" instead of "off" for the
        // 10-90 s a cold enable needs (see tunnel::start_async).
        if let Some(obj) = status.as_object_mut() {
            obj.insert("starting".to_string(), serde_json::json!(starting));
            // A rejected token, or a connector that never registered, must SAY so:
            // the POST only answers "starting", so without this the dashboard polled,
            // saw `enabled:false`, and displayed a bare "Tunnel is off".
            let err = tunnel::last_error();
            if !starting && !enabled {
                // A failed enable leaves no tunnel: never report the stray pid of a
                // cloudflared that already exited as a running tunnel.
                obj.insert("running".to_string(), serde_json::json!(false));
                obj.insert("pid".to_string(), serde_json::json!(0));
                if !err.is_empty() {
                    obj.insert("error".to_string(), serde_json::json!(true));
                    obj.insert("message".to_string(), serde_json::json!(err));
                }
            }
        }
        return json_response(200, &status);
    }

    if p == "/api/tunnel/enable" && m == "POST" {
        // Changing the machine's internet exposure is an operator action.
        if let Some(resp) = require_privileged(&ctx) {
            return resp;
        }
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        let raw_token = input
            .get("tunnel_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        // A malformed token is rejected HERE, synchronously: the user gets the reason
        // in the click's own response instead of a "starting" spinner that ends in
        // "Tunnel is off" twenty seconds later. Nothing is stored, nothing is killed.
        let token = match tunnel::validate_token_input(&raw_token) {
            Ok(t) => t,
            Err(message) => {
                return json_response(
                    200,
                    &serde_json::json!({
                        "ok": 0,
                        "enabled": false,
                        "starting": false,
                        "error": true,
                        "message": message,
                    }),
                );
            }
        };
        // NON-BLOCKING: a cold enable can take 10-90 s (install the Termux
        // cloudflared, write a CA bundle, wait for the connector to register), and
        // holding the request that long reads as a hang on a phone. The dashboard
        // already polls /api/tunnel/status every 1.5 s and finishes on
        // `enabled && tunnel_url`, so answer "starting" and let it watch.
        let fresh = tunnel::start_async(state.port, &token);
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "enabled": false,
                "starting": true,
                "message": if fresh {
                    "Starting Cloudflare tunnel..."
                } else {
                    "Cloudflare tunnel is already starting..."
                },
            }),
        );
    }

    if p == "/api/tunnel/disable" && m == "POST" {
        if let Some(resp) = require_privileged(&ctx) {
            return resp;
        }
        return json_response(200, &tunnel::disable().await);
    }

    /* ── catalog settings (backend.php:18496-18657) ───────────────────────── */
    if p == "/api/catalog-settings" && m == "GET" {
        // `fd_load_catalog_settings()` always returns a FULL document; a missing
        // `storage/catalog_settings.json` is not an empty settings object. The
        // dashboard renders the Catalog Configuration / Streams Configuration
        // panels from this shape, so returning `{}` left every control blank.
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "settings": load_catalog_settings_merged(),
                "catalog_options": default_catalog_options(),
                "is_tunnel": false,
            }),
        );
    }

    if p == "/api/catalog-settings" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "error": "Invalid JSON input"}));
        }
        let mut current = load_catalog_settings_merged();
        if current.get("enabled_types").is_none() {
            current["enabled_types"] = serde_json::json!({});
        }
        if current.get("enabled_catalogs").is_none() {
            current["enabled_catalogs"] = serde_json::json!({});
        }
        if let Some(v) = input.get("catalogs_enabled") {
            current["catalogs_enabled"] = serde_json::json!(v.as_bool().unwrap_or(false));
        }
        if let Some(v) = input.get("country") {
            let code: String = v
                .as_str()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
                .to_ascii_uppercase();
            current["country"] = serde_json::json!(code);
        }
        if let Some(types) = input.get("enabled_types").and_then(|v| v.as_object()) {
            for key in ["movie", "series", "other"] {
                if let Some(v) = types.get(key) {
                    current["enabled_types"][key] = serde_json::json!(v.as_bool().unwrap_or(false));
                }
            }
        }
        if let Some(cats) = input.get("enabled_catalogs").and_then(|v| v.as_object()) {
            for (cid, v) in cats {
                current["enabled_catalogs"][cid] = serde_json::json!(v.as_bool().unwrap_or(false));
            }
        }
        // Master upstream switch: keep the stored list when disabling with an
        // empty incoming list (backend.php:18540-18566).
        let upstream_disabled = input
            .get("upstream_enabled")
            .map(|v| !v.as_bool().unwrap_or(true))
            .unwrap_or(false);
        if let Some(v) = input.get("upstream_enabled") {
            current["upstream_enabled"] = serde_json::json!(v.as_bool().unwrap_or(false));
        }
        if let Some(list) = input.get("upstream_manifests").and_then(|v| v.as_array()) {
            let cleaned: Vec<Value> = list
                .iter()
                .filter(|m| m.get("url").and_then(|u| u.as_str()).map(|u| !u.trim().is_empty()).unwrap_or(false))
                .map(|m| {
                    serde_json::json!({
                        "name": m.get("name").and_then(|v| v.as_str()).unwrap_or("Addon").trim(),
                        "url": m.get("url").and_then(|v| v.as_str()).unwrap_or("").trim(),
                        "id": m.get("id").and_then(|v| v.as_str()).unwrap_or("").trim(),
                        "version": m.get("version").and_then(|v| v.as_str()).unwrap_or("").trim(),
                    })
                })
                .collect();
            if !(upstream_disabled && cleaned.is_empty()) {
                current["upstream_manifests"] = Value::Array(cleaned);
            }
        }
        if let Some(sc_in) = input.get("stream_config").and_then(|v| v.as_object()) {
            if current.get("stream_config").map(|v| v.is_object()).unwrap_or(false) == false {
                current["stream_config"] = serde_json::json!({});
            }
            let sc = current["stream_config"].as_object_mut();
            if let Some(sc) = sc {
                let mut sc_value = Value::Object(sc.clone());
                apply_stream_config(&mut sc_value, sc_in);
                current["stream_config"] = sc_value;
            }
        }
        let saved = write_catalog_settings(&current);
        if saved {
            // New stream configuration must apply immediately. This port does not
            // cache stream RESPONSES (the AIOStreams filters run per request), so
            // only the (currently empty) `stream` prefix would need dropping — done
            // anyway so a future response cache cannot silently serve old filters.
            crate::cache::clear(Some("stream"));
        }
        return json_response(
            200,
            &serde_json::json!({
                "ok": if saved { 1 } else { 0 },
                "settings": current,
                "message": if saved { "Catalog settings updated successfully" } else { "Failed to save settings" },
            }),
        );
    }

    /* ── media-ids (backend.php:18668-18712) ──────────────────────────────── */
    if p == "/api/media-ids" && (m == "POST" || m == "DELETE") {
        if let Some(resp) = crate::auth::require_auth(&ctx) {
            return resp;
        }
        if m == "DELETE" {
            let raw = ctx.qs("media_ids");
            let mut pairs = serde_json::Map::new();
            for chunk in raw.split(',') {
                let t = chunk.trim();
                if t.is_empty() {
                    continue;
                }
                if let Some((k, v)) = t.split_once(':') {
                    if !k.is_empty() && !v.is_empty() {
                        pairs.insert(k.to_string(), Value::String(v.to_string()));
                    }
                }
            }
            if pairs.is_empty() {
                return json_response(
                    400,
                    &serde_json::json!({"ok": 0, "error": "Missing media_ids (prefix:value list)"}),
                );
            }
            let url = format!("{}/media-ids", wp_api_base());
            let r = http_json(
                &url,
                &[
                    ("media_ids".to_string(), Value::Object(pairs.clone()).to_string()),
                    ("delete".to_string(), "1".to_string()),
                ],
                "POST",
                12,
            )
            .await;
            return json_response(
                200,
                &serde_json::json!({"ok": 1, "deleted": pairs.len(), "upstream": r}),
            );
        }
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "error": "Invalid JSON body"}));
        }
        let media_ids = input
            .get("media_ids")
            .cloned()
            .filter(|v| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        let title = jstr(&input, "title");
        if media_ids.as_object().map(|o| o.is_empty()).unwrap_or(true) || title.is_empty() {
            return json_response(
                400,
                &serde_json::json!({"ok": 0, "error": "media_ids and title are required"}),
            );
        }
        // The port has no local Manticore; WordPress owns media_ids_idx.
        let url = format!("{}/media-ids", wp_api_base());
        let params = vec![
            ("media_ids".to_string(), media_ids.to_string()),
            ("title".to_string(), title),
            ("year".to_string(), input.get("year").and_then(|v| v.as_i64()).unwrap_or(0).to_string()),
            ("imdb_id".to_string(), jstr(&input, "imdb_id")),
            (
                "media_type".to_string(),
                {
                    let t = jstr(&input, "media_type");
                    if t.is_empty() { "movie".to_string() } else { t }
                },
            ),
            (
                "post_id".to_string(),
                input.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0).to_string(),
            ),
        ];
        let written = http_json(&url, &params, "POST", 12).await;
        let count = written
            .get("written")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| {
                if written.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                    media_ids.as_object().map(|o| o.len() as i64).unwrap_or(0)
                } else {
                    0
                }
            });
        return json_response(
            if count > 0 { 200 } else { 500 },
            &serde_json::json!({
                "ok": if count > 0 { 1 } else { 0 },
                "post_id": input.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0),
                "written": count,
            }),
        );
    }

    /* ── validate-manifest (backend.php:18715-18751) ──────────────────────── */
    if p == "/api/validate-manifest" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        let mut url = jstr(&input, "url");
        if url.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "error": "Please provide a manifest URL."}));
        }
        if let Some(rest) = url.strip_prefix("stremio://") {
            url = format!("https://{}", rest);
        }
        if !url.to_ascii_lowercase().contains("/manifest.json") {
            url = format!("{}/manifest.json", rtrim(&url));
        }
        let m2 = http_json(&url, &[], "GET", 8).await;
        let id = jstr(&m2, "id");
        let name = jstr(&m2, "name");
        if id.is_empty() || name.is_empty() {
            return json_response(
                400,
                &serde_json::json!({
                    "ok": 0,
                    "error": "Invalid Stremio manifest. Make sure the URL points to a valid manifest.json responding with id and name."
                }),
            );
        }
        let version = {
            let v = jstr(&m2, "version");
            if v.is_empty() { "1.0.0".to_string() } else { v }
        };
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "manifest": {
                    "id": id,
                    "name": name,
                    "version": version,
                    "description": jstr(&m2, "description"),
                    "url": url,
                    "resources": m2.get("resources").cloned().unwrap_or_else(|| serde_json::json!([])),
                },
                "message": "Manifest validated successfully.",
            }),
        );
    }

    /* ── country (backend.php:18755-18802) ────────────────────────────────── */
    if p == "/api/country" && m == "GET" {
        let settings = read_catalog_settings();
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "country": detect_country(&ctx),
                "configured_country": jstr(&settings, "country"),
                "available_countries": country_list(),
            }),
        );
    }

    if p == "/api/country" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "error": "Invalid JSON input"}));
        }
        let code: String = jstr(&input, "country")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
            .to_ascii_uppercase();
        let mut settings = read_catalog_settings();
        if !settings.is_object() {
            settings = serde_json::json!({});
        }
        settings["country"] = serde_json::json!(code);
        let saved = write_catalog_settings(&settings);
        return json_response(
            200,
            &serde_json::json!({
                "ok": if saved { 1 } else { 0 },
                "country": detect_country(&ctx),
                "configured_country": code,
                "message": if saved { "Country updated successfully" } else { "Failed to save country" },
            }),
        );
    }

    /* ── session / clock / provision (backend.php:18804-18925) ────────────── */
    if p == "/api/session" {
        let bot_id = get_bot_id();
        let meta = load_session_meta();
        let has_session = has_local_session("") && !bot_id.is_empty();
        let pool = get_bot_pool();
        let mut bot_username = jstr(&meta, "bot_username");
        let mut bot_name = jstr(&meta, "bot_name");
        if (bot_username.is_empty() || bot_name.is_empty()) && !pool.is_empty() {
            if let Some(pb) = pool.iter().find(|b| jstr(b, "bot_id") == bot_id) {
                if bot_username.is_empty() {
                    bot_username = jstr(pb, "bot_username");
                }
                if bot_name.is_empty() {
                    bot_name = jstr(pb, "bot_name");
                }
            }
        }
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "version": app_version(),
                "has_session": has_session,
                "is_provisioning": is_guest_provision_in_progress(),
                "bot_id": if has_session { bot_id } else { String::new() },
                "bot_username": if has_session { bot_username } else { String::new() },
                "bot_name": if has_session { bot_name } else { String::new() },
                "api_secret": if has_session && auth_privileged(&ctx) { get_api_secret() } else { String::new() },
                "device_id": get_device_id(),
                "bot_count": pool.len(),
                "bot_pool": pool,
                "environment_ok": true,
                "environment_fatal": false,
                "environment_problems": [],
                "environment_hints": [],
                // Neutral label: this is shown in the dashboard's bot/session
                // card, so it must not name the engine.
                "runtime": "pencarimovie-server",
            }),
        );
    }

    if p == "/api/clock-check" && m == "GET" {
        let probe = measure_clock_offset().await;
        let abs = probe.offset.abs();
        return json_response(
            200,
            &serde_json::json!({
                "ok": if probe.ok { 1 } else { 0 },
                "offset_seconds": probe.offset,
                "server_time": probe.server_time,
                "local_time": probe.local_time,
                "skewed": probe.ok && abs > 30,
                "critical": probe.ok && abs > 60,
                "message": probe.error.unwrap_or_default(),
            }),
        );
    }

    if p == "/api/provision" && (m == "GET" || m == "POST") {
        return match auto_provision_guest(&state.telegram).await {
            Ok(prov) => json_response(
                200,
                &serde_json::json!({
                    "ok": 1,
                    "message": "Guest bot session initialized successfully.",
                    "bot_id": prov.bot_id,
                    "bot_username": prov.bot_username,
                    "bot_name": prov.bot_name,
                    "api_secret": get_api_secret(),
                    "pool": get_bot_pool(),
                }),
            ),
            Err(prov) => json_response(
                500,
                &serde_json::json!({
                    "ok": 0,
                    "message": prov.error.unwrap_or_else(|| "Could not obtain guest bot session from server.".to_string()),
                }),
            ),
        };
    }

    /* ── bots (backend.php:18928-19112) ───────────────────────────────────── */
    if p == "/api/bots" && m == "GET" {
        let pool = get_bot_pool();
        let active = get_bot_id();
        let list: Vec<Value> = pool
            .iter()
            .map(|b| {
                let id = jstr(b, "bot_id");
                let mut out = b.clone();
                if let Some(obj) = out.as_object_mut() {
                    obj.insert("has_session".into(), serde_json::json!(has_local_session(&id)));
                    obj.insert("is_active".into(), serde_json::json!(id == active));
                }
                out
            })
            .collect();
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "active_bot_id": active,
                "total_bots": list.len(),
                "bots": list,
            }),
        );
    }

    if p == "/api/bots/add" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "Invalid JSON body"}));
        }
        let mut tokens: Vec<String> = Vec::new();
        let single = jstr(&input, "bot_token");
        if !single.is_empty() {
            tokens.push(single);
        } else if let Some(list) = input.get("tokens").and_then(|v| v.as_array()) {
            tokens.extend(list.iter().filter_map(|t| t.as_str()).map(|t| t.trim().to_string()));
        } else if let Some(text) = input.get("tokens_text").and_then(|v| v.as_str()) {
            tokens.extend(
                text.split(|c: char| c == ',' || c.is_whitespace())
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty()),
            );
        }
        let mut seen = std::collections::HashSet::new();
        tokens.retain(|t| !t.is_empty() && seen.insert(t.clone()));
        if tokens.is_empty() {
            return json_response(
                400,
                &serde_json::json!({"ok": 0, "message": "Provide bot_token, tokens[] or tokens_text."}),
            );
        }
        let reserve = is_guest_provision_in_progress();
        let mut added: Vec<Value> = Vec::new();
        let mut failed: Vec<Value> = Vec::new();
        for token in tokens {
            let id_part = token.split(':').next().unwrap_or("").to_string();
            let hint = if id_part.is_empty() {
                format!("add_{}", rand_suffix())
            } else {
                format!("add_{}", id_part)
            };
            match state.telegram.boot(&hint, Some(&token)).await {
                Ok(entry) => {
                    add_pool_bot(serde_json::json!({
                        "bot_id": entry.identity.id,
                        "bot_username": entry.identity.username,
                        "bot_name": entry.identity.first_name,
                        "status": "online",
                        "is_active": true,
                        "updated_at": now(),
                    }));
                    added.push(serde_json::json!({
                        "bot_id": entry.identity.id,
                        "bot_username": entry.identity.username,
                        "bot_name": entry.identity.first_name,
                    }));
                }
                Err(e) => failed.push(serde_json::json!({
                    "token": if id_part.is_empty() { "…".to_string() } else { format!("{}:…", id_part) },
                    "error": e,
                })),
            }
        }
        if !added.is_empty() && get_bot_id().is_empty() {
            let first = &added[0];
            save_session_meta(
                &jstr(first, "bot_id"),
                &jstr(first, "bot_username"),
                &jstr(first, "bot_name"),
            );
        }
        let ok = !added.is_empty();
        return json_response(
            if ok { 200 } else { 400 },
            &serde_json::json!({
                "ok": if ok { 1 } else { 0 },
                "message": if ok {
                    format!("Added {} bot(s) to the pool.", added.len())
                } else {
                    "No bot could be added (check the token).".to_string()
                },
                "added": added,
                "failed": failed,
                "added_count": added.len(),
                "pool": get_bot_pool(),
                "tunnel_restarted": reserve,
            }),
        );
    }

    if p == "/api/bots/remove" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "Invalid JSON body"}));
        }
        let bot_id = jstr(&input, "bot_id");
        if bot_id.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "bot_id is required."}));
        }
        let pool = remove_pool_bot(&bot_id);
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "message": "Bot removed from pool.",
                "pool": pool,
                "active_bot_id": get_bot_id(),
            }),
        );
    }

    if p == "/api/bots/set-active" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        if !input.is_object() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "Invalid JSON body"}));
        }
        let bot_id = jstr(&input, "bot_id");
        if bot_id.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "bot_id is required."}));
        }
        let target = get_bot_pool()
            .into_iter()
            .find(|b| jstr(b, "bot_id") == bot_id);
        let Some(target) = target else {
            return json_response(404, &serde_json::json!({"ok": 0, "message": "Bot ID not found in pool."}));
        };
        let username = jstr(&target, "bot_username");
        let name = jstr(&target, "bot_name");
        save_session_meta(&bot_id, &username, &name);
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "active_bot_id": bot_id,
                "bot_username": username,
                "bot_name": name,
                "tunnel_restarted": false,
            }),
        );
    }

    /* ── version gate: REMOVED on purpose ─────────────────────────────────── */
    // The PHP build blocked every /api/* call with HTTP 426 when the engine was
    // older than the WordPress minimum. That made an outdated binary unusable
    // instead of merely out of date, and it fought the native build (which
    // reports `git describe` version strings). The check still RUNS — `/api/version`
    // returns `update_needed`, `minimum_version` and `update_url`, and the
    // dashboard shows a banner in #settings / #addon with a one-click update —
    // but nothing is ever blocked.
    let _ = check_version;

    /* ── desktop self-update ──────────────────────────────────────────────── */
    // `check` only reads the GitHub release feed. `apply` downloads and then hands
    // the archive to the shipped apply-update script, so it is privileged (a LAN
    // peer must not be able to push a binary onto the host).
    if p == "/api/update/check" && m == "GET" {
        return json_response(200, &crate::update::check().await);
    }
    if p == "/api/update/apply" && m == "POST" {
        if let Some(resp) = require_privileged(&ctx) {
            return resp;
        }
        return json_response(200, &crate::update::apply().await);
    }

    /* ── diagnostics / reports (backend.php:19149-19202) ──────────────────── */
    if p == "/api/diagnostics" && m == "GET" {
        let mut reports = diag_list();
        reports.truncate(10);
        // `activity` is what the Android app polls to decide whether it is
        // "Active" (hold the wake/Wi-Fi lock, say so in the notification) or
        // "Idle" (release them after a grace period) — the TDLib app's
        // markActive/ACTIVE_LOCK_TIMEOUT_MS behaviour.
        let streams = crate::download::active_streams();
        let preflights = crate::download::active_preflights();
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "info": device_info(),
                "counts": diag_counts(),
                "activity": {
                    "streams": streams,
                    "preflights": preflights,
                    "total": streams + preflights,
                },
                "reports": reports,
            }),
        );
    }

    if p == "/api/diagnostics/settings" {
        if m == "POST" || ctx.has_q("enabled") {
            if let Some(resp) = require_privileged(&ctx) {
                return resp;
            }
            let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
            if let Some(v) = input.get("auto_report") {
                set_auto_report(v.as_bool().unwrap_or(true));
            } else if ctx.has_q("enabled") {
                let v = ctx.qs("enabled").to_ascii_lowercase();
                set_auto_report(v == "1" || v == "true" || v == "on");
            }
        }
        return json_response(
            200,
            &serde_json::json!({"ok": 1, "auto_report": auto_report_enabled()}),
        );
    }

    if p == "/api/reports" && m == "GET" {
        if ctx.has_q("clear") || ctx.has_q("send") || ctx.has_q("flush") {
            if let Some(resp) = require_privileged(&ctx) {
                return resp;
            }
        }
        if ctx.has_q("clear") {
            diag_clear();
            return json_response(200, &serde_json::json!({"ok": 1, "cleared": true}));
        }
        if ctx.has_q("send") {
            let file = ctx.qs("send");
            let base = std::path::Path::new(&file)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            if base.is_empty() {
                return json_response(400, &serde_json::json!({"ok": 0, "message": "file is required"}));
            }
            let path = crate::diagnostics::reports_dir().join(&base);
            let ok = crate::diagnostics::report_send(&path).await;
            return json_response(
                200,
                &serde_json::json!({
                    "ok": if ok { 1 } else { 0 },
                    "file": base,
                    "message": if ok { "Report sent." } else { "Report upload failed." },
                }),
            );
        }
        if ctx.has_q("flush") {
            let (sent, failed) = crate::diagnostics::flush_pending().await;
            return json_response(200, &serde_json::json!({"ok": 1, "sent": sent, "failed": failed}));
        }
        if ctx.has_q("file") {
            let name = ctx.qs("file");
            let base = std::path::Path::new(&name)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            let path = crate::diagnostics::reports_dir().join(base);
            return match std::fs::read_to_string(path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            {
                Some(v) => json_response(200, &serde_json::json!({"ok": 1, "report": v})),
                None => json_response(404, &serde_json::json!({"ok": 0, "message": "Report not found"})),
            };
        }
        let limit: usize = ctx.qs("limit").parse().unwrap_or(20).max(1);
        let mut reports = diag_list();
        reports.truncate(limit);
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "auto_report": auto_report_enabled(),
                "counts": diag_counts(),
                "reports": reports,
            }),
        );
    }

    /* ── debug-mode / logs (backend.php:19206-19260) ──────────────────────── */
    if p == "/api/debug-mode" {
        if m == "POST" {
            let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
            set_debug_enabled(input.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false));
            return json_response(200, &serde_json::json!({"ok": 1, "enabled": is_debug_enabled()}));
        }
        if m == "GET" {
            if ctx.has_q("enable") {
                let v = ctx.qs("enable").to_ascii_lowercase();
                set_debug_enabled(v == "1" || v == "true" || v == "on");
            }
            return json_response(200, &serde_json::json!({"ok": 1, "enabled": is_debug_enabled()}));
        }
    }

    if p == "/api/logs" && m == "GET" {
        if !is_debug_enabled() {
            return json_response(
                403,
                &serde_json::json!({"ok": 0, "message": "Debug logging is currently disabled. Enable it via /api/debug-mode?enable=1"}),
            );
        }
        let req_file = {
            let a = ctx.qs("file");
            let a = if a.is_empty() { ctx.qs("path") } else { a };
            let a = if a.is_empty() { "debug.log".to_string() } else { a };
            a.to_ascii_lowercase()
        };
        let base = std::path::Path::new(&req_file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let target = match base.as_str() {
            "debug.log" => Some(storage_path("debug.log")),
            "madelineproto.log" => Some(storage_path("MadelineProto.log")),
            _ => None,
        };
        let Some(target) = target else {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "File not allowed"}));
        };
        let text = std::fs::read_to_string(target).unwrap_or_default();
        let lines: Vec<&str> = text.split('\n').collect();
        let tail: Vec<&str> = lines.iter().rev().take(400).rev().cloned().collect();
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "file": base,
                "total_lines": lines.len(),
                "lines": tail,
            }),
        );
    }

    /* ── bot login/logout (backend.php:19416-19576) ───────────────────────── */
    if p == "/api/botlogin" && m == "POST" {
        let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
        let token = {
            let a = jstr(&input, "bot_token");
            if a.is_empty() { jstr(&input, "token") } else { a }
        };
        if token.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "bot_token is required."}));
        }
        let hint = {
            let id = token.split(':').next().unwrap_or("").to_string();
            if id.is_empty() {
                format!("login_{}", rand_suffix())
            } else {
                format!("login_{}", id)
            }
        };
        return match state.telegram.boot(&hint, Some(&token)).await {
            Ok(entry) => {
                save_session_meta(&entry.identity.id, &entry.identity.username, &entry.identity.first_name);
                json_response(
                    200,
                    &serde_json::json!({
                        "ok": 1,
                        "message": "Bot logged in successfully.",
                        "bot_id": entry.identity.id,
                        "bot_username": entry.identity.username,
                        "bot_name": entry.identity.first_name,
                        "api_secret": get_api_secret(),
                        "pool": get_bot_pool(),
                    }),
                )
            }
            Err(e) => json_response(500, &serde_json::json!({"ok": 0, "message": e})),
        };
    }

    if p == "/api/botlogout" && m == "POST" {
        // Identity first, then sessions/pool (the PHP's self-resurrecting-pool fix).
        let active = get_bot_id();
        let _ = std::fs::remove_file(storage_path("session_meta.json"));
        let _ = std::fs::remove_file(storage_path("bot_id.txt"));
        clear_session();
        let _ = std::fs::remove_file(storage_path("bot_pool.json"));
        clear_api_secret();
        log("bot logout completed", &serde_json::json!({"previous_bot_id": active}));
        return json_response(200, &serde_json::json!({"ok": 1, "message": "Bot session cleared."}));
    }

    /* ── proxy-stream (backend.php:19577+) ────────────────────────────────── */
    if p == "/api/proxy-stream" && m == "GET" {
        let action = ctx.qs("action");
        if action.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "action is required."}));
        }
        // The serializer is NOT Send: build the query string in its own scope so
        // it is dropped before the `.await` (a `url::form_urlencoded::Serializer`
        // held across an await makes the whole handler future non-Send).
        let query_string = {
            let mut qs = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in &ctx.query {
                if k != "action" {
                    qs.append_pair(k, v);
                }
            }
            qs.finish()
        };
        // REST, not `admin-ajax.php`: the same mapping `wp_data::fetch_stream_ajax`
        // uses (`posts` -> `/posts`, `post_files` -> `/post-files`). The AJAX bridge
        // was the old PHP dashboard's transport, and keeping it here meant the port
        // still depended on a PHP endpoint that no other route touches. This route
        // exists only so the shell can read the origin without CORS.
        let url = format!(
            "{}/{}?{}",
            wp_api_base().trim_end_matches('/'),
            action.replace('_', "-"),
            query_string
        );
        let mut extra = HashMap::new();
        extra.insert("Accept".to_string(), "application/json".to_string());
        let body = get_contents(&url, 15, extra).await;
        return match body {
            None => {
                log("proxy-stream failed", &serde_json::json!({"action": action}));
                json_response(
                    502,
                    &serde_json::json!({"ok": 0, "message": "Failed to fetch data from PencariMovie."}),
                )
            }
            Some(text) => match serde_json::from_str::<Value>(&text) {
                Ok(v) => json_response(200, &v),
                Err(_) => text_response(200, "application/json; charset=utf-8", text),
            },
        };
    }

    /* ── download ─────────────────────────────────────────────────────────── */
    if is_public_download_path(&p) {
        return handle_download(&state, &ctx).await;
    }

    /* ── addon aliases + manifest ─────────────────────────────────────────── */
    let mut addon_path = p.clone();
    if let Some(rest) = p.strip_prefix("/nuvio") {
        addon_path = rest.to_string();
    } else if let Some(rest) = p.strip_prefix("/stremio") {
        addon_path = rest.to_string();
    }
    if addon_path.is_empty() {
        addon_path = "/manifest.json".to_string();
    }
    if addon_path == "/manifest.json" {
        return stremio_json(200, &manifest(&ctx.base_url).await, "no-store");
    }

    /* ── /stream ──────────────────────────────────────────────────────────── */
    if let Some(rest) = addon_path.strip_prefix("/stream/") {
        if let Some((type_part, id_part)) = rest.split_once('/') {
            let id = id_part.strip_suffix(".json").map(|s| s.to_string());
            if let Some(id) = id {
                if !is_authenticated(&ctx) {
                    return stremio_locked_stream(&ctx.base_url);
                }
                let decoded_id = crate::download::percent_decode(&id);
                return handle_stream(&state, &ctx, type_part, &decoded_id).await;
            }
        }
        return not_found_json();
    }

    if addon_path.starts_with("/catalog/") {
        return handle_catalog(&ctx, &addon_path).await;
    }
    if addon_path.starts_with("/meta/") {
        return handle_meta(&ctx, &addon_path).await;
    }
    if addon_path.starts_with("/subtitles/") {
        return handle_subtitles(&addon_path).await;
    }

    /* ── eclipse ──────────────────────────────────────────────────────────── */
    if p.starts_with("/eclipse") {
        if !is_authenticated(&ctx) {
            return eclipse_locked_response(&ctx.base_url);
        }
        if p == "/eclipse" || p == "/eclipse/manifest.json" {
            return stremio_json(
                200,
                &serde_json::json!({
                    "id": "org.pencarimovie.eclipse",
                    "name": "PencariMovie",
                    "version": "1.0.0",
                    "description": "PencariMovie music addon",
                    "icon": format!("{}/logo.png", rtrim(&ctx.base_url)),
                    "contentType": "music",
                    "resources": ["search", "stream", "catalog"],
                    "types": ["track"],
                    // The rows the dashboard and Stremio/Nuvio browse. Kept small on purpose
                    // (Stremio rejects a descriptor over ~49 KB) and with no `genres` array —
                    // genre values live in the `genre` extra, exactly like cinemeta.
                    "catalogs": [],
                    "settings": [],
                }),
                "no-store",
            );
        }
        if p.starts_with("/eclipse/search") {
            let q = {
                let a = ctx.qs("q");
                if a.is_empty() { ctx.qs("query") } else { a }
            };
            let tracks = eclipse_search(&q, 30).await;
            return stremio_json(200, &serde_json::json!({"tracks": tracks}), "no-store");
        }
        if let Some(rest) = p.strip_prefix("/eclipse/catalog/") {
            // `latest` (and any keyword catalog) reuses the music search: the
            // origin indexes tracks, not rows, so "latest" is a plain search.
            let cat = rest.trim_end_matches(".json").trim();
            let q = if cat == "latest" || cat.is_empty() { "" } else { cat };
            let tracks = eclipse_search(q, 50).await;
            return stremio_json(200, &serde_json::json!({"tracks": tracks}), "no-store");
        }
        if let Some(rest) = p.strip_prefix("/eclipse/stream/") {
            return eclipse_stream(&ctx, rest.trim_end_matches(".json")).await;
        }
        if let Some(rest) = p.strip_prefix("/eclipse/play/") {
            return eclipse_stream(&ctx, rest.trim_end_matches(".json")).await;
        }
        return stremio_json(404, &serde_json::json!({"error": "Not found"}), "no-store");
    }

    /* ── /api/resolve-shortcode, /api/warmup-resolve ──────────────────────── */
    if p == "/api/resolve-shortcode" && m == "GET" {
        let short_code = {
            let a = ctx.qs("short_code");
            if a.is_empty() { ctx.qs("sc") } else { a }
        };
        let bot_id = ctx.qs("bot_id");
        if short_code.is_empty() {
            return json_response(400, &serde_json::json!({"ok": 0, "message": "short_code is required."}));
        }
        let mut resolved = if bot_id.is_empty() {
            resolve_shortcode_concurrent(&short_code, &[]).await
        } else {
            resolve_shortcode(&short_code, &bot_id, false).await
        };
        let has_id = {
            let a = jstr(&resolved, "file_id_mt");
            let b = jstr(&resolved, "file_id");
            !a.is_empty() || !b.is_empty()
        };
        if !has_id {
            let message = {
                let m = jstr(&resolved, "message");
                if m.is_empty() { "File could not be resolved.".to_string() } else { m }
            };
            return json_response(
                404,
                &serde_json::json!({"ok": 0, "message": message, "short_code": short_code}),
            );
        }
        // A batch-warmed cache row can lack file_size/title/mime. Enrich it with
        // a single resolve so the download payload is complete.
        let lacks = resolved.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0) == 0
            || (jstr(&resolved, "title").is_empty() && jstr(&resolved, "file_name").is_empty())
            || jstr(&resolved, "mime").is_empty();
        if lacks {
            let bot = {
                let a = jstr(&resolved, "bot_id");
                if a.is_empty() { bot_id.clone() } else { a }
            };
            if !bot.is_empty() {
                let rich = resolve_shortcode(&short_code, &bot, true).await;
                if !jstr(&rich, "file_id_mt").is_empty() || !jstr(&rich, "file_id").is_empty() {
                    if let (Some(dst), Some(src)) = (resolved.as_object_mut(), rich.as_object()) {
                        for (k, v) in src {
                            dst.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
        let mut out = resolved;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("ok".into(), serde_json::json!(1));
            obj.insert("short_code".into(), serde_json::json!(short_code));
        }
        return json_response(200, &out);
    }

    if p == "/api/warmup-resolve" && (m == "GET" || m == "POST") {
        let mut codes: Vec<String> = Vec::new();
        if m == "POST" {
            let input: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);
            let raw = input.get("short_codes").or_else(|| input.get("query"));
            match raw {
                Some(Value::Array(list)) => {
                    codes.extend(list.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()));
                }
                Some(Value::String(s)) => {
                    codes.extend(s.split(|c: char| c == ',' || c.is_whitespace()).map(|x| x.to_string()));
                }
                _ => {}
            }
        } else {
            codes.extend(
                ctx.qs("short_codes")
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .map(|x| x.to_string()),
            );
        }
        let codes: Vec<String> = codes
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let result = warmup_resolve_batch(&codes, &ctx.qs("bot_id")).await;
        return json_response(
            200,
            &serde_json::json!({
                "ok": 1,
                "total": result.total,
                "resolved": result.resolved,
                "failed": result.failed,
                "failed_codes": result.failed_codes,
                "results": result.results,
                "bot_id": result.bot_id,
            }),
        );
    }

    if p.starts_with("/api/") {
        return json_response(404, &serde_json::json!({"ok": 0, "message": "Unknown API endpoint"}));
    }

    /* ── root / configure / static / unknown ──────────────────────────────── */
    if p == "/configure" {
        return redirect("/#addon");
    }
    // Everything below `/api/*`, `/manifest.json`, `/catalog|meta|stream|
    // subtitles|eclipse` has already been answered above, so anything left is
    // the dashboard and its assets. They are served FROM DISK — this port does
    // not need the PHP runtime for the UI.
    if let Some(resp) = serve_static(&p) {
        return resp;
    }
    if p == "/" || p == "/index.html" {
        // `public/` is missing (a bare binary next to no checkout): say so
        // instead of returning a 404 that looks like a routing bug.
        let mut resp = text_response(
            200,
            "text/html; charset=utf-8",
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>PencariMovie Server</title></head>\n\
<body style=\"background:#141414;color:#eee;font-family:system-ui,sans-serif;padding:40px;line-height:1.6\">\n\
<h1>PencariMovie Server</h1>\n\
<p><code>public/index.html</code> was not found next to the binary, so the dashboard cannot be served.\n\
The REST API and the Stremio addon routes are unaffected:</p>\n\
<ul><li><a style=\"color:#ff6b35\" href=\"/manifest.json\">/manifest.json</a></li>\n\
<li><a style=\"color:#ff6b35\" href=\"/api/version\">/api/version</a></li>\n\
<li><a style=\"color:#ff6b35\" href=\"/api/session\">/api/session</a></li>\n\
<li><a style=\"color:#ff6b35\" href=\"/api/bots\">/api/bots</a></li></ul></body></html>"
                .to_string(),
        );
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
        return resp;
    }

    not_found_json()
}

/// Serve the dashboard, with an SPA fallback to `index.html` for extension-less
/// paths (it is a hash-router app, so `/anything` must still boot it).
///
/// The assets are **embedded in this binary** ([`crate::assets`]), which is what
/// makes the server a single `pencarimovie-server` executable — no `public/`
/// folder to ship and no PHP runtime. `PM_ASSETS=disk` switches to reading `public/` from the
/// checkout instead, for frontend iteration without a rebuild.
///
/// A path containing `..` or a dot-segment is rejected: `storage/` (sessions,
/// `api_secret.key`, `auth.json`) sits beside `public/` in the source tree and
/// must stay private.
fn serve_static(p: &str) -> Option<Response> {
    let rel = p.trim_start_matches('/');
    if rel.split('/').any(|seg| seg == ".." || seg.starts_with('.')) {
        return None;
    }
    if crate::storage::env("PM_ASSETS").as_deref() != Some("disk") {
        if let Some(bytes) = crate::assets::get(rel) {
            // `/` resolves to index.html, so the content type must come from the
            // RESOLVED name — deriving it from an empty path produced
            // `application/octet-stream` and the browser DOWNLOADED the
            // dashboard instead of rendering it.
            let named = if rel.is_empty() { "index.html" } else { rel };
            let ctype = content_type_for(std::path::Path::new(named));
            return crate::respond::bytes_response(ctype, bytes::Bytes::from_static(bytes));
        }
        if rel.is_empty() || !rel.contains('.') {
            if let Some(index) = crate::assets::get("index.html") {
                return crate::respond::bytes_response(
                    "text/html; charset=utf-8",
                    bytes::Bytes::from_static(index),
                );
            }
        }
        if !crate::assets::is_empty() {
            return None;
        }
    }
    serve_static_from_disk(rel)
}

/// `PM_ASSETS=disk` (and any build where `public/` was absent at compile time).
fn serve_static_from_disk(rel: &str) -> Option<Response> {
    let public = crate::storage::public_dir()?;
    if rel.is_empty() {
        return crate::respond::file_response(&public.join("index.html"), "text/html; charset=utf-8");
    }
    let candidate = public.join(rel);
    if candidate.is_file() {
        let ctype = content_type_for(&candidate);
        return crate::respond::file_response(&candidate, ctype);
    }
    if !rel.contains('.') {
        return crate::respond::file_response(&public.join("index.html"), "text/html; charset=utf-8");
    }
    None
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

/* ── stream route ─────────────────────────────────────────────────────────── */

async fn handle_stream(state: &Arc<AppState>, ctx: &ReqCtx, item_type: &str, item_id: &str) -> Response {
    // `/stream/*` response cache (300 s), the equivalent of the PHP's
    // `stream_cache_*.json`. Checked BEFORE any origin/upstream work, so a warm
    // list costs one hash lookup. `finish_streams()` fills it.
    let skey = stream_cache_key(item_type, item_id, &ctx.base_url);
    if let Some(hit) = crate::cache::get("stream", &skey) {
        if hit.get("streams").map(|s| s.is_array()).unwrap_or(false) {
            log(
                "stremio stream served from cache",
                &serde_json::json!({"type": item_type, "id": item_id}),
            );
            return stremio_json(200, &hit, "no-store");
        }
    }

    // NO BOT SESSION -> a TAPPABLE CARD, never an empty list.
    //
    // backend.php answers a stream ITEM here because Stremio/Nuvio render a
    // stream's `description` and never an HTTP error body; this port had no gate
    // at all, so with no session every title fell through to the origin search
    // and answered `{"streams":[]}` — indistinguishable from "this title has no
    // sources", and it never told the user a guest bot was being provisioned.
    //
    // Only the clock probe runs inline (one unauthenticated request, ~150ms: a
    // skewed clock breaks the MTProto handshake, so it must be diagnosed BEFORE
    // provisioning). The provisioner itself is spawned, exactly like
    // `fd_spawn_guest_provision()`, so this response is immediate.
    if !has_local_session("") {
        let base = ctx.base_url.trim_end_matches('/');
        let clock = measure_clock_offset().await;
        if clock.ok && clock.offset.abs() > 30 {
            log(
                "stream blocked: clock skew",
                &serde_json::json!({"offset_seconds": clock.offset}),
            );
            return stremio_json(
                200,
                &serde_json::json!({
                    "streams": [{
                        "name": "PencariMovie",
                        "description": format!(
                            "{}\nTap to open PencariMovie and re-check the clock, then retry.",
                            clock_skew_message(clock.offset)
                        ),
                        "externalUrl": format!("{}/#settings", base),
                    }]
                }),
                "no-store",
            );
        }
        let already = is_guest_provision_in_progress();
        if !already {
            let st = state.clone();
            tokio::spawn(async move {
                let _ = auto_provision_guest(&st.telegram).await;
            });
        }
        log(
            "stream blocked: provisioning guest bot",
            &serde_json::json!({"already_running": already, "type": item_type, "id": item_id}),
        );
        return stremio_json(
            200,
            &serde_json::json!({
                "streams": [{
                    "name": "PencariMovie",
                    "description": format!(
                        "{}\n⏳ {} a guest Telegram bot — this takes a few seconds. Reopen this title in a moment.",
                        if already {
                            "Telegram bot not connected yet."
                        } else {
                            "Telegram bot not connected."
                        },
                        if already { "Still provisioning" } else { "Provisioning" }
                    ),
                    "externalUrl": format!("{}/#settings", base),
                }]
            }),
            "no-store",
        );
    }

    let is_direct = item_id.starts_with("pm_file_") || item_id.starts_with("pm:file:");
    let code = item_id
        .strip_prefix("pm_file_")
        .or_else(|| item_id.strip_prefix("pm:file:"))
        .unwrap_or("");
    if is_direct && !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric()) {
        let active_bot = get_bot_id();
        let resolved = if active_bot.is_empty() {
            resolve_shortcode_concurrent(code, &[]).await
        } else {
            resolve_shortcode(code, &active_bot, false).await
        };
        let file_id = {
            let a = jstr(&resolved, "file_id_mt");
            if a.is_empty() { jstr(&resolved, "file_id") } else { a }
        };
        if file_id.is_empty() {
            let msg = jstr(&resolved, "message");
            return stremio_json(
                200,
                &serde_json::json!({
                    "streams": [{
                        "name": "PencariMovie",
                        "description": format!("File could not be resolved.\n{}", msg),
                    }]
                }),
                "no-cache",
            );
        }
        let size = resolved.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
        let title = {
            let t = jstr(&resolved, "title");
            if t.is_empty() {
                let f = jstr(&resolved, "file_name");
                if f.is_empty() { format!("{}.mp4", code) } else { f }
            } else {
                t
            }
        };
        let file_name = stream_filename(&title, &jstr(&resolved, "mime"));
        let mime = {
            let m = jstr(&resolved, "mime");
            let m = if m.is_empty() { jstr(&resolved, "file_type") } else { m };
            guess_video_mime(&file_name, &m)
        };
        let bot_for_payload = {
            let b = jstr(&resolved, "bot_id");
            if b.is_empty() { active_bot.clone() } else { b }
        };
        let payload = serde_json::json!({
            "file_id_mt": file_id,
            "file_size": size,
            "file_name": file_name,
            "mime": mime,
            "short_code": code,
            "bot_id": bot_for_payload,
        });
        let b64 = crate::fileid::base64url_encode(payload.to_string().as_bytes());
        let url = build_stream_url(&ctx.base_url, &b64, &file_name);
        // A direct `pm_file_` item keeps its file_id in the payload (no
        // resolution at download time) and still gets the sponsored message on
        // top, exactly like every other stream list.
        return finish_streams(
            ctx,
            item_type,
            &title,
            item_id,
            vec![{
                let _ = &url;
                serde_json::json!({
                    "name": "PencariMovie",
                    "description": format!("{}\n💾 {} • ⚡ Telegram", file_name, format_bytes(size)),
                    "url": url,
                    "behaviorHints": {
                        "filename": file_name,
                        "videoSize": size,
                        "notWebReady": false,
                        "bingeGroup": "pencarimovie"
                    },
                })
            }],
            false,
            1,
        )
        .await;
    }

    // `pm:post:<id>[:S:E]` — the post's own file set through the matcher.
    if let Some(rest) = item_id.strip_prefix("pm:post:") {
        return handle_post_stream(ctx, item_type, item_id, rest, None).await;
    }

    // Any other id (`tt…`, `tmdb:…`, `kitsu:…`, `mal:…`): resolve the identity,
    // search the origin by title, then let the matcher decide which of the
    // returned files belong to this title. Upstreams that declare `stream` are
    // asked CONCURRENTLY — the task is spawned here and awaited at the end, so
    // they never add their latency to the local list (backend.php:16606-16739
    // does the same with `\Amp\async`).
    if parse_external_id(item_id).is_some() {
        let upstream = spawn_upstream_streams(item_type, item_id);
        if let Some(resp) = handle_external_stream(ctx, item_type, item_id, upstream).await {
            return resp;
        }
    }

    // A shape the local matcher cannot use (an unknown `foo:` prefix, anything
    // odd): an upstream declaring `stream` may still know it, so ask before
    // giving up. A `pm:` id never reaches here (handled above) and never fans
    // out. (`tvdb:` / `anilist:` / `tvmaze:` are parsed above, so they take the
    // local path in `handle_external_stream()`.)
    let upstream_items = await_upstream(spawn_upstream_streams(item_type, item_id), true).await;
    if !upstream_items.is_empty() {
        return finish_streams(ctx, item_type, item_id, item_id, upstream_items, false, 0).await;
    }
    crate::diagnostics::record(
        "stream_incident",
        "stream list requested for an unsupported item shape",
        "",
        "",
        "NO_STREAMS",
        "",
        item_id,
        serde_json::json!({"item_type": item_type, "runtime": "pencarimovie-server"}),
    );
    stremio_json(200, &serde_json::json!({"streams": []}), "no-store")
}

/// Upstream stream-list task (`fd_http_get_many_amp` over the upstream
/// `/stream/:type/:id.json` URLs, backend.php:16706).
///
/// Spawned BEFORE the local resolution so the two overlap. A `pm:` / `pm_` id
/// never fans out — it is ours and no upstream can answer it
/// (`$isLocalPmId` in the PHP). `None` also means "nothing configured", which
/// keeps the zero-upstream path allocation-free.
fn spawn_upstream_streams(
    item_type: &str,
    item_id: &str,
) -> Option<tokio::task::JoinHandle<Vec<Value>>> {
    if item_id.starts_with("pm:") || item_id.starts_with("pm_") {
        return None;
    }
    let settings = load_catalog_settings_merged();
    if upstream::configured(&settings, true).is_empty() {
        return None;
    }
    let ty = item_type.to_string();
    let id = item_id.to_string();
    Some(tokio::spawn(async move {
        upstream::streams(&settings, &ty, &id).await
    }))
}

/// Await an upstream task with the PHP's non-blocking bound
/// (backend.php:18121): 0.8 s when local streams already exist, 2.5 s when the
/// list is still empty. Losing the race costs nothing — the local list is
/// delivered without the external items.
async fn await_upstream(
    handle: Option<tokio::task::JoinHandle<Vec<Value>>>,
    local_empty: bool,
) -> Vec<Value> {
    let Some(handle) = handle else {
        return Vec::new();
    };
    let wait_ms: u64 = if local_empty { 2500 } else { 800 };
    match tokio::time::timeout(std::time::Duration::from_millis(wait_ms), handle).await {
        Ok(Ok(items)) => items,
        Ok(Err(e)) => {
            log(
                "upstream stream task failed",
                &serde_json::json!({"error": e.to_string()}),
            );
            Vec::new()
        }
        Err(_) => {
            log(
                "upstream stream merge timed out",
                &serde_json::json!({"wait_ms": wait_ms}),
            );
            Vec::new()
        }
    }
}

/// `pm:post:<id>[:S:E]` — Fast Path 2a/2b, the same shape as the PHP:
/// title/year come from the post, the file set from the post's own pages, and
/// the matcher decides. `episode_names` is empty because the upstream
/// episode-name lookup is not ported yet — that guard fails OPEN, so it can
/// never hide a stream.
async fn handle_post_stream(
    ctx: &ReqCtx,
    item_type: &str,
    item_id: &str,
    rest: &str,
    upstream: Option<tokio::task::JoinHandle<Vec<Value>>>,
) -> Response {
    let mut it = rest.split(':');
    let post_id = it.next().unwrap_or("").trim().to_string();
    let season: i32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let episode: i32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    if post_id.is_empty() || !post_id.chars().all(|c| c.is_ascii_digit()) {
        return stremio_json(200, &serde_json::json!({"streams": []}), "no-store");
    }

    let post = crate::wp_data::get_post(&post_id).await;
    let mut title = crate::wp_data::post_title(&post);
    let mut year = crate::wp_data::post_year(&post);
    // Step 2b: the `media_ids_idx` ROW is the fallback when the id is not a real
    // WordPress post (pseudo ids `9000000000+`, e.g. `pm:post:9001801064`). It
    // also carries the ALTERNATE title — which is exactly what makes
    // "Khurafat: Perjanjian Syaitan" playable while the origin indexes it as
    // "Superstition" (backend.php's AKA chain).
    let row = crate::wp_data::lookup_id("post", &post_id).await;
    if title.is_empty() {
        title = jstr(&row, "title");
    }
    if year.is_empty() {
        year = jstr(&row, "year");
        if year.is_empty() {
            year = jstr(&row, "release_year");
        }
    }
    let mut aka = jstr(&row, "aka");
    if title.is_empty() && !aka.is_empty() {
        title = std::mem::take(&mut aka);
    }
    // The id the upstream episode-name lookup asks about. Only the `media_ids_idx`
    // row knows it for a post id; an empty value simply means "no evidence"
    // (the guard fails OPEN), exactly like the PHP's `$episodeMediaId`.
    let imdb_id = jstr(&row, "imdb_id");
    if title.is_empty() {
        log(
            "stream: post has no title",
            &serde_json::json!({"post_id": post_id, "item_id": item_id}),
        );
        crate::diagnostics::record(
            "stream_incident",
            "post files could not be identified",
            "",
            "",
            "NO_STREAMS",
            "",
            item_id,
            serde_json::json!({"item_type": item_type, "post_id": post_id, "runtime": "pencarimovie-server"}),
        );
        // No title means the matcher has nothing to match against — answer with
        // the tappable "No Streams Found" card instead of an empty array. An
        // upstream item, when there is one, replaces that card: the PHP merges
        // upstreams BEFORE deciding `$hasPlayableStreams`.
        let merged = await_upstream(upstream, true).await;
        return finish_streams(ctx, item_type, item_id, item_id, merged, false, 0).await;
    }
    // Paged, never a single request: the origin returns newest-episode-first and
    // a single `limit=200` call truncates a long series (measured: E1 missing on
    // a 65-episode post).
    let mut files = crate::wp_data::fetch_post_files_paged(&post_id, 200, 600, 3).await;
    if files.is_empty() {
        // The post has NO files attached of its own (Khurafat's files live under
        // the alternate title). Search the origin by title, then by AKA.
        let kind = if item_type == "series" || season > 0 { "series" } else { "movie" };
        let (_, f) = crate::wp_data::stream_files_by_title(kind, &title, &year, item_id).await;
        files = f;
        if files.is_empty() && !aka.is_empty() {
            let (_, f2) = crate::wp_data::stream_files_by_title(kind, &aka, &year, item_id).await;
            files = f2;
        }
    }
    build_streams_from_files(
        ctx, item_type, item_id, &title, &year, &aka, &imdb_id, files, season, episode, upstream,
    )
    .await
}

/// The first 4-digit year in a string (`"2023–"`, `"2019-03-01"`, `"2019"`).
fn first_year_of(s: &str) -> String {
    let d: Vec<char> = s.chars().collect();
    if d.len() < 4 {
        return String::new();
    }
    for i in 0..=(d.len() - 4) {
        if d[i..i + 4].iter().all(|c| c.is_ascii_digit()) {
            return d[i..i + 4].iter().collect();
        }
    }
    String::new()
}

/// `tt…` / `tmdb:…` / `kitsu:…` / `mal:…` / `tvdb:…` / `anilist:…` /
/// `tvmaze:…` — identity lookup, origin title search, matcher. Returns `None`
/// when the id shape is not one of ours.
async fn handle_external_stream(
    ctx: &ReqCtx,
    item_type: &str,
    item_id: &str,
    upstream: Option<tokio::task::JoinHandle<Vec<Value>>>,
) -> Option<Response> {
    let (prefix, value, season, episode) = parse_external_id(item_id)?;
    let row = crate::wp_data::lookup_id(prefix, &value).await;
    let mut title = jstr(&row, "title");
    let mut year = jstr(&row, "year");
    if year.is_empty() {
        year = jstr(&row, "release_year");
    }
    let aka = jstr(&row, "aka");
    // The episode-name lookup asks about the IMDb id when the row knows one —
    // the PHP's `$imdbId !== '' ? $imdbId : $itemId` (backend.php:17084). A
    // `tvdb:` / `tvmaze:` id is usually NOT served by a meta provider (measured:
    // AIOMetadata answers `anilist:21` but not `tvdb:81797`), while the row's
    // `imdb_id` always is, so using the item id alone left the guard inert for
    // exactly the prefixes this port just learned to resolve.
    let episode_media_id = {
        let imdb = jstr(&row, "imdb_id");
        if imdb.is_empty() {
            item_id.to_string()
        } else {
            imdb
        }
    };

    // A row may already know which catalog post owns the id — then the post path
    // is exact and the title search is unnecessary.
    if title.is_empty() {
        if let Some(pid) = row
            .get("post_id")
            .and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))
        {
            let rest = if season > 0 {
                format!("{}:{}:{}", pid, season, episode)
            } else {
                pid
            };
            return Some(handle_post_stream(ctx, item_type, item_id, &rest, upstream).await);
        }
        // (1) ID-ONLY ORIGIN PROBE. `/stream-files` resolves the identity from
        // `id` itself and answers `resolved_title`, so an id that is not in the
        // local `media_ids_idx` can still be recovered in ONE origin call.
        let kind = if item_type == "series" || season > 0 { "series" } else { "movie" };
        let (by_id_title, by_id_files) =
            crate::wp_data::stream_files_by_title(kind, "", "", item_id).await;
        if !by_id_files.is_empty() {
            let t = if by_id_title.is_empty() {
                item_id.to_string()
            } else {
                by_id_title
            };
            log(
                "stream: external id resolved by origin id search",
                &serde_json::json!({"item_id": item_id, "title": t, "files": by_id_files.len()}),
            );
            return Some(
                build_streams_from_files(
                    ctx,
                    item_type,
                    item_id,
                    &t,
                    &year,
                    &aka,
                    &episode_media_id,
                    by_id_files,
                    season,
                    episode,
                    upstream,
                )
                .await,
            );
        }

        // (2) UPSTREAM META. `backend.php`'s `fd_resolve_external_media_metadata()`
        // asks the meta-capable addons (AIOMetadata) for `/meta/<type>/<id>`;
        // without this an unindexed id had NO title at all, so the matcher had
        // nothing to search and the item answered "No Streams Found". Measured
        // on a real phone: tt6751668, tt21952924, tt34809853, tt34991493.
        let settings = load_catalog_settings_merged();
        if let Some(body) = upstream::forward_meta(&settings, item_type, item_id).await {
            let meta = body.get("meta").cloned().unwrap_or(Value::Null);
            let name = jstr(&meta, "name");
            if !name.is_empty() {
                title = name;
                if year.is_empty() {
                    let rel = {
                        let r = jstr(&meta, "releaseInfo");
                        if r.is_empty() { jstr(&meta, "year") } else { r }
                    };
                    year = first_year_of(&rel);
                }
                log(
                    "stream: external id resolved from upstream meta",
                    &serde_json::json!({"item_id": item_id, "title": title, "year": year}),
                );
            }
        }
    }

    if title.is_empty() {
        log(
            "stream: external id resolved without a title",
            &serde_json::json!({"item_id": item_id}),
        );
        crate::diagnostics::record(
            "stream_incident",
            "external id resolved without a title",
            "",
            "",
            "NO_STREAMS",
            "",
            item_id,
            serde_json::json!({"item_type": item_type, "runtime": "pencarimovie-server"}),
        );
        let merged = await_upstream(upstream, true).await;
        return Some(finish_streams(ctx, item_type, item_id, item_id, merged, false, 0).await);
    }

    let kind = if item_type == "series" || season > 0 { "series" } else { "movie" };
    let (resolved, mut files) = crate::wp_data::stream_files_by_title(kind, &title, &year, item_id).await;
    // AKA fallback: "Superstition" finds nothing, "Khurafat" finds the files.
    if files.is_empty() && !aka.is_empty() {
        let (r2, f2) = crate::wp_data::stream_files_by_title(kind, &aka, &year, item_id).await;
        if !f2.is_empty() {
            files = f2;
            title = if r2.is_empty() { aka.clone() } else { r2 };
        }
    }
    // The origin answers `resolved_title` with the SEARCH KEYWORD, not the real
    // title (it drops a trailing type word: "The Super Mario Galaxy"), so the
    // matcher is given both names — the type-word rule is what keeps that case
    // from collapsing to a single stream.
    if !resolved.is_empty() && resolved != title {
        log(
            "stream: origin resolved title differs",
            &serde_json::json!({"item_id": item_id, "title": title, "resolved": resolved}),
        );
        title = resolved;
    }
    Some(
        build_streams_from_files(
            ctx,
            item_type,
            item_id,
            &title,
            &year,
            &aka,
            &episode_media_id,
            files,
            season,
            episode,
            upstream,
        )
        .await,
    )
}

/// `tt1234567[:S:E]`, `tmdb:123`, `kitsu:12:1`, `mal:5`, `tvdb:81797:1:1`,
/// `anilist:21:1:1`, `tvmaze:82:1:1`.
///
/// Every prefix the PHP resolves through `fd_resolve_external_media_metadata`
/// belongs here. `wp_data::lookup_id()` is prefix-agnostic (the WordPress
/// `/lookup-id` handler reads the generic `media_ids_idx` row), so a bridged
/// catalog row carrying `tvdb:` / `anilist:` / `tvmaze:` gets a LOCAL stream
/// list instead of only the meta bridge — which previously opened the item and
/// then answered the empty card, because `handle_external_stream()` returned
/// `None` and the route fell through to the upstream-only branch.
fn parse_external_id(item_id: &str) -> Option<(&'static str, String, i32, i32)> {
    let id = item_id.trim();
    if let Some(rest) = id.strip_prefix("tt") {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 6 {
            // `tt0944947:8:6` — the season/episode suffix is REQUIRED for a
            // series, exactly as the OpenSubtitles/Stremio id shape carries it.
            let tail = &rest[digits.len()..];
            let mut parts = tail.trim_start_matches(':').split(':');
            let season: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let episode: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            return Some(("imdb", format!("tt{}", digits), season, episode));
        }
        return None;
    }
    for (name, prefix) in [
        ("tmdb", "tmdb:"),
        ("kitsu", "kitsu:"),
        ("mal", "mal:"),
        ("tvdb", "tvdb:"),
        ("anilist", "anilist:"),
        ("tvmaze", "tvmaze:"),
    ] {
        if let Some(rest) = id.strip_prefix(prefix) {
            let mut parts = rest.split(':');
            let value = parts.next().unwrap_or("").trim().to_string();
            if value.is_empty() {
                return None;
            }
            let season: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let episode: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            return Some((name, value, season, episode));
        }
    }
    // `pm:post:` and `pm:file:` are handled before this point; an id with a
    // prefix we do not know is left to the upstream-only branch.
    None
}

/// The shared tail of both stream paths: matcher → split parts → filters →
/// sort → up to 100 playable items.
#[allow(clippy::too_many_arguments)]
async fn build_streams_from_files(
    ctx: &ReqCtx,
    item_type: &str,
    item_id: &str,
    title: &str,
    year: &str,
    aka: &str,
    episode_media_id: &str,
    files: Vec<crate::wp_data::WpFile>,
    season: i32,
    episode: i32,
    upstream: Option<tokio::task::JoinHandle<Vec<Value>>>,
) -> Response {
    let is_series = item_type == "series" || season > 0 || episode > 0;
    // Verified episode names (`fd_upstream_episode_names`) let the matcher
    // reject a file that names ANOTHER show's episode after this episode's
    // marker — the sibling-work case. It is looked up once per stream request,
    // 7 d cached, and `[]` (no metadata / transport failure / placeholder name)
    // means "no evidence": the matcher then fails OPEN, so adding this can never
    // hide a stream on its own.
    let episode_names: Vec<String> = if is_series && !episode_media_id.trim().is_empty() {
        let settings = load_catalog_settings_merged();
        crate::upstream::episode_names(
            &settings,
            episode_media_id,
            season,
            episode,
            title,
            year,
        )
        .await
    } else {
        Vec::new()
    };
    let mut matched: Vec<crate::wp_data::WpFile> = files
        .iter()
        .filter(|f| {
            if is_series {
                crate::matcher::series_file_matches_title(
                    &f.title,
                    title,
                    season.max(1),
                    episode.max(1),
                    year,
                    &f.caption,
                    &episode_names,
                )
            } else {
                crate::matcher::movie_file_matches_title(&f.title, title, year, &f.caption, aka)
            }
        })
        .cloned()
        .collect();
    // Last resort for a renamed film: match the files against the ALTERNATE
    // title only ("Khurafat: Perjanjian Syaitan" vs the indexed "Superstition").
    if matched.is_empty() && !is_series && !aka.is_empty() && aka != title {
        matched = files
            .iter()
            .filter(|f| crate::matcher::movie_file_matches_title(&f.title, aka, year, &f.caption, title))
            .cloned()
            .collect();
    }

    if matched.is_empty() {
        log(
            "stream: no file matched the title",
            &serde_json::json!({
                "item_id": item_id, "title": title, "year": year,
                "season": season, "episode": episode, "is_series": is_series,
            }),
        );
        crate::diagnostics::record(
            "stream_incident",
            "no file matched the title",
            title,
            year,
            "NO_STREAMS",
            "",
            item_id,
            serde_json::json!({
                "item_type": item_type, "title": title, "year": year,
                "season": season, "episode": episode, "runtime": "pencarimovie-server",
            }),
        );
        let merged = await_upstream(upstream, true).await;
        return finish_streams(ctx, item_type, title, item_id, merged, false, 0).await;
    }

    // Split-part annotation + ordering runs BEFORE the filters, because
    // `exclude_part_title` reads the flag it writes.
    let mut items: Vec<crate::matcher::FileItem> = matched
        .iter()
        .map(|f| crate::matcher::FileItem {
            title: f.title.clone(),
            caption: f.caption.clone(),
            file_size: f.file_size,
            ..Default::default()
        })
        .collect();
    crate::matcher::group_split_parts(&mut items);

    let settings = read_catalog_settings();
    let cfg = crate::matcher::StreamFilterConfig::from_json(
        settings.get("stream_config").unwrap_or(&Value::Null),
    );
    let kept = crate::matcher::apply_stream_filters(&items, &cfg);

    if kept.is_empty() {
        log(
            "stream: files filtered out by stream_config",
            &serde_json::json!({"item_id": item_id, "matched": matched.len()}),
        );
        crate::diagnostics::record(
            "stream_incident",
            "streams filtered out by the AIOStreams config",
            title,
            year,
            "FILTERED_OUT",
            "",
            item_id,
            serde_json::json!({
                "item_type": item_type, "title": title,
                "files_before_filter": matched.len(), "runtime": "pencarimovie-server",
            }),
        );
        let merged = await_upstream(upstream, true).await;
        return finish_streams(ctx, item_type, title, item_id, merged, true, matched.len()).await;
    }

    let mut scored: Vec<(f64, Value)> = kept
        .iter()
        .map(|(idx, _res_key)| {
            let f = &matched[*idx];
            let score = crate::matcher::stream_sort_score(
                &f.title,
                &f.caption,
                f.file_size,
                "auto",
            );
            (
                score,
                stream_item(ctx, &f.short_code, &f.title, &f.caption, f.file_size, &f.mime),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(100);

    let mut streams: Vec<Value> = scored.into_iter().map(|(_, v)| v).collect();
    let built = streams.len();
    // A browser / WebView player cannot demux Matroska: Chrome rejects
    // `video/x-matroska` inside `<video>`, aborts on the container and re-requests the
    // same file every few seconds. Measured on a real phone: `range: bytes=2848468527-`
    // (the last ~178 KB of a 2.8 GB MKV) repeating every 4-5 s while the server
    // answered 206 correctly — "it's not streaming". So for a browser UA the
    // playable containers come first. NOTHING is dropped: an MKV-only title still
    // lists, just after any MP4/WEBM alternative.
    if crate::matcher::wants_web_container(&ctx.hdr("user-agent")) {
        let (playable, matroska): (Vec<Value>, Vec<Value>) = streams.into_iter().partition(|s| {
            let hint = s
                .get("behaviorHints")
                .and_then(|b| b.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let url = s.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let name = if hint.is_empty() { url } else { hint };
            !name.to_ascii_lowercase().ends_with(".mkv")
        });
        let moved = matroska.len();
        streams = playable.into_iter().chain(matroska).collect();
        if moved > 0 {
            log(
                "stream: matroska moved last for a browser client",
                &serde_json::json!({"item_id": item_id, "mkv": moved}),
            );
        }
    }
    // Warm the resolve cache for everything in this list (backend.php:3701,
    // `fd_prewarm_streams_batch`). The list is built from metadata only, so the
    // Bot API file_id is resolved on the first Range request — the Kotlin app got
    // an instant Play out of exactly this and the port did not. Background, so the
    // list still returns immediately; a resolve error only logs.
    crate::resolve::prewarm_streams_batch(
        kept.iter().map(|(idx, _)| matched[*idx].short_code.clone()).collect(),
        &get_bot_id(),
        "stream",
    );
    // Upstream items go AFTER ours (backend.php:18124) — a local file is always
    // the better source, and the sponsor is still prepended by
    // `finish_streams()`, so the order ends up sponsor → ours → upstream.
    let upstream_items = await_upstream(upstream, built == 0).await;
    let upstream_count = upstream_items.len();
    streams.extend(upstream_items);
    log(
        "stream: list built",
        &serde_json::json!({
            "item_id": item_id, "title": title, "matched": matched.len(),
            "kept": built, "upstream": upstream_count,
        }),
    );
    finish_streams(ctx, item_type, title, item_id, streams, false, matched.len()).await
}

/// Final assembly, exactly like `backend.php:18206-18244`:
///
/// 1. an empty list is NOT `{"streams":[]}` — it carries ONE tappable card
///    ("Streams Filtered Out" + `#configure`, or "No Streams Found" + the
///    keyword search on THIS origin), so the user has somewhere to go;
/// 2. the sponsored message is prepended whenever the WordPress version
///    endpoint configures one — **even when the list is empty** (PHP: "ALWAYS
///    add the sponsored message on top if configured").
/// How long a finished stream list is reused (`/stream/*` response cache).
///
/// The PHP cached the same responses for 300 s and the missing cache was the
/// single biggest latency item here: a warm `/stream` measured **430-470 ms**
/// (origin + upstream addons recomputed every time) versus **~1 ms** from the cache.
/// 300 s is short enough that a freshly indexed file or a changed sponsor still
/// appears quickly.
const STREAM_CACHE_TTL: i64 = 300;

/// Cache key for a finished stream list.
///
/// The TYPE is part of the key: `tt123` is a different request as a movie and as a
/// series root. The base url is too, because every download url in the payload is
/// absolute (`http://192.168.1.5:8088/api/download/...`).
fn stream_cache_key(item_type: &str, item_id: &str, base_url: &str) -> String {
    format!("{}|{}|{}", item_type, item_id, base_url)
}

async fn finish_streams(
    ctx: &ReqCtx,
    item_type: &str,
    title: &str,
    item_id: &str,
    mut streams: Vec<Value>,
    filtered: bool,
    files_before_filter: usize,
) -> Response {
    if streams.is_empty() {
        streams.push(no_streams_card(ctx, title, item_id, filtered, files_before_filter));
    }
    if let Some(sponsor) = sponsor_item().await {
        streams.insert(0, sponsor);
    }
    let body = serde_json::json!({"streams": streams});
    crate::cache::put(
        "stream",
        &stream_cache_key(item_type, item_id, &ctx.base_url),
        &body,
        STREAM_CACHE_TTL,
        0,
    );
    stremio_json(200, &body, "no-store")
}

/// `⚠️ Streams Filtered Out` vs `No Streams Found` (backend.php:18206-18234).
fn no_streams_card(
    ctx: &ReqCtx,
    title: &str,
    item_id: &str,
    filtered: bool,
    files_before_filter: usize,
) -> Value {
    if filtered && files_before_filter > 0 {
        return serde_json::json!({
            "name": "⚠️ Streams Filtered Out",
            "description": format!(
                "🔍 {} stream(s) found on PencariMovie, but hidden by your active filters (Resolution / Codec / Size / Keywords).\n👉 Tap to open settings & adjust filters.",
                files_before_filter
            ),
            "externalUrl": format!("{}/#configure", ctx.base_url.trim_end_matches('/')),
        });
    }
    // The keyword comes from the TITLE, falling back to the item id. The link
    // points at the keyword search on the CURRENT origin, not at the sponsor.
    let keyword = if !title.trim().is_empty() {
        title.trim().to_string()
    } else {
        item_id.to_string()
    };
    let base = ctx.base_url.trim_end_matches('/');
    serde_json::json!({
        "name": "No Streams Found",
        "description": "⚠️ No streams available for this title.",
        "externalUrl": format!(
            "https://pencarimovie.com/search/{}?ddldomain={}",
            urlencode_component(&keyword),
            urlencode_component(base)
        ),
    })
}

/// The sponsored message from `GET {WP_API_BASE}/version` (`sponsor.{name,
/// description,url}`), cached 10 min. Absent configuration means no item.
async fn sponsor_item() -> Option<Value> {
    let version = crate::httpc::cached_wp_json(&crate::config::wp_version_url(), 600, 8).await;
    let sponsor = version.get("sponsor")?;
    let url = sponsor.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
    if url.is_empty() {
        return None;
    }
    let name = sponsor.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    let desc = sponsor
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    Some(serde_json::json!({
        "name": if name.is_empty() { "PencariMovie" } else { name },
        "description": if desc.is_empty() { "Join our Telegram channel for updates & requests" } else { desc },
        "externalUrl": url,
    }))
}

/// One Stremio stream item, in the PHP's exact shape
/// (`backend.php:17832-17907`):
///
/// ```text
/// line 1: <clean release filename>
/// line 2: 💾 1.6 GB • ⚡ Telegram • BluRay • HEVC • AAC5.1
/// ```
///
/// `behaviorHints.filename` carries the real name, `videoSize` the byte count,
/// and `notWebReady` is set for anything that is not a plain MP4/M4V/WebM —
/// PHP's rule, which is what keeps a Matroska stream from being handed to the
/// HTML5 player. `bingeGroup` (series only) carries RELEASE attributes and never
/// an episode number, because Stremio compares it with `===` across episodes.
fn stream_item(
    ctx: &ReqCtx,
    short_code: &str,
    title: &str,
    caption: &str,
    size: u64,
    mime_hint: &str,
) -> Value {
    // Generic `video.mp4` style names take their display title from the caption
    // (backend.php:17739-17744).
    let mut display_title = title.to_string();
    if !caption.is_empty()
        && fancy_regex::Regex::new("(?i)^(?:video(?:\\.\\d+)*|\\d+|document|file)\\.(?:mp4|mkv|avi|mov|ts|flv)$")
            .map(|re| re.is_match(title.trim()).unwrap_or(false))
            .unwrap_or(false)
    {
        if let Some(first) = caption.lines().next() {
            let first = first.trim();
            if !first.is_empty() {
                display_title = first.to_string();
            }
        }
    }
    let tags = crate::matcher::extract_media_tags(&display_title, caption);
    let clean = crate::matcher::clean_media_title(&display_title);
    let display_name = if !clean.is_empty() {
        clean.clone()
    } else if !display_title.is_empty() {
        display_title.clone()
    } else {
        format!("File ShortCode: {}", short_code)
    };
    let file_name = stream_filename(
        if clean.is_empty() { &display_title } else { &clean },
        mime_hint,
    );
    let mime = guess_video_mime(&file_name, mime_hint);

    // `name` = "PencariMovie <resolution badge>" plus a visual-tag line
    // (backend.php:17783-17796).
    let res_badge = if tags.resolution.is_empty() {
        "Direct".to_string()
    } else {
        tags.resolution.clone()
    };
    let mut visual_pills: Vec<&str> = Vec::new();
    for tag in ["HDR", "DV", "10bit", "IMAX"] {
        if tags.visual.iter().any(|v| v == tag) {
            visual_pills.push(tag);
        }
    }
    let name = if visual_pills.is_empty() {
        format!("PencariMovie {}", res_badge)
    } else {
        format!("PencariMovie {}\n{}", res_badge, visual_pills.join(" | "))
    };

    // metaPills order: platform, source, visual*, codec, audio, edition — unique
    // (backend.php:17750-17771).
    let mut meta_pills: Vec<String> = Vec::new();
    if !tags.platform.is_empty() {
        meta_pills.push(tags.platform.clone());
    }
    if !tags.source.is_empty() {
        meta_pills.push(tags.source.clone());
    }
    for v in &tags.visual {
        meta_pills.push(v.clone());
    }
    if !tags.codec.is_empty() {
        meta_pills.push(tags.codec.clone());
    }
    if !tags.audio.is_empty() {
        meta_pills.push(tags.audio.clone());
    }
    if !tags.edition.is_empty() {
        meta_pills.push(tags.edition.clone());
    }
    let mut spec_bits: Vec<String> = Vec::new();
    if size > 0 {
        spec_bits.push(format!("💾 {}", format_bytes(size)));
    }
    spec_bits.push("⚡ Telegram".to_string());
    for pill in meta_pills {
        if pill != "HDR" {
            spec_bits.push(pill);
        }
    }

    let mut description = format!("{}\n{}", display_name, spec_bits.join(" • "));
    let flags = language_flags(&display_title, caption);
    if !flags.is_empty() {
        description.push_str(&format!("\n💬 {}", flags.join(" / ")));
    }

    let ext = std::path::Path::new(&file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let web_ready = matches!(ext.as_str(), "mp4" | "m4v" | "webm");

    let mut hints = serde_json::Map::new();
    hints.insert("filename".into(), serde_json::json!(file_name.clone()));
    if !web_ready {
        hints.insert("notWebReady".into(), serde_json::json!(true));
    }
    if size > 0 {
        hints.insert("videoSize".into(), serde_json::json!(size));
    }
    hints.insert(
        "bingeGroup".into(),
        serde_json::json!(format!(
            "pencarimovie|{}|{}|{}",
            if tags.resolution.is_empty() { "unknown" } else { &tags.resolution },
            if tags.source.is_empty() { "unknown" } else { &tags.source },
            if tags.codec.is_empty() { "unknown" } else { &tags.codec },
        )),
    );

    let payload = serde_json::json!({
        "file_size": size,
        "file_name": file_name,
        "mime": mime,
        "short_code": short_code,
    });
    let b64 = crate::fileid::base64url_encode(payload.to_string().as_bytes());
    let url = build_stream_url(&ctx.base_url, &b64, &file_name);
    serde_json::json!({
        "name": name,
        "description": description,
        "url": url,
        "behaviorHints": Value::Object(hints),
    })
}

/// `💬 🇲🇾 Malay / 🇬🇧 English` (backend.php:17803-17830). Compiled once.
fn language_flags(title: &str, caption: &str) -> Vec<&'static str> {
    static PATTERNS: std::sync::OnceLock<Vec<(fancy_regex::Regex, &'static str)>> =
        std::sync::OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        let table: [(&str, &'static str); 8] = [
            (r"(?i)\b(malay|malaysub|sub\s*malay|msia|melayu)\b", "🇲🇾 Malay"),
            (r"(?i)\b(eng|english|esub|sub\s*eng)\b", "🇬🇧 English"),
            (r"(?i)\b(indo|indonesia|indosub)\b", "🇮🇩 Indo"),
            (r"(?i)\b(hindi|hin|dub\s*hindi)\b", "🇮🇳 Hindi"),
            (r"(?i)\b(korean|kor|kdrama)\b", "🇰🇷 Korean"),
            (r"(?i)\b(japanese|jap|anime)\b", "🇯🇵 Japanese"),
            (r"(?i)\b(chinese|mandarin|cantonese|c-drama)\b", "🇨🇳 Chinese"),
            (r"(?i)\b(thai)\b", "🇹🇭 Thai"),
        ];
        table
            .into_iter()
            .filter_map(|(p, label)| fancy_regex::Regex::new(p).ok().map(|re| (re, label)))
            .collect()
    });
    let text = format!("{} {}", title, caption);
    let mut out: Vec<&'static str> = Vec::new();
    for (re, label) in patterns {
        if re.is_match(&text).unwrap_or(false) && !out.contains(label) {
            out.push(label);
        }
    }
    out
}

fn build_stream_url(base_url: &str, payload_b64: &str, file_name: &str) -> String {
    let token = if auth_enabled() { auth_token() } else { String::new() };
    let token_segment = if token.is_empty() {
        String::new()
    } else {
        format!("/{}", urlencode_component(&token))
    };
    format!(
        "{}{}/api/download/{}/{}",
        rtrim(base_url),
        token_segment,
        urlencode_component(payload_b64),
        urlencode_component(file_name)
    )
}

/* ── small helpers ───────────────────────────────────────────────────────── */

fn req_for_headers() -> Request<Body> {
    Request::builder().body(Body::empty()).unwrap()
}

fn base_url_of(ctx: &ReqCtx, port: u16) -> String {
    let proto = {
        let p = ctx.hdr("x-forwarded-proto");
        if p.is_empty() {
            if ctx.hdr("cf-visitor").contains("https") {
                "https".to_string()
            } else {
                "http".to_string()
            }
        } else {
            p.split(',').next().unwrap_or("http").trim().to_string()
        }
    };
    let host = {
        let h = ctx.hdr("x-forwarded-host");
        if h.is_empty() {
            let h2 = ctx.hdr("host");
            if h2.is_empty() {
                format!("127.0.0.1:{}", port)
            } else {
                h2
            }
        } else {
            h
        }
    };
    format!("{}://{}", proto, host.split(',').next().unwrap_or("").trim())
}

fn urlencode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn srt_timestamps_to_vtt(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if line.contains("-->") {
            out.push_str(&line.replace(',', "."));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn rand_suffix() -> String {
    format!("{:08x}", rand::random::<u32>())
}

/// RFC1918 only: 10/8, 172.16/12, 192.168/16.
fn is_private_ipv4(ip: &str) -> bool {
    let parts: Vec<u8> = ip.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 4 {
        return false;
    }
    matches!(
        (parts[0], parts[1]),
        (10, _) | (192, 168) | (172, 16..=31)
    )
}

/// The address a device on the SAME NETWORK can reach, never the carrier's.
///
/// The UDP-connect trick reports the EGRESS address, which on mobile data is the
/// ISP's public IP — advertising `http://<public-ip>:8088` is wrong twice over: no
/// LAN device can reach it, and it leaks the carrier address into the dashboard and
/// the addon manifest. So the egress address is only accepted when it is private;
/// otherwise the interfaces are scanned (preferring wifi/ethernet) and, failing
/// that, loopback is returned so the caller falls back to the request's own host.
fn lan_ip() -> String {
    let egress = (|| -> Option<String> {
        let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        sock.connect("8.8.8.8:80").ok()?;
        let ip = sock.local_addr().ok()?.ip().to_string();
        if ip.contains('.') && !ip.starts_with("127.") && is_private_ipv4(&ip) {
            Some(ip)
        } else {
            None
        }
    })();
    if let Some(ip) = egress {
        return ip;
    }
    // `ip -4 addr` is present on Android (toybox), Linux and macOS; parse `inet`.
    if let Ok(out) = std::process::Command::new("ip").args(["-4", "addr"]).output() {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let mut best: Option<(i32, String)> = None;
        let mut iface = String::new();
        for line in text.lines() {
            let t = line.trim();
            if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && t.contains(':') {
                // "2: wlan0: <BROADCAST…>"
                iface = t.split(':').nth(1).unwrap_or("").trim().to_string();
                continue;
            }
            if let Some(rest) = t.strip_prefix("inet ") {
                let ip = rest.split('/').next().unwrap_or("").trim().to_string();
                if !is_private_ipv4(&ip) {
                    continue;
                }
                let name = iface.to_ascii_lowercase();
                let prio = if name.starts_with("ap") || name.starts_with("swlan") {
                    100
                } else if name.starts_with("wlan") {
                    90
                } else if name.starts_with("eth") || name.starts_with("rndis") {
                    80
                } else if name.starts_with("rmnet") || name.starts_with("tun") || name.starts_with("ppp") {
                    0
                } else {
                    50
                };
                if prio > 0 && best.as_ref().map(|(p, _)| prio > *p).unwrap_or(true) {
                    best = Some((prio, ip));
                }
            }
        }
        if let Some((_, ip)) = best {
            return ip;
        }
    }
    "127.0.0.1".to_string()
}

/// PHP `fd_load_catalog_settings()` defaults. The `stream_config` keys are the
/// ones the AIOStreams filter block reads, so an absent file must still produce
/// them (`exclude_unplayable` / `exclude_part_title` default to ON).
fn default_catalog_settings() -> Value {
    serde_json::json!({
        "catalogs_enabled": true,
        "country": "",
        "enabled_types": {"movie": true, "series": true, "other": true},
        "enabled_catalogs": {},
        "upstream_enabled": true,
        // Fresh installs ship with the PHP's two defaults (AIOMetadata for
        // catalog/meta, OpenSubtitles v3 for subtitles). `upstream_enabled` is
        // the single master switch; the stored list is kept when it is off, so
        // turning it back on restores everything without re-adding URLs.
        "upstream_manifests": upstream::default_manifests(),
        "stream_config": {
            "resolutions": {"4k": true, "1080p": true, "720p": true, "sd": true, "unknown": true},
            "qualities": {"remux": true, "bluray": true, "webdl": true, "webrip": true, "hdtv": true, "cam": true},
            "encodes": {"hevc": true, "avc": true, "av1": true},
            "visual_tags": {"hdr": true, "dv": true},
            "exclude_cam": false,
            "exclude_unplayable": true,
            "exclude_part_title": true,
            "preferred_resolution": "auto",
            "max_streams_per_resolution": 0,
            "max_streams_total": 0,
            "min_size_mb": 0,
            "max_size_gb": 0,
            "excluded_keywords": "",
            "required_keywords": ""
        }
    })
}

/// Stored settings merged over the defaults (one level deep for `stream_config`,
/// so a file written before a key existed still gains it).
fn load_catalog_settings_merged() -> Value {
    let mut base = default_catalog_settings();
    let stored = read_catalog_settings();
    let Some(stored_obj) = stored.as_object() else {
        return base;
    };
    for (k, v) in stored_obj {
        if k == "stream_config" {
            if let Some(sc) = v.as_object() {
                for (sk, sv) in sc {
                    base["stream_config"][sk] = sv.clone();
                }
            }
            continue;
        }
        // `upstream_manifests` is handled below: an EMPTY stored list must not
        // wipe the built-in defaults (that is the PHP's rule — see
        // backend.php:12011-12016 — otherwise every upgrade silently loses the
        // upstream addons, and the AIOMetadata default in particular).
        if k == "upstream_manifests" {
            continue;
        }
        base[k] = v.clone();
    }
    if let Some(list) = stored_obj.get("upstream_manifests").and_then(|v| v.as_array()) {
        let valid: Vec<Value> = list
            .iter()
            .filter(|m| !jstr(m, "url").is_empty())
            .cloned()
            .collect();
        if !valid.is_empty() {
            base["upstream_manifests"] = Value::Array(valid);
        }
    }
    base
}

fn read_catalog_settings() -> Value {
    std::fs::read_to_string(storage_path("catalog_settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn write_catalog_settings(data: &Value) -> bool {
    std::fs::write(
        storage_path("catalog_settings.json"),
        serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string()),
    )
    .is_ok()
}

/// `fd_get_default_catalog_options()` (backend.php:11743) — an **id → info map**,
/// not a list.
///
/// The dashboard's Catalog Configuration panel iterates
/// `Object.entries(catalog_options)` and files each row under
/// `info.group === 'special'` or the main list ([`public/app.js:700`]), so the
/// previous `{catalogs: [manifest rows]}` shape rendered an EMPTY panel: none of
/// its keys carried a `group`, so every entry was skipped. The PHP map is
/// reproduced verbatim, plus the port's two merged trending post rows.
fn default_catalog_options() -> Value {
    serde_json::json!({
        // The four rows this port's manifest actually advertises. They are not
        // in the PHP's map (the PHP builds equivalent rows under the `top` /
        // `year` ids) but the panel needs one checkbox per advertised row.
        "pm_movies": {"type": "movie", "group": "row", "name": "PencariMovie Movies", "default": true},
        "pm_latest_movies": {"type": "movie", "group": "row", "name": "Latest Movies", "default": true},
        "pm_series": {"type": "series", "group": "row", "name": "PencariMovie Series", "default": true},
        "pm_latest_series": {"type": "series", "group": "row", "name": "Latest Series", "default": true},
        // Special catalogs: Popular, New, Search, Telegram Files, Trending.
        "top": {"type": "movie", "group": "special", "name": "Popular (Movies)", "default": true},
        "year": {"type": "movie", "group": "special", "name": "New (Movies)", "default": true},
        "pm_search_movie": {"type": "movie", "group": "special", "name": "Search Movies", "default": true},
        "pm_series_top": {"type": "series", "group": "special", "name": "Popular (Series)", "default": true},
        "pm_series_year": {"type": "series", "group": "special", "name": "New (Series)", "default": true},
        "pm_search_series": {"type": "series", "group": "special", "name": "Search Series", "default": true},
        "pm_files_year": {"type": "other", "group": "special", "name": "New (Telegram Files)", "default": true},
        "pm_search_files": {"type": "other", "group": "special", "name": "Telegram Files (Search)", "default": true},
        "pm_trending_keywords": {"type": "other", "group": "special", "name": "Trending Keywords (Files)", "default": true},
        // The port's own merged trending rows. `group: "trending"` puts them in
        // the main list (only `special` gets its own column) and gives each row
        // its own checkbox, so it can be hidden without touching the PHP's
        // `pm_trending_keywords` (file-catalog) master toggle.
        "pm_trending_movies": {"type": "movie", "group": "trending", "name": "Trending Movies", "default": true},
        "pm_trending_series": {"type": "series", "group": "trending", "name": "Trending Series", "default": true},
        // Movies country catalogs.
        "pm_movies_malay": {"type": "movie", "group": "country", "name": "Malaysia (Movie)", "default": true},
        "pm_movies_indo": {"type": "movie", "group": "country", "name": "Indonesia (Movie)", "default": true},
        "pm_movies_korean": {"type": "movie", "group": "country", "name": "Korea (Movie)", "default": false},
        "pm_movies_japan": {"type": "movie", "group": "country", "name": "Japan (Movie)", "default": false},
        "pm_movies_anime": {"type": "movie", "group": "country", "name": "Anime (Movie)", "default": true},
        "pm_movies_chinese": {"type": "movie", "group": "country", "name": "China / HK (Movie)", "default": true},
        "pm_movies_thai": {"type": "movie", "group": "country", "name": "Thailand (Movie)", "default": false},
        "pm_movies_bollywood": {"type": "movie", "group": "country", "name": "India (Movie)", "default": true},
        "pm_movies_philippines": {"type": "movie", "group": "country", "name": "Philippines (Movie)", "default": false},
        "pm_movies_english": {"type": "movie", "group": "country", "name": "English (Movie)", "default": true},
        // Series country catalogs.
        "pm_series_kdrama": {"type": "series", "group": "country", "name": "K-Drama (Series)", "default": true},
        "pm_series_anime": {"type": "series", "group": "country", "name": "Anime (Series)", "default": true},
        "pm_series_japan": {"type": "series", "group": "country", "name": "J-Drama (Series)", "default": false},
        "pm_series_malay": {"type": "series", "group": "country", "name": "Malaysia (Series)", "default": true},
        "pm_series_cdrama": {"type": "series", "group": "country", "name": "C-Drama (Series)", "default": true},
        "pm_series_thai": {"type": "series", "group": "country", "name": "Thailand (Series)", "default": false},
        "pm_series_philippines": {"type": "series", "group": "country", "name": "Philippines (Series)", "default": false},
        "pm_series_india": {"type": "series", "group": "country", "name": "India (Series)", "default": true},
        "pm_series_english": {"type": "series", "group": "country", "name": "English (Series)", "default": true},
        "pm_series_indo": {"type": "series", "group": "country", "name": "Indonesia (Series)", "default": true}
    })
}

/// PHP's per-row gate on the descriptor (backend.php:15303-15336): a local row
/// is advertised when its media type is enabled AND `enabled_catalogs[id]` is
/// not explicitly `false`. A missing type / missing id means ENABLED, which is
/// what makes every row appear on a fresh install.
fn catalog_row_enabled(settings: &Value, cat: &Value) -> bool {
    let ctype = cat.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let type_on = settings
        .get("enabled_types")
        .and_then(|t| t.get(ctype))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !type_on {
        return false;
    }
    let id = cat.get("id").and_then(|v| v.as_str()).unwrap_or("");
    match settings.get("enabled_catalogs").and_then(|m| m.get(id)) {
        Some(v) => v.as_bool().unwrap_or(true),
        None => true,
    }
}

fn apply_stream_config(sc: &mut Value, input: &serde_json::Map<String, Value>) {
    if let Some(res) = input.get("resolutions").and_then(|v| v.as_object()) {
        sc["resolutions"] = serde_json::json!({
            "4k": res.get("4k").and_then(|v| v.as_bool()).unwrap_or(false),
            "1080p": res.get("1080p").and_then(|v| v.as_bool()).unwrap_or(false),
            "720p": res.get("720p").and_then(|v| v.as_bool()).unwrap_or(false),
            "sd": res.get("sd").and_then(|v| v.as_bool()).unwrap_or(false),
            "unknown": res.get("unknown").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    if let Some(q) = input.get("qualities").and_then(|v| v.as_object()) {
        sc["qualities"] = serde_json::json!({
            "remux": q.get("remux").and_then(|v| v.as_bool()).unwrap_or(false),
            "bluray": q.get("bluray").and_then(|v| v.as_bool()).unwrap_or(false),
            "webdl": q.get("webdl").and_then(|v| v.as_bool()).unwrap_or(false),
            "webrip": q.get("webrip").and_then(|v| v.as_bool()).unwrap_or(false),
            "hdtv": q.get("hdtv").and_then(|v| v.as_bool()).unwrap_or(false),
            "cam": q.get("cam").and_then(|v| v.as_bool()).unwrap_or(false),
            "unknown": q.get("unknown").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    if let Some(e) = input.get("encodes").and_then(|v| v.as_object()) {
        sc["encodes"] = serde_json::json!({
            "hevc": e.get("hevc").and_then(|v| v.as_bool()).unwrap_or(false),
            "avc": e.get("avc").and_then(|v| v.as_bool()).unwrap_or(false),
            "av1": e.get("av1").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    if let Some(v) = input.get("visual_tags").and_then(|v| v.as_object()) {
        sc["visual_tags"] = serde_json::json!({
            "hdr": v.get("hdr").and_then(|x| x.as_bool()).unwrap_or(false),
            "dv": v.get("dv").and_then(|x| x.as_bool()).unwrap_or(false),
        });
    }
    if let Some(pref) = input.get("preferred_resolution") {
        let pref = pref.as_str().unwrap_or("auto").trim().to_ascii_lowercase();
        let allowed = ["auto", "4k", "1080p", "720p", "sd"];
        sc["preferred_resolution"] =
            serde_json::json!(if allowed.contains(&pref.as_str()) { pref } else { "auto".to_string() });
    }
    let clamp = |v: Option<&Value>, lo: i64, hi: i64| -> i64 {
        let n = v.and_then(|x| x.as_i64()).unwrap_or(0);
        n.clamp(lo, hi)
    };
    if input.contains_key("max_streams_per_resolution") {
        sc["max_streams_per_resolution"] =
            serde_json::json!(clamp(input.get("max_streams_per_resolution"), 0, 50));
    }
    if input.contains_key("max_streams_total") {
        sc["max_streams_total"] = serde_json::json!(clamp(input.get("max_streams_total"), 0, 200));
    }
    if input.contains_key("min_size_mb") {
        sc["min_size_mb"] = serde_json::json!(clamp(input.get("min_size_mb"), 0, 100_000));
    }
    if input.contains_key("max_size_gb") {
        sc["max_size_gb"] = serde_json::json!(clamp(input.get("max_size_gb"), 0, 200));
    }
    for key in ["exclude_cam", "exclude_unplayable", "exclude_part_title"] {
        if let Some(v) = input.get(key) {
            sc[key] = serde_json::json!(v.as_bool().unwrap_or(false));
        }
    }
    for key in ["excluded_keywords", "required_keywords"] {
        if let Some(v) = input.get(key) {
            sc[key] = serde_json::json!(v.as_str().unwrap_or("").trim());
        }
    }
}

fn country_list() -> Value {
    serde_json::json!([
        {"code": "", "name": "Auto (Detected)"},
        {"code": "MY", "name": "Malaysia"},
        {"code": "ID", "name": "Indonesia"},
        {"code": "SG", "name": "Singapore"},
        {"code": "TH", "name": "Thailand"},
        {"code": "PH", "name": "Philippines"},
        {"code": "VN", "name": "Vietnam"},
        {"code": "KR", "name": "Korea"},
        {"code": "JP", "name": "Japan"},
        {"code": "CN", "name": "China"},
        {"code": "HK", "name": "Hong Kong"},
        {"code": "TW", "name": "Taiwan"},
        {"code": "IN", "name": "India"},
        {"code": "US", "name": "United States"},
        {"code": "GB", "name": "United Kingdom"},
        {"code": "AU", "name": "Australia"},
        {"code": "DE", "name": "Germany"},
        {"code": "NL", "name": "Netherlands"},
        {"code": "FR", "name": "France"},
        {"code": "CA", "name": "Canada"}
    ])
}

fn detect_country(ctx: &ReqCtx) -> String {
    for h in ["cf-ipcountry", "x-country-code"] {
        let v = ctx.hdr(h).trim().to_ascii_uppercase();
        if v.len() == 2 && v.chars().all(|c| c.is_ascii_alphabetic()) && v != "XX" {
            return v;
        }
    }
    String::new()
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 10.0 || i == 0 {
        format!("{:.0} {}", v, units[i])
    } else {
        format!("{:.1} {}", v, units[i])
    }
}

/// Minimal descriptor; the PHP's catalog builder is not ported (gap list).
/* ── catalog / meta builders (backend.php:11626 / 12082) ──────────────────── */

/// WordPress category slug per catalog id (backend.php:15869-15893, verbatim —
/// including the `filipino` alias for the Philippines rows and the
/// `pm_series_india` → `bollywood` alias the origin uses for one India bucket).
const CATALOG_CATEGORY: &[(&str, &str)] = &[
    ("pm_movies_malay", "malay"),
    ("pm_movies_indo", "indonesian"),
    ("pm_movies_korean", "korea"),
    ("pm_movies_japan", "japan"),
    ("pm_movies_anime", "anime"),
    ("pm_movies_chinese", "china"),
    ("pm_movies_thai", "thai"),
    ("pm_movies_bollywood", "bollywood"),
    ("pm_movies_philippines", "filipino"),
    ("pm_movies_pinoy", "filipino"),
    ("pm_movies_english", "english"),
    ("pm_series_malay", "malay"),
    ("pm_series_indo", "indonesian"),
    ("pm_series_kdrama", "korea"),
    ("pm_series_anime", "anime"),
    ("pm_series_japan", "japan"),
    ("pm_series_cdrama", "china"),
    ("pm_series_thai", "thai"),
    ("pm_series_philippines", "filipino"),
    ("pm_series_pinoy", "filipino"),
    ("pm_series_india", "bollywood"),
    ("pm_series_english", "english"),
];

/// Country slug → ISO-3166 code, for a `pm_<type>_<country>` id the category
/// map above does not cover. The WordPress `/posts` handler accepts the same
/// `country=<CC>` parameter the PHP passes for its detected-region `top` rows
/// (backend.php:15898-15901), so an id family the dashboard emits later
/// (`pm_movies_canada`, `pm_series_vietnam`, …) resolves without a code change.
const COUNTRY_SLUG_CC: &[(&str, &str)] = &[
    ("malay", "MY"),
    ("malaysia", "MY"),
    ("my", "MY"),
    ("indo", "ID"),
    ("indonesian", "ID"),
    ("indonesia", "ID"),
    ("id", "ID"),
    ("korean", "KR"),
    ("korea", "KR"),
    ("kdrama", "KR"),
    ("kr", "KR"),
    ("japan", "JP"),
    ("japanese", "JP"),
    ("jdrama", "JP"),
    ("jp", "JP"),
    ("chinese", "CN"),
    ("china", "CN"),
    ("cdrama", "CN"),
    ("cn", "CN"),
    ("hongkong", "HK"),
    ("hk", "HK"),
    ("thai", "TH"),
    ("thailand", "TH"),
    ("th", "TH"),
    ("philippines", "PH"),
    ("filipino", "PH"),
    ("pinoy", "PH"),
    ("ph", "PH"),
    ("bollywood", "IN"),
    ("india", "IN"),
    ("indian", "IN"),
    ("hindi", "IN"),
    ("in", "IN"),
    ("english", "US"),
    ("hollywood", "US"),
    ("usa", "US"),
    ("us", "US"),
    ("vietnam", "VN"),
    ("vn", "VN"),
    ("singapore", "SG"),
    ("sg", "SG"),
    ("taiwan", "TW"),
    ("tw", "TW"),
    ("australia", "AU"),
    ("au", "AU"),
    ("germany", "DE"),
    ("de", "DE"),
    ("netherlands", "NL"),
    ("nl", "NL"),
    ("france", "FR"),
    ("fr", "FR"),
    ("canada", "CA"),
    ("ca", "CA"),
];

/// `pm_movies_malay_2024` → `pm_movies_malay`, `pm_series_malay_year` →
/// `pm_series_malay` (backend.php:15865 `$baseCatId`). The year itself still
/// arrives through the `year`/`genre` extra, so only the id is normalised.
fn base_catalog_id(id: &str) -> &str {
    for suffix in ["_latest", "_year", "_genre", "_older"] {
        if let Some(base) = id.strip_suffix(suffix) {
            return base;
        }
    }
    if let Some((head, tail)) = id.rsplit_once('_') {
        if tail.len() == 4 && tail.chars().all(|c| c.is_ascii_digit()) {
            return head;
        }
    }
    id
}

fn country_code_for_slug(slug: &str) -> Option<&'static str> {
    let s = slug.trim().to_ascii_lowercase();
    COUNTRY_SLUG_CC
        .iter()
        .find(|(k, _)| *k == s.as_str())
        .map(|(_, cc)| *cc)
}

/// `pm_<type>_<country>` → ISO code, when the category map has no entry.
/// `pm_cat_*` / `pm_topkw_*` / `*_top` never match, so this can never shadow a
/// row that already worked.
fn catalog_country_code(cat_id: &str) -> Option<&'static str> {
    let rest = cat_id.strip_prefix("pm_")?;
    let (_type_word, slug) = rest.split_once('_')?;
    if slug.is_empty() {
        return None;
    }
    country_code_for_slug(slug)
}

/// The `<extra>` path segment EXACTLY as it arrived (still percent-encoded).
///
/// [`split_addon`] decodes it, which is right for the local catalog parser and
/// wrong for forwarding: the upstream addon expects the encoding it published.
fn raw_addon_extra(path: &str, prefix: &str) -> String {
    let Some(rest) = path.strip_prefix(prefix) else {
        return String::new();
    };
    let rest = rest.strip_suffix(".json").unwrap_or(rest);
    let mut it = rest.splitn(3, '/');
    let _item_type = it.next();
    let _id = it.next();
    it.next().unwrap_or("").to_string()
}

/// `/catalog/<type>/<id>[/<extra>].json`
///
/// Every segment is percent-decoded. The web UI (and Stremio clients) send the
/// id encoded (`pm%3Apost%3A9000016669`), and an undecoded `pm%3Apost%3A…` never
/// matches `pm:post:` — the meta router then answered `{"meta":{}}`, so a series
/// page rendered its title from the catalog row but "No episodes found." and
/// "No description.".
fn split_addon(path: &str, prefix: &str) -> Option<(String, String, String)> {
    let rest = path.strip_prefix(prefix)?;
    let rest = rest.strip_suffix(".json").unwrap_or(rest);
    let mut it = rest.splitn(3, '/');
    let a = crate::download::percent_decode(it.next()?);
    let b = crate::download::percent_decode(it.next()?);
    let c = crate::download::percent_decode(it.next().unwrap_or(""));
    Some((a, b, c))
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn post_id(post: &Value) -> String {
    post.get("id")
        .map(|v| {
            v.as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| v.as_str().unwrap_or("").to_string())
        })
        .unwrap_or_default()
}

/// `fd_clean_post_title()`: drop the "• Movie" / "| TvSeries" decoration.
fn clean_post_title(raw: &str) -> String {
    let decoded = crate::matcher::decode_html_entities(raw);
    let re = fancy_regex::Regex::new(
        "(?i)\\s*[•|·]\\s*(?:movie|tv\\s*series|tvseries|series|drama|episode|bahagian)\\s*$",
    );
    let stripped = match re {
        Ok(re) => re.replace_all(decoded.trim(), "").into_owned(),
        Err(_) => decoded.trim().to_string(),
    };
    collapse_ws(&stripped)
}

/// `fd_clean_post_plot()`: strip HTML, decode entities, cap the length.
fn clean_post_plot(raw: &str) -> String {
    let no_tags = match fancy_regex::Regex::new("<[^>]+>") {
        Ok(re) => re.replace_all(raw, " ").into_owned(),
        Err(_) => raw.to_string(),
    };
    let text = collapse_ws(&crate::matcher::decode_html_entities(&no_tags));
    text.chars().take(400).collect()
}

fn post_genres(post: &Value) -> Vec<String> {
    post.get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn post_release_year(post: &Value) -> String {
    for key in ["year", "release_year"] {
        if let Some(v) = post.get(key) {
            let s = v
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string());
            if s.len() >= 4 && s[..4].chars().all(|c| c.is_ascii_digit()) {
                return s[..4].to_string();
            }
        }
    }
    let hay = format!("{} {}", jstr(post, "title"), jstr(post, "date"));
    if let Ok(re) = fancy_regex::Regex::new("\\b(19\\d{2}|20\\d{2})\\b") {
        if let Ok(Some(m)) = re.captures(&hay) {
            if let Some(g) = m.get(1) {
                return g.as_str().to_string();
            }
        }
    }
    String::new()
}

fn post_is_series(post: &Value) -> bool {
    let hay = format!(
        "{} {}",
        jstr(post, "title"),
        post_genres(post).join(" ")
    );
    fancy_regex::Regex::new("(?i)tvseries|tv\\s*series|\\bseries\\b|season|episode|drama")
        .map(|re| re.is_match(&hay).unwrap_or(false))
        .unwrap_or(false)
}

/// How many post-files prefetches may run at once.
///
/// The Kotlin app landed on the same bound after measuring that a catalog page's 24
/// simultaneous `post_files` fetches were themselves what tripped the origin's rate
/// limiter (`[Resolve] upstream rate limit …` and `[PostFiles] page 0 failed: timeout`
/// in the same second). Two at a time warms a page without competing with the user.
static POST_PREFETCH_INFLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const MAX_POST_PREFETCH_INFLIGHT: usize = 2;

/// Kotlin `prefetchPostFiles()`: warm ONE post's file set off the critical path.
///
/// The catalog/meta warm caches the META but not the file set the stream route
/// reads, so the first Play of a title just browsed still paid a cold origin fetch
/// (measured in the Android app: 10.4 s cold → 2.2 s warm). Bounds, all three of
/// which matter:
///   1. skipped while a download is streaming, so it can never compete with live
///      playback for Telegram/origin bandwidth;
///   2. skipped while the origin is refusing resolves (`resolve_cooldown.marker`);
///   3. at most one warm per post per 300 s, checked BEFORE the mark is written so a
///      suppressed call can still warm later.
fn prefetch_post_files(post_id: &str) {
    let post_id = post_id.trim();
    if post_id.is_empty() || post_id == "0" {
        return;
    }
    if crate::download::active_streams() > 0 {
        log("post files prefetch skipped (stream active)", &serde_json::json!({"post": post_id}));
        return;
    }
    if crate::resolve::resolve_cooldown_remaining_sec() > 0 {
        log("post files prefetch skipped (resolve cooldown)", &serde_json::json!({"post": post_id}));
        return;
    }
    // A cache entry, not a 0-byte marker file: reading a marker cost two syscalls
    // (~200 us) on every info page, and expiry is now the entry's own deadline.
    if crate::cache::get("post_prefetch", post_id).is_some() {
        return;
    }
    crate::cache::put("post_prefetch", post_id, &serde_json::json!(1), 300, 0);
    let pid = post_id.to_string();
    tokio::spawn(async move {
        // Bounded wait for a slot rather than an unbounded spawn storm.
        let mut waited = 0u32;
        while POST_PREFETCH_INFLIGHT.load(std::sync::atomic::Ordering::SeqCst)
            >= MAX_POST_PREFETCH_INFLIGHT
        {
            if waited >= 100 {
                return;
            }
            waited += 1;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        POST_PREFETCH_INFLIGHT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let files = crate::wp_data::fetch_post_files_paged(&pid, 200, 600, 3).await;
        POST_PREFETCH_INFLIGHT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        log(
            "post files prefetched",
            &serde_json::json!({"post": pid, "files": files.len()}),
        );
    });
}

/// `buildPostMeta()` — the Stremio meta for one catalog post.
fn meta_from_post(post: &Value, item_type: &str) -> Value {
    let pid = post_id(post);
    let plot = {
        let e = jstr(post, "excerpt");
        if e.is_empty() {
            jstr(post, "content")
        } else {
            e
        }
    };
    let mut m = serde_json::json!({
        "id": format!("pm:post:{}", pid),
        "type": if item_type == "series" { "series" } else { "movie" },
        "name": clean_post_title(&jstr(post, "title")),
        "poster": jstr(post, "thumbnail_url"),
        "posterShape": "poster",
        "description": clean_post_plot(&plot),
        "genres": post_genres(post),
    });
    let year = post_release_year(post);
    if !year.is_empty() {
        m["releaseInfo"] = serde_json::json!(year);
    }
    m
}

/// The two merged trending rows advertised in the descriptor.
const TRENDING_MOVIE_CATALOG: &str = "pm_trending_movies";
const TRENDING_SERIES_CATALOG: &str = "pm_trending_series";

/// How many entries one trending row keeps. The PHP's per-keyword file catalogs
/// fetch 50; a merged post row is capped lower so the row stays browsable.
const TRENDING_ROW_MAX: usize = 40;
/// Keywords searched concurrently per round. `search` is a Manticore query, so a
/// large fan-out competes with the origin; two rounds of five fill a 40-entry
/// row for both types.
const TRENDING_BATCH: usize = 5;

/// One entry of a trending row, appended only when it belongs to this type.
/// `seen` dedupes by post id ACROSS keywords (the same title trends twice as
/// often as not) and never resets, so a later page cannot repeat an earlier one.
fn push_trending_metas(
    metas: &mut Vec<Value>,
    seen: &mut std::collections::HashSet<String>,
    posts: &[Value],
    item_type: &str,
    target: usize,
) {
    let want_series = item_type == "series";
    for post in posts {
        if metas.len() >= target {
            return;
        }
        let pid = post_id(post);
        if pid.is_empty() || !seen.insert(pid) {
            continue;
        }
        if post_is_series(post) != want_series {
            continue;
        }
        metas.push(meta_from_post(post, item_type));
    }
}

/// `pm_trending_movies` / `pm_trending_series`.
///
/// For every trending keyword the post search is asked once (30 posts), and only
/// the entries whose movie/series classification matches the row are kept. The
/// PHP turns each keyword into its own `pm_topkw_*` **file** catalog; this row
/// merges the keywords into one post row per type, so both depend on the same
/// keyword map ([`crate::wp_data::trending_keywords`]).
///
/// Keywords are searched a batch at a time so a cold request costs one round
/// trip per batch, not one per keyword, and the loop stops as soon as the row is
/// full.
async fn build_trending_row(item_type: &str, skip: usize) -> Vec<Value> {
    let keywords = crate::wp_data::trending_keywords().await;
    if keywords.is_empty() {
        return Vec::new();
    }
    let target = TRENDING_ROW_MAX + skip.min(400);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut metas: Vec<Value> = Vec::new();

    for chunk in keywords.chunks(TRENDING_BATCH) {
        if metas.len() >= target {
            break;
        }
        let futures = chunk.iter().map(|(kw, _)| {
            let kw = kw.clone();
            async move {
                let params = vec![
                    ("limit".to_string(), "30".to_string()),
                    ("offset".to_string(), "0".to_string()),
                    ("search".to_string(), kw),
                ];
                crate::wp_data::search_posts(&params).await
            }
        });
        // `join_all` keeps the keyword order, which is the origin's relevance
        // order — the same order the PHP's map preserves.
        for posts in futures_util::future::join_all(futures).await {
            push_trending_metas(&mut metas, &mut seen, &posts, item_type, target);
            if metas.len() >= target {
                break;
            }
        }
    }

    let pool = metas.len();
    let page: Vec<Value> = metas
        .into_iter()
        .skip(skip)
        .take(TRENDING_ROW_MAX)
        .collect();
    log(
        "trending catalog built",
        &serde_json::json!({
            "type": item_type,
            "keywords": keywords.len(),
            "pool": pool,
            "skip": skip,
            "count": page.len(),
        }),
    );
    page
}

async fn handle_catalog(ctx: &ReqCtx, path: &str) -> Response {
    let Some((item_type, cat_id, extra)) = split_addon(path, "/catalog/") else {
        return stremio_json(200, &serde_json::json!({"metas": []}), "no-store");
    };
    // ── upstream bridged catalog (`up_<i>_<realId>`) ──────────────────────
    // Forwarded verbatim (backend.php:15497-15537) with a 1 h disk cache, so a
    // row the user browsed repeatedly costs one request. The extra segment is
    // forwarded RAW: it still carries its own percent-encoding, and re-encoding
    // a decoded `genre=Science+Fiction` would corrupt it.
    if let Some((idx, real_id)) = upstream::parse_bridged(&cat_id) {
        let raw_extra = raw_addon_extra(path, "/catalog/");
        let query = ctx
            .query
            .iter()
            .map(|(k, v)| format!("{}={}", urlencode_component(k), urlencode_component(v)))
            .collect::<Vec<_>>()
            .join("&");
        let settings = load_catalog_settings_merged();
        if let Some(body) = upstream::forward_catalog(
            &settings,
            idx,
            &item_type,
            &real_id,
            &raw_extra,
            &query,
        )
        .await
        {
            return stremio_json(200, &body, "max-age=3600, public");
        }
        // A stale `up_*` id (the addon was removed, or the upstream is down):
        // an empty list, never the SPA fallback.
        return stremio_json(200, &serde_json::json!({"metas": []}), "no-store");
    }
    let mut search = String::new();
    let mut genre = String::new();
    let mut year = String::new();
    let mut skip = 0usize;
    for pair in extra.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "search" => search = v.replace('+', " "),
                "genre" => genre = v.replace('+', " "),
                "year" => year = v.to_string(),
                "skip" => skip = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    // A year sub-catalog carries it in the ID (`pm_movies_malay_2024`), and a
    // 4-digit `genre` is really a year (backend.php:15568-15578 / 15928-15938).
    if year.is_empty() {
        if let Some((_, tail)) = cat_id.rsplit_once('_') {
            if tail.len() == 4 && tail.chars().all(|c| c.is_ascii_digit()) {
                year = tail.to_string();
            }
        }
    }
    if year.is_empty() && genre.len() == 4 && genre.chars().all(|c| c.is_ascii_digit()) {
        year = genre.clone();
        genre.clear();
    }
    // `/catalog/*` response cache (the PHP's `cat_cache_`, 10 min). A warm row cost
    // ~190-470 ms to rebuild even with `posts` cached (per-post regex/derivation work
    // over 49-100 posts), and a client asks for every row of the home screen. Only a
    // non-empty list is stored, so an origin outage is never pinned for 10 minutes.
    let cat_key = format!("{}|{}|{}", item_type, cat_id, extra);
    if let Some(hit) = crate::cache::get("catalog", &cat_key) {
        if hit.get("metas").and_then(|m| m.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
            log(
                "catalog served from cache",
                &serde_json::json!({"type": item_type, "id": cat_id}),
            );
            return stremio_json(200, &hit, "no-store");
        }
    }
    let limit = if search.is_empty() { 100 } else { 30 };
    // ── trending rows (merged over the trending keyword map) ───────────────
    // Only the bare row (and its `skip` paging) is trending-derived; a client
    // that adds `search`/`genre` to this id wants the ordinary catalog path.
    if (cat_id == TRENDING_MOVIE_CATALOG || cat_id == TRENDING_SERIES_CATALOG)
        && search.is_empty()
        && genre.is_empty()
        && year.is_empty()
    {
        let row_type = if cat_id == TRENDING_MOVIE_CATALOG { "movie" } else { "series" };
        let metas = build_trending_row(row_type, skip).await;
        return stremio_json(200, &serde_json::json!({"metas": metas}), "no-store");
    }
    let mut params: Vec<(String, String)> = vec![
        ("limit".to_string(), limit.to_string()),
        ("offset".to_string(), skip.to_string()),
    ];
    let posts = if !search.is_empty() {
        params.push(("search".to_string(), search.clone()));
        crate::wp_data::search_posts(&params).await
    } else {
        params.push((
            "media_type".to_string(),
            if item_type == "series" { "series" } else { "movie" }.to_string(),
        ));
        // A year/genre sub-catalog (`pm_movies_malay_2024`) resolves through its
        // base id, exactly like the PHP's `$baseCatId`.
        let base_id = base_catalog_id(&cat_id);
        if let Some((_, slug)) = CATALOG_CATEGORY
            .iter()
            .find(|(id, _)| *id == base_id || *id == cat_id)
        {
            params.push(("category".to_string(), (*slug).to_string()));
        } else if let Some(slug) = cat_id.strip_prefix("pm_cat_") {
            // The dashboard's own genre rows (`pm_cat_action`, `pm_cat_indo`, …).
            let slug = if slug == "indo" { "indonesian" } else { slug };
            params.push(("category".to_string(), slug.to_string()));
        } else if let Some(cc) = catalog_country_code(base_id) {
            // `pm_<type>_<country>` the map above does not name — the origin's
            // own country filter, same parameter its `top` rows use.
            params.push(("country".to_string(), cc.to_string()));
        } else if cat_id == "top" || cat_id.ends_with("_top") {
            if year.is_empty() {
                params.push(("category".to_string(), "popular".to_string()));
            }
        }
        if !genre.is_empty() {
            params.push(("genre".to_string(), genre.clone()));
        }
        if !year.is_empty() {
            params.push(("year".to_string(), year.clone()));
        }
        crate::wp_data::posts(&params).await
    };

    let mut metas: Vec<Value> = Vec::new();
    for post in &posts {
        if post_id(post).is_empty() {
            continue;
        }
        let is_series = post_is_series(post);
        if item_type == "series" && !is_series {
            continue;
        }
        if item_type == "movie" && is_series {
            continue;
        }
        if !year.is_empty() {
            let ry = post_release_year(post);
            if !ry.is_empty() && ry != year {
                continue;
            }
        }
        metas.push(meta_from_post(post, &item_type));
    }
    log(
        "catalog built",
        &serde_json::json!({"type": item_type, "id": cat_id, "search": search, "count": metas.len()}),
    );
    // Warm the POST'S FILE SET for the movies on this page (Kotlin
    // `prefetchMetaFor`'s second loop). A catalog row's Play would otherwise pay a
    // cold `post_files` fetch; each call is bounded (2 at a time, one per post per
    // 300 s, skipped while streaming or rate-limited) so a page of 24 rows queues
    // instead of stampeding the origin.
    if item_type == "movie" {
        for m in metas.iter().take(24) {
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(pid) = id.strip_prefix("pm:post:") {
                prefetch_post_files(pid);
            }
        }
    }
    let body = serde_json::json!({"metas": metas});
    if body.get("metas").and_then(|m| m.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
        crate::cache::put("catalog", &cat_key, &body, 600, 0);
    }
    stremio_json(200, &body, "no-store")
}

async fn handle_meta(ctx: &ReqCtx, path: &str) -> Response {
    let Some((item_type, id, _)) = split_addon(path, "/meta/") else {
        return stremio_json(200, &serde_json::json!({"meta": Value::Null}), "no-store");
    };
    // A catalog post: details, plus a `videos` list for a series (one entry per
    // distinct S/E that the post's own files carry).
    if let Some(pid) = id.strip_prefix("pm:post:") {
        let post = crate::wp_data::get_post(pid).await;
        let mut meta = meta_from_post(&post, &item_type);
        if meta["name"].as_str().unwrap_or("").is_empty() {
            let row = crate::wp_data::lookup_id("post", pid).await;
            let title = jstr(&row, "title");
            if !title.is_empty() {
                meta["name"] = serde_json::json!(clean_post_title(&title));
                let year = jstr(&row, "year");
                if !year.is_empty() {
                    meta["releaseInfo"] = serde_json::json!(year);
                }
            }
        }
        if item_type == "series" {
            let files = crate::wp_data::fetch_post_files_paged(pid, 200, 600, 3).await;
            let mut seen = std::collections::BTreeSet::new();
            let mut videos: Vec<Value> = Vec::new();
            for f in &files {
                let (s, e, _end) = crate::matcher::classify_season_episode(
                    &f.title,
                    f.season_num,
                    f.episode_num,
                    &f.caption,
                );
                if e > 0 && seen.insert((s, e)) {
                    videos.push(serde_json::json!({
                        "id": format!("{}:{}:{}", id, s, e),
                        "season": s,
                        "episode": e,
                        "name": format!("S{:02}E{:02}", s, e),
                    }));
                }
            }
            if !videos.is_empty() {
                meta["videos"] = serde_json::json!(videos);
            }
        } else {
            // Kotlin `buildAndCachePostMeta()` warms the post's file set as it builds a
            // MOVIE meta (line 7503). A series already reads its files above, so this
            // only fills the gap that made the first Play of a just-opened movie pay a
            // cold origin fetch.
            prefetch_post_files(pid);
        }
        return stremio_json(200, &serde_json::json!({"meta": meta}), "no-store");
    }
    // A direct Telegram file: resolve the display title.
    if let Some(code) = id.strip_prefix("pm:file:") {
        let bot = get_bot_id();
        let resolved = if bot.is_empty() {
            resolve_shortcode_concurrent(code, &[]).await
        } else {
            resolve_shortcode(code, &bot, false).await
        };
        let title = {
            let t = jstr(&resolved, "title");
            if t.is_empty() {
                jstr(&resolved, "file_name")
            } else {
                t
            }
        };
        if title.is_empty() {
            return stremio_json(200, &serde_json::json!({"meta": Value::Null}), "no-store");
        }
        return stremio_json(
            200,
            &serde_json::json!({"meta": {
                "id": id,
                "type": if item_type == "series" { "series" } else { "movie" },
                "name": clean_post_title(&title),
                "posterShape": "poster",
            }}),
            "no-store",
        );
    }
    // ── meta bridge (backend.php:16410-16455) ─────────────────────────────
    // An id that is NOT ours (`tt…`, `tmdb:…`, `tvdb:…`, …) is forwarded to the
    // first upstream that declares `meta`. Without this every catalog row
    // bridged from AIOMetadata opens as a BLANK page: the client resolves the
    // upstream's own id against whichever addon advertises `meta` for that
    // prefix, and this addon advertises `tt`/`tmdb:`/… .
    let settings = load_catalog_settings_merged();
    if let Some(body) = upstream::forward_meta(&settings, &item_type, &id).await {
        return stremio_json(200, &body, "max-age=3600, public");
    }
    stremio_json(200, &serde_json::json!({"meta": Value::Null}), "no-store")
}

/// The catalog rows the dashboard, Stremio and Nuvio browse.
///
/// Deliberately small (Stremio rejects a descriptor over ~49 KB) and with **no
/// `genres` array** on any catalog — the genre values live in the `genre` extra,
/// exactly like cinemeta's own manifest.
fn pm_catalogs() -> Value {
    serde_json::json!([
        {"type": "movie", "id": "pm_movies", "name": "PencariMovie Movies",
         "extra": [{"name": "search", "isRequired": false}, {"name": "genre", "isRequired": false}, {"name": "skip", "isRequired": false}]},
        {"type": "movie", "id": "pm_latest_movies", "name": "Latest Movies",
         "extra": [{"name": "skip", "isRequired": false}]},
        // Merged trending rows: every valid keyword from `GET /trending`, split
        // by the post's movie/series classification. Both are served from the
        // same keyword map, so they cost nothing extra in the descriptor.
        {"type": "movie", "id": "pm_trending_movies", "name": "Trending Movies",
         "extra": [{"name": "skip", "isRequired": false}]},
        {"type": "series", "id": "pm_series", "name": "PencariMovie Series",
         "extra": [{"name": "search", "isRequired": false}, {"name": "genre", "isRequired": false}, {"name": "skip", "isRequired": false}]},
        {"type": "series", "id": "pm_latest_series", "name": "Latest Series",
         "extra": [{"name": "skip", "isRequired": false}]},
        {"type": "series", "id": "pm_trending_series", "name": "Trending Series",
         "extra": [{"name": "skip", "isRequired": false}]}
    ])
}

/* ── Eclipse music (backend.php:13568-14259) ─────────────────────────────── */

/// `{WP_API_BASE}/search-music?search=…&limit=…&t=…` → formatted `track` objects.
///
/// Called **directly** rather than through `fetch_stream_ajax`: the music search
/// is the one endpoint with a meaningful cache-buster (`t=<unixtime>`), because a
/// newly indexed track is invisible behind a cached page for minutes (the
/// documented "freshly added track does not match" bug).
async fn eclipse_search(query: &str, limit: usize) -> Vec<Value> {
    let url = format!(
        "{}/search-music?search={}&limit={}&offset=0&t={}",
        crate::config::wp_api_base().trim_end_matches('/'),
        urlencode_component(query),
        limit,
        crate::storage::now()
    );
    let res = crate::httpc::http_json(&url, &[], "GET", 15).await;
    let rows = if let Some(arr) = res.as_array() {
        arr.clone()
    } else {
        res.get("tracks")
            .or_else(|| res.get("items"))
            .or_else(|| res.get("files"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let mut tracks: Vec<Value> = rows
        .iter()
        .filter(|r| !str_at(r, "short_code").is_empty())
        .map(|r| crate::eclipse::format_track(r, query))
        .collect();
    // Best audio first, then the longest — the addon's own ordering.
    tracks.sort_by(|a, b| {
        let ka = crate::eclipse::quality_score(
            a["format"].as_str().unwrap_or(""),
            a["duration"].as_i64().unwrap_or(0) as u64,
        );
        let kb = crate::eclipse::quality_score(
            b["format"].as_str().unwrap_or(""),
            b["duration"].as_i64().unwrap_or(0) as u64,
        );
        kb.cmp(&ka)
    });
    tracks
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// `/eclipse/stream/<short_code>` and `/eclipse/play/<short_code>` — resolve and
/// hand back a downloadable URL. The URL extension must match the ADVERTISED
/// format (`.wav` for an ALAC `.m4a`, which [`crate::download`] decodes to PCM
/// behind a WAV header), or the client skips the track.
async fn eclipse_stream(ctx: &ReqCtx, short_code: &str) -> Response {
    let code = short_code
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if code.is_empty() {
        return stremio_json(404, &serde_json::json!({"error": "Not found"}), "no-store");
    }
    let bot = get_bot_id();
    let resolved = if bot.is_empty() {
        resolve_shortcode_concurrent(&code, &[]).await
    } else {
        resolve_shortcode(&code, &bot, false).await
    };
    let file_name = {
        let t = jstr(&resolved, "title");
        if t.is_empty() {
            jstr(&resolved, "file_name")
        } else {
            t
        }
    };
    if file_name.is_empty() {
        return stremio_json(404, &serde_json::json!({"error": "Track not found"}), "no-store");
    }
    let mime = {
        let m = jstr(&resolved, "mime");
        let m = if m.is_empty() { jstr(&resolved, "file_type") } else { m };
        m
    };
    let size = resolved.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
    let lower = file_name.to_lowercase();
    let is_alac = mime.contains("mp4") || lower.ends_with(".m4a") || lower.ends_with(".alac");
    let served_name = if is_alac {
        format!(
            "{}.wav",
            file_name
                .rsplit_once('.')
                .map(|(a, _)| a.to_string())
                .unwrap_or_else(|| file_name.clone())
        )
    } else {
        file_name.clone()
    };
    let served_mime = if is_alac {
        "audio/wav".to_string()
    } else if mime.is_empty() {
        "audio/mpeg".to_string()
    } else {
        mime
    };
    let payload = serde_json::json!({
        "file_size": size,
        "file_name": served_name,
        "mime": served_mime,
        "short_code": code,
    });
    let b64 = crate::fileid::base64url_encode(payload.to_string().as_bytes());
    let url = build_stream_url(&ctx.base_url, &b64, &served_name);
    stremio_json(
        200,
        &serde_json::json!({
            "url": url,
            "format": if is_alac { "wav" } else { served_name.rsplit('.').next().unwrap_or("mp3") },
            "mimeType": served_mime,
        }),
        "no-store",
    )
}

/* ── subtitles (backend.php:13789 + fd_get_item_subtitles) ────────────────── */

/// Default subtitle provider (PHP `FD_DEFAULT_SUBTITLE_MANIFEST_URL`). It matches
/// on the **IMDb id in the path** only — `videoHash`/`filename` are ignored by v3
/// — so every `pm:` id has to be translated to `tt…` first, and a series id must
/// keep its `:S:E` suffix or it answers an empty list.
const DEFAULT_SUBTITLE_BASE: &str = "https://opensubtitles-v3.strem.io";

/// Translate any id the addon can be asked about into an OpenSubtitles id.
/// `lookup_imdb_by_title()` with the 600 s NEGATIVE cache the TDLib app has
/// (`subtitleResolveMiss`) and the port was missing.
///
/// Only successes were ever cached, so a title with no IMDb entry re-ran the whole
/// chain — index lookup + own API search + upstream search, 4-7 s — on **every**
/// episode request, on the same event loop as the real work. Measured in PHP: one
/// episode cost 3.2 s with 2 chain calls, the next 0.5 s with none once the marker
/// existed. Keep the TTL short: it delays a title that appears in the index later.
const TITLE_IMDB_MISS_TTL: i64 = 600;

async fn imdb_by_title_cached(title: &str, year: &str) -> String {
    let key = format!("{}|{}", title.trim().to_lowercase(), year.trim());
    // A NEGATIVE cache entry (value `1`), so a hit is one hash lookup instead of a
    // marker-file stat, and the TTL is enforced by the cache itself.
    if crate::cache::get("title_imdb_miss", &key).is_some() {
        return String::new();
    }
    let imdb = crate::wp_data::lookup_imdb_by_title(title, year).await;
    if imdb.is_empty() {
        crate::cache::put(
            "title_imdb_miss",
            &key,
            &serde_json::json!(1),
            TITLE_IMDB_MISS_TTL,
            0,
        );
        log(
            "title imdb miss cached",
            &serde_json::json!({"title": title, "year": year, "ttl": TITLE_IMDB_MISS_TTL}),
        );
    } else {
        crate::cache::invalidate("title_imdb_miss", &key);
    }
    imdb
}

async fn subtitle_target_id(item_type: &str, id: &str) -> String {
    let lower = id.to_lowercase();
    if lower.starts_with("tt") {
        return id.to_string();
    }
    // `pm:post:<id>[:S:E]`
    if let Some(rest) = id.strip_prefix("pm:post:") {
        let mut parts = rest.split(':');
        let pid = parts.next().unwrap_or("");
        let season = parts.next().unwrap_or("");
        let episode = parts.next().unwrap_or("");
        let row = crate::wp_data::lookup_id("post", pid).await;
        let mut imdb = jstr(&row, "imdb_id");
        if imdb.is_empty() {
            imdb = jstr(&row, "imdb");
        }
        if imdb.is_empty() {
            // Resolve by title (two steps, both on our own stack).
            let title = jstr(&row, "title");
            let year = jstr(&row, "year");
            if !title.is_empty() {
                imdb = imdb_by_title_cached(&title, &year).await;
            }
        }
        if imdb.is_empty() {
            return String::new();
        }
        if !season.is_empty() && !episode.is_empty() {
            return format!("{}:{}:{}", imdb, season, episode);
        }
        if item_type == "series" {
            // A series without S/E can never match — answer empty rather than
            // letting the provider return a junk list.
            return String::new();
        }
        return imdb;
    }
    // `pm:file:<short_code>` -> title -> imdb
    if let Some(code) = id.strip_prefix("pm:file:") {
        let bot = get_bot_id();
        let resolved = if bot.is_empty() {
            resolve_shortcode_concurrent(code, &[]).await
        } else {
            resolve_shortcode(code, &bot, false).await
        };
        let mut title = jstr(&resolved, "title");
        if title.is_empty() {
            title = jstr(&resolved, "file_name");
        }
        if title.is_empty() {
            return String::new();
        }
        return imdb_by_title_cached(&title, "").await;
    }
    String::new()
}

async fn handle_subtitles(path: &str) -> Response {
    let Some((item_type, id, _extra)) = split_addon(path, "/subtitles/") else {
        return stremio_json(200, &serde_json::json!({"subtitles": []}), "no-store");
    };
    let target = subtitle_target_id(&item_type, &id).await;
    if target.is_empty() {
        log(
            "subtitles: no target id",
            &serde_json::json!({"item_type": item_type, "id": id}),
        );
        return stremio_json(200, &serde_json::json!({"subtitles": []}), "no-store");
    }
    let url = format!(
        "{}/subtitles/{}/{}.json",
        DEFAULT_SUBTITLE_BASE, item_type, target
    );
    let body = crate::httpc::cached_wp_json(&url, 3600, 12).await;
    let subs = body
        .get("subtitles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    log(
        "subtitles served",
        &serde_json::json!({"id": id, "target": target, "count": subs.len()}),
    );
    stremio_json(200, &serde_json::json!({"subtitles": subs}), "no-store")
}

/// The addon descriptor, with the upstream catalogs bridged in
/// (backend.php:15343-15489).
///
/// `up_<index>_<realCatalogId>` keeps the upstream's `type`/`name`/`extra` but
/// TRIMMED, because copying an upstream catalog verbatim carried every `extra`
/// `options` list and blew past Stremio's descriptor limit. The result is then
/// fitted to [`upstream::MANIFEST_DESCRIPTOR_BUDGET`] by dropping trailing
/// bridged catalogs — never the local `pm_*` ones.
async fn manifest(base_url: &str) -> Value {
    let mode: String = std::env::var("PM_MANIFEST_MODE")
        .unwrap_or_else(|_| "local".to_string())
        .chars()
        .filter(|c| c.is_ascii_lowercase())
        .collect();
    let mode = if mode.is_empty() { "local".to_string() } else { mode };

    let settings = load_catalog_settings_merged();
    let mut catalogs: Vec<Value> = pm_catalogs().as_array().cloned().unwrap_or_default();
    // `catalogs_enabled: false` advertises NO catalogs at all (the PHP empties
    // the list AND drops the `catalog` resource); otherwise each row still has
    // to pass its own media-type + checkbox gate.
    let catalogs_enabled = settings
        .get("catalogs_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if catalogs_enabled {
        catalogs.retain(|c| catalog_row_enabled(&settings, c));
    } else {
        catalogs.clear();
    }
    let local_count = catalogs.len();
    let mut types: Vec<String> = vec![
        "movie".to_string(),
        "series".to_string(),
        "other".to_string(),
    ];
    let (bridged, bridged_types) = upstream::bridged_catalogs(&settings).await;
    for t in bridged_types {
        if !types.contains(&t) {
            types.push(t);
        }
    }
    let bridged_count = bridged.len();
    catalogs.extend(bridged);
    log(
        "manifest built",
        &serde_json::json!({
            "local_catalogs": local_count,
            "bridged_catalogs": bridged_count,
            "types": types.len(),
        }),
    );

    // The PHP drops the `catalog` resource when nothing survived the filters,
    // so Stremio stops asking this addon for catalogs at all.
    let mut resources: Vec<&str> = vec!["meta", "stream", "subtitles"];
    if !catalogs.is_empty() {
        resources.insert(0, "catalog");
    }

    let doc = serde_json::json!({
        "id": format!("org.pencarimovie.addon.{}", mode),
        // The APP version (`1.0.52` on Android), not "1.0.0" and not the engine's
        // `git describe`: Stremio/Nuvio parse and DISPLAY this, so a fixed value
        // made every install look like the first release, and an engine stamp
        // made every APK release look like the same addon version.
        "version": crate::config::descriptor_version(),
        "name": "PencariMovie",
        "description": "Telegram movie, series and music catalogs with 4K streaming, subtitles and direct downloads.",
        "resources": resources,
        "types": types,
        // `tvdb:` / `anilist:` / `kitsu` are declared so a row bridged from an
        // upstream (whose ids are one of those) is still resolved against THIS
        // addon: that is what routes it through the meta bridge instead of
        // leaving the client with no addon to ask.
        "idPrefixes": ["pm:", "pm_", "tt", "tmdb:", "kitsu:", "kitsu", "mal:", "anilist:", "tvdb:"],
        "catalogs": catalogs,
        // A path that EXISTS in the embedded `public/`: the old `/logo.png`
        // 404s, so clients showed a broken logo (and an empty background).
        "logo": format!("{}/assets/images/icon.png", rtrim(base_url)),
        "background": format!("{}/assets/images/icon.png", rtrim(base_url)),
        "behaviorHints": {},
    });
    upstream::fit_descriptor(doc, local_count)
}

/* ── version (fd_check_version, backend.php:7719) ─────────────────────────── */

fn version_less(a: &str, b: &str) -> bool {
    let pa: Vec<i64> = a.split('.').map(|n| n.parse().unwrap_or(0)).collect();
    let pb: Vec<i64> = b.split('.').map(|n| n.parse().unwrap_or(0)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = *pa.get(i).unwrap_or(&0);
        let y = *pb.get(i).unwrap_or(&0);
        if x != y {
            return x < y;
        }
    }
    false
}

async fn check_version() -> Value {
    // The WordPress body is already cached by `cached_wp_json`; this extra memo is
    // for the DERIVED answer (version comparison), so it is keyed `version`.
    if let Some(hit) = crate::cache::get("version", "check") {
        let mut v = (*hit).clone();
        // `client_version` describes THIS process, not the cached answer: stamp it
        // on the way out so a cached body (TTL 1h) can never hide an APK update,
        // and so a server that never got `--app-version` reports "" rather than a
        // stale value from a previous run.
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "client_version".to_string(),
                serde_json::json!(crate::config::client_version()),
            );
            obj.insert(
                "build".to_string(),
                serde_json::json!(crate::config::build_id()),
            );
        }
        return v;
    }
    let r = cached_wp_json(&wp_version_url(), 3600, 8).await;
    let min_version = jstr(&r, "min_version");
    // `client_version` is the APP shell that launched this engine (the Android
    // APK's versionName). `current_version` stays the ENGINE version because the
    // minimum-version gate is about the engine; the dashboard displays
    // `client_version` when it exists so a new APK visibly changes the number.
    let client_version = crate::config::client_version();
    if min_version.is_empty() {
        return serde_json::json!({
            "ok": false,
            "update_needed": false,
            "current_version": app_version(),
            "client_version": client_version,
            "build": crate::config::build_id(),
        });
    }
    let current = app_version();
    let data = serde_json::json!({
        "ok": true,
        "update_needed": version_less(&current, &min_version),
        "current_version": current,
        "client_version": client_version,
        "build": crate::config::build_id(),
        "minimum_version": min_version,
        "update_url": jstr(&r, "update_url"),
        "release_notes": jstr(&r, "release_notes"),
    });
    crate::cache::put("version", "check", &data, 3600, 0);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_ip_accepts_only_rfc1918() {
        // A carrier address must never be advertised as the LAN IP.
        for bad in [
            "175.139.1.2",
            "8.8.8.8",
            "127.0.0.1",
            "169.254.10.1",
            "172.32.0.1",
            "192.169.1.1",
            "",
            "not-an-ip",
        ] {
            assert!(!super::is_private_ipv4(bad), "{bad} must be rejected");
        }
        for good in ["10.0.0.5", "172.16.4.9", "172.31.255.254", "192.168.2.118"] {
            assert!(super::is_private_ipv4(good), "{good} must be accepted");
        }
    }

    #[test]
    fn base_catalog_id_normalises_sub_catalogs() {
        // A year/genre sub-catalog resolves through its base id, exactly like
        // the PHP's `$baseCatId` (backend.php:15865).
        assert_eq!(base_catalog_id("pm_movies_malay_2024"), "pm_movies_malay");
        assert_eq!(base_catalog_id("pm_series_malay_year"), "pm_series_malay");
        assert_eq!(base_catalog_id("pm_movies_indo_genre"), "pm_movies_indo");
        assert_eq!(base_catalog_id("pm_movies_malay_latest"), "pm_movies_malay");
        // Ids that only LOOK similar are left alone.
        assert_eq!(base_catalog_id("pm_latest_movies"), "pm_latest_movies");
        assert_eq!(base_catalog_id("pm_movies"), "pm_movies");
        assert_eq!(base_catalog_id("pm_trending_movies"), "pm_trending_movies");
    }

    #[test]
    fn country_ids_fall_back_to_the_country_parameter() {
        // Known country slugs whose category row does NOT exist in the map.
        assert_eq!(catalog_country_code("pm_movies_canada"), Some("CA"));
        assert_eq!(catalog_country_code("pm_series_vietnam"), Some("VN"));
        assert_eq!(catalog_country_code("pm_movies_us"), Some("US"));
        // The mapping also has to work for the ones the category map DOES cover,
        // because it is only consulted when that map misses.
        assert_eq!(catalog_country_code("pm_movies_malay"), Some("MY"));
        // Rows that are not `<type>_<country>` never match, so they keep their
        // existing behaviour.
        assert_eq!(catalog_country_code("pm_cat_action"), None);
        assert_eq!(catalog_country_code("pm_movies_top"), None);
        assert_eq!(catalog_country_code("top"), None);
        assert_eq!(catalog_country_code("pm_movies"), None);
    }

    #[test]
    fn trending_row_splits_by_type_and_dedupes() {
        let posts = vec![
            serde_json::json!({"id": 1, "title": "The Rookie 2018 • TvSeries"}),
            serde_json::json!({"id": 2, "title": "The Rookie 1990 • Movie"}),
            serde_json::json!({"id": 3, "title": "The Rookies 2019 • Movie"}),
            serde_json::json!({"id": 2, "title": "The Rookie 1990 • Movie"}),
            serde_json::json!({"title": "no id"}),
        ];
        let mut seen = std::collections::HashSet::new();
        let mut movies = Vec::new();
        push_trending_metas(&mut movies, &mut seen, &posts, "movie", 40);
        assert_eq!(movies.len(), 2);
        assert_eq!(movies[0]["id"], "pm:post:2");
        assert_eq!(movies[1]["id"], "pm:post:3");
        assert_eq!(movies[0]["type"], "movie");

        let mut seen = std::collections::HashSet::new();
        let mut series = Vec::new();
        push_trending_metas(&mut series, &mut seen, &posts, "series", 40);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0]["id"], "pm:post:1");
        assert_eq!(series[0]["type"], "series");

        // The cap is honoured, and a row that only ever sees the other type
        // stays empty rather than borrowing the sibling's entries.
        let mut seen = std::collections::HashSet::new();
        let mut capped = Vec::new();
        push_trending_metas(&mut capped, &mut seen, &posts, "movie", 1);
        assert_eq!(capped.len(), 1);
        let mut seen = std::collections::HashSet::new();
        let only_movies = vec![serde_json::json!({"id": 9, "title": "Something 2020 • Movie"})];
        let mut empty = Vec::new();
        push_trending_metas(&mut empty, &mut seen, &only_movies, "series", 40);
        assert!(empty.is_empty());
    }

    #[test]
    fn catalog_options_are_a_grouped_map() {
        // The dashboard iterates `Object.entries(catalog_options)` and files
        // each row by `info.group` (public/app.js:700). A list-of-manifest-rows
        // shape made the panel render EMPTY, which is what this pins.
        let opts = default_catalog_options();
        let obj = opts.as_object().expect("options must be a map");
        assert!(obj.contains_key("top"), "PHP special row kept");
        assert!(obj.contains_key("pm_movies_malay"), "PHP country row kept");
        for (id, info) in obj {
            assert!(
                info.get("type").and_then(|v| v.as_str()).is_some()
                    && info.get("group").and_then(|v| v.as_str()).is_some()
                    && info.get("name").and_then(|v| v.as_str()).is_some(),
                "option {id} needs type/group/name"
            );
        }
        let tm = &opts["pm_trending_movies"];
        assert_eq!(tm["type"], "movie");
        assert_eq!(tm["group"], "trending");
        assert_eq!(tm["name"], "Trending Movies");
        let ts = &opts["pm_trending_series"];
        assert_eq!(ts["type"], "series");
        assert_eq!(ts["group"], "trending");
        // Every row the manifest advertises must have its own checkbox.
        for cat in pm_catalogs().as_array().cloned().unwrap_or_default() {
            let id = cat["id"].as_str().unwrap_or("");
            assert!(obj.contains_key(id), "no catalog option for {id}");
        }
    }

    #[test]
    fn catalog_row_gate_matches_php() {
        let cats = pm_catalogs();
        let movie = &cats[0];
        // Fresh install: everything on.
        let fresh = default_catalog_settings();
        assert!(catalog_row_enabled(&fresh, movie));
        // A media type switched off hides its rows.
        let mut no_movies = default_catalog_settings();
        no_movies["enabled_types"]["movie"] = serde_json::json!(false);
        assert!(!catalog_row_enabled(&no_movies, movie));
        // An explicit per-catalog `false` hides just that row; `true` and an
        // absent key both keep it (the PHP's `!isset || !empty` rule).
        let mut hidden = default_catalog_settings();
        hidden["enabled_catalogs"]["pm_movies"] = serde_json::json!(false);
        assert!(!catalog_row_enabled(&hidden, movie));
        let mut shown = default_catalog_settings();
        shown["enabled_catalogs"]["pm_movies"] = serde_json::json!(true);
        assert!(catalog_row_enabled(&shown, movie));
        // The trending rows follow the same gate.
        let trending = cats
            .as_array()
            .and_then(|a| a.iter().find(|c| c["id"] == TRENDING_MOVIE_CATALOG))
            .cloned()
            .expect("trending row");
        let mut off = default_catalog_settings();
        off["enabled_catalogs"][TRENDING_MOVIE_CATALOG] = serde_json::json!(false);
        assert!(!catalog_row_enabled(&off, &trending));
    }

    #[test]
    fn descriptor_advertises_the_two_trending_rows() {
        let cats = pm_catalogs();
        let arr = cats.as_array().cloned().unwrap_or_default();
        assert_eq!(arr.len(), 6, "4 base rows + 2 trending rows");
        let find = |id: &str| arr.iter().find(|c| c["id"] == id).cloned();
        let movies = find(TRENDING_MOVIE_CATALOG).expect("trending movies row");
        assert_eq!(movies["type"], "movie");
        assert_eq!(movies["name"], "Trending Movies");
        assert_eq!(movies["extra"][0]["name"], "skip");
        let series = find(TRENDING_SERIES_CATALOG).expect("trending series row");
        assert_eq!(series["type"], "series");
        assert_eq!(series["name"], "Trending Series");
        assert_eq!(series["extra"][0]["name"], "skip");
        // Never `genres`: Stremio caps the descriptor at ~49 KB and the genre
        // values belong in the `genre` extra (see pm_catalogs() docs).
        assert!(arr.iter().all(|c| c.get("genres").is_none()));
        // Two added rows must stay far below Stremio's descriptor cap.
        assert!(cats.to_string().len() < 1_500, "local rows are small");
    }

    #[test]
    fn external_id_prefixes_cover_every_php_resolvable_prefix() {
        // `tt…` keeps the full id and the S:E suffix.
        assert_eq!(
            parse_external_id("tt0944947:8:6"),
            Some(("imdb", "tt0944947".to_string(), 8, 6))
        );
        assert_eq!(
            parse_external_id("tt11540908:1:1"),
            Some(("imdb", "tt11540908".to_string(), 1, 1))
        );
        // An `tt` id shorter than 6 digits is not an IMDb id.
        assert_eq!(parse_external_id("tt123"), None);
        // The scalar prefixes, with and without `:S:E`.
        for (id, prefix, value) in [
            ("tmdb:123", "tmdb", "123"),
            ("kitsu:12:1", "kitsu", "12"),
            ("mal:5", "mal", "5"),
            // Task A: a bridged catalog row carrying one of these used to open
            // (meta bridge) and then answer the empty card, because the local
            // stream path never recognised the shape.
            ("tvdb:81797", "tvdb", "81797"),
            ("tvdb:81797:1:1", "tvdb", "81797"),
            ("anilist:21", "anilist", "21"),
            ("anilist:21:1:3", "anilist", "21"),
            ("tvmaze:82", "tvmaze", "82"),
            ("tvmaze:82:2:5", "tvmaze", "82"),
        ] {
            let parsed = parse_external_id(id).unwrap_or_else(|| panic!("{id} must parse"));
            assert_eq!(parsed.0, prefix, "{id} prefix");
            assert_eq!(parsed.1, value, "{id} value");
        }
        assert_eq!(
            parse_external_id("tvdb:81797:1:1"),
            Some(("tvdb", "81797".to_string(), 1, 1))
        );
        assert_eq!(
            parse_external_id("tvmaze:82:2:5"),
            Some(("tvmaze", "82".to_string(), 2, 5))
        );
        // A prefix with no value, an unknown prefix, and our own ids stay out.
        assert_eq!(parse_external_id("tvdb:"), None);
        assert_eq!(parse_external_id("anilist:"), None);
        assert_eq!(parse_external_id("foo:12"), None);
        assert_eq!(parse_external_id("pm:post:9000031194"), None);
        assert_eq!(parse_external_id("pm_file_1v9OPj"), None);
        assert_eq!(parse_external_id(""), None);
    }
}
