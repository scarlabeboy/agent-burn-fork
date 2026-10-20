//! Focused regressions A–Q of the G3B gate, including ABF103B-N1 (cases E, F, G).
mod support;

use abf_counting::*;
use abf_identity_relations::UnverifiedHint::*;
use abf_parser::{Field, ParseResult, Status};
use support::*;

use ComponentValue::{NoValidClaims, ReportedClaimSum};
use IndeterminateReason::*;

fn windows_only(
    set: &abf_identity_relations::SyntheticSet,
    inv: RunInventory<'_>,
) -> EnvironmentResult {
    *sel(&run(set, inv, Selection::WindowsOnly).windows)
}

#[test]
fn a_valid_zero_versus_absent_claim() {
    let zero = windows_only(
        &build(vec![win("h1", r#"{"tokens":{"input":0}}"#)]),
        RunInventory::Undeclared,
    );
    let absent = windows_only(
        &build(vec![win("h1", r#"{"tokens":{"output":1}}"#)]),
        RunInventory::Undeclared,
    );
    check(
        &zero.components.input,
        ReportedClaimSum(0),
        (1, 1, 1),
        FieldStateCounts { valid_zero: 1, ..Z },
        &[RunInventoryUndeclared],
    );
    check(
        &absent.components.input,
        NoValidClaims,
        (1, 0, 0),
        FieldStateCounts { absent: 1, ..Z },
        &[MissingClaims, RunInventoryUndeclared],
    );
    assert_ne!(zero.components.input.value, absent.components.input.value);
}

#[test]
fn b_missing_fields_versus_no_valid_claims_versus_indeterminate() {
    // Missing field on an eligible record: NoValidClaims with an `absent` bin.
    let missing = windows_only(
        &build(vec![win("h1", r#"{"tokens":{"input":1}}"#)]),
        RunInventory::Declared(&[r1(Complete)]),
    );
    check(
        &missing.components.output,
        NoValidClaims,
        (1, 0, 0),
        FieldStateCounts { absent: 1, ..Z },
        &[MissingClaims],
    );
    // No eligible record but reported traversal: NoValidClaims with empty bins.
    let none = windows_only(&build(vec![]), RunInventory::Declared(&[r1(Partial)]));
    check(
        &none.components.output,
        NoValidClaims,
        (0, 0, 0),
        Z,
        &[PartialRun],
    );
    // No traversal reported: Indeterminate.
    let indet = windows_only(&build(vec![]), RunInventory::Declared(&[r1(NotCollected)]));
    check(
        &indet.components.output,
        indeterminate(NoTraversalReported),
        (0, 0, 0),
        Z,
        &[NotCollectedRun],
    );
}

#[test]
fn c_qualified_and_incomplete_outcomes_with_constructed_contradictions() {
    let ParseResult::Observation(mut q_bad) = parsed(r#"{"tokens":{"input":9}}"#) else {
        unreachable!()
    };
    q_bad.status = Status::Qualified; // C4
    let ParseResult::Observation(mut i_bad) = parsed(r#"{}"#) else {
        unreachable!()
    };
    i_bad.status = Status::Incomplete;
    i_bad.tokens = Field::Valid(());
    i_bad.cache = Field::Absent;
    i_bad.input = Field::Absent;
    i_bad.output = Field::Absent;
    i_bad.total = Field::Valid(4); // C1
    let set = build(vec![
        win("q", r#"{"tokens":{"input":2,"output":null}}"#),
        win("i", r#"{"tokens":{}}"#),
        occ(
            "qb",
            ParseResult::Observation(q_bad),
            W,
            ("W1", "S1", "R1"),
            Complete,
        ),
        occ(
            "ib",
            ParseResult::Observation(i_bad),
            W,
            ("W1", "S1", "R1"),
            Complete,
        ),
    ]);
    let w = windows_only(&set, RunInventory::Declared(&[r1(Complete)]));
    assert_eq!(w.outcomes.0, outcomes(0, 2, 2, 0));
    assert_eq!((w.records.0.eligible, w.records.0.contradictory), (1, 2));
    let shared = [IncompleteRecordsPresent, ContradictoryRecordsExcluded];
    check(
        &w.components.input,
        ReportedClaimSum(2),
        (1, 1, 0),
        FieldStateCounts {
            valid_positive: 1,
            ..Z
        },
        &[FromQualifiedRecords, shared[0], shared[1]],
    );
    check(
        &w.components.total,
        NoValidClaims,
        (1, 0, 0),
        FieldStateCounts { absent: 1, ..Z },
        &[MissingClaims, shared[0], shared[1]],
    );
}

#[test]
fn d_rejected_records_preserve_existing_eligible_sums() {
    let frames = [
        r#"{"tokens":{"input":40,"cache":{"read":0}}}"#,
        r#"{"tokens":{"input":60}}"#,
    ];
    let base = windows_only(
        &build(vec![win("a", frames[0]), win("b", frames[1])]),
        RunInventory::Undeclared,
    );
    let with_rejections = windows_only(
        &build(vec![
            occ("r0", rejected(), W, ("W1", "S1", "R1"), Complete),
            win("a", frames[0]),
            occ("r1", rejected(), W, ("W1", "S1", "R1"), Complete),
            win("b", frames[1]),
        ]),
        RunInventory::Undeclared,
    );
    assert_eq!(with_rejections.outcomes.0.rejected, 2);
    for (b, r) in all(&base.components)
        .into_iter()
        .zip(all(&with_rejections.components))
    {
        assert_eq!(b.value, r.value);
        assert_eq!(
            (
                b.evidence.eligible_records,
                b.evidence.contributing,
                b.evidence.zero_valued,
                b.evidence.field_states
            ),
            (
                r.evidence.eligible_records,
                r.evidence.contributing,
                r.evidence.zero_valued,
                r.evidence.field_states
            )
        );
        assert!(r.evidence.qualifications.contains(RejectedRecordsPresent));
    }
    assert_eq!(
        with_rejections.components.input.value,
        ReportedClaimSum(100)
    );
    assert_eq!(
        with_rejections.components.cache_read.value,
        ReportedClaimSum(0)
    );
}

#[test]
fn e_empty_scope_without_run_evidence_is_indeterminate() {
    for inv in [RunInventory::Undeclared, RunInventory::Declared(&[])] {
        for selection in [
            Selection::WindowsOnly,
            Selection::SelectedWslOnly,
            Selection::Both,
        ] {
            let p = run(&build(vec![]), inv, selection);
            for s in [&p.windows, &p.selected_wsl] {
                if let ScopeResult::Selected(r) = s {
                    assert_eq!(r.runs.coverage, CoverageState::NoEvidence);
                    for c in all(&r.components) {
                        assert_eq!(c.value, indeterminate(NoRunEvidence));
                    }
                }
            }
        }
    }
}

#[test]
fn f_one_rejected_record_with_reported_complete_run() {
    // N1-A: Undeclared and Declared matching; no invented numeric zero.
    let set = build(vec![occ("h1", rejected(), W, ("W1", "S1", "R1"), Complete)]);
    for (inv, extra) in [
        (RunInventory::Undeclared, Some(RunInventoryUndeclared)),
        (RunInventory::Declared(&[r1(Complete)]), None),
    ] {
        let w = windows_only(&set, inv);
        assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
        let mut flags = vec![RejectedRecordsPresent];
        flags.extend(extra);
        for c in all(&w.components) {
            check(c, NoValidClaims, (0, 0, 0), Z, &flags);
        }
    }
}

#[test]
fn g_declared_empty_inventory_contradicted_by_a_record_is_inconsistent() {
    // N1-B with an eligible record and with a rejected record, for every status.
    for s in [Complete, Partial, Failed, NotCollected, Unavailable] {
        let eligible = windows_only(
            &build(vec![occ(
                "h1",
                parsed(r#"{"tokens":{"input":100}}"#),
                W,
                ("W1", "S1", "R1"),
                s,
            )]),
            RunInventory::Declared(&[]),
        );
        assert_eq!(eligible.runs.coverage, CoverageState::Inconsistent);
        assert_eq!(eligible.runs.undeclared_record_keys, 1);
        assert_eq!(eligible.runs.inventory, InventoryBasis::Declared);
        assert!(
            eligible
                .runs
                .qualifications
                .contains(UndeclaredRunInRecords)
        );
        assert_eq!(eligible.components.input.value, ReportedClaimSum(100));
        let rejected_only = windows_only(
            &build(vec![occ("h1", rejected(), W, ("W1", "S1", "R1"), s)]),
            RunInventory::Declared(&[]),
        );
        assert_eq!(rejected_only.runs.coverage, CoverageState::Inconsistent);
        assert_eq!(rejected_only.components.input.value, NoValidClaims);
    }
    // The four inventory/evidence combinations of ABF103C §5 stay distinct.
    let one = build(vec![win("h1", r#"{"tokens":{"input":1}}"#)]);
    let empty = build(vec![]);
    assert_eq!(
        windows_only(&empty, RunInventory::Undeclared).runs.coverage,
        CoverageState::NoEvidence
    );
    assert_eq!(
        windows_only(&empty, RunInventory::Declared(&[]))
            .runs
            .coverage,
        CoverageState::NoEvidence
    );
    assert_eq!(
        windows_only(&one, RunInventory::Undeclared).runs.coverage,
        CoverageState::ReportedComplete
    );
    assert_eq!(
        windows_only(&one, RunInventory::Declared(&[]))
            .runs
            .coverage,
        CoverageState::Inconsistent
    );
}

#[test]
fn h_conflicting_raw_statuses_keep_failed_and_conflict_flags() {
    // Record Complete versus declaration Failed: no arbitration either way.
    let w = windows_only(
        &build(vec![win("h1", r#"{"tokens":{"input":5}}"#)]),
        RunInventory::Declared(&[r1(Failed)]),
    );
    assert_eq!(
        (w.runs.conflicting_keys, w.runs.resolved),
        (1, status(0, 0, 0, 0, 0))
    );
    assert_eq!(w.runs.raw_record_claims, status(0, 1, 0, 0, 0));
    assert_eq!(w.runs.raw_declared_claims, status(0, 0, 0, 1, 0));
    assert_eq!(w.runs.coverage, CoverageState::Inconsistent);
    assert_eq!(
        w.runs.qualifications.bits(),
        q(&[FailedRun, RunStatusConflict])
    );
    // Unavailable/Partial/NotCollected raw evidence survives a conflict too.
    let set = build(vec![
        occ(
            "a",
            parsed(r#"{"tokens":{"input":1}}"#),
            W,
            ("W1", "S1", "R1"),
            Unavailable,
        ),
        occ(
            "b",
            parsed(r#"{"tokens":{"input":1}}"#),
            W,
            ("W1", "S1", "R1"),
            Partial,
        ),
    ]);
    let w = windows_only(&set, RunInventory::Declared(&[r1(NotCollected)]));
    assert_eq!(w.runs.conflicting_keys, 1);
    assert_eq!(
        w.runs.qualifications.bits(),
        q(&[
            UnavailableRun,
            PartialRun,
            NotCollectedRun,
            RecordWithoutReportedTraversal,
            RunStatusConflict
        ])
    );
    assert_eq!(w.components.input.value, ReportedClaimSum(2));
}

#[test]
fn i_ineligible_observation_with_unsupported_component_stays_visible() {
    for frame in [
        r#"{"tokens":{"reasoning":5}}"#,
        r#"{"tokens":{"cache":{"other":1}}}"#,
    ] {
        let w = windows_only(&build(vec![win("h1", frame)]), RunInventory::Undeclared);
        assert_eq!(w.records.0.eligible, 0);
        assert_eq!(w.records.0.unsupported_component_observations, 1);
        for c in all(&w.components) {
            assert!(
                c.evidence
                    .qualifications
                    .contains(UnknownComponentsDiscarded)
            );
            assert_eq!(c.value, NoValidClaims);
        }
    }
}

#[test]
fn j_equal_claimed_message_ids_are_never_deduplicated() {
    let frame = r#"{"id":"msg-1","sessionID":"s-1","modelID":"m","providerID":"p","tokens":{"input":100,"output":5}}"#;
    let set = build(vec![
        win("h1", frame),
        win("h2", frame),
        occ("h3", parsed(frame), L, ("W1", "S1", "R1"), Complete),
    ]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    assert_eq!(
        sel(&p.windows).components.input.value,
        ReportedClaimSum(200)
    );
    assert_eq!(
        sel(&p.selected_wsl).components.input.value,
        ReportedClaimSum(100)
    );
    assert_eq!(p.computer.components.input.value, ReportedClaimSum(300));
    assert_eq!(p.computer.components.output.value, ReportedClaimSum(15));
    assert_eq!(p.computer.stored_occurrences, Exact(3));
    assert_eq!(p.computer.physical_events, PhysicalEventCount::Unresolved);
    assert_eq!(p.computer.usage, UsageTotal::Unsupported);
}

#[test]
fn k_adding_removing_and_reversing_assertions_leaves_nonrelation_results() {
    let make = |rels: &[(&str, &str)]| {
        let mut set = build(vec![
            win("a", r#"{"tokens":{"input":3}}"#),
            win("b", r#"{"tokens":{"input":3}}"#),
            occ(
                "c",
                parsed(r#"{"tokens":{"input":3}}"#),
                L,
                ("L1", "S1", "R1"),
                Failed,
            ),
        ]);
        for (f, t) in rels {
            relate(&mut set, f, t, EqualReportedValues);
        }
        run(&set, RunInventory::Undeclared, Selection::Both)
    };
    let base = make(&[]);
    for rels in [
        &[("a", "b")][..],
        &[("b", "a")],
        &[("a", "b"), ("b", "a"), ("a", "c"), ("c", "b")],
    ] {
        let p = make(rels);
        assert_eq!(without_relation_outputs(&p, &base), base);
        assert_eq!(p.relations.selected.total, rels.len() as u32);
    }
}

#[test]
fn l_one_cross_environment_assertion_is_one_stored_assertion() {
    let mut set = build(vec![
        win("a", r#"{"tokens":{"input":3}}"#),
        occ(
            "b",
            parsed(r#"{"tokens":{"input":3}}"#),
            L,
            ("L1", "S1", "R1"),
            Complete,
        ),
    ]);
    relate(&mut set, "a", "b", MatchingClaimedIdentifiers);
    let g = run(&set, RunInventory::Undeclared, Selection::Both).relations;
    assert_eq!(
        g.selected,
        AssertionPartition {
            windows_within: 0,
            selected_wsl_within: 0,
            cross_environment: 1,
            total: 1
        }
    );
    assert_eq!(g.set_global_entries, Exact(1));
    assert_eq!(
        g.windows_incident_entries.0 + g.selected_wsl_incident_entries.0,
        2
    );
    assert_eq!(g.referenced_occurrences, Exact(2));
}

#[test]
fn m_missing_partial_failed_unavailable_not_collected_evidence() {
    let frame = r#"{"tokens":{"input":8}}"#;
    let cases = [
        (Partial, CoverageState::ReportedIncomplete, PartialRun),
        (Failed, CoverageState::ReportedIncomplete, FailedRun),
        (Unavailable, CoverageState::Inconsistent, UnavailableRun),
        (NotCollected, CoverageState::Inconsistent, NotCollectedRun),
    ];
    for (s, cov, flag) in cases {
        // With a record: claim retained, status flag visible, never repaired.
        let w = windows_only(
            &build(vec![occ("h1", parsed(frame), W, ("W1", "S1", "R1"), s)]),
            RunInventory::Declared(&[r1(s)]),
        );
        assert_eq!(w.runs.coverage, cov);
        assert_eq!(w.components.input.value, ReportedClaimSum(8));
        assert!(w.components.input.evidence.qualifications.contains(flag));
        // Without a record: never a numeric zero.
        let w = windows_only(&build(vec![]), RunInventory::Declared(&[r1(s)]));
        assert!(w.components.input.evidence.qualifications.contains(flag));
        assert!(!matches!(w.components.input.value, ReportedClaimSum(_)));
    }
    // Missing environment: Both with nothing for Selected WSL shows NoEvidence, not zero.
    let p = run(
        &build(vec![win("h1", frame)]),
        RunInventory::Declared(&[r1(Complete)]),
        Selection::Both,
    );
    assert_empty_scope(sel(&p.selected_wsl), &[]);
    assert!(
        p.computer
            .components
            .input
            .evidence
            .qualifications
            .contains(EnvironmentIndeterminate)
    );
}

#[test]
fn n_zero_record_declarations_and_reference_multiplicity() {
    // Equal refs across kinds are separate scopes; equal refs within a scope count once.
    let set = build(vec![
        win("a", r#"{"tokens":{"input":1}}"#),
        occ(
            "b",
            parsed(r#"{"tokens":{"input":1}}"#),
            W,
            ("W1", "S2", "R2"),
            Complete,
        ),
    ]);
    let d = [
        r1(Complete),
        decl(W, ("W1", "S2", "R2"), Complete),
        decl(W, ("W3", "S3", "R3"), Complete),
        decl(L, ("W1", "S1", "R1"), NotCollected),
    ];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.environment_refs, RefMultiplicity::Multiple(2));
    assert_eq!(w.runs.run_keys, 3);
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    assert!(
        w.components
            .input
            .evidence
            .qualifications
            .contains(EnvironmentRefAmbiguous)
    );
    let l = sel(&p.selected_wsl);
    assert_eq!(l.environment_refs, RefMultiplicity::One);
    assert_eq!((l.runs.run_keys, l.stored_occurrences.0), (1, 0));
    assert_eq!(l.components.input.value, indeterminate(NoTraversalReported));
    // Under single selection the other kind's declarations are excluded and counted.
    let p = run(&set, RunInventory::Declared(&d), Selection::WindowsOnly);
    assert_eq!(
        p.excluded.0,
        ExcludedCounts {
            occurrences: 0,
            declarations: 1,
            relation_entries: 0
        }
    );
}

#[test]
fn o_independent_raw_token_fields_and_cache_components() {
    let set = build(vec![
        win(
            "a",
            r#"{"tokens":{"input":1,"output":2,"total":3,"cache":{"read":4,"write":5}}}"#,
        ),
        win("b", r#"{"tokens":{"total":1000,"cache":{"read":0}}}"#),
        win("c", r#"{"tokens":{"output":7,"cache":null}}"#),
    ]);
    let w = windows_only(&set, RunInventory::Undeclared);
    let c = &w.components;
    let flags = [MissingClaims, RunInventoryUndeclared];
    let shared_q = |extra: &[Qualification]| [extra, &[RunInventoryUndeclared]].concat();
    check(
        &c.input,
        ReportedClaimSum(1),
        (3, 1, 0),
        FieldStateCounts {
            valid_positive: 1,
            absent: 2,
            ..Z
        },
        &flags,
    );
    // `c` is parser-Qualified (explicit null cache) and contributes to `output`.
    check(
        &c.output,
        ReportedClaimSum(9),
        (3, 2, 0),
        FieldStateCounts {
            valid_positive: 2,
            absent: 1,
            ..Z
        },
        &shared_q(&[MissingClaims, FromQualifiedRecords]),
    );
    check(
        &c.total,
        ReportedClaimSum(1003),
        (3, 2, 0),
        FieldStateCounts {
            valid_positive: 2,
            absent: 1,
            ..Z
        },
        &flags,
    );
    check(
        &c.cache_read,
        ReportedClaimSum(4),
        (3, 2, 1),
        FieldStateCounts {
            valid_zero: 1,
            valid_positive: 1,
            unsupported: 1,
            ..Z
        },
        &flags,
    );
    check(
        &c.cache_write,
        ReportedClaimSum(5),
        (3, 1, 0),
        FieldStateCounts {
            valid_positive: 1,
            absent: 1,
            unsupported: 1,
            ..Z
        },
        &flags,
    );
    // `total` is never compared with or derived from other components.
    assert_eq!(w.outcomes.0, outcomes(2, 1, 0, 0));
}

#[test]
fn p_multiple_u64_max_claims_use_checked_u128() {
    let max = r#"{"tokens":{"input":18446744073709551615,"output":18446744073709551615}}"#;
    let set = build(vec![
        win("a", max),
        win("b", max),
        occ("c", parsed(max), L, ("L1", "S1", "R1"), Complete),
    ]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    let two = 2 * u128::from(u64::MAX);
    assert_eq!(
        sel(&p.windows).components.input.value,
        ReportedClaimSum(two)
    );
    assert_eq!(
        p.computer.components.input.value,
        ReportedClaimSum(3 * u128::from(u64::MAX))
    );
    assert_eq!(
        p.computer.components.output.value,
        ReportedClaimSum(3 * u128::from(u64::MAX))
    );
    assert!(two > u128::from(u64::MAX));
}

#[test]
fn q_identical_input_yields_identical_results() {
    let make = || {
        let mut set = build(vec![
            win("a", r#"{"tokens":{"input":1}}"#),
            occ("b", rejected(), W, ("W1", "S1", "R1"), Failed),
            occ(
                "c",
                parsed(r#"{"tokens":{"output":2}}"#),
                L,
                ("L1", "S1", "R1"),
                Partial,
            ),
        ]);
        relate(&mut set, "a", "c", PossibleReplayOrCopy);
        set
    };
    let d = [r1(Complete), decl(L, ("L2", "S2", "R2"), Unavailable)];
    let (s1, s2) = (make(), make());
    let first = run(&s1, RunInventory::Declared(&d), Selection::Both);
    assert_eq!(run(&s1, RunInventory::Declared(&d), Selection::Both), first);
    assert_eq!(run(&s2, RunInventory::Declared(&d), Selection::Both), first);
}
