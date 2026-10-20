#!/bin/bash
# Isolate the MTProto path with a SMALL file (the Bot API getFile 20 MB limit
# hides whether a big id is valid; a small id can be checked both ways).
#   bash probe_small.sh [search-term]
set -u
Q=${1:-music}
APP="$HOME/pm-rust"
S=$(cat "$APP/storage/api_secret.key")
BASE=https://pencarimovie.com/wp-json/pencarimovie-server/v1
BOT=$(python3 -c 'import json;print(json.load(open("'"$APP"'/storage/session_meta.json"))["bot_id"])')
echo "existing bot=$BOT"

echo "--- search_files via the port ---"
curl -s -m 60 "http://127.0.0.1:8088/api/proxy-stream?action=search_files&search=$Q&limit=40" > /tmp/sf.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/sf.json'))
data = d.get('data') if isinstance(d, dict) else None
items = (data or {}).get('files') or (data or {}).get('items') or []
if not items and isinstance(data, list):
    items = data
small = []
for it in items:
    sc = it.get('short_code') or it.get('shortcode') or ''
    size = it.get('file_size') or it.get('size') or 0
    try: size = int(size)
    except Exception: size = 0
    if sc:
        small.append((size, sc, (it.get('title') or it.get('file_name') or '')[:60]))
small.sort()
print("candidates:", len(small))
for s in small[:8]:
    print(f"  {s[0]:>12}  {s[1]}  {s[2]}")
open('/tmp/cands.txt','w').write("\n".join(f"{s[1]}\t{s[0]}" for s in small))
PY

SC=$(awk -F'\t' 'NF && $2>0 && $2<15000000 {print $1; exit}' /tmp/cands.txt)
if [ -z "${SC:-}" ]; then SC=$(awk -F'\t' 'NF{print $1; exit}' /tmp/cands.txt); fi
echo "picked short_code=$SC"
if [ -z "${SC:-}" ]; then echo "no candidate"; exit 1; fi

T=$(date +%s)
curl -s -m 120 -H "X-API-Secret: $S" \
  "$BASE/resolve-file?short_code=$SC&bot_id=$BOT&force=1&nocache=1&_t=$T" > /tmp/res2.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/res2.json'))
print("resolve:", {k: d.get(k) for k in ("ok", "file_size", "file_type", "mime", "message")})
open('/tmp/id2.txt','w').write(d.get('file_id_mt') or '')
PY
ID=$(cat /tmp/id2.txt)
echo "id_len=${#ID}"

echo "--- grammers range download through the port (256 KB) ---"
SIZE=$(python3 -c 'import json;print(json.load(open("/tmp/res2.json")).get("file_size") or 0)')
TITLE=$(python3 -c 'import json;print((json.load(open("/tmp/res2.json")).get("title") or "probe.mp3").replace("/","_"))')
MIME=$(python3 -c 'import json;print(json.load(open("/tmp/res2.json")).get("mime") or "audio/mpeg")')
PAYLOAD=$(python3 - "$ID" "$SIZE" "$TITLE" "$MIME" <<'PY'
import base64, json, sys
fid, size, title, mime = sys.argv[1:5]
p = {"file_id_mt": fid, "file_size": int(size or 0), "file_name": title, "mime": mime}
print(base64.urlsafe_b64encode(json.dumps(p).encode()).decode().rstrip("="))
PY
)
curl -s -m 180 -D /tmp/small_h.txt -o /tmp/small.bin -H 'Range: bytes=0-262143' \
  "http://127.0.0.1:8088/api/download?d=$PAYLOAD"
head -1 /tmp/small_h.txt
grep -i -E 'content-range|^content-type' /tmp/small_h.txt | tr -d '\r'
echo "bytes=$(stat -c %s /tmp/small.bin 2>/dev/null)"
xxd -l 16 /tmp/small.bin 2>/dev/null
