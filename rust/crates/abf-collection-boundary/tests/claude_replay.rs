//! ABF140B: fictional records only; no filesystem, discovery or provider access.
use abf_collection_boundary::{claude_replay::*, *};

const RECORD: &str = r#"{"type":"assistant","uuid":"fictional-uuid-a","isSidechain":false,"sessionId":"fictional-session","requestId":"fictional-request","timestamp":"2026-01-02T03:04:05Z","message":{"id":"fictional-message","model":"fictional-model","usage":{"input_tokens":10,"output_tokens":3,"cache_creation_input_tokens":11,"cache_read_input_tokens":4}}}"#;

fn batch(records: &[&str]) -> ClaudeBatch {
    let text = format!("{}\n", records.join("\n"));
    let id = |s| OpaqueId::new(s).unwrap();
    collect_claude_jsonl_memory(
        EnvironmentIdentity::windows(id("fictional-environment")),
        SourceInstance::new(id("fictional-source"), Harness::ClaudeCode),
        &ScanRun::new(id("fictional-run"), CaptureTime::NotRecorded),
        &[text.as_bytes()],
        ClaudeReadLimits {
            max_records: MAX_RECORDS_CEILING,
            ..ClaudeReadLimits::default()
        },
    )
    .unwrap()
}

fn claims(result: ClaudeReplaySummary) -> ReplayClaims {
    match result.evaluation {
        ReplayEvaluation::Evaluated(claims) => claims,
        other => panic!("expected evaluated claims, got {other:?}"),
    }
}

#[test]
fn exact_replay_has_separate_claim_sums_and_preserves_raw_summary() {
    let b = batch(&[RECORD, RECORD]);
    let result = b.exact_replay_summary(2);
    assert_eq!(result.raw, b.summary());
    assert_eq!(
        result.raw.components.input.value,
        ComponentValue::ReportedClaimSum(20)
    );
    let c = claims(result);
    assert_eq!(c.basis, ReplayBasis::SourceScopedExactObservationClaimsV1);
    assert_eq!(c.eligible_observations, 2);
    assert_eq!(c.representatives, 1);
    assert_eq!(c.replay_observations, 1);
    assert_eq!(c.unresolved_observations, 0);
    assert_eq!(
        c.components.input.value,
        ComponentValue::ReportedClaimSum(10)
    );
}

#[test]
fn distinct_turns_with_equal_claims_are_not_merged() {
    let other = RECORD
        .replace("fictional-message", "fictional-other-message")
        .replace("fictional-request", "fictional-other-request")
        .replace("fictional-uuid-a", "fictional-uuid-b");
    let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
    assert_eq!(c.representatives, 2);
    assert_eq!(c.conflicts, ConflictCounts::default());
    assert_eq!(c.replay_observations, 0);
    assert_eq!(
        c.components.input.value,
        ComponentValue::ReportedClaimSum(20)
    );
}

#[test]
fn conflicting_late_claim_invalidates_even_preceding_exact_replays() {
    let changed = RECORD.replace("\"output_tokens\":3", "\"output_tokens\":7");
    let b = batch(&[RECORD, RECORD, &changed]);
    let result = b.exact_replay_summary(3);
    assert_eq!(
        result.raw.components.output.value,
        ComponentValue::ReportedClaimSum(13)
    );
    let c = claims(result);
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(c.replay_observations, 0);
    assert_eq!(c.unresolved_observations, 3);
    assert_eq!(c.unresolved.conflicting_evidence, 3);
    assert_eq!(
        c,
        claims(batch(&[&changed, RECORD, RECORD]).exact_replay_summary(3))
    );
}

#[test]
fn uuid_differences_and_uuid_reuse_are_unresolved() {
    for other in [
        RECORD.replace("fictional-uuid-a", "fictional-uuid-b"),
        RECORD.replace("fictional-message", "fictional-other-message"),
        RECORD.replace("\"type\":\"assistant\"", "\"type\":\"progress\""),
    ] {
        let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
        assert_eq!(c.eligible_observations, 0);
        assert_eq!(c.unresolved_observations, 2);
        assert_eq!(c.unresolved.conflicting_evidence, 2);
    }
}

