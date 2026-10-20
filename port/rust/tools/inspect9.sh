#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
M=$(ls -d "$SRC"/*/grammers-mtsender-0.11.0 | head -1)
echo "######## generated.rs"
cat "$T/src/generated.rs"
echo "######## find InputDocumentFileLocation"
grep -arn 'InputDocumentFileLocation' "$T" | head -5
echo "######## tl src listing"
find "$T" -name '*.rs' | head -20
echo "######## find User struct"
grep -arn -m1 -A 40 'pub struct User {' "$T" | head -50
echo "######## SenderPool fn new"
grep -an 'fn new' "$M/src/sender_pool.rs" | head
grep -an -A 40 'impl SenderPool {' "$M/src/sender_pool.rs" | head -60
