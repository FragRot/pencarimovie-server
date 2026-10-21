# PORT_GAPS — `port/rust` vs `backend.php`

Audit of the Rust port against `backend.php` (20,527 lines), route by route and
function by function. `backend.php` is authoritative: where this document says a
behaviour was "ported", the Rust code reproduces the PHP's observable output
(status code, body shape, field names, defaults, fallbacks, cache windows).

Verification commands (both green as of this audit):

```cmd
cd port\rust
"%USERPROFILE%\.cargo\bin\cargo.exe" test --bin pencarimovie-server
```

```cmd
powershell -NoProfile -ExecutionPolicy Bypass -File storage\_rust_manifest_check.ps1
```

Test suite: **151 passed, 0 failed** (144 before this audit + 7 new descriptor
tests). Live descriptor check: **30/30 assertions PASS**.

---

## 1. Route map

PHP routes were enumerated from the dispatch block in `backend.php`
(`handle_request()`, lines ~14112-20527). Every one of them has a Rust handler in
`routes.rs::handle()` unless listed in section 3.

| PHP route                                              | PHP lines     | Rust handler                | Status                                             |
| ------------------------------------------------------ | ------------- | --------------------------- | -------------------------------------------------- |
| `GET /api/sub-proxy`                                   | 18321         | `handle()` 172              | Equivalent                                         |
| `GET /api/auth/status`                                 | 18369         | 244                         | Equivalent                                         |
| `POST /api/auth/login`                                 | 18381         | 256                         | Equivalent (lockout + timing parity)               |
| `POST /api/auth/logout`                                | 18429         | 317                         | Equivalent                                         |
| `POST /api/auth/password`                              | 18434         | 326                         | Equivalent                                         |
| `POST /api/auth/token/rotate`                          | 18448         | 343                         | Equivalent (privileged)                            |
| `POST /api/auth/reset`                                 | 18455         | 350                         | Equivalent (privileged)                            |
| `GET /api/version`                                     | 18465         | 360                         | Equivalent, minus the hard 426 gate (see §3.1)     |
| `GET /api/lan-ip`                                      | 18469         | 364                         | Equivalent                                         |
| `GET /api/tunnel/status`                               | 18477         | 373                         | Equivalent                                         |
| `POST /api/tunnel/enable`                              | 18481         | 399                         | Equivalent (privileged)                            |
| `POST /api/tunnel/disable`                             | 18490         | 450                         | Equivalent (privileged)                            |
| `GET/POST /api/catalog-settings`                       | 18496 / 18507 | 458 / 474                   | Equivalent (full document merged over defaults)    |
| `POST/DELETE /api/media-ids`                           | 18668         | 567                         | Equivalent                                         |
| `POST /api/validate-manifest`                          | 18715         | 664                         | Equivalent (real fetch, 8 s)                       |
| `GET/POST /api/country`                                | 18755 / 18787 | 710 / 723                   | Equivalent                                         |
| `/api/session`                                         | 18804         | 751                         | Equivalent (delegated)                             |
| `GET /api/clock-check`                                 | 18868         | 798                         | Equivalent                                         |
| `GET/POST /api/provision`                              | 18888         | 815                         | Equivalent                                         |
| `GET /api/bots`                                        | 18928         | 840                         | Equivalent                                         |
| `POST /api/bots/add`                                   | 18949         | 866                         | Equivalent                                         |
| `POST /api/bots/remove`                                | 19059         | 951                         | Equivalent                                         |
| `POST /api/bots/set-active`                            | 19078         | 972                         | Equivalent                                         |
| `GET /api/diagnostics`                                 | 19149         | 1027                        | Equivalent                                         |
| `/api/diagnostics/settings`                            | 19158         | 1052                        | Equivalent (privileged on write)                   |
| `GET /api/reports[?clear\|send\|flush]`                | 19172         | 1071                        | Equivalent (privileged on mutate)                  |
| `/api/debug-mode`                                      | 19206         | 1135                        | Equivalent                                         |
| `GET /api/logs`                                        | 19225         | 1150                        | Equivalent                                         |
| `GET/POST /api/resolve-shortcode`                      | 19313         | 1396                        | **Ported this audit** (§2.16)                      |
| `GET/POST /api/warmup-resolve`                         | 19352         | 1471                        | Equivalent                                         |
| `POST /api/botlogin`                                   | 19416         | 1202                        | Equivalent                                         |
| `POST /api/botlogout`                                  | 19556         | 1239                        | Equivalent                                         |
| `GET /api/proxy-stream`                                | 19577         | 1250                        | Equivalent data, REST transport (§3.4)             |
| `GET /api/download*`                                   | 11108         | `download::handle_download` | Equivalent (range, resume, ALAC)                   |
| `GET /manifest.json`                                   | 14972         | 1301                        | **Ported this audit** (§2.1-2.7)                   |
| `GET /catalog/:type/:id[/extra].json`                  | 15492         | `handle_catalog`            | Equivalent + fixes (§2.13-2.15)                    |
| `GET /meta/:type/:id.json`                             | 16057         | `handle_meta`               | **Ported this audit** (§2.9-2.12)                  |
| `GET /stream/:type/:id.json`                           | 16489         | `handle_stream`             | Equivalent (locked-stream item, bingeGroup)        |
| `GET /subtitles/:type/:id.json`                        | 18268         | `handle_subtitles`          | Equivalent (pm: -> tt: translation)                |
| `/eclipse/*` (manifest, search, catalog, stream, play) | 14138-14564   | 1344                        | Equivalent + manifest fix (§2.18)                  |
| `GET /nuvio`, `GET /stremio`, `/configure`             | 14688-14725   | 1299 / 1499                 | Alias redirects + `/configure` -> `/#addon` (§3.2) |

