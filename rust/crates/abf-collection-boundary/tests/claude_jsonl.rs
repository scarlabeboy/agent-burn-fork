//! Fictional Claude-shaped JSONL collection tests (ABF134D). Every tree is created under this
//! crate's `target/abf134d/` directory and removed afterwards.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use abf_collection_boundary::*;
use abf_parser::{Code, Field, Status};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SENTINEL: &str = "ABF134D-SENTINEL-CONTENT";
const SECRET_DIR: &str = "SECRET-PROJECT-NAME";

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("abf134d")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0); // does not follow symbolic links
    }
}

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).unwrap()
}
fn scope_at(root: &Path, harness: Harness) -> CollectionScope {
    CollectionScope::new(
        EnvironmentIdentity::wsl(
            id("env-wsl"),
            WslDistroName::new("Fictional-Ubuntu").unwrap(),
        ),
        SourceInstance::new(id("src-claude"), harness),
        AuthorizedRoot::new(root).unwrap(),
        HostPolicy::RequireNative,
    )
    .unwrap()
}
fn scope(tree: &Tree) -> CollectionScope {
    scope_at(&tree.0, Harness::ClaudeCode)
}
fn run() -> ScanRun {
    ScanRun::new(id("run-1"), CaptureTime::UnixMicros(1_780_000_000_000_000))
}
fn collect(scope: &CollectionScope, limits: ClaudeReadLimits) -> ClaudeBatch {
    collect_claude_jsonl_root(scope, &run(), limits).expect("collection")
}
fn default(scope: &CollectionScope) -> ClaudeBatch {
    collect(scope, ClaudeReadLimits::default())
}

fn record(tag: &str, usage: &str, content: &str) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"sess-{tag}","requestId":"req-{tag}","timestamp":"2026-01-02T03:04:05.678Z","cwd":"/fictional/{SENTINEL}","message":{{"id":"msg-{tag}","model":"fict-model","content":{content},"usage":{usage}}}}}"#
    )
}
const FLAT: &str = r#"{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}"#;
const SPLIT: &str = r#"{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":99,"cache_creation":{"ephemeral_5m_input_tokens":5,"ephemeral_1h_input_tokens":6}}"#;

fn text(content: &str) -> String {
    format!(r#"[{{"type":"text","text":"{content}"}}]"#)
}

#[test]
fn single_file_preserves_the_root_pipeline_without_enumerating_siblings() {
    let tree = Tree::new();
    let line = record("single", r#"{"input_tokens":0,"output_tokens":null}"#, "[]");
    tree.write(
        "chosen.jsonl",
        format!("{line}\n{{\"message\":\nunfinished"),
    );
    let scope = scope(&tree);
    let expected = default(&scope);
    assert_eq!(expected.totals().counts.rejected, 1);
    assert_eq!(expected.totals().pending_tail_bytes, 10);
    assert_eq!(
        expected.observations().next().unwrap().observation.input,
        Field::Valid(0)
    );
    assert_eq!(
        expected.observations().next().unwrap().observation.output,
        Field::Null
    );
    tree.write("sibling.jsonl", "not JSON\n");
    let scope = scope
        .with_limits(CollectionLimits {
            max_entries: 1,
            ..CollectionLimits::default()
        })
        .unwrap();
    let actual =
        collect_claude_jsonl_file(&scope, &run(), "chosen.jsonl", ClaudeReadLimits::default())
            .unwrap();
    assert_eq!(actual.summary(), expected.summary());
    assert_eq!(actual, expected);
}

#[test]
fn single_file_rejects_missing_paths_wrong_extensions_and_directories() {
    let tree = Tree::new();
    tree.write("wrong.txt", "{}\n");
    fs::create_dir(tree.0.join("directory.jsonl")).unwrap();
    for relative in [
        "missing.jsonl",
        "wrong.txt",
        "directory.jsonl",
        "../outside.jsonl",
        "/absolute.jsonl",
        "a//b.jsonl",
        "a\\b.jsonl",
        "C:stream.jsonl",
    ] {
        let batch =
            collect_claude_jsonl_file(&scope(&tree), &run(), relative, ClaudeReadLimits::default())
                .unwrap();
        assert!(
            matches!(
                batch.files[0].status,
                ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(_))
            ),
            "{relative}"
        );
        assert_eq!(batch.completeness, Completeness::Incomplete);
        assert_eq!(batch.files[0].counts, ClaudeLineCounts::default());
        assert!(batch.files[0].lines.is_empty());
        assert_eq!(batch.files[0].pending_tail_bytes, 0);
        assert!(batch.summary().caveats.rejected_files);
    }
    let missing = collect_claude_jsonl_file(
        &scope(&tree),
        &run(),
        "missing.jsonl",
        ClaudeReadLimits::default(),
    )
    .unwrap();
    assert_eq!(
        missing.files[0].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(PathRejection::Io(
            std::io::ErrorKind::NotFound
        )))
    );
}

