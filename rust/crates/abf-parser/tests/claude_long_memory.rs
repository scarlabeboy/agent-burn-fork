//! Single-test binary: a counting global allocator would otherwise see other tests' threads.
use abf_parser::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

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
            let now =
                CURRENT.fetch_add(new_size - layout.size(), SeqCst) + new_size - layout.size();
            PEAK.fetch_max(now, SeqCst);
        } else {
            CURRENT.fetch_sub(layout.size() - new_size, SeqCst);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn peak_during(record: &[u8]) -> usize {
    let baseline = CURRENT.load(SeqCst);
    PEAK.store(baseline, SeqCst);
    let result = parse_claude_long(record, Some("c"), LongLimits::MAX);
    let peak = PEAK.load(SeqCst) - baseline;
    assert!(result.observation().is_some());
    drop(result);
    peak
}

#[test]
fn heap_use_does_not_grow_with_record_size_or_content() {
    let build = |content_bytes: usize, elements: usize| {
        format!(
            r#"{{"sessionId":"s","requestId":"r","timestamp":"t","message":{{"id":"i","model":"m","content":[{{"text":"{}"}},[{}]],"usage":{{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation":{{"ephemeral_5m_input_tokens":4,"ephemeral_1h_input_tokens":5}}}}}}}}"#,
            "x".repeat(content_bytes),
            vec!["0"; elements].join(",")
        )
    };
    let small = build(1_000, 10);
    let large = build(4 * 1024 * 1024, 500_000);
    assert!(large.len() > 5_000_000 && large.len() <= LongLimits::MAX.record_bytes);
    let small_peak = peak_during(small.as_bytes());
    let large_peak = peak_during(large.as_bytes());
    // Retained strings, key candidates and the result only: far below the record size.
    assert!(large_peak <= 4096, "large peak {large_peak}");
    assert_eq!(small_peak, large_peak, "heap use depends on record size");
}
