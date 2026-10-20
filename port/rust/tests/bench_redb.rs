//! redb vs the one-JSON-file-per-entry cache — measured, not assumed.
//!
//!   cargo test --release --test bench_redb -- --nocapture
//!
//! The file numbers to beat come from `tests/bench_cache.rs` (same machine, same
//! run family): **~190-226 us/hit** for `stat + read_to_string + serde_json`,
//! of which only ~4-10 us is the JSON parse and ~76-92 us is the `stat` that
//! carries the TTL. So the interesting question is not "does redb parse faster"
//! (it cannot — we still deserialize) but "how much of those two syscalls does an
//! mmap'd B-tree remove".
//!
//! redb is a dev-dependency on purpose: this is a decision benchmark, and the
//! shipped binary must not grow an embedded database just to satisfy curiosity.

use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableTable, TableDefinition};

/// `&str` keys are what the cache already uses (`resolve_cache_<md5>`) and
/// `&[u8]` values are the serialized body, i.e. `serde_json::to_vec` output.
const CACHE: TableDefinition<&str, &[u8]> = TableDefinition::new("cache");

const N: u32 = 20_000;

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pm-redb-bench-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The same body shape `bench_cache.rs` uses for `resolve_cache`.
fn body() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file_id_mt": "BQACAgQAAxkBAAIC1234567890abcdefghijklmnopqrstuvwxyz",
        "file_id": "AgADMG4AAqklwUs",
        "file_size": 1500000000u64,
        "file_name": "Some.Movie.2026.1080p.mkv",
        "mime": "video/x-matroska",
        "bot_id": "8273163250",
        "verified_at": 1792480000u64
    }))
    .unwrap()
}

#[test]
fn redb_read_write_and_open_cost() {
    let dir = scratch();
    let path = dir.join("cache.redb");
    let body = body();
    println!("body                          {} B", body.len());

    // ---- cold create + bulk insert -------------------------------------------
    let t = Instant::now();
    let db = Database::create(&path).unwrap();
    let create_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    {
        let wtx = db.begin_write().unwrap();
        {
            let mut table = wtx.open_table(CACHE).unwrap();
            for i in 0..N {
                let key = format!("resolve_cache_{i:040}");
                table.insert(key.as_str(), body.as_slice()).unwrap();
            }
        }
        wtx.commit().unwrap();
    }
    let bulk_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("redb create                   {create_ms:.1} ms   bulk insert {N} = {bulk_ms:.1} ms ({:.1} us/entry)", bulk_ms * 1000.0 / N as f64);

    // ---- read: long-lived txn, no deserialize (the floor) --------------------
    const READS: u32 = 200_000;
    let t = Instant::now();
    {
        let rtx = db.begin_read().unwrap();
        let table = rtx.open_table(CACHE).unwrap();
        for i in 0..READS {
            let key = format!("resolve_cache_{:040}", i % N);
            let v = table.get(key.as_str()).unwrap().unwrap();
            std::hint::black_box(v.value());
        }
    }
    // NB: `as_nanos()` is NANOseconds — divide by 1000 for us (an earlier version
    // printed ns as us and made redb look 1000x slower than it is).
    let ns = t.elapsed().as_nanos() as f64 / READS as f64;
    println!("redb get, one txn, no parse   {:7.2} us/hit", ns / 1000.0);

    // ---- read: a fresh read txn per lookup (what a route would do) -----------
    let t = Instant::now();
    for i in 0..READS {
        let key = format!("resolve_cache_{:040}", i % N);
        let rtx = db.begin_read().unwrap();
        let table = rtx.open_table(CACHE).unwrap();
        let v = table.get(key.as_str()).unwrap().unwrap();
        std::hint::black_box(v.value());
    }
    let ns = t.elapsed().as_nanos() as f64 / READS as f64;
    println!("redb get, fresh txn, no parse {:7.2} us/hit", ns / 1000.0);

    // ---- read: fresh txn + serde_json::from_slice (a real cache hit) --------
    let t = Instant::now();
    let mut parsed = 0u32;
    for i in 0..READS {
        let key = format!("resolve_cache_{:040}", i % N);
        let rtx = db.begin_read().unwrap();
        let table = rtx.open_table(CACHE).unwrap();
        let v = table.get(key.as_str()).unwrap().unwrap();
        if serde_json::from_slice::<serde_json::Value>(v.value()).is_ok() {
            parsed += 1;
        }
    }
    let ns = t.elapsed().as_nanos() as f64 / READS as f64;
    println!(
        "redb get + serde_json         {:7.2} us/hit  (parsed {parsed})",
        ns / 1000.0
    );

    // ---- write: one entry per transaction (the save path) -------------------
    // A default commit is a FULL fsync — right for a database, wrong for a cache.
    // Durability::None is the setting that makes per-request writes affordable.
    const WRITES: u32 = 500;
    // redb's own docs: `None` commits are "not persisted to disk unless followed by
    // a commit with a higher durability level", and "exclusively using this
    // durability level will result in rapid growth of the database file" (pages are
    // only freed by higher-durability commits). So `None` is unusable for a cache
    // that must survive an app kill — `Eventual` is the interesting one.
    for (label, durability) in [
        ("redb commit (Immediate)", redb::Durability::Immediate),
        ("redb commit (Eventual)", redb::Durability::Eventual),
        ("redb commit (None)", redb::Durability::None),
    ] {
        let t = Instant::now();
        for i in 0..WRITES {
            let mut wtx = db.begin_write().unwrap();
            wtx.set_durability(durability); // returns () in redb 2.6
            {
                let mut table = wtx.open_table(CACHE).unwrap();
                let key = format!("hot_{i:040}");
                table.insert(key.as_str(), body.as_slice()).unwrap();
            }
            wtx.commit().unwrap();
        }
        let us = t.elapsed().as_nanos() as f64 / WRITES as f64 / 1000.0;
        println!("{label:<29} {us:7.2} us/write");
    }
    let _ = std::fs::remove_dir_all(&dir);

    // ---- reopen cost (process start / after a crash) ------------------------
    let t = Instant::now();
    let path2 = scratch().join("reopen.redb");
    let db2 = Database::create(&path2).unwrap();
    {
        let wtx = db2.begin_write().unwrap();
        {
            let mut table = wtx.open_table(CACHE).unwrap();
            for i in 0..N {
                let key = format!("resolve_cache_{i:040}");
                table.insert(key.as_str(), body.as_slice()).unwrap();
            }
        }
        wtx.commit().unwrap();
    }
    let reopen_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("redb full {N}-entry reopen    {reopen_ms:.1} ms (create+insert)");
    if let Some(p) = path2.parent() {
        let _ = std::fs::remove_dir_all(p);
    }
}
