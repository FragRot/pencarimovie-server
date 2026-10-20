// Probe a single /api/download URL the way a player does:
//   node port/rust/tools/probe_download.mjs "<url>" [bytes]
//
// Prints HEAD, a bounded Range read, and a bounded open-ended read (what the
// player does on startup). Timing is wall-clock to first byte and total.

const url = process.argv[2];
const windowBytes = Number(process.argv[3] || 1048576);
if (!url) {
  console.error('usage: node probe_download.mjs <url> [bytes]');
  process.exit(2);
}

const fmt = (n) => `${(n / 1024).toFixed(1)} KiB`;

async function head() {
  try {
    const r = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(15000) });
    const h = Object.fromEntries(r.headers.entries());
    console.log(`HEAD -> ${r.status}`);
    for (const k of ['content-type', 'content-length', 'accept-ranges', 'content-range', 'content-disposition', 'cache-control']) {
      if (h[k] !== undefined) console.log(`  ${k}: ${h[k]}`);
    }
  } catch (e) {
    console.log(`HEAD -> FAILED: ${e.message}`);
  }
}

async function range(bytes) {
  const t0 = Date.now();
  let firstByteMs = null;
  let total = 0;
  try {
    const r = await fetch(url, {
      headers: { Range: `bytes=0-${bytes - 1}` },
      signal: AbortSignal.timeout(120000),
    });
    console.log(`RANGE 0-${bytes - 1} -> ${r.status} ${r.headers.get('content-type')} range=${r.headers.get('content-range')}`);
    const reader = r.body.getReader();
    while (total < bytes) {
      const { done, value } = await reader.read();
      if (done) break;
      if (firstByteMs === null) firstByteMs = Date.now() - t0;
      total += value.length;
    }
    reader.cancel().catch(() => {});
  } catch (e) {
    console.log(`RANGE -> FAILED after ${fmt(total)}: ${e.message}`);
  }
  const ms = Date.now() - t0;
  console.log(`  got ${fmt(total)} in ${ms} ms (first byte ${firstByteMs ?? '-'} ms, ${(total / 1024 / (ms / 1000 || 1)).toFixed(0)} KiB/s)`);
}

async function openEnded(seconds) {
  const t0 = Date.now();
  let firstByteMs = null;
  let total = 0;
  try {
    const r = await fetch(url, { signal: AbortSignal.timeout(seconds * 1000) });
    console.log(`GET (no Range) -> ${r.status} ${r.headers.get('content-type')} len=${r.headers.get('content-length')}`);
    const reader = r.body.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (firstByteMs === null) firstByteMs = Date.now() - t0;
      total += value.length;
    }
  } catch (e) {
    console.log(`GET -> stopped after ${fmt(total)} in ${Date.now() - t0} ms (${e.name}: ${e.message})`);
  }
  const ms = Date.now() - t0;
  console.log(`  got ${fmt(total)} in ${ms} ms (first byte ${firstByteMs ?? '-'} ms, ${(total / 1024 / (ms / 1000 || 1)).toFixed(0)} KiB/s)`);
}

await head();
console.log('');
await range(windowBytes);
console.log('');
await openEnded(20);
