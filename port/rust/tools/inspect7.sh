#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
M=$(ls -d "$SRC"/*/grammers-mtsender-0.11.0 | head -1)
echo "######## InputDocumentFileLocation"
grep -n -A 14 'pub struct InputDocumentFileLocation' "$T/src/generated.rs" | head -40
echo "######## enums::InputFileLocation"
grep -n -A 14 'pub enum InputFileLocation' "$T/src/generated.rs" | head -40
echo "######## types::User"
grep -n -A 40 '^pub struct User {' "$T/src/generated.rs" | head -60
echo "######## tl lib.rs exports"
sed -n '1,60p' "$T/src/lib.rs"
echo "######## SenderPool"
grep -rn -B6 -A 40 'pub struct SenderPool' "$M/src" | head -90
echo "######## SenderPool::new"
grep -rn -A 30 'impl SenderPool' "$M/src" | head -70
echo "######## Runner"
grep -rn -B3 -A 25 'pub struct SenderPoolRunner\|pub async fn run' "$M/src" | head -90
