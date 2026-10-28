//! ABF139: invented session records only; no personal histories or discovery.
use abf_collection_boundary::*;

const EVENTS: &str =
    include_str!("../../../../desktop/windows-bootstrap/fixtures/abf139-session-events.jsonl");
const USAGE: &str =
    include_str!("../../../../desktop/windows-bootstrap/fixtures/abf139-assistant-usage.jsonl");

fn collect(bytes: &[u8], limits: ClaudeReadLimits) -> ClaudeSummary {
    let id = |s| OpaqueId::new(s).unwrap();
    collect_claude_jsonl_memory(
        EnvironmentIdentity::windows(id("fictional-host")),
        SourceInstance::new(id("fictional-source"), Harness::ClaudeCode),
        &ScanRun::new(id("fictional-run"), CaptureTime::NotRecorded),
        &[bytes],
        limits,
    )
    .unwrap()
    .summary()
}

#[test]
fn no_usage_session_is_thirteen_observations_without_zero_claims() {
    let s = collect(EVENTS.as_bytes(), ClaudeReadLimits::default());
    assert_eq!(s.files.read, 1);
    assert_eq!(s.records.observations, 13);
    assert_eq!(s.records.incomplete, 13);
    assert_eq!(s.records.with_valid_claim, 0);
    assert_eq!(s.records.without_valid_claim, 13);
    assert_eq!(s.lines.rejected.total(), 0);
    for component in [
        s.components.input,
        s.components.output,
        s.components.cache_creation,
        s.components.cache_read,
    ] {
        assert_eq!(component.value, ComponentValue::NoValidClaims);
        assert_eq!(component.states.valid_zero, 0);
    }
    // Nested progress data is not the record's message.usage.
    assert_eq!(s.components.input.states.null, 1);
    assert_eq!(s.components.input.states.absent, 1);
    assert_eq!(s.components.input.states.unsupported, 11);
}

#[test]
fn mixed_session_recognizes_assistant_claims_without_defaulting_missing_fields() {
    let s = collect(
        format!("{EVENTS}{USAGE}").as_bytes(),
        ClaudeReadLimits::default(),
    );
    assert_eq!(s.records.observations, 17);
    assert_eq!(s.records.with_valid_claim, 4);
    assert_eq!(s.records.without_valid_claim, 13);
    assert_eq!(
        s.components.input.value,
        ComponentValue::ReportedClaimSum(130)
    );
    assert_eq!(
        s.components.output.value,
        ComponentValue::ReportedClaimSum(32)
    );
    assert_eq!(
        s.components.cache_creation.value,
        ComponentValue::ReportedClaimSum(64)
    );
    assert_eq!(
        s.components.cache_read.value,
        ComponentValue::ReportedClaimSum(500)
    );
    assert_eq!(s.records.cache_creation_flat_ignored, 1);
    assert_eq!(s.components.input.states.valid_zero, 1);
    assert_eq!(s.components.output.states.null, 2);
    assert_eq!(s.components.cache_read.states.absent, 4);
    assert_eq!(s.duplicates, DuplicateIdentity::Unresolved);
    assert_eq!(s.basis, CountingBasis::ObservedRecordClaims);
}

#[test]
fn malformed_oversized_and_pending_lines_leave_truthful_coverage() {
    let bytes = format!(
        "{EVENTS}{{bad}}\n{}\n{USAGE}{{\"message\":",
        "x".repeat(65537)
    );
    let s = collect(
        bytes.as_bytes(),
        ClaudeReadLimits {
            max_line_bytes: 65536,
            ..ClaudeReadLimits::default()
        },
    );
    assert_eq!(s.files.read, 1);
    assert_eq!(s.records.with_valid_claim, 4);
    assert_eq!(
        s.components.input.value,
        ComponentValue::ReportedClaimSum(130)
    );
    assert_eq!(s.lines.rejected.malformed_json, 1);
    assert_eq!(s.lines.oversized, 1);
    assert_eq!(s.lines.files_with_pending_tail, 1);
    assert!(s.caveats.unparsed_lines);
    assert!(s.caveats.pending_tails);
}