#[test]
fn missing_invalid_or_partial_identity_never_establishes_replay() {
    for other in [
        RECORD.replace("\"uuid\":\"fictional-uuid-a\",", ""),
        RECORD.replace("\"sessionId\":\"fictional-session\",", ""),
        RECORD.replace("\"requestId\":\"fictional-request\",", ""),
        RECORD.replace("\"id\":\"fictional-message\",", ""),
        RECORD.replace("\"isSidechain\":false,", ""),
        RECORD.replace("\"uuid\":\"fictional-uuid-a\"", "\"uuid\":null"),
        RECORD.replace("\"uuid\":\"fictional-uuid-a\"", "\"uuid\":[]"),
    ] {
        let c = claims(batch(&[&other, &other]).exact_replay_summary(2));
        assert_eq!(c.eligible_observations, 0);
        assert_eq!(c.unresolved_observations, 2);
        assert_eq!(c.unresolved.invalid_identity, 2);
    }
}

#[test]
fn sidechains_and_matching_parents_are_unresolved_even_across_request_ids() {
    let sidechain = RECORD
        .replace("\"isSidechain\":false", "\"isSidechain\":true")
        .replace("fictional-request", "fictional-sidechain-request")
        .replace("fictional-uuid-a", "fictional-uuid-b");
    let c = claims(batch(&[RECORD, &sidechain]).exact_replay_summary(2));
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(c.unresolved_observations, 2);
    assert_eq!(c.unresolved.sidechain, 1);
}

#[test]
fn isolated_ineligible_observation_is_not_a_conflict_with_itself() {
    let sidechain = RECORD.replace("\"isSidechain\":false", "\"isSidechain\":true");
    let c = claims(batch(&[&sidechain]).exact_replay_summary(1));
    assert_eq!(c.unresolved_observations, 1);
    assert_eq!(c.unresolved.sidechain, 1);
    assert_eq!(c.unresolved.conflicting_evidence, 0);
    assert_eq!(c.conflicts, ConflictCounts::default());
}

#[test]
fn non_claude_source_cannot_gain_claude_replay_semantics() {
    let mut b = batch(&[RECORD, RECORD]);
    b.source = SourceInstance::new(
        OpaqueId::new("fictional-source").unwrap(),
        Harness::OpenCode,
    );
    let result = b.exact_replay_summary(2);
    assert_eq!(result.raw, b.summary());
    assert_eq!(result.evaluation, ReplayEvaluation::WrongHarness);
}

#[test]
fn partial_or_invalid_usage_stays_raw_and_unresolved() {
    for other in [
        RECORD.replace("\"input_tokens\":10", "\"input_tokens\":null"),
        RECORD.replace(
            "\"cache_read_input_tokens\":4",
            "\"cache_read_input_tokens\":-1",
        ),
        RECORD.replace("\"cache_read_input_tokens\":4", "\"unknown_tokens\":4"),
    ] {
        let result = batch(&[&other, &other]).exact_replay_summary(2);
        assert_eq!(result.raw.records.observations, 2);
        let c = claims(result);
        assert_eq!(c.eligible_observations, 0);
        assert_eq!(c.unresolved.invalid_claims, 2);
    }
}

#[test]
fn cache_claims_compare_full_evidence_and_never_add_flat_to_breakdown() {
    let split = RECORD.replace("\"cache_creation_input_tokens\":11", "\"cache_creation_input_tokens\":99,\"cache_creation\":{\"ephemeral_5m_input_tokens\":5,\"ephemeral_1h_input_tokens\":6}");
    let c = claims(batch(&[&split, &split]).exact_replay_summary(2));
    assert_eq!(
        c.components.cache_creation.value,
        ComponentValue::ReportedClaimSum(11)
    );
    assert_eq!(
        c.components.cache_read.value,
        ComponentValue::ReportedClaimSum(4)
    );
    let c = claims(batch(&[RECORD, &split]).exact_replay_summary(2));
    assert_eq!(c.conflicts.token_fields, 2);
    assert_eq!(c.conflicts.identity_metadata, 0);
    assert_eq!(c.conflicts.parser_evidence, 0);
    assert_eq!(c.conflicts.ineligible_peer, 0);
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(c.unresolved.conflicting_evidence, 2);
}

