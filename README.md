# 🎬 PencariMovie Server

Self-hosted Telegram MTProto bridge that streams your own Telegram media to
Stremio, Nuvio, Eclipse Music and the browser — with `HTTP 206` seeking.

One self-contained binary. It runs on your hardware, talks to Telegram directly,
and never relays your files through anyone else's server.

---

## Install

| Platform          | Command                                  |
| ----------------- | ---------------------------------------- |
| **Android (APK)** | [telegra.my/apk](https://telegra.my/apk) |
| **Windows 10/11** | `irm telegra.my/win \| iex` (PowerShell) |
| **Linux**         | `curl -fsSL telegra.my/linux \| bash`    |
| **macOS**         | `curl -fsSL telegra.my/mac \| bash`      |
| **Termux**        | `curl -fsSL telegra.my/termux \| bash`   |
| **Docker**        | see below                                |

Then open the dashboard: **`http://127.0.0.1:8088`**
The terminal also prints your LAN address, e.g. `http://192.168.1.20:8088`.

On Android: install the APK, tap **Start Server**, open the dashboard.

## Docker

```bash
docker run -d --name pencarimovie-server --restart unless-stopped \
  -p 8088:8088 -v pencarimovie-data:/app/storage \
  ghcr.io/aiskendi/pencarimovie-server:latest
```

One-click deploys (Orkestr / Railway / Render / Heroku) are on the
[releases page](https://github.com/aiskendi/pencarimovie-server/releases).

## Add the addon

Open the dashboard → **Addon** → _Install via Stremio API Sync_, or paste the
manifest URL into your player by hand:

- **Same Wi-Fi:** `http://<LAN-IP>:8088/manifest.json`
- **Remote / VPS:** `http://<SERVER-IP>:8088/<token>/manifest.json`

Nuvio: **Profile → Content & Discovery → Addons**.
Eclipse Music: **Settings → Connections → Addon**, URL `/eclipse/manifest.json`.

## Configuration

Set only what you need — everything else is automatic.

| Variable          | Default    | Purpose                                           |
| ----------------- | ---------- | ------------------------------------------------- |
| `PORT`            | `8088`     | Listen port.                                      |
| `SERVER_PASSWORD` | `123456`   | Dashboard password for public/tunnel access.      |
| `SERVER_TOKEN`    | generated  | Fixed token so your addon URL survives redeploys. |
| `BOT_TOKEN`       | auto       | Use your own Telegram bot instead of a guest bot. |
| `PM_APP_ROOT`     | `storage/` | Where sessions, auth and caches live.             |

Engine tuning, for advanced setups only: `PM_INFLIGHT`, `PM_MAX_STREAMS`,
`PM_PIPELINE`, `PM_CACHE_DB`, `PM_LISTEN_HOST`.

## Security

- **Localhost and your own Wi-Fi (RFC-1918) are password-free.** Nothing to
  configure on a desktop, a Pi, or a phone at home.
- **Public, VPS and tunnel access require `SERVER_PASSWORD`**, and remote players
  authenticate with the token in the URL path.
- Admin actions (settings, bot pool, tunnel) are refused over public tunnels.

```bash
pms password <new>     # change the password
pms token              # print the access token
pms token rotate       # rotate it (invalidates the old URL)
```

## CLI (`pms`)

```bash
pms start | stop | restart      # run the server
pms tunnel                      # Cloudflare tunnel
pms autostart on|off            # start on boot
pms password | token | uninstall
```

Windows additionally gets a tray icon; Android runs it as a background service.

## Repository layout

```
src/        the engine (axum + grammers)
public/     the dashboard, embedded into the binary at build time
desktop/    start / stop / tray launchers shipped in each release tarball
legacy/     the retired PHP engine, kept for reference only
```

The APK projects are built locally and are not part of this repository — see
[ANDROID-SIGNING.md](ANDROID-SIGNING.md).

## Legal

**No content is hosted, cached or redistributed.** Streams go from Telegram
directly to your player. You are responsible for using this software lawfully.

Copyright inquiries: **dmca@pencarimovie.com**

Licensed under **GPL-3.0**. Built with [grammers](https://codeberg.org/grammers/grammers),
[cloudflared](https://github.com/cloudflare/cloudflared) and
[Termux](https://github.com/termux/termux-app).
