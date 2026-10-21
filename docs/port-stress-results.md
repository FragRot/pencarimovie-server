# backend.php → TypeScript / Rust: port + stress results

Measured 2026-10-19/20 on VPS `159.195.95.182` (8× AMD EPYC-Genoa, 15.9 GB RAM, Ubuntu 24.04).
Host load during the runs: **6.2 – 17.9** from unrelated work, so treat absolute numbers as a lower bound;
all three stacks ran on the same box, same file, same byte windows, same offsets.

- File: `short_code=1v9OPj`, **3,967,229,576 B** (3.97 GB MKV), same one used for every throughput number in [`AGENTS.md`](../AGENTS.md).
- Windows: **1 MB** (`Range: bytes=off-(off+1048575)`), 4 MB stride between readers, 4 KB aligned.
- Metric definitions: `agg_MBps` = total bytes ÷ tier wall clock; `med_KBps` = median per-stream rate;
  `MB_per_stream` = (peak RSS − idle RSS) ÷ N; `cpu_s_per_GB` = CPU-seconds ÷ GB served.
- Harness: [`storage/_vps_stress.mjs`](../storage/_vps_stress.mjs) (node, no deps). Raw CSV in `/tmp/round2.csv`, `/tmp/r4.csv`, `/tmp/r5.csv` on the box.

## Ports

|          | TypeScript                                                                       | Rust                                                                 |
| -------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Library  | **teleproto** 1.230.1                                                            | **grammers** 0.11.0 (`grammers-client`/`-session`/`-tl-types`)       |
| HTTP     | `node:http`                                                                      | `axum` + `tokio`                                                     |
| Location | [`port/ts/src`](../port/ts/src) (17 modules)                                     | [`port/rust/src`](../port/rust/src) (17 modules)                     |
| Build    | `npm run build` → `dist/server.js`                                               | `cargo build --release` → **11,718,624 B** binary (cold 1 m 58 s)    |
| Ledger   | [`port/COVERAGE.md`](../port/COVERAGE.md) — 106 ported / 129 not ported / 24 N/A | [`port/rust/PARITY.md`](../port/rust/PARITY.md) — every group mapped |

Both do their **own** bot login via `GET {WP_API_BASE}/provision-session` → `{bot_token, api_secret}`
(the same path as [`backend.php:1146`](../backend.php:1146)); no session was copied between stacks.

## Throughput / latency / resource table

| stack                                | N=1                                            | N=10                                                                 | N=50                                                                                                            | N=200                                                                                                                                                             |
| ------------------------------------ | ---------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **PHP** (FrankenPHP + MadelineProto) | 5.2 MB/s agg · ttfb 189 ms · RSS 92.7 MB       | 8.1 MB/s · ttfb 855/1233 ms · 1.33 MB/s per stream · peak 154 MB     | 10 MB/s · ttfb 2766/4812 ms · 381 KB/s per stream · peak 235 MB                                                 | **200/200** · 10.5 MB/s · ttfb **9674/18299 ms** · **107 KB/s** per stream · peak **262 MB** · CPU 14 s                                                           |
| **TS** (teleproto)                   | 9.7 MB/s · ttfb **99 ms** · RSS 86 → 88 MB     | 11.3 MB/s · ttfb 727/881 ms · 1.44 MB/s · peak 100 MB                | 16.5 MB/s · ttfb 486/2988 ms · 2.23 MB/s · peak 126 MB · 75 FDs                                                 | **200/200** · **17–19.7 MB/s** · ttfb 3.0–6.5 s / 10–11.5 s · 201–341 KB/s · peak **103–152 MB**, settles to **90 MB** · 225 FDs · CPU 3.8 s (**19.5 cpu-s/GB**)  |
| **Rust** (grammers)                  | **13.9 MB/s** · ttfb **67 ms** · RSS **13 MB** | **36.5 MB/s** · ttfb **229/271 ms** · **4.58 MB/s** · peak **19 MB** | 15.8–22 MB/s · ttfb 1686–2664 ms · 388–614 KB/s · peak **32–62 MB** · 62–69 FDs · CPU 0.5 s (**10.9 cpu-s/GB**) | **200/200** (after the fix, `--workers 1 --inflight 8`) · 16.7 MB/s · ttfb p95 10.7 s · 158–220 KB/s · peak **40 MB**, settles to **20–30 MB** · **0 × HTTP 500** |

Memory behaviour after the burst settles (the "does the garbage come back" question):

| stack | idle    | peak @ N=200 | 5 s after burst          | MB per concurrent stream |
| ----- | ------- | ------------ | ------------------------ | ------------------------ |
| PHP   | 92.7 MB | 262 MB       | 232 MB (post-run sample) | ~1.3                     |
| TS    | 28.1 MB | 102.7 MB     | **89.8 MB**              | ~0.4–0.5                 |
| Rust  | 21.9 MB | 23.0 MB      | **23.1 MB**              | ~0.1                     |

