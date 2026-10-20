#!/bin/bash
# Print the grammers 0.11.0 API surface this port needs, straight from the
# vendored crate sources on the build host. Read-only.
set -u
SRC="$HOME/.cargo/registry/src"
C=$(ls -d "$SRC"/*/grammers-client-0.11.0 2>/dev/null | head -1)
S=$(ls -d "$SRC"/*/grammers-session-0.11.0 2>/dev/null | head -1)
T=$(ls -d "$SRC"/*/grammers-tl-types-0.11.0 2>/dev/null | head -1)
echo "client=$C session=$S"
echo "==== client tree ===="
find "$C/src" -name '*.rs' | sed "s|$C/src/||" | sort
echo "==== connect / sign_in / is_authorized ===="
grep -rn -A 20 -e 'pub async fn connect' "$C/src" | head -60
grep -rn -A 20 -e 'fn bot_sign_in' "$C/src" | head -50
grep -rn -A 10 -e 'fn is_authorized' "$C/src" | head -30
echo "==== Config / InitParams ===="
grep -rn -A 26 -e 'pub struct Config' "$C/src" | head -60
grep -rn -A 22 -e 'pub struct InitParams' "$C/src" | head -50
echo "==== download API ===="
grep -rn -A 30 -e 'pub fn iter_download' "$C/src" | head -90
grep -rn -A 14 -e 'fn chunk_size' -e 'fn skip_chunks' -e 'pub struct DownloadIter' "$C/src" | head -90
echo "==== Downloadable ===="
grep -rn -B3 -A 20 -e 'pub trait Downloadable' "$C/src" | head -60
echo "==== session trait ===="
grep -rn -A 45 -e 'pub trait Session' "$S/src" | head -120
echo "==== session save/load ===="
grep -rn -A 10 -e 'pub fn save' -e 'pub fn load' "$S/src/session.rs" | head -60
echo "==== storages ===="
ls "$S/src/storages"
echo "==== errors ===="
grep -rn -A 22 -e 'pub enum SignInError' -e 'pub enum InvocationError' "$C/src" "$(ls -d "$SRC"/*/grammers-mtproto-0.11.0)/src" | head -120
echo "==== ImportBotAuthorization ===="
grep -rn 'ImportBotAuthorization' "$T/src" | head -6
echo "==== InputDocumentFileLocation ===="
grep -rn 'InputDocumentFileLocation' "$T/src" | head -6
echo "==== get_me / get_me signature ===="
grep -rn -A 6 'fn get_me' "$C/src" | head -30
