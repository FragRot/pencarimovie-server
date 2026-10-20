// Concurrency stress for the cache layers (L1 memory + redb) — measures the
// end-to-end effect, not the microbenchmark.
//
//   node port/rust/tools/cache_stress.mjs [--base http://127.0.0.1:8088]
//                                        [--concurrency 16] [--seconds 20]
//                                        [--token <token>] [--post 9000031194]
//
// What it does, per run:
//   * warms nothing — the FIRST pass is deliberately cold for the caches it can
//     clear (`/api/cache/clear`), so both passes are comparable;
//   * drives a realistic mix (manifest, catalog, meta, stream, version) with N
//     concurrent workers for `--seconds`;
//   * prints p50/p95/p99 + rps per endpoint, then the cache stats delta;
//   * fails (exit 1) on any 5xx or if the L1 never records a hit.
//
// The old cache was one JSON file per key (182-224 us per hit, measured): with a
// warm cache the median for `/stream` should now be dominated by the origin, and
// a repeated run must cost **zero** origin calls (visible as the same p50).
import { setTimeout as sleep } from 'node:timers/promises';

const argv = process.argv.slice(2);
const arg = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : dflt;
};
const BASE = arg('base', 'http://127.0.0.1:8088').replace(/\/$/, '');
const CONC = Number(arg('concurrency', '16'));
const SECONDS = Number(arg('seconds', '20'));
const TOKEN = arg('token', '');
const POST = arg('post', '9000031194');
// `--no-clear` keeps the caches intact, which is how the redb layer is proven:
// after a server RESTART the first pass must still be answered from L2.
const NO_CLEAR = argv.includes('--no-clear');

const headers = {};
if (TOKEN) headers['X-Auth-Token'] = TOKEN;

const targets = [
  { name: 'version', path: '/api/version' },
  { name: 'manifest', path: '/manifest.json' },
  { name: 'catalog', path: '/catalog/movie/pm_movies_top.json' },
  { name: 'meta', path: `/meta/movie/pm:post:${POST}.json` },
  { name: 'stream', path: `/stream/movie/pm:post:${POST}.json` },
];

async function get(path, timeoutMs = 30_000) {
  const t0 = performance.now();
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    const res = await fetch(BASE + path, { headers, signal: ctl.signal });
    const body = await res.arrayBuffer();
    return { ms: performance.now() - t0, status: res.status, bytes: body.byteLength };
  } catch (e) {
    return { ms: performance.now() - t0, status: 0, bytes: 0, error: String(e && e.message ? e.message : e) };
  } finally {
    clearTimeout(timer);
  }
}

function pct(sorted, p) {
  if (!sorted.length) return 0;
  const i = Math.min(sorted.length - 1, Math.max(0, Math.round((p / 100) * (sorted.length - 1))));
  return sorted[i];
}

async function stats() {
  const r = await get('/api/cache/stats', 10_000);
  if (r.status !== 200) return null;
  // Re-read as text: `get` consumed the body, so ask again with a plain fetch.
  try {
    const res = await fetch(BASE + '/api/cache/stats', { headers });
    return await res.json();
  } catch {
    return null;
  }
}

async function clearCache() {
  const r = await get('/api/cache/clear', 15_000);
  return r.status === 200;
}

async function runPass(label) {
  const per = new Map(targets.map((t) => [t.name, { ms: [], err: 0, bad: 0, bytes: 0 }]));
  const deadline = Date.now() + SECONDS * 1000;
  let total = 0;

  async function worker(id) {
    let i = id;
    while (Date.now() < deadline) {
      const t = targets[i % targets.length];
      i += 1;
      const r = await get(t.path);
      const slot = per.get(t.name);
      total += 1;
      slot.bytes += r.bytes;
      if (r.status === 0) slot.err += 1;
      else if (r.status >= 500) slot.bad += 1;
      else slot.ms.push(r.ms);
    }
  }

  const started = Date.now();
  await Promise.all(Array.from({ length: CONC }, (_, k) => worker(k)));
  const wall = (Date.now() - started) / 1000;

  console.log(`\n── pass: ${label}  (${CONC} workers, ${wall.toFixed(1)} s, ${total} requests, ${(total / wall).toFixed(1)} rps)`);
  console.log('   endpoint    ok   err  5xx     p50      p95      p99    avg');
  let anyBad = 0;
  for (const t of targets) {
    const s = per.get(t.name);
    const sorted = s.ms.slice().sort((a, b) => a - b);
    const avg = sorted.reduce((a, b) => a + b, 0) / (sorted.length || 1);
    anyBad += s.bad;
    console.log(
      `   ${t.name.padEnd(10)} ${String(sorted.length).padStart(4)} ${String(s.err).padStart(5)} ${String(s.bad).padStart(5)}` +
        `  ${pct(sorted, 50).toFixed(0).padStart(6)}ms ${pct(sorted, 95).toFixed(0).padStart(7)}ms` +
        `  ${pct(sorted, 99).toFixed(0).padStart(6)}ms ${avg.toFixed(0).padStart(6)}ms`,
    );
  }
  return { anyBad, total, wall };
}

function showStats(tag, s) {
  if (!s) {
    console.log(`   stats (${tag}): unavailable`);
    return;
  }
  const l1 = s.l1 || {};
  console.log(
    `   stats (${tag}): L1 entries=${l1.entries} hits=${l1.hits} misses=${l1.misses} evictions=${l1.evictions}` +
      ` | L2 open=${s.l2?.open} rows=${s.l2?.rows} puts=${s.l2_puts} hits=${s.l2_hits} coalesced=${s.coalesced}`,
  );
  if (l1.prefixes) {
    const parts = Object.entries(l1.prefixes)
      .sort((a, b) => b[1] - a[1])
      .map(([k, v]) => `${k}=${v}`);
    if (parts.length) console.log(`   prefixes: ${parts.join(' ')}`);
  }
}

const before = await stats();
showStats('before', before);

const cleared = NO_CLEAR ? 'skipped (--no-clear)' : await clearCache();
console.log(`\ncache cleared before the cold pass: ${cleared}`);

const cold = await runPass('cold (caches cleared)');
const mid = await stats();
showStats('after cold', mid);

const warm = await runPass('warm (same targets)');
const after = await stats();
showStats('after warm', after);

const l1Hits = (after?.l1?.hits ?? 0) - (mid?.l1?.hits ?? 0);
const problems = [];
if (cold.anyBad + warm.anyBad > 0) problems.push(`${cold.anyBad + warm.anyBad} HTTP 5xx responses`);
if (l1Hits <= 0) problems.push('the warm pass recorded no L1 hits at all');
if (after?.l2?.open === false) problems.push('L2 (redb) is not open - check the server log for DatabaseAlreadyOpen');

console.log('');
if (problems.length) {
  console.log(`RESULT: FAIL - ${problems.join('; ')}`);
  process.exit(1);
}
console.log(`RESULT: PASS - warm pass served ${l1Hits} L1 hits, 0 x 5xx, L2 rows=${after?.l2?.rows ?? 0}`);
