#!/bin/bash
# Load sweep against the Rust port on the build host.
#
# Two workloads:
#   * `/api/download?d=...` with a REAL Bot API file id and NO short_code, so
#     the handler makes exactly ONE grammers upload.getFile attempt. With the
#     auto-provisioned guest bot that attempt answers FILE_REFERENCE_EXPIRED, so
#     this measures the HTTP + grammers error path per request — it is NOT a
#     byte-throughput measurement (see README).
#   * `/api/version` for the plain HTTP path.
#   PORT=8088 bash bench_vps.sh
set -u
PORT="${PORT:-8088}"
B="${B:-http://127.0.0.1:$PORT}"
PID=$(pgrep -f "pencarimovie-rust --port $PORT" | head -1)
if [ -z "${PID:-}" ]; then echo "server not running on $PORT"; exit 1; fi

D=$(python3 - <<'PY'
import base64, json
p = {
  "file_id_mt": "BQACAgUAAxkDASkV1mrVoxtmjeajGGVQGazxMmuiPS5kAAJzBQACk8FIV9cDMiae-wJuPQQ",
  "file_size": 3967229576,
  "file_name": "Avengers.mkv",
  "mime": "video/x-matroska",
}
print(base64.urlsafe_b64encode(json.dumps(p).encode()).decode().rstrip("="))
PY
)

rss() { awk '/VmRSS/{print $2}' "/proc/$PID/status" 2>/dev/null; }
fds() { ls "/proc/$PID/fd" 2>/dev/null | wc -l; }
cpu() { awk '{print $14+$15}' "/proc/$PID/stat" 2>/dev/null; }

echo "pid=$PID target=$B"
echo "csv: workload,N,wall_s,ok_codes,min_s,avg_s,max_s,rss_before_kb,rss_after_kb,fd_before,fd_after,cpu_ticks"

run() { # label, N, url
  local label=$1 n=$2 url=$3
  local r0 f0 c0 r1 f1 c1 t0 t1 out
  out="/tmp/bench_${label}_${n}.txt"
  r0=$(rss); f0=$(fds); c0=$(cpu)
  t0=$(date +%s.%N)
  seq "$n" | xargs -P "$n" -I{} curl -s -o /dev/null -m 120 -w '%{http_code} %{time_total}\n' "$url" > "$out"
  t1=$(date +%s.%N)
  r1=$(rss); f1=$(fds); c1=$(cpu)
  awk -v label="$label" -v n="$n" -v t0="$t0" -v t1="$t1" \
      -v r0="$r0" -v r1="$r1" -v f0="$f0" -v f1="$f1" -v dc="$((c1-c0))" '
    { c[$1]++; s+=$2; if(min==""||$2<min)min=$2; if($2>max)max=$2 }
    END { printf "%s,%s,%.2f,ok=%d,min=%.3f,avg=%.3f,max=%.3f,%s,%s,%s,%s,%s\n",
          label, n, t1-t0, c["206"]+c["200"]+c["410"], min, s/NR, max, r0, r1, f0, f1, dc }' "$out"
}

for n in 1 10 50; do
  run "download_410" "$n" "$B/api/download?d=$D&file_size=3967229576&file_name=Avengers.mkv&mime=video/x-matroska"
done
for n in 10 50; do
  run "version" "$n" "$B/api/version"
done
