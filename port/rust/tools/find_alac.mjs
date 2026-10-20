// Find a real ALAC track in the Eclipse catalogue.
//   node port/rust/tools/find_alac.mjs [base]
//
// Many Telegram "m4a" files are actually AAC (`mp4a` in `stsd`), which Android
// decodes natively and which this server deliberately leaves on the raw
// download path. ALAC carries an `alac` sample-entry box instead, so the only
// reliable test is to read the container header of the real file.

const base = process.argv[2] || 'http://127.0.0.1:8088';
const queries = ['alac', 'flac', 'lossless', 'adele', 'hires', '24bit', 'm4a', 'audiophile'];

async function search(q) {
  const r = await fetch(`${base}/eclipse/search?q=${encodeURIComponent(q)}`, {
    signal: AbortSignal.timeout(60000),
  });
  if (!r.ok) return [];
  const j = await r.json();
  return Array.isArray(j) ? j : j.tracks || [];
}

async function containerTags(shortCode) {
  const r = await fetch(`${base}/eclipse/stream/${shortCode}`, { signal: AbortSignal.timeout(60000) });
  if (!r.ok) return null;
  const j = await r.json();
  if (!j.url) return null;
  const head = await fetch(j.url, {
    headers: { Range: 'bytes=0-65535' },
    signal: AbortSignal.timeout(120000),
  });
  const buf = Buffer.from(await head.arrayBuffer());
  const find = (tag) => buf.indexOf(Buffer.from(tag));
  return {
    url: j.url,
    format: j.format,
    mime: j.mimeType,
    ftyp: find('ftyp') >= 0,
    mp4a: find('mp4a') >= 0,
    alac: find('alac') >= 0,
    flac: buf.slice(0, 4).toString('binary') === 'fLaC',
    id3: buf.slice(0, 3).toString('binary') === 'ID3',
  };
}

const seen = new Set();
for (const q of queries) {
  const tracks = await search(q);
  const candidates = tracks.filter((t) => (t.format === 'wav' || t.format === 'flac') && !seen.has(t.id));
  console.log(`\n[${q}] ${tracks.length} tracks, ${candidates.length} audio candidates`);
  for (const t of candidates.slice(0, 12)) {
    seen.add(t.id);
    let info;
    try {
      info = await containerTags(t.id);
    } catch (e) {
      console.log(`  ${t.id} ${t.artist} - ${t.title}: ${e.message}`);
      continue;
    }
    if (!info) {
      console.log(`  ${t.id} ${t.artist} - ${t.title}: no stream url`);
      continue;
    }
    const kind = info.alac ? 'ALAC' : info.mp4a ? 'AAC' : info.flac ? 'FLAC' : info.id3 ? 'MP3' : '?';
    console.log(`  ${t.id} ${kind.padEnd(4)} ${t.artist} - ${t.title}`);
    if (info.alac) {
      console.log(`\nALAC FOUND: ${t.id} ${t.artist} - ${t.title}\n  ${info.url}`);
      process.exit(0);
    }
  }
}
console.log('\nNo ALAC track found in the sampled catalogue.');
process.exit(1);