#[test]
fn budget_exhaustion_exposes_no_partial_deduplicated_result() {
    let b = batch(&[RECORD, RECORD]);
    let result = b.exact_replay_summary(1);
    assert_eq!(result.raw, b.summary());
    assert_eq!(
        result.evaluation,
        ReplayEvaluation::IdentityBudgetExhausted {
            observations: 2,
            limit: 1
        }
    );
    assert_eq!(
        b.exact_replay_summary(0).evaluation,
        ReplayEvaluation::IdentityBudgetExhausted {
            observations: 2,
            limit: 0
        }
    );
    assert_eq!(
        b.exact_replay_summary(usize::MAX).evaluation,
        ReplayEvaluation::InvalidIdentityLimit
    );
}

#[test]
fn coverage_gaps_and_raw_field_states_are_carried_unchanged() {
    let b = batch(&[RECORD, "{bad}", "{}"]);
    let result = b.exact_replay_summary(3);
    assert_eq!(result.raw, b.summary());
    assert!(result.raw.caveats.unparsed_lines);
    assert_eq!(result.raw.lines.rejected.malformed_json, 1);
    let c = claims(result);
    assert_eq!(c.eligible_observations, 1);
    assert_eq!(c.unresolved_observations, 1);
    assert_eq!(
        c.components.input.value,
        ComponentValue::ReportedClaimSum(10)
    );
}

#[test]
fn timestamp_model_and_cache_partition_changes_are_not_exact_replays() {
    for other in [
        RECORD.replace("03:04:05Z", "03:04:06Z"),
        RECORD.replace("fictional-model", "fictional-other-model"),
        RECORD.replace(
            "\"cache_creation_input_tokens\":11",
            "\"cache_creation\":{\"ephemeral_5m_input_tokens\":11,\"ephemeral_1h_input_tokens\":0}",
        ),
    ] {
        let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
        assert_eq!(c.unresolved_observations, 2);
        assert_eq!(c.unresolved.conflicting_evidence, 2);
    }
}

#[test]
fn changed_request_for_a_shared_message_remains_unresolved() {
    let other = RECORD
        .replace("fictional-request", "fictional-other-request")
        .replace("fictional-uuid-a", "fictional-uuid-b");
    let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
    assert_eq!(c.unresolved_observations, 2);
    assert_eq!(c.eligible_observations, 0);
}

#[test]
fn zero_and_overflow_remain_explicit_in_the_eligible_subset() {
    let zero = RECORD.replace("\"input_tokens\":10", "\"input_tokens\":0");
    let c = claims(batch(&[&zero, &zero]).exact_replay_summary(2));
    assert_eq!(
        c.components.input.value,
        ComponentValue::ReportedClaimSum(0)
    );
    assert_eq!(c.components.input.states.valid_zero, 1);
    let huge = RECORD.replace(
        "\"input_tokens\":10",
        "\"input_tokens\":18446744073709551615",
    );
    let other = huge
        .replace("fictional-message", "fictional-other-message")
        .replace("fictional-uuid-a", "fictional-uuid-b");
    let c = claims(batch(&[&huge, &other]).exact_replay_summary(2));
    assert_eq!(c.components.input.value, ComponentValue::Overflow);
    assert_eq!(c.representatives, 2);
}

#[test]
fn no_eligible_observations_is_indeterminate_and_not_zero_usage() {
    let c = claims(batch(&["{}"]).exact_replay_summary(MAX_RECORDS_CEILING));
    assert_eq!(
        c.components.input.value,
        ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
    );
    assert_eq!(c.unresolved_observations, 1);
    let c = claims(batch(&[]).exact_replay_summary(0));
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(c.unresolved_observations, 0);
    assert_eq!(
        c.components.input.value,
        ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
    );
}

