//! Micro-benchmark for the on-disk cache layer — NOT a correctness test.
//!
//! Every cache read in the server is `metadata()` (for the TTL) + `read_to_string`
//! + `serde_json::from_str`. This measures that exact triple so the answer to "is
//! the file cache fast enough?" is a number and not an opinion.
//!
//!   cargo test --release --test bench_cache -- --nocapture
//!
//! Representative sizes are taken from the real `storage/cache/` when this is run
//! inside the repo; otherwise a synthetic body of the same shape is used.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn cache_dir() -> PathBuf {
    // <repo>/port/rust/tests/bench_cache.rs -> <repo>/storage/cache
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.join("../../storage/cache")
}

fn sample(prefix: &str) -> Option<Vec<u8>> {
    let dir = cache_dir();
    let mut best: Option<(u64, Vec<u8>)> = None;
    for e in fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with(prefix) || !name.ends_with(".json") {
            continue;
        }
        let len = e.metadata().ok()?.len();
        // Median-ish: keep the file closest to 1 KB, which is a typical entry.
        if best.as_ref().map(|(l, _)| len.abs_diff(1024) < l.abs_diff(1024)).unwrap_or(true) {
            if let Ok(b) = fs::read(e.path()) {
                best = Some((len, b));
            }
        }
    }
    best.map(|(_, b)| b)
}

fn bench(label: &str, body: &[u8], iters: u32) {
    let dir = std::env::temp_dir().join(format!("pm-bench-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("entry.json");
    fs::write(&path, body).unwrap();

    // Warm the page cache, then measure: stat + read + parse, exactly as a hit does.
    for _ in 0..100 {
        let _ = fs::metadata(&path).and_then(|m| m.modified());
        let t = fs::read_to_string(&path).unwrap();
        let _ = serde_json::from_str::<serde_json::Value>(&t);
    }

    let mut read_ns = 0u128;
    let mut parse_ns = 0u128;
    let mut stat_ns = 0u128;
    let mut ok = 0u32;
    let start = Instant::now();
    for _ in 0..iters {
        let t0 = Instant::now();
        let _ = fs::metadata(&path).and_then(|m| m.modified());
        stat_ns += t0.elapsed().as_nanos();

        let t1 = Instant::now();
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        read_ns += t1.elapsed().as_nanos();

        let t2 = Instant::now();
        if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
            ok += 1;
        }
        parse_ns += t2.elapsed().as_nanos();
    }
    let total = start.elapsed();
    let n = iters.max(1) as f64;
    println!(
        "{label:<26} {:>6} B  {} iters  total {:.1} ms  {:>7.1} us/hit  (stat {:.1} + read {:.1} + parse {:.1})  parsed {ok}",
        body.len(),
        iters,
        total.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1e6 / n,
        stat_ns as f64 / n / 1000.0,
        read_ns as f64 / n / 1000.0,
        parse_ns as f64 / n / 1000.0,
    );
    let _ = fs::remove_dir_all(&dir);
    // Keep the crate's clock dependency honest (avoids an "unused" warning).
    let _ = SystemTime::now().duration_since(UNIX_EPOCH);
}

/// The in-process alternative: `Mutex<HashMap<String, Value>>` get + clone, i.e.
/// exactly what `resolve.rs` does today as its L1 (and what an L1 in front of the
/// other prefixes would cost). Measured on the same run as the file numbers.
#[test]
fn cache_memory_hit_cost() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let value = serde_json::json!({
        "file_id_mt": "BQACAgQAAxkBAAIC1234567890abcdefghijklmnopqrstuvwxyz",
        "file_id": "AgADMG4AAqklwUs",
        "file_size": 1500000000u64,
        "file_name": "Some.Movie.2026.1080p.mkv",
        "mime": "video/x-matroska",
        "bot_id": "8273163250",
        "verified_at": 1792480000u64
    });
    let key = "abc123:8273163250".to_string();

    // (a) owned Value clone out of a mutex-guarded map — today's resolve.rs shape.
    let owned = Mutex::new(HashMap::new());
    owned.lock().unwrap().insert(key.clone(), value.clone());
    // (b) Arc<Value>: one atomic increment instead of a deep clone.
    let shared = Mutex::new(HashMap::new());
    shared.lock().unwrap().insert(key.clone(), Arc::new(value.clone()));

    const N: u32 = 2_000_000;
    let t = Instant::now();
    for _ in 0..N {
        let g = owned.lock().unwrap();
        let v = g.get(&key).unwrap().clone();
        std::hint::black_box(&v);
    }
    let clone_ns = t.elapsed().as_nanos() as f64 / N as f64;

    let t = Instant::now();
    for _ in 0..N {
        let g = shared.lock().unwrap();
        let v = g.get(&key).unwrap().clone();
        std::hint::black_box(&v);
    }
    let arc_ns = t.elapsed().as_nanos() as f64 / N as f64;

    println!(
        "memory Mutex<HashMap>           {N} iters  clone {:.3} us/hit   Arc {:.3} us/hit",
        clone_ns / 1000.0,
        arc_ns / 1000.0,
    );
    assert!(clone_ns < 100_000.0);
}

#[test]
fn cache_file_hit_cost() {
    let real_wp = sample("wp_json_");
    let real_resolve = sample("resolve_cache_");
    let real_stream = sample("stream_cache_");

    bench(
        "wp_json (real/synthetic)",
        real_wp.as_deref().unwrap_or_else(|| br#"{"ok":1,"total":40,"items":[{"short_code":"abc123","title":"Some Movie 2026 1080p WEB-DL x264","file_size":1500000000,"mime":"video/mp4","caption":"a caption of typical length that an uploader would write"},{"short_code":"def456","title":"Another Movie 2025 720p HDTV","file_size":700000000,"mime":"video/mp4"}]}"#),
        20_000,
    );
    bench(
        "resolve_cache (real/synthetic)",
        real_resolve.as_deref().unwrap_or_else(|| br#"{"file_id_mt":"BQACAgQAAxkBAAIC1234567890abcdefghijklmnopqrstuvwxyz","file_id":"AgADMG4AAqklwUs","file_size":1500000000,"file_name":"Some.Movie.2026.1080p.mkv","mime":"video/x-matroska","bot_id":"8273163250","verified_at":1792480000}"#),
        20_000,
    );
    bench(
        "stream_cache (real/synthetic)",
        real_stream.as_deref().unwrap_or_else(|| br#"{"streams":[{"name":"PencariMovie","description":"1080p WEB-DL | 1.4 GB","url":"http://127.0.0.1:8088/api/download/abc/f.mkv"}]}"#),
        20_000,
    );
}

/// The write side of the same comparison, for an honest file-vs-redb write cost.
#[test]
fn cache_file_write_cost() {
    let dir = std::env::temp_dir().join(format!("pm-bench-w-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let body = br#"{"file_id_mt":"BQACAgQAAxkBAAIC1234567890abcdefghijklmnopqrstuvwxyz","file_id":"AgADMG4AAqklwUs","file_size":1500000000,"file_name":"Some.Movie.2026.1080p.mkv","mime":"video/x-matroska","bot_id":"8273163250","verified_at":1792480000}"#;
    let path = dir.join("entry.json");
    const N: u32 = 2_000;
    let t = Instant::now();
    for _ in 0..N {
        fs::write(&path, body).unwrap();
    }
    let us = t.elapsed().as_nanos() as f64 / N as f64 / 1000.0;
    println!("file fs::write {} B            {us:.1} us/write", body.len());
    let _ = fs::remove_dir_all(&dir);
}
