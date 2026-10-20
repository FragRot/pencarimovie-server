// Catalog Configuration gate probe:  node port/rust/tools/probe_catalog_toggle.mjs [base]
//
// Asserts the PHP's descriptor rules against a RUNNING server:
//   `catalogs_enabled: false`  -> NO catalogs at all AND no `catalog` resource;
//   `catalogs_enabled: true`   -> the six local rows are back;
//   `enabled_catalogs[id]=false` -> just that row disappears.
//
// It SNAPSHOTS storage/catalog_settings.json and restores it byte-for-byte
// afterwards (this is live per-user state), and it restores it even when an
// assertion fails. Exit code 0 = pass.
//
// The file it touches is the one the server itself reads. If the server runs
// with a different working directory (PM_ASSETS=disk / a packaged exe), pass the
// settings path as a second argument:
//   node port/rust/tools/probe_catalog_toggle.mjs http://127.0.0.1:8088 ../storage/catalog_settings.json

import fs from 'node:fs';
import path from 'node:path';

const base = process.argv[2] || 'http://127.0.0.1:8088';
// The port's OWN state file. Run this from the repo root; pass the path as the
// 2nd argument when the server runs from somewhere else (a packaged exe).
// Do NOT auto-pick the first file that exists — the repo root also has a PHP
// install whose storage/catalog_settings.json is a DIFFERENT user's state.
const settingsPath =
  process.argv[3] || path.resolve('port/rust/storage/catalog_settings.json');

let failures = 0;
function check(cond, label, detail = '') {
  console.log(`${cond ? '  PASS' : '  FAIL'}  ${label}${detail ? '  ' + detail : ''}`);
  if (!cond) failures += 1;
}

async function get(p) {
  try {
    const r = await fetch(base + p, { signal: AbortSignal.timeout(120000) });
    const text = await r.text();
    let body = null;
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
    return { status: r.status, body, text, bytes: text.length };
  } catch (e) {
    return { status: 0, body: null, text: '', bytes: 0, err: e.message };
  }
}

async function post(p, payload) {
  try {
    const r = await fetch(base + p, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(120000),
    });
    const text = await r.text();
    let body = null;
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
    return { status: r.status, body, text };
  } catch (e) {
    return { status: 0, body: null, text: '', err: e.message };
  }
}

const manifest = async () => {
  const r = await get('/manifest.json');
  const cats = (r.body?.catalogs ?? []).filter((c) => !String(c.id).startsWith('up_'));
  return {
    bytes: r.bytes,
    all: (r.body?.catalogs ?? []).length,
    local: cats.map((c) => c.id),
    resources: r.body?.resources ?? [],
    raw: r.body,
  };
};

// ── snapshot ───────────────────────────────────────────────────────────────
const existed = fs.existsSync(settingsPath);
const snapshot = existed ? fs.readFileSync(settingsPath) : null;
console.log(`settings: ${settingsPath}${existed ? ' (snapshotted)' : ' (absent)'}`);
if (!existed) {
  console.log('  WARNING: no settings file found; the toggle will CREATE one, and this');
  console.log('  probe deletes it again on exit. Point the 2nd argument at the real path');
  console.log('  if the server uses a different one.');
}

let restore = true;
try {
  const before = await manifest();
  console.log(`\nbaseline: ${before.all} catalogs, local=${before.local.length}`);

  // ── catalogs OFF ─────────────────────────────────────────────────────────
  const off = await post('/api/catalog-settings', { catalogs_enabled: false });
  check(off.status === 200 && off.body?.ok !== 0, 'POST catalogs_enabled=false', JSON.stringify(off.body).slice(0, 120));
  const offMan = await manifest();
  console.log(`off: ${offMan.all} catalogs, local=${offMan.local.length}, resources=${JSON.stringify(offMan.resources)}`);
  check(offMan.all === 0, 'catalogs_enabled=false advertises NO catalogs', String(offMan.all));
  check(!offMan.resources.includes('catalog'), 'catalogs_enabled=false drops the catalog resource');

  // The ROUTE still answers (the PHP only gates the descriptor).
  const stillRoute = await get('/catalog/movie/pm_trending_movies.json');
  check(
    stillRoute.status === 200 && (stillRoute.body?.metas ?? []).length > 0,
    'the catalog route still serves a row while disabled',
    `${(stillRoute.body?.metas ?? []).length} metas`,
  );

  // ── catalogs ON again ────────────────────────────────────────────────────
  const on = await post('/api/catalog-settings', { catalogs_enabled: true });
  check(on.status === 200, 'POST catalogs_enabled=true');
  const onMan = await manifest();
  console.log(`on: ${onMan.all} catalogs, local=${onMan.local.join(',')}`);
  check(onMan.local.length === 6, 'six local rows back', String(onMan.local.length));
  check(onMan.resources.includes('catalog'), 'catalog resource restored');

  // ── a single row hidden ──────────────────────────────────────────────────
  const hide = await post('/api/catalog-settings', {
    enabled_catalogs: { pm_trending_series: false },
  });
  check(hide.status === 200, 'POST enabled_catalogs.pm_trending_series=false');
  const hidden = await manifest();
  check(
    !hidden.local.includes('pm_trending_series'),
    'a disabled row is not advertised',
    hidden.local.join(','),
  );
  check(
    hidden.local.includes('pm_trending_movies'),
    'its sibling stays advertised',
    hidden.local.join(','),
  );
  const unhide = await post('/api/catalog-settings', {
    enabled_catalogs: { pm_trending_series: true },
  });
  check(unhide.status === 200, 'POST enabled_catalogs.pm_trending_series=true');
  const finalMan = await manifest();
  check(finalMan.local.length === 6, 'six rows after re-enabling', String(finalMan.local.length));
} finally {
  if (restore) {
    if (existed) {
      fs.writeFileSync(settingsPath, snapshot);
    } else {
      try {
        fs.unlinkSync(settingsPath);
      } catch {
        /* already gone */
      }
    }
    const back = await manifest();
    console.log(`\nrestored settings: ${existed ? 'byte-identical snapshot' : 'file removed'}`);
    console.log(`after restore: ${back.all} catalogs, local=${back.local.length}`);
    check(back.local.length === 6, 'restore leaves the six local rows', back.local.join(','));
  }
}

console.log('');
console.log(failures === 0 ? 'ALL TOGGLE CHECKS PASSED' : `${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