#[test]
fn unknown_flags_and_missing_uuid_can_invalidate_an_otherwise_eligible_group() {
    for other in [
        RECORD.replace("\"uuid\":\"fictional-uuid-a\",", ""),
        RECORD.replace("\"isSidechain\":false,", ""),
    ] {
        let c = claims(batch(&[RECORD, RECORD, &other]).exact_replay_summary(3));
        assert_eq!(c.eligible_observations, 0);
        assert_eq!(c.unresolved_observations, 3);
        assert_eq!(c.replay_observations, 0);
    }
}

#[test]
fn hard_identity_limit_is_inclusive_and_lowered_limit_never_returns_a_prefix() {
    let records = vec![RECORD; MAX_RECORDS_CEILING];
    let b = batch(&records);
    let result = b.exact_replay_summary(MAX_RECORDS_CEILING - 1);
    assert_eq!(
        result.evaluation,
        ReplayEvaluation::IdentityBudgetExhausted {
            observations: MAX_RECORDS_CEILING as u64,
            limit: MAX_RECORDS_CEILING - 1,
        }
    );
    let c = claims(b.exact_replay_summary(MAX_RECORDS_CEILING));
    assert_eq!(c.eligible_observations, MAX_RECORDS_CEILING as u64);
    assert_eq!(c.representatives, 1);
    assert_eq!(c.replay_observations, MAX_RECORDS_CEILING as u64 - 1);
    assert_eq!(
        c.components.input.value,
        ComponentValue::ReportedClaimSum(10)
    );
}

#[test]
fn malformed_oversized_blank_and_pending_lines_keep_original_coverage() {
    let mut b = batch(&[RECORD, "{bad}"]);
    // Collect the same synthetic scope with smaller framing limits and a pending tail.
    let text = format!(
        "{RECORD}\n{{bad}}\n{}\n  \n{{\"message\":",
        "x".repeat(1025)
    );
    b = collect_claude_jsonl_memory(
        b.environment,
        b.source,
        &b.scan_run,
        &[text.as_bytes()],
        ClaudeReadLimits {
            max_line_bytes: 1024,
            ..ClaudeReadLimits::default()
        },
    )
    .unwrap();
    let result = b.exact_replay_summary(1);
    assert_eq!(result.raw, b.summary());
    assert_eq!(result.raw.lines.oversized, 1);
    assert_eq!(result.raw.lines.blank, 1);
    assert_eq!(result.raw.lines.rejected.malformed_json, 1);
    assert_eq!(result.raw.lines.files_with_pending_tail, 1);
    assert!(result.raw.caveats.unparsed_lines && result.raw.caveats.pending_tails);
    assert_eq!(claims(result).representatives, 1);
}

#[test]
fn replays_across_files_are_compared_only_within_the_explicit_batch() {
    let base = batch(&[RECORD]);
    let text = format!("{RECORD}\n");
    let b = collect_claude_jsonl_memory(
        base.environment,
        base.source,
        &base.scan_run,
        &[text.as_bytes(), text.as_bytes()],
        ClaudeReadLimits::default(),
    )
    .unwrap();
    let result = b.exact_replay_summary(2);
    assert_eq!(result.raw.files.read, 2);
    assert_eq!(result.raw.records.observations, 2);
    assert_eq!(claims(result).replay_observations, 1);
    assert_eq!(
        claims(batch(&[RECORD]).exact_replay_summary(1)).replay_observations,
        0
    );
}

