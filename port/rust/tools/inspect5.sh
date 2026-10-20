#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
S=$(ls -d "$SRC"/*/grammers-session-0.11.0 | head -1)
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 | head -1)
echo "######## client/mod.rs"
sed -n '1,160p' "$C/src/client/mod.rs"
echo "######## grep connect/InitParams"
grep -rn 'fn connect\|InitParams\|pub struct Config' "$C/src" | head -30
echo "######## client/client.rs head"
sed -n '1,120p' "$C/src/client/client.rs"
echo "######## serde derives in session types"
grep -n -B3 'pub struct DcOption\|pub struct UpdatesState\|pub struct PeerId' "$S/src/types.rs" "$S/src/peer.rs" | head -40
echo "######## tl types: File location + File"
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
grep -rn 'pub struct InputDocumentFileLocation' -A 12 "$T/src" | head -30
grep -rn 'pub struct File ' -A 10 "$T/src" | head -30
echo "######## tl user first_name"
grep -rn 'pub struct User' -A 30 "$T/src/types/mod.rs" 2>/dev/null | head -40
ls "$T/src"
