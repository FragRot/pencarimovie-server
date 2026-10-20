// Episode-name guard (fd_upstream_episode_names) — live A/B proof.
//
//   node port/rust/tools/probe_episode_names.mjs [base] [settings] [cache-dir]
//
// What it proves, end to end through the real stream route:
//
//   A. With the REAL upstream, `tt11540908:1:1` ("Carrie", E1 is named
//      "FIRST PERIOD" in the provider metadata) keeps every stream and keeps the
//      files whose tail MATCHES that name — the guard is active, not inert.
//   B. With a STUB upstream that names the same episode something else, the
//      matching-tailed files are REJECTED — the sibling-work case the guard
//      exists for.
//   C. After the settings file is restored byte-identically, the same request
//      returns the original list again: the guard does not change a baseline
//      count when the metadata agrees (or when it cannot be verified, which is
//      the fail-open contract).
//
// Only writable action: a snapshot/restore of `catalog_settings.json` (the same
// pattern probe_catalog_toggle.mjs uses) and clearing the ephemeral
// `stream_cache_*` files, so each phase really recomputes.

import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const base = (process.argv[2] || 'http://127.0.0.1:8088').replace(/\/$/, '');
const settingsPath = process.argv[3] || 'port/rust/storage/catalog_settings.json';
const cacheDir = process.argv[4] || 'port/rust/storage/cache';

const ITEM = 'series/tt11540908:1:1';
const MARK = 'FIRST.PERIOD';

let pass = 0;
let fail = 0;
function check(ok, label, detail = '') {
  if (ok) {
    pass += 1;
    console.log(`  PASS  ${label}${detail ? ` — ${detail}` : ''}`);
  } else {
    fail += 1;
    console.log(`  FAIL  ${label}${detail ? ` — ${detail}` : ''}`);
  }
}

function clearStreamCache() {
  let n = 0;
  for (const f of fs.readdirSync(cacheDir)) {
    if (f.startsWith('stream_cache_') && f.endsWith('.json')) {
      try {
        fs.unlinkSync(path.join(cacheDir, f));
        n += 1;
      } catch {
        /* ignore */
      }
    }
  }
  return n;
}

async function fetchStream() {
  const t0 = Date.now();
  const r = await fetch(`${base}/stream/${ITEM}.json`, { signal: AbortSignal.timeout(180000) });
  const body = await r.json();
  const names = (body.streams || []).map((s) => String(s.description || '').split('\n')[0]);
  return { status: r.status, count: names.length, names, ms: Date.now() - t0 };
}

const countMarked = (names) => names.filter((n) => n.includes(MARK)).length;

function startStub(name) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const url = req.url || '/';
      if (url.includes('/manifest.json')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(
          JSON.stringify({
            id: 'org.pencarimovie.nametest',
            version: '1.0.0',
            name: 'NameTest',
            description: 'episode-name guard probe',
            resources: ['meta'],
            types: ['series', 'movie'],
            idPrefixes: ['tt'],
          })
        );
        return;
      }
      if (url.includes('/meta/series/tt11540908.json')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(
          JSON.stringify({
            meta: {
              id: 'tt11540908',
              type: 'series',
              name: 'Carrie',
              releaseInfo: '2026-',
              videos: [
                {
                  id: 'tt11540908:1:1',
                  season: 1,
                  episode: 1,
                  title: name,
                },
              ],
            },
          })
        );
        return;
      }
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ err: 'not found' }));
    });
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

// The file is OPTIONAL: a fresh install runs on `default_catalog_settings()`
// and writes nothing until the dashboard saves. Snapshot "absent" too, so the
// restore is truthful in both cases.
const hadSettings = fs.existsSync(settingsPath);
const originalSettings = hadSettings ? fs.readFileSync(settingsPath) : Buffer.from('{}');

async function withSettings(manifests) {
  const doc = JSON.parse(originalSettings.toString('utf8'));
  doc.upstream_enabled = true;
  doc.upstream_manifests = manifests;
  fs.writeFileSync(settingsPath, JSON.stringify(doc));
  clearStreamCache();
}

let stub = null;
try {
  // ── A: real upstream ──────────────────────────────────────────────────────
  console.log('== A: real upstream (guard active, matching names kept) ==');
  clearStreamCache();
  const a = await fetchStream();
  console.log(`  ${ITEM} -> ${a.count} streams (${a.ms}ms), ${countMarked(a.names)} with ${MARK}`);
  check(a.status === 200, 'A status 200', String(a.status));
  check(a.count === 15, 'A baseline count is 15', String(a.count));
  check(countMarked(a.names) >= 1, 'A a name-matching file survives', `${countMarked(a.names)} x ${MARK}`);

  // ── B: stub upstream naming the episode something else ────────────────────
  console.log('== B: stub upstream with a WRONG episode name (sibling rejected) ==');
  stub = await startStub('Totally Wrong Sibling Episode');
  const stubPort = stub.address().port;
  await withSettings([
    { url: `http://127.0.0.1:${stubPort}/manifest.json`, name: 'NameTest' },
  ]);
  const b = await fetchStream();
  console.log(`  ${ITEM} -> ${b.count} streams (${b.ms}ms), ${countMarked(b.names)} with ${MARK}`);
  check(b.status === 200, 'B status 200', String(b.status));
  check(b.count < a.count, 'B the list shrinks (files rejected)', `${a.count} -> ${b.count}`);
  check(countMarked(b.names) === 0, 'B every name-matching file is rejected', `${countMarked(b.names)} left`);
  check(b.names.length > 0, 'B the untail-ed files still play (fail open)');

  // ── C: restore ────────────────────────────────────────────────────────────
  console.log('== C: settings restored byte-identically (baseline back) ==');
} finally {
  if (hadSettings) {
    fs.writeFileSync(settingsPath, originalSettings);
  } else if (fs.existsSync(settingsPath)) {
    fs.unlinkSync(settingsPath);
  }
  if (stub) stub.close();
  clearStreamCache();
}

const restored = hadSettings
  ? fs.readFileSync(settingsPath).equals(originalSettings)
  : !fs.existsSync(settingsPath);
check(restored, 'C catalog_settings.json restored byte-identically', hadSettings ? 'file' : 'still absent');

const c = await fetchStream();
console.log(`  ${ITEM} -> ${c.count} streams (${c.ms}ms), ${countMarked(c.names)} with ${MARK}`);
check(c.count === 15, 'C baseline count is 15 again', String(c.count));
check(countMarked(c.names) >= 1, 'C the name-matching files are back', `${countMarked(c.names)} x ${MARK}`);

console.log(`\nRESULT: ${fail === 0 ? 'PASS' : 'FAIL'} (${pass} passed, ${fail} failed)`);
process.exit(fail === 0 ? 0 : 1);
