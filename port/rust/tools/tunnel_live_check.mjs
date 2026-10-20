#!/usr/bin/env node
/**
 * tunnel_live_check.mjs — start a real server, enable a REAL Cloudflare tunnel,
 * print the JSON answer plus the cloudflared log tail, then clean up.
 *
 * This is the only check that proves the enable/disable path end to end: the stub
 * harness (probe_tunnel.mjs) cannot tell a published URL from a CONNECTED tunnel,
 * and that difference is exactly what broke on Android.
 *
 * Usage:
 *   node port/rust/tools/tunnel_live_check.mjs [--exe <path>] [--port 8096]
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : dflt;
};

const ROOT = resolve(import.meta.dirname, '..');
const EXE = resolve(arg('exe', join(ROOT, 'dist-rust', 'windows-x86_64', 'pencarimovie-rust.exe')));
const PORT = Number(arg('port', '8096'));
const STATE = join(tmpdir(), `pmtunnel-${Date.now()}`);

if (!existsSync(EXE)) {
  console.error(`missing ${EXE} — run scripts\\build-rust-windows.bat first`);
  process.exit(2);
}
mkdirSync(STATE, { recursive: true });

const base = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const post = async (path, body = '{}') => {
  const r = await fetch(`${base}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
    signal: AbortSignal.timeout(200000),
  });
  return r.json();
};

let server;
const kill = (pid) => {
  try {
    if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(pid), '/T', '/F']);
    else process.kill(pid, 'SIGKILL');
  } catch {}
};

const cleanup = () => {
  try {
    kill(server.pid);
  } catch {}
  try {
    const st = JSON.parse(readFileSync(join(STATE, 'storage', 'tunnel', 'state.json'), 'utf8'));
    if (st.pid) kill(st.pid);
  } catch {}
  try {
    rmSync(STATE, { recursive: true, force: true });
  } catch {}
};

server = spawn(EXE, ['--port', String(PORT), '--host', '127.0.0.1'], {
  env: { ...process.env, PM_APP_ROOT: STATE },
  stdio: 'ignore',
});
process.on('exit', cleanup);
process.on('SIGINT', () => {
  cleanup();
  process.exit(130);
});

const up = async () => {
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(`${base}/api/version`, { signal: AbortSignal.timeout(2000) })).ok) return true;
    } catch {}
    await sleep(500);
  }
  return false;
};

if (!(await up())) {
  console.error('server never came up');
  cleanup();
  process.exit(2);
}

console.log(`exe    : ${EXE}`);
console.log(`state  : ${STATE}`);

// `/api/tunnel/enable` now answers immediately with `starting: true` and the work
// continues in the background, because a cold enable takes 10-90 s and the
// dashboard polls /api/tunnel/status for it. Poll the same way the UI does.
const t0 = Date.now();
const accepted = await post('/api/tunnel/enable');
console.log(`\nenable accepted : ${JSON.stringify(accepted)}`);

let res = accepted;
let status = null;
for (let i = 0; i < 120; i++) {
  await sleep(1000);
  status = await (await fetch(`${base}/api/tunnel/status`)).json();
  if (status.enabled && status.tunnel_url) {
    res = { ...status, ok: 1 };
    break;
  }
  if (!status.starting && i > 3) {
    res = { ...status, ok: 0 };
    break;
  }
}
const secs = ((Date.now() - t0) / 1000).toFixed(1);

console.log(`connected : ${secs}s`);
console.log(JSON.stringify(res, null, 2));

const logPath = join(STATE, 'storage', 'tunnel', 'cloudflared.log');
if (existsSync(logPath)) {
  const text = readFileSync(logPath, 'utf8');
  const registered = /registered tunnel connection/i.test(text);
  console.log(`\nconnector registered : ${registered}`);
  console.log(`log tail             : ${text.trim().split('\n').slice(-3).join(' | ').slice(-500)}`);
}

const status = await (await fetch(`${base}/api/tunnel/status`)).json();
console.log(`\nstatus : enabled=${status.enabled} running=${status.running} url=${status.tunnel_url || '-'}`);

const disabled = await post('/api/tunnel/disable');
console.log(`disable: ok=${disabled.ok} enabled=${disabled.enabled}`);

cleanup();
const ok = res.ok === 1 && res.enabled === true;
console.log(`\n${ok ? 'PASS' : 'FAIL'} — enable returned ${ok ? 'a connected tunnel' : 'a failure (see message)'}`);
process.exit(ok ? 0 : 1);
