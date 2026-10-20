he#!/bin/bash
# Resolve a SMALL music short code with the port's active bot and try a Range
# download through the Rust port. Small files avoid the Bot API 20 MB wall and
# make the failure mode unambiguous.
#   bash probe_small2.sh BB0p2e
set -u
SC=${1:-BB0p2e}
APP="$HOME/pm-rust"
S=$(cat "$APP/storage/api_secret.key")
BASE=https://pencarimovie.com/wp-json/pencarimovie-server/v1
BOT=$(python3 -c 'import json;print(json.load(open("'"$APP"'/storage/session_meta.json"))["bot_id"])')
T=$(date +%s)
echo "bot=$BOT short_code=$SC"

curl -s -m 120 -H "X-API-Secret: $S" \
  "$BASE/resolve-file?short_code=$SC&bot_id=$BOT&force=1&nocache=1&_t=$T" > /tmp/res3.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/res3.json'))
print("resolve:", {k: d.get(k) for k in ("ok", "file_size", "file_type", "mime", "message")})
open('/tmp/id3.txt','w').write(d.get('file_id_mt') or '')
PY
ID=$(cat /tmp/id3.txt)
echo "id_len=${#ID}"
[ -z "$ID" ] && { echo "no id"; exit 1; }

echo "--- decode ---"
"$APP/target/release/pencarimovie-rust" --decode "$ID" | head -14

echo "--- grammers 256 KB range via the port (with short_code so the retry ladder runs) ---"
curl -s -m 180 -D /tmp/s2_h.txt -o /tmp/s2.bin -H 'Range: bytes=0-262143' \
  "http://127.0.0.1:8088/api/download?short_code=$SC"
head -1 /tmp/s2_h.txt
grep -i -E 'content-range|^content-type' /tmp/s2_h.txt | tr -d '\r'
echo "bytes=$(stat -c %s /tmp/s2.bin 2>/dev/null)"
xxd -l 16 /tmp/s2.bin 2>/dev/null
echo "--- server log tail ---"
tail -8 "$APP/server-8088.log"
