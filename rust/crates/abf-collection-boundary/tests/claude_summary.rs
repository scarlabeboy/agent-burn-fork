//! File-to-summary tests (ABF134E). Fictional trees under this crate's `target/abf134e/` only.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use abf_collection_boundary::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SENTINEL: &str = "ABF134E-SENTINEL-CONTENT";
const SECRET_DIR: &str = "SECRET-PROJECT-NAME";

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("abf134e")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).unwrap()
}
fn scope(tree: &Tree) -> CollectionScope {
    CollectionScope::new(
        EnvironmentIdentity::wsl(
            id("env-wsl"),
            WslDistroName::new("Fictional-Ubuntu").unwrap(),
        ),
        SourceInstance::new(id("src-claude"), Harness::ClaudeCode),
        AuthorizedRoot::new(&tree.0).unwrap(),
        HostPolicy::RequireNative,
    )
    .unwrap()
}
fn summarize(tree: &Tree, limits: ClaudeReadLimits) -> ClaudeSummary {
    let run = ScanRun::new(id("run-1"), CaptureTime::UnixMicros(1_780_000_000_000_000));
    collect_claude_jsonl_root(&scope(tree), &run, limits)
        .unwrap()
        .summary()
}

fn record(tag: &str, usage: &str, content: &str) -> String {
    format!(
        r#"{{"sessionId":"sess-{tag}","requestId":"req-{tag}","timestamp":"2026-01-02T03:04:05.678Z","cwd":"/fictional/{SENTINEL}","message":{{"id":"msg-{tag}","model":"fict-model","content":{content},"usage":{usage}}}}}"#
    )
}
const FLAT: &str = r#"{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}"#;
const SPLIT: &str = r#"{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":99,"cache_creation":{"ephemeral_5m_input_tokens":5,"ephemeral_1h_input_tokens":6}}"#;
const ZERO: &str = r#"{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}"#;
const INVALID: &str = r#"{"input_tokens":-5,"output_tokens":7}"#;
const NULLED: &str = r#"{"input_tokens":null}"#;

