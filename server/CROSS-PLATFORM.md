# Cross-platform builds and packaging (Rust port)

The port is **one executable** — the dashboard (`public/`, 113 files / ~16 MB) is
compiled in with `include_dir!` ([`src/assets.rs`](src/assets.rs)) — so every
artifact below is "a binary, a launcher and an empty `storage/`". No PHP, no
Composer, no `vendor/`, no `public/` folder beside it.

Verified numbers are from this workspace on 2026-10-19/20; anything marked
**untested** was _not_ executed and says exactly what it needs.

---

## 1. Per-target table

| #   | Target triple                           | Toolchain needed                                                              | Artifact                                                                                 | How to run                                                                  | Status                                                                                                                                                                                                                                                                                                                                                                                                  |
| --- | --------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `x86_64-pc-windows-msvc`                | rustup target + MSVC build tools (VS "Desktop development with C++")          | `pencarimovie-rust.exe` **30,980,608 B** + `start.bat` + `README.txt` (zip 10,105,556 B) | `start.bat`, or `pencarimovie-rust.exe --port 8088`                         | **VERIFIED** — `cargo test --release` **108/108**, exe ran from a fresh temp `PM_APP_ROOT`, [`tools/smoke_boot.mjs`](tools/smoke_boot.mjs) **16/16** (boot, `/api/version`, `/`, `/app.js`, `/css/bundle.css`, `/manifest.json` 200 / **7,516 B** / 46 catalogs, `/nuvio` alias, `/catalog` 44 metas 308 ms, `/api/session` first run, `/api/auth/status`, `/stream` card, `/api/download` payload 400) |
| 2   | `x86_64-unknown-linux-gnu`              | rustup + gcc/ld (any distro)                                                  | `pencarimovie-rust` **34,751,096 B** (tar.gz 10,915,618 B, sha256 `43f80c9b7f8b…e8bf`)   | `./start.sh` (`PORT=… ./start.sh`, `--host 0.0.0.0` for LAN)                | **VERIFIED on a real Linux host** (Ubuntu 24.04, 8 cores, cold build **1 m 18 s**): `--version` → `… (x86_64-linux)`; the packaged tarball extracted and run under a fresh `PM_APP_ROOT` → **16/16** of the same smoke (manifest 7,516 B / 46 catalogs, `/` 65,182 B **2 ms**, `/catalog` 44 metas **95 ms**)                                                                                           |
| 3   | `aarch64-unknown-linux-gnu`             | rustup on the ARM64 host itself (the same script maps `uname -m` → triple)    | `pencarimovie-rust` + `start.sh`                                                         | `./start.sh`                                                                | **untested** — the ARM64 host (`rpi4`) was unreachable from here. Build it _on_ the Pi/VPS (`./scripts/build-rust-linux.sh`); it is a normal gnu build, nothing Android-specific                                                                                                                                                                                                                        |
| 4   | `aarch64-linux-android` (arm64-v8a)     | **NDK r27** (`ndk;27.0.12077973`) + `rustup target add aarch64-linux-android` | `pencarimovie-rust` **34,925,248 B**, ELF PIE, `for Android 21`, `linker64`              | Termux: `./start.sh`; adb: `adb push` → `/data/local/tmp`, `chmod 755`, run | **built + ELF-verified** (machine `AArch64`, `DYN`/PIE), on-device run **untested** (no arm64 device here — LDPlayer is a 32-bit x86 image)                                                                                                                                                                                                                                                             |
| 5   | `armv7-linux-androideabi` (armeabi-v7a) | NDK r27 + `rustup target add armv7-linux-androideabi`                         | `pencarimovie-rust` **29,568,840 B**, ELF PIE, `for Android 21`, `linker`                | same                                                                        | **built + ELF-verified** (machine `ARM`, 32-bit, `DYN`/PIE), on-device run **untested** (the harness ran the native x86 build instead)                                                                                                                                                                                                                                                                  |
| 6   | `i686-linux-android` (x86)              | NDK r27 + `rustup target add i686-linux-android`                              | `pencarimovie-rust` **35,294,516 B**, ELF PIE, `for Android 21`, `linker`                | same                                                                        | **VERIFIED end-to-end** on LDPlayer9 `emulator-5554` (Android 7.1.2 / API 25): `--version` → `… (x86-android)`, bound `0.0.0.0:8088`, banner `Storage: …/state/storage` (the `PM_APP_ROOT` override, see §3.1), `/api/version` ok, `/manifest.json` 200 **7,519 B / 46 catalogs**, `/` 200 65,182 B **4 ms**, `/app.js` 268,715 B, `/api/session` → `runtime: rust-grammers`                            |
| 7   | `x86_64-linux-android` (x86_64)         | NDK r27 + `rustup target add x86_64-linux-android`                            | `pencarimovie-rust` **34,721,368 B**, ELF PIE, `for Android 21`, `linker64`              | same                                                                        | **built + ELF-verified** (machine `x86-64`, 64-bit, `DYN`/PIE), on-device run **untested** (no x86_64 emulator here)                                                                                                                                                                                                                                                                                    |

Minimum Android API is **21 (Android 5.0/5.1)** — the same `minSdk` the Kotlin
app (`android/pencarimovie-server`) uses, which is also why the two 32-bit ABIs
are built.

## 1.1 Android has an installable APK — `android/pencarimovie-rust`

The bare ELF in the table above is for Termux / `adb push`. **End users install
the APK**: [`android/pencarimovie-rust`](../../android/pencarimovie-rust) is the
`android/pencarimovie-server` app shell (same screens, notification, wake/Wi-Fi
locks, LAN IP, watchdog, boot receiver) with the TDLib engine replaced by this
binary, which it runs as a child process.

