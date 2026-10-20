#!/bin/bash
# Look for a BOT_TOKEN the operator already supplied to the PHP stack.
# Prints WHERE it lives and the bot id prefix — never the token itself.
set -u
echo "--- docker containers ---"
docker ps --format '{{.Names}}  {{.Image}}' 2>/dev/null | head -10
for c in $(docker ps -q 2>/dev/null); do
  name=$(docker inspect "$c" --format '{{.Name}}' 2>/dev/null)
  line=$(docker inspect "$c" --format '{{range .Config.Env}}{{println .}}{{end}}' 2>/dev/null | grep -E '^(BOT_TOKEN|TG_BOT_TOKEN)=' | head -1)
  if [ -n "$line" ]; then
    id=$(echo "$line" | sed 's/^[A-Z_]*=//' | cut -d: -f1)
    echo "FOUND in container $name -> bot_id prefix $id"
  fi
done
echo "--- files mentioning BOT_TOKEN ---"
grep -rl 'BOT_TOKEN' "$HOME/pencarimovie-docker" "$HOME/pencarimovie-server" "$HOME/pencarimovie-server-fresh" 2>/dev/null | head -5
echo "--- systemd ---"
systemctl list-units --type=service 2>/dev/null | grep -i -E 'pencari|franken' | head -5
for u in $(systemctl list-units --type=service --no-legend 2>/dev/null | awk '{print $1}' | grep -i -E 'pencari|franken'); do
  echo "unit $u:"
  systemctl cat "$u" 2>/dev/null | grep -i -E 'BOT_TOKEN|EnvironmentFile|Environment=' | sed 's/=.*BOT_TOKEN.*/=<redacted>/' | head -5
done
echo "--- process env (php/frankenphp) ---"
for p in $(pgrep -f 'frankenphp|php-fpm|php -S' 2>/dev/null | head -5); do
  if tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qE '^(BOT_TOKEN|TG_BOT_TOKEN)='; then
    id=$(tr '\0' '\n' < "/proc/$p/environ" | grep -E '^BOT_TOKEN=' | head -1 | sed 's/^BOT_TOKEN=//' | cut -d: -f1)
    echo "FOUND in pid $p -> bot_id prefix $id"
  fi
done
echo "done"