#[test]
fn single_file_enforces_file_byte_and_retained_outcome_limits() {
    let tree = Tree::new();
    let line = format!("{}\n", record("limit", FLAT, "[]"));
    let bytes = line.len() as u64;
    tree.write("a.jsonl", &line);
    let scope = scope(&tree)
        .with_limits(CollectionLimits {
            max_file_bytes: bytes,
            ..CollectionLimits::default()
        })
        .unwrap();
    let exact = ClaudeReadLimits {
        max_total_bytes: bytes,
        max_records: 1,
        max_line_bytes: line.len() - 1,
        ..ClaudeReadLimits::default()
    };
    let batch = collect_claude_jsonl_file(&scope, &run(), "a.jsonl", exact).unwrap();
    assert_eq!(batch.totals().counts.observations, 1);
    let byte_limited = collect_claude_jsonl_file(
        &scope,
        &run(),
        "a.jsonl",
        ClaudeReadLimits {
            max_total_bytes: bytes - 1,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(
        byte_limited.files[0].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::ByteBudget)
    );
    tree.write("a.jsonl", format!("{line}\n"));
    let too_large = collect_claude_jsonl_file(&scope, &run(), "a.jsonl", exact).unwrap();
    assert_eq!(
        too_large.files[0].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::TooLarge)
    );
    tree.write("a.jsonl", "{}\nnot-json\n");
    let output_limited = collect_claude_jsonl_file(&scope, &run(), "a.jsonl", exact).unwrap();
    assert_eq!(
        output_limited.files[0].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::OutputLimit)
    );
    assert!(output_limited.files[0].lines.is_empty());
    assert_eq!(output_limited.files[0].counts, ClaudeLineCounts::default());
    assert_eq!(output_limited.completeness, Completeness::Incomplete);
}

#[test]
fn single_file_line_limit_reports_oversized_records_and_unparsed_tails() {
    let tree = Tree::new();
    tree.write("a.jsonl", "123456\n123456");
    let batch = collect_claude_jsonl_file(
        &scope(&tree),
        &run(),
        "a.jsonl",
        ClaudeReadLimits {
            max_line_bytes: 5,
            ..ClaudeReadLimits::default()
        },
    )
    .unwrap();
    assert_eq!(batch.files[0].status, ClaudeFileStatus::Read);
    assert_eq!(
        batch.files[0].lines[0].outcome,
        ClaudeLineOutcome::Oversized
    );
    assert_eq!(batch.files[0].lines[0].byte_length, 6);
    assert_eq!(batch.files[0].pending_tail_bytes, 6);
    assert!(batch.summary().caveats.unparsed_lines);
    assert!(batch.summary().caveats.pending_tails);
}