---

## 2. Divergences FIXED (this audit)

### Descriptor (`/manifest.json`)

1. **`resources` is an array of OBJECTS** (`backend.php:15418-15438`), not strings:
   `{name:"catalog",types}` only when a catalog row survived the filters, then
   `{name:"meta"|"stream"|"subtitles", types, idPrefixes}`. The old string array
   lost the per-resource scoping, so a client could not tell that a `tvdb:` /
   `anilist:` id is answerable. Pinned by
   `manifest_resources_are_objects_scoped_to_the_active_types`.
2. **`types` = `$activeTypes`** (`backend.php:15336-15341`): derived from
   `enabled_types` (PHP `!empty()` semantics, per key), falling back to all three
   when the result is empty, then extended with the types of bridged upstream
   catalogs. A disabled media type now disappears from `types` **and** from every
   resource's `types`. Pinned by
   `active_types_follow_enabled_types_with_a_three_type_fallback`.
3. **`idPrefixes` order** is the PHP's (`backend.php:15458`):
   `pm_, pm:, tt, tmdb:, kitsu:, kitsu, mal:, anilist:, tvdb:` — `pm_` **before**
   `pm:`.
4. **`extra[].options`** on every local row (`backend.php:15141-15291`):
   `top`/`pm_search_*` carry genres + years, `year`/`pm_series_year`/
   `pm_files_year` carry the YEAR list on their `genre` extra, and country rows
   carry `$filterOptionsCombined` — 47 years FIRST, then 22 genres (69 options).
   Pinned by assertions inside `descriptor_advertises_every_php_catalog_row`.
5. **`pm_topkw_*` rows emit no `extraSupported` / `extraRequired`**
   (`backend.php:15282-15291`) and carry the 6-pill quality list on `genre`.
6. **Row names** follow the PHP: `other/top` = "Telegram Files",
   `pm_files_year` = "New", `pm_search_files` = "Telegram Files".