Rust and TS both return memory to the OS after the burst (no leak); Rust's footprint is effectively
flat because it holds no per-stream heap beyond a 512 KB chunk buffer.

## Why Rust failed at N=200, and the fix that worked

Two independent ceilings, both real:

1. **One MTProto connection per DC per `Client`.** 200 concurrent `upload.getFile` multiplexed onto a
   single TCP connection and the DC reset it: `Connection reset by peer (os error 104)` →
   **104/200 OK, 96 × HTTP 500**. Adding `--workers` alone (more `Client`s) did not fix it — 8 clients
   gave 61–196/200 with the error class merely shifting.
2. **Telegram's per-account budget.** The 8-client run logged **1,206 × `FLOOD_WAIT (420)`** plus 914
   resets in one burst — 7 minutes of throttle per hit, for **one bot**. No language or connection count
   changes that; the unit of rate-limit budget is the **bot**, which is why the app already has a pool.

The fix is the shape TS already had (`MediaScheduler`: 4 sender slots × 4 in-flight per DC) — bound the
work, don't add threads:

- `BotEntry` gained **one `tokio::sync::Semaphore` per `Client`**; `pick_slot()` returns client + its own
  gate, so a request can never borrow another connection's permit. `--inflight N` (default **8**) per
  connection: `--workers 8 --inflight 8` = 64 parts in flight, the multi-connection model Telegram
  documents. Requests queue instead of failing.
- `FLOOD_WAIT` is classified (`rpc.value` = seconds): ≤ 30 s → sleep `n+1`, retry the part **once**;
  otherwise **429 + `Retry-After`** with `{"ok":0,"message":"Upstream Telegram rate limit, please retry shortly.","retry_after":N}`.
  Never a 500.
- Transport resets (`reset by peer`, `read 0 bytes`, `broken pipe`) retried **3×** with 100/300/700 ms
  backoff inside the same permit. `FILE_REFERENCE_EXPIRED` keeps its single re-resolve path.
- `--max-streams` (default **256**) caps concurrent bodies; over 5 s of waiting → **503 + `Retry-After: 1`**.

### Where the throttle actually starts — viewers vs parts in flight

The throttle tracks **concurrent `upload.getFile` parts per account**, not viewers. Measured ramp
(`--workers 1`, 1 MB windows, 200 × 1 MB per tier, 8 s cooldown, `flood_lines` = log lines matching
`rate limited`/`FLOOD_WAIT`, all requests still answered):

| viewers | `--inflight 8`              | `--inflight 32`              |
| ------- | --------------------------- | ---------------------------- |
| 8       | 8/8 · 17.9 MB/s · **0**     | 8/8 · 17.6 MB/s · **0**      |
| 16      | 16/16 · 17.1 MB/s · **0**   | 16/16 · 41.9 MB/s · **0**    |
| 32      | 32/32 · 28.9 MB/s · **0**   | 32/32 · 29.6 MB/s · **0**    |
| 64      | 64/64 · 19.7 MB/s · **0**   | 64/64 · 22.9 MB/s · **0**    |
| 128     | 128/128 · 19.5 MB/s · **0** | 128/128 · 15.9 MB/s · **30** |
| 200     | **200/200 · 16.5 MB/s · 0** | 200/200 · 16.8 MB/s · **6**  |

Two conclusions:

1. **With the gate at 8 parts in flight there is no flood up to 200 concurrent viewers.** Telegram never
   throttled, because the queue keeps the request rate below its budget. The cost is latency, not errors:
   ttfb p95 goes 444 ms at N=8 → 6.5 s at N=128 → 8.4 s at N=200.
2. **Raise the gate and the flood appears** — at 32 parts in flight it starts between 64 and 128 viewers
   (30 events at N=128). Requests still succeed (429 → sleep → retry), but the account is now the limiter.
   That is the number to size against: **~8 parts in flight per bot is free; ~32 per bot starts throttling.**

So "how many users until FLOOD_WAIT" is the wrong question to size a deployment on — the answer for one bot
is "200 viewers at 8 in flight, 0 floods, ~85 KB/s each". The question that decides capacity is
"how many parts in flight per bot", and the answer from this ramp is roughly **≤8 free, ≈32 throttled**,
i.e. throughput per bot is bounded near 16-20 MB/s regardless of how many viewers are queued behind it.

### Scaling by bots is blocked upstream, not by the runtime

Bots are not the scarce resource: `GET /provision-session` minted **4 distinct bots in 4 consecutive
calls** (8100453070, 8148573276, 7894320434, 7834258785), unbounded as far as we pushed it.

What does not work is **using a second bot to serve a file the first bot resolved**. Measured with 3 extra
instances, each provisioned with its own bot, all watching the same 3.97 GB title:

```
bot-8092-n100  100/100   24.1 MB/s        (owns the reference)
bot-8093-n100    0/100   410:HTTP 410
bot-8094-n100    0/100   410:HTTP 410
bot-8095-n100    0/100   410:HTTP 410
```

