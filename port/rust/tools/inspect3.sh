#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 | head -1)
S=$(ls -d "$SRC"/*/grammers-session-0.11.0 | head -1)
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
echo "######## client Cargo.toml features"
sed -n '1,80p' "$C/Cargo.toml"
echo "######## session Cargo.toml features"
sed -n '1,80p' "$S/Cargo.toml"
echo "######## storages"
ls "$S/src/storages"
echo "######## SessionData"
grep -n -B6 -A 60 -e 'pub struct SessionData' "$S/src/session_data.rs" | head -120
echo "######## SessionData impl"
grep -n -A 20 -e 'impl SessionData' "$S/src/session_data.rs" | head -80
echo "######## Config struct location"
grep -rn 'pub struct Config' "$C/src" | head
grep -rn -A 30 'pub struct Config' "$C/src/lib.rs" | head -80
echo "######## InitParams"
grep -rn -A 25 'pub struct InitParams' "$C/src" | head -60
echo "######## connect"
grep -rn -A 25 'pub async fn connect' "$C/src" | head -50
echo "######## tl InputDocumentFileLocation"
grep -rn -A 12 'pub struct InputDocumentFileLocation' "$T/src/types/mod.rs" | head -30
echo "######## tl uploaded File"
grep -rn -A 10 'pub struct File ' "$T/src/types/mod.rs" | head -30
echo "######## user fields"
grep -rn -A 40 'pub struct User ' "$T/src/types/mod.rs" | head -60
echo "######## DcOption"
grep -rn -A 16 'pub struct DcOption' "$S/src/types.rs" "$S/src/types"/*.rs 2>/dev/null | head -40
echo "######## UpdatesState / UpdateState"
grep -rn -A 14 -e 'pub struct UpdatesState' -e 'pub enum UpdateState' "$S/src" | head -60
