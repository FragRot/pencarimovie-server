# Rust parity ledger

Machine-readable source: [`../COVERAGE.md`](../COVERAGE.md) /
[`../ledger.json`](../ledger.json) (20,527 lines, 266 `fd_*` functions,
57 route patterns).

`COVERAGE.md` already has a `Rust` column, but it was filled in as
`N/A: Rust port not in scope` before this port existed. **This file supersedes
that column** â€” every group below is accounted for, and no row is dropped
silently. Status vocabulary:

- **ported** â€” a Rust counterpart exists **and** was exercised on the VPS.
- **partial** â€” counterpart exists but a documented part is missing.
- **N/A** â€” deliberately not applicable to this runtime, with the reason.
- **gap** â€” not ported yet, with the reason.

| Group            | # funcs | Rust                                            | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ---------------- | ------- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| foundation       | 15      | **ported**                                      | `storage.rs` (`app_root`, `storage_dir`, `storage_path`, `cache_path`, `prune_cache_files` with the persisted `.prune_stamp`, `limit_file_size`), `log.rs` (`fd_log`, `fd_log_stderr`, `fd_is_debug_enabled`, `fd_set_debug_enabled`), `config.rs` (`fd_env`, version, port), `respond.rs` (`fd_json`), device id in `diagnostics.rs`. `fd_is_windows` / `fd_can_spawn_process` are **N/A** (no PHP process model).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| auth             | 27      | **ported** (25) / **gap** (1) / **N/A** (1)     | `auth.rs`: tunnel + https + local + loopback detection, `fd_url_host_is_public` (resolves all A/AAAA and requires every address to be public), auth load/save/token/rotate/verify/set-password, lockout ladder 30/120/600/1800 s, token from `/<hex32+>/`, `/t/<token>/`, `?token=`, `X-Auth-Token`, `pm_auth`, `fd_require_auth`, privileged gate, `fd_strip_token_prefix`. **gap:** `fd_serve_auth_gate_html` (an HTML login page; this port answers JSON). **N/A:** `fd_auth_cache` as a separate function (memoised inside `auth_load`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| http             | 13      | **partial**                                     | `httpc.rs` ports `fd_http_json`, `fd_cached_wp_json` (disk cache keyed on the full URL + stale fallback, only truthy `ok` cached), `fd_fallback_url`, `fd_is_connection_reset`, `fd_http_get_contents` (telegra.my DPI retry), `fd_http_fire_and_forget`, `fd_http_download_file`. **N/A:** `fd_curl_resolve_entries`, `fd_get_amphp_client`, `fd_amphp_http_request`, `fd_http_get_many_amp` (no Amp/cURL). **gap:** `fd_resolve_host_ips` / `fd_get_upstream_manifest_hosts` (anycast pinning; reqwest uses the system resolver â€” only matters on Android/Termux, which this port does not target).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| telegram_session | 20      | **ported** (17) / **partial** (2) / **N/A** (1) | `session.rs`: meta, bot id, pool (add/remove/pick with round-robin index, cooldown, "never resurrect a cleared session" guard), `has_local_session`, guest provisioning with `O_EXCL` lock + heartbeat + 300 s reclaim, clock probe, skew message, friendly login error, orphan-session abort, clear session. `credentials.rs`: AES-256-CBC decrypt, cached credentials, WP fetch. **partial:** `fd_get_bot_session_path` returns the PHP directory name (`sessions/<id>/session.madeline`) for layout parity while the port's own file is `session.rust.json`; `fd_remove_pool_bot` does not restart a tunnel. **N/A:** `fd_environment_preflight` (no PHP extensions).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| madeline         | 11      | **ported behaviour** (2) / **N/A** (9)          | **N/A by design:** `fd_load_madeline_autoload`, `fd_ensure_autoload`, `fd_require_fileinfo`, `fd_ipc_worker_running`, `fd_ipc_worker_ready`, `fd_ipc_state_is_success`, `fd_ipc_state_is_failed`, `fd_ensure_ipc_worker`, `fd_prewarm_ipc_worker` â€” grammers is an in-process library, there is no IPC worker to supervise. `fd_boot_madeline` â†’ `telegram::TelegramPool::boot` (session resume, fresh `auth.importBotAuthorization`, per-bot session file, `--workers` clients). `fd_is_dpi_transport_error` â†’ **N/A** (the obfuscated-transport fallback lives in the PHP's MTProto stack; grammers has no equivalent switch). `fd_is_clock_or_authkey_error`, `fd_measure_clock_offset`, `fd_clock_skew_message`, `fd_friendly_login_error` are ported.                                                                                                                                                                                                                                                                                                                                                                                                 |
| resolve          | 11      | **ported** (10) / **N/A** (1)                   | `resolve.rs`: in-memory + `resolve_cache_<md5>.json` (2 h), `verified_at` preference, invalidate, 10 s fresh memo, `force=1&nocache=1` ladder with the 1.2/1.5/1.5 s polls, "not found for this request" short-circuit, concurrent candidate walk, batch `/resolve-files` with merge-preserving rows, `warmup_resolve_batch` (persists). `fd_decode_download_payload` in `download.rs`. **N/A:** `fd_spawn_warmup_resolve` (runs inline on the request). `fd_prewarm_streams_batch` is a **gap** (belongs to the title matcher, see `matching`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| matching         | 30      | **ported**                                      | `matcher.rs` ports the whole group: `fd_movie_file_matches_title` (strict equality, ±1-year guard, sequel guard, type-word suffix, AKA prefix, accent folding), `fd_series_file_matches_title` (embedded-year / codec-suffix / kind-marker rules), `fd_fetch_episode_stream_files`, `fd_classify_season_episode` (ranges, `Combined`, `_END`), `fd_extract_media_tags`, `fd_group_split_parts` (only with real sibling evidence), `fd_fold_latin_accents`, `fd_title_words_prefix_match`, and the AIOStreams filters. **The verified-episode-name chain is ported too** (`fd_normalize_episode_name`, `fd_episode_meta_identity_matches`, `fd_extract_episode_names`, `fd_upstream_episode_names` in `upstream.rs`, fed into step 5d of the series matcher). Evidence: `pm:post:` 5 / 101 / 7, `tt11540908:1:1` 15, `tt34384900` 49, `tt27369017` 50, and the "Do not regress" traps of the root `AGENTS.md` are pinned by 27 unit tests (plus 7 for the episode-name rules). See [verified episode names](#verified-episode-names-fd_upstream_episode_names).                                                                                                   |
| wp_data          | 31      | **partial** (11) / **gap** (20)                 | Ported: `/api/media-ids` POST/DELETE (proxied to WordPress), `/api/catalog-settings` GET/POST (including `upstream_enabled` and the whole `stream_config` shape, `stream_cache_*` invalidation), `/api/validate-manifest`, `/api/country` GET/POST, `/api/proxy-stream`, `/api/sub-proxy`, `fd_stream_ajax_ttl`-equivalent caching via `fd_cached_wp_json`, the post list + post search (`posts` / `search_posts`), post-files paging (`fd_fetch_post_files_paged`), and the trending keyword map (`fd_get_trending_keywords_map` + `fd_is_topkeyword_valid` as `trending_keywords` / `is_topkeyword_valid`). **gap:** Manticore lookups (`fd_lookup_local_catalog_by_prefix`, `fd_media_ids_upsert/delete` locally, `fd_manticore_lookup_by_id`) and the AKA/IMDb derivation chain â€” writes are still proxied to WordPress. `fd_prewarm_stream_list` is not ported (the catalog route does not warm a stream list).                                                                                                                                                                                                                                           |
| stremio          | 8       | **partial**                                     | `fd_stremio_json`, `fd_guess_video_mime`, `fd_mime_to_extension`, `fd_stremio_stream_filename`, `fd_build_stremio_stream_url` (token prefix + real extension), `fd_is_public_download_path`, `fd_extract_download_payload_from_path` are ported. **gap:** `fd_stremio_manifest_identity` (the port emits a fixed identity `org.pencarimovie.addon.<mode>` with `logo`/`background` and no per-address variants) and `fd_get_stremio_base_url` beyond the header-derived base URL.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| eclipse          | 3       | **ported**                                      | `eclipse.rs`: `fd_eclipse_parse_audio_title` (dash/by/known-artist/caption line, `AgAD` codes, noise stop-words), `fd_eclipse_audio_quality_score`, `fd_eclipse_format_track` (query-split rules + reversed known-artist tail, `durationMs`, `m4a`/`.alac`→`wav`). Routes: `/eclipse/search?q=`, `/eclipse/catalog/<id>` (latest), `/eclipse/stream                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | play/<code>`(extension rewritten to`.wav`when the payload is ALAC, and`format`/`mimeType`advertise`wav`/`audio/wav`so the URL extension and the advertised format agree). Measured: armada 30 tracks, adele 30, "the weeknd timeless" 16, catalog latest 50, stream resolves in ~1.2 s. Known simplification (same as the app): the exotic`AgAD` `Title Artist` caption walker is partly simplified, so some titles keep a codec suffix. |
| network          | 10      | **partial**                                     | `fd_get_lan_ip` â†’ `lan_ip()` (UDP-connect trick, no interface enumeration); `fd_is_https_request` ported. **gap:** the interface scoring table (`fd_is_usable_lan_ipv4`, `fd_is_skipped_lan_iface`, `fd_lan_iface_score`, `fd_pick_lan_ip_from_text`, `fd_lan_ip_from_php_ifaces`, `fd_cached_lan_ip`, `fd_get_lan_ip_fast`), `fd_is_android_runtime`, `fd_get_live_tunnel_https_origin`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| tunnel           | 40      | **ported**                                      | `tunnel.rs` ports the whole group: paths/state (`token.txt`, `state.json`, `cloudflared.pid`, `cloudflared{,.err}.log`, `config.yml` under `storage/tunnel/`), `read/write_pid`, `pid_alive` (metrics `/ready` first, then `tasklist`/`/proc`), `kill_pid` / `kill_leftovers` (recorded pid **plus** any `cloudflared.exe` still holding our metrics port — never a blanket `taskkill /IM`), binary resolution (`PM_CLOUDFLARED_BIN` → `<appRoot>/bin` → `storage/bin` → `storage/tunnel/bin` → the Windows install dirs → `PATH` → a GitHub download + `tar -xzf` for the Darwin `.tgz`), url/log parsers (`normalize_url`, `parse_url_from_text`, `parse_quicktunnel_body`, `parse_ingress_domains` — pure-JSON **and** logfmt `config="…"` log lines), the metrics-port discovery (`netstat -ano` / `ss -lntp`), `spawn`, `wait_for_url`, `auto_restart`, `extract_token` (`eyJh…` out of a whole `cloudflared.exe service install …`), and the three lifecycle entry points `enable` / `disable` / `get_tunnel_status`. Both tunnel kinds work (Quick Tunnel `--url` and Named Tunnel `run --token`). See the [tunnel section](#cloudflare-tunnel-tunnelrs). |
| diagnostics      | 14      | **partial** (12) / **gap** (2)                  | `diagnostics.rs`: settings, auto-report toggle, reports dir, device info (`/proc` RSS/FDs/meminfo), prune (10 per type), record, list, counts (the "strip only the trailing `_<digits>.json`" fix), clear. `fd_diag_install_fatal_handlers` â†’ **N/A** (no PHP request lifecycle; the axum handler catches errors). **gap:** `fd_diag_report_send` / `fd_diag_flush_pending` â€” `/api/reports?send` answers `ok:0, "Report upload is not ported."` instead of a silent 0.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| version          | 2       | **partial**                                     | `fd_check_version` is ported (1 h cache, `update_needed`, 426 gate) using `fd_cached_wp_json`. `fd_update_version_state` (on-disk stamp) is a **gap**; the port reads `.release-tag` only.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

## Ungrouped functions (COVERAGE.md:270-278)

| Function                                                                                 | Rust                                                                               |
| ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `fd_is_temp_app_dir`                                                                     | ported (`storage::is_temp_app_dir`)                                                |
| `fd_storage_has_session`                                                                 | ported                                                                             |
| `fd_prewarm_stream_list`                                                                 | **gap** (belongs to the matcher/catalog surface)                                   |
| `fd_spawn_backend_cli`                                                                   | **N/A** (no detached PHP CLI workers; work runs inline or in a spawned tokio task) |
| `fd_get_item_subtitles` / `fd_resolve_subtitle_target_id` / `fd_shortcode_display_title` | **ported** (`subtitle_target_id()` + `wp_data::lookup_imdb_by_title()`)            |

## Routes

| Pattern                                                                                                                 | Rust                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --- | ------------------------------------------------------------ |
| `/`                                                                                                                     | **ported** - the dashboard is EMBEDDED in the binary (`include_dir!` over `public/`, 113 files / ~16 MB, 28,216,832 B exe) and served with the right content type. Verified on Windows from an empty directory with the exe alone: `/` 200 `text/html`, `/app.bundle.js` 200 in 4.6 ms, `/styles.css` and `/assets/images/icon.png` 200. `PM_ASSETS=disk` reads the checkout instead.                                                                                                                                                                                                                                                                                                                                          |
| `/api/download`, `/api/download/*`                                                                                      | **ported** â€” payload from `?d=`, POST body, or path segment; Range/416/206; retry ladder; diagnostic record **ALAC:** an ALAC `.m4a` is decoded to 16-bit PCM with Symphonia and answered as `audio/wav` (RIFF header, `Range` mapped to sample frames) instead of transcoding to disk                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `/api/auth/*` (login, logout, password, reset, status, token/rotate)                                                    | ported                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/version`, `/api/session`, `/api/lan-ip`, `/api/clock-check`, `/api/provision`                                     | ported                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/bots`, `/api/bots/add`, `/api/bots/remove`, `/api/bots/set-active`                                                | ported                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/botlogin`, `/api/botlogout`                                                                                       | ported (fresh login, identity cleared first on logout)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/catalog-settings` (GET/POST), `/api/country` (GET/POST), `/api/media-ids` (POST/DELETE), `/api/validate-manifest` | ported — `catalog_options` is the PHP's **id → {type, group, name, default}** map (35 rows: specials, country rows, the four base rows, the two trending rows), which is the shape the dashboard's Catalog Configuration panel iterates (`Object.entries`, filed by `info.group`). A list-of-manifest-rows payload rendered the panel EMPTY. `catalogs_enabled: false` empties local **and** bridged catalogs and drops the `catalog` resource; `enabled_types` / `enabled_catalogs[id]` gate each advertised row. Pinned by `tools/probe_catalog_toggle.mjs`.                                                                                                                                                                 |
| `/api/diagnostics`, `/api/diagnostics/settings`, `/api/reports`, `/api/debug-mode`, `/api/logs`                         | ported (report upload = explicit gap)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `/api/proxy-stream`, `/api/sub-proxy`                                                                                   | ported                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/resolve-shortcode`, `/api/warmup-resolve`                                                                         | ported                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/api/tunnel/status`                                                                                                    | **ported** — reports `enabled` / `running` / `pid` / `metrics_port` / `tunnel_url` / `public_url` / `manifest_url` from a LIVE process (metrics `/ready` + `tasklist`, plus `/quicktunnel` and the log for the URL). Also the watchdog: when the state says enabled but the process is gone it touches `storage/tunnel/.last_autorestart` and revives it on a **background** tokio task (300 s stamp, so a 2 s status poll can never queue a restart storm). Verified against a real Quick Tunnel: `running=true` with the spawned pid, and after `taskkill` of `cloudflared` a single status poll produced a NEW pid + NEW `*.trycloudflare.com` URL that served `/manifest.json` 200.                                        |
| `/api/tunnel/enable`, `/api/tunnel/disable`                                                                             | **ported** — `enable` behind `fd_require_privileged` (loopback or a valid token), returns HTTP 200 on `ok:1` and 500 otherwise; `disable` kills the process tree, clears `state.json`/the pid file and **keeps `token.txt`**. Measured real binary: enable 9.2 s → live URL serving `manifest.json` 200 (7,636 B through Cloudflare); disable 0.29 s → pid gone, URL 502.                                                                                                                                                                                                                                                                                                                                                      |
| `/manifest.json`, `/nuvio`, `/nuvio/*`, `/stremio`, `/stremio/*`, `addon:/manifest.json`                                | ported - **6 local** `pm_*` catalogs (4 base + `pm_trending_movies` + `pm_trending_series`) **plus every upstream catalog bridged as `up_<index>_<realId>`** (trimmed `extra`), `types` extended with the upstream catalog types, descriptor fitted to **< 45,000 B** (trailing bridged catalogs dropped, local ones never). Measured with the seeded defaults: **7,514 B / 46 catalogs (6 local + 40 bridged)**.                                                                                                                                                                                                                                                                                                              |
| `addon:/configure`, `/configure`                                                                                        | 302 â†’ `/#addon`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `/stream/*`                                                                                                             | **ported** — `pm_file_*`/`pm:file:*`, `pm:post:<id>[:S:E]`, `tt…`/`tmdb:`/`kitsu:`/`mal:`/`tvdb:`/`anilist:`/`tvmaze:` all build real lists, and series requests feed **verified upstream episode names** into the matcher. Verified on Windows: `pm:post:9000031194` → 5, `pm:post:8591885201` → 101, `pm:post:9001801064` → 5, `pm:post:9000016669:1:1` → 7, `tt11540908:1:1` → 15, `tt34384900` → 49, `tt27369017` → 50, `tvdb:81797:1:1` → 101, `anilist:21:1:1` → 101, `tvmaze:82:1:1` → 38; each item is then served as **HTTP 206** with a real MP4 body. Remaining narrowness: a catalog post whose id has no WordPress post and no `media_ids` row still answers empty (the AKA/IMDb chain is a gap).                 |
| `/catalog/*`                                                                                                            | **ported** - `/posts` (list, `media_type`/`category`/`genre`/`year`/`limit`/`offset`), `/search` (query), series/movie segregation, `pm:post:` ids, the `CATALOG_CATEGORY` rows, `pm_cat_<genre>` rows, a `pm_<type>_<country>` → `country=<CC>` fallback, year sub-catalogs (`pm_movies_malay_2019` → base id + year), and the two **trending** rows (`pm_trending_movies` / `pm_trending_series`). Measured live: 99 movie metas, 99 series metas (the totals float with the live index — the probes assert floors), `search=carrie` 18, `genre=Drama` 97, `skip=20` 97 (< the first page), `pm_cat_action` 98, `pm_movies_canada` 98, `pm_movies_malay_2019` 60 (all 2019), trending movies **40**, trending series **40**. |
| `/meta/*`                                                                                                               | **ported** - `pm:post:` -> post meta (+ a `videos` list for a series, one entry per distinct S/E: Kudrat = 4), `pm:file:` -> resolved title, anything else `{"meta":null}`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `/subtitles/*`                                                                                                          | **ported** - `pm:post:`/`pm:file:` translated to `tt…` (row `imdb_id`, else title lookup with decorations stripped), series keep `:S:E`, then the OpenSubtitles v3 list is proxied and cached 1h. Measured: `tt4154756` 98, `tt0944947:1:1` 94, `pm:file:1v9OPj` 98, `pm:post:9000031194` 2, a post with no IMDb row 0.                                                                                                                                                                                                                                                                                                                                                                                                        |
| `/eclipse`, `/eclipse/*`                                                                                                | **ported** - descriptor, `/eclipse/search`, `/eclipse/catalog/<id>`, `/eclipse/stream/<code>`, `/eclipse/play/<code>`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `re:^/([0-9a-fA-F]{32,})(?:/                                                                                            | $)`, `re:^/t/([A-Za-z0-9]+)(?:/                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | $)` | ported (`strip_token_prefix`, `{,}`-free: any length â‰¥ 32) |
| unknown `/api/*`                                                                                                        | 404 `{"ok":0,"message":"Unknown API endpoint"}` (never an SPA fallback)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## Deliberate design differences (not gaps)

1. **No IPC workers** â€” one in-process grammers `Client` per bot per worker
   (`--workers N` for more connection sets). Measured in
   [`README.md`](README.md#measured-1-client--4-tasks-vs-4-clients--1-task).
2. **Headers are committed after the first chunk** â€” the PHP sends them first;
   committing late lets a failed first fetch answer JSON instead of hanging the
   client on a `Content-Length` that never completes.
3. **`storage/sessions/<bot>/session.rust.json`** â€” the port never reads or
   writes a MadelineProto session.
4. **Auth hashing** â€” `bcrypt` hashes written by PHP are verified; the port
   writes its own `sha256$salt$hash` (20,000 rounds). The two runtimes are
   separate installs.
5. **Log timestamps are UTC** (the PHP used the process timezone; this binary
   carries no tz database).

6. **Dashboard embedded in the executable** - `public/` is compiled in
   (`include_dir!`), so the port is a single `pencarimovie-rust.exe`: no
   `public/` folder, no PHP, no Composer, no `vendor/`. A frontend change
   needs `cargo build --release` again; `PM_ASSETS=disk` reads `public/`
   from the checkout for UI iteration.

## Music playback: ALAC → WAV PCM (`audio.rs`)

Android/Eclipse cannot decode ALAC in an `.m4a`. The PHP port solved that by
transcoding to FLAC **on disk**, which needed a detached encode-only worker, a
per-short-code conversion `flock`, a prewarm worker and a 30 s first play. The
port does it **inside the request**: the `.m4a` is pulled from Telegram into
memory in 1 MB batches, decoded from a `Cursor`, and the PCM is framed by a
44-byte canonical WAV header. Nothing is written to `storage/`, so neither PHP
trap can occur — nothing is piped into the decoder, and no second decoder ever
touches the same file.

- `probe(bytes) -> Option<WavInfo>` / `probe_why(...) -> Result<_, String>` —
  Symphonia probe + default track. **The channel layout is taken from the
  decoder's first packet, not from `codec_params`: ALAC keeps its layout in the
  magic cookie, and `symphonia-format-isomp4` only fills `extra_data` + sample
  rate for an `alac` stsd entry** (so a `params.channels` check rejects every
  real ALAC file — that cost a round trip). The first packet's 16-bit PCM is kept
  and reused when a range starts inside it.
- `wav_header(total_pcm_bytes, sample_rate, channels) -> [u8; 44]` —
  `RIFF`/`fmt `/`data`, 16-bit PCM, data length clamped to a valid RIFF size.
  A bare PCM stream is unplayable; WAV **is** the container here.
- `decode_range(bytes, start_pcm_byte, want)` — `frame = (offset - 44) /
bytes_per_frame`, seek with `SeekMode::Accurate` + `SeekTo::Time`, decode
  forward, emit 16-bit LE. A failed/refused seek is not fatal (decode forward).
- `download.rs` branches **before** the byte-for-byte path when the resolved
  file is ALAC (`.m4a`/`.alac` name, or an `audio/*` mime naming mp4/alac — a
  `video/mp4` is never routed here) and `probe_why` agrees. A HEAD is answered
  from the header alone, without decoding. If the probe rejects the file (e.g.
  **AAC** in an `.m4a`, which Android decodes natively) the ordinary download
  runs unchanged and the reason is logged.
- `--audio-probe <file.m4a> [start_pcm_byte] [bytes]` explains a rejection
  and decodes a window locally.

Verified end to end on `lM6tIJ` ("LINKIN PARK – The Emptiness Machine", ALAC
24-bit/48 kHz, 43,850,194 B `.m4a`): `HEAD` 200 `audio/wav`
`Content-Length: 36562172`; `bytes=0-1023` 206 with `RIFF`/`WAVE`/`fmt `
(2ch, 48000 Hz, 16-bit, byte rate 192000) and `data` = 36,562,128 B (190.4 s);
`bytes=10-49` emits the header prefix then the first PCM bytes; a mid-track
range (18,281,084) and a suffix range both answer exact `Content-Range`s with
non-silent samples; `cargo test --release` 55/55. Regressions checked: a
`video/mp4` movie still streams raw `ftyp`, a native `.flac` track still serves
`audio/x-flac`, an AAC `.m4a` still falls through to the raw download.

Costs and limits, measured: every range request re-pulls the whole `.m4a`
(~10.5 s for the 43.8 MB track, ~3-4 MB/s) because there is no cache file by
design; music is normally a few MB, so the first byte lands in ~1-3 s. A 4 GB
WAV is impossible (RIFF is 32-bit), which no music file approaches.

Dependency cost: `symphonia` with the ALAC/ISOMP4/PCM/WAV features takes the single
exe from 28,499,815 B to **30,062,592 B** (+1.56 MB; the crate pulls flac/vorbis/
mkv/ogg too, because its default features stay on).

Tools: `tools/probe_alac.mjs "<url>"` (HEAD + header/inside-header/mid/suffix
ranges, asserts every field) and `tools/find_alac.mjs` (scans the Eclipse
catalogue for a track whose stsd really says `alac` — most `m4a` there are AAC).

### CORS: the preflight must echo the requested headers

The Eclipse Music web client (`https://eclipsemusic.app` -> this server) sends its
own `X-Client-Platform` / `X-Client-Version` headers, and the fixed
`Access-Control-Allow-Headers` list (`Content-Type, Authorization, X-API-Secret,
X-Auth-Token`) made Chrome refuse **every** request to the addon: the console showed
`header field x-client-platform is not allowed by Access-Control-Allow-Headers in
preflight response` for `/eclipse/manifest.json`, so no manifest, no search, no
stream and therefore no ALAC playback. The `OPTIONS` branch in `routes.rs` now
echoes `Access-Control-Request-Headers` (safe, because `Allow-Origin: *` carries no
credentials), adds `HEAD` to the allowed methods and exposes
`Accept-Ranges, Content-Range, Content-Length, Content-Type`; the same list is used
by `respond.rs` for ordinary responses. Verified: `OPTIONS
/<token>/eclipse/manifest.json` with `x-client-platform,x-client-version` answers
`204` echoing both, and `<token>/eclipse/manifest.json`, `/eclipse/manifest.json`
(no token) and `<token>/eclipse/search?q=alac` all answer `200`.

## Upstream addon fan-out (`upstream.rs`)

Ported from `backend.php:2900-3040` + the `up_*` branches of the manifest /
catalog / meta / stream handlers. Vocabulary used below is the PHP's.

| Piece                                                     | PHP                                                                                                                   | Rust                                                                                                                                                 |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fd_upstream_manifest_json`                               | memory → 2 h disk cache → 6 s fetch, cached whenever the body is a valid descriptor (NOT only when it lists catalogs) | `upstream::manifest_json` (memory cache stores SUCCESSES only — a failed fetch must be retried, whereas the PHP static simply dies with the request) |
| `fd_manifest_has_resource`                                | string **and** object `resources[]` forms                                                                             | `upstream::has_resource`                                                                                                                             |
| `fd_upstream_manifests_with_resource($r, $respectMaster)` | master-switch aware; an UNFETCHABLE manifest is KEPT                                                                  | `upstream::with_resource`                                                                                                                            |
| `fd_get_upstream_resolver_manifests`                      | meta-capable, **ignores** the master switch, AIOMetadata default as fallback                                          | `upstream::resolver_manifests`                                                                                                                       |
| manifest bridge                                           | `up_<index>_<realCatalogId>`, trimmed `extra`, `types` extended                                                       | `upstream::bridged_catalogs` + `manifest()`                                                                                                          |
| catalog forward                                           | `{upstream}/catalog/…` verbatim, `up_cat_<md5>.json` 1 h                                                              | `upstream::forward_catalog`                                                                                                                          |
| meta forward                                              | first upstream with a non-empty `meta.name`, `up_meta_<md5>.json` 1 h                                                 | `upstream::forward_meta`                                                                                                                             |
| stream merge                                              | ours first, upstream appended, 0.8 s / 2.5 s bound                                                                    | `spawn_upstream_streams` + `await_upstream` in `routes.rs`                                                                                           |

Measured (Windows, seeded defaults): manifest \*\*7,287 B / 44 catalogs (4 local

- 40 bridged)**, `up_0_tmdb.top` **20 metas**, `up_0_search.movie/search=matrix`
  **14**, `/meta/movie/tt38672588` → `meta.name = "Masterplan"` with 39 keys
  (poster/background/genres/videos all forwarded). With Torrentio added as a
  third upstream, `/stream/movie/tt0133093` went **101 → 212** (sponsor, then 100
  local, then 111 upstream) while `pm:post:9000031194` stayed at **5\*\_ (a `pm:`
  id never fans out). `upstream_enabled: false` put it back to 4 catalogs /
  1,038 B, `{"metas":[]}` for a stale `up\__` id, and 101 streams for the same
  title.

Traps that are easy to reintroduce:

1. **The `up_<index>` number is the position in the STORED list**, not in the
   filtered list — PHP skips URL-less entries while keeping their original
   index, and clients then ask for that exact id.
2. **The catalog `extra` path segment is forwarded RAW.** The local parser
   percent-decodes it; re-encoding a decoded `genre=Science+Fiction` corrupts
   the forwarded URL, so `raw_addon_extra()` keeps the original bytes.
3. **Never cache a failed manifest fetch in memory.** The PHP's cache is a
   per-request `static`; a permanent in-process entry would disable bridging
   until restart.
4. **The stream fan-out must not block the local list.** It is spawned BEFORE
   the local resolution and awaited at the end with 0.8 s (local items exist) /
   2.5 s (list still empty), which is the PHP's exact bound.
5. **An EMPTY stored `upstream_manifests` must not wipe the seeded defaults**
   (`load_catalog_settings_merged`): the PHP only overrides when the file lists
   at least one valid URL, otherwise every settings save would remove
   AIOMetadata again.
6. **`catalogs_enabled: false` empties the bridged rows too.** The PHP builds one
   `$manifestCatalogs` list (local + upstream) and keeps it only when
   `catalogs_enabled` is truthy, so `bridged_catalogs()` returns nothing in that
   state. Gating only the local rows left **40** upstream catalogs in a "no
   catalogs" descriptor (measured: 40 catalogs / 0 local).
7. **The descriptor budget is enforced by dropping TRAILING bridged catalogs.**
   Stremio rejects > ~49 KB with `code 20004` ("Max descriptor size reached");
   the budget here is 45,000 B and the local `pm_*` catalogs are never dropped.
8. `aiometadata.strem.io` does **not** resolve, so the seeded default is the
   PHP's real `FD_DEFAULT_UPSTREAM_MANIFEST_URL`
   (`aiometadata.elfhosted.com/stremio/<config>/manifest.json`), which answers
   200 with 40 catalogs. The id is account-specific, exactly like the PHP's.

## Trending catalogs (`pm_trending_movies` / `pm_trending_series`)

Ported from `fd_is_topkeyword_valid()` (backend.php:11655),
`fd_get_trending_keywords_map()` (backend.php:11793) and the trending file rows
of `handle_catalog` (backend.php:15750-15834). The PHP turns each keyword into
its own **`other`-type file** catalog (`pm_topkw_<md5[0..10]>`, capped to the
top **5** keywords); this port instead advertises **two post rows**, one per
type, and merges every valid keyword into them. That difference is deliberate:
`pm_topkw_*` rows are direct Telegram-file items (no poster, no meta), while a
`pm:post:` row opens a real detail page with its own stream list.

| Piece         | PHP                                                                | Rust                                                                                                    |
| ------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------- |
| keyword fetch | `fd_fetch_stream_ajax('trending', ['limit' => 15])`                | `fetch_stream_ajax("trending", [limit=15])` (same 1 h `fd_stream_ajax_ttl`)                             |
| `bot_id`      | explicitly unset for `trending`                                    | `stream_ajax_wants_bot()` returns `false` for it (pinned by a test)                                     |
| junk filter   | `fd_is_topkeyword_valid()` — len ≥ 3, 60 banned filler/adult words | `is_topkeyword_valid()` with the same list, verbatim                                                    |
| map cache     | `storage/cache/trending_keywords.json`, 600 s + single-flight lock | in-process memo, 600 s (the AJAX response itself is the shared disk cache, so no second file is needed) |
| keyword cap   | top **5** (each becomes its own catalog row)                       | whole list; the row stops as soon as it is full                                                         |
| row build     | per-keyword `search_files` (`pm_file_*` items)                     | per-keyword `search` (30 posts) → type split → dedupe by post id → cap **40**                           |
| concurrency   | one call per keyword                                               | `join_all` in batches of 5 (a cold row costs one round trip per batch, never one per keyword)           |
| paging        | `skip` maps to the file search offset                              | pool of `40 + skip` (skip capped at 400), then `.skip(skip).take(40)`                                   |

Rules that are easy to reintroduce:

1. **The trending fetch must stay `bot_id`-free.** The keyword table is global;
   a bot-keyed cache entry would multiply origin calls for an identical answer.
2. **The keyword list is NOT capped at 5.** Five keywords routinely contain only
   one of the two types (the live list is mostly drama/series titles), which
   would leave the movie row empty. The merged rows exist precisely so the tail
   is not wasted.
3. **A post is classified with `post_is_series()`**, the same function the
   normal catalog route uses — not by the `type` segment of the requested id —
   so `pm_trending_movies` can never show a series.
4. **Dedupe is by post id across keywords**; the same title trends under
   multiple keywords and would otherwise repeat in the row.
5. **The two rows add ~230 B to the descriptor** (`7,287 → 7,514 B` measured
   with the seeded upstream), so they are advertised unconditionally — no network
   call is made while building the manifest.
6. **The `skip` extra is the only one advertised**, so a client never sends
   `search`/`genre` here; if one does, the route falls through to the ordinary
   catalog path instead of ignoring it.

## Cloudflare tunnel (`tunnel.rs`)

Port of `backend.php:11199-13545` (39 `fd_tunnel_*` functions plus
`fd_enable_tunnel` / `fd_disable_tunnel` / `fd_get_tunnel_status`). The on-disk
contract is unchanged, so both runtimes can drive the same tunnel:

```
storage/tunnel/token.txt          saved Named-Tunnel token (survives disable)
storage/tunnel/state.json         {enabled,pid,tunnel_url,public_url,local_port,metrics_port,started_at,bin,custom_domain(s),tunnel_token}
storage/tunnel/cloudflared.pid
storage/tunnel/cloudflared.log    cloudflared's own --logfile
storage/tunnel/cloudflared.err.log  the process's stdout/stderr
storage/tunnel/config.yml         the ingress + keep-alive block
storage/tunnel/.last_autorestart  watchdog stamp (300 s)
storage/tunnel/bin/               where a downloaded binary lands
```

| Piece             | Behaviour                                                                                                                                                                            |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| binary resolution | `PM_CLOUDFLARED_BIN` → `<appRoot>/bin` → `storage/bin` → `storage/tunnel/bin` → `C:\Program Files (x86)\cloudflared` → `PATH` → GitHub download (+ `tar -xzf` for the Darwin `.tgz`) |
| Quick Tunnel      | `tunnel --url <local> --config … --metrics 127.0.0.1:<ephemeral>`; the URL comes from `/quicktunnel`, then `/metrics`, then the log                                                  |
| Named Tunnel      | `tunnel … run --token <jwt>`; the public host comes from the ingress `config="…"` line cloudflared logs                                                                              |
| token input       | a raw JWT, or a whole `cloudflared.exe service install eyJh…` command line (`extract_token`)                                                                                         |
| metrics port      | `pick_metrics_port()` binds `127.0.0.1:0` and reuses what the OS just handed out (fallback `20241`)                                                                                  |
| alive             | metrics `/ready` contains `"status":200` (cannot be a reused PID) → else `tasklist` (Windows) / `/proc` (Unix)                                                                       |
| stop              | `taskkill /PID N /T /F` on Windows, `TERM` then `KILL` on Unix                                                                                                                       |
| watchdog          | `get_tunnel_status` → `enabled` but not alive → touch the stamp, `tokio::spawn(auto_restart)` (the PHP ran it inline)                                                                |

Verified against the **real** `cloudflared.exe` (`C:\Program Files (x86)\cloudflared`):

| Step                                                  | Result                                                                                                                                              |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /api/tunnel/enable {}`                          | 200 in **9.2 s** → `tunnel_url` `https://questionnaire-ambien-hopefully-lane.trycloudflare.com`, pid tracked                                        |
| `GET https://<quick-tunnel>/manifest.json`            | **200, 7,636 B**, same 46 catalogs as the local one (the extra 122 B are `logo`/`background`/`description` carrying the public base URL)            |
| `GET /api/tunnel/status`                              | `enabled=true running=true pid=<spawned> metrics_port=63625`, URL re-read from the live metrics endpoint                                            |
| `taskkill /F /PID <cloudflared>` then one status poll | watchdog spawned a NEW process: `pid=2620`, NEW URL `https://thumbnail-sound-adware-gibson.trycloudflare.com`, which served `manifest.json` **200** |
| `POST /api/tunnel/disable {}`                         | 200 in **0.29 s**, `tasklist` shows no `cloudflared.exe`, the old public URL now answers **502**                                                    |

`tools/probe_tunnel.mjs` drives the same lifecycle against a **stub**
`cloudflared` (injected via `PM_CLOUDFLARED_BIN`), so the parsers and the
lifecycle are testable on a machine without the binary at all: **43/43 checks**,
covering the Quick-Tunnel URL, the state/pid/config/log files, `disable` really
killing the child, the Named-Tunnel `custom_domain`, the `http_status:404` host
being skipped, the token being persisted and then **reused** by a later
`enable {}`.

### Traps (each cost a round-trip)

1. **The child is spawned directly, NOT through `cmd /c start "" /b`.** The PHP
   needs `start "" /b` because every FrankenPHP request is its own process and
   `Start-Process` keeps the child in the parent's job object. This binary is one
   long-lived process with no job object, so `std::process::Command` is already
   enough — and it is the only way to get the **real PID**, which `pid_alive`,
   `kill_pid` and `state.json` all need (`start "" /b` returns the PID of an
   intermediate `cmd.exe`). On Windows the child gets
   `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP`, so Ctrl-C in the server's
   console cannot reach it; on Unix it is wrapped in `setsid` when available.
2. **`cloudflared` writes JSON per line, not the PHP's guessed shape.**
   A real Quick Tunnel logs
   `{"level":"info","time":"…","message":"|  https://x.trycloudflare.com  |"}`
   and the same banner also contains `https://api.trycloudflare.com`, which is
   **never** a tunnel — `normalize_url` rejects the `api` host and
   `parse_url_from_text` takes the LAST non-`api` match.
3. **A Named Tunnel's hostname only exists in the ingress config**, and modern
   cloudflared logs it as logfmt (`… config="{\"ingress\":[…]}")`) rather than a
   pure JSON line. `config_json_from_line` accepts both; a host whose service is
   `http_status:404` is skipped, and a host whose service contains our local port
   wins over the rest (the PHP's fallback is "all hosts").
4. **The metrics endpoint is the only trustworthy liveness signal.** A PID can
   be recycled; `pid_alive` therefore tries `/ready` first and only falls back to
   the process table. The PHP additionally required the image name to contain
   `cloudflared`, which this port deliberately does not (a wrapper/stub process
   would never be recognised, and `kill_leftovers` filters by image name itself
   before killing anything).
5. **`kill_leftovers` must never be a blanket `taskkill /IM cloudflared.exe`.**
   It kills the recorded PID plus any `cloudflared.exe` still LISTENING on one of
   our metrics ports (the state's, or `20241`) — the same `netstat`-based
   discovery the PHP uses for `fd_tunnel_metrics_ports()`.
6. **The watchdog runs on a background task, and the stamp is touched first.**
   The PHP ran the restart inline and needed the 5-minute stamp to survive a 2 s
   status poll; here the poll returns immediately and the restart is a
   `tokio::spawn`, but the stamp is still written before spawning so a poll storm
   cannot queue more than one restart per 5 minutes.
7. **`disable` keeps `token.txt`.** A Named Tunnel comes straight back on the
   next `enable {}` with no token typed in (pinned by the probe). `state.json` and
   the pid file are removed so a stale URL is never reported.
8. **Relay registration is fire-and-forget and switchable.** The PHP spawns a CLI
   worker because two HTTPS calls measured 8-9 s and no response depends on them;
   here it is a `tokio::spawn`. `PM_TUNNEL_NO_REGISTER=1` disables it, which is
   how the stub probe avoids publishing a dead `*.trycloudflare.com` name.

## Verified episode names (`fd_upstream_episode_names`)

`build_streams_from_files()` used to pass `&[]` for `episode_names`, so step 5d of
[`series_file_matches_title()`](src/matcher.rs) could never fire: a file that
names ANOTHER show's episode after this episode's marker was still offered. The
whole PHP chain is now ported into [`upstream.rs`](src/upstream.rs):

| PHP (backend.php)                            | Rust                                                     |
| -------------------------------------------- | -------------------------------------------------------- |
| `fd_normalize_episode_name()` (:8569)        | `matcher::normalize_episode_name()` (was already ported) |
| `fd_episode_meta_identity_matches()` (:8636) | `upstream::episode_meta_identity_matches()`              |
| `fd_extract_episode_names()` (:8668)         | `upstream::extract_episode_names()`                      |
| `fd_upstream_episode_names()` (:8747)        | `upstream::episode_names()`                              |
| `preg_replace('/:\d+:\d+$/', '', $mediaId)`  | `upstream::strip_episode_suffix()`                       |

Behaviour, in the PHP's order:

1. **The media id** is the catalog id minus a trailing `:S:E`, and it must match
   `^(?:tt\d{6,10}|[a-z][a-z0-9_-]*:\d+)$` — so `pm:post:…` and `pm_file_…` are
   never sent to a provider (no evidence, guard inert).
2. **The providers** are the first three of `upstream::resolver_manifests()`,
   which deliberately **ignores the master switch** (an external id must keep
   resolving with bridging off). Each is asked for `/meta/series/<id>.json` with
   a **2 s** budget, concurrently, and **without** the WordPress API secret
   (`httpc::get_contents` sends no credentials — a third-party addon must never
   see it; the PHP uses a separate secret-free helper for the same reason).
3. **Identity is verified before any name is trusted**: the record's `id` must
   equal the media id (case-insensitively), `type` must be `series`, a KNOWN
   `name` must normalise to the same words as the catalog title, and a KNOWN
   `releaseInfo`/`year` differing by more than ±1 invalidates the record.
4. **Only `videos[]` entries with matching EXPLICIT `season`/`episode` count**;
   an id suffix is accepted only when the entry carries no explicit numbering,
   so contradictory numbering is never overridden. `name` AND `title` are both
   read (providers alias them), placeholders (`Episode 1`, `S01`) are skipped,
   and the FIRST verified provider that has names wins — never a union of
   potentially contradictory providers.
5. **Cache** `storage/cache/episode_names_<md5>.json` with the TTL **inside** the
   file and freshness from the mtime: **7 d** for names, **1 h** for "verified
   but no names for this episode", **5 min** for a transport failure. The md5 key
   covers the media id, S/E, the normalised expected title, the expected year and
   the provider URLs, so changing the upstream list cannot serve a stale name.
6. **Fail open, always.** An empty list is "no evidence": `episode_subtitle_matches()`
   then returns `true`, a ONE-WORD filename tail stays inconclusive, and an
   unknown structure, a release annotation or an unparsable name all pass. Only a
   multi-word tail that disagrees with a VERIFIED name can reject.

**Measured** (live, 2026-10-19):

| item                                                | names                                              | streams                                             |
| --------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------- |
| `tt11540908:1:1` (Carrie)                           | verified, `["first period"]` from `up_0`           | **15** (unchanged; the 2 `FIRST.PERIOD` files pass) |
| `pm:post:9000016669:1:1` (Kudrat)                   | identity REJECTED (`tt35052365` ≠ "Kudrat 1968 …") | **7** (unchanged — fail open)                       |
| `tt0388629:1:1` (One Piece anime)                   | verified, 1 name                                   | **101** (unchanged; every file's tail is empty)     |
| `tvmaze:82:1:1` / `tt0944947:1:1` (Game of Thrones) | verified, `["winter is coming"]`                   | **38** (was 46 before the port fed names)           |

The Game of Thrones drop is **the PHP's own behaviour**, not a port regression:
`…S01E01.Steelbook.Edition.1080.part001.mkv` yields the tail
`steelbook edition 1080 part001` and `…S01.E01.05.COMBiNED…` yields
`05 combined`, neither is in the stop-list/annotation list the PHP uses, so PHP
rejects them too. Nothing else in the pinned set moved (5 / 101 / 5 / 7 / 15 /
49 / 50 / 2 / 2).

Demonstrated end to end by [`tools/probe_episode_names.mjs`](tools/probe_episode_names.mjs)
(10/10): with the real upstream the list is 15 and the `FIRST.PERIOD` files
survive; with a stub upstream that names the same episode something else the list
shrinks to 13 and **every** name-matching file is rejected while the untail-ed
files still play; after a byte-identical settings restore it is 15 again.

## Extra external id prefixes (`tvdb:` / `anilist:` / `tvmaze:`)

[`parse_external_id()`](src/routes.rs) only knew `tt`, `tmdb:`, `kitsu:` and
`mal:`, so a bridged catalog row carrying one of the other prefixes opened
through the meta bridge and then answered the **empty card**: the route's
`handle_external_stream()` returned `None` and the fallback only asks upstreams
that declare `stream`. The prefix list now matches every prefix the PHP resolves
through `fd_resolve_external_media_metadata`, with the same `:S:E` suffix
handling. No other change was needed — `wp_data::lookup_id()` is
prefix-agnostic (the WordPress `/lookup-id` handler reads the generic
`media_ids_idx` row) and the title search is the same one `tt…` uses.

**Verified live** (the index carries `tvdb` rows; `anilist`/`tvmaze` rows were
seeded through the canonical `POST {WP_API_BASE}/media-ids` writer with the
correct ids and titles, since the map is prefix-generic by design):

| id               | row                                           | streams |
| ---------------- | --------------------------------------------- | ------- |
| `tvdb:81797:1:1` | One Piece 1999 • TvSeries (`tt0388629`)       | **101** |
| `anilist:21:1:1` | One Piece 1999 • TvSeries (`tt0388629`)       | **101** |
| `tvmaze:82:1:1`  | Game of Thrones 2011 • TvSeries (`tt0944947`) | **38**  |

`tvdb:` and `tvmaze:` ids are usually NOT served by a meta provider (measured:
AIOMetadata answers `anilist:21` but 404s `tvdb:81797`), so the episode-name
lookup prefers the row's `imdb_id` when present — the PHP's
`$imdbId !== '' ? $imdbId : $itemId` — and the guard is then active for those
prefixes too. Pinned by `tools/e2e_matrix.mjs`.

## Streaming throughput and seek alignment (measured 2026-10-19)

Two defects made a 1.8 GB 1080p file (`The.Love.Hypothesis...mp4`, `short_code=1jFV4U`) stall with `readyState 2 / networkState 2` and break fast-forward:

1. **Serial chunk fetches.** The body producer awaited one 1 MB `fetch_range_gated` at a time, so every megabyte paid a full Telegram round-trip: measured **870 KiB/s**. It now builds an MB-stepped plan and drains it with `StreamExt::buffered(4)` (in order, 4 windows in flight, still bounded by the per-connection `--inflight` gate): **4.7 MB/s** open-ended (92 MB in 20 s), **2.8 MB/s** on a bounded 4 MB range, first byte 1.2-1.4 s.
2. **A seek hung.** The fetch base was aligned to 4096 only, and `plan_limit(pos, MB)` picks the largest `4096<<k` dividing `pos` without crossing a 1 MB boundary — for a 4096-aligned `pos` that can collapse to 4 KB, so `fetch_range_once()` had to skip `pos / 4 KB` chunks. A seek to byte 900,000,000 skipped ~220,000 parts and returned **0 bytes in 15 s**. The base is now aligned to **1 MB**, so every window is a legal full-size part and a seek costs at most one discarded chunk: byte 900,000,000 -> **3.7 MB, first byte 1.43 s**; byte 1,200,000,000 -> 3.6 MB, first byte 1.29 s.

Do not re-introduce 4096-only alignment, and do not serialise the producer again.

## Cross-platform runtime: Linux + Android/Termux (measured 2026-10-19)

The port now builds and runs outside Windows. **No behaviour changed** — same
routes, same JSON, same matcher — so the parity rows above stay as they are. The
differences are confined to three places:

| Area                   | Windows                           | Linux                       | Android (bionic)                                                                        |
| ---------------------- | --------------------------------- | --------------------------- | --------------------------------------------------------------------------------------- |
| Default bind           | `127.0.0.1`                       | `127.0.0.1`                 | **`0.0.0.0`** (a phone/TV server must be reachable; the device run bound 0.0.0.0:8088)  |
| `shutdown_signal()`    | Ctrl-C only                       | Ctrl-C **or** SIGTERM       | Ctrl-C or SIGTERM (`cfg(unix)`, so `kill` / container stop is graceful)                 |
| Tunnel process helpers | `netstat`, `tasklist`, `taskkill` | `ss`, `/proc/<pid>`, `kill` | same as Linux; `ss` may be absent, so it degrades to the preferred/`20241` metrics port |

`foundation` / `network` / `diagnostics` are affected only in the way their gaps
were described: `fd_is_android_runtime` stays a **gap** on purpose —
[`default_bind_host()`](src/main.rs) uses `cfg!(target_os = "android")` instead —
and `fd_device_info()` fills `model`/`os_version` from `build.prop`
(`ro.product.model` / `ro.build.version.release` / `ro.build.version.sdk`)
because Android's `/proc/sys/kernel/hostname` is unreadable to an app.

`asset_name_for()` is the one tunnel function that had to learn a new platform:
there is no `cloudflared-android` asset, so Android takes the **static Linux Go**
build (`linux-arm64` / `-arm` / `-amd64` / `-386`). Metrics-port discovery on
Android is best-effort by design (no `ss`/`netstat` in a bare image).

**Verified**: the same commit served `/manifest.json` **7,519 B / 46 catalogs** and
`/` **65,182 B** from a Linux binary on Ubuntu 24.04 and from an
`i686-linux-android` binary on Android 7.1.2 (API 25) — matching the Windows
baseline (**7,516 B** / 65,182 B). Full per-target detail, including what is
still **untested** (arm64 Linux, arm64/armv7/x86_64 Android, the Termux
on-device build, Docker), is in [`CROSS-PLATFORM.md`](CROSS-PLATFORM.md).

### `storage/` root is decided by `PM_APP_ROOT`, and this WAS a real defect

`storage_dir()` kept the PHP heuristic even when `PM_APP_ROOT` was set: prefer a
candidate that already holds a session, never adopt a directory under the system
temp dir. Both rules could redirect an explicitly rooted server, so it wrote
**beside its own binary** instead of into the directory it was told to use —
measured on Windows (`PM_APP_ROOT=<temp>` → `Storage: <dist>/windows-x86_64/storage`)
and on Android (`PM_APP_ROOT=/data/local/tmp/pm/state` →
`Storage: /data/local/tmp/pm/storage`). That silently breaks the whole point of
the container volume, the PaaS mount and an Android data dir.

Now [`choose_storage()`](src/storage.rs) branches: an explicit `PM_APP_ROOT` is
authoritative (`<root>/storage`, created if missing, no search, no temp-dir
skip); without it the PHP behaviour is unchanged. Pinned by two unit tests, and
the banner prints the resolved directory. Verified after the fix on Android:
`Storage: …/state/storage` and the whole server (manifest 46 catalogs, `/`
65,182 B in 4 ms, `/api/session` → `runtime: rust-grammers`) served from it.

**Packaging trap found the same session:** `scripts/build-rust-android.sh` named
its tarballs `pencarimovie-rust-<abi>.tar.gz`, and `x86_64` is both an ABI and
the desktop Linux artifact, so an Android run **overwrote the Linux tarball**
(10,915,618 → 10,891,696 B) and left its `.sha256` describing the wrong bytes.
Android names now carry `android-` (`pencarimovie-rust-android-x86_64.tar.gz`).
Always compare the desktop tarball against its sidecar after a packaging run.

### Stream throughput: the pipeline depth was 4, and 4 is too shallow

The body producer keeps `PIPELINE` 1 MB windows in flight
(`StreamExt::buffered`). It was 4; measured on a phone (Android 16, arm64,
**loopback** so the Wi-Fi is out of the picture) an 8 MB range ran at
**348-704 KiB/s**, and the Wi-Fi leg cost nothing on top of it (laptop → phone
was 304-342 KiB/s, while the same phone pulled a static asset at 3.4 MB/s and a
10 MB Cloudflare file at **4.95 MB/s** — i.e. the phone's own uplink was never
the limit).

`PIPELINE` is now **8** (the depth the PHP downloader measured at 7.5-9.7 MiB/s)
and overridable with `PM_PIPELINE=<1..32>`. Same test after the change:

|                        | before (4)    | after (8)       |
| ---------------------- | ------------- | --------------- |
| 8 MB range, cold       | 348-704 KiB/s | **1,071 KiB/s** |
| 8 MB range, warm       | 350-621 KiB/s | **1,419 KiB/s** |
| 4 MB range, cold (206) | —             | 785 KiB/s       |

### A stale `file_reference` at the ORIGIN cannot be fixed here — but it must not be retried blindly

Playback of one title failed with `410 FILE_REFERENCE_EXPIRED`. The chain,
read from `/api/reports` and `/api/logs` on the device:

1. Telegram answers `FILE_REFERENCE_EXPIRED` for the `file_id_mt` the client holds.
2. The app re-resolves with `force=1&nocache=1` → the plugin answers
   **503 `{"ok":0,"message":"Server busy resolving file, please retry shortly.","retry_after":5}`**,
   and when it answers **200 it returns the SAME `file_id_mt` string**.
3. Four attempts fired ~2 s apart all landed in that same window → `410`.

**A `file_id` is stable per bot — receiving the same string is expected, not a
bug.** It is a Bot API id: the _string_ never changes for a given bot+file, while
the MTProto `file_reference` embedded in it rotates. So re-resolving cannot
refresh an expired reference by itself; only re-deriving it from the message (the
plugin's relay path, currently answering 503/504 "busy") produces a new one.
That is why the failure is a server-side problem and why the app can only retry,
fall back and report it.

Two app-side fixes, both verified in the device log:

- **Fallback chain**: forced resolve → **plain resolve** → **cached resolve** →
  **active bot** (a payload can name a bot whose session a reinstall wiped).
  Log markers: `forced resolve gave no new id - falling back to the plain resolve`,
  `resolve still stale - trying the active bot`.
- **Back off before retrying**: `http_json()` drops the body of any non-2xx, so
  the plugin's `retry_after` is usually lost; the loop now uses it when present
  and otherwise waits `1..4 s` (`resolve busy upstream - backing off …`).

What remains **server-side** (WordPress plugin / Manticore row): the reference
must be re-derived through the relay. Until that happens the title cannot play in
_any_ client — the TDLib app and `backend.php` hit the same wall — and the app
correctly reports it as a `playback_incident` in `#settings → Diagnostics`.

### Abandoned requests: pre-flight budget + activity counters

Two things were added after watching a real player loop over a title:

- **`PREFLIGHT_BUDGET_SECS = 20`** caps the pre-flight (resolve + retries +
  priming chunk). Nothing has been written to the client yet, so an abandoned
  request used to keep resolving through the whole retry chain (~60 s with the
  backoff), and a looping player had several of those running at once.
- **`/api/diagnostics` now carries `activity: {streams, preflights, total}`**
  (`ActivityGuard` in [`download.rs`](src/download.rs)) — the counter the Android
  app polls every 3 s to decide **Idle vs Active**, hold the wake/Wi-Fi locks only
  during transfers and release them 30 s after the last byte. Verified on the
  phone: idle → `Idle · …` / lock free, streaming → `Active · 1 stream · …` / lock
  held, cancelled + 30 s → `Idle · …` / lock released with `activity.total` back
  to 0.

### The clock-skew sign was INVERTED vs PHP (found by the device stress run)

`fd_measure_clock_offset()` in [`backend.php`](../../backend.php:7397) returns
`offset = $localTime - $serverTime`, i.e. **positive means the local clock is
AHEAD**, and `fd_clock_skew_message()` maps `> 0` to "ahead of". The Rust port
computed `offset = server - now()` — the opposite sign — while keeping the same
message mapping, so every sentence it printed was backwards: a device **40 s
behind** was told `Your clock is 42s ahead of Telegram's server time.`

Everything else survived the wrong sign because the only other consumers test
`offset.abs()` (the >30 s provisioning guard and the >30 s / >60 s
`skewed`/`critical` flags in `/api/clock-check`), so the bug was invisible except
in the one string a user acts on. Now `offset = now() - server`, matching PHP,
pinned by `session.rs::clock_skew_message_says_ahead_for_a_fast_clock`, and proven
on-device with [`android/pencarimovie-rust/tools/skew_check.mjs`](../../android/pencarimovie-rust/tools/skew_check.mjs):

| device clock | `/api/clock-check`                   | provision refusal                                           |
| ------------ | ------------------------------------ | ----------------------------------------------------------- |
| **+120 s**   | `offset_seconds=119`, `skewed=true`  | "Your clock is 1m 59s **ahead of** Telegram's server time." |
| **−120 s**   | `offset_seconds=-121`, `skewed=true` | "Your clock is 2m 1s **behind** Telegram's server time."    |
| restored     | `-1`, `skewed=false`                 | provisioning succeeds                                       |

Keep the sign and the wording in step: if the subtraction is ever flipped back,
`ahead`/`behind` swap with it.

## Series episodes: the whole `fd_fetch_episode_stream_files()` ladder

[`routes.rs`](src/routes.rs) `resolve_episode_files()` is now a step-for-step port
of `fd_fetch_episode_stream_files()` (backend.php:9987-10388), and the post route
runs it for **every** series request — a series ROOT included.

| Piece               | PHP                                                                                                   | Rust                                                     |
| ------------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| root is S01E01      | `$targetSeason ?? 1` / `$targetEpisode ?? 1` (backend.php:16828)                                      | `effective_season_episode()` (unit-tested)               |
| step 0              | the post's OWN files, `fd_fetch_post_files_paged(200/600/3)`                                          | `fetch_post_files_paged()` + `add_episode_files()`       |
| year from files     | mode of 4-digit years, ≥20 tagged, ≥60 % (backend.php:10121-10142)                                    | `wp_data::derive_year_from_files()` (4 unit tests)       |
| step 1              | per-episode `stream-files?type=series&season&episode&limit&title&id`, NOT gated on step 0 being empty | `wp_data::stream_files_episode()`                        |
| title/year adoption | `resolved_title` / `resolved_year` when the title is empty                                            | adopted in step 1                                        |
| early stop          | `if ($fastPathDone && $keyword !== '' && !$fetchedFallbackPost) return`                               | `skip_ladder`                                            |
| step 2              | `get_post` when the title is still unknown                                                            | same (cache-first)                                       |
| step 3              | 13 `search_files` probes, rare episode-token conventions first                                        | `episode_probe_queries()` + `join_all`                   |
| episode names       | `fd_upstream_episode_names()` resolved ONCE for the whole function                                    | `upstream::episode_names()` in `resolve_episode_files()` |

Four things are load-bearing and each was measured:

1. **The per-episode call must NOT go through `fetch_stream_ajax()`.** That layer
   unwraps `$decoded['items']` (PHP does the same at backend.php:9561), which
   drops `ok` and `resolved_title`/`resolved_year` — `ok` is the flag that stops
   the fallback ladder, so without it every episode paid the full 13-probe scan
   (measured: 719 ms → 549 ms on `pm:post:9000020617`, and the probe block
   disappears from the log). Both `/stream-files` helpers therefore use
   `httpc::cached_wp_json()`, the port of `fd_cached_wp_json()`.
2. **The per-episode TTL is 15 s** (`PER_EPISODE_SEARCH_TTL`), the PHP's explicit
   `fd_cached_wp_json($url, 15)`, so a freshly uploaded episode appears as fast as
   it does on PHP. The by-title search keeps the generic 300 s.
3. **`year` is not sent on the per-episode URL.** The PHP does not send it either,
   the origin ignores it, and sending it would split the cache key the moment the
   year becomes known from the post's own file set.
4. **`fd_stream_keyword_variants()` is a quirk-for-quirk port.** The PHP's
   alternation `['’`]s|s['’`]|\bs\b`scans left to right, so`"Princess's"`yields`Princes`while`"Grey's"`yields`Grey`. Verified against
`bin/php.exe`and pinned by`matcher::stream_keyword_variants_cover_the_possessive_forms`.

Verified with [`storage/_ep_list.ps1`](../../storage/_ep_list.ps1) and
[`storage/_type_check.ps1`](../../storage/_type_check.ps1) (cold caches):
root **20** media (was 5 — it is episode 1's whole file list now, like PHP),
`:1:1` 20, `:1:2` 14, `:1:12` 12, Kudrat 9, Siccîn 4, Zul Aaryan 3;
`pm:post:8798795472:1:1` **54** with 0 × 2023 / 0 × LEGO / 0 × live-action;
`storage/_id_resolve.ps1` unchanged (101/2/30/5/101/38).
