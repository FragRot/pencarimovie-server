#!/bin/bash
set -u
G=$(find "$HOME/pm-rust/target/release/build" -name 'generated_types.rs' | head -1)
E=$(find "$HOME/pm-rust/target/release/build" -name 'generated_enums.rs' | head -1)
echo "file=$G"
echo "######## InputDocumentFileLocation"
grep -n -A 12 'pub struct InputDocumentFileLocation' "$G" | head -30
echo "######## enums InputFileLocation"
grep -n -A 12 'pub enum InputFileLocation' "$E" | head -30
echo "######## types User"
grep -n -A 45 'pub struct User {' "$G" | head -60
echo "######## upload File"
grep -n -A 8 'pub struct File {' "$G" | head -20
echo "######## uploaded File enum"
grep -rn -A 10 'pub enum File {' "$E" | head -20
