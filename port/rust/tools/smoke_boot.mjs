// Boot a pencarimovie-rust binary from cold and probe it, then shut it down.
//
//   node port/rust/tools/smoke_boot.mjs <binary> [port] [app-root]
//
// Proves the "one executable, no PHP, no public/, no pre-made storage" claim:
// a fresh temp app root is used unless one is passed, so this is a real
// first-run experience (the server has to create storage/ itself).
//
// Node only (global fetch, no deps). No PowerShell anywhere. Always exits, and
// always kills the process tree, so it can never hang a terminal.

import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const bin = process.argv[2];
if (!bin) {
  console.error('usage: node smoke_boot.mjs <binary> [port] [app-root]');
  process.exit(2);
}
const binPath = resolve(bin);
if (!existsSync(binPath)) {
  console.error(`missing binary: ${binPath}`);
  process.exit(2);
}
const port = Number(process.argv[3] || 8099);
const freshRoot = !process.argv[4];
const root = process.argv[4] ? resolve(process.argv[4]) : mkdtempSync(join(tmpdir(), 'pm-rust-smoke-'));
mkdirSync(root, { recursive: true });
const logPath = join(root, 'server.log');
const base = `http://127.0.0.1:${port}`;

let pass = 0;
let fail = 0;
const check = (name, ok, detail = '') => {
  if (ok) pass++;
  else fail++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ${detail}` : ''}`);
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function req(path, opts = {}) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), opts.timeoutMs || 30000);
  const t0 = Date.now();
  try {
    const r = await fetch(base + path, { ...opts, signal: ac.signal });
    const buf = Buffer.from(await r.arrayBuffer());
    return { status: r.status, headers: r.headers, body: buf, ms: Date.now() - t0 };
  } catch (e) {
    return { status: 0, headers: new Headers(), body: Buffer.alloc(0), ms: Date.now() - t0, error: String(e.message || e) };
  } finally {
    clearTimeout(t);
  }
}

function killTree() {
  if (!proc || proc.exitCode !== null) return;
  try {
    if (process.platform === 'win32') {
      spawn('taskkill', ['/F', '/T', '/PID', String(proc.pid)], { stdio: 'ignore' });
    } else {
      proc.kill('SIGKILL');
    }
  } catch {
    /* best effort */
  }
}

console.log(`binary  : ${binPath}`);
console.log(`app root: ${root}${freshRoot ? '  (fresh temp dir)' : ''}`);
console.log(`port    : ${port}`);
console.log('');

const out = [];
const proc = spawn(binPath, ['--port', String(port)], {
  cwd: root,
  env: { ...process.env, PM_APP_ROOT: root },
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: process.platform !== 'win32',
  windowsHide: true,
});
proc.stdout.on('data', (d) => out.push(String(d)));
proc.stderr.on('data', (d) => out.push(String(d)));
proc.on('exit', (code) => out.push(`[exit ${code}]`));

for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => { killTree(); process.exit(130); });