- `applicationId com.pencarimovie.rust` — it coexists with the TDLib app.
- Each ABI's ELF is shipped as `jniLibs/<abi>/libpencarimovie_rust.so`, because
  `nativeLibraryDir` is the only app-readable path that carries `exec_type`
  (`extractNativeLibs="true"` + `keepDebugSymbols` are load-bearing).
- `PM_APP_ROOT` = `<filesDir>/pm`, so `storage/` is app-private.
- Build: `scripts\build-rust-apk.bat` (cross-builds the four ABIs, stages
  jniLibs, runs `assembleRelease`, copies APKs + `.sha256` into
  `android\_apk_dist\`). APKs are **20.1-21.2 MB** per ABI.
- Verified on LDPlayer9 (Android 7.1.2 / API 25, x86, signed **release** APK):
  `server is up on port 8088` ~400 ms after Start; `/api/version` 200,
  `/manifest.json` 200 / 7,519 B / 46 catalogs, `/` 200 / 65,182 B,
  `/catalog/movie/pm_movies_top.json` 200 / 21,148 B; STOP →
  `server stopped (pid=…)` and the port is released.

Details and the two Android-API traps (`Process.isAlive()` and friends are
API 26, this app is minSdk 21): [`android/pencarimovie-rust/README.md`](../../android/pencarimovie-rust/README.md).

## 2. Scripts

| Script                           | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/build-rust-windows.bat` | `cargo build --release` → `server/dist-rust/windows-x86_64/` (exe, `start.bat`, `README.txt`, empty `storage/`) + `pencarimovie-rust-windows-x86_64.zip`. Asserts `--version`. Pure `cmd`, no PowerShell.                                                                                                                                                                                                                                                                                                                                                                                                          |
| `scripts/build-rust-linux.sh`    | Builds on the host, maps `uname -m` → triple, asserts `--version`, stages `dist-rust/linux-<arch>/` and `pencarimovie-rust-<arch>.tar.gz` (binary + `start.sh` + `README.txt` + empty `storage/`). The tarball's `start.sh` sets `ulimit -n 65536`.                                                                                                                                                                                                                                                                                                                                                                   |
| `scripts/build-rust-android.sh`  | Cross-builds with the NDK for any of `arm64-v8a,armeabi-v7a,x86_64,x86` (default: all four). Finds the NDK via `--ndk` / `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` / `$ANDROID_HOME/ndk/*` / `$HOME/AppData/Local/Android/Sdk/ndk/*`; sets `CARGO_TARGET_<TRIPLE>_LINKER`, `CC_`/`AR_`/`RANLIB_<triple>`; stages `dist-rust/android-<abi>/` + `pencarimovie-rust-android-<abi>.tar.gz`. The `android-` infix is load-bearing: `x86_64`/`arm64-v8a` would otherwise clobber the desktop `pencarimovie-rust-<arch>.tar.gz` from `build-rust-linux.sh` (it did — and the `.sha256` sidecar then described the wrong file). |
| `scripts/build-rust-termux.sh`   | Builds **on the device** with Termux's own `rust` + `clang` (`pkg install -y rust clang make`). No NDK, no cross compiler; the host triple _is_ the target. **Untested** (no Termux device here) — see §5.                                                                                                                                                                                                                                                                                                                                                                                                            |
| `server/tools/smoke_boot.mjs` | Boots any built binary **cold** (fresh temp `PM_APP_ROOT` unless one is given), polls `/api/version`, then runs the 16 checks in §1 row 1 and kills the process tree. Node only, no PowerShell, always exits. `node tools/smoke_boot.mjs <binary> [port] [app-root]`.                                                                                                                                                                                                                                                                                                                                                 |
| `server/tools/build-dist.ps1` | The original Windows stager. Still works, unchanged. Prefer the `.bat` (the rules forbid PowerShell for build steps).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `server/Dockerfile`           | Multi-stage container build (repo root as context). `ENTRYPOINT /app/start.sh`, `PORT`/`PM_LISTEN_HOST`/`PM_PAAS` env, `/app/storage` volume, `/api/version` healthcheck. **Untested** — no Docker engine on this machine (`where docker` → not found).                                                                                                                                                                                                                                                                                                                                                               |
| `server/docker-compose.yml`   | One service, `8088:8088`, named volume `pm-rust-storage`, and the optional `PM_WORKERS` / `PM_INFLIGHT` / `PM_MAX_STREAMS` knobs. **Untested** (same reason).                                                                                                                                                                                                                                                                                                                                                                                                                                                         |

`server/dist-rust/` is generated output — do not commit it (`storage/` inside
the checkout holds live secrets and is never copied into an artifact).

## 3. Bind address: desktop vs Android vs PaaS

`--host` > `PM_LISTEN_HOST` > `HOST` > a default chosen by
[`default_bind_host()`](src/main.rs) (`#[cfg(target_os = "android")]` + the PaaS
markers):

| Situation                                                                                                                             | Default bind | Why                                                                                                            |
| ------------------------------------------------------------------------------------------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------- |
| Windows / Linux / macOS desktop                                                                                                       | `127.0.0.1`  | The documented default; `stop.bat` / `stop.sh` and every doc assume a loopback listener. **Do not change it.** |
| Android (Termux, adb, TV box)                                                                                                         | `0.0.0.0`    | A phone/TV server is useless if only the phone can reach it, and there is no host firewall to hide behind.     |
| `DYNO` / `RENDER` / `FLY_APP_NAME` / `RAILWAY_ENVIRONMENT` / `K_SERVICE` / `WEBSITE_SITE_NAME` / `PM_PAAS` / `PM_BIND_PUBLIC` present | `0.0.0.0`    | Every PaaS routes to the container's external interface; a loopback listener fails the health check.           |

`PORT` alone is **not** treated as "public" — the PHP launchers export
`PORT=8088` on a desktop, and that must not silently expose a LAN listener.
Both the rule and the marker list are pinned by unit tests
(`main.rs::tests`), and the startup banner prints `Local:` (a usable URL) plus a
`LAN/TV:` hint when bound to `0.0.0.0`.

Container/PaaS environment contract (same names as
[`docker/start.sh`](docker/start.sh)):

```
PORT=8088              listen port (PaaS sets this)
PM_LISTEN_HOST=0.0.0.0 bind address
PM_APP_ROOT=/app       where storage/ lives
PM_WORKERS=1           Telegram connections (one client each)
PM_INFLIGHT=8          upload.getFile parts in flight per connection
PM_MAX_STREAMS=256     concurrent /api/download bodies (503 + Retry-After past it)
PM_PAAS=1              forces the 0.0.0.0 default if PM_LISTEN_HOST is unset
```

`PM_ASSETS=disk` still reads `public/` from the checkout instead of the embedded
copy (UI iteration). In a container there is no `public/`, so leave it unset.

## 3.1 Where `storage/` lives — `PM_APP_ROOT` is authoritative

`storage_dir()` used to run the PHP-derived heuristic **even when
`PM_APP_ROOT` was set**: prefer a candidate that already holds a session, and
never adopt a directory under the system temp dir. Both rules could redirect an
explicitly rooted server, so it wrote beside its own binary instead of into the
directory it was told to use. Measured before the fix: `PM_APP_ROOT=<fresh temp
dir>` on Windows produced `Storage: <dist>/windows-x86_64/storage`, and on
Android `Storage: /data/local/tmp/pm/storage` instead of
`/data/local/tmp/pm/state/storage`.

Now [`choose_storage()`](src/storage.rs) branches on whether `PM_APP_ROOT` is
set:

| `PM_APP_ROOT` | Behaviour                                                                                                                                                                                       |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| set           | `<PM_APP_ROOT>/storage`, created if missing. **No** session-preference search, **no** temp-dir skip — an explicit root is an operator decision (container volume, Android data dir, PaaS mount) |
| unset         | PHP behaviour: a candidate that already holds a session wins, a FrankenPHP temp extraction dir is never adopted, else `<app root>/storage`                                                      |

Pinned by two unit tests in `storage.rs` (`explicit_app_root_beats_the_exe_dir_and_the_temp_skip`,
`without_an_explicit_root_a_session_candidate_is_preferred`), and every artifact
in §1 now prints the resolved directory in its startup banner.

This is also why `tools/smoke_boot.mjs` can boot a **zero-state** install: it
hands the binary a fresh temp directory and then asserts that the binary —
not the packager — created `storage/` there.

## 4. What is platform-specific in the source

Everything Windows-only is behind `#[cfg]`; there is no `std::os::windows`
outside those blocks.

| File                                   | Split                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`main.rs`](src/main.rs)               | `shutdown_signal()`: `#[cfg(unix)]` = Ctrl-C **or** SIGTERM; `#[cfg(windows)]` = Ctrl-C only (no SIGTERM to intercept). Android is `unix`, so `docker stop` / `kill` shut down gracefully.                                                                                                                                                                                                                                                                                                                                                                                                   |
| [`tunnel.rs`](src/tunnel.rs)           | `bin_name_for()` (`cloudflared.exe` vs `cloudflared`); `metric_ports()` uses `netstat -ano -p tcp` on Windows vs `ss -lntp`/`netstat -lntp` on Unix; `pid_alive()` = `tasklist` vs `/proc/<pid>`; `kill_pid()` = `taskkill /T /F` vs `kill` then `kill -9`; `kill_leftovers()` = metrics-port scan vs `pkill -f <config path>`; `spawn()` = `CREATE_NO_WINDOW \| CREATE_NEW_PROCESS_GROUP` vs `setsid`; `asset_name_for()` = `cloudflared-windows-amd64.exe` / `darwin-*.tgz` / `linux-arm64\|arm\|386\|amd64`. Pure parts are unit-tested for **every** (os, arch) pair, including Android. |
| [`diagnostics.rs`](src/diagnostics.rs) | `device_info()` reads `/proc/meminfo`, `/proc/self/status`, `/proc/self/fd`, `/proc/uptime` and **degrades to `0`/`""`** when a field is unreadable. On Android `model`/`os_version` come from `build.prop` (`ro.product.model`, `ro.build.version.release`, `ro.build.version.sdk`), because `/proc/sys/kernel/hostname` is not readable there. `read_first_line()` is lossy-UTF-8 so one bad byte in a `/proc` file cannot blank the report.                                                                                                                                               |
| [`storage.rs`](src/storage.rs)         | Path handling is `PathBuf`-only; `public_dir()` walks `<exe dir>` ancestors (works in `dist/`, `target/release/`, and `/data/local/tmp`); `is_temp_app_dir()` skips `sys_get_temp_dir()`. `PM_APP_ROOT` overrides — authoritatively, see §3.1.                                                                                                                                                                                                                                                                                                                                               |
| [`assets.rs`](src/assets.rs)           | `include_dir!("$CARGO_MANIFEST_DIR/../../public")` — absolute-path-safe on every target; a missing `public/` is a compile error, which is why both packagers assert it first.                                                                                                                                                                                                                                                                                                                                                                                                                |

