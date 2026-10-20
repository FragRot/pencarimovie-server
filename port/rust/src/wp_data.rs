//! WordPress data layer for the stream route — the `wp_data` group in
//! [`port/COVERAGE.md`](../../COVERAGE.md) that the title matcher needs in
//! order to see any files at all.
//!
//! What `/stream/*`, `/catalog/*` and `/meta/*` need is here: the cache-first
//! AJAX fetch ([`fetch_stream_ajax`] = `fd_fetch_stream_ajax`), paged post files
//! ([`fetch_post_files_paged`] = `fd_fetch_post_files_paged`), the post list /
//! search, the trending keyword map ([`trending_keywords`] =
//! `fd_get_trending_keywords_map`) and the two identity lookups the non-`pm:`
//! ids need.
//!
//! ## The contract that is easy to get wrong
//!
//! `fd_fetch_stream_ajax('post_files')` does **not** hit `admin-ajax.php`. It
//! lowercases the action to a REST route by replacing `_` with `-`:
//!
//! ```text
//! post_files   ->  GET {WP_API_BASE}/post-files?post_id=…&limit=…&offset=…
//! search_files ->  GET {WP_API_BASE}/search-files?search=…&limit=…&offset=…
//! ```
//!
//! and then unwraps WordPress's envelope: `{success, data}` when `success` is
//! truthy, else `data`, else `items`, else the body itself. Post files live
//! under the unwrapped `files` key, not `items`, and the loop must honour
//! `has_more` **or** a full page — the endpoint returns newest-episode-first and
//! silently truncates a long series on a single call (65-episode post listed
//! only E2..E65 with one `limit=200` request).

use serde_json::Value;

use crate::config;
use crate::httpc;
use crate::log::log;

/// `fd_stream_ajax_ttl()`.
pub fn stream_ajax_ttl(action: &str) -> i64 {
    match action {
        "search" | "search_files" => 300,
        "posts" | "post_files" | "get_post" | "search_music" => 600,
        "categories" | "trending" => 3600,
        _ => 300,
    }
}

fn ajax_timeout(action: &str) -> u64 {
    if matches!(action, "search" | "search_files") {
        6
    } else {
        8
    }
}

/// `post_files` -> `/post-files`.
fn rest_route(action: &str) -> String {
    format!(
        "{}/{}",
        config::wp_api_base().trim_end_matches('/'),
        action.replace('_', "-")
    )
}

/// WordPress envelope unwrap (see the module docs).
pub fn unwrap_data(v: &Value) -> Value {
    if v.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
        if let Some(d) = v.get("data") {
            return d.clone();
        }
    }
    if let Some(d) = v.get("data") {
        return d.clone();
    }
    if let Some(i) = v.get("items") {
        return i.clone();
    }
    v.clone()
}

fn query_add(mut url: String, params: &[(String, String)]) -> String {
    if params.is_empty() {
        return url;
    }
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in params {
        ser.append_pair(k, v);
    }
    let q = ser.finish();
    url.push(if url.contains('?') { '&' } else { '?' });
    url.push_str(&q);
    url
}

/// `fd_fetch_stream_ajax()`'s `bot_id` rule: every action gets one EXCEPT
/// `trending`. The keyword table is global, so a per-bot `bot_id` would only
/// multiply the cache keys (and the origin calls) for an identical answer — the
/// PHP explicitly unsets it for that action.
pub fn stream_ajax_wants_bot(action: &str) -> bool {
    action != "trending"
}

/// How long a stale ajax body may still be served when a refresh fails. The old
/// code read the file "at any age" for this; bounded now.
const AJAX_STALE_GRACE: i64 = 21_600;