#[test]
fn single_file_rejects_invalid_limits_wrong_harness_and_changed_root() {
    let tree = Tree::new();
    let scope = scope(&tree);
    assert_eq!(
        collect_claude_jsonl_file(
            &scope,
            &run(),
            "a.jsonl",
            ClaudeReadLimits {
                max_records: 0,
                ..ClaudeReadLimits::default()
            }
        )
        .unwrap_err(),
        ClaudeCollectError::InvalidLimits
    );
    assert_eq!(
        collect_claude_jsonl_file(
            &scope_at(&tree.0, Harness::Codex),
            &run(),
            "a.jsonl",
            ClaudeReadLimits::default()
        )
        .unwrap_err(),
        ClaudeCollectError::WrongHarness
    );
    fs::remove_dir(&tree.0).unwrap();
    assert!(matches!(
        collect_claude_jsonl_file(&scope, &run(), "a.jsonl", ClaudeReadLimits::default()),
        Err(ClaudeCollectError::Boundary(CollectError::RootChanged(_)))
    ));
}

#[cfg(unix)]
#[test]
fn single_file_rejects_detected_links_and_linked_parents() {
    let tree = Tree::new();
    tree.write("real.jsonl", "{}\n");
    std::os::unix::fs::symlink(tree.0.join("real.jsonl"), tree.0.join("link.jsonl")).unwrap();
    std::os::unix::fs::symlink(&tree.0, tree.0.join("parent")).unwrap();
    fs::hard_link(tree.0.join("real.jsonl"), tree.0.join("hard.jsonl")).unwrap();
    for (path, reason) in [
        ("link.jsonl", PathRejection::Symlink),
        ("parent/real.jsonl", PathRejection::SymlinkComponent),
        ("hard.jsonl", PathRejection::HardLinked),
    ] {
        let batch =
            collect_claude_jsonl_file(&scope(&tree), &run(), path, ClaudeReadLimits::default())
                .unwrap();
        assert_eq!(
            batch.files[0].status,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(reason))
        );
        assert!(batch.files[0].lines.is_empty());
    }
}

