# Architecture

PencariMovie Server turns Telegram file references into HTTP streams that Stremio,
Nuvio, Eclipse Music and a browser can play. It is **one Rust binary**.

```
                       ┌──────────────────────────────────────────────┐
  Stremio / Nuvio ────▶│  pencarimovie-server  (server/)              │
  Eclipse Music  ────▶│                                              │
  browser        ────▶│  axum router                                 │
                       │   ├── /            → embedded dashboard      │
                       │   ├── /manifest.json, /catalog, /meta,       │
                       │   │   /stream, /subtitles  (Stremio addon)   │
                       │   ├── /eclipse/*               (music addon) │
                       │   ├── /api/*                   (dashboard)   │
                       │   └── /api/download            (file bytes)  │
                       │                                              │
                       │  grammers MTProto client ──▶ Telegram DCs    │
                       │  reqwest ──▶ WordPress REST/AJAX (catalog)   │
                       └──────────────────────────────────────────────┘
```

## Components

| Path                                                        | Responsibility                                                                                                                              |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| [`server/src/routes.rs`](../server/src/routes.rs)           | Every HTTP route: the Stremio/Eclipse addon surface, `/api/*`, and the download streamer.                                                   |
| [`server/src/telegram.rs`](../server/src/telegram.rs)       | grammers client lifecycle, bot login, file download chunks.                                                                                 |
| [`server/src/resolve.rs`](../server/src/resolve.rs)         | `short_code` → `file_id` resolution, cache layers, plugin fallbacks.                                                                        |
| [`server/src/wp_data.rs`](../server/src/wp_data.rs)         | Cached WordPress calls (catalogs, posts, post-files) with single-flight and stale-while-revalidate.                                         |
| [`server/src/matcher.rs`](../server/src/matcher.rs)         | Filename → title/episode matching. This is where most regressions historically came from; it mirrors the archived PHP matcher case by case. |
| [`server/src/assets.rs`](../server/src/assets.rs)           | `include_dir!` of `<repo>/public` — the dashboard is compiled into the binary.                                                              |
| [`server/src/auth.rs`](../server/src/auth.rs)               | Server password + static token, loopback detection, route policy.                                                                           |
| [`server/src/update.rs`](../server/src/update.rs)           | In-app OTA against the release feed.                                                                                                        |
| [`server/src/tunnel.rs`](../server/src/tunnel.rs)           | Cloudflare tunnel management.                                                                                                               |
| [`server/src/diagnostics.rs`](../server/src/diagnostics.rs) | Incident capture (crash / ANR / stream-miss / playback) and upload.                                                                         |

## The two things that must stay where they are

1. **`public/` at the repository root.** [`server/src/assets.rs`](../server/src/assets.rs)
   embeds it at compile time with `include_dir!("$CARGO_MANIFEST_DIR/../public")`, and
   [`server/build.rs`](../server/build.rs) folds every file's path+size+mtime into the
   crate fingerprint so a dashboard change always forces a rebuild. Moving `public/`
   requires editing both, plus the `COPY` pair in the [`Dockerfile`](../Dockerfile).
   `PM_ASSETS=disk` switches the server to reading `public/` from the checkout instead
   of the embedded copy, which is how you iterate on the UI without a rebuild.

2. **The root installers.** `pencarimovie-linux.sh`, `pencarimovie-termux.sh`,
   `pencarimovie-windows.bat`, `pencarimovie-libwrt.sh` and `update.ps1` are reached
   through `telegra.my/<platform>` (a Cloudflare worker) and through documented
   `raw.githubusercontent.com/aiskendi/pencarimovie-server/main/<file>` URLs. Their
   names and locations are a public contract.

## Release feed

[`release.yml`](../.github/workflows/release.yml) is manual (`workflow_dispatch`).
It publishes five native tarballs — `pencarimovie-server-{linux-x86_64,
linux-aarch64,windows-x86_64,macos-aarch64,macos-x86_64}.tar.gz` — each containing:

```
pencarimovie-server-<target>/
├── pencarimovie-server        (or .exe)
├── README.txt
├── start.sh | start.bat | start.command      the desktop launchers
├── stop / restart / autostart / apply-update …
└── storage/                   deliberately EMPTY
```

The in-app OTA and the POSIX installers resolve those exact names, so the internal
layout is part of the contract too. The same workflow builds a musl binary per Linux
arch and publishes `ghcr.io/<repo>:native` (multi-arch), which is what
[`render.yaml`](../render.yaml) and the Heroku promotion workflow consume.

## Deployment

| Target                     | File                                          | What it does                                                           |
| -------------------------- | --------------------------------------------- | ---------------------------------------------------------------------- |
| Docker (build from source) | [`Dockerfile`](../Dockerfile)                 | `FROM rust:1-alpine`, builds `server/`, one-binary alpine runtime.     |
| Docker (prebuilt)          | [`Dockerfile.release`](../Dockerfile.release) | COPY-only: packages the musl binary from CI.                           |
| Heroku                     | [`heroku.yml`](../heroku.yml)                 | container stack, builds `Dockerfile`, `run: web: pencarimovie-server`. |
| Render                     | [`render.yaml`](../render.yaml)               | `runtime: image` on `ghcr.io/...:native`.                              |
| Compose                    | [`docker-compose.yml`](../docker-compose.yml) | consumes the published image; `./storage:/data`.                       |

Runtime state lives wherever `PM_APP_ROOT` points (default: the directory holding the
binary), which is why every image sets it to `/data` and mounts a volume there.

## Historical engine

The PHP + FrankenPHP + MadelineProto implementation is archived under
[`legacy/php/`](../legacy/php). It is not built, released or deployed. It is kept because
[`server/PARITY.md`](../server/PARITY.md) and [`server/PORT_GAPS.md`](../server/PORT_GAPS.md)
use it as the behavioural specification — go there for "what did the old engine do in
this case", not here.
