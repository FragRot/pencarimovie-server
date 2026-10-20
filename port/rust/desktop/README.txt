PencariMovie Server
===================

IT IS ONE BINARY. `pencarimovie-server` (or `pencarimovie-server.exe`) contains
the whole server: the dashboard, the Stremio/Eclipse addon, the Telegram engine
and the caches. Run it and it works:

    ./pencarimovie-server --port 8088        # http://127.0.0.1:8088/
    pencarimovie-server.exe --port 8088

No PHP, no Composer, no vendor/, no public/ folder. `storage/` next to the binary
holds sessions, auth.json and caches - KEEP IT between upgrades.

Everything else in this folder is an OPTIONAL convenience so the server can be
started without a console and without remembering a command:

  Windows
    start.bat          console launcher (you see the log)
    start-tray.bat     starts hidden + tray icon (double-click = dashboard)
    tray.ps1           the tray itself (open pages, copy addon URL, restart,
                       stop, open log, run-at-login)
    autostart-on.bat   start at login (HKCU Run, no admin rights)
    autostart-off.bat  remove that entry
    pencarimovie.ico   the logo used by the tray

  Linux
    start.sh           console launcher (--open also opens the browser)
    start-tray.sh      tray icon via `yad` if installed, else background + URL
    autostart-on.sh    install ~/.config/autostart/pencarimovie-server.desktop
    autostart-off.sh   remove it
    pencarimovie.png   the logo used by the icon / autostart entry

  macOS
    start.command      double-click launcher (opens the dashboard)
    autostart-on.sh    LaunchAgent (no window, starts at login)
    autostart-off.sh   remove it
    pencarimovie.png   logo for the icon

  (macOS has no shell-level menu-bar API, so there is no tray there without
   shipping a compiled .app bundle. The LaunchAgent covers "always running".)

First run
  1. start the server (any way above)
  2. open http://127.0.0.1:8088/            dashboard
  3. install the add-on from http://127.0.0.1:8088/manifest.json
     (or open http://127.0.0.1:8088/#addon and use the buttons)

LAN / TV access
  bind to all interfaces:  ./pencarimovie-server --host 0.0.0.0
  the dashboard footer / /api/lan-ip show the LAN URL to install on a TV box.

Flags
  --port N         listen port (or env PORT)
  --host ADDR      bind address (or env HOST / PM_LISTEN_HOST)
  --workers N      worker threads
  --inflight N     concurrent Telegram parts per stream (default 8)
  --max-streams N  concurrent streaming responses (default 256)
  --version        build identity