### Android caveats (the root `AGENTS.md` list, applied to this port)

1. **It is a plain ELF executable, not a JNI `.so`.** Never
   `System.loadLibrary()`. Run it from a shell (Termux) or from
   `/data/local/tmp` — that directory is `shell_data_file` and _is_ executable;
   an app's private `files/` dir is `app_data_file` and is not.
2. **Termux prefix.** Everything (`clang`, `llvm-ar`, `pkg`) lives in
   `$PREFIX/bin`; `scripts/build-rust-termux.sh` points rustc at
   `$PREFIX/bin/clang` explicitly because Android's system clang has a
   different sysroot. If you build elsewhere and copy the binary in, only the
   binary is needed (statically linked against nothing but bionic).
3. **`TMPDIR`.** `/tmp` is not writable by an app UID. The build script exports
   `TMPDIR=$PREFIX/tmp`; the runtime creates no temp files (storage lives next
   to the binary / `PM_APP_ROOT`), but a wrapper that shells out (e.g. `tar` for
   a Darwin cloudflared) needs it set.
4. **No `/proc` cmdlines.** `/proc/<pid>/cmdline` for other processes is
   invisible to a non-rooted app, so process identity is never derived from it:
   `pid_alive()` uses the cloudflared metrics `/ready` endpoint first and
   `/proc/<pid>` (the directory, not the cmdline) as the fallback. `/proc/self/fd`
   is readable, so the fd count works.