fn text(content: &str) -> String {
    format!(r#"[{{"type":"text","text":"{content}"}}]"#)
}

#[test]
fn fictional_files_produce_the_known_summary() {
    let tree = Tree::new();
    let line1 = record("1", FLAT, &text(SENTINEL));
    // Eight LF-terminated records and lines, then an unterminated tail.
    let a = [
        line1.clone(),                                // 1: 10/20/cc3/cr4
        record("2", SPLIT, &text(SENTINEL)),          // 2: 1/2/cc 5+6=11 (flat 99 ignored)/no cr
        record("3", ZERO, "[]"),                      // 3: explicit zeros
        line1,                                        // 4: duplicated claim, counted again
        record("5", INVALID, "[]"),                   // 5: input invalid, output 7
        record("6", NULLED, "[]"),                    // 6: null only: incomplete
        r#"{"message":{"id":"x"}}"#.to_owned(),       // 7: no usage object: incomplete
        r#"{"message":"#.to_owned(),                  // 8: malformed
        "  ".to_owned(),                              // 9: blank
        record("10", FLAT, &text(&"z".repeat(6000))), // 10: oversized for the 4096-byte limit
    ];
    let tail = r#"{"message":{"usage":{"input_tokens":999}}}"#;
    tree.write(
        &format!("{SECRET_DIR}/a.jsonl"),
        format!("{}\n{tail}", a.join("\n")),
    );
    tree.write("b.jsonl", format!("{}\n", record("b", FLAT, "[]")));
    tree.write("readme.txt", "out of scope");

    let limits = ClaudeReadLimits {
        max_line_bytes: 4096,
        ..ClaudeReadLimits::default()
    };
    let summary = summarize(&tree, limits);

    assert_eq!(summary.basis, CountingBasis::ObservedRecordClaims);
    assert_eq!(summary.duplicates, DuplicateIdentity::Unresolved);
    assert_eq!(summary.completeness, Completeness::Complete);
    assert_eq!(
        summary.environment.wsl_distro().unwrap().as_str(),
        "Fictional-Ubuntu"
    );
    assert_eq!(summary.host_match, HostMatch::Native);

    // Sums of observed record claims. The duplicate (line 4) and the unterminated tail
    // (999) behave as documented: duplicates count, the tail is never parsed.
    let c = summary.components;
    assert_eq!(
        c.input.value,
        ComponentValue::ReportedClaimSum(31) // lines 1+2+3(zero)+4+b = 10+1+0+10+10
    ); // 31; line 5 is invalid
    assert_eq!(
        c.output.value,
        ComponentValue::ReportedClaimSum(69) // lines 1+2+3(zero)+4+5+b = 20+2+0+20+7+20
    ); // 69
    assert_eq!(
        c.cache_creation.value,
        ComponentValue::ReportedClaimSum(20) // lines 1+2+3(zero)+4+b = 3+11+0+3+3
    ); // 20, flat 99 not added
    assert_eq!(
        c.cache_read.value,
        ComponentValue::ReportedClaimSum(12) // lines 1+3(zero)+4+b = 4+0+4+4
    ); // 12
    assert_eq!(
        c.input.states,
        ClaimStates {
            valid: 5,
            valid_zero: 1,
            absent: 0,
            null: 1,
            wrong_type: 0,
            out_of_range: 1,
            unsupported: 1
        }
    );
    assert_eq!(
        c.output.states,
        ClaimStates {
            valid: 6,
            valid_zero: 1,
            absent: 1,
            null: 0,
            wrong_type: 0,
            out_of_range: 0,
            unsupported: 1
        }
    );
    assert_eq!(
        c.cache_creation.states,
        ClaimStates {
            valid: 5,
            valid_zero: 1,
            absent: 2,
            null: 0,
            wrong_type: 0,
            out_of_range: 0,
            unsupported: 1
        }
    );
    assert_eq!(
        c.cache_read.states,
        ClaimStates {
            valid: 4,
            valid_zero: 1,
            absent: 3,
            null: 0,
            wrong_type: 0,
            out_of_range: 0,
            unsupported: 1
        }
    );

    assert_eq!(
        summary.records,
        RecordCounts {
            observations: 8,
            accepted: 5,
            qualified: 1,
            incomplete: 2,
            with_valid_claim: 6,
            without_valid_claim: 2,
            with_invalid_claim: 1,
            with_null_claim: 1,
            with_missing_claim: 4,
            cache_creation_flat_ignored: 1,
        }
    );
    assert_eq!(
        summary.records.accepted + summary.records.qualified + summary.records.incomplete,
        summary.records.observations
    );
    assert_eq!(summary.lines.terminated_lines, 11);
    assert_eq!(summary.lines.rejected.incomplete_json, 1);
    assert_eq!(summary.lines.rejected.total(), 1);
    assert_eq!((summary.lines.oversized, summary.lines.blank), (1, 1));
    assert_eq!(summary.lines.files_with_pending_tail, 1);
    assert_eq!(summary.lines.pending_tail_bytes, tail.len() as u64);
    assert_eq!((summary.files.read, summary.files.rejected), (2, 0));
    assert_eq!(
        summary.skipped,
        SkipSummary {
            total: 1,
            may_hide_data: 0,
            out_of_scope: 1
        }
    );
    // Complete traversal, yet a partial view: unparsed lines and a pending tail.
    assert_eq!(
        summary.caveats,
        Caveats {
            incomplete_batch: false,
            unparsed_lines: true,
            pending_tails: true,
            rejected_files: false,
            skipped_in_scope: false,
        }
    );
}

#[test]
fn partial_coverage_is_reported_and_readable_claims_survive() {
    let tree = Tree::new();
    let outside = Tree::new();
    let line = format!("{}\n", record("x", FLAT, "[]"));
    outside.write("secret.jsonl", &line);
    for name in ["a", "b", "c"] {
        tree.write(&format!("{name}.jsonl"), &line);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.0.join("secret.jsonl"), tree.0.join("link.jsonl")).unwrap();
    fs::hard_link(outside.0.join("secret.jsonl"), tree.0.join("hard.jsonl")).unwrap();
    let limits = ClaudeReadLimits {
        max_total_bytes: line.len() as u64 * 2,
        ..ClaudeReadLimits::default()
    };
    let summary = summarize(&tree, limits);
    assert_eq!(summary.completeness, Completeness::Incomplete);
    assert!(summary.caveats.incomplete_batch && summary.caveats.rejected_files);
    assert!(summary.caveats.skipped_in_scope);
    assert_eq!(summary.skipped.may_hide_data, 1); // the symlink
    assert_eq!(summary.files.path, 1); // the hard link
    assert_eq!(summary.files.byte_budget, 1); // a file over the byte budget
    assert_eq!(summary.files.read, 2);
    // Two files read; the withheld ones are not zeros.
    assert_eq!(
        summary.components.input.value,
        ComponentValue::ReportedClaimSum(20)
    );
    assert_eq!(summary.records.observations, 2);
}

#[test]
fn nothing_collected_is_indeterminate_not_zero() {
    let tree = Tree::new();
    tree.write("notes.txt", "x");
    let summary = summarize(&tree, ClaudeReadLimits::default());
    assert_eq!(summary.completeness, Completeness::Complete);
    assert_eq!(
        summary.components.input.value,
        ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
    );
    assert_eq!(summary.files, FileSummary::default());
}

#[test]
fn summary_exposes_no_content_paths_or_identifiers() {
    let tree = Tree::new();
    tree.write(
        &format!("{SECRET_DIR}/s.jsonl"),
        format!(
            "{}\n{}\n",
            record("one", FLAT, &text(SENTINEL)),
            record("two", SPLIT, &text(SENTINEL))
        ),
    );
    let summary = summarize(&tree, ClaudeReadLimits::default());
    let debug = format!("{summary:?}");
    for secret in [
        SENTINEL,
        SECRET_DIR,
        "fictional",
        "sess-one",
        "req-one",
        "msg-one",
        "fict-model",
        "2026-01-02",
        tree.0.to_str().unwrap(),
        "target/abf134e",
    ] {
        assert!(!debug.contains(secret), "Debug leaked {secret}");
    }
    // Only the caller's own labels remain as provenance.
    assert!(debug.contains("run-1") && debug.contains("src-claude"));
    assert_eq!(summary, summarize(&tree, ClaudeReadLimits::default()));
}
