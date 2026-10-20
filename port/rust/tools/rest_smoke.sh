#!/bin/bash
# Direct curl sweep of the REST API — the same set the TS port used
# (port/ts/scripts/rest_smoke.sh), pointed at the Rust port.
#   B=http://127.0.0.1:8088 bash rest_smoke.sh
set -u
B=${B:-http://127.0.0.1:8088}
show() { # label, method, path, [body]
  local label=$1 method=$2 path=$3 body=${4:-}
  local code
  if [ -n "$body" ]; then
    code=$(curl -s -m 60 -o /tmp/rust_body -w '%{http_code}' -X "$method" -H 'Content-Type: application/json' \
          -d "$body" "$B$path")
  else
    code=$(curl -s -m 60 -o /tmp/rust_body -w '%{http_code}' -X "$method" "$B$path")
  fi
  printf '\n### %s\n%s %s -> HTTP %s\n' "$label" "$method" "$path" "$code"
  head -c 320 /tmp/rust_body
  printf '\n'
}

show 'version'            GET  '/api/version'
show 'auth/status'        GET  '/api/auth/status'
show 'session'            GET  '/api/session'
show 'bots'               GET  '/api/bots'
show 'lan-ip'             GET  '/api/lan-ip'
show 'clock-check'        GET  '/api/clock-check'
show 'tunnel/status'      GET  '/api/tunnel/status'
show 'catalog-settings'   GET  '/api/catalog-settings'
show 'country'            GET  '/api/country'
show 'diagnostics'        GET  '/api/diagnostics'
show 'diagnostics set'    GET  '/api/diagnostics/settings'
show 'reports'            GET  '/api/reports'
show 'debug-mode'         GET  '/api/debug-mode'
show 'logs'               GET  '/api/logs'
show 'resolve'            GET  '/api/resolve-shortcode?short_code=1v9OPj'
show 'warmup'             GET  '/api/warmup-resolve?short_codes=1v9OPj'
show 'manifest'           GET  '/manifest.json'
show 'resolve-shortcode no code' GET '/api/resolve-shortcode'
show 'provision'          GET  '/api/provision'
show 'botlogin no token'  POST '/api/botlogin' '{}'
show 'bots/add no token'  POST '/api/bots/add' '{}'
show 'bots/remove empty'  POST '/api/bots/remove' '{"bot_id":""}'
show 'bots/set-active bad' POST '/api/bots/set-active' '{"bot_id":"0"}'
show 'validate ok'        POST '/api/validate-manifest' '{"url":"https://opensubtitles-v3.strem.io"}'
show 'validate empty'     POST '/api/validate-manifest' '{"url":""}'
show 'auth/login wrong'   POST '/api/auth/login' '{"password":"__x__"}'
show 'auth/password wrong' POST '/api/auth/password' '{"current":"__x__","next":"abcd1234"}'
show 'catalog-settings post' POST '/api/catalog-settings' '{}'
show 'download no params' GET  '/api/download'
show 'download empty sc'  GET  '/api/download?short_code='
show 'unknown api'        GET  '/api/does-not-exist'
show 'stream direct file' GET  '/stream/movie/pm_file_1v9OPj.json'
show 'stream unported'    GET  '/stream/movie/tt99999999.json'
show 'eclipse manifest'   GET  '/eclipse/manifest.json'
show 'nuvio manifest'     GET  '/nuvio/manifest.json'
