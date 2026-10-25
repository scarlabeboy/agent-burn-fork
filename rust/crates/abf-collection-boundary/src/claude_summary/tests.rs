//! Summary rules on hand-built batches: overflow, empty/absent evidence, file rejections and
//! every parser rejection code. Fictional data only; no I/O.

use abf_parser::{LongLimits, parse_claude_long};

use super::*;
use crate::claude_jsonl::{ClaudeFileReport, ClaudeLine, ClaudeLineCounts};
use crate::identity::{Harness, WslDistroName};
use crate::observation::{CaptureTime, FileObjectRef, ScanRun, SkipReason, SkippedEntry};
use crate::scope::PathRejection;

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).unwrap()
}

fn lines_from(texts: &[&str]) -> Vec<ClaudeLine> {
    texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let outcome = match parse_claude_long(text.as_bytes(), None, LongLimits::MAX) {
                abf_parser::ClaudeParseResult::Observation(o) => {
                    ClaudeLineOutcome::Observation(Box::new(o))
                }
                abf_parser::ClaudeParseResult::Rejected(code) => ClaudeLineOutcome::Rejected(code),
            };
            ClaudeLine {
                line_ordinal: index as u64 + 1,
                byte_offset: 0,
                byte_length: text.len() as u64,
                outcome,
            }
        })
        .collect()
}

fn file(ordinal: u32, status: ClaudeFileStatus, lines: Vec<ClaudeLine>) -> ClaudeFileReport {
    let mut counts = ClaudeLineCounts::default();
    for line in &lines {
        counts.terminated_lines += 1;
        match line.outcome {
            ClaudeLineOutcome::Observation(_) => counts.observations += 1,
            ClaudeLineOutcome::Rejected(_) => counts.rejected += 1,
            ClaudeLineOutcome::Oversized => counts.oversized += 1,
        }
    }
    ClaudeFileReport {
        file_ordinal: ordinal,
        file_object: FileObjectRef::Unavailable,
        status,
        lines,
        counts,
        pending_tail_bytes: 0,
    }
}

fn batch(files: Vec<ClaudeFileReport>, skipped: Vec<SkippedEntry>) -> ClaudeBatch {
    let incomplete = skipped.iter().any(|s| s.reason.makes_incomplete())
        || files.iter().any(|f| f.status != ClaudeFileStatus::Read);
    ClaudeBatch {
        scan_run: ScanRun::new(id("run"), CaptureTime::NotRecorded),
        environment: EnvironmentIdentity::wsl(id("env"), WslDistroName::new("Fictional").unwrap()),
        source: SourceInstance::new(id("src"), Harness::ClaudeCode),
        host_match: HostMatch::Native,
        files,
        skipped,
        completeness: if incomplete {
            Completeness::Incomplete
        } else {
            Completeness::Complete
        },
    }
}

