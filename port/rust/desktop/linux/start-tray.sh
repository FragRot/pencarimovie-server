#!/bin/sh
# PencariMovie Server - tray launcher (Linux).
#
# Uses `yad --notification` for the icon when yad is installed
#   Debian/Ubuntu: sudo apt install yad
#   Fedora:        sudo dnf install yad
#   Arch:          sudo pacman -S yad
# Without yad the server still starts (detached, log in storage/server.log) and
# the dashboard URL is printed / shown via notify-send. A desktop tray icon needs
# a GTK tray implementation, so it is optional by design rather than a hard
# dependency of the server.
set -eu
cd "$(dirname "$0")"
PORT="${PORT:-8088}"
BIN="./pencarimovie-server"
LOG="./storage/server.log"
URL="http://127.0.0.1:$PORT/"
mkdir -p ./storage
[ -x "$BIN" ] || { echo "pencarimovie-server not found next to this script" >&2; exit 1; }

ulimit -n 65536 2>/dev/null || true

start_server() {
  if curl -fsS -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/api/version" 2>/dev/null; then
    return 0
  fi
  nohup "$BIN" --port "$PORT" >>"$LOG" 2>&1 &
  echo $! > ./storage/server.pid
  # wait for the port, at most ~10s
  i=0
  while [ $i -lt 50 ]; do
    if curl -fsS -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/api/version" 2>/dev/null; then
      return 0
    fi
    i=$((i + 1))
    sleep 0.2
  done
  return 1
}

stop_server() {
  if [ -f ./storage/server.pid ]; then
    kill "$(cat ./storage/server.pid)" 2>/dev/null || true
    rm -f ./storage/server.pid
  fi
  pkill -f "pencarimovie-server --port $PORT" 2>/dev/null || true
}

if ! start_server; then
  echo "server did not answer on $URL - see $LOG" >&2
fi

if command -v yad >/dev/null 2>&1; then
  ICON="$(pwd)/pencarimovie.png"
  [ -f "$ICON" ] || ICON=utilities-terminal
  exec yad --notification --image="$ICON" --text="PencariMovie Server on port $PORT" \
    --command="xdg-open '$URL' 2>/dev/null || true" \
    --menu="Open dashboard!xdg-open $URL 2>/dev/null || true|\
Copy add-on URL!sh -c 'printf %s http://127.0.0.1:$PORT/manifest.json | xclip -selection clipboard 2>/dev/null || true'|\
Restart server!sh -c 'pkill -f \"pencarimovie-server --port $PORT\"; sleep 1; nohup $(pwd)/pencarimovie-server --port $PORT >>$(pwd)/storage/server.log 2>&1 &'|\
Stop server!sh -c 'pkill -f \"pencarimovie-server --port $PORT\"'|\
Open log!xdg-open $(pwd)/storage/server.log|Quit!quit"
fi

echo "PencariMovie Server is running in the background."
echo "  dashboard : $URL"
echo "  log       : $LOG"
echo "  stop      : ./autostart-off.sh does not stop it - use: pkill -f pencarimovie-server"
if command -v notify-send >/dev/null 2>&1; then
  notify-send "PencariMovie Server" "Running on $URL (install yad for a tray icon)"
fi
