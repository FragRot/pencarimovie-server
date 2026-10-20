// Probe an ALAC→WAV download URL the way a music client does.
//   node port/rust/tools/probe_alac.mjs "<url>"
//
// Asserts the shape the Android/Eclipse player needs:
//   * HEAD 200, `audio/wav`, `Accept-Ranges: bytes`, `Content-Length = 44 + pcm`
//   * `Range: bytes=0-…` answers 206 with a `RIFF`/`WAVE`/`fmt `/`data` header
//   * a range that starts inside the header emits the header prefix first
//   * a range deep into the track answers 206 with the matching Content-Range
//     and 16-bit sample bytes (no second RIFF header)

const url = process.argv[2];
if (!url) {
  console.error('usage: node probe_alac.mjs <url>');
  process.exit(2);
}

let failures = 0;
const ok = (cond, label, extra = '') => {
  if (!cond) failures++;
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${label}${extra ? `  ${extra}` : ''}`);
};

const u16 = (b, at) => b[at] | (b[at + 1] << 8);
const u32 = (b, at) => (b[at] | (b[at + 1] << 8) | (b[at + 2] << 16) | (b[at + 3] << 24)) >>> 0;
const ascii = (b, at, n) => String.fromCharCode(...b.slice(at, at + n));

async function probeRange(rangeHeader, want) {
  const t0 = Date.now();
  const r = await fetch(url, {
    headers: { Range: rangeHeader },
    signal: AbortSignal.timeout(120000),
  });
  const buf = new Uint8Array(await r.arrayBuffer());
  const ms = Date.now() - t0;
  const h = Object.fromEntries(r.headers.entries());
  console.log(
    `\n${rangeHeader} -> ${r.status} ${h['content-type']} range=${h['content-range']} ` +
      `len=${h['content-length']} in ${ms} ms`,
  );
  if (want !== undefined) ok(buf.length === want, `body length ${want}`, `got ${buf.length}`);
  return { status: r.status, headers: h, bytes: buf };
}

/* ── HEAD ──────────────────────────────────────────────────────────────── */
const head = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(30000) });
const hh = Object.fromEntries(head.headers.entries());
console.log(`HEAD -> ${head.status} ${hh['content-type']} len=${hh['content-length']} ranges=${hh['accept-ranges']}`);
ok(head.status === 200, 'HEAD 200');
ok(hh['content-type'] === 'audio/wav', 'content-type audio/wav', hh['content-type']);
ok(hh['accept-ranges'] === 'bytes', 'accept-ranges bytes', hh['accept-ranges']);
const total = Number(hh['content-length'] || 0);
ok(total > 44, 'content-length > 44', String(total));

/* ── header range ──────────────────────────────────────────────────────── */
const first = await probeRange('bytes=0-1023', 1024);
ok(first.status === 206, 'header range is 206');
ok(first.headers['content-range'] === `bytes 0-1023/${total}`, 'Content-Range 0-1023', first.headers['content-range']);
ok(ascii(first.bytes, 0, 4) === 'RIFF', 'RIFF magic', [...first.bytes.slice(0, 4)].map((b) => b.toString(16)).join(' '));
ok(ascii(first.bytes, 8, 4) === 'WAVE', 'WAVE magic');
ok(ascii(first.bytes, 12, 4) === 'fmt ', 'fmt chunk');
ok(u32(first.bytes, 16) === 16, 'fmt chunk size 16');
ok(u16(first.bytes, 20) === 1, 'PCM format tag');
const channels = u16(first.bytes, 22);
const sampleRate = u32(first.bytes, 24);
const byteRate = u32(first.bytes, 28);
const blockAlign = u16(first.bytes, 32);
ok(u16(first.bytes, 34) === 16, 'bits per sample 16');
ok(blockAlign === channels * 2, 'block align = channels*2', `${blockAlign} vs ${channels * 2}`);
ok(byteRate === sampleRate * blockAlign, 'byte rate = rate*blockAlign', `${byteRate} vs ${sampleRate * blockAlign}`);
const dataLen = u32(first.bytes, 40);
const bytesPerFrame = blockAlign;
const expectPcm = Math.round(((total - 44) / bytesPerFrame) * bytesPerFrame);
console.log(
  `\n${channels}ch @ ${sampleRate} Hz, ${bytesPerFrame} B/frame, data=${dataLen} B ` +
    `(${(dataLen / bytesPerFrame / sampleRate).toFixed(1)} s), total=${total} B`,
);
ok(dataLen === total - 44, 'data length matches content-length - 44', `${dataLen} vs ${total - 44}`);
ok(u32(first.bytes, 4) === 36 + dataLen, 'RIFF size = 36 + data');

/* ── range inside the header ───────────────────────────────────────────── */
// 10-49 spans the tail of the header (bytes 10-43) plus the first 6 PCM bytes:
// the header prefix must be emitted first, then decoding continues from PCM 0.
const inside = await probeRange('bytes=10-49', 40);
ok(inside.status === 206, 'inside-header range is 206');
ok(ascii(inside.bytes, 0, 2) === 'VE', 'starts mid-"WAVE"', ascii(inside.bytes, 0, 2));
const headerTail = first.bytes.slice(10, 44);
ok(
  Buffer.from(inside.bytes.slice(0, 34)).equals(Buffer.from(headerTail)),
  'header prefix bytes 10-43 are emitted verbatim',
);
const lead = first.bytes.slice(44, 50);
ok(
  Buffer.from(inside.bytes.slice(34)).equals(Buffer.from(lead)),
  'then the first 6 PCM bytes match an open range',
  `${inside.bytes.slice(34, 40).toString('hex')} vs ${Buffer.from(lead).toString('hex')}`,
);
ok(inside.headers['content-range'] === `bytes 10-49/${total}`, 'Content-Range 10-49', inside.headers['content-range']);

/* ── range deep into the track ─────────────────────────────────────────── */
const midStart = Math.floor(total / 2 / bytesPerFrame) * bytesPerFrame;
const midEnd = midStart + 4095;
const mid = await probeRange(`bytes=${midStart}-${midEnd}`, 4096);
ok(mid.status === 206, 'mid range is 206');
ok(
  mid.headers['content-range'] === `bytes ${midStart}-${midEnd}/${total}`,
  'Content-Range mid',
  mid.headers['content-range'],
);
ok(ascii(mid.bytes, 0, 4) !== 'RIFF', 'mid range is not a second WAV header', ascii(mid.bytes, 0, 4));
ok(mid.bytes.some((b) => b !== 0), 'mid range is not silence');

/* ── suffix range ──────────────────────────────────────────────────────── */
const tailRange = await probeRange('bytes=-2048', 2048);
ok(tailRange.status === 206, 'suffix range is 206');
ok(
  tailRange.headers['content-range'] === `bytes ${total - 2048}-${total - 1}/${total}`,
  'Content-Range suffix',
  tailRange.headers['content-range'],
);

console.log(`\n${failures === 0 ? 'ALL CHECKS PASSED' : `${failures} CHECK(S) FAILED`}`);
process.exit(failures === 0 ? 0 : 1);