/// `fd_fetch_stream_ajax()`: cache + single-flight + stale fallback, `bot_id` added
/// when the caller did not supply one (the origin resolves files per bot, so a
/// missing `bot_id` changes the answer, not just the cache key).
pub async fn fetch_stream_ajax(action: &str, params: &[(String, String)]) -> Value {
    let mut params: Vec<(String, String)> = params.to_vec();
    if stream_ajax_wants_bot(action) && !params.iter().any(|(k, _)| k == "bot_id") {
        let bot = crate::session::get_bot_id();
        if !bot.is_empty() {
            params.push(("bot_id".to_string(), bot));
        }
    }
    let url = query_add(rest_route(action), &params);
    // Key on the action AND the full url: the same action resolves to different
    // answers per `bot_id`, season, episode and search term.
    let key = format!("{}|{}", action, url);
    let ttl = stream_ajax_ttl(action);

    let url_owned = url.clone();
    let action_owned = action.to_string();
    let body = crate::cache::fetch_cached("wp_ajax", &key, ttl, AJAX_STALE_GRACE, move || async move {
        let body = httpc::get_contents(
            &url_owned,
            ajax_timeout(&action_owned),
            [("Accept".to_string(), "application/json".to_string())]
                .into_iter()
                .collect(),
        )
        .await;
        let decoded = match body.and_then(|b| serde_json::from_str::<Value>(&b).ok()) {
            Some(v) => v,
            None => return (Value::Null, false),
        };
        let data = unwrap_data(&decoded);
        // Only a real payload is cacheable; an error envelope must not be pinned.
        let cacheable = data.is_object() || data.is_array();
        (data, cacheable)
    })
    .await;

    if body.is_null() {
        if let Some(stale) = crate::cache::get_stale("wp_ajax", &key) {
            log(
                "wp ajax served stale",
                &serde_json::json!({"action": action}),
            );
            return (*stale).clone();
        }
        log(
            "wp ajax fetch empty",
            &serde_json::json!({"action": action, "had_stale": false}),
        );
    }
    body
}

/// One file row as the matcher and the stream builder consume it.
#[derive(Debug, Clone, Default)]
pub struct WpFile {
    pub short_code: String,
    pub title: String,
    pub caption: String,
    pub file_size: u64,
    pub mime: String,
    pub season_num: i32,
    pub episode_num: i32,
}