#[test]
fn multiple_files_yield_claims_provenance_and_counts() {
    let tree = Tree::new();
    let long = text(&format!("{SENTINEL} {}", "long ".repeat(60_000)));
    assert!(long.len() > 65_536);
    let a1 = record("a1", FLAT, &text(SENTINEL));
    let a2 = record("a2", SPLIT, &long);
    tree.write(&format!("{SECRET_DIR}/b.jsonl"), format!("{a1}\n{a2}\n"));
    let c1 = record("c1", r#"{"input_tokens":0,"output_tokens":null}"#, "[]");
    tree.write("a/c.jsonl", format!("{c1}\n"));
    tree.write("top.jsonl", format!("{}\n", record("t1", FLAT, "[]")));
    tree.write("notes.txt", "ignored");

    let scope = scope(&tree);
    let batch = default(&scope);
    assert_eq!(batch.completeness, Completeness::Complete);
    assert_eq!(batch.host_match, HostMatch::Native);
    // Sorted by relative path: "SECRET-PROJECT-NAME/b.jsonl" < "a/c.jsonl" < "top.jsonl".
    let ordinals: Vec<u32> = batch.files.iter().map(|f| f.file_ordinal).collect();
    assert_eq!(ordinals, [0, 1, 2]);
    assert!(
        batch
            .files
            .iter()
            .all(|f| f.status == ClaudeFileStatus::Read)
    );
    // Non-.jsonl files are out of scope, not an incompleteness.
    assert!(
        batch
            .skipped
            .iter()
            .all(|s| s.reason == SkipReason::NotFixtureExtension)
    );
    assert_eq!(batch.skipped.len(), 1);
    assert!(batch.skipped.iter().all(|s| s.relative_path.is_none()));

    let records: Vec<_> = batch.observations().collect();
    assert_eq!(records.len(), 4);
    let first = &records[0];
    assert_eq!(first.scan_run, &id("run-1"));
    assert_eq!(first.environment, scope.environment());
    assert_eq!(first.source, scope.source());
    assert_eq!(
        (first.file_ordinal, first.line_ordinal, first.byte_offset),
        (0, 1, 0)
    );
    assert_eq!(first.byte_length, a1.len() as u64);
    assert_eq!(first.observation.input, Field::Valid(10));
    assert_eq!(first.observation.cache_creation(), Field::Valid(3));
    assert_eq!(first.observation.session_id, Field::Valid("sess-a1".into()));
    assert_eq!(first.observation.status, Status::Accepted);
    // A long skipped content array neither disturbs claims nor is retained.
    let second = &records[1];
    assert_eq!(
        (second.line_ordinal, second.byte_offset),
        (2, a1.len() as u64 + 1)
    );
    assert_eq!(second.byte_length, a2.len() as u64);
    assert!(second.byte_length > 300_000);
    assert_eq!(second.observation.cache_creation_flat, Field::Valid(99));
    assert_eq!(second.observation.cache_creation(), Field::Valid(11));
    assert_eq!(second.observation.output, Field::Valid(2));
    // Null stays distinct from zero.
    let third = &records[2];
    assert_eq!(third.file_ordinal, 1);
    assert_eq!(third.observation.input, Field::Valid(0));
    assert_eq!(third.observation.output, Field::Null);
    assert_eq!(third.observation.status, Status::Qualified);
    assert_eq!(records[3].file_ordinal, 2);

    let totals = batch.totals();
    assert_eq!((totals.files_read, totals.files_rejected), (3, 0));
    assert_eq!(totals.counts.terminated_lines, 4);
    assert_eq!(totals.counts.observations, 4);
    assert_eq!(totals.pending_tail_bytes, 0);
}

#[test]
fn missing_final_lf_is_a_pending_tail_never_parsed() {
    let tree = Tree::new();
    let complete = record("p1", FLAT, "[]");
    let tail = record("p2", FLAT, "[]");
    tree.write("s.jsonl", format!("{complete}\n{tail}"));
    tree.write("only-tail.jsonl", &tail);
    let batch = default(&scope(&tree));
    assert_eq!(batch.completeness, Completeness::Complete);
    // Sorted: "only-tail.jsonl" (ordinal 0) before "s.jsonl" (ordinal 1).
    let file = &batch.files[1];
    assert_eq!(file.status, ClaudeFileStatus::Read);
    assert_eq!(file.counts.terminated_lines, 1);
    assert_eq!(file.counts.observations, 1);
    assert_eq!(file.pending_tail_bytes, tail.len() as u64);
    assert_eq!(batch.files[0].counts, ClaudeLineCounts::default());
    assert_eq!(batch.files[0].pending_tail_bytes, tail.len() as u64);
    let totals = batch.totals();
    assert_eq!(totals.files_with_pending_tail, 2);
    assert_eq!(batch.observations().count(), 1);
}

#[test]
fn line_outcomes_are_counted_separately() {
    let tree = Tree::new();
    let ok = record("ok", FLAT, "[]");
    let oversized = record("big", FLAT, &text(&"z".repeat(5_000)));
    let content = [
        ok.clone(),
        String::new(),
        "   \t\r".to_owned(),
        "{\"message\":".to_owned(),
        "[1,2]".to_owned(),
        r#"{"sessionId":"a","sessionId":"b"}"#.to_owned(),
        oversized.clone(),
        format!("{ok}\r"),
    ]
    .join("\n")
        + "\n";
    tree.write("s.jsonl", &content);
    let limits = ClaudeReadLimits {
        max_line_bytes: 2048,
        ..ClaudeReadLimits::default()
    };
    let batch = collect(&scope(&tree), limits);
    assert_eq!(batch.completeness, Completeness::Complete);
    let file = &batch.files[0];
    assert_eq!(
        file.counts,
        ClaudeLineCounts {
            terminated_lines: 8,
            observations: 2,
            rejected: 3,
            oversized: 1,
            blank: 2
        }
    );
    // Blank lines are counted, not retained; everything else keeps its physical locator.
    let ordinals: Vec<u64> = file.lines.iter().map(|l| l.line_ordinal).collect();
    assert_eq!(ordinals, [1, 4, 5, 6, 7, 8]);
    let outcomes: Vec<_> = file.lines.iter().map(|l| &l.outcome).collect();
    assert!(matches!(outcomes[0], ClaudeLineOutcome::Observation(_)));
    assert_eq!(
        outcomes[1],
        &ClaudeLineOutcome::Rejected(Code::IncompleteJson)
    );
    assert_eq!(outcomes[2], &ClaudeLineOutcome::Rejected(Code::RootShape));
    assert_eq!(
        outcomes[3],
        &ClaudeLineOutcome::Rejected(Code::DuplicateKey)
    );
    assert_eq!(outcomes[4], &ClaudeLineOutcome::Oversized);
    assert!(matches!(outcomes[5], ClaudeLineOutcome::Observation(_)));
    assert_eq!(file.lines[4].byte_length, oversized.len() as u64);
    let expected_offset: usize = content.split('\n').take(6).map(|l| l.len() + 1).sum();
    assert_eq!(file.lines[4].byte_offset, expected_offset as u64);
    assert_eq!(file.lines[5].byte_length, ok.len() as u64 + 1); // CR counts as length
    assert_eq!(batch.totals().counts, file.counts);
}

#[test]
fn line_length_boundary_is_exact() {
    let tree = Tree::new();
    let ok = record("edge", FLAT, "[]");
    tree.write("a.jsonl", format!("{ok}\n"));
    let at = ClaudeReadLimits {
        max_line_bytes: ok.len(),
        ..ClaudeReadLimits::default()
    };
    let below = ClaudeReadLimits {
        max_line_bytes: ok.len() - 1,
        ..ClaudeReadLimits::default()
    };
    assert_eq!(collect(&scope(&tree), at).files[0].counts.observations, 1);
    assert_eq!(collect(&scope(&tree), below).files[0].counts.oversized, 1);
}

#[test]
fn oversized_and_unterminated_giants_are_not_buffered_or_parsed() {
    let tree = Tree::new();
    let giant = "g".repeat(3 * 1024 * 1024);
    tree.write(
        "a.jsonl",
        format!("{giant}\n{}\n{giant}", record("z", FLAT, "[]")),
    );
    let limits = ClaudeReadLimits {
        max_line_bytes: 4096,
        ..ClaudeReadLimits::default()
    };
    let batch = collect(&scope(&tree), limits);
    let file = &batch.files[0];
    assert_eq!(file.counts.oversized, 1);
    assert_eq!(file.counts.observations, 1);
    assert_eq!(file.lines[0].byte_length, giant.len() as u64);
    assert_eq!(file.pending_tail_bytes, giant.len() as u64);
}

#[test]
fn symlinks_hard_links_and_outside_data_are_refused_and_visible() {
    let tree = Tree::new();
    let outside = Tree::new();
    let line = record("out", FLAT, &text("OUTSIDE-SENTINEL"));
    outside.write("secret.jsonl", format!("{line}\n"));
    tree.write("real.jsonl", format!("{}\n", record("in", FLAT, "[]")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(outside.0.join("secret.jsonl"), tree.0.join("link.jsonl")).unwrap();
        symlink(&outside.0, tree.0.join("linkdir")).unwrap();
    }
    fs::hard_link(outside.0.join("secret.jsonl"), tree.0.join("hard.jsonl")).unwrap();
    let batch = default(&scope(&tree));
    assert_eq!(batch.completeness, Completeness::Incomplete);
    assert_eq!(batch.observations().count(), 1);
    let reasons: Vec<SkipReason> = batch.skipped.iter().map(|s| s.reason).collect();
    assert_eq!(
        reasons
            .iter()
            .filter(|r| **r == SkipReason::Symlink)
            .count(),
        2
    );
    let hard = batch
        .files
        .iter()
        .find(|f| f.status != ClaudeFileStatus::Read)
        .expect("hard link rejected");
    assert_eq!(
        hard.status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(PathRejection::HardLinked))
    );
    assert!(hard.lines.is_empty());
    assert_eq!(batch.totals().files_rejected, 1);
    assert!(!format!("{batch:?}").contains("OUTSIDE"));
    // A symlinked root is refused outright.
    let link_root = outside.0.join("rootlink");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&tree.0, &link_root).unwrap();
    assert!(AuthorizedRoot::new(&link_root).is_err());
}

