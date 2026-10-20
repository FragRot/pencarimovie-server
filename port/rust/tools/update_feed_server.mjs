// Local update-feed harness for the Android self-updater.
//
//   node port/rust/tools/update_feed_server.mjs make  --dir DIR --apk FILE --code 10050 --name 1.0.50 [--abi x86]
//   node port/rust/tools/update_feed_server.mjs serve --dir DIR [--port 8099]
//
// `make` writes a `version.json` in the exact shape `android/release.ps1` publishes
// (`applicationId`, `versionName`, `versionCode`, `apks{abi:{url,sha256}}`, `apkUrl`,
// `sha256`) pointing at the local HTTP server, with the APK's real sha256 — so the
// app's hash check is exercised rather than bypassed.
//
// `serve` is a dependency-free static server. Test on a device with:
//   adb reverse tcp:8099 tcp:8099
//   adb shell am start -n com.pencarimovie.server/.MainActivity -e update_feed \
//       http://127.0.0.1:8099/version.json
//
// Remember to `adb reverse --remove tcp:8099` afterwards, and note that the override
// is persisted in `pencarimovie_server` prefs until it is cleared.
import { createHash } from 'node:crypto';
import { createReadStream, existsSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { basename, extname, join } from 'node:path';

const argv = process.argv.slice(2);
const mode = argv[0];
const arg = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : dflt;
};

if (mode === 'make') {
  const dir = arg('dir', 'storage/_update_feed');
  const apk = arg('apk', '');
  const code = Number(arg('code', '10050'));
  const name = arg('name', '1.0.50');
  const abi = arg('abi', 'x86');
  const port = arg('port', '8099');
  if (!apk || !existsSync(apk)) {
    console.error(`missing APK: ${apk}`);
    process.exit(2);
  }
  const sha = createHash('sha256').update(readFileSync(apk)).digest('hex');
  const asset = basename(apk);
  const url = `http://127.0.0.1:${port}/${asset}`;
  const doc = {
    applicationId: 'com.pencarimovie.server',
    versionName: name,
    versionCode: code,
    apks: { [abi]: { url, sha256: sha }, universal: { url, sha256: sha } },
    apkUrl: url,
    sha256: sha,
    builtAt: new Date().toISOString().replace(/\.\d+Z$/, 'Z'),
    commit: 'local-test',
  };
  const out = join(dir, 'version.json');
  // No BOM: the app parses it with JSONObject, which rejects one.
  writeFileSync(out, JSON.stringify(doc, null, 2), 'utf8');
  console.log(`feed written: ${out}`);
  console.log(`  apk    : ${asset} (${statSync(apk).size} bytes)`);
  console.log(`  sha256 : ${sha}`);
  console.log(`  url    : ${url}`);
  console.log(`  code   : ${code} (${name})`);
} else if (mode === 'serve') {
  const dir = arg('dir', 'storage/_update_feed');
  const port = Number(arg('port', '8099'));
  const types = {
    '.json': 'application/json',
    '.apk': 'application/vnd.android.package-archive',
    '.sha256': 'text/plain',
  };
  createServer((req, res) => {
    const name = decodeURIComponent((req.url || '/').split('?')[0]).replace(/^\/+/, '') || 'version.json';
    const file = join(dir, name);
    if (!existsSync(file) || !statSync(file).isFile()) {
      console.log(`404 ${name}`);
      res.writeHead(404).end('not found');
      return;
    }
    const size = statSync(file).size;
    console.log(`200 ${name} (${size} bytes)`);
    res.writeHead(200, {
      'Content-Type': types[extname(file)] || 'application/octet-stream',
      'Content-Length': String(size),
      'Cache-Control': 'no-store',
    });
    createReadStream(file).pipe(res);
  }).listen(port, '127.0.0.1', () => console.log(`serving ${dir} on http://127.0.0.1:${port}`));
} else {
  console.error('usage: update_feed_server.mjs make|serve --dir DIR [--apk FILE] [--code N] [--port N]');
  process.exit(2);
}
