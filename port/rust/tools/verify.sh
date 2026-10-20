#!/bin/bash
# One-shot verification of the Rust port on the build host.
#   bash verify.sh [short_code]
set -u
SC=${1:-1v9OPj}
APP="$HOME/pm-rust"
B=http://127.0.0.1:8088
BIN="$APP/target/release/pencarimovie-rust"

echo "== binary =="
echo "bytes=$(stat -c %s "$BIN")  mtime=$(stat -c %y "$BIN")"

echo "== /api/version =="
curl -s -m 30 -w '  HTTP %{http_code} %{time_total}s\n' "$B/api/version"; echo

echo "== /api/session =="
curl -s -m 30 "$B/api/session" | python3 -c 'import sys,json;d=json.load(sys.stdin);print({k:d.get(k) for k in ("ok","has_session","bot_id","bot_username","bot_count")})'

echo "== /api/provision =="
curl -s -m 180 -o /tmp/v_prov.json -w '  HTTP %{http_code} %{time_total}s\n' "$B/api/provision"
python3 -c 'import json;d=json.load(open("/tmp/v_prov.json"));print({k:d.get(k) for k in ("ok","bot_id","bot_username")})' 2>/dev/null || head -c 200 /tmp/v_prov.json
echo

echo "== /api/resolve-shortcode?short_code=$SC =="
curl -s -m 120 "$B/api/resolve-shortcode?short_code=$SC" > /tmp/v_res.json
python3 - <<'PY'
import json
d = json.load(open('/tmp/v_res.json'))
print({k: d.get(k) for k in ("ok", "bot_id", "file_size", "mime", "title")})
open('/tmp/v_id.txt','w').write(d.get('file_id_mt') or '')
PY
ID=$(cat /tmp/v_id.txt)
echo "== decoder =="
"$BIN" --decode "$ID" | python3 -c 'import sys,json;d=json.load(sys.stdin);print({k:d.get(k) for k in ("id","access_hash","dc_id","type_name","file_reference_len")})'

echo "== Range 0-262143 =="
curl -s -m 180 -D /tmp/v_h.txt -o /tmp/v_256k.bin -H 'Range: bytes=0-262143' "$B/api/download?short_code=$SC"
head -1 /tmp/v_h.txt | tr -d '\r'
grep -i -E 'content-range|^content-type' /tmp/v_h.txt | tr -d '\r'
echo "bytes=$(stat -c %s /tmp/v_256k.bin) magic=$(xxd -p -l 4 /tmp/v_256k.bin)"

echo "== Range 0-4194303 (4 MB, one request) =="
curl -s -m 300 -D /tmp/v_h2.txt -o /tmp/v_4m.bin -w 'time_total=%{time_total} speed=%{speed_download}\n' \
  -H 'Range: bytes=0-4194303' "$B/api/download?short_code=$SC"
head -1 /tmp/v_h2.txt | tr -d '\r'
S=$(stat -c %s /tmp/v_4m.bin)
echo "bytes=$S"
awk -v s="$S" 'BEGIN{printf "MB_per_s=%.2f\n", (s/1048576)/1}'
