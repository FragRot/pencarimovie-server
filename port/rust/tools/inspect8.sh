#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
M=$(ls -d "$SRC"/*/grammers-mtsender-0.11.0 | head -1)
echo "size: $(stat -c %s "$T/src/generated.rs")"
echo "######## count"
grep -ac 'InputDocumentFileLocation' "$T/src/generated.rs" || true
echo "######## struct"
grep -an -A 12 'pub struct InputDocumentFileLocation' "$T/src/generated.rs" | head -30
echo "######## enum InputFileLocation"
grep -an -A 12 'pub enum InputFileLocation' "$T/src/generated.rs" | head -30
echo "######## struct User"
grep -an -A 45 'pub struct User {' "$T/src/generated.rs" | head -60
echo "######## upload File enum"
grep -an -A 12 'pub enum File {' "$T/src/generated.rs" | head -30
echo "######## SenderPool::new"
grep -an -B8 -A 30 'pub fn new(' "$M/src/sender_pool.rs" | head -120
echo "######## SenderPool struct decl"
grep -an -A 30 'pub struct SenderPool {' "$M/src/sender_pool.rs" | head -50