// ABF142: each counter counts affected observations, never pairs or physical events.
#[test]
fn conflict_attribution_separates_retained_evidence_and_preserves_accounting() {
    let cases = [
        (RECORD.replace("03:04:05Z", "03:04:06Z"), [0, 2, 0, 0]),
        (
            RECORD.replace("\"output_tokens\":3", "\"output_tokens\":7"),
            [0, 0, 2, 0],
        ),
        (
            RECORD.replace(
                "\"input_tokens\":10",
                "\"unknown_tokens\":0,\"input_tokens\":10",
            ),
            [1, 0, 0, 2],
        ),
        (
            RECORD.replace("\"isSidechain\":false", "\"isSidechain\":true"),
            [1, 2, 0, 0],
        ),
    ];
    for (other, expected) in cases {
        let b = batch(&[RECORD, &other]);
        let result = b.exact_replay_summary(2);
        assert_eq!(result.raw, b.summary());
        let c = claims(result);
        assert_eq!(c.unresolved.conflicting_evidence, 2);
        assert_eq!(c.eligible_observations, 0);
        let d = c.conflicts;
        assert_eq!(
            [
                d.ineligible_peer,
                d.identity_metadata,
                d.token_fields,
                d.parser_evidence
            ],
            expected
        );
        assert_eq!(d.uuid_related, 2);
        assert_eq!(d.session_message_related, 2);
    }
}

#[test]
fn identical_ineligible_peers_need_no_evidence_difference_to_conflict() {
    let sidechain = RECORD.replace("\"isSidechain\":false", "\"isSidechain\":true");
    let c = claims(batch(&[&sidechain, &sidechain]).exact_replay_summary(2));
    assert_eq!(
        c.conflicts,
        ConflictCounts {
            ineligible_peer: 2,
            uuid_related: 2,
            session_message_related: 2,
            ..ConflictCounts::default()
        }
    );
}

#[test]
fn conflict_relations_are_reported_only_for_conflicting_peers() {
    for (other, uuid_related, session_message_related) in [
        (RECORD.replace("fictional-message", "other-message"), 2, 0),
        (RECORD.replace("fictional-uuid-a", "other-uuid"), 0, 2),
    ] {
        let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
        assert_eq!(c.conflicts.uuid_related, uuid_related);
        assert_eq!(c.conflicts.session_message_related, session_message_related);
        assert_eq!(c.conflicts.identity_metadata, 2);
    }
    let c = claims(batch(&[RECORD, RECORD]).exact_replay_summary(2));
    assert_eq!(c.conflicts, ConflictCounts::default());
}

#[test]
fn overlapping_conflict_reasons_count_each_observation_once_across_peers() {
    let other = RECORD.replace("03:04:05Z", "03:04:06Z").replace(
        "\"output_tokens\":3",
        "\"unknown_tokens\":0,\"output_tokens\":7",
    );
    let b = batch(&[RECORD, RECORD, &other]);
    let result = b.exact_replay_summary(3);
    assert_eq!(result.raw, b.summary());
    assert_eq!(
        result.raw.components.output.value,
        ComponentValue::ReportedClaimSum(13)
    );
    let c = claims(result);
    assert_eq!(
        c.conflicts,
        ConflictCounts {
            ineligible_peer: 2,
            identity_metadata: 3,
            token_fields: 3,
            parser_evidence: 3,
            uuid_related: 3,
            session_message_related: 3
        }
    );
    assert_eq!(c.unresolved.conflicting_evidence, 3);
    assert_eq!(c.representatives, 0);
    assert_eq!(c.replay_observations, 0);
    assert_eq!(
        c,
        claims(batch(&[&other, RECORD, RECORD]).exact_replay_summary(3))
    );
}

#[test]
fn metadata_only_candidates_are_unconfirmed_and_preserve_accounting() {
    let other = RECORD
        .replace("fictional-uuid-a", "fictional-uuid-b")
        .replace("03:04:05Z", "03:04:06Z");
    let b = batch(&[RECORD, &other]);
    let result = b.exact_replay_summary(2);
    assert_eq!(result.raw, b.summary());
    let c = claims(result);
    assert_eq!(c.candidates.matching_request, 2);
    assert_eq!(c.candidates.request_mismatch, 0);
    assert_eq!(c.candidates.missing_identity, 0);
    assert_eq!(c.candidates.other_metadata, 0);
    assert_eq!(c.unresolved.conflicting_evidence, 2);
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(c.replay_observations, 0);
}