5. **`ulimit -n 65536` before starting.** Android app UIDs default to a **1024**
   soft fd limit; the Telegram part pool plus streaming sockets goes past it and
   the process dies at roughly the 1025th descriptor. Both `start.sh` files do
   `ulimit -n 65536 2>/dev/null || true`.
6. **Linker warning is benign.** Android ≤ 7 prints
   `WARNING: linker: … unsupported flags DT_FLAGS_1=0x8000001` for a `-pie` ELF
   linked with `DF_1_PIE | DF_1_NOW`; the binary runs normally (measured on
   API 25). Do not "fix" it by disabling PIE.
7. **cloudflared**: there is no `-android` asset. `asset_name_for()` returns the
   static Linux build (`cloudflared-linux-arm64` / `-arm` / `-amd64` / `-386`),
   which is what runs under bionic. Measured only as far as "the name/URL is
   right"; a live tunnel from a phone was **not** exercised here.
8. **No service supervision.** Unlike the Kotlin app there is no
   `ForegroundService`; use Termux's `termux-services`/`nohup`,
   `adb … &`, or a `tmux`/`screen` session if the process must outlive the shell.

## 5. Termux on-device build (`scripts/build-rust-termux.sh`) — untested

Provided for a device with no NDK and no PC:

```bash
pkg install -y rust clang make        # the script does this if missing
./scripts/build-rust-termux.sh        # or --skip-build to only stage
```

It exports `CARGO_TARGET_<TRIPLE>_LINKER=$PREFIX/bin/clang`,
`CC_/AR_/RANLIB_<triple>`, `TMPDIR=$PREFIX/tmp`, builds for the device's own
triple, asserts `--version`, and stages `dist-rust/android-<arch>/`. It has **not
been executed here** (no Termux device was available); the cross-built NDK
binary in §1 is the verified path.

## 6. Verification commands (cheap, run after any change)

```bash
# any target, one command, 16 checks, never hangs (Node only)
node server/tools/smoke_boot.mjs server/dist-rust/windows-x86_64/pencarimovie-rust.exe 8095
node server/tools/smoke_boot.mjs server/dist-rust/android-x86/pencarimovie-rust 8096
node server/tools/smoke_boot.mjs ./linux-x86_64/pencarimovie-rust 8097

# Windows (cmd; PowerShell hangs on this box)
scripts\build-rust-windows.bat --skip-build
server\dist-rust\windows-x86_64\pencarimovie-rust.exe --version

# Linux (on the host)
./scripts/build-rust-linux.sh          # writes dist-rust/pencarimovie-rust-<arch>.tar.gz
mkdir -p /tmp/pm-run && tar -xzf server/dist-rust/pencarimovie-rust-*.tar.gz -C /tmp/pm-run
cd /tmp/pm-run/linux-* && PORT=18081 ./start.sh &

# Android (LDPlayer / any adb)
adb push server/dist-rust/android-x86/pencarimovie-rust /data/local/tmp/pm/
adb shell "chmod 755 /data/local/tmp/pm/pencarimovie-rust && /data/local/tmp/pm/pencarimovie-rust --version"
adb shell "cd /data/local/tmp/pm && PM_APP_ROOT=/data/local/tmp/pm/state nohup ./pencarimovie-rust --port 8088 --host 0.0.0.0 >run.log 2>&1 &"
adb forward tcp:18088 tcp:8088 && curl -s http://127.0.0.1:18088/api/version
adb shell "pkill -f 'pencarimovie-rust --port[ ]8088'"
```

Four traps worth keeping:

- **Never let the Android and Linux tarballs share a name.** `build-rust-android.sh`
  and `build-rust-linux.sh` both stage into `dist-rust/`, and the ABI list
  contains `x86_64` — a bare `pencarimovie-rust-x86_64.tar.gz` is the DESKTOP
  artifact. Android names now carry `android-` (`pencarimovie-rust-android-x86_64.tar.gz`);
  before that fix, running the Android build after the Linux one silently
  replaced the Linux tarball (10,915,618 B → 10,891,696 B) and left its
  `.sha256` describing the wrong bytes. Check with
  `sha256sum server/dist-rust/pencarimovie-rust-x86_64.tar.gz` against the
  sidecar after any packaging run.

