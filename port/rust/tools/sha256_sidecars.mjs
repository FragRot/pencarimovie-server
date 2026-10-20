// Write <file>.sha256 sidecars for every APK in a directory.
//
//   node port/rust/tools/sha256_sidecars.mjs <dir>
//
// Exists because passing a JS one-liner through a .bat file mangles the `%`
// and backslash characters cmd expands inside it (the harness turns
// `node -e "...%DIST%..."` into a syntax error). A file has no such problem.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
if (!dir) {
  console.error('usage: node sha256_sidecars.mjs <dir>');
  process.exit(2);
}

let n = 0;
for (const f of readdirSync(dir)) {
  if (!f.endsWith('.apk') && !f.endsWith('.zip') && !f.endsWith('.tar.gz')) continue;
  const full = join(dir, f);
  if (!statSync(full).isFile()) continue;
  const buf = readFileSync(full);
  const sha = createHash('sha256').update(buf).digest('hex');
  writeFileSync(`${full}.sha256`, `${sha}  ${f}\n`);
  console.log(`    ${f}  ${(buf.length / 1048576).toFixed(1)} MB  sha256 ${sha.slice(0, 16)}…`);
  n++;
}
if (n === 0) {
  console.error(`no artifacts found in ${dir}`);
  process.exit(1);
}