fn usage(fields: &str) -> String {
    format!(r#"{{"message":{{"usage":{{{fields}}}}}}}"#)
}

#[test]
fn checked_sums_overflow_explicitly_and_stay_overflowed() {
    let max = u64::MAX;
    let cases: [(&[u64], ComponentValue); 5] = [
        (&[max], ComponentValue::ReportedClaimSum(max)),
        (&[max - 1, 1], ComponentValue::ReportedClaimSum(max)),
        (&[max, 0], ComponentValue::ReportedClaimSum(max)),
        (&[max, 1], ComponentValue::Overflow),
        (&[max, 1, 0], ComponentValue::Overflow), // a later zero does not undo it
    ];
    for (values, expected) in cases {
        let texts: Vec<String> = values
            .iter()
            .map(|v| usage(&format!(r#""input_tokens":{v},"output_tokens":1"#)))
            .collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let summary = batch(
            vec![file(0, ClaudeFileStatus::Read, lines_from(&refs))],
            vec![],
        )
        .summary();
        assert_eq!(summary.components.input.value, expected, "{values:?}");
        // Other components are unaffected by one component's overflow.
        assert_eq!(
            summary.components.output.value,
            ComponentValue::ReportedClaimSum(values.len() as u64)
        );
        assert_eq!(summary.components.input.states.valid, values.len() as u64);
    }
    // Overflow on two different components is reported per component.
    let both = usage(
        r#""input_tokens":18446744073709551615,"cache_read_input_tokens":18446744073709551615"#,
    );
    let summary = batch(
        vec![file(0, ClaudeFileStatus::Read, lines_from(&[&both, &both]))],
        vec![],
    )
    .summary();
    assert_eq!(summary.components.input.value, ComponentValue::Overflow);
    assert_eq!(
        summary.components.cache_read.value,
        ComponentValue::Overflow
    );
    assert_eq!(
        summary.components.output.value,
        ComponentValue::NoValidClaims
    );
}

#[test]
fn no_observations_is_indeterminate_and_no_valid_claims_is_not_zero() {
    let empty = batch(vec![], vec![]).summary();
    for component in [
        empty.components.input,
        empty.components.output,
        empty.components.cache_creation,
        empty.components.cache_read,
    ] {
        assert_eq!(
            component.value,
            ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
        );
    }
    assert_eq!(empty.completeness, Completeness::Complete);
    assert_eq!(empty.basis, CountingBasis::ObservedRecordClaims);
    assert_eq!(empty.duplicates, DuplicateIdentity::Unresolved);

    let nulls = usage(r#""input_tokens":null,"output_tokens":-1"#);
    let summary = batch(
        vec![file(0, ClaudeFileStatus::Read, lines_from(&[&nulls]))],
        vec![],
    )
    .summary();
    assert_eq!(
        summary.components.input.value,
        ComponentValue::NoValidClaims
    );
    assert_eq!(summary.components.input.states.null, 1);
    assert_eq!(summary.components.output.states.out_of_range, 1);
    assert_eq!(summary.records.incomplete, 1);
    assert_eq!(summary.records.without_valid_claim, 1);
    let zero = usage(r#""input_tokens":0"#);
    let summary = batch(
        vec![file(0, ClaudeFileStatus::Read, lines_from(&[&zero]))],
        vec![],
    )
    .summary();
    assert_eq!(
        summary.components.input.value,
        ComponentValue::ReportedClaimSum(0)
    );
    assert_eq!(summary.components.input.states.valid_zero, 1);
    assert_eq!(
        summary.components.output.value,
        ComponentValue::NoValidClaims
    );
}

#[test]
fn file_rejections_skips_and_caveats_are_all_visible() {
    let ok = usage(r#""input_tokens":5"#);
    let files = vec![
        file(0, ClaudeFileStatus::Read, lines_from(&[&ok])),
        file(
            1,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::ChangedDuringRead),
            vec![],
        ),
        file(
            2,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::Io(
                std::io::ErrorKind::PermissionDenied,
            )),
            vec![],
        ),
        file(
            3,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::TooLarge),
            vec![],
        ),
        file(
            4,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::ByteBudget),
            vec![],
        ),
        file(
            5,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::OutputLimit),
            vec![],
        ),
        file(
            6,
            ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(PathRejection::HardLinked)),
            vec![],
        ),
    ];
    let skipped = vec![
        SkippedEntry {
            relative_path: None,
            reason: SkipReason::Symlink,
        },
        SkippedEntry {
            relative_path: None,
            reason: SkipReason::NotFixtureExtension,
        },
    ];
    let summary = batch(files, skipped).summary();
    assert_eq!(summary.completeness, Completeness::Incomplete);
    assert_eq!(
        summary.files,
        FileSummary {
            read: 1,
            rejected: 6,
            path: 1,
            too_large: 1,
            byte_budget: 1,
            output_limit: 1,
            changed_during_read: 1,
            io: 1,
        }
    );
    assert_eq!(
        summary.skipped,
        SkipSummary {
            total: 2,
            may_hide_data: 1,
            out_of_scope: 1
        }
    );
    assert_eq!(
        summary.caveats,
        Caveats {
            incomplete_batch: true,
            unparsed_lines: false,
            pending_tails: false,
            rejected_files: true,
            skipped_in_scope: true,
        }
    );
    // The readable file still contributes its claim; the rest is not turned into zero.
    assert_eq!(
        summary.components.input.value,
        ComponentValue::ReportedClaimSum(5)
    );
}

#[test]
fn every_parser_rejection_code_has_its_own_counter() {
    let codes = [
        Code::FrameBudget,
        Code::StringBudget,
        Code::DepthBudget,
        Code::EntryBudget,
        Code::ResultBudget,
        Code::InvalidLimits,
        Code::InvalidUtf8,
        Code::IncompleteJson,
        Code::MalformedJson,
        Code::TrailingData,
        Code::RootShape,
        Code::DuplicateKey,
    ];
    let lines = codes
        .iter()
        .enumerate()
        .map(|(i, code)| ClaudeLine {
            line_ordinal: i as u64 + 1,
            byte_offset: 0,
            byte_length: 0,
            outcome: ClaudeLineOutcome::Rejected(*code),
        })
        .collect();
    let summary = batch(vec![file(0, ClaudeFileStatus::Read, lines)], vec![]).summary();
    assert_eq!(summary.lines.rejected.total(), 12);
    let r = summary.lines.rejected;
    assert_eq!(
        [
            r.frame_budget,
            r.string_budget,
            r.depth_budget,
            r.entry_budget,
            r.result_budget,
            r.invalid_limits,
            r.invalid_utf8,
            r.incomplete_json,
            r.malformed_json,
            r.trailing_data,
            r.root_shape,
            r.duplicate_key
        ],
        [1; 12]
    );
    assert!(summary.caveats.unparsed_lines);
    assert_eq!(summary.records.observations, 0);
    assert_eq!(
        summary.components.input.value,
        ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
    );
}
