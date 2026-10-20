#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 | head -1)
M=$(ls -d "$SRC"/*/grammers-mtsender-0.11.0 | head -1)
echo "######## examples"
ls "$C/examples"
echo "######## ping example"
cat "$C/examples/ping.rs"
echo "######## downloader example"
cat "$C/examples/downloader.rs"
echo "######## lib.rs (300 lines)"
sed -n '1,300p' "$C/src/lib.rs"
echo "######## mtsender SenderPool"
grep -rn -B4 -A 40 -e 'pub struct SenderPool' "$M/src/lib.rs" | head -80
grep -rn -A 30 -e 'impl.*SenderPool' "$M/src/lib.rs" | head -60