try {
  // --- 1. cold boot -------------------------------------------------------
  let up = false;
  for (let i = 0; i < 60; i++) {
    if (proc.exitCode !== null) break;
    const v = await req('/api/version', { timeoutMs: 3000 });
    if (v.status === 200) { up = true; break; }
    await sleep(250);
  }
  const versionText = out.join('').match(/pencarimovie-rust[^\r\n]*/i)?.[0] || '';
  check('cold boot /api/version', up, up ? `${versionText.trim()}` : 'never answered');
  if (!up) {
    console.log('\n--- server output ---');
    console.log(out.join('').split('\n').slice(-40).join('\n'));
    writeFileSync(logPath, out.join(''));
    process.exit(1);
  }

  const version = await req('/api/version');
  const vj = JSON.parse(version.body.toString() || '{}');
  const ver = vj.current_version || vj.version || '';
  check('version payload', version.status === 200 && !!ver && vj.update_needed === false,
    `current=${ver} min=${vj.minimum_version || '-'} update_needed=${vj.update_needed}`);

  // --- 2. embedded dashboard --------------------------------------------
  const index = await req('/');
  const html = index.body.toString('utf8');
  check('/ serves dashboard', index.status === 200 && /<html/i.test(html),
    `${index.status} ${index.headers.get('content-type')} ${index.body.length} B in ${index.ms} ms`);
  check('assets are embedded (no public/ dir)', !existsSync(join(root, 'public')), 'public/ absent next to the exe');
  const js = await req('/app.js');
  check('/app.js embedded', js.status === 200 && js.body.length > 1000, `${js.status} ${js.body.length} B`);
  const css = await req('/css/bundle.css');
  check('/css/bundle.css embedded', css.status === 200 && css.body.length > 1000, `${css.status} ${css.body.length} B`);

  // --- 3. addon descriptor ----------------------------------------------
  const man = await req('/manifest.json');
  const mj = JSON.parse(man.body.toString() || '{}');
  check('/manifest.json', man.status === 200 && Array.isArray(mj.catalogs),
    `${man.status} ${man.body.length} B catalogs=${(mj.catalogs || []).length} resources=${JSON.stringify(mj.resources || [])}`);
  check('descriptor under Stremio cap', man.body.length < 45000, `${man.body.length} B < 45000 B`);

  // --- 4. alias + 404 contract ------------------------------------------
  const alias = await req('/nuvio/manifest.json');
  check('/nuvio/manifest.json alias', alias.status === 200 && alias.body.length === man.body.length, `${alias.status} ${alias.body.length} B`);
  const nf = await req('/api/definitely-not-a-route');
  check('unknown /api/* 404s as JSON', nf.status === 404 && nf.body.toString().trimStart().startsWith('{'), `${nf.status} ${nf.body.toString().slice(0, 60)}`);

  // --- 5. catalog -------------------------------------------------------
  const cat = await req('/catalog/movie/pm_movies_top.json', { timeoutMs: 60000 });
  const cj = JSON.parse(cat.body.toString() || '{}');
  const metas = Array.isArray(cj.metas) ? cj.metas : (cj.metas?.metas || []);
  check('/catalog rows', cat.status === 200 && metas.length > 0, `${cat.status} ${metas.length} metas ${cat.body.length} B in ${cat.ms} ms`);

  // --- 6. first-run state machine (no session yet) ----------------------
  const ses = await req('/api/session');
  const sj = JSON.parse(ses.body.toString() || '{}');
  check('/api/session first run', ses.status === 200 && sj.has_session === false, `has_session=${sj.has_session} env_ok=${sj.environment_ok}`);
  const auth = await req('/api/auth/status');
  const aj = JSON.parse(auth.body.toString() || '{}');
  check('/api/auth/status', auth.status === 200 && aj.authenticated === true, `local bypass authenticated=${aj.authenticated}`);
  check('storage/ created by the binary', existsSync(join(root, 'storage')), join(root, 'storage'));

  // --- 7. gates ---------------------------------------------------------
  const streams = await req('/stream/movie/tt4154756.json', { timeoutMs: 60000 });
  const stj = JSON.parse(streams.body.toString() || '{}');
  check('/stream with no bot answers a card', streams.status === 200 && (stj.streams || []).length >= 1,
    `${(stj.streams || []).length} stream(s): ${String(stj.streams?.[0]?.description || '').split('\n')[0].slice(0, 60)}`);
  // No payload at all: this must be a JSON 400, not a crash and not the
  // dashboard. Playback itself is covered by e2e_matrix.mjs against a rooted
  // install, because a well-formed payload legitimately tries to provision a
  // guest bot (15 s + a new Telegram bot), which a cold-start smoke must not do.
  const dl = await req('/api/download');
  check('/api/download validates its payload', dl.status === 400 && dl.body.toString().trimStart().startsWith('{'),
    `HTTP ${dl.status} ${dl.body.toString().slice(0, 70)}`);
} catch (e) {
  fail++;
  console.log(`FAIL  harness: ${e.message}`);
} finally {
  killTree();
  await sleep(700);
  writeFileSync(logPath, out.join(''));
  if (freshRoot) {
    // keep the log, drop the rest so the temp dir does not accumulate
    console.log(`\nserver log: ${logPath}`);
  } else {
    console.log(`\nserver log: ${logPath}`);
  }
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail === 0 ? 0 : 1);
}