- **`pkill -f pencarimovie-rust …` over `ssh`/`adb` kills the shell that is
  running it**, because the remote command line contains the pattern. Use a
  bracketed pattern (`--port[ ]8088`) or a pid.
- `adb shell` does not return while the backgrounded server holds the shell's
  stdio, even with `> run.log 2>&1`. That is an adb artifact; the server is
  running (check `ps` / `run.log`), and the `adb forward` must be issued from a
  second shell or it never lands.
- `cargo` is not on `PATH` in a fresh terminal on this machine
  (`%USERPROFILE%\.cargo\bin`). The `.bat` adds it itself; a bare `cargo …`
  fails with `'cargo' is not recognized`.

## 7.1 Diagnosing "the player keeps re-requesting the same file"

Signature in `storage/debug.log` / `/api/logs`: the **same** `download request received`
line for the same `file_name` every 2-15 s, with **no** `download attempt failed`,
no `pre-flight budget exhausted` and no resolve line — the handler never errored, the
CLIENT gave up and asked again. That is a player-side abort, not a server fault, so
never "fix" it by loosening the download path.

Two measurements separate the two possible causes. Run from the repo root, with the
server up on the device:

```bash
node android/pencarimovie-rust/tools/playback_loop_check.mjs tt22084616 DS4K
```

It prints the stream URL the addon hands out, three ranged requests with
status/TTFB/throughput, the **container magic** of the first bytes, and the server
log lines produced while it ran.

- **Container.** `1a45dfa3` = EBML/Matroska (`.mkv`). A browser `<video>` element and
  the WebView player cannot demux Matroska, so they abort on the container and retry
  forever. ExoPlayer plays it. Measured on the phone 2026-10-20: the looped title was
  `Spider.Man.Brand.New.Day.2026.*.mkv`, magic `1a45dfa3a3428681`, HTTP **206**,
  `video/x-matroska`, no server error.
- **Latency.** TTFB 9.1-13.4 s for a 1 MB range (~44-53 KiB/s) under the retry storm.
  The retry cadence matched the TTFB, so the loop was self-sustaining: every retry
  adds another 1 MB pre-flight to the same 8-slot in-flight gate, which makes the next
  first-byte later. Measured on an **mp4** title in the same window: TTFB 9.5 s /
  53 KiB/s — i.e. the slowness was NOT container-specific and could not be attributed
  to the port without removing the concurrent retries first.

`download request received` now also logs `ua` and `range`. PHP has always logged the
UA and that is what attributes a loop to a client: a WebView/browser UA on a `.mkv`
is the container case, an `okhttp`/ExoPlayer UA is not. `range` distinguishes an
initial `bytes=0-` probe from a seek.

## 7.2 Cloudflare Tunnel on Android — four things must all be true

`/api/tunnel/enable` now returns a **working** tunnel on Android (verified in the APK on
the emulator: `ok:1`, `Registered tunnel connection`, and the public
`https://…-tunnel.pencarimovie.com/api/version` answering **200**). Three independent
faults had to be fixed, and each one alone produces the same symptom — a tunnel that
says "enabled" and 530s:

1. **A static cloudflared cannot resolve DNS on Android.** The GitHub
   `cloudflared-linux-*` asset is a CGO-free Go build; it resolves with the pure Go
   resolver, which reads `/etc/resolv.conf` — a file Android does not have for **any**
   user (verified as `shell` _and_ as `root`). It hangs silently at
   `Requesting new quick Tunnel on trycloudflare.com...`. `ensure_cloudflared()` now
   installs **Termux's bionic package** instead (index → `.deb` → `data.tar.xz` →
   `bin/cloudflared`, decompressed in-process with `lzma-rs`, so the device needs no
   `xz`), and skips a non-dynamic ELF left behind by an older version (detected via a
   missing `PT_INTERP`).
2. **The Go runtime has no trust roots on Android** (`/etc/ssl/certs` is absent), which
   makes **edge discovery fatal**: `edge discovery: error looking up Cloudflare edge
IPs` → `Tunnel server stopped` **one second after** the URL is printed. With
   `SSL_CERT_FILE` pointing at a Mozilla bundle the same binary, user and args register
   in ~1 s (`Registered tunnel connection`, 0 SRV errors). `spawn()` sets it from
   `storage/tunnel/cacert.pem`, and `ensure_ca_bundle()` fills that file (release
   `bin/`, else a one-off download from `curl.se`); the APK also ships
   `assets/cacert.pem` and `RustServer.kt` copies it into place at start.
3. **The `setsid` wrapper silently dropped the child's whole environment.** `spawn()`
   applied stdout/stderr redirection and the env vars to `cmd`, then — on any Unix with
   `setsid` (Android has it) — replaced `cmd` with a fresh `Command::new("setsid")`,
   losing all of them. The child ran with no `SSL_CERT_FILE` and no
   `TUNNEL_TRANSPORT_PROTOCOL`. The program and argv are now decided **first**, and
   stdio/env are applied to that one `Command`. **Do not reintroduce a `cmd = wrapped`
   reassignment.**

