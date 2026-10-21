# PencariMovie Server — the engine (Rust, native `grammers`)

This is **the product**: one self-contained binary. The crate lives at `<repo>/server`
and embeds the dashboard from `<repo>/public` at compile time (see
[`src/assets.rs`](src/assets.rs)), so a release artifact is a single executable
plus its launchers.

The engine is a faithful translation of the retired PHP backend
([`legacy/php/backend.php`](../legacy/php/backend.php)) onto **native grammers** —
`grammers-client` / `grammers-session` / `grammers-tl-types` **0.11.0** from
crates.io (same source as codeberg master, no wrapper, no FFI) — plus `axum` +
`tokio`. Translation, not redesign: same function names, same defaults, same
`storage/...` paths, same JSON shapes, same error strings.

The work list is [`COVERAGE.md`](COVERAGE.md) (the original `backend.php` line-by-line
inventory, 266 `fd_*` functions, 57 route patterns); the per-group state of this port
is [`PARITY.md`](PARITY.md) and the remaining divergences are in
[`PORT_GAPS.md`](PORT_GAPS.md). Cross-platform build notes are in
[`CROSS-PLATFORM.md`](CROSS-PLATFORM.md).

---

## 1. Build and run

```bash
# toolchain (measured here)
#   rustc 1.99.1 (58cf8f9c3 2026-10-13)   cargo 1.99.1
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

cargo build --release          # Windows: ~51 s warm; 30,980,608 B binary
cargo test  --release          # 109 unit tests (matcher traps, decoder, auth, caches,
                               #  tunnel platform table, build.prop, bind defaults,
                               #  PM_APP_ROOT storage override, clock-skew sign)

./target/release/pencarimovie-rust --port 8088                    # 1 client (default)
./target/release/pencarimovie-rust --port 8088 --workers 4         # 4 clients
./target/release/pencarimovie-rust --version                       # build identity
./target/release/pencarimovie-rust --decode <file_id_mt> [exp_id]   # decoder self-test
```

**Multi-target builds (Linux, Android/Termux, Docker/PaaS):**
[`CROSS-PLATFORM.md`](CROSS-PLATFORM.md) is the per-target table — target
triples, NDK/toolchain, artifact names, what was executed and what is still
untested. Short version: `scripts/build-rust-windows.bat`,
`scripts/build-rust-linux.sh` (verified on a real Ubuntu host),
`scripts/build-rust-android.sh` (NDK r27, API 21, four ABIs — the x86 build was
executed on an Android 7.1 device), `scripts/build-rust-termux.sh` (on-device,
untested), and `Dockerfile` + `docker-compose.yml`.

Prove any of them in one command — cold boot, 16 checks, no PowerShell:

```bash
node server/tools/smoke_boot.mjs server/dist-rust/windows-x86_64/pencarimovie-rust.exe 8095
```

`PM_APP_ROOT` decides where `storage/` lives and, when set, it is authoritative
(no session-preference search, no temp-dir skip) — the contract the Docker
volume, the PaaS mount and an Android data dir rely on.

**It is ONE executable.** `public/` (the dashboard: `index.html`,
`app.bundle.js`, `css/`, `styles.css`, `res/`, `assets/` â€” 113 files, ~16 MB) is
compiled into the binary with `include_dir!` ([`src/assets.rs`](src/assets.rs)),
so there is no `public/` folder to ship, no PHP runtime, no Composer and no
`vendor/`. Copy `pencarimovie-rust.exe` anywhere and run it:

```powershell
# verified on Windows: an EMPTY temp dir, the exe alone
PS> Get-ChildItem $env:TEMP\pm-standalone      # pencarimovie-rust.exe only
PS> .\pencarimovie-rust.exe --port 8093
PS> curl -sI http://127.0.0.1:8093/
HTTP/1.1 200 OK
content-type: text/html; charset=utf-8        # renders, does not download
content-length: 65182
```

A frontend change therefore needs `cargo build --release` again. To iterate on
the UI without rebuilding, set `PM_ASSETS=disk` and the server reads `public/`
from the checkout instead (same files, no embedding).

`scripts/build-rust-windows.bat` stages the release the same way as
`tools/build-dist.ps1` (which still works): the exe, an empty `storage/`,
`start.bat` and a `README.txt` (only `storage/` is state).

