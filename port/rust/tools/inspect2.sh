#!/bin/bash
set -u
SRC="$HOME/.cargo/registry/src"
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 | head -1)
S=$(ls -d "$SRC"/*/grammers-session-0.11.0 | head -1)
M=$(ls -d "$SRC"/*/grammers-mtsender-0.11.0 | head -1)
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 | head -1)
echo "######## files.rs"
cat "$C/src/client/files.rs"
echo "######## downloadable.rs"
cat "$C/src/media/downloadable.rs"
echo "######## session trait"
sed -n '1,140p' "$S/src/session.rs"
echo "######## session lib exports"
sed -n '1,80p' "$S/src/lib.rs"
echo "######## Config/InitParams/connect"
grep -rn -B4 -A 40 -e 'pub struct Config' -e 'pub struct InitParams' "$C/src/client/client.rs" | head -160
grep -rn -A 30 -e 'pub async fn connect' "$C/src/client/client.rs" | head -60
echo "######## InputDocumentFileLocation def"
grep -rn -A 14 -e 'pub struct InputDocumentFileLocation' "$T/src" | head -40
echo "######## files types (upload.getFile)"
grep -rn -A 12 -e 'pub struct GetFile ' "$T/src" | head -40
echo "######## InvocationError"
grep -rn -A 30 -e 'pub enum InvocationError' "$M/src" "$C/src" | head -60