7. **Descriptor response headers** (`backend.php:15468-15488`): byte-stable
   `ETag` (md5 of the encoded body), `Cache-Control: max-age=1800,
stale-while-revalidate=3600`, and **304** on a matching `If-None-Match`
   (quoted or bare). Was `no-store`. Pinned by
   `manifest_response_sets_an_etag_and_answers_304`.
8. **`manifest_identity()`** — a port of `fd_stremio_manifest_identity()`: the id
   and description follow the address the addon was installed from
   (`addon.local` / `.lan` / `.tunnel` / `.server`) and the description NAMES that
   address; `?mode=` forces one. Pinned by
   `manifest_identity_names_the_address_it_was_installed_from`.

### `/meta`

9. **`{"meta": null}` is now HTTP 404** (`backend.php:16485`). Answering 200 made
   Stremio treat a missing item as existing and render an empty page instead of
   falling back.
10. **The direct-Telegram-file branch handles `pm_file_`, `pm:file_` AND
    `pm:file:`** (`backend.php:16073-16165`). The catalog's "Telegram Files" rows
    emit `pm_file_<code>`, so every one of those items previously fell through to
    the upstream bridge and answered `{"meta":null}` — a blank info page with no
    poster, description or Play button. The branch now reproduces the PHP:
    `search_files` exact/single-result lookup -> `resolve_shortcode` for anything
    missing -> `File <code>` title fallback (never null) ->
    `clean_media_title()` -> the META pill set (`4K UHD`, `HDTV`, `HDR`, `AVC`)
    joined with `"  •  "` -> `poster`/`background`/`logo`/`genres`/`description`
    -> 3600 s cache, `max-age=600, public`.