pub fn wp_file_from(v: &Value) -> Option<WpFile> {
    let short_code = v
        .get("short_code")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if short_code.is_empty() {
        return None;
    }
    let title = ["title", "file_name", "name"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    let caption = v
        .get("caption")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let file_size = v
        .get("file_size")
        .and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0);
    let mime = ["mime", "file_type", "mime_type"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    Some(WpFile {
        short_code,
        title,
        caption,
        file_size,
        mime,
        season_num: json_i32(v, "season_num"),
        episode_num: json_i32(v, "episode_num"),
    })
}

fn json_i32(v: &Value, key: &str) -> i32 {
    v.get(key)
        .and_then(|x| {
            x.as_i64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .unwrap_or(0) as i32
}

/// `fd_fetch_post_files_paged()` — pages until a short page or `!has_more`.
pub async fn fetch_post_files_paged(
    post_id: &str,
    page_size: usize,
    max_files: usize,
    max_pages: usize,
) -> Vec<WpFile> {
    let page_size = page_size.clamp(1, 200);
    let mut all: Vec<WpFile> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut offset = 0usize;

    for _page in 0..max_pages.max(1) {
        let params = vec![
            ("post_id".to_string(), post_id.to_string()),
            ("limit".to_string(), page_size.to_string()),
            ("offset".to_string(), offset.to_string()),
        ];
        let res = fetch_stream_ajax("post_files", &params).await;
        // `post-files` answers `{files: […], has_more}`; the unwrapped `data` may
        // also be the bare array, so accept both shapes.
        let files: Vec<Value> = rows_of(&res, &["files", "items"]);
        if files.is_empty() {
            break;
        }
        let returned = files.len();
        let mut added = 0usize;
        for f in &files {
            let Some(item) = wp_file_from(f) else { continue };
            if !seen.insert(item.short_code.clone()) {
                continue;
            }
            all.push(item);
            added += 1;
            if all.len() >= max_files {
                return all;
            }
        }
        let has_more = res
            .get("has_more")
            .map(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
            .unwrap_or(false);
        if !has_more && returned < page_size {
            break;
        }
        if added == 0 {
            break;
        }
        offset += page_size;
    }
    all
}

/// `GET {WP_API_BASE}/posts` — the catalog list (`media_type`, `category`,
/// `genre`, `year`, `limit`, `offset`). Answers a bare array.
pub async fn posts(params: &[(String, String)]) -> Vec<Value> {
    rows_of(
        &fetch_stream_ajax("posts", params).await,
        &["posts", "items", "files"],
    )
}

/// `GET {WP_API_BASE}/search` — WordPress post search. Bare array as well.
pub async fn search_posts(params: &[(String, String)]) -> Vec<Value> {
    rows_of(
        &fetch_stream_ajax("search", params).await,
        &["posts", "items", "files"],
    )
}

/* ── trending keywords (backend.php:11655 / 11793) ───────────────────────── */

/// No keyword blocklist here on purpose: the WordPress REST endpoint is the
/// single place that decides which keywords are acceptable (it already drops
/// NSFW/filler terms), so an app-side list only duplicated that policy and
/// produced false negatives — it hid legitimate keywords such as `melayu`, and
/// `Up`-style short titles. The only thing dropped locally is an empty token.
pub fn is_topkeyword_valid(keyword: &str) -> bool {
    !keyword.trim().is_empty()
}

/// Pure parser for the `/trending` payload.
///
/// The origin answers `{success:true, data:[{keyword, hits, total_found}, …]}`
/// and [`unwrap_data`] has already unwrapped it by the time this runs, but a
/// bare array of strings and the `keywords`/`items` object shapes are accepted
/// too. Order is the ORIGIN's (relevance), not hits-descending — the PHP's map
/// keeps that order as well.
pub fn trending_keywords_from(v: &Value) -> Vec<(String, u64)> {
    let rows = rows_of(v, &["keywords", "items", "data"]);
    let mut out: Vec<(String, u64)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in rows {
        let kw = match row.as_str() {
            Some(s) => s.trim().to_string(),
            None => ["keyword", "query", "term"]
                .iter()
                .find_map(|k| row.get(*k).and_then(|x| x.as_str()))
                .unwrap_or("")
                .trim()
                .to_string(),
        };
        if !is_topkeyword_valid(&kw) {
            continue;
        }
        let hits = row
            .get("hits")
            .map(|h| {
                h.as_u64()
                    .or_else(|| h.as_str().and_then(|s| s.trim().parse().ok()))
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        if !seen.insert(kw.to_lowercase()) {
            continue;
        }
        out.push((kw, hits));
    }
    out
}

/// `fd_get_trending_keywords_map()` — the trending keywords with their hit
/// counts, in the origin's order. No keyword is dropped (empty tokens aside):
/// content filtering belongs to the REST endpoint, not to the app.
///
/// Two caches, exactly like the PHP: the AJAX response itself is disk-cached for
/// an hour (`fd_stream_ajax_ttl('trending')`), and this list is memoised in
/// process for 10 minutes. The PHP additionally caps its map to the top **5**
/// keywords because each one becomes a separate `pm_topkw_*` catalog row; here
/// the keywords are merged into two type rows, so dropping the tail would make
/// the rarer type (movies) collapse to nothing. The list is therefore returned
/// whole and the row builder stops as soon as it is full.
pub async fn trending_keywords() -> Vec<(String, u64)> {
    /// TTL of the memoised keyword list.
    const TTL: i64 = 600;
    /// How long the last good list may still be served when the origin is empty.
    const GRACE: i64 = 21_600;

    fn decode(v: &Value) -> Option<Vec<(String, u64)>> {
        let arr = v.as_array()?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let pair = item.as_array()?;
            let k = pair.first()?.as_str()?.to_string();
            let n = pair.get(1).and_then(|x| x.as_u64()).unwrap_or(0);
            out.push((k, n));
        }
        (!out.is_empty()).then_some(out)
    }

    if let Some(hit) = crate::cache::get("trending", "keywords") {
        if let Some(list) = decode(&hit) {
            return list;
        }
    }
    // `fetch_stream_ajax` deliberately does NOT send `bot_id` for `trending`
    // (see its docs): the keyword table is global, and a per-bot cache key would
    // multiply the origin calls for no gain.
    let res = fetch_stream_ajax("trending", &[("limit".to_string(), "15".to_string())]).await;
    let list = trending_keywords_from(&res);
    if list.is_empty() {
        // Origin down / stale beyond the grace window: keep serving the last list.
        if let Some(stale) = crate::cache::get_stale("trending", "keywords") {
            if let Some(prev) = decode(&stale) {
                return prev;
            }
        }
        log(
            "trending keywords empty",
            &serde_json::json!({"shape": if res.is_array() { "array" } else { "object" }}),
        );
        return list;
    }
    crate::cache::put("trending", "keywords", &serde_json::json!(list), TTL, GRACE);
    list
}

/// `GET {WP_API_BASE}/lookup-id?prefix=imdb&title=…&year=…` — a Manticore title
/// match over `media_ids_idx.title`.
///
/// The title MUST be stripped of decorations first (bullet separator, year,
/// trailing type word) or the literal phrase match can never hit — the exact bug
/// documented for `fd_find_imdb_id_for_title()` ("Veil of Shadows 2026 • TvSeries").
pub async fn lookup_imdb_by_title(title: &str, year: &str) -> String {
    let clean = crate::matcher::strip_title_decorations(title);
    if clean.is_empty() {
        return String::new();
    }
    let mut params = vec![
        ("prefix".to_string(), "imdb".to_string()),
        ("title".to_string(), clean),
    ];
    if !year.is_empty() {
        params.push(("year".to_string(), year.to_string()));
    }
    let url = query_add(rest_route("lookup_id"), &params);
    let row = unwrap_data(&httpc::http_json(&url, &[], "GET", 8).await);
    let id = row
        .get("imdb_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !id.is_empty() {
        return id;
    }
    row.get("imdb")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// `GET {WP_API_BASE}/lookup-id?prefix=…&id=…` — the local `media_ids_idx` row.
pub async fn lookup_id(prefix: &str, id: &str) -> Value {
    let url = query_add(
        rest_route("lookup_id"),
        &[
            ("prefix".to_string(), prefix.to_string()),
            ("id".to_string(), id.to_string()),
        ],
    );
    unwrap_data(&httpc::http_json(&url, &[], "GET", 8).await)
}

/// `GET {WP_API_BASE}/stream-files?type=…&title=…&year=…&id=…`.
///
/// Used for ids that are not one of our own posts (`tt…`, `tmdb:…`): the origin
/// searches by title and answers `{ok, resolved_title, items: […]}`. The
/// matcher still decides which of those items belong to the title, because the
/// origin's title search returns a superset (it ignores `year`).
pub async fn stream_files_by_title(
    media_type: &str,
    title: &str,
    year: &str,
    id: &str,
) -> (String, Vec<WpFile>) {
    let mut params = vec![
        ("type".to_string(), media_type.to_string()),
        ("title".to_string(), title.to_string()),
    ];
    if !year.is_empty() {
        params.push(("year".to_string(), year.to_string()));
    }
    if !id.is_empty() {
        params.push(("id".to_string(), id.to_string()));
    }
    let res = fetch_stream_ajax("stream_files", &params).await;
    let resolved_title = res
        .get("resolved_title")
        .and_then(|v| v.as_str())
        .unwrap_or(title)
        .to_string();
    // CRITICAL: `/stream-files` answers a BARE JSON ARRAY of file rows, not an
    // object with `items`. Reading only `items` made every `tt…` stream request
    // answer "no file matched the title" (measured: `tt11540908:1:1` "Carrie"
    // returned 75,264 B of real files that were parsed as ZERO).
    let items = rows_of(&res, &["items", "files"]);
    let files = items.iter().filter_map(wp_file_from).collect();
    (resolved_title, files)
}

/// `GET {WP_API_BASE}/search-files?search=…&limit=…` — the keyword search the
/// dashboard uses.
///
/// `stream-files?title=` only returns the rows the origin ties to that post /
/// identity, so the SAME episode uploaded by another release group is invisible
/// to it (measured on "Positively Yours 2026": the post set holds 22 files but
/// the OMG 720p/1080p MalaySub variants only appear in the keyword search).
/// Merging both is what makes an episode list several sources, like the PHP.
pub async fn search_files(search: &str, limit: usize) -> Vec<WpFile> {
    let res = fetch_stream_ajax(
        "search_files",
        &[
            ("search".to_string(), search.to_string()),
            ("limit".to_string(), limit.to_string()),
        ],
    )
    .await;
    rows_of(&res, &["files", "items"])
        .iter()
        .filter_map(wp_file_from)
        .collect()
}

/// Rows from any of the shapes the origin uses: a bare array, or an object with
/// one of `keys` holding the array.
fn rows_of(v: &Value, keys: &[&str]) -> Vec<Value> {
    if let Some(arr) = v.as_array() {
        return arr.clone();
    }
    for key in keys {
        if let Some(arr) = v.get(*key).and_then(|x| x.as_array()) {
            return arr.clone();
        }
    }
    Vec::new()
}

/// The WordPress post behind a `pm:post:<id>` catalog item.
pub async fn get_post(post_id: &str) -> Value {
    let params = vec![("post_id".to_string(), post_id.to_string())];
    let res = fetch_stream_ajax("get_post", &params).await;
    // The handler answers either the post object or `{post: {...}}`.
    match res.get("post") {
        Some(p) if p.is_object() => p.clone(),
        _ => res,
    }
}

pub fn post_title(post: &Value) -> String {
    ["title", "post_title", "name"]
        .iter()
        .find_map(|k| post.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string()
}

pub fn post_year(post: &Value) -> String {
    for key in ["year", "release_year", "release_year_nr"] {
        if let Some(v) = post.get(key) {
            let s = v
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string());
            let s = s.trim().to_string();
            if s.chars().count() >= 4 && s.chars().take(4).all(|c| c.is_ascii_digit()) {
                return s.chars().take(4).collect();
            }
        }
    }
    // Fall back to the year embedded in the title ("The Boy 2026 • Movie").
    let title = post_title(post);
    if let Some(caps) = fancy_regex::Regex::new(r"\b(19\d{2}|20\d{2})\b")
        .ok()
        .and_then(|re| re.captures(&title).ok().flatten())
    {
        if let Some(m) = caps.get(1) {
            return m.as_str().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_matches_php() {
        assert_eq!(stream_ajax_ttl("search"), 300);
        assert_eq!(stream_ajax_ttl("post_files"), 600);
        assert_eq!(stream_ajax_ttl("trending"), 3600);
        assert_eq!(stream_ajax_ttl("something_else"), 300);
    }

    #[test]
    fn trending_never_gets_a_bot_id() {
        // The cache key is the URL, so a `bot_id` here would give every bot its
        // own copy of the same global keyword table.
        assert!(!stream_ajax_wants_bot("trending"));
        assert!(stream_ajax_wants_bot("posts"));
        assert!(stream_ajax_wants_bot("search"));
        assert!(stream_ajax_wants_bot("post_files"));
    }

    #[test]
    fn topkeyword_filter_only_drops_empty_tokens() {
        // The app performs NO keyword filtering: whatever the REST endpoint
        // returns is what we show. Only an empty token is dropped, because it
        // would render an unusable row.
        assert!(!is_topkeyword_valid(""));
        assert!(!is_topkeyword_valid("   "));
        assert!(is_topkeyword_valid("the rookie"));
    }

    #[test]
    fn parses_trending_payload() {
        // The live shape: `{success:true, data:[{keyword,hits,total_found}]}`.
        let live = serde_json::json!([
            {"keyword": "the rookie", "hits": 1814, "total_found": 1814},
            {"keyword": "spiderman", "hits": "2251"},
            {"keyword": "THE ROOKIE", "hits": 3}
        ]);
        // Order is the origin's; only the case-insensitive duplicate is dropped.
        assert_eq!(
            trending_keywords_from(&live),
            vec![
                ("the rookie".to_string(), 1814),
                ("spiderman".to_string(), 2251)
            ]
        );
        // A bare array of strings is accepted (empty entries skipped), and an
        // envelope-shaped body is unwrapped by the caller — neither may panic.
        let bare = serde_json::json!(["kudrat", "", "hometown cha cha cha"]);
        assert_eq!(
            trending_keywords_from(&bare),
            vec![
                ("kudrat".to_string(), 0),
                ("hometown cha cha cha".to_string(), 0)
            ]
        );
        assert!(trending_keywords_from(&serde_json::json!({"message": "nope"})).is_empty());
    }

    #[test]
    fn route_replaces_underscores() {
        let r = rest_route("post_files");
        assert!(r.ends_with("/post-files"), "{r}");
        let r = rest_route("search_files");
        assert!(r.ends_with("/search-files"), "{r}");
        let r = rest_route("stream_files");
        assert!(r.ends_with("/stream-files"), "{r}");
    }

    #[test]
    fn unwraps_wordpress_envelope() {
        let v = serde_json::json!({"success": true, "data": {"files": [1]}});
        assert_eq!(unwrap_data(&v)["files"][0], 1);
        let v = serde_json::json!({"data": {"x": 2}});
        assert_eq!(unwrap_data(&v)["x"], 2);
        let v = serde_json::json!({"items": [3]});
        assert_eq!(unwrap_data(&v)[0], 3);
        let v = serde_json::json!({"ok": 1});
        assert_eq!(unwrap_data(&v)["ok"], 1);
    }

    #[test]
    fn reads_rows_from_either_shape() {
        // `/stream-files` answers a BARE ARRAY (this is the shape that broke
        // tt11540908 "Carrie": 75 KB of files read as zero).
        let arr = serde_json::json!([
            {"short_code": "m9qZjD", "title": "Carrie.2026.S01E01.1080p.mkv"}
        ]);
        assert_eq!(rows_of(&arr, &["items"]).len(), 1);
        let obj = serde_json::json!({"items": [{"short_code": "a"}]});
        assert_eq!(rows_of(&obj, &["items", "files"]).len(), 1);
        let obj = serde_json::json!({"files": [{"short_code": "a"}, {"short_code": "b"}]});
        assert_eq!(rows_of(&obj, &["files"]).len(), 2);
        assert!(rows_of(&serde_json::json!({"message": "nope"}), &["items"]).is_empty());
    }

    #[test]
    fn parses_a_file_row() {
        let v = serde_json::json!({
            "short_code": "1v9OPj",
            "title": "Avengers.Infinity.War.2018.2160p.mkv",
            "caption": "Avengers Infinity War (2018)",
            "file_size": "3967229576",
            "mime": "video/x-matroska",
            "season_num": 1,
            "episode_num": 3
        });
        let f = wp_file_from(&v).expect("row");
        assert_eq!(f.short_code, "1v9OPj");
        assert_eq!(f.file_size, 3_967_229_576);
        assert_eq!(f.mime, "video/x-matroska");
        assert_eq!(f.season_num, 1);
        assert_eq!(f.episode_num, 3);
        assert!(wp_file_from(&serde_json::json!({"title": "no code"})).is_none());
    }

    #[test]
    fn post_year_from_title_or_field() {
        let p = serde_json::json!({"title": "The Boy 2026 • Movie"});
        assert_eq!(post_year(&p), "2026");
        let p = serde_json::json!({"title": "Something", "year": "2011"});
        assert_eq!(post_year(&p), "2011");
        let p = serde_json::json!({"title": "No Year Here"});
        assert_eq!(post_year(&p), "");
    }
}
