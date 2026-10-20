// Bot-pool + stream-list matrix against a running Rust port.
//
//   node port/rust/tools/e2e_matrix.mjs [base]
//
// Node only (global fetch, no deps) so there is no PowerShell in the loop.
// Read-only except for a set-active round-trip that switches to a non-active bot
// and immediately switches back, so the active session that streaming needs is
// preserved.

const base = process.argv[2] || 'http://127.0.0.1:8088';

async function getJson(path, timeoutMs = 90000) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), timeoutMs);
  try {
    const r = await fetch(base + path, { signal: ac.signal });
    const text = await r.text();
    let body = null;
    try { body = JSON.parse(text); } catch { body = text; }
    return { status: r.status, body };
  } catch (e) {
    return { status: 0, body: String(e.message || e) };
  } finally {
    clearTimeout(t);
  }
}

async function postJson(path, obj) {
  const ac = new AbortController();
  const t = setTimeout(() => ac.abort(), 60000);
  try {
    const r = await fetch(base + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(obj),
      signal: ac.signal,
    });
    const text = await r.text();
    let body = null;
    try { body = JSON.parse(text); } catch { body = text; }
    return { status: r.status, body };
  } catch (e) {
    return { status: 0, body: String(e.message || e) };
  } finally {
    clearTimeout(t);
  }
}

const line = (s) => console.log(s);

async function botPool() {
  line('== BOT POOL ==');
  const { status, body } = await getJson('/api/bots');
  if (status !== 200) { line(`  /api/bots -> HTTP ${status}`); return null; }
  line(`  total=${body.total_bots} active=${body.active_bot_id}`);
  for (const b of body.bots) {
    line(`  ${b.bot_id}  ${String(b.bot_username).padEnd(32)} session=${b.has_session} active=${b.is_active} status=${b.status}`);
  }
  const add = await postJson('/api/bots/add', {});
  line(`  add(empty)          -> ${add.status} ${add.body.message ?? ''}`);
  const setActive = await postJson('/api/bots/set-active', { bot_id: '0' });
  line(`  set-active(0)       -> ${setActive.status} ${setActive.body.message ?? ''}`);
  const remove = await postJson('/api/bots/remove', { bot_id: '' });
  line(`  remove(empty)       -> ${remove.status} ${remove.body.message ?? ''}`);

  const other = body.bots.find((b) => !b.is_active);
  if (other) {
    const to = await postJson('/api/bots/set-active', { bot_id: other.bot_id });
    line(`  set-active(${other.bot_id}) -> ${to.status} ok=${to.body.ok}`);
    const back = await postJson('/api/bots/set-active', { bot_id: body.active_bot_id });
    line(`  set-active(back)    -> ${back.status} ok=${back.body.ok}`);
  }
  return body;
}

async function authChecks() {
  line('');
  line('== PASSWORD / TOKEN ==');
  const st = await getJson('/api/auth/status');
  line(`  /api/auth/status    -> ${st.status} enabled=${st.body.enabled} authenticated=${st.body.authenticated} token_len=${(st.body.token || '').length}`);
  // The default password is the documented default; a SUCCESSFUL login resets the
  // failure ladder, so this does not push us toward a lockout.
  const login = await postJson('/api/auth/login', { password: '123456' });
  line(`  /api/auth/login(default) -> ${login.status} ok=${login.body.ok}`);
  // Tokenised path form: /<token>/api/version must equal /api/version.
  const tok = st.body.token;
  if (tok) {
    const plain = await getJson('/api/version');
    const viaTok = await getJson('/' + tok + '/api/version');
    const same = JSON.stringify(viaTok.body) === JSON.stringify(plain.body);
    line(`  /<token>/api/version -> ${viaTok.status} same_body=${same}`);
    // Loopback is trusted by design (`fd_is_authenticated` returns true for
    // 127.0.0.1), so a bad token from THIS machine still answers 200. The token
    // check only bites for a public client, which cannot be simulated locally.
    const badTok = await getJson('/ffffffffffffffffffffffffffffffff/api/version');
    line(`  bad token (loopback) -> ${badTok.status} (200 expected: loopback bypasses auth)`);
  }
}