Binds `127.0.0.1` by default (`--host`, `PM_LISTEN_HOST` or `HOST` to change;
Android and a detected PaaS default to `0.0.0.0` — see
[`CROSS-PLATFORM.md`](CROSS-PLATFORM.md#3-bind-address-desktop-vs-android-vs-paas)).
Storage is
`<cwd>/storage` (or `PM_APP_ROOT`), i.e. the **same layout as the PHP**:
`storage/sessions/<bot_id>/`, `storage/cache/resolve_cache_<md5>.json`,
`storage/cache/wp_ajax_<md5>.json`, `storage/auth.json`, `storage/debug.log`, â€¦
Session files are named `session.rust.json` so they can never be mistaken for a
MadelineProto session.

Environment: `PORT`, `PM_LISTEN_HOST`/`HOST`, `PM_APP_ROOT`, `PM_ASSETS=disk`,
`PM_INFLIGHT`, `PM_MAX_STREAMS`, `PM_PAAS`, `FD_WP_API_BASE`,
`FD_API_ID`/`FD_API_HASH`, `BOT_TOKEN`, `SERVER_PASSWORD`, `SERVER_TOKEN`,
`DEBUG`.

### End-to-end proof (Windows, this checkout)

```powershell
powershell -NoProfile -File server\tools\e2e_range.ps1 -Item "pm:post:9000031194"
# streams = 4
# payload = {"file_name":"Siccin.9.2026.1080p.WEBRip.x264.AAC5.1.YTS.mp4",...}
# HTTP/1.1 206 Partial Content
# content-range: bytes 0-262143/1793429858
# bytes = 262144   magic = 00 00 00 20 66 74 79 70   (MP4 `ftyp`)
```

Stream counts measured against the same live origin: `pm:post:9000031194` â†’ 4,
`pm:post:8591885201` (**The Super Mario Galaxy Movie**) â†’ **100** (the type-word
rule), `pm:post:9000016669:1:1` (**Kudrat 1968 S01E01**) â†’ 6 (the embedded-year
rule). Provisioning a guest bot took **6 s**; the 206 above came from that bot.

---

## 2. THE bug that made both ports fail: TL string padding

Every download failed with `FILE_REFERENCE_EXPIRED` in the TypeScript port and
then in this one. It was **not** a WordPress or bot-reference problem â€” it was a
one-byte decoding error, present in both ports.

Inside a Bot API `file_id` the `file_reference` is a **TL string**, and TL pads a
field so that _the whole field_ (length prefix **plus** body) is a multiple of 4:

```
pad = (4 - ((prefix_len + len) % 4)) % 4      # correct
pad = (4 - (len % 4)) % 4                     # what both ports did  â† wrong
```

For a 25-byte reference the correct pad is **2**, the wrong one **3**, so the
reader sat one byte past the string and every following field was read shifted:
the `id` and the `access_hash`. The `file_reference` itself was read correctly â€”
which is precisely why Telegram answered `FILE_REFERENCE_EXPIRED` (the reference
exists, the id it belongs to does not) instead of reporting a malformed request.

Measured on a live id (`â€¦ASkV1mrVoxtmjeajGGVQGazxMmuiPS5kâ€¦`):

| field            | wrong pad (before)                               | correct pad (now)           |
| ---------------- | ------------------------------------------------ | --------------------------- |
| `id`             | `-2929793036349079547` (negative â€” impossible) | **`6289489716727252339`**   |
| `access_hash`    | `4426478763465126403`                            | `7927174950789710807`       |
| `file_reference` | `03012915d66aâ€¦` (25 B)                         | _identical_ â€” never wrong |
| `dc_id` / `type` | `5` / `document`                                 | `5` / `document`            |

Fixed in [`src/fileid.rs`](src/fileid.rs) **and** in
[`../ts/src/fileid.ts`](../ts/src/fileid.ts). Regression guard:
`fileid::tests::tl_padding_counts_the_length_prefix` plus
`decodes_live_document_id` (asserts `id > 0`; an access_hash is a random i64 and
may legitimately be negative, so it is not asserted).

Two independent cross-checks are kept in [`tools/`](tools):

```bash
python3 tools/check_padding.py <file_id_mt>   # prints both pad rules side by side
./target/release/pencarimovie-rust --decode <file_id_mt>   # Rust values
cd ../ts && node dist/fileid.js <file_id_mt>               # TS values (now identical)
```

Before/after, on the same `short_code=1v9OPj`, same auto-provisioned bot:

```
before:  HTTP/1.1 410 Gone   {"ok":0,"message":"FILE_REFERENCE_EXPIRED (400)",...}
after:   HTTP/1.1 206 Partial Content
         content-type: video/x-matroska
         content-range: bytes 0-262143/3967229576
         262144 bytes, magic 1a 45 df a3 = EBML (Matroska)
```

and a FLAC track (`BB0p2e`): `206`, 262144 B, `content-range: bytes 0-262143/95547741`,
magic `66 4c 61 43` = `fLaC`.

---

## 3. Telegram layer

- **Login is fresh** â€” `GET {WP_API_BASE}/provision-session` â†’ `{ok, bot_token,
api_secret}`, then `Client::bot_sign_in(&token, &api_hash)`, which sends
  `auth.importBotAuthorization` (the exact TL constructor). `api_id`/`api_hash`
  come from `storage/api_credentials.json` (decrypted from WordPress with
  AES-256-CBC, key `sha256(bot_token)`). Sessions are **never** copied from the
  PHP install.
- **Session persistence** â€” `JsonSession` implements `grammers_session::Session`
  over a small JSON file (home DC, DC options, auth keys, cached peers, update
  state), written atomically after every mutation.
- **Streaming primitive** â€” `Client::iter_download(&target)` +
  `chunk_size(4096 * 2^k)` + `skip_chunks(pos / limit)`, i.e. the per-connection
  `upload.getFile` path (`DownloadIter::next()`), not a fake pool.
- **Alignment** â€” `plan_limit(pos, want)` picks the largest `4096 * 2^k`
  (â‰¤ 512 KB, and â‰¤ the crate's `MAX_CHUNK_SIZE`) that **divides `pos`** and does
  not cross the next 1 MB boundary, so every call satisfies Telegram's rules
  (offset/limit divisible by 4096, `1048576 % limit == 0`, a part never crosses
  1 MB, `limit â‰¤ 1 MB`). A Range start is rounded **down** to 4096 and the
  leading bytes are discarded (chosen for byte-accuracy over `skip_chunks`
  granularity, which can only step in whole chunks **at that offset's** limit).
- **Backpressure** â€” the response body is a bounded `mpsc` channel (capacity 4)
  fed by one producer task; when the client disconnects the channel closes, the
  producer's `send()` fails and it stops pulling from Telegram. Headers are
  committed only **after** the first chunk arrives, so a failed first fetch can
  still answer JSON instead of leaving the client waiting on a `Content-Length`
  that never completes.
- **Retry ladder (PHP parity)** â€” `max_attempts = short_code ? 4 : 1`; on a
  reference error the short_code is re-resolved with `force=1&nocache=1`, else
  the pool is walked for an alternate bot; a non-reference failure re-boots the
  session; `410` + the exact message when everything fails. The id that worked is
  persisted with `verified_at` so the next Range starts from a live reference.
- **`--workers N` = N `grammers` clients** (N `SenderPool`s â†’ N connection sets),
  round-robin per request. grammers multiplexes all requests for a DC over that
  DC's single connection, so this is the only way to get more than one connection
  per DC without separate processes.

### Measured: `1 client Ã— 4 tasks` vs `4 clients Ã— 1 task`

1 MB Range windows, `short_code=1v9OPj` (4K MKV on DC 5), 8-core host in Europe,
`curl` concurrency N. **Real bytes**, not error responses:

| workers       | N=1                     | N=10                        | N=50                                   | RSS before â†’ after | FDs       | CPU ticks |
| ------------- | ----------------------- | --------------------------- | -------------------------------------- | -------------------- | --------- | --------- |
| `--workers 1` | **10.2 MB/s** (0.081 s) | **35.5 MB/s** (avg 0.214 s) | **20.5 MB/s** (avg 1.73 s, max 2.35 s) | 8,988 â†’ 27,720 KB  | 11 â†’ 11 | 47        |
| `--workers 4` | 8.6 MB/s (0.103 s)      | 37.6 MB/s (avg 0.192 s)     | 18.8 MB/s (avg 2.22 s, max 2.61 s)     | 8,344 â†’ 53,972 KB  | 11 â†’ 14 | 54        |

Reading: at N=10 four clients are marginally faster (37.6 vs 35.5 MB/s, within
run-to-run noise) and at N=50 slightly worse; peak RSS roughly doubles. The
bottleneck is the Telegram DC and the TLS/handshake cost, not local CPU â€” so the
default is **one client**, and more workers are for isolating a single slow
download, not for aggregate throughput.

---

## 4. Verification (VPS `admin@159.195.95.182`)

Everything below was run on the VPS against the release binary on `:8088`, with
the PHP runtime live on `:8088` for comparison.

### 4.1 Decoder side-by-side (Rust vs TS vs `--decode`)

| field                  | Rust                                                 | TypeScript              |
| ---------------------- | ---------------------------------------------------- | ----------------------- |
| `id`                   | `6289489716727252339`                                | `6289489716727252339`   |
| `access_hash`          | `7927174950789710807`                                | `7927174950789710807`   |
| `dc_id`                | `5`                                                  | `5`                     |
| `file_reference` (hex) | `03012915d66ad5a31b668de6a318655019acf1326ba23d2e64` | identical               |
| reference length       | `25`                                                 | `25`                    |
| type / version / sub   | `document` / `4` / `61`                              | `document` / `4` / `61` |

### 4.2 REST smoke (`tools/rest_smoke.sh`, the TS set)

| endpoint                                              | Rust                                                                                        |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `GET /api/version`                                    | 200 `{"ok":true,"current_version":"2.7.9","minimum_version":"2.7.9","update_needed":false}` |
| `GET /api/auth/status`                                | 200 `{"enabled":true,"authenticated":true,"token":"â€¦"}`                                   |
| `GET /api/session`                                    | 200 `has_session:true`, `bot_id`, `bot_pool`, `api_secret` (loopback only)                  |
| `GET /api/bots`                                       | 200                                                                                         |
| `GET /api/lan-ip`                                     | 200 `{"lan_ip":"159.195.95.182","port":8088}`                                               |
| `GET /api/clock-check`                                | 200 `{"ok":1,"offset_seconds":0}`                                                           |
| `GET /api/tunnel/status`                              | 200 (state file read)                                                                       |
| `GET /api/catalog-settings`                           | 200                                                                                         |
| `GET /api/country`                                    | 200 (+ 20-country list)                                                                     |
| `GET /api/diagnostics` + `/settings` + `/api/reports` | 200                                                                                         |
| `GET /api/debug-mode` / `/api/logs`                   | 200 / 200 (403 while debug off)                                                             |
| `GET /api/resolve-shortcode?short_code=1v9OPj`        | **200** in 1.46 s, full payload (`file_id_mt`, `file_size`, `mime`, `title`)                |
| `GET /api/warmup-resolve?short_codes=â€¦`             | 200 `{total:1,resolved:1,failed:0}`                                                         |
| `GET /manifest.json` (+ `/nuvio`, `/stremio` aliases) | 200                                                                                         |
| `GET /api/provision`                                  | **200** (bot `7841750221 @EncantoSolMonbot`, 18.7 s cold)                                   |
| `POST /api/botlogin {}`                               | 400 `bot_token is required.`                                                                |
| `POST /api/bots/add {}`                               | 400 `Provide bot_token, tokens[] or tokens_text.`                                           |
| `POST /api/bots/remove {"bot_id":""}`                 | 400 `bot_id is required.`                                                                   |
| `POST /api/bots/set-active {"bot_id":"0"}`            | 404 `Bot ID not found in pool.`                                                             |
| `POST /api/validate-manifest` (opensubtitles-v3)      | 200 with the real manifest                                                                  |
| `POST /api/validate-manifest {""}`                    | 400 `Please provide a manifest URL.`                                                        |
| `POST /api/auth/login` (wrong pw)                     | 401 `Invalid password. 4 attempt(s) left before lockout.`                                   |
| `POST /api/auth/password` (wrong)                     | 401 `Current password is wrong`                                                             |
| `POST /api/catalog-settings {}`                       | 200 `Catalog settings updated successfully`                                                 |
| `GET /api/download` / `?short_code=`                  | 400 `file_id or valid short_code is requiredâ€¦`                                            |
| `GET /api/does-not-exist`                             | 404 `Unknown API endpoint`                                                                  |
| `GET /stream/movie/pm_file_1v9OPj.json`               | 200 with a playable stream item                                                             |
| `GET /stream/movie/tt99999999.json`                   | 200 `{"streams":[]}` (matcher not ported â€” see PARITY)                                    |
| `GET /eclipse/manifest.json`                          | 200                                                                                         |
| `GET /nuvio/manifest.json`                            | 200                                                                                         |

### 4.3 Range request (the thing that used to 410)

```
$ curl -s -D- -o r.bin -H 'Range: bytes=0-262143' \
    'http://127.0.0.1:8088/api/download?short_code=1v9OPj'
HTTP/1.1 206 Partial Content
content-type: video/x-matroska
content-length: 262144
content-range: bytes 0-262143/3967229576
accept-ranges: bytes
262144 bytes, magic 1a45dfa3 (EBML/Matroska)
```

### 4.4 Throughput / resources

| measurement                    | result                                                                           |
| ------------------------------ | -------------------------------------------------------------------------------- |
| 1 request, 4 MB range          | **206, 4,194,304 B in 0.30 s â†’ 13.9 MB/s** (`curl %{speed_download}=13902464`) |
| 1 request, 256 KB range        | 206, 262,144 B, magic `1a45dfa3` (Matroska)                                      |
| N=10 / N=50 Ã— 1 MB, 1 client  | 35.5 / 20.5 MB/s aggregate, RSS 9 â†’ 28 MB, FDs flat at 11                      |
| N=10 / N=50 Ã— 1 MB, 4 clients | 37.6 / 18.8 MB/s aggregate, RSS 25 â†’ 54 MB                                     |

Error-path-only measurements were never needed: real 206s were available once
the padding bug was fixed.

### 4.5 Re-runnable commands

```bash
# on the build host, from ~/pm-rust
./target/release/pencarimovie-rust --port 8088 --workers 1 &
B=http://127.0.0.1:8088 bash ~/rest_smoke_rust.sh          # tools/rest_smoke.sh
B=http://127.0.0.1:8088 bash ~/try_shortcodes.sh 1v9OPj    # tools/try_shortcodes.sh
PORT=8088 LABEL=workers-1 bash ~/bench_bytes.sh            # tools/bench_bytes.sh
python3 ~/check_padding.py <file_id_mt>                    # tools/check_padding.py
```

`tools/probe_botapi.sh` is the diagnostic that proved the id was **valid for the
bot** (`api.telegram.org/getFile` answered `file is too big`, never
`invalid file_id`) while MTProto rejected it â€” that asymmetry is what pointed at
the decoder rather than at WordPress.

---

## 5. Honest gap list vs PHP

Documented per group in [`PARITY.md`](PARITY.md). The load-bearing ones:

| area                                                                                                                                                                                                                            | state                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/api/*` REST surface (auth, session, bots, provision, resolve, warmup, download, catalog-settings, media-ids, validate-manifest, country, diagnostics, reports, debug-mode, logs, botlogin/botlogout, proxy-stream, sub-proxy) | ported                                                                                                                                                                                                                                        |
| `/api/download` (payload forms, Range, 206/416, retry ladder, `verified_at`, streaming)                                                                                                                                         | ported, verified with real 206s                                                                                                                                                                                                               |
| `/manifest.json`, `/nuvio`, `/stremio`, `pm_file_*` / `pm:post:` / `ttâ€¦` stream lists, the **embedded dashboard**, eclipse stub                                                                                               | ported (dashboard served from the binary; eclipse content still a gap)                                                                                                                                                                        |
| **Title/episode matcher** (`fd_movie_file_matches_title`, `fd_series_file_matches_title`, `fd_classify_season_episode`, `fd_extract_split_part_info`, `fd_group_split_parts`, `apply_stream_filters`, â€¦)                      | **ported** (`matcher.rs`, 30 functions) and verified end to end: `pm:post:9000031194` 4 streams, `pm:post:8591885201` 100, `pm:post:9000016669:1:1` 6, each served as a real **206**. All seven AGENTS "Do not regress" traps are unit tests. |
| Catalog builder / `fd_load_catalog_settings` full shape / trending / country catalogs                                                                                                                                           | not ported (manifest advertises no catalog rows)                                                                                                                                                                                              |
| `media_ids_idx` (Manticore)                                                                                                                                                                                                     | not local; writes are proxied to WordPress `/media-ids`                                                                                                                                                                                       |
| Tunnel control (`cloudflared` spawn/parse)                                                                                                                                                                                      | status is read from `storage/tunnel/state.json`; enable/disable answer an explicit "not implemented"                                                                                                                                          |
| Diagnostics **uploader** (`fd_diag_report_send` / flush)                                                                                                                                                                        | not ported: records locally, `/api/reports?send` answers `ok:0, "Report upload is not ported."`                                                                                                                                               |
| FLAC/ALAC transcode pipeline (`audio-encode-worker`, ffmpeg)                                                                                                                                                                    | not ported                                                                                                                                                                                                                                    |
| IPC worker model                                                                                                                                                                                                                | **N/A by design** (grammers is in-process); `--workers` covers the multi-connection intent                                                                                                                                                    |
| PHP `auth.json` bcrypt hashes                                                                                                                                                                                                   | verified via the `bcrypt` crate; the port writes its own `sha256$salt$hash` (20k rounds)                                                                                                                                                      |
| `patches/**`, `vendor/**`                                                                                                                                                                                                       | untouched (this port has no vendor patch step)                                                                                                                                                                                                |

Not touched, per the task: `backend.php`, `public/`, `android/`, `composer.json`,
`patches/**`. No `git commit`.
