#!/bin/bash
# Decisive test: is the file_id WordPress hands a fresh bot actually valid FOR
# THAT BOT? Provision a bot, resolve a short_code for it, then ask the Telegram
# Bot API getFile. Prints only ok/error_code — never the token or the id.
#   bash probe_botapi.sh [short_code]
set -u
SC=${1:-1v9OPj}
APP="$HOME/pm-rust"
S=$(cat "$APP/storage/api_secret.key")
BASE=https://pencarimovie.com/wp-json/pencarimovie-server/v1
T=$(date +%s)

curl -s -m 60 "$BASE/provision-session?_nocache=${T}_77" > /tmp/prov.json
TOKEN=$(python3 -c 'import json;print(json.load(open("/tmp/prov.json")).get("bot_token") or "")')
BOT=$(python3 -c 'import json;print(json.load(open("/tmp/prov.json")).get("bot_id") or "")')
echo "provisioned bot_id=$BOT token_len=${#TOKEN}"
if [ -z "$TOKEN" ]; then echo "no token"; exit 1; fi

sleep 3
curl -s -m 120 -H "X-API-Secret: $S" \
  "$BASE/resolve-file?short_code=$SC&bot_id=$BOT&force=1&nocache=1&_t=$T" > /tmp/res.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/res.json'))
print("resolve:", {k: d.get(k) for k in ("ok", "bot_id", "message")})
print("has_file_id:", bool(d.get("file_id_mt")), "len:", len(d.get("file_id_mt") or ""))
PY
ID=$(python3 -c 'import json;print(json.load(open("/tmp/res.json")).get("file_id_mt") or "")')
if [ -z "$ID" ]; then echo "resolve returned no id"; exit 1; fi

echo "--- Bot API getFile with THAT bot's token ---"
curl -s -m 60 "https://api.telegram.org/bot${TOKEN}/getFile?file_id=${ID}" > /tmp/botapi.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/botapi.json'))
print("ok:", d.get("ok"), "error_code:", d.get("error_code"),
      "description:", (d.get("description") or "")[:120],
      "file_path_present:", bool((d.get("result") or {}).get("file_path")))
PY