And on a short_code no bot had ever resolved (`6sNjWN`), **both** the first and a second bot got `410` —
so it is not "first resolver wins", it is "the reference belongs to the bot WordPress used as **sender**".
A forced re-resolve (`force=1&nocache=1`) does not re-mint it. PHP is equally unable to resolve that code,
while the owned code streams `206` in 25 ms.

Consequence for capacity: **`bots × ~16-20 MB/s` only holds for titles each bot owns.** Pooling bots does
not scale a single hot title until WordPress relays **per requesting bot** (or a `BOT_TOKEN` the operator
owns is used, the [`backend.php:1123`](../backend.php:1123) path). That is the prerequisite for the
horizontal-scaling plan, and it is a WordPress-side change, not a port-side one.

### Before / after, N=200, 1 MB, median of 3 runs

| config                     | before  | after           | agg MB/s | ttfb p95  | peak RSS | FDs | HTTP 500 |
| -------------------------- | ------- | --------------- | -------- | --------- | -------- | --- | -------- |
| `--workers 1 --inflight 8` | 104/200 | **200/200**     | 16.7     | 10 740 ms | 40.2 MB  | 212 | **0**    |
| `--workers 4 --inflight 8` | –       | **200/200**     | 17.1     | 9 738 ms  | 82.8 MB  | 215 | **0**    |
| `--workers 8 --inflight 8` | 61/200  | **195–196/200** | 15.6     | 11 045 ms | 110 MB   | 219 | **0**    |

Error classes, before → after: `FLOOD_WAIT` 1206 → **26 lines / 13 requests, all answered 429**;
`Connection reset` 914 → **0 at w1/w4**, 25 at w8 all absorbed by the retry ladder (253 part retries);
`read 0 bytes` 150 → **0**; HTTP 500 → **0**.

`ttfb` in the after-runs includes **queueing behind the permit** — that is the intended trade: latency
for correctness. `--workers 8` still loses 4–5 of 200 to a 429 because Telegram throttles one account at
64 concurrent parts; that is a retryable 429, and `--workers 1/4` are clean 200/200.

## Two defects found (both real, one shared)

1. **Bot API `file_id` decoder: TL string padding.** A TL string pads `(len prefix + body)` to a
   multiple of 4, not `body` alone. For a 25-byte `file_reference` the pad is **2**, not 3, so `id` and
   `access_hash` were read one byte shifted — `id` came out **negative** (`-2929793036349079547`).
   Telegram then answered `FILE_REFERENCE_EXPIRED` (reference valid, id not). Fixed in
   [`port/rust/src/fileid.rs`](../port/rust/src/fileid.rs:145) **and** [`port/ts/src/fileid.ts`](../port/ts/src/fileid.ts:145).
   After the fix both decode `id=6289489716727252339`, `access_hash=7927174950789710807`, `dc=5`, 25 B ref,
   and the same range returns `206` with `1a45dfa3` (Matroska) / `fLaC` magic.
2. **Not a defect, but worth recording:** the resolve cache is _already_ keyed per
   `(short_code, bot)` in the TS port (`${shortCode}:${botId}`, [`port/ts/src/resolve.ts:41`](../port/ts/src/resolve.ts:41)).
   A bot change still produced `410` after clearing that cache and re-resolving, which is what proved
   the cause is upstream (next section) rather than local caching.

## Upstream blocker for reproducible multi-bot runs (not in this repo)

WordPress `/resolve-file` returns a `file_id_mt` whose `file_reference` was minted for the **sender**
bot. A bot that did not mint it gets `FILE_REFERENCE_EXPIRED` — measured as a **single** request, not a
concurrency effect (1/1 at 410). Evidence: the fresh bot's row carries its own `bot_id` and a new
`file_id_mt` in the response, yet `upload.getFile` still answers 410; the bot that originally resolved
the file streams it fine. Until WordPress relays per requesting bot (or a `BOT_TOKEN` the operator owns
is supplied, the [`backend.php:1123`](../backend.php:1123) path both ports implement), a fresh bot
cannot stream a file another bot resolved, and N=200 byte-throughput can only be measured for a bot
that is the sender.

## Answer to the original question — "what should we test for massive users, low resource"

The stack is not the ceiling; the Telegram account is. Test in this order:

1. **Per-bot concurrency to first `FLOOD_WAIT`** (we saw it at N=200 on one bot; the useful number is
   the N where the first `FLOOD_WAIT` appears, per bot) → that number × pool size = the real capacity.
2. **RSS per concurrent stream and RSS after settle** (0.1 MB Rust / 0.4 MB TS / 1.3 MB PHP here).
3. **TTFR p95 at N=200** — the only metric where the three stacks differ by more than 2×
   (Rust 67 ms at N=1, TS 99 ms, PHP 189 ms; at N=200 PHP is 9.7 s p50).
4. **CPU-seconds per GB** (Rust 10.9–16.8, TS 19.5, PHP ~7.3 but at 107 KB/s per stream).
5. Then, and only then, the bot-pool sharding experiment: N viewers spread over M bots, and find
   whether the per-bot ceiling is additive.