#[test]
fn traversal_depth_entries_and_file_counts_are_bounded() {
    let tree = Tree::new();
    let line = format!("{}\n", record("d", FLAT, "[]"));
    tree.write("a.jsonl", &line);
    tree.write("l1/b.jsonl", &line);
    tree.write("l1/l2/c.jsonl", &line);
    tree.write("l1/l2/l3/d.jsonl", &line);
    let shallow = |depth| {
        CollectionScope::new(
            EnvironmentIdentity::wsl(id("env"), WslDistroName::new("F").unwrap()),
            SourceInstance::new(id("src"), Harness::ClaudeCode),
            AuthorizedRoot::new(&tree.0).unwrap(),
            HostPolicy::RequireNative,
        )
        .unwrap()
        .with_limits(CollectionLimits {
            max_depth: depth,
            ..CollectionLimits::default()
        })
        .unwrap()
    };
    let batch = collect(&shallow(3), ClaudeReadLimits::default());
    assert_eq!(batch.files.len(), 3);
    assert!(
        batch
            .skipped
            .iter()
            .any(|s| s.reason == SkipReason::DepthLimit)
    );
    assert_eq!(batch.completeness, Completeness::Incomplete);
    assert_eq!(
        collect(&shallow(4), ClaudeReadLimits::default()).completeness,
        Completeness::Complete
    );

    let few_entries = scope(&tree)
        .with_limits(CollectionLimits {
            max_entries: 3,
            ..CollectionLimits::default()
        })
        .unwrap();
    assert_eq!(
        collect_claude_jsonl_root(&few_entries, &run(), ClaudeReadLimits::default()).unwrap_err(),
        ClaudeCollectError::Boundary(CollectError::EntryLimitExceeded)
    );
    let few_files = ClaudeReadLimits {
        max_files: 3,
        ..ClaudeReadLimits::default()
    };
    assert_eq!(
        collect_claude_jsonl_root(&scope(&tree), &run(), few_files).unwrap_err(),
        ClaudeCollectError::FileLimitExceeded
    );
    let exact_files = ClaudeReadLimits {
        max_files: 4,
        ..ClaudeReadLimits::default()
    };
    assert_eq!(collect(&scope(&tree), exact_files).files.len(), 4);
}

