#!/bin/bash
# Probe a Range download for several short codes and print status + magic bytes.
#   B=http://127.0.0.1:8088 bash try_shortcodes.sh xn9eQ7 ZMf0Rn
set -u
B=${B:-http://127.0.0.1:8088}
CODES=${*:-"1v9OPj xn9eQ7 ZMf0Rn 9HJMjC"}
for sc in $CODES; do
  echo "=== $sc ==="
  hdr="/tmp/h_$sc.txt"
  body="/tmp/r_$sc.bin"
  rm -f "$hdr" "$body"
  curl -s -m 180 -D "$hdr" -o "$body" -H 'Range: bytes=0-262143' "$B/api/download?short_code=$sc"
  head -1 "$hdr"
  grep -i -E 'content-range|^content-type|^content-length' "$hdr" | tr -d '\r'
  echo "bytes=$(stat -c %s "$body" 2>/dev/null)"
  xxd -l 16 "$body" 2>/dev/null
done
