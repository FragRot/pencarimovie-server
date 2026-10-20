#!/bin/bash
# Start the Rust port on the build host, detached, and print the banner.
#   PORT=8088 WORKERS=1 bash start-vps.sh [extra args...]
set -u
PORT="${PORT:-8088}"
WORKERS="${WORKERS:-1}"
APP="$HOME/pm-rust"
cd "$APP" || exit 1
pkill -f "pencarimovie-rust --port $PORT" 2>/dev/null
sleep 1
setsid nohup "$APP/target/release/pencarimovie-rust" --port "$PORT" --workers "$WORKERS" "$@" \
  >"$APP/server-$PORT.log" 2>&1 < /dev/null &
sleep 3
echo "--- log ---"
cat "$APP/server-$PORT.log"
echo "--- listen ---"
ss -ltnp 2>/dev/null | grep ":$PORT" || echo "not listening"
