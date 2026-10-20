#!/bin/bash
# REAL byte throughput: 1 MB Range windows through the Rust port, N concurrent.
#   B=http://127.0.0.1:8088 LABEL=workers-1 bash bench_bytes.sh
set -u
B=${B:-http://127.0.0.1:8088}
LABEL=${LABEL:-workers-1}
SC=${SC:-1v9OPj}
PORT=${PORT:-8088}
PID=$(pgrep -f "pencarimovie-rust --port $PORT" | head -1)
rss() { awk '/VmRSS/{print $2}' "/proc/$PID/status" 2>/dev/null; }
cpu() { awk '{print $14+$15}' "/proc/$PID/stat" 2>/dev/null; }
fd() { ls "/proc/$PID/fd" 2>/dev/null | wc -l; }

echo "label=$LABEL pid=$PID"
echo "csv: label,N,wall_s,requests_ok,bytes_total,MB_per_s,avg_s,max_s,rss_before_kb,rss_after_kb,fd_before,fd_after,cpu_ticks"

for N in 1 10 50; do
  out="/tmp/bb_${LABEL}_${N}.txt"
  r0=$(rss); f0=$(fd); c0=$(cpu)
  t0=$(date +%s.%N)
  seq "$N" | xargs -P "$N" -I{} curl -s -o /dev/null -m 300 \
    -H 'Range: bytes=0-1048575' \
    -w '%{http_code} %{time_total} %{size_download}\n' \
    "$B/api/download?short_code=$SC" > "$out"
  t1=$(date +%s.%N)
  r1=$(rss); f1=$(fd); c1=$(cpu)
  awk -v label="$LABEL" -v n="$N" -v t0="$t0" -v t1="$t1" \
      -v r0="$r0" -v r1="$r1" -v f0="$f0" -v f1="$f1" -v dc="$((c1-c0))" '
    { c[$1]++; s+=$2; b+=$3; if(min==""||$2<min)min=$2; if($2>max)max=$2 }
    END {
      wall = t1 - t0
      printf "%s,%s,%.2f,ok=%d,bytes=%d,%.2f,avg=%.3f,max=%.3f,%s,%s,%s,%s,%s\n",
        label, n, wall, c["206"]+c["200"], b, (wall>0 ? (b/1048576)/wall : 0),
        s/NR, max, r0, r1, f0, f1, dc
    }' "$out"
done
echo "--- per-request detail ---"
for f in /tmp/bb_${LABEL}_*.txt; do echo "$f: $(wc -l < "$f") req"; head -3 "$f"; done