4. **Android 10+ will not execute a file from app data.** A cloudflared unpacked into
   `storage/tunnel/bin/` dies with `setsid: exec …/bin/cloudflared: Permission denied`
   (SELinux `app_data_file`). Invisible on the Android 7 emulator — it only appeared on
   a real phone (Android 16). `nativeLibraryDir` is the one app-readable path labelled
   `exec_type`, so the binary now ships in `jniLibs/<abi>/libcloudflared.so` (the Termux
   bionic build, the same file the TDLib app uses) and `RustServer.kt` points
   `PM_CLOUDFLARED_BIN` at it. Also add `**/libcloudflared.so` to
   `packagingOptions.jniLibs.keepDebugSymbols`, or AGP's `llvm-strip` corrupts it.
   APK cost: arm64-v8a 21.9 MB → **31.6 MB**.

Also:

- `enable` no longer trusts the URL. A quick tunnel is _published_ before the connector
  connects, so [`wait_for_connector()`](src/tunnel.rs) requires metrics `/ready`
  `status:200` (or a `Registered tunnel connection` log line), fails fast when the
  process dies, and otherwise returns `ok:0` with the log tail instead of a false
  success.
- **`enable` answers immediately.** A cold enable needs 10-90 s, which reads as a hang,
  so [`start_async()`](src/tunnel.rs) runs it in the background and returns
  `{"ok":1,"starting":true}`; `/api/tunnel/status` carries `starting`, and the dashboard
  (which already polls every 1.5 s) finishes when `enabled && tunnel_url` arrive. A
  second call while one is in flight does not spawn a second cloudflared.

Verification: `node server/tools/tunnel_live_check.mjs` (Windows, real tunnel, PASS)
and, on Android, `POST /api/tunnel/enable` then `curl https://<tunnel>/api/version` → 200.
Measured on a real OnePlus (Android 16, arm64): **accepted in 0 s, live in 6 s**,
`PHONE_tunnel_http=200`.

## 7.3 Fast start — the TDLib app's machinery, ported

"It streams slowly / it keeps asking for a new file_id" had two separate causes, and
both are now fixed:

**1. A Play with no `file_id` in the URL resolved on EVERY Range request.** The stream
list is built from metadata only, so the URL carries just the `short_code`; the reuse
of the cached id was gated on `!file_id.is_empty()`, which is false exactly then. Every
seek therefore cost a resolve, and against a busy origin (`/resolve-file` answering
**503**) that stretched the first byte to 11.7 s. `handle_download` now takes the
cached id whenever a `short_code` is present. Measured on a real phone, three
sequential 1 MB ranges of the same URL: **11.30 s → 1.02 s → 2.22 s**, with
`download using the cached file_id (no re-resolve) {"verified":true}` on every one and
zero `/resolve-file` calls.

**A Bot API file_id belongs to ONE bot.** The cache key is `short_code:bot_id`, which
is what keeps bots apart; the id-reuse path additionally rejects an entry whose
`bot_id` names a different bot than the one about to be used, because a foreign id can
only fail later (after a wasted round-trip) with `FILE_REFERENCE_EXPIRED`.

**2. The prefetch machinery the Kotlin app has and the port did not.** Audited
`NativeHttpServer.kt` call site by call site:

| Kotlin                                                             | Ported as                                   | Fires from                            |
| ------------------------------------------------------------------ | ------------------------------------------- | ------------------------------------- |
| `prewarmStreamsBatch` (5378)                                       | [`prewarm_streams_batch()`](src/resolve.rs) | end of `/stream`                      |
| `prefetchPostFiles` (5721, 7503)                                   | [`prefetch_post_files()`](src/routes.rs)    | `/catalog` (movies) + `/meta` (movie) |
| `noteResolveRateLimit` / `resolveCooldownRemainingSec` (3790-3805) | same names                                  | resolve failure + every prefetcher    |
| `warmupResolveBatch` (3862)                                        | `warmup_resolve_batch`                      | already existed                       |
| `subtitleResolveMiss` (3100, 600 s)                                | [`imdb_by_title_cached()`](src/routes.rs)   | subtitle title lookup                 |

**A prewarm must be BOUNDED — learned the hard way.** The first version warmed every
short code of a 366-file title (285 misses). Every one of them answered **HTTP 503**
("server busy resolving file") and the storm starved the user's own Play. Now:
`PREWARM_LIMIT = 12` codes per list, the per-code fallback in
`resolve_shortcodes_batch()` **stops at the first 503**, and every prefetcher skips
while `resolve_cooldown_remaining_sec() > 0` **or** a download is streaming
(`active_streams() > 0`). Same lesson the Kotlin app records: the prefetches themselves
were the load that tripped the origin.

**Browser clients cannot play Matroska.** Chrome rejects `video/x-matroska` inside
`<video>`, so an `.mkv` in a WebView aborts on the container and re-requests the same
file forever — measured: `range: bytes=2848468527-` (the last ~178 KB of a 2.8 GB MKV)
repeating every 4-5 s while the server answered 206 correctly. `wants_web_container()`
now moves Matroska to the END of the list for browser UAs only (native players —
okhttp/ExoPlayer/Stremio/VLC — keep the score order). Nothing is filtered: an MKV-only
title still lists. Log marker: `stream: matroska moved last for a browser client`.

## 7.4 Footprint while downloading (measured)

Real phone (OnePlus, Android 16, arm64, 4G), 64 MB sequential range, sampled every 2 s:

|                                              | idle    | during                     | after   |
| -------------------------------------------- | ------- | -------------------------- | ------- |
| server RSS (`/api/diagnostics` **and** `ps`) | 27 MB   | **25-28 MB (peak 28)**     | 26 MB   |
| server open FDs                              | 23      | 24 (one 29 spike)          | 19      |
| app process RSS (`com.pencarimovie.rust`)    | 158 MB  | 158-159 MB                 | 164 MB  |
| system free RAM                              | 2606 MB | 2561-2596 MB               | 2883 MB |
| transfer                                     | —       | 64 MB in 172 s (381 KiB/s) | —       |

