// Catalog + meta probe:  node port/rust/tools/probe_catalog.mjs [base]
const base = process.argv[2] || 'http://127.0.0.1:8088';

async function get(path) {
  const t0 = Date.now();
  try {
    const r = await fetch(base + path, { signal: AbortSignal.timeout(120000) });
    const text = await r.text();
    let body = null;
    try { body = JSON.parse(text); } catch { body = null; }
    return { status: r.status, body, ms: Date.now() - t0, bytes: text.length };
  } catch (e) {
    return { status: 0, body: null, ms: Date.now() - t0, bytes: 0, err: e.message };
  }
}

const man = await get('/manifest.json');
const cats = man.body?.catalogs ?? [];
const localCats = cats.filter((c) => !String(c.id).startsWith('up_'));
console.log(`manifest ${man.status} ${man.bytes}B  catalogs=${cats.length} local=${localCats.length}`);
for (const c of localCats) console.log(`  ${c.type}/${c.id}  "${c.name}"  extra=${(c.extra || []).map((e) => e.name).join(',')}`);
console.log('');

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}`);
};
// Stremio rejects a descriptor over ~49 KB (`addonCollectionSet` code 20004),
// and the port's own budget is 45,000 B — the two trending rows must not push
// it over.
check(man.status === 200, 'manifest 200');
check(man.bytes < 45000, `manifest ${man.bytes}B < 45000B`);
check(
  cats.some((c) => c.type === 'movie' && c.id === 'pm_trending_movies'),
  'manifest advertises pm_trending_movies',
);
check(
  cats.some((c) => c.type === 'series' && c.id === 'pm_trending_series'),
  'manifest advertises pm_trending_series',
);
check(
  cats.every((c) => !Array.isArray(c.genres)),
  'no catalog carries a genres array',
);

// The Catalog Configuration panel iterates `Object.entries(catalog_options)` and
// files rows by `info.group`; a list-of-manifest-rows shape renders it EMPTY.
const cs = await get('/api/catalog-settings');
const om = cs.body?.catalog_options ?? {};
const optKeys = Object.keys(om);
check(
  cs.status === 200 && om.pm_trending_movies?.group === 'trending' && om.pm_trending_movies?.type === 'movie',
  'catalog_options.pm_trending_movies grouped/typed',
);
check(
  om.pm_trending_series?.group === 'trending' && om.pm_trending_series?.type === 'series',
  'catalog_options.pm_trending_series grouped/typed',
);
check(
  optKeys.length > 0 &&
    optKeys.every(
      (k) =>
        om[k] &&
        typeof om[k] === 'object' &&
        typeof om[k].group === 'string' &&
        typeof om[k].name === 'string' &&
        typeof om[k].type === 'string',
    ),
  `every catalog option carries type/group/name (${optKeys.length} options)`,
);
check(
  cats
    .filter((c) => !String(c.id).startsWith('up_'))
    .every((c) => Object.prototype.hasOwnProperty.call(om, c.id)),
  'every advertised row has a catalog option',
);
console.log('');

const cases = [
  '/catalog/movie/pm_movies.json',
  '/catalog/movie/pm_latest_movies.json',
  '/catalog/series/pm_series.json',
  '/catalog/series/pm_latest_series.json',
  '/catalog/movie/pm_movies/search=carrie.json',
  '/catalog/movie/pm_movies/genre=Drama.json',
  '/catalog/movie/pm_movies/skip=20.json',
];
for (const p of cases) {
  const r = await get(p);
  const metas = r.body?.metas ?? [];
  const first = metas[0] ? `${metas[0].id} "${metas[0].name}" ${metas[0].releaseInfo || ''}` : (r.err || '');
  console.log(`${p.padEnd(46)} ${String(r.status)} ${String(metas.length).padStart(4)} metas ${String(r.ms).padStart(6)}ms  ${first}`);
}
console.log('');

// ── trending rows + country fallback ───────────────────────────────────────
// `pm_trending_movies` / `pm_trending_series` merge the live `/trending` keyword
// map into one post row per type; both must be NON-EMPTY on the origin.
const trendingCases = [
  '/catalog/movie/pm_trending_movies.json',
  '/catalog/series/pm_trending_series.json',
  '/catalog/movie/pm_trending_movies/skip=5.json',
  '/catalog/movie/pm_movies_canada.json',
  '/catalog/movie/pm_movies_malay.json',
  '/catalog/movie/pm_movies_malay_2019.json',
  '/catalog/movie/pm_cat_action.json',
];
for (const p of trendingCases) {
  const r = await get(p);
  const metas = r.body?.metas ?? [];
  const first = metas[0] ? `${metas[0].id} "${metas[0].name}" ${metas[0].releaseInfo || ''}` : (r.err || '');
  console.log(`${p.padEnd(46)} ${String(r.status)} ${String(metas.length).padStart(4)} metas ${String(r.ms).padStart(6)}ms  ${first}`);
  if (p.includes('pm_trending_')) {
    const kind = p.includes('pm_trending_movies') ? 'movie' : 'series';
    check(metas.length > 0, `${p} non-empty`);
    check(metas.every((m) => m.type === kind), `${p} only ${kind} entries`);
    const ids = metas.map((m) => m.id);
    check(new Set(ids).size === ids.length, `${p} ids deduped`);
  }
  if (p.includes('pm_movies_canada') || p.includes('pm_movies_malay')) {
    check(metas.length > 0, `${p} non-empty`);
  }
}
console.log('');

const metaCases = [
  '/meta/movie/pm:post:9000031194.json',
  '/meta/series/pm:post:9000016669.json',
  '/meta/movie/pm:file:1v9OPj.json',
  '/meta/movie/tt99999999.json',
];
for (const p of metaCases) {
  const r = await get(p);
  const m = r.body?.meta;
  const vids = Array.isArray(m?.videos) ? m.videos.length : 0;
  console.log(`${p.padEnd(42)} ${String(r.status)} ${String(r.ms).padStart(6)}ms  name=${JSON.stringify(m?.name ?? null)} videos=${vids}`);
  if (m?.videos?.length) {
    const v = m.videos[0];
    console.log(`    first video: ${v.id} S${v.season}E${v.episode} "${v.name}"`);
  }
}
console.log('');

if (failures.length) {
  console.log(`FAILED ${failures.length}:`);
  for (const f of failures) console.log(`  - ${f}`);
  process.exit(1);
}
console.log('ALL CATALOG CHECKS PASSED');
