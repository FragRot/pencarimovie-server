#!/bin/bash
# Compare the per-code resolve with the BATCH resolve for the SAME bot and try to
# download with whatever the batch minted.
#   bash probe_resolve.sh [short_code]
set -u
SC=${1:-1v9OPj}
APP="$HOME/pm-rust"
BOT=$(python3 -c 'import json;print(json.load(open("'"$APP"'/storage/session_meta.json"))["bot_id"])')
S=$(cat "$APP/storage/api_secret.key")
BASE=https://pencarimovie.com/wp-json/pencarimovie-server/v1
T=$(date +%s)
echo "bot=$BOT short_code=$SC"

echo "--- single /resolve-file (force=1&nocache=1) ---"
curl -s -m 150 -H "X-API-Secret: $S" \
  "$BASE/resolve-file?short_code=$SC&bot_id=$BOT&force=1&nocache=1&_t=$T" > /tmp/single.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/single.json'))
print({k: d.get(k) for k in ("ok", "file_id_mt", "bot_id", "message")})
PY

echo "--- batch /resolve-files ---"
curl -s -m 180 -H "X-API-Secret: $S" \
  "$BASE/resolve-files?short_codes=$SC&bot_id=$BOT&_t=$T" > /tmp/batch.json
head -c 800 /tmp/batch.json; echo

echo "--- compare ---"
python3 - "$SC" <<'PY'
import json, sys
sc = sys.argv[1]
single = json.load(open('/tmp/single.json'))
batch = json.load(open('/tmp/batch.json'))
row = (batch.get('results') or {}).get(sc) or {}
sid = single.get('file_id_mt') or ''
bid = row.get('file_id_mt') or ''
print("single_id =", sid[:48], "...")
print("batch_id  =", bid[:48], "...")
print("SAME" if sid and sid == bid else "DIFFERENT")
open('/tmp/ids.txt','w').write(f"{sc}\n{sid}\n{bid}\n{single.get('file_size') or row.get('file_size') or 0}\n{single.get('title') or ''}\n{single.get('mime') or ''}\n")
PY

echo "--- decode both with the Rust decoder ---"
SC2=$(sed -n 1p /tmp/ids.txt)
SID=$(sed -n 2p /tmp/ids.txt)
BID=$(sed -n 3p /tmp/ids.txt)
[ -n "$SID" ] && echo "single:" && "$APP/target/release/pencarimovie-rust" --decode "$SID" | head -12
[ -n "$BID" ] && echo "batch:" && "$APP/target/release/pencarimovie-rust" --decode "$BID" | head -12

echo "--- try a Range download with the BATCH id (no short_code) ---"
SIZE=$(sed -n 4p /tmp/ids.txt)
TITLE=$(sed -n 5p /tmp/ids.txt)
MIME=$(sed -n 6p /tmp/ids.txt)
PAYLOAD=$(python3 - "$BID" "$SIZE" "$TITLE" "$MIME" <<'PY'
import base64, json, sys
fid, size, title, mime = sys.argv[1:5]
p = {"file_id_mt": fid, "file_size": int(size or 0),
     "file_name": title or "probe.mkv", "mime": mime or "video/x-matroska"}
print(base64.urlsafe_b64encode(json.dumps(p).encode()).decode().rstrip("="))
PY
)
curl -s -m 180 -D /tmp/probe_h.txt -o /tmp/probe.bin \
  -H 'Range: bytes=0-262143' \
  "http://127.0.0.1:8088/api/download?d=$PAYLOAD"
head -1 /tmp/probe_h.txt
grep -i -E 'content-range|^content-type|^content-length' /tmp/probe_h.txt | tr -d '\r'
echo "bytes=$(stat -c %s /tmp/probe.bin 2>/dev/null)"
xxd -l 16 /tmp/probe.bin 2>/dev/null