async function catalogSettings() {
  line('');
  line('== CATALOG CONFIGURATION ==');
  const { status, body } = await getJson('/api/catalog-settings');
  if (status !== 200) { line(`  /api/catalog-settings -> HTTP ${status}`); return; }
  // The document lives under `settings` (PHP shape), not at the top level.
  const s = body.settings || {};
  const sc = s.stream_config || {};
  line(`  settings keys       = ${Object.keys(s).join(',') || '(empty)'}`);
  line(`  catalogs_enabled    = ${s.catalogs_enabled}`);
  line(`  upstream_enabled    = ${s.upstream_enabled}`);
  line(`  enabled_types       = ${JSON.stringify(s.enabled_types ?? null)}`);
  line(`  upstream_manifests  = ${Array.isArray(s.upstream_manifests) ? s.upstream_manifests.length : 'n/a'}`);
  line(`  catalog_options     = ${JSON.stringify(body.catalog_options ?? null)}`);
  line(`  stream_config keys  = ${Object.keys(sc).length}`);
  line(`  required_keywords   = "${sc.required_keywords ?? ''}"   <-- non-empty hides every non-matching release`);
  line(`  excluded_keywords   = "${sc.excluded_keywords ?? ''}"`);
  line(`  exclude_part_title  = ${sc.exclude_part_title}   exclude_unplayable = ${sc.exclude_unplayable}`);
  line(`  resolutions         = ${JSON.stringify(sc.resolutions ?? null)}`);
  line(`  preferred_resolution= ${sc.preferred_resolution}   max_total = ${sc.max_streams_total}`);
}

// Pinned by the episode-name guard work: these two must never drift, because a
// name that disagrees with the file tail may only remove a WRONG sibling, never
// a legitimate release.
const EXACT_STREAMS = {
  'pm:post:9000016669:1:1': 7, // Kudrat S1E1 (identity unverified -> fail open)
  'tt11540908:1:1': 15, // Carrie S1E1, provider name "FIRST PERIOD"
};

let failures = 0;
function assert(label, ok, detail) {
  line(`  ${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? ` — ${detail}` : ''}`);
  if (!ok) failures += 1;
}

async function streams() {
  line('');
  line('== STREAM LISTS ==');
  const cases = [
    ['movie', 'pm:post:9000031194'],
    ['movie', 'pm:post:8591885201'],
    ['movie', 'pm:post:9001801064'],
    ['series', 'pm:post:9000016669:1:1'],
    ['series', 'tt11540908:1:1'],
    ['movie', 'tt34384900'],
    ['movie', 'tt27369017'],
    ['movie', 'pm_file_1v9OPj'],
    ['movie', 'tt99999999'],
  ];
  for (const [type, id] of cases) {
    const started = Date.now();
    const { status, body } = await getJson(`/stream/${type}/${encodeURIComponent(id)}.json`);
    const ms = Date.now() - started;
    const n = body && Array.isArray(body.streams) ? body.streams.length : -1;
    let first = '';
    if (n > 0) {
      const d = String(body.streams[0].description || '').split('\n');
      first = d[d.length - 1];
    }
    line(`${id.padEnd(26)} ${String(n).padStart(4)} streams ${String(ms).padStart(6)}ms  ${first}`);
    if (Object.prototype.hasOwnProperty.call(EXACT_STREAMS, id)) {
      assert(`${id} pinned to ${EXACT_STREAMS[id]}`, n === EXACT_STREAMS[id], `${n}`);
    }
  }
}

// Task A: a bridged catalog row carrying `tvdb:` / `anilist:` / `tvmaze:` must
// produce a LOCAL stream list. Before `parse_external_id()` knew those prefixes
// the local path returned `None`, the route answered the empty card, and the
// user saw a play button that led nowhere.
async function externalPrefixStreams() {
  line('');
  line('== EXTERNAL ID PREFIXES (tvdb / anilist / tvmaze) ==');
  const cases = [
    ['tvdb:81797:1:1', 90], // One Piece 1999 (capped at 100 + sponsor)
    ['anilist:21:1:1', 90], // One Piece 1999 via the AniList id
    // Game of Thrones. 38 streams measured: the episode-name guard drops the
    // S01E01 files whose tail is NOT "winter is coming" (`…Steelbook.Edition…`,
    // `…S01.E01.05.COMBiNED…`). That is the SAME result the PHP produces — it
    // runs the identical stop-list/annotation rules — so the floor only asserts
    // "a real local list, not the empty card".
    ['tvmaze:82:1:1', 30],
  ];
  for (const [id, floor] of cases) {
    const started = Date.now();
    const { status, body } = await getJson(`/stream/series/${encodeURIComponent(id)}.json`);
    const ms = Date.now() - started;
    const list = body && Array.isArray(body.streams) ? body.streams : [];
    const playable = list.filter((s) => s && s.url).length;
    const firstLocal = list.find((s) => s && s.url);
    const name = firstLocal ? String(firstLocal.description || '').split('\n')[0] : '(none)';
    line(`${id.padEnd(20)} ${String(list.length).padStart(4)} streams (${playable} playable) ${String(ms).padStart(6)}ms  ${name}`);
    assert(`${id} status 200`, status === 200, String(status));
    assert(`${id} has a real local stream list`, playable >= floor, `${playable} playable`);
  }
}

await botPool();
await authChecks();
await catalogSettings();
await streams();
await externalPrefixStreams();

line('');
line(`RESULT: ${failures === 0 ? 'PASS' : 'FAIL'} (${failures} failed check(s))`);
process.exit(failures === 0 ? 0 : 1);
