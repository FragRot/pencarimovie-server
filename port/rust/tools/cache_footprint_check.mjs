#!/usr/bin/env node
/**
 * cache_footprint_check.mjs — what does a 64 MB download do to RSS and to the
 * on-disk cache? Runs a REAL server against a throwaway PM_APP_ROOT (so the cache
 * dir is readable), provisions a guest bot, streams 64 MB and reports:
 *
 *   - server RSS before / peak / after (sampled from /api/diagnostics)
 *   - storage/cache size and file count before / after, broken down by cache kind
 *
 * The point: media bytes must go straight to the socket. If `cache_kb` grows by
 * megabytes, something is buffering the file to disk and the TTL pruner cannot save
 * the device.
 *
 * Usage: node port/rust/tools/cache_footprint_check.mjs [--exe <path>] [--port 8091]
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, statSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir, platform } from 'node:os';

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : dflt;
};
const ROOT = resolve(import.meta.dirname, '..');
const EXE = resolve(
  arg('exe', join(ROOT, 'dist-rust', platform() === 'win32' ? 'windows-x86_64/pencarimovie-rust.exe' : 'linux-x86_64/pencarimovie-rust')),
);
const PORT = Number(arg('port', '8091'));
const STATE = join(tmpdir(), `pmcache-${Date.now()}`);
const CACHE = join(STATE, 'storage', 'cache');
const ITEM = arg('item', 'pm:post:9000031194');
const RANGE = 64 * 1024 * 1024;

if (!existsSync(EXE)) {
  console.error(`missing ${EXE} — build it first (scripts/build-rust-windows.bat)`);
  process.exit(2);
}
mkdirSync(CACHE, { recursive: true });

const base = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const dirStats = (dir) => {
  let bytes = 0;
  let files = 0;
  const byKind = {};
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const full = join(d, e.name);
      if (e.isDirectory()) {
        walk(full);
        continue;
      }
      const sz = statSync(full).size;
      bytes += sz;
      files++;
      const kind = e.name.replace(/[0-9a-f]{8,}\./, '*.');
      byKind[kind] = (byKind[kind] || 0) + sz;
    }
  };
  try {
    walk(dir);
  } catch {}
  return { bytes, files, byKind };
};

const rss = async () => {
  try {
    const d = await (await fetch(`${base}/api/diagnostics`, { signal: AbortSignal.timeout(5000) })).json();
    return d?.info?.process_rss_mb ?? -1;
  } catch {
    return -1;
  }
};

const kill = (pid) => {
  try {
    if (platform() === 'win32') spawnSync('taskkill', ['/PID', String(pid), '/T', '/F']);
    else process.kill(pid, 'SIGKILL');
  } catch {}
};

const server = spawn(EXE, ['--port', String(PORT), '--host', '127.0.0.1'], {
  env: { ...process.env, PM_APP_ROOT: STATE },
  stdio: 'ignore',
});
const cleanup = () => {
  kill(server.pid);
  try {
    rmSync(STATE, { recursive: true, force: true });
  } catch {}
};
process.on('exit', cleanup);
process.on('SIGINT', () => {
  cleanup();
  process.exit(130);
});

for (let i = 0; i < 60; i++) {
  try {
    if ((await fetch(`${base}/api/version`, { signal: AbortSignal.timeout(2000) })).ok) break;
  } catch {}
  await sleep(500);
}

// A download needs a bot; /api/provision mints a guest one.
try {
  const p = await (await fetch(`${base}/api/provision`, { signal: AbortSignal.timeout(120000) })).json();
  console.log(`provision : ok=${p.ok} bot=${p.bot_id || '-'} ${p.message ? `(${String(p.message).slice(0, 80)})` : ''}`);
} catch (e) {
  console.log(`provision : failed (${e.message})`);
}

const before = dirStats(CACHE);
const rssBefore = await rss();
console.log(`\nBEFORE   : rss=${rssBefore} MB  cache=${(before.bytes / 1024).toFixed(0)} KiB in ${before.files} files`);

let list;
try {
  list = await (await fetch(`${base}/stream/movie/${ITEM}.json`, { signal: AbortSignal.timeout(60000) })).json();
} catch (e) {
  console.log(`stream list failed: ${e.message}`);
  process.exit(2);
}
const pick = (list.streams || []).find((s) => s.url);
if (!pick) {
  console.log('no playable stream (locked / no session)');
  process.exit(2);
}

console.log(`streaming : ${RANGE / 1048576} MB …`);
const t0 = Date.now();
let peak = rssBefore;
const sampler = setInterval(async () => {
  const r = await rss();
  if (r > peak) peak = r;
}, 700);

const res = await fetch(pick.url, {
  headers: { Range: `bytes=0-${RANGE - 1}` },
  signal: AbortSignal.timeout(300000),
});
let got = 0;
const reader = res.body.getReader();
while (got < RANGE) {
  const { done, value } = await reader.read();
  if (done) break;
  got += value.length;
}
reader.cancel().catch(() => {});
clearInterval(sampler);
const secs = ((Date.now() - t0) / 1000).toFixed(1);

const rssAfter = await rss();
const after = dirStats(CACHE);
console.log(`\nDONE     : http=${res.status} ${(got / 1048576).toFixed(1)} MB in ${secs}s (${(got / 1024 / secs).toFixed(0)} KiB/s)`);
console.log(`RSS      : before=${rssBefore} MB peak=${peak} MB after=${rssAfter} MB  Δpeak=${peak - rssBefore} MB`);
console.log(`CACHE    : before=${(before.bytes / 1024).toFixed(0)} KiB  after=${(after.bytes / 1024).toFixed(0)} KiB  Δ=${((after.bytes - before.bytes) / 1024).toFixed(0)} KiB (${before.files} → ${after.files} files)`);
const kinds = Object.entries(after.byKind)
  .sort((a, b) => b[1] - a[1])
  .slice(0, 8)
  .map(([k, v]) => `${k}=${(v / 1024).toFixed(0)}K`)
  .join(' ');
if (kinds) console.log(`CACHE KINDS: ${kinds}`);

const grewMb = (after.bytes - before.bytes) / 1048576;
const ok = grewMb < RANGE / 1048576 / 8;
console.log(
  `\n${ok ? 'PASS' : 'FAIL'} — media bytes did not land in storage/cache ` +
    `(grew ${grewMb.toFixed(2)} MB for a ${RANGE / 1048576} MB transfer, only metadata is cached)`,
);
cleanup();
process.exit(ok ? 0 : 1);
