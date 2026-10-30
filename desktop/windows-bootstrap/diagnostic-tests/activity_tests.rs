#[test]
fn parser_field_alias_preserves_the_existing_collection_field() {
    use abf_collection_boundary::{ClaudeField, Field};
    let timestamp = ClaudeField::Valid("2026-01-01T00:00:00Z");
    assert!(matches!(timestamp, ClaudeField::Valid(_)));
    let collection_field = Field::<u64>::NotCollected;
    assert_eq!(collection_field, Field::NotCollected);
}

use crate::claude_copy_activity::{activity_json, project};
use abf_collection_boundary::*;

fn batch(lines: &str) -> ClaudeBatch {
    let id = |s| OpaqueId::new(s).unwrap();
    collect_claude_jsonl_memory(
        EnvironmentIdentity::windows(id("fictional-windows")),
        SourceInstance::new(id("fictional-source"), Harness::ClaudeCode),
        &ScanRun::new(id("fictional-run"), CaptureTime::NotRecorded),
        &[lines.as_bytes()],
        ClaudeReadLimits {
            max_records: 4096,
            ..ClaudeReadLimits::default()
        },
    )
    .unwrap()
}

#[test]
fn daily_rows_normalize_offsets_and_sort_across_calendar_boundaries() {
    let batch = batch(concat!(
        "{\"timestamp\":\"2026-01-01T00:15:00.123456789+01:00\",\"message\":{\"usage\":{\"input_tokens\":2}}}\n",
        "{\"timestamp\":\"2024-03-01T00:00:00+00:01\",\"message\":{\"usage\":{\"input_tokens\":3}}}\n",
        "{\"timestamp\":\"2025-12-31T23:45:00-01:00\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n",
    ));
    let rows = project(&batch).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.day.as_deref()).collect::<Vec<_>>(),
        [Some("2024-02-29"), Some("2025-12-31"), Some("2026-01-01")]
    );
    assert_eq!(
        rows[0].components.input.value,
        ComponentValue::ReportedClaimSum(3)
    );
    assert_eq!(
        rows[1].components.input.value,
        ComponentValue::ReportedClaimSum(2)
    );
    assert_eq!(
        rows[2].components.input.value,
        ComponentValue::ReportedClaimSum(5)
    );
}

#[test]
fn daily_rows_keep_invalid_missing_and_unsupported_timestamps_undated() {
    let mut lines = String::new();
    for timestamp in [
        "null",
        "42",
        "{}",
        "\"PRIVATE\"",
        "\"2025-02-29T00:00:00Z\"",
        "\"2026-01-01T24:00:00Z\"",
        "\"2026-01-01T00:00:00+01:60\"",
        "\"2026-01-01T00:00:00+24:00\"",
        "\"2026-01-01T00:00:00-00:00\"",
        "\"0001-01-01T00:00:00+01:00\"",
        "\"9999-12-31T23:59:59-01:00\"",
    ] {
        lines.push_str(&format!(
            "{{\"timestamp\":{timestamp},\"message\":{{\"usage\":{{\"input_tokens\":1}}}}}}\n"
        ));
    }
    lines.push_str("{\"message\":{\"usage\":{\"input_tokens\":1}}}\n");
    let rows = project(&batch(&lines)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].day, None);
    assert_eq!(rows[0].observations, 12);
    assert_eq!(
        rows[0].components.input.value,
        ComponentValue::ReportedClaimSum(12)
    );
}

#[test]
fn daily_rows_reuse_raw_sums_keep_repeats_zero_missing_and_overflow() {
    let line = "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"usage\":{\"input_tokens\":18446744073709551615,\"output_tokens\":0,\"cache_creation_input_tokens\":99,\"cache_creation\":{\"ephemeral_5m_input_tokens\":2,\"ephemeral_1h_input_tokens\":3}}}}\n";
    let rows = project(&batch(&line.repeat(2))).unwrap();
    assert_eq!(rows[0].observations, 2);
    assert_eq!(rows[0].components.input.value, ComponentValue::Overflow);
    assert_eq!(
        rows[0].components.output.value,
        ComponentValue::ReportedClaimSum(0)
    );
    assert_eq!(rows[0].components.output.states.valid_zero, 2);
    assert_eq!(
        rows[0].components.cache_creation.value,
        ComponentValue::ReportedClaimSum(10)
    );
    assert_eq!(
        rows[0].components.cache_read.value,
        ComponentValue::NoValidClaims
    );
    assert_eq!(rows[0].components.cache_read.states.absent, 2);
}