11. **Post meta** (`backend.php:16374-16394`): echoes the id the CLIENT asked
    with, adds `background` and `year` next to `releaseInfo`, and each episode
    carries `number`, `released` (RFC3339 UTC) and `thumbnail`. Episode labels are
    `S{s}E{e}` (the PHP's, no zero padding). A series with no resolvable episode
    gets the PHP's single fallback `Episode 1` entry, without which Stremio
    rejects the meta.
12. **Local `media_ids_idx` fallback for an external id**
    (`backend.php:16456-16478`): after the upstream meta miss, the port now looks
    the id up locally (including the PHP's `tt` <-> `imdb` swap) and answers a
    minimal meta with `name`/`year`/`releaseInfo`/`posterShape`, so a known title
    keeps its Play button. The PHP's prefix regex semantics are reproduced
    exactly, including its behaviour on a `tt...:S:E` id.
13. **File catalog rows** (`backend.php:15784-15856`):
    - the title is the file name with HTML entities decoded, NOT
      `fd_clean_post_title()` (which strips a trailing "• Movie" from a real
      filename);
    - the description is `"{prefix} · {pillLine}\n\n{title}"` with the PHP's prefix
      per row (`File` on the year rows, `Trending File` on keyword rows,
      `Direct Telegram File` otherwise);
    - the primary genre label is `Year` on the year rows and `Trending File` on
      every other file row (`backend.php:15806`);
    - the `genre` extra is compared EXACTLY against the built genre list;
    - pills use the PHP's `\b` boundaries, so a standalone `HD`/`SD` token is a
      resolution pill (`\b(720p|hd)\b`) but `HDU` is not. The port previously used
      `contains()` and had dropped `hd`/`sd` entirely.
      Pinned by `file_pills_use_the_php_word_boundaries` and
      `file_meta_pills_use_the_php_meta_set`.
14. **A search on `other/*` also lists matching POSTS**
    (`backend.php:15615-15656`), except on `pm_search_files`, which the PHP resets
    to files only (`backend.php:15662`). The port had been files-only for every
    `other` row.
15. **Catalog cache TTL**: 300 s for a SEARCH result, 600 s for a browse row
    (`backend.php:15586` / `15595`). The port cached both for 600 s.
16. **`/api/resolve-shortcode` accepts GET and POST, and the plural
    `short_codes` batch form** (`backend.php:19313-19344`). The dashboard's batch
    pre-warm ([`public/app.js:5064`](../../public/app.js)) was calling
    `?short_codes=...` and getting a 400, so it never warmed anything. The batch
    branch now runs `warmup_resolve_batch()` and answers its body; the 400 message
    is the PHP's ("short_code or short_codes is required."); an unresolved code
    answers the resolver's own body with **HTTP 200** (not 404), so the dashboard
    can display `message`.
17. **`/stremio` and `/stremio/` answer 301 to `/nuvio`** (`backend.php:14688`)
    instead of quietly serving the manifest. `/stremio/manifest.json` and the
    other `/stremio/*` addon paths stay aliased.
18. **Eclipse manifest** (`backend.php:14315-14346`): id
    `org.pencarimovie.eclipsemusic`, name `PencariMusic`, the PHP's description,
    `types: ["track","album","artist"]`, the `latest` track catalog row and the
    `quality` select setting, served `max-age=3600`. Eclipse Music renders its
    catalog list and settings sheet from this descriptor, so the empty
    `catalogs`/`settings` made the addon look installed-but-broken.

### Cache / plumbing layer (audited, no change needed)

| Item                                       | PHP                                                                                                                   | Rust                                                                                   | Result                                                                                                                           |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Prune TTL table                            | `fd_prune_cache_files()` (`backend.php:302-324`), 13 entries                                                          | `storage.rs::CACHE_TTLS`, same 13 + values                                             | Identical, incl. `up_cat_` = 3600 (the historical bug where 600 deleted the bridged catalog cache every 10 min)                  |
| `fd_stream_ajax_ttl()`                     | 300 search, 600 posts/post_files/get_post/search_music, 3600 categories/trending                                      | `wp_data.rs::stream_ajax_ttl`                                                          | Identical                                                                                                                        |
| AJAX cache-first layer                     | `fd_fetch_stream_ajax` + `fd_cached_wp_json` (single-flight, stale-while-revalidate, 6 s/8 s timeouts, 6 h stale cap) | `wp_data::fetch_stream_ajax` + `httpc::cached_wp_json`                                 | Equivalent; `stream_files` uses the whole-body cache at 15 s like the PHP                                                        |
| `upstream_enabled` master switch           | empties the manifest list for every consumer, KEEPS the stored list                                                   | `load_catalog_settings_merged()` keeps the stored list, `upstream::` honors the switch | Equivalent                                                                                                                       |
| `enabled_types` / `enabled_catalogs` seeds | `backend.php:11964` seeds each switch from its `default` flag                                                         | `default_catalog_settings()` + `default_enabled_catalogs()`                            | Equivalent (`fresh_install_seeds_every_catalog_switch_from_its_default`, `catalog_gate_matches_the_php_remaps` kept and passing) |

---

## 3. Remaining known divergences (deliberate, with justification)

### 3.1 The 426 version gate is not implemented

`backend.php` blocks every `/api/*` route with HTTP 426 when the engine is older
than the WordPress `min_version`. The port reports the comparison
(`/api/version` -> `update_needed`, `minimum_version`, `update_url`) and the
dashboard renders the upgrade banner, but does not lock the API.
Justification: an outdated binary that cannot even answer `/api/version` is
unrecoverable in place, and the app-shell build (`--app-version`) must never be
gated by the PHP engine lineage. Documented in code at `routes.rs:1002-1010`.

### 3.2 Landing pages are not ported

- `/nuvio` in the PHP is an addon-install landing page (HTML, 250 lines at
  `backend.php:14725`). The port answers the addon MANIFEST there
  (`/nuvio` -> `/manifest.json`), because that is the URL clients install and the
  embedded dashboard already carries the install UI at `/#addon`.
- `/eclipse` in the PHP is an Eclipse landing page; the port answers the Eclipse
  manifest at both `/eclipse` and `/eclipse/manifest.json`.

### 3.3 Post meta is thinner than the PHP's

- `released` is now() rather than the file's `added_date` (the origin's
  `added_date` is not carried on `WpFile`).
- An episode's `name` is `S{s}E{e}`; the PHP substitutes the uploader's derived
  episode title when it has one (`$epInfo['title']`).
- The PHP pre-seeds and re-reads `meta_cache_<md5('post_<id>')>.json` (1800 s) and
  `post_data_<id>.json` (3600 s). The port caches the CATALOG row and the direct
  file meta, but not the post meta document.

### 3.4 `/api/proxy-stream` uses the REST routes, not `admin-ajax.php`

The PHP proxies `?action=stream_*` to `admin-ajax.php` and passes the WordPress
AJAX envelope (`{success, data}`) through. The port calls the equivalent REST
paths (`/posts`, `/post-files`, ...) directly. Same data, same shape the shell
consumes, one less PHP dependency; no other port route touches admin-ajax.

### 3.5 Cosmetic / non-observable

- `fd_stremio_json()`'s DEFAULT `Cache-Control` is
  `no-cache, no-store, must-revalidate`; several port responses say `no-store`.
  Both are uncacheable; only the header string differs. Responses that matter
  (descriptor, bridged catalog, meta bridge, direct-file meta) use the PHP's
  explicit values.
- The ECLIPSE manifest icon and the addon `logo` point at the SHIPPED
  `public/logo.png`. The PHP keeps
  `raw.githubusercontent.com/.../public/logo.png` as a fallback for the addon
  logo and a WordPress upload for the Eclipse icon (operator decision: never
  depend on either host).
- File-row descriptions drop the PHP's calendar/lightning/document glyphs
  (`backend.php:15819-15831`); the labels themselves are identical. Operator
  decision: plain text.
- `manifest()` writes `manifest_cache_.json` in the PHP; the port reuses its
  redb/in-memory cache for upstream manifests instead. Same freshness windows.

---

## 4. Measured before / after

| Measurement                            | Before                                              | After                                                                        |
| -------------------------------------- | --------------------------------------------------- | ---------------------------------------------------------------------------- |
| `resources` in `/manifest.json`        | `["catalog","meta","stream","subtitles"]` (strings) | 4 objects, each with `types` (+`idPrefixes` on meta/stream/subtitles)        |
| `types` with movie disabled            | still `["movie","series","other"]`                  | `["series","other"]`                                                         |
| Top-level `idPrefixes`                 | `pm:` before `pm_`                                  | `pm_` before `pm:` (PHP order)                                               |
| Country row `genre` options            | 47 (years only)                                     | **69** (47 years first, then 22 genres)                                      |
| `pm_topkw_*` rows                      | carried `extraSupported`/`extraRequired`            | neither, 6-pill `genre` list                                                 |
| Descriptor headers                     | `Cache-Control: no-store`                           | `max-age=1800, stale-while-revalidate=3600` + `ETag`, 304 on `If-None-Match` |
| `GET /meta/movie/pm_file_<code>.json`  | `{"meta":null}` 200 (blank page)                    | full meta, `max-age=600, public`, 1 h cache                                  |
| `GET /meta/movie/tt99999999.json`      | `{"meta":null}` 200                                 | `{"meta":null}` **404**                                                      |
| Episode entries                        | `id/season/episode/name`                            | + `number`, `released`, `thumbnail`; `S1E1` labels; fallback `Episode 1`     |
| `?short_codes=a,b,c` resolve           | 400 `short_code is required.`                       | batch warm-up result                                                         |
| `/stremio`                             | manifest JSON                                       | 301 -> `/nuvio`                                                              |
| Eclipse manifest                       | 1 type, 0 catalogs, 0 settings                      | `track/album/artist`, `latest` row, quality setting, `max-age=3600`          |
| `cargo test --bin pencarimovie-server` | 144 passed                                          | **151 passed**, 0 failed, 0 new warnings                                     |
| `storage/_rust_manifest_check.ps1`     | n/a                                                 | **30/30 PASS**                                                               |