#[test]
fn metadata_candidates_separate_requests_missing_identity_and_other_metadata() {
    for (other, expected) in [
        (
            RECORD.replace("fictional-request", "other-request"),
            [0, 2, 0, 0],
        ),
        (
            RECORD.replace("\"requestId\":\"fictional-request\",", ""),
            [0, 0, 2, 0],
        ),
        (
            RECORD.replace("\"uuid\":\"fictional-uuid-a\"", "\"uuid\":null"),
            [0, 0, 2, 0],
        ),
        // Wrong-type session evidence also changes parser diagnostics, outside metadata-only scope.
        (
            RECORD.replace("\"sessionId\":\"fictional-session\"", "\"sessionId\":[]"),
            [0, 0, 0, 0],
        ),
        (
            RECORD.replace("\"id\":\"fictional-message\"", "\"id\":\"\""),
            [0, 0, 2, 0],
        ),
        (
            RECORD.replace("fictional-model", "other-model"),
            [0, 0, 0, 2],
        ),
        (
            RECORD.replace("\"isSidechain\":false", "\"isSidechain\":true"),
            [0, 0, 0, 2],
        ),
        (
            RECORD.replace("\"type\":\"assistant\"", "\"type\":\"progress\""),
            [0, 0, 0, 2],
        ),
        (RECORD.replace("03:04:05Z", "03:04:06Z"), [2, 0, 0, 0]),
        (
            RECORD.replace("fictional-uuid-a", "other-uuid"),
            [2, 0, 0, 0],
        ),
    ] {
        let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
        assert_eq!(
            [
                c.candidates.matching_request,
                c.candidates.request_mismatch,
                c.candidates.missing_identity,
                c.candidates.other_metadata,
            ],
            expected,
            "synthetic case: {other}"
        );
        assert_eq!(c.unresolved.conflicting_evidence, 2);
        assert_eq!(c.replay_observations, 0);
    }
}

#[test]
fn changed_usage_or_parser_evidence_never_forms_metadata_only_candidates() {
    for other in [
        RECORD.replace("\"input_tokens\":10", "\"input_tokens\":12"),
        RECORD.replace("\"input_tokens\":10", "\"input_tokens\":null"),
        RECORD.replace("\"input_tokens\":10", "\"unknown_tokens\":1,\"input_tokens\":10"),
        RECORD.replace("\"cache_creation_input_tokens\":11", "\"cache_creation_input_tokens\":11,\"cache_creation\":{\"ephemeral_5m_input_tokens\":11,\"ephemeral_1h_input_tokens\":0}"),
    ] {
        let other = other.replace("fictional-uuid-a", "other-uuid");
        let c = claims(batch(&[RECORD, &other]).exact_replay_summary(2));
        assert_eq!(c.candidates, CandidateCounts::default());
        assert_eq!(c.eligible_observations, 0);
    }
    assert_eq!(
        claims(batch(&[RECORD, RECORD]).exact_replay_summary(2)).candidates,
        CandidateCounts::default()
    );
}

#[test]
fn candidate_categories_overlap_across_peers_without_transitive_deduplication() {
    let metadata = RECORD.replace("fictional-uuid-a", "other-uuid");
    let request = RECORD.replace("fictional-request", "other-request");
    let changed = RECORD.replace("\"input_tokens\":10", "\"input_tokens\":12");
    let records = [RECORD, RECORD, &metadata, &request, &changed];
    let c = claims(batch(&records).exact_replay_summary(5));
    assert_eq!(c.candidates.matching_request, 3);
    assert_eq!(c.candidates.request_mismatch, 4);
    assert_eq!(c.conflicts.token_fields, 5);
    assert_eq!(c.unresolved.conflicting_evidence, 5);
    assert_eq!(c.eligible_observations, 0);
    assert_eq!(
        c,
        claims(batch(&[&changed, &request, &metadata, RECORD, RECORD]).exact_replay_summary(5))
    );
}