#[test]
fn byte_record_and_file_size_budgets_reject_explicitly() {
    let tree = Tree::new();
    let line = format!("{}\n", record("b", FLAT, "[]"));
    for name in ["a", "b", "c"] {
        tree.write(&format!("{name}.jsonl"), &line);
    }
    let size = line.len() as u64;
    // Byte budget: two files fit, the third is rejected, never silently dropped.
    let bytes = ClaudeReadLimits {
        max_total_bytes: size * 2,
        ..ClaudeReadLimits::default()
    };
    let batch = collect(&scope(&tree), bytes);
    assert_eq!(batch.completeness, Completeness::Incomplete);
    assert_eq!(
        batch.files[2].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::ByteBudget)
    );
    assert_eq!(batch.observations().count(), 2);
    // Retained-record budget is shared across files and rejects the whole affected file.
    let records = ClaudeReadLimits {
        max_records: 2,
        ..ClaudeReadLimits::default()
    };
    let batch = collect(&scope(&tree), records);
    assert_eq!(
        batch.files[2].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::OutputLimit)
    );
    assert!(
        batch.files[2].lines.is_empty() && batch.files[2].counts == ClaudeLineCounts::default()
    );
    let many = format!("{}{}", line.repeat(3), "\n");
    tree.write("many.jsonl", &many);
    let one = ClaudeReadLimits {
        max_records: 3,
        ..ClaudeReadLimits::default()
    };
    let batch = collect(&scope(&tree), one);
    let many_report = batch
        .files
        .iter()
        .find(|f| f.status != ClaudeFileStatus::Read)
        .unwrap();
    assert_eq!(
        many_report.status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::OutputLimit)
    );
    // Per-file size comes from the scope.
    let tiny = scope(&tree)
        .with_limits(CollectionLimits {
            max_file_bytes: size,
            ..CollectionLimits::default()
        })
        .unwrap();
    let batch = collect(&tiny, ClaudeReadLimits::default());
    assert!(
        batch
            .files
            .iter()
            .any(|f| f.status == ClaudeFileStatus::Rejected(ClaudeFileRejection::TooLarge))
    );
}

