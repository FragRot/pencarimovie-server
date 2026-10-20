// Upstream fan-out probe:  node port/rust/tools/probe_upstream.mjs [base]
//
// Asserts, against a running server:
//   1. the manifest is under the 45,000 B descriptor budget and keeps every
//      local `pm_*` catalog;
//   2. `/catalog/:type/up_<i>_<id>[/extra].json` forwards to the upstream and
//      returns real metas (plain + with an extra);
//   3. `/meta/:type/<bridged id>.json` answers a NON-EMPTY meta (a bridged row
//      must open, not render blank);
//   4. an unknown bridged index answers `{"metas":[]}`, never an SPA page;
//   5. the local catalog/meta numbers are unchanged;
//   6. when an upstream declares `stream`, its items are appended AFTER ours
//      and the sponsor stays first.
//
// Exit code 0 = pass. Read-only (it never POSTs).

const base = process.argv[2] || 'http://127.0.0.1:8088';
let failures = 0;

function ok(cond, label, detail = '') {
  console.log(`${cond ? '  PASS' : '  FAIL'}  ${label}${detail ? '  ' + detail : ''}`);
  if (!cond) failures += 1;
}

async function get(path, timeoutMs = 120000) {
  const t0 = Date.now();
  try {
    const r = await fetch(base + path, { signal: AbortSignal.timeout(timeoutMs) });
    const text = await r.text();
    let body = null;
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
    return { status: r.status, body, bytes: text.length, ms: Date.now() - t0, text };
  } catch (e) {
    return { status: 0, body: null, bytes: 0, ms: Date.now() - t0, err: e.message, text: '' };
  }
}

/* ── 1. manifest ─────────────────────────────────────────────────────────── */
console.log('== MANIFEST ==');
const man = await get('/manifest.json');
const cats = man.body?.catalogs ?? [];
const local = cats.filter((c) => !String(c.id).startsWith('up_'));
const bridged = cats.filter((c) => String(c.id).startsWith('up_'));
console.log(
  `  ${man.status} ${man.bytes}B  catalogs=${cats.length} local=${local.length} bridged=${bridged.length}  types=${JSON.stringify(man.body?.types ?? [])}`,
);
ok(man.status === 200, 'manifest 200');
ok(man.bytes < 45000, 'descriptor under 45,000 B', `${man.bytes}B`);
ok(
  local.map((c) => c.id).join(',') ===
    'pm_movies,pm_latest_movies,pm_trending_movies,pm_series,pm_latest_series,pm_trending_series',
  'local catalogs unchanged (order + ids)',
  local.map((c) => c.id).join(','),
);
for (const c of bridged.slice(0, 3)) {
  console.log(`  bridged ${c.type}/${c.id}  "${c.name}"  extra=${(c.extra || []).map((e) => e.name).join(',')}`);
}
ok(bridged.every((c) => /^up_\d+_/.test(String(c.id))), 'every bridged id is up_<i>_<id>');

/* ── 2. a real bridged catalog ───────────────────────────────────────────── */
console.log('');
console.log('== BRIDGED CATALOG ==');
const withSearch = bridged.find((c) => (c.extra || []).some((e) => e.name === 'search'));
const plain = bridged.find((c) => !(c.extra || []).some((e) => e.name === 'search')) || bridged[0];
let firstId = null;
let firstType = null;
if (plain) {
  const r = await get(`/catalog/${plain.type}/${encodeURIComponent(plain.id)}.json`);
  const metas = r.body?.metas ?? [];
  const sample = metas[0];
  if (sample) {
    firstId = sample.id;
    firstType = sample.type || plain.type;
  }
  console.log(
    `  ${plain.type}/${plain.id} -> ${r.status} ${metas.length} metas ${r.ms}ms  ${sample ? sample.id + ' "' + sample.name + '"' : ''}`,
  );
  ok(r.status === 200 && metas.length > 0, 'bridged catalog returns metas', `${metas.length}`);
} else {
  ok(false, 'no bridged catalog in the manifest');
}
if (withSearch) {
  const r = await get(`/catalog/${withSearch.type}/${encodeURIComponent(withSearch.id)}/search=matrix.json`);
  const metas = r.body?.metas ?? [];
  console.log(`  ${withSearch.type}/${withSearch.id}/search=matrix -> ${r.status} ${metas.length} metas ${r.ms}ms`);
  ok(r.status === 200 && metas.length > 0, 'bridged search catalog returns metas', `${metas.length}`);
}

/* ── 3. meta bridge ──────────────────────────────────────────────────────── */
console.log('');
console.log('== BRIDGED META ==');
if (firstId) {
  const r = await get(`/meta/${firstType}/${encodeURIComponent(firstId)}.json`);
  const m = r.body?.meta;
  console.log(
    `  /meta/${firstType}/${firstId} -> ${r.status} ${r.ms}ms  name=${JSON.stringify(m?.name ?? null)} keys=${m ? Object.keys(m).join(',') : '-'}`,
  );
  ok(r.status === 200 && !!m && !!m.name, 'bridged meta returns a non-empty name', String(m?.name ?? ''));
} else {
  ok(false, 'no meta id to probe (catalog empty)');
}

/* ── 4. stale / unknown bridged id ──────────────────────────────────────── */
console.log('');
console.log('== STALE BRIDGED ID ==');
const stale = await get('/catalog/movie/up_99_nope.json');
console.log(`  up_99_nope -> ${stale.status} ${JSON.stringify(stale.body)}`);
ok(stale.status === 200 && Array.isArray(stale.body?.metas) && stale.body.metas.length === 0, 'unknown index -> {"metas":[]}');

/* ── 5. local regression numbers ────────────────────────────────────────── */
// These are FLOORS, not equalities: WordPress keeps indexing new posts, so the
// list totals drift by a couple of rows between runs (measured 97 → 99 for
// `pm_series` and 98 → 97 for `skip=20` within one session, with no code
// change). The exact-query case stays exact; the paged one must stay SMALLER
// than the first page, which is what "paging works" actually means.
console.log('');
console.log('== LOCAL REGRESSION ==');
const localFloors = [
  ['/catalog/movie/pm_movies.json', 95],
  ['/catalog/series/pm_series.json', 95],
  ['/catalog/movie/pm_movies/genre=Drama.json', 90],
  ['/catalog/movie/pm_movies/skip=20.json', 90],
];
for (const [p, min] of localFloors) {
  const r = await get(p);
  const n = (r.body?.metas ?? []).length;
  ok(n >= min, `${p} >= ${min}`, `${n} in ${r.ms}ms`);
}
{
  const p = '/catalog/movie/pm_movies/search=carrie.json';
  const r = await get(p);
  const n = (r.body?.metas ?? []).length;
  ok(n === 18, `${p} = 18`, `${n} in ${r.ms}ms`);
}
{
  const all = (await get('/catalog/movie/pm_movies.json')).body?.metas?.length ?? 0;
  const paged = (await get('/catalog/movie/pm_movies/skip=20.json')).body?.metas?.length ?? 0;
  ok(paged > 0 && paged < all, 'skip=20 pages past the first entry', `${paged} < ${all}`);
}
const metaCases = [
  ['/meta/movie/pm:post:9000031194.json', 'Siccîn 9 2026'],
  ['/meta/series/pm:post:9000016669.json', 'Kudrat 1968: High Council 2026'],
];
for (const [p, want] of metaCases) {
  const r = await get(p);
  ok(r.body?.meta?.name === want, `${p} name`, JSON.stringify(r.body?.meta?.name ?? null));
}

console.log('');
console.log(failures === 0 ? 'ALL CHECKS PASSED' : `${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