**Flat.** RSS does not grow with bytes transferred: the body is streamed through an
8-deep 1 MB pipeline, so the working set is bounded (~28 MB) regardless of a 2.8 GB
source. FDs return _below_ the idle value because idle sockets closed. The 158 MB is the
Kotlin app (WebView + service), not the engine; the 4G link, not memory, is the limit.

Disk: on a desktop run of the same transfer (`cache_footprint_check.mjs`, 64 MB at
3878 KiB/s) `storage/cache` grew **7 KiB** — only `wp_ajax_*.json` and
`resolve_cache_*.json` metadata. **Media bytes are never cached to disk**; the only
disk-writing playback path is ALAC, which writes `storage/cache/audio/<code>.flac` and
is pruned by its own TTL. Reproduce with
`node server/tools/cache_footprint_check.mjs` (throws; provisions a guest bot,
streams 64 MB, asserts the cache did not grow by more than 1/8 of the transfer).

## 7.5 The Android app replaces the TDLib app in place

`android/pencarimovie-rust` now ships with **`applicationId com.pencarimovie.server`**
— the same identity as the TDLib app it replaces — so an existing install upgrades in
place and the user migrates instantly: same launcher entry, same data dir, no
uninstall, and any external shortcut / TV launcher intent that holds
`com.pencarimovie.server/.ServerService` keeps resolving.

Three things make that work and each one is load-bearing:

