//! Single-test binary: a counting global allocator must not see other tests' threads.
//! Fictional data under this crate's `target/abf134d-memory/` only.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

use abf_collection_boundary::*;

struct Counting;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let now = CURRENT.fetch_add(layout.size(), SeqCst) + layout.size();
        PEAK.fetch_max(now, SeqCst);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), SeqCst);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size >= layout.size() {
            let grown = new_size - layout.size();
            let now = CURRENT.fetch_add(grown, SeqCst) + grown;
            PEAK.fetch_max(now, SeqCst);
        } else {
            CURRENT.fetch_sub(layout.size() - new_size, SeqCst);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn heap_use_is_bounded_by_the_line_limit_not_by_the_file_size() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("abf134d-memory")
        .join(std::process::id().to_string());
    fs::create_dir_all(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let _cleanup = Cleanup(root.clone());

    let valid = format!(
        r#"{{"sessionId":"s","requestId":"r","message":{{"id":"m","model":"x","content":"{}","usage":{{"input_tokens":1}}}}}}"#,
        "c".repeat(100 * 1024)
    );
    let oversized = "o".repeat(1024 * 1024);
    let mut file = String::new();
    for _ in 0..40 {
        file.push_str(&valid);
        file.push('\n');
    }
    for _ in 0..10 {
        file.push_str(&oversized);
        file.push('\n');
    }
    file.push_str(&oversized); // unterminated giant tail
    assert!(file.len() > 15_000_000);
    fs::write(root.join("big.jsonl"), &file).unwrap();
    let file_len = file.len();
    drop(file);
    drop(oversized);
    drop(valid);

    let id = |v: &str| OpaqueId::new(v).unwrap();
    let scope = CollectionScope::new(
        EnvironmentIdentity::wsl(id("env"), WslDistroName::new("Fictional").unwrap()),
        SourceInstance::new(id("src"), Harness::ClaudeCode),
        AuthorizedRoot::new(&root).unwrap(),
        HostPolicy::RequireNative,
    )
    .unwrap()
    .with_limits(CollectionLimits {
        max_file_bytes: 64 * 1024 * 1024,
        ..CollectionLimits::default()
    })
    .unwrap();
    let limits = ClaudeReadLimits {
        max_line_bytes: 128 * 1024,
        ..ClaudeReadLimits::default()
    };
    let run = ScanRun::new(id("run"), CaptureTime::NotRecorded);

    let baseline = CURRENT.load(SeqCst);
    PEAK.store(baseline, SeqCst);
    let batch = collect_claude_jsonl_root(&scope, &run, limits).unwrap();
    let peak = PEAK.load(SeqCst) - baseline;

    let totals = batch.totals();
    assert_eq!(totals.counts.observations, 40);
    assert_eq!(totals.counts.oversized, 10);
    assert_eq!(totals.pending_tail_bytes, 1024 * 1024);
    let retained = CURRENT.load(SeqCst) - baseline;
    // Line buffer (<= 2 x 128 KiB with Vec growth) + one 64 KiB chunk + retained outcomes.
    assert!(
        peak < 1024 * 1024,
        "peak {peak} bytes for a {file_len}-byte file"
    );
    assert!(retained < 256 * 1024, "retained {retained} bytes");
    drop(batch);
}
