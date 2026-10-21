# Telegram client concurrency evaluation — teleproto (TS) vs grammers (Rust)

Scope: research + test plan only. No app code was changed. Baseline facts for MadelineProto/PHP come from
[`AGENTS.md`](../AGENTS.md); library facts come from the libraries' own source; Telegram-side limits come from
`core.telegram.org`. Anything not measured is marked `unmeasured`.

Survey date: 2026-10-19. teleproto `main` @ `v1.230.1` (2026-10-16); grammers `master` @ `grammers-client 0.11.0`
(2026-10-15).

---

## 1. Verdict

**Benchmark teleproto first, grammers second, and only then decide whether to port at all.** teleproto is the only
candidate that already contains the download architecture Telegram recommends: [`core.telegram.org/api/files`](https://core.telegram.org/api/files)
says large `upload.getFile` traffic "should be handled through one or more separate sessions and separate connections",
and that such sessions are "just an extra `session_id` over the same `auth_key_id`", openable "without any
re-authorization". teleproto's `MediaScheduler` implements exactly that: a per-DC pool of 4 independent `MTProtoSender`
connections with 4 in-flight parts each
([`BalancePolicy.ts:14-18`](https://github.com/sanyok12345/teleproto/blob/main/teleproto/network/BalancePolicy.ts#L14-L18)),
FLOOD_WAIT-aware pacing, DC migration and optional hash verification. **Caveat, and the first thing a PoC must nail:**
its convenience streaming generator `iterDownload()` bypasses that pool and calls `client.invoke()` directly, so it is
sequential on a single connection; the pooled range primitive is `client._media.getFile(...)` (§3).

grammers is the better _runtime_ (no GC, one socket per DC, 4-worker chunk pump) but its `SenderPool` routes every
request for a DC to a **single** connection
([`sender_pool.rs:328-332`](https://codeberg.org/Lonami/grammers/src/branch/master/grammers-mtsender/src/sender_pool.rs)),
so N concurrent downloads inside one process share one TCP connection and one MTProto container window. Both libraries
are equally blocked on the same porting cost — turning the app's Bot API `file_id` into `InputDocumentFileLocation`
plus a `dc_id`, and refreshing `file_reference` on expiry — so that cost does not separate them.

---

## 2. Why the current stack is the thing we measure against

From [`AGENTS.md`](../AGENTS.md) ("Concurrency, thread pool, and the session-save deadlock", "Streaming throughput and
first-byte latency", "The Android IPC root cause"):

- Each concurrent stream occupies one FrankenPHP thread for its whole life; the pool is `num_threads 8` /
  `max_threads 16` (`Caddyfile`), and a saturated pool answers 503 instead of queueing.
- A full MadelineProto boot costs 1-3 s (on shared hosting, every range request). IPC avoids that, but the IPC worker
  is the single largest operational risk: one worker per session, `session is busy` on a second boot, and the
  `entry.php` argv / DoH / SIGSYS failure modes each took a device round-trip to find.
- Memory: ~30-50 MB per MadelineProto instance; the whole-server failure mode is a session-save deadlock on
  `safe.php.lock`.
- Measured Telegram throughput for this workload: **294 KB/s** (Heroku US) and **660-712 KB/s** (Render SG) for 1 MB
  ranges of `short_code=1v9OPj` (3.97 GB). That is the _ceiling the Telegram DC gave us_, not a PHP limit — which is
  exactly why the benchmark below must separate "origin-bound" from "stack-bound".
- 50% of warmup resolves fail (`{"total":40,"resolved":20,"failed":20}`), so half of first plays pay a
  `FILE_REFERENCE_EXPIRED` re-resolve. Both candidate libraries leave that logic to the caller (§3.5, §4.4).

Baseline code facts (read-only): `downloadToBrowser()` is at
[`vendor/danog/madelineproto/src/MTProtoTools/FilesLogic.php:93`](../vendor/danog/madelineproto/src/MTProtoTools/FilesLogic.php:93),
and the reason a Bot API `file_id` works today with no decoding on our side is
[`MTProtoTools/Files.php:991 extractBotAPIFile()`](../vendor/danog/madelineproto/src/MTProtoTools/Files.php:991).

---

## 3. teleproto (TypeScript) — capability table

Repo: <https://github.com/sanyok12345/teleproto>. MIT, fork of GramJS since 2025 (README), `v1.230.1` published
2026-10-16, 9 open issues, pure JS (no native build step).

| Concern                                 | Finding                                                                                                                                                                                                                                                                                                                   | Citation                                                                                                         |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Transport model                         | One `MTProtoSender` per "slot"; slots are keyed by **shifted** dc id, so one DC can hold several independent connections. Slots reconnect on demand and disconnect when idle (`idleTimeoutMs`).                                                                                                                           | `network/Network.ts:30-96` (`_slots` by `ShiftedDcId`, `_makeSlot`, `getSession`), `network/SenderSlot.ts:1-160` |
| Concurrency default                     | Download balance = **4 sessions x 4 in-flight per DC** (16 concurrent `upload.getFile` per DC, per download). Configurable via `downloadPool: { sessions, inflightPerDc, partSize, idleTimeoutMs }`.                                                                                                                      | `network/BalancePolicy.ts:14-18`; `client/telegramBaseClient.ts:238-245`, `533-537`                              |
| Request queue                           | Each sender has its own `_queued: RequestState[]` with dependency chaining (`USE_INVOKE_AFTER_WITH`); replies are matched per sender. The queue is therefore per connection, and the pool is what produces concurrency.                                                                                                   | `network/MTProtoSender.ts:144`, `456-466`, `544-577`                                                             |
| Streaming API 1: async generator        | `iterDownload()` is a real async generator with `offset`, `limit`, `requestSize` (divisible by 4096, default 512 KB), `dcId`, `signal`; it sends `precise: true` and auto-corrects on `FILE_MIGRATE`. **But it calls `client.invoke()` directly, bypassing `MediaScheduler`** — sequential, unpaced, one connection.      | `client/downloads.ts:751-831`                                                                                    |
| Streaming API 2: writer                 | `downloadFile()` writes into a `Buffer` or `WriteStream`, **always from offset 0** (`DownloadFileParams` has no offset), and is the path that uses the pool: `streamParallel` plus `OrderedWriter` plus a `sessions x inFlight` semaphore.                                                                                | `client/downloads.ts:34-42`, `163-253`, `289-374`                                                                |
| Streaming API 3: pooled range primitive | `client._media.getFile(dcId, location, offset, limit, signal, onMigrate)` — per part, pooled, paced, FLOOD- and DC-migration-aware. This is what an HTTP Range handler should drive.                                                                                                                                      | `network/MediaScheduler.ts:195-240`                                                                              |
| Arbitrary offset                        | Accepted, but the request is sent with `precise: false`, so the **caller must align**: offset divisible by 4 KB, limit divisible by 4 KB, 1 MB divisible by limit, and the part must not cross a 1 MB boundary (Telegram's rule, §5). Part size defaults to **512 KB**, is capped at 1 MB and must be a multiple of 4 KB. | `network/MediaScheduler.ts:203-209`, `48-55`, `171-174`; `core.telegram.org/api/files`                           |
| Parallel chunks                         | On by default when the size is known (`streamParallel`), ordered output via `OrderedWriter`, in-flight bounded by a semaphore of `sessions x inFlight`.                                                                                                                                                                   | `client/downloads.ts:289-374`; `network/OrderedWriter.ts`                                                        |
| `file_reference`                        | Taken from the supplied `Document` or `InputDocumentFileLocation`; the library never refreshes it. Retry logic covers flood, transport and server errors only, so a `FILE_REFERENCE_EXPIRED` RPC error is **rethrown**.                                                                                                   | `client/downloads.ts:503-520`; `network/MediaScheduler.ts:281-297`                                               |
| DC handling                             | `FileMigrateError` switches DC and calls `onMigrate`; `streamParallel` probes the first part to learn the real DC; `fileCdnRedirect` throws (CDN disabled by default).                                                                                                                                                    | `network/MediaScheduler.ts:224-236`, `52`; `client/downloads.ts:299-312`                                         |
| Bot mode                                | `auth.ImportBotAuthorization` via `signInBot`, plus a `client._bot` flag. Downloads use the same `upload.getFile` MTProto path; there is no forced Bot-API HTTP route.                                                                                                                                                    | `client/auth.ts:159-163`, `1103-1130`; `client/telegramBaseClient.ts:326`                                        |
| Session persistence                     | `MemorySession`, `StringSession`, `StoreSession` (node-localstorage directory). `save()` is explicit and there is **no inter-process lock** — two processes on one session are last-write-wins.                                                                                                                           | `sessions/Abstract.ts`, `sessions/StoreSession.ts:1-45`                                                          |
| Footprint per connection                | `unmeasured`. Expect a shared V8 heap plus one socket and read buffer per slot; 4x4 slots per DC exist only while a download runs (idle slots disconnect).                                                                                                                                                                | —                                                                                                                |
| Rate-limit handling                     | Per-DC request pacing of 50 ms decaying to 3 ms between download parts, `FLOOD_WAIT` honoured by sleeping the indicated seconds, `requestRetries` default 5, per-request deadline 15 s.                                                                                                                                   | `network/MediaScheduler.ts:27-31`, `102-111`, `281-297`, `48-55`                                                 |
| Operational maturity                    | Actively pushed (2026-10-18), small issue count. The GramJS lineage means the MTProto/TL layer is battle-tested, but the download pool (`MediaScheduler`, `BalancePolicy`, `SenderSlot`) is new code — which is the part the benchmark exists to stress.                                                                  | repo metadata; `network/*`                                                                                       |

### 3.5 What a bot `file_id` costs in teleproto

The app's payload is a **Bot API** `file_id` string (plus size, name, mime). teleproto has no Bot-API-string decoder; it
needs `InputDocumentFileLocation{ id, accessHash, fileReference, thumbSize }` plus a `dc_id`. So the port must do one of:

1. decode the Bot API file id (MadelineProto does this in
   [`extractBotAPIFile()`](../vendor/danog/madelineproto/src/MTProtoTools/Files.php:991); the format is not documented
   by Telegram's Bot API docs), or
2. call WordPress `/resolve-file` and get the MTProto fields, or
3. fetch the message and read `document.fileReference` (an extra RPC per file).

…and on `FILE_REFERENCE_EXPIRED` re-resolve and rebuild the location, exactly as [`backend.php`](../backend.php) does
today. Neither library provides any of this.

---

## 4. grammers (Rust) — capability table, crate by crate

Repo: <https://codeberg.org/Lonami/grammers>. Dual Apache-2.0 / MIT (README "License"), 69 stars, 4 open issues,
updated 2026-10-15. `grammers-client 0.11.0` on crates.io (222,323 downloads, updated 2026-10-15) — **pre-1.0**, so a
minor bump may break the API.

### 4.1 `grammers-mtproto` and `grammers-mtsender`

| Concern           | Finding                                                                                                                                                                                              | Citation                                                                                           |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Transport model   | `Sender` is one connection; `SenderPool` owns "any number of `Sender`". A request for a DC is routed to the first connection whose `dc_id` matches, i.e. one connection per DC per pool.             | `grammers-mtsender/src/lib.rs:9-22`; `sender_pool.rs:328-332`                                      |
| Practical meaning | N concurrent downloads in one `Client` share a single TCP connection and a single MTProto container stream. Getting Telegram's "separate connections" model requires **N `Client`s or N processes**. | derived from the two citations above                                                               |
| Retries           | `RetryPolicy` / `AutoSleep` per connection; `do_invoke_in_dc` loops through `check_retry`.                                                                                                           | `sender_pool.rs:170-207`; `client/retry_policy.rs`                                                 |
| Cross-DC          | Auth keys are per DC in the session; `Client::copy_auth_to_dc()` exports and imports an auth key on demand.                                                                                          | `grammers-client/src/client/files.rs:127-132` (`AUTH_KEY_UNREGISTERED` leads to `copy_auth_to_dc`) |

### 4.2 `grammers-client` (download path)

| Concern          | Finding                                                                                                                                                                                                                                                                                              | Citation                                                            |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Streaming API    | **Yes** — `iter_download()` returns a pull-based `DownloadIter`; `next().await` yields `Vec<u8>` chunks. This maps directly onto an HTTP response body pump.                                                                                                                                         | `client/files.rs:36-141`, `162-194`                                 |
| Arbitrary offset | `chunk_size(4 KB..512 KB)` plus `skip_chunks(n)` (offset = n x chunk_size). Offset granularity is one chunk, so a Range start must be rounded down and the leading bytes discarded — or `chunk_size(4096)` used, at one RPC per 4 KB.                                                                | `client/files.rs:52-76`                                             |
| Parallel chunks  | Only the disk helper `download_media()` uses workers: `WORKER_COUNT = 4`, engaged above `BIG_FILE_SIZE = 10 MB`, each worker pulling `MAX_CHUNK_SIZE = 512 KB` parts with `precise: true`. `iter_download` itself is sequential. Results arrive out of order through a channel and the writer seeks. | `client/files.rs:30-34`, `211-234`, `246-350`                       |
| Chunk limits     | `MIN_CHUNK_SIZE 4 KB`, `MAX_CHUNK_SIZE 512 KB`, `cdn_supported: false`.                                                                                                                                                                                                                              | `client/files.rs:30-31`, `176-182`                                  |
| `file_reference` | Supplied by the caller through the `Downloadable` trait (`to_raw_input_location`, `size`, `dc_id`). The library carries an explicit `// TODO handle maybe FILEREF_UPGRADE_NEEDED`, so there is no refresh logic.                                                                                     | `media/downloadable.rs:11-26`; `client/files.rs:100`                |
| DC handling      | `FILE_MIGRATE_*` (303) retries that chunk in the target DC; `AUTH_KEY_UNREGISTERED` calls `copy_auth_to_dc` and retries.                                                                                                                                                                             | `client/files.rs:127-136`, `308-322`                                |
| Bot mode         | `auth::ImportBotAuthorization` is implemented, so bot login works and `invoke_in_dc` / `upload.getFile` are the same as for users.                                                                                                                                                                   | `client/auth.rs:186`                                                |
| Session          | `grammers-session` `Session` trait: `home_dc_id`, `dc_option`, `auth_key(dc)`, explicit `save()`. Storages are memory, sqlite and erased; the sqlite storage holds an in-process `AsyncMutex<Database>`, so one process owns a session.                                                              | `grammers-session/src/session.rs:20-60`; `storages/sqlite.rs:35-36` |
| Footprint        | `unmeasured`. One socket per DC, 512 KB per in-flight part, no GC.                                                                                                                                                                                                                                   | —                                                                   |

### 4.3 `grammers-session` and `grammers-tl-types`

- `grammers-tl-types` is generated from Telegram's TL schema and pinned to a layer, so `InputFileLocation` constructors
  match the current schema with no hand-written marshalling.
- Session `save()` is explicit and there is no cross-process locking — the same shape as teleproto. The app's
  "one session, many readers" problem is therefore **not solved by switching library**; it is solved by the bot pool
  (one session per bot).

### 4.4 Parity of the porting cost

Both libraries require the caller to (a) build the MTProto file location from the app's Bot API payload and (b) re-resolve
on `FILE_REFERENCE_EXPIRED`. Neither ships a bot-file-id decoder. **This is the single biggest porting item and it is
identical for both**, so it must not be used to break the tie.

---

## 5. Telegram-side limits that no library escapes

All from <https://core.telegram.org/api/files> (fetched 2026-10-19; `upload.getFile` plus "General Considerations").

| Limit                       | What it says                                                                                                                                                                                                                                                                                                 | Consequence for us                                                                                                                                                                                                                   |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Part alignment              | Without `precise`: offset and limit divisible by **4 KB**, and `1 MB % limit == 0`. With `precise`: 1 KB granularity and `limit <= 1 MB`.                                                                                                                                                                    | An HTTP `Range: bytes=1000-` cannot be passed through. Round down to 4 KB (or 1 KB with `precise`), discard the leading bytes, cap the part at 1 MB.                                                                                 |
| 1 MB chunk containment      | "the requested part should be within one 1 MB chunk from the beginning of the file", i.e. `offset / 1048576 == (offset + limit - 1) / 1048576`.                                                                                                                                                              | A 2 MB window must be split into two parts. 256 KB and 512 KB parts are safe.                                                                                                                                                        |
| Parallelism cap             | "clients should limit the parallelism to download at most `small_queue_max_active_operations_count` / `large_queue_max_active_operations_count` files in parallel when downloading files smaller/bigger than **20 MB**".                                                                                     | This is the server's stated bound on parallel downloads per DC. teleproto's 4 x 4 = 16 in-flight parts may already exceed it for multi-file traffic; the benchmark must find whether `FLOOD_WAIT` appears before the configured cap. |
| Dedicated connections       | "It is recommended that large queries (`upload.getFile`, `upload.saveFilePart`, `upload.getWebFile`) be handled through **one or more separate sessions and separate connections**, in which no methods other than these should be executed."                                                                | Favours teleproto's design, and works against grammers' one-connection-per-DC-per-process shape.                                                                                                                                     |
| Parallel sessions are legal | "each is just an extra `session_id` over the same `auth_key_id`, never carries updates, and can be opened without any re-authorization. The other use — running several parallel sessions on the main connection to increase RPC throughput — is controlled by the server through the `tmp_sessions` field." | The pool needs no re-login and no extra bots; rate limiting stays a server decision.                                                                                                                                                 |
| Own-DC requirement          | "If a media DC with the required DC ID is available (`dcOption` will have the `media` flag set), queries **must** be sent to that DC."                                                                                                                                                                       | Every candidate needs per-DC connections; files on DC1/DC2/DC4/DC5 behave differently from the home DC.                                                                                                                              |
| `file_reference` expiry     | `FILE_REFERENCE_EXPIRED` — "the `file_reference` field of the input location must be refreshed"; `FILE_REFERENCE_INVALID` is treated the same.                                                                                                                                                               | The adapter owns this. Expect it on every long-lived stream URL (AGENTS.md: half of warmup resolves fail).                                                                                                                           |
| Integrity                   | `upload.getFileHashes` returns a SHA-256 per part; "clients are recommended to verify hashes".                                                                                                                                                                                                               | Nice to have: teleproto exposes it behind `verifyHashes` (`client/downloads.ts:112-160`); grammers does not.                                                                                                                         |

**Not documented (so `unmeasured`):** a hard "max connections per account" number, and whether **bot** accounts get a
different `upload.getFile` budget than users. The observable limit is `FLOOD_WAIT`: measure the N at which it starts, per
bot, instead of assuming a number. This is also why "massive users" needs the existing bot pool regardless of language —
the pool is the unit of rate-limit budget, not the process.

---

## 6. VPS baseline (measured 2026-10-19)

`ssh -o BatchMode=yes admin@159.195.95.182` succeeds with the local default key (no password used, none recorded).

```
CPU(s): 8, Thread(s) per core: 1, Model name: AMD EPYC-Genoa Processor
Mem: 15986 MB total, 6740 used, 9467 buff/cache, 9246 available
Swap: 20479 MB total, 6964 MB used  (/swapfile, 7.2G per swapon)
Disk /: 503G, 259G used, 224G available
Kernel: 6.8.0-147-generic (Ubuntu 24.04)
Docker 29.1.3 | node v20.20.2 | cargo NOT INSTALLED | PHP 8.3.6 (cli, NTS)
php -m includes curl, fileinfo, mbstring, openssl, igbinary, redis, sockets, sodium (no grpc)
ss -s: TCP 1781 inet (1753 estab, 3806 timewait); ulimit -n: 65535
Listeners: 127.0.0.1:3010 (imdb-api), :3333 (nsfw-detector), 127.0.0.1:9306/9312 (Manticore), 127.0.0.1:8880 (telegram-bot)
docker ps: imdb-api (up 2 days), nsfw-detector (up 2 days)
The app itself is NOT running: no :8088 listener, and ~/pencarimovie-server does not exist — the checkout here is
~/pencarimovie-docker.
```

Two caveats that invalidate a benchmark run taken _today_:

1. **Load average 8.40 / 8.57 / 10.43 on 8 cores** — the box is saturated by unrelated work.
2. **7.2 GB of swap already in use** — RSS numbers would include paging noise.

So §7 requires an idle window (load below 1.0, swap usage flat) before numbers are comparable. `cargo` is absent, so the
Rust candidate must be built elsewhere (a musl static build is recommended) or `rustup` installed on the box.

---

## 7. The benchmark matrix

### 7.1 The one scenario that matters

N concurrent HTTP **Range** readers of one large file, served through each candidate's own HTTP surface, all on the VPS,
all from `127.0.0.1` (removes WAN and Cloudflare from the measurement).

- File: `short_code=1v9OPj`, 3.97 GB (`Avengers.Infinity.War.2018.2160p.10bit.HDR.BluRay.8CH.x265.HEVC.mkv`) — the file
  already used for every throughput number in AGENTS.md. Same file and same DC for every candidate.
- Byte windows: **256 KB** and **2 MB** (`Range: bytes=<off>-<off+size-1>`). 256 KB models a player's first-chunk fetch;
  2 MB models sustained streaming and forces the 1 MB-chunk split rule (§5).
- Ramp: **N = 1, 10, 50, 200** concurrent readers. Stop a tier early if the host is clearly saturated, and record why.
- Rounds: **3 per (N, window)**, reporting **min and median**, never the mean.
- Offsets: deterministic spread (`off = (i * 977 * 4096) % (size - window)`, floored to 4 KB) so every candidate reads
  the same bytes at the same indices.

### 7.2 Metrics and exactly how each is obtained

| Metric                     | How                                                                                                            | Notes                                                                        |
| -------------------------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| TTFR (time to first byte)  | `curl -w '%{time_starttransfer}'` per request; report p50 and p95                                              | The metric that decides whether seeking feels instant                        |
| Sustained KB/s per stream  | `curl -w '%{speed_download}'`, divided by 1024                                                                 | Compare against the 660-712 KB/s ceiling already measured                    |
| Aggregate KB/s             | sum of bytes divided by tier wall clock                                                                        | Shows whether N streams share one pipe or scale                              |
| Peak RSS                   | `ps -o rss= -p PID` sampled every 200 ms during the tier, keeping the max                                      | Report peak **and** per-active-stream: `(peak - idle) / N`                   |
| Idle RSS                   | one sample before the tier, after a warm-up request                                                            | The "what does an idle server cost" number                                   |
| CPU                        | `utime + stime` delta from `/proc/PID/stat`, divided by `getconf CLK_TCK`                                      | Report as **CPU-seconds per GB streamed**                                    |
| FDs                        | `ls /proc/PID/fd` line count at peak                                                                           | Catches socket leaks in a pool implementation                                |
| Error classes              | HTTP code per request, plus body search for `FLOOD_WAIT`, `FILE_REFERENCE_EXPIRED`, connection-limit text, 503 | Counts by class, not one total                                               |
| Saturation shape           | p95/p99 latency growth per tier, plus the arrival rate at which the first 503 or timeout appears               | MadelineProto exposes no queue depth; inferred queue depth is latency growth |
| Cost per concurrent stream | MB RSS per stream, CPU-seconds per GB streamed                                                                 | The two numbers the question actually asks for                               |

### 7.3 Controls

1. Same file, same DC, same offsets, same two window sizes.
2. Same host and same idle window (load below 1.0, swap flat), same `ulimit -n`.
3. Same client vantage point: on-VPS loopback to `127.0.0.1:<port>`; WAN runs are a separate, clearly labelled tier.
4. Warm state declared per tier: process idle RSS, bot session logged in, `file_id` freshly resolved.
5. 3 runs, min/median; discard a run only with a recorded reason (for example a `FLOOD_WAIT` the harness logged).

### 7.4 Pass/fail thresholds (derived from "massive users, low resource")

A candidate is **viable** if, at N = 200 concurrent 2 MB readers on the 8-core / 16 GB box with the box otherwise idle:

| Gate                 | Threshold                                                                                            | Rationale                                                                                                                       |
| -------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Peak RSS             | at most 1.5 GB total, at most 6 MB per active stream                                                 | 8-16 threads x 30-50 MB is what forces the current thread-cap dance; 1.5 GB leaves room for nginx, Manticore clients and the OS |
| TTFR p95             | at most 1.5 s loopback for 256 KB                                                                    | A seeking player should not wait; on loopback this isolates stack cost from Telegram latency                                    |
| Sustained per stream | at least 400 KB/s median at N = 50                                                                   | Below the measured 660-712 KB/s ceiling: allows DC variance, not stack collapse                                                 |
| Error rate           | 0 unhandled `FILE_REFERENCE_EXPIRED`, 0 connection-limit errors, `FLOOD_WAIT` at most 5% of requests | The app already re-resolves; an unhandled expiry is a port bug                                                                  |
| FDs                  | below `N * 2 + 200`                                                                                  | Catches socket leaks                                                                                                            |
| CPU                  | at most 0.5 CPU-seconds per GB streamed                                                              | Above that, TLS/AES/serialization dominates and the runtime choice is wrong                                                     |

Failing a row means that candidate does not carry "massive users" on this box without horizontal scaling. It may still be
the right pick if the failure is origin-bound (per-stream KB/s) rather than stack-bound (RSS, CPU, TTFR).

### 7.5 Runnable commands

Baseline (PHP/FrankenPHP), from the VPS:

```sh
# app already running on :8088, bot logged in, short_code resolved once
./_bench_stream_concurrency.sh --url 'http://127.0.0.1:8088/api/download/<token>/<b64payload>/x.mp4' \
    --n 50 --bytes 2097152 --rounds 3 --pid "$(pgrep -f frankenphp | head -1)"
```

teleproto (Node; one process per bot session):

```sh
NODE_OPTIONS=--max-old-space-size=1024 node teleproto-server.js --port 8091 --session ./sessions/bot1 \
    --download-pool '{"sessions":4,"inflightPerDc":4}'
./_bench_stream_concurrency.sh --url 'http://127.0.0.1:8091/dl/1v9OPj' --n 50 --bytes 2097152 --rounds 3 \
    --pid "$(pgrep -f teleproto-server | head -1)"
```

grammers (one process per connection set, Caddy or nginx in front):

```sh
./target/release/pm-stream --port 8092 --session ./sessions/bot1 --workers 4 &
./_bench_stream_concurrency.sh --url 'http://127.0.0.1:8092/dl/1v9OPj' --n 50 --bytes 2097152 --rounds 3 \
    --pid "$(pgrep -f pm-stream | head -1)"
```

The harness ([`storage/_bench_stream_concurrency.sh`](../storage/_bench_stream_concurrency.sh), written because VPS access
worked) takes `--url`, `--n`, `--bytes`, `--rounds`, `--pid` and prints one CSV line per run. It was validated **on the
VPS**: `sh -n` clean, placeholder URLs refused with exit 2, and a real run against a range-capable endpoint produced
`1,4,2048,4,0,206x4,54,58,35.2,34.5,27,293,2.1,2.1,0.0,3,0.00` — all four readers got HTTP 206 and the RSS, FD and
CPU columns populated from the sampled `--pid`. It is not wired into anything.

### 7.6 The glue each option implies (who owns what)

| Concern                             | Today (PHP)                                                     | teleproto (Node)                                                                                                                    | grammers (Rust)                                      |
| ----------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| HTTP Range serving                  | Caddy to FrankenPHP, one thread per stream                      | Node `http` (single event loop) or Caddy to Node                                                                                    | Caddy or nginx to N `pm-stream` processes            |
| Concurrency unit                    | FrankenPHP thread (8-16)                                        | event loop plus per-DC sender pool (4 x 4)                                                                                          | tokio task on one connection per DC per process      |
| Beating the single-connection limit | not applicable: the IPC worker is one session on one connection | the pool does it for you (`_media.getFile`); `iterDownload` does not                                                                | you do it: N processes, or N `Client`s in-process    |
| Single-thread honesty               | 8-16 threads                                                    | **one** JS thread per process; AES-CTR and TL marshalling are JS — expect `worker_threads` (one per bot session) at N of 50 or more | tokio multi-thread, no workaround needed             |
| Bot pool owner                      | `backend.php` (`fd_get_bot_pool`)                               | the adapter: one client or worker thread per bot                                                                                    | the adapter: one `Client` per bot, per process       |
| `file_reference` cache              | `storage/cache/resolve_cache_*.json` with `verified_at`         | port the same file, keyed per (short_code, bot)                                                                                     | same                                                 |
| Range alignment                     | already inside `downloadToBrowser`                              | adapter aligns to 4 KB and the 1 MB chunk                                                                                           | adapter aligns to `chunk_size` and discards the head |
| Streaming to the socket             | `downloadToBrowser` writes the response itself                  | `for await` over `iterDownload` or `getFile`, then `res.write()` with `drain` backpressure                                          | response body fed by `DownloadIter::next()`          |

### 7.7 What would make us NOT migrate (measurable triggers)

Keep MadelineProto if **any** of these holds after the benchmark:

1. The baseline already passes every gate in §7.4 at N = 200 on an idle box. The bottleneck is then the DC, and the port
   buys nothing.
2. Per-stream KB/s is within 10% of the 660-712 KB/s ceiling for all three candidates at N of 10 or less — the stack is
   not the limiter at the concurrency that matters.
3. A candidate's peak RSS at N = 50 exceeds 1.5 GB or its CPU exceeds 0.5 CPU-seconds per GB — porting would trade a
   known operational risk (IPC) for a bigger one (memory).
4. `FLOOD_WAIT` starts at N below 20 per bot for a candidate: the bot pool, not the library, is the limit, and a rewrite
   does not raise the ceiling.
5. The Bot API `file_id` decode plus `FILE_REFERENCE_EXPIRED` port costs more than 2 engineer-weeks without a per-candidate
   PoC that already streams one range successfully (§3.5 and §4.4 make this the likely long pole).

Migrate if the baseline fails RSS, CPU or TTFR while a candidate passes all six gates — that is the only outcome that
justifies the port.

---

## 8. Recommendation and open questions

**Recommendation.** Build the two PoCs in this order, about one day each:

1. **teleproto first.** Range endpoint driving `client._media.getFile(dcId, location, offset4k, 512 KB)` in a loop with
   4 sessions per DC and `write()` backpressure. If it holds 200 streams under 1.5 GB RSS with p95 TTFR at or below
   1.5 s, it is the answer. Also run `iterDownload` once to quantify how much the pool actually buys.
2. **grammers second**, but as **N processes** (one `Client` or connection set per worker), not one process with 200
   tasks — otherwise the benchmark measures a single MTProto connection, not the library.
3. Measure the **current PHP build on the same idle box** in the same session. Without that column the comparison is a
   guess, and AGENTS.md's existing numbers were taken on Heroku and Render, not here.

**Open questions (all `unmeasured`, with the measurement that answers each):**

- Peak RSS per active stream for teleproto and grammers — §7.2 RSS sampling.
- Whether `FLOOD_WAIT` appears at a lower N than Telegram's documented `..._max_active_operations_count` for **bot**
  accounts — ramp N against a single bot until the first `FLOOD_WAIT`, then repeat with the pool.
- Whether Node's single event loop keeps up with AES-CTR plus TL marshalling at N = 200 — run teleproto with and without
  `worker_threads`.
- Whether grammers' one connection per DC actually throttles a single download with 4 in-flight parts (MTProto containers
  batch, so it may not) — compare 1 process x 4 workers against 4 processes x 1 worker.
- The real cost of the Bot API `file_id` decoder plus expiry refresh in each language — timebox a PoC; it is the same
  work in both.
- Whether 200 concurrent readers is even reachable through Cloudflare Tunnel in production — out of scope here; measure
  loopback first, then one WAN tier.