#[test]
fn daily_activity_has_bounded_rows_and_bytes_without_truncation() {
    let lines = (1..=31)
        .map(|day| format!("{{\"timestamp\":\"2026-01-{day:02}T00:00:00Z\"}}\n"))
        .collect::<String>()
        + "{\"timestamp\":\"2026-02-01T00:00:00Z\"}\n";
    let admitted = batch(&lines);
    assert_eq!(project(&admitted).unwrap().len(), 32);
    assert_eq!(project(&batch(&(lines + "{}\n"))), Err(()));
    let unavailable = "{\"status\":\"budget-exhausted\"}";
    assert_eq!(activity_json(&admitted, 100), unavailable);
    let small = batch("{}\n");
    let payload = activity_json(&small, 16384);
    assert_eq!(activity_json(&small, payload.len()), payload);
    assert_eq!(activity_json(&small, payload.len() - 1), unavailable);
}

#[test]
fn daily_activity_empty_is_not_zero_and_private_metadata_never_leaves_rust() {
    assert_eq!(
        activity_json(&batch(""), 16384),
        "{\"status\":\"available\",\"rows\":[]}"
    );
    let payload = activity_json(
        &batch(
            "{\"timestamp\":\"2026-01-01T12:34:56Z\",\"sessionId\":\"PRIVATE-session\",\"message\":{\"id\":\"PRIVATE-id\",\"model\":\"PRIVATE-model\",\"content\":\"PRIVATE-content\",\"usage\":{\"input_tokens\":1}}}\n",
        ),
        16384,
    );
    assert!(payload.contains("\"day\":\"2026-01-01\""));
    for secret in [
        "PRIVATE",
        "12:34:56",
        "sessionId",
        "model",
        "content",
        "filename",
        "path",
    ] {
        assert!(!payload.contains(secret));
    }
}

#[test]
fn daily_4096_observations_are_counted_and_input_is_unchanged() {
    let batch = batch(
        &"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n"
            .repeat(4096),
    );
    let original = batch.clone();
    let rows = project(&batch).unwrap();
    assert_eq!(rows[0].observations, 4096);
    assert_eq!(
        rows[0].components.input.value,
        ComponentValue::ReportedClaimSum(4096)
    );
    assert_eq!(batch, original);
}

#[test]
fn daily_unsupported_field_state_is_undated_and_keeps_claims() {
    let mut batch = batch("{\"message\":{\"usage\":{\"input_tokens\":7}}}\n");
    let ClaudeLineOutcome::Observation(observation) = &mut batch.files[0].lines[0].outcome else {
        panic!("expected observation");
    };
    observation.timestamp = ClaudeField::Unsupported;
    let rows = project(&batch).unwrap();
    assert_eq!(rows[0].day, None);
    assert_eq!(
        rows[0].components.input.value,
        ComponentValue::ReportedClaimSum(7)
    );
}

#[test]
fn daily_calendar_and_fraction_support_are_deterministic() {
    for (timestamp, expected) in [
        ("2000-02-29T23:59:59.1Z", Some("2000-02-29")),
        ("1900-02-29T00:00:00Z", None),
        ("2026-03-01T00:00:00+23:59", Some("2026-02-28")),
        ("2026-04-30T23:59:59-23:59", Some("2026-05-01")),
        ("2026-01-01T00:00:00+00:00", Some("2026-01-01")),
        ("2026-01-01T00:00:00.Z", None),
        ("2026-01-01T00:00:00.1234567890Z", None),
        ("2026-01-01T00:00:60Z", None),
        ("2026-01-01T00:00:00", None),
        ("2026-01-01T00:00:00z", None),
        ("2026-01-01T00:00:00ZPRIVATE", None),
        ("é2026-01-01T00:00:00Z", None),
    ] {
        let lines = format!("{{\"timestamp\":\"{timestamp}\"}}\n");
        let rows = project(&batch(&lines)).unwrap();
        assert_eq!(rows[0].day.as_deref(), expected, "{timestamp}");
    }
}
