//! Fixed fictional demo payload and the in-memory demo entry (ABF134F). No filesystem use.

use abf_collection_boundary::claude_demo::*;
use abf_collection_boundary::*;

const GOLDEN: &str = include_str!("../fixtures/fictional-claude-demo/expected-payload.json");
const FIXTURE_A: &[u8] = include_bytes!("../fixtures/fictional-claude-demo/a.jsonl");

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).unwrap()
}
fn env() -> EnvironmentIdentity {
    EnvironmentIdentity::wsl(id("e"), WslDistroName::new("Fictional").unwrap())
}
fn source(harness: Harness) -> SourceInstance {
    SourceInstance::new(id("s"), harness)
}
fn run() -> ScanRun {
    ScanRun::new(id("r"), CaptureTime::NotRecorded)
}
const UNTERMINATED: &[u8] = b"{\"message\":{\"usage\":{\"input_tokens\":1}}}";
const LINE: &[u8] = b"{\"message\":{\"usage\":{\"input_tokens\":7}}}\n";

#[test]
fn payload_is_the_real_pipeline_output_and_matches_the_golden_file() {
    let payload = fictional_demo_payload_json();
    assert_eq!(
        payload,
        GOLDEN.trim_end(),
        "regenerate the golden file only for a reviewed change"
    );
    assert_eq!(payload, fictional_demo_payload_json(), "deterministic");

    // Independent expectations derived from the fixture design, not from the golden file.
    let summary = fictional_demo_summary();
    assert_eq!(summary.completeness, Completeness::Complete);
    assert_eq!(summary.host_match, HostMatch::Simulated);
    let sum = |c: ComponentSummary| match c.value {
        ComponentValue::ReportedClaimSum(n) => n,
        other => panic!("expected a sum, got {other:?}"),
    };
    assert_eq!(sum(summary.components.input), 31);
    assert_eq!(sum(summary.components.output), 69);
    assert_eq!(sum(summary.components.cache_creation), 20);
    assert_eq!(sum(summary.components.cache_read), 12);
    assert_eq!(summary.records.observations, 8);
    assert_eq!(summary.lines.terminated_lines, 11);
    assert_eq!((summary.lines.oversized, summary.lines.blank), (1, 1));
    assert_eq!(summary.lines.rejected.incomplete_json, 1);
    assert_eq!(summary.lines.files_with_pending_tail, 1);
    for needle in [
        r#""amount":"31""#,
        r#""amount":"69""#,
        r#""amount":"20""#,
        r#""amount":"12""#,
        r#""observations":8"#,
        r#""duplicates":"unresolved""#,
        r#""basis":"observed-record-claims""#,
        r#""provenance":"fictional-fixture""#,
    ] {
        assert!(payload.contains(needle), "missing {needle}");
    }
    // The unterminated tail of the fixture file is reported by length, never parsed.
    let tail = FIXTURE_A.len() - FIXTURE_A.iter().rposition(|&b| b == b'\n').unwrap() - 1;
    assert!(tail > 0);
    assert_eq!(summary.lines.pending_tail_bytes, tail as u64);
    assert!(payload.contains(&format!(r#""pendingTailBytes":{tail}"#)));
    // Cache creation counted once: 3 + 11 + 0 + 3 + 3, never plus the ignored flat 99.
    assert_eq!(summary.records.cache_creation_flat_ignored, 1);
}

#[test]
fn payload_carries_no_identifiers_labels_content_or_paths() {
    let payload = fictional_demo_payload_json();
    for secret in [
        "FICTIONAL-DEMO-CONTENT",
        "demo-session",
        "demo-req",
        "demo-msg",
        "fictional-model",
        "demo-project",
        "Fictional-Demo",
        "demo-env",
        "demo-source",
        "demo-run",
        "2026-01-02",
        "fixtures",
        "jsonl",
        "/",
        "\\",
    ] {
        assert!(!payload.contains(secret), "payload leaked {secret}");
    }
    // Plain JSON of ASCII with no control characters.
    assert!(payload.is_ascii() && !payload.chars().any(char::is_control));
    assert!(!format!("{:?}", fictional_demo_summary()).contains("FICTIONAL-DEMO-CONTENT"));
}

#[test]
fn no_public_route_stamps_an_arbitrary_summary_as_fictional() {
    // The serializer that writes the fixed `fictional-fixture` provenance must stay private:
    // the only public functions are the fixed-fixture summary and its payload.
    let source = include_str!("../src/claude_demo.rs");
    let public: Vec<&str> = source
        .lines()
        .filter(|line| line.trim_start().starts_with("pub fn "))
        .map(|line| line.trim())
        .collect();
    assert_eq!(
        public,
        [
            "pub fn fictional_demo_summary() -> ClaudeSummary {",
            "pub fn fictional_demo_payload_json() -> String {"
        ]
    );
    assert!(source.contains("fn payload_json(summary: &ClaudeSummary) -> String {"));
    assert!(
        !source.contains("pub fn payload_json") && !source.contains("pub(crate) fn payload_json")
    );
    assert!(!source.contains("pub fn fictional_payload_json"));
    // Nothing else exports a payload writer under another name.
    assert!(!source.lines().any(|l| {
        let l = l.trim_start();
        (l.starts_with("pub use") || l.starts_with("pub(crate) use")) && l.contains("payload")
    }));
}

#[test]
fn memory_entry_is_simulated_bounded_and_shares_the_file_pipeline() {
    let batch = collect_claude_jsonl_memory(
        env(),
        source(Harness::ClaudeCode),
        &run(),
        &[LINE, UNTERMINATED],
        ClaudeReadLimits::default(),
    )
    .unwrap();
    assert_eq!(batch.host_match, HostMatch::Simulated);
    assert_eq!(batch.completeness, Completeness::Complete);
    assert_eq!(batch.files.len(), 2);
    assert_eq!(batch.files[0].counts.observations, 1);
    // An unterminated tail is pending, never parsed.
    assert_eq!(batch.files[1].counts.observations, 0);
    assert_eq!(batch.files[1].pending_tail_bytes, UNTERMINATED.len() as u64);
    assert!(
        batch
            .files
            .iter()
            .all(|f| f.file_object == FileObjectRef::Unavailable)
    );

    let limits = |f: fn(&mut ClaudeReadLimits)| {
        let mut l = ClaudeReadLimits::default();
        f(&mut l);
        l
    };
    let budget = collect_claude_jsonl_memory(
        env(),
        source(Harness::ClaudeCode),
        &run(),
        &[LINE, LINE],
        limits(|l| l.max_total_bytes = LINE.len() as u64),
    )
    .unwrap();
    assert_eq!(budget.completeness, Completeness::Incomplete);
    assert_eq!(
        budget.files[1].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::ByteBudget)
    );
    let records = collect_claude_jsonl_memory(
        env(),
        source(Harness::ClaudeCode),
        &run(),
        &[LINE, LINE],
        limits(|l| l.max_records = 1),
    )
    .unwrap();
    assert_eq!(
        records.files[1].status,
        ClaudeFileStatus::Rejected(ClaudeFileRejection::OutputLimit)
    );
    assert_eq!(
        collect_claude_jsonl_memory(
            env(),
            source(Harness::ClaudeCode),
            &run(),
            &[LINE, LINE],
            limits(|l| l.max_files = 1)
        )
        .unwrap_err(),
        ClaudeCollectError::FileLimitExceeded
    );
    assert_eq!(
        collect_claude_jsonl_memory(
            env(),
            source(Harness::Codex),
            &run(),
            &[LINE],
            ClaudeReadLimits::default()
        )
        .unwrap_err(),
        ClaudeCollectError::WrongHarness
    );
    assert_eq!(
        collect_claude_jsonl_memory(
            env(),
            source(Harness::ClaudeCode),
            &run(),
            &[LINE],
            limits(|l| l.max_files = 0)
        )
        .unwrap_err(),
        ClaudeCollectError::InvalidLimits
    );
}
