#!/bin/bash
# Same bot, same file_id: Bot API download vs MTProto (grammers) download.
# The Bot API path proves whether the id is genuinely valid for the bot; if it
# downloads and the port 410s, the bug is in the port's MTProto call.
#
#   FIND: pick a <20 MB music file (Bot API getFile caps at 20 MB)
#   then: provision -> /api/botlogin (port active bot = same bot) -> resolve
#         -> api.telegram.org getFile + download  -> our /api/download
set -u
APP="$HOME/pm-rust"
PORT=8088
S=$(cat "$APP/storage/api_secret.key")
BASE=https://pencarimovie.com/wp-json/pencarimovie-server/v1
T=$(date +%s)

echo "=== find a small file ==="
curl -s -m 60 "http://127.0.0.1:8088/eclipse/search?query=love" > /tmp/ecl.json
python3 - <<'PY' > /tmp/ecl_ids.txt
import json
d = json.load(open('/tmp/ecl.json'))
for t in (d.get('tracks') or [])[:25]:
    if t.get('id'):
        print(t['id'])
PY
SC=""
for c in $(cat /tmp/ecl_ids.txt); do
  curl -s -m 60 -H "X-API-Secret: $S" "$BASE/resolve-file?short_code=$c&bot_id=7841750221&_t=$T" > /tmp/r.json
  size=$(python3 -c 'import json;d=json.load(open("/tmp/r.json"));print(int(d.get("file_size") or 0))')
  echo "  $c size=$size"
  if [ "$size" -gt 1000000 ] && [ "$size" -lt 19000000 ]; then SC=$c; SZ=$size; break; fi
done
if [ -z "$SC" ]; then echo "no small file found"; exit 1; fi
echo "picked $SC size=$SZ"

echo "=== provision a fresh bot ==="
curl -s -m 60 "$BASE/provision-session?_nocache=${T}_91" > /tmp/prov2.json
TOKEN=$(python3 -c 'import json;print(json.load(open("/tmp/prov2.json")).get("bot_token") or "")')
BOT=$(python3 -c 'import json;print(json.load(open("/tmp/prov2.json")).get("bot_id") or "")')
echo "bot=$BOT token_len=${#TOKEN}"
[ -z "$TOKEN" ] && { echo "no token"; exit 1; }

echo "=== make the port log in as THAT bot ==="
python3 - "$TOKEN" <<'PY' > /tmp/login_body.json
import json, sys
print(json.dumps({"bot_token": sys.argv[1]}))
PY
curl -s -m 180 -X POST -H 'Content-Type: application/json' --data @/tmp/login_body.json \
  "http://127.0.0.1:$PORT/api/botlogin" | python3 -c 'import sys,json;d=json.load(sys.stdin);print("botlogin ok:",d.get("ok"),"bot_id:",d.get("bot_id"),d.get("message") or "")'

echo "=== resolve for that bot ==="
T2=$(date +%s)
curl -s -m 150 -H "X-API-Secret: $S" \
  "$BASE/resolve-file?short_code=$SC&bot_id=$BOT&force=1&nocache=1&_t=$T2" > /tmp/r2.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/r2.json'))
print("resolve:", {k: d.get(k) for k in ("ok", "file_size", "file_type", "mime", "message")})
open('/tmp/id4.txt','w').write(d.get('file_id_mt') or '')
PY
ID=$(cat /tmp/id4.txt)
echo "id_len=${#ID}"

echo "=== Bot API getFile ==="
curl -s -m 60 "https://api.telegram.org/bot${TOKEN}/getFile?file_id=${ID}" > /tmp/ba2.json
FP=$(python3 -c 'import json;d=json.load(open("/tmp/ba2.json"));print((d.get("result") or {}).get("file_path") or "")')
python3 -c 'import json;d=json.load(open("/tmp/ba2.json"));print("ok:",d.get("ok"),"err:",d.get("error_code"),(d.get("description") or "")[:100])'

if [ -n "$FP" ]; then
  echo "=== Bot API download (first 256 KB) ==="
  curl -s -m 120 -r 0-262143 -o /tmp/ba_body.bin "https://api.telegram.org/file/bot${TOKEN}/${FP}"
  echo "botapi bytes=$(stat -c %s /tmp/ba_body.bin 2>/dev/null)"
  xxd -l 16 /tmp/ba_body.bin 2>/dev/null
fi

echo "=== MTProto (grammers) via the port ==="
curl -s -m 180 -D /tmp/mt_h.txt -o /tmp/mt.bin -H 'Range: bytes=0-262143' \
  "http://127.0.0.1:$PORT/api/download?short_code=$SC"
head -1 /tmp/mt_h.txt
grep -i -E 'content-range|^content-type' /tmp/mt_h.txt | tr -d '\r'
echo "mtproto bytes=$(stat -c %s /tmp/mt.bin 2>/dev/null)"
xxd -l 16 /tmp/mt.bin 2>/dev/null
echo "=== log ==="
tail -6 "$APP/server-$PORT.log"