#[test]
fn invalid_limits_and_wrong_harness_are_refused() {
    let tree = Tree::new();
    tree.write("a.jsonl", "{}\n");
    let zero = ClaudeReadLimits {
        max_files: 0,
        ..ClaudeReadLimits::default()
    };
    let high = ClaudeReadLimits {
        max_line_bytes: MAX_LINE_BYTES_CEILING + 1,
        ..ClaudeReadLimits::default()
    };
    for limits in [zero, high] {
        assert_eq!(
            collect_claude_jsonl_root(&scope(&tree), &run(), limits).unwrap_err(),
            ClaudeCollectError::InvalidLimits
        );
    }
    assert_eq!(
        collect_claude_jsonl_root(
            &scope_at(&tree.0, Harness::OpenCode),
            &run(),
            ClaudeReadLimits::default()
        )
        .unwrap_err(),
        ClaudeCollectError::WrongHarness
    );
}

#[test]
fn existing_fixture_collection_is_unaffected_and_formats_do_not_mix() {
    let tree = Tree::new();
    tree.write("claude.jsonl", format!("{}\n", record("m", FLAT, "[]")));
    tree.write(
        "fixture.abf-fixture",
        format!("{FIXTURE_HEADER}\nmessage_id=m1\tinput_tokens=5\n"),
    );
    let scope = scope(&tree);
    let synthetic = collect_synthetic_root(&scope, &run()).unwrap();
    assert_eq!(synthetic.files.len(), 1);
    assert_eq!(synthetic.observations().count(), 1);
    assert!(
        synthetic
            .skipped
            .iter()
            .any(|s| s.reason == SkipReason::NotFixtureExtension)
    );
    let claude = default(&scope);
    assert_eq!(claude.files.len(), 1);
    assert_eq!(claude.observations().count(), 1);
}

#[test]
fn results_expose_no_content_paths_or_raw_identifiers_and_are_deterministic() {
    let tree = Tree::new();
    let first = format!("{}\n", record("one", FLAT, &text(SENTINEL)));
    let second = format!("{}\n", record("two", SPLIT, &text(SENTINEL)));
    tree.write(&format!("{SECRET_DIR}/x.jsonl"), &first);
    tree.write("y.jsonl", &second);
    let scope = scope(&tree);
    let batch = default(&scope);
    let debug = format!("{batch:?} {:?}", batch.observations().collect::<Vec<_>>());
    for secret in [
        SENTINEL,
        SECRET_DIR,
        "fictional",
        "sess-one",
        "req-one",
        "msg-one",
        "fict-model",
        tree.0.to_str().unwrap(),
        "target/abf134d",
    ] {
        assert!(!debug.contains(secret), "leaked {secret}");
    }
    assert_eq!(batch, default(&scope));
    // Same content created in the opposite order gives the same ordered result.
    let other = Tree::new();
    other.write("y.jsonl", &second);
    other.write(&format!("{SECRET_DIR}/x.jsonl"), &first);
    // File-object evidence legitimately differs between trees; everything else is equal.
    let again = default(&scope_at(&other.0, Harness::ClaudeCode));
    let strip = |batch: &ClaudeBatch| -> Vec<_> {
        batch
            .files
            .iter()
            .map(|f| {
                (
                    f.file_ordinal,
                    f.status,
                    f.lines.clone(),
                    f.counts,
                    f.pending_tail_bytes,
                )
            })
            .collect()
    };
    assert_eq!(strip(&batch), strip(&again));
}