| Requirement                               | Where                                                                               | Why                                                                                                                                     |
| ----------------------------------------- | ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Same signing key                          | `app/release-key.jks` (byte-identical to the TDLib app's, SHA-256 cert `0e5d7935…`) | Android refuses an update signed by a different key                                                                                     |
| `versionCode` continues the TDLib lineage | `gradle.properties` → `versionCodeOverride=10049` (TDLib shipped 10048)             | A lower `versionCode` is rejected as a downgrade                                                                                        |
| Same channel id + label                   | `CHANNEL_ID = "pencarimovie-server"`, `app_name = PencariMovie Server`              | An in-place update keeps the notification channel the user configured; the engine is identified by the version string, not the app name |

Verified on LDPlayer (`emulator-5554`, Android 7.1.2) over the TDLib app at
1.0.10/10010: `adb install -r` returned **Success** (no
`INSTALL_FAILED_UPDATE_INCOMPATIBLE`), `dumpsys package` showed
`versionName=1.0.49 versionCode=10049` with **`firstInstallTime` unchanged**
(`2026-10-09 21:03:37`) and `lastUpdateTime` bumped — i.e. a real in-place update —
and the Settings overlay still listed the bot pool from the previous install.

### Do not skip `build.rs`: `include_dir!` + preserved timestamps = a stale dashboard

`src/assets.rs` embeds the dashboard with
`include_dir!("$CARGO_MANIFEST_DIR/../../public")`. Cargo decides whether the crate
needs rebuilding from file **mtimes**, so a dashboard copied in with preserved
timestamps (`robocopy /E`, `unzip`, `git checkout`, a release archive) looks _older_
than the last build and cargo reports the crate up to date — the old embed is reused
silently. Observed: `public/` was refreshed from the Android app's ES5-lowered
`assets/web`, the APK was rebuilt, and it still served the **desktop** `app.js`; on
Android 5 the WebView threw `SyntaxError: Unexpected identifier` and `#settings` /
`#addon` could never open (the boot guard showed "Nuvio TV could not start").

[`build.rs`](build.rs) fixes it on two rails: `cargo:rerun-if-changed` for the
directory and every file, plus a FNV-1a fingerprint of `(path, size, mtime)` exported
as `cargo:rustc-env=PM_PUBLIC_FINGERPRINT`, which cargo folds into the crate
fingerprint — so even a timestamp-preserving copy forces a rebuild. Measured:
arm64 binary **35,242,040 → 37,987,928 B** (+2.75 MB, the larger ES5 bundle), and the
served tree then matched `public/` exactly (`/` 63,558 B, `/app.js` 250,363 B,
`/app.bundle.js` 4,377,006 B) with both overlays rendering on the WebView.

**Consequence for "why is the dashboard old?": check the embed, not the routes.** The
engine never reads `public/` at runtime unless `PM_ASSETS=disk`; it serves what was
compiled in.

## 7.6 The cache: memory L1 + one redb file (replaces one JSON file per key)

Everything that used to be `storage/cache/<prefix>_<md5>.json` now goes through
[`cache.rs`](src/cache.rs): a sharded in-process map (L1) over a single
`storage/cache/cache.redb` (L2), with TTLs, a stale window, and single-flight.
Key prefixes and their TTLs are unchanged in spirit — `resolve` 2 h,
`resolve_fresh` 10 s, `wp_json`/`wp_ajax` per action + a 6 h stale grace,
`manifest`/`up_cat`/`up_meta` 1–2 h, `episode_names` 7 d/1 h/5 min, `catalog` 10 min,
`stream` 300 s, `version` 1 h, `post_prefetch` 300 s, `title_imdb_miss` 600 s,
`resolve_miss` 60 s.

### Measured (same box, release build)

| operation                                  | cost                                                     |
| ------------------------------------------ | -------------------------------------------------------- |
| old: JSON file hit (`stat`+`read`+`serde`) | **182–224 us**                                           |
| old: `fs::write` one entry                 | **763 us**                                               |
| L1 hit                                     | **1.08 us** (`Value` clone) / **0.040 us** (`Arc`)       |
| redb get (fresh read txn)                  | **2.75 us**, +`serde_json` **6.17 us**                   |
| redb commit                                | `Immediate` 1 697 us, `Eventual` 1 761 us, `None` 165 us |

End-to-end ([`tools/cache_stress.mjs`](tools/cache_stress.mjs), 16 workers, same 5
targets): **56.8 rps → 624 rps**, and p50 dropped from **430–547 ms to 20–27 ms**
per endpoint, 0 × 5xx.

### Why writes are BUFFERED

A redb commit is a fsync (~1.7 ms) — committing per entry would be **slower** than the
JSON file it replaced. So `put()` writes L1 + a pending map and a 400 ms flusher
commits the batch in one transaction. `Durability::None` is tempting (165 us) but is
unusable: redb's own docs say such commits are "not persisted to disk unless followed
by a commit with a higher durability level" and "pages are only freed during commits
with higher durability levels… will result in rapid growth of the database file" —
and an app kill is the NORMAL lifecycle on Android. Verified: after `taskkill /F` and
a restart, `/api/cache/stats` reported **rows=59** on the fresh process (`l2_hits=1`).

### Three traps this work found

1. **`redb` needs `mut wtx`, and `commit(self)`/`abort(self)` both CONSUME it** — only
   one may appear per path, and calling `abort()` inside a `match wtx.open_table(..)`
   arm is a borrow error (`E0505`). Dropping the transaction aborts it.
2. **The per-code prewarm had no negative cache.** Every `/stream` re-attempted the
   same 12 codes the origin had already refused (~1 700 useless resolve attempts and
   ~4 400 L1 misses in one 12 s pass). `resolve_miss` (60 s) now suppresses them, and
   a success clears the marker.
3. **`/stream/*` and `/catalog/*` were never response-cached**, so a warm list paid the
   full origin + upstream + per-post work every time (430–547 ms). They are now cached
   for 300 s / 600 s, keyed with the item TYPE and the base url (every download url in
   a stream payload is absolute). The settings save clears the `stream` prefix so new
   AIOStreams filters apply immediately.

`/api/cache/stats` and `/api/cache/clear[?prefix=]` (both privileged) replace the
inspectability of a folder of JSON files.

## 7.7 Android self-update — one feed, one name

The app has **no "rust" identity anywhere the user can see**: `applicationId`,
label, notification channel, feed repo, feed tag and APK asset names all say
`pencarimovie-server`. That is what makes the migration free — an existing install
updates into this build, and this build updates from the same feed.

|                    | value                                                                                              |
| ------------------ | -------------------------------------------------------------------------------------------------- |
| feed               | `https://github.com/aiskendi/pencarimovie-server/releases/download/apk-server-latest/version.json` |
| assets             | `pencarimovie-server_release_<abi>.apk` + `.sha256`                                                |
| provider authority | `com.pencarimovie.server.updates` (FileProvider, `cacheDir/updates/`)                              |
| permission         | `REQUEST_INSTALL_PACKAGES` (API 26+; ignored below)                                                |

The previous updater **could not update the app**: it only opened
`/api/version`'s `update_url` — the WordPress landing page (`http://telegra.my`) —
and compared the ENGINE version (2.7.9) with the WordPress minimum, never this APK's
`versionCode`. [`UpdateChecker.kt`](../android/pencarimovie-rust/app/src/main/java/com/pencarimovie/server/UpdateChecker.kt)
restores the real thing: read `version.json`, refuse a feed whose `applicationId` is
not our own package, pick `apks[<abi>]` (then `universal`), download, verify the
published sha256, and hand the file to the package installer via the FileProvider.
The automatic check is throttled to once per 10 min (`onStateChanged(Running)` fires
on every transition and was fetching the feed three times per launch).

### Verified end to end on the emulator (Android 5.1, x86)

1. install `1.0.49` (10049); serve a local feed with `1.0.50` (10050) via
   [`tools/update_feed_server.mjs`](tools/update_feed_server.mjs) and
   `adb reverse tcp:8099 tcp:8099`;
2. `am start -n com.pencarimovie.server/.MainActivity -e update_feed http://127.0.0.1:8099/version.json`
   → banner `Update available: v1.0.50 (10050) (installed v1.0.49 (10049))`;
3. tap **DOWNLOAD UPDATE** → feed log `200 ...apk (33984330 bytes)`, sha256 verified;
4. system installer: _"Do you want to install an update to this existing
   application? Your existing data will not be lost."_ → INSTALL;
5. `dumpsys package` → **versionCode 10050 / versionName 1.0.50**, `lastUpdateTime`
   bumped, `firstInstallTime` unchanged.
6. Reinstalled 1.0.50 against the LIVE GitHub feed (which currently publishes
   1.0.48): no banner — the "up to date" path is quiet, not a false positive.

`-e update_feed <url>` is the documented test hook (it persists in
`pencarimovie_server` prefs, so clear it — or `pm clear`-free equivalent — after
testing).

## 7. Parity

Cross-platform packaging changes no behaviour: the JSON shapes, the matcher and
the route table are untouched. The only observable additions are `--version`, the
`PM_LISTEN_HOST`/`HOST` override, the Android/PaaS bind default, Android
`build.prop` fields in `/api/diagnostics`, and the `PM_APP_ROOT` storage
override (§3.1). Test count went **93 → 108** (5 bind/version, 4 tunnel
platform, 5 diagnostics `build.prop`, 1 storage-root override pair).
