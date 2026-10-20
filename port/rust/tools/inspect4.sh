#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
S=$(ls -d "$SRC"/*/grammers-session-0.11.0 | head -1)
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 | head -1)
echo "######## storages/mod.rs"
cat "$S/src/storages/mod.rs"
echo "######## memory.rs"
sed -n '1,140p' "$S/src/storages/memory.rs"
echo "######## serde usage"
grep -rn 'Serialize\|cfg(feature = "serde")' "$S/src" | head -30
echo "######## session_data From/import_to"
grep -n -A 30 -e 'impl.*From<&' -e 'pub async fn import_to' "$S/src/session_data.rs" | head -100
echo "######## types.rs part 1"
sed -n '1,120p' "$S/src/types.rs"
echo "######## PeerInfo / PeerAuth"
grep -rn -A 30 -e 'pub enum PeerInfo' -e 'pub struct PeerAuth' -e 'pub enum PeerKind\|pub struct PeerId' "$S/src/peer.rs" "$S/src/types.rs" 2>/dev/null | head -120
echo "######## Config in client"
grep -rn -B2 -A 40 'pub struct Config' "$C/src/client/client.rs" "$C/src/client/mod.rs" "$C/src/lib.rs" | head -120
echo "######## Client::connect"
grep -rn -A 20 -e 'pub async fn connect' "$C/src" | head -40
echo "######## User struct"
grep -rn -A 45 'pub struct User ' "$(ls -d "$SRC"/*/grammers-tl-types-0.11.0)/src/types/mod.rs" | head -60
