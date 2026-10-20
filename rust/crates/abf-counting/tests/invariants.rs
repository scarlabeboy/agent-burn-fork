//! Contract §10.3 acceptance tests T2–T19 (T1 lives in `scenarios.rs`; T6b and
//! the C1–C7 check identities are crate-internal unit tests).
mod support;

use abf_counting::*;
use abf_identity_relations::{SyntheticSet, UnverifiedHint::*};
use abf_parser::{Field, ParseResult, Status};
use support::*;

use ComponentValue::{NoValidClaims, ReportedClaimSum};
use IndeterminateReason::*;

fn observation(frame: &str) -> abf_parser::Observation {
    match parsed(frame) {
        ParseResult::Observation(o) => o,
        ParseResult::Rejected(c) => panic!("fixture rejected: {c:?}"),
    }
}

// ---- T2 permutation invariance ------------------------------------------------

#[test]
fn t2_occurrence_and_declaration_order_do_not_change_results() {
    // S03
    let a = r#"{"id":"msg-1","tokens":{"input":100}}"#;
    let d = [r1(Complete)];
    let p1 = run(
        &build(vec![win("h1", a), win("h2", a)]),
        RunInventory::Declared(&d),
        Selection::Both,
    );
    let p2 = run(
        &build(vec![win("h2", a), win("h1", a)]),
        RunInventory::Declared(&d),
        Selection::Both,
    );
    assert_eq!(p1, p2);

    // S17: record order and declaration order (with an unrelated zero-record key).
    let ok = || win("h1", r#"{"tokens":{"input":100}}"#);
    let failed = || {
        occ(
            "h2",
            parsed(r#"{"tokens":{"input":100}}"#),
            W,
            ("W1", "S1", "R1"),
            Failed,
        )
    };
    let d1 = [r1(Complete), decl(W, ("W1", "S9", "R9"), Partial)];
    let d2 = [decl(W, ("W1", "S9", "R9"), Partial), r1(Complete)];
    let base = run(
        &build(vec![ok(), failed()]),
        RunInventory::Declared(&d1),
        Selection::Both,
    );
    for (set, d) in [
        (build(vec![failed(), ok()]), &d1),
        (build(vec![ok(), failed()]), &d2),
        (build(vec![failed(), ok()]), &d2),
    ] {
        assert_eq!(run(&set, RunInventory::Declared(d), Selection::Both), base);
    }

    // S18: occurrence insertion order and relation insertion order.
    let s18 = |order: [usize; 3], rel_order: [usize; 4]| {
        let make = |i: usize| match i {
            0 => win("h1", r#"{"tokens":{"input":100}}"#),
            1 => win("h2", r#"{"tokens":{"input":100}}"#),
            _ => occ(
                "h3",
                parsed(r#"{"tokens":{"input":100}}"#),
                L,
                ("L1", "S2", "RL"),
                Complete,
            ),
        };
        let mut set = build(order.iter().map(|i| make(*i)).collect());
        let rels = [
            ("h1", "h2", MatchingClaimedIdentifiers),
            ("h2", "h1", MatchingClaimedIdentifiers),
            ("h1", "h2", DifferentClaimedIdentifiers),
            ("h2", "h3", PossibleReplayOrCopy),
        ];
        for i in rel_order {
            let (f, t, h) = rels[i];
            relate(&mut set, f, t, h);
        }
        let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Complete)];
        run(&set, RunInventory::Declared(&d), Selection::Both)
    };
    let base = s18([0, 1, 2], [0, 1, 2, 3]);
    assert_eq!(s18([2, 0, 1], [3, 2, 1, 0]), base);
    assert_eq!(s18([1, 2, 0], [1, 3, 0, 2]), base);

    // S20: declaration order.
    let set = build(vec![occ(
        "h1",
        parsed(r#"{"tokens":{"input":10}}"#),
        L,
        ("L1", "S1", "RL1"),
        Complete,
    )]);
    let a = [
        decl(L, ("L1", "S1", "RL1"), Complete),
        decl(L, ("L2", "S2", "RL2"), NotCollected),
    ];
    let b = [a[1].clone(), a[0].clone()];
    assert_eq!(
        run(&set, RunInventory::Declared(&a), Selection::Both),
        run(&set, RunInventory::Declared(&b), Selection::Both)
    );
}

// ---- T3 relation isolation ----------------------------------------------------

fn s18_set(relations: &[(&str, &str, abf_identity_relations::UnverifiedHint)]) -> SyntheticSet {
    let mut set = build(vec![
        win("h1", r#"{"tokens":{"input":100}}"#),
        win("h2", r#"{"tokens":{"input":100}}"#),
        occ(
            "h3",
            parsed(r#"{"tokens":{"input":100}}"#),
            L,
            ("L1", "S2", "RL"),
            Complete,
        ),
    ]);
    for (f, t, h) in relations {
        relate(&mut set, f, t, *h);
    }
    set
}

#[test]
fn t3_relation_changes_affect_only_relations_and_excluded_relation_entries() {
    let original = [
        ("h1", "h2", MatchingClaimedIdentifiers),
        ("h2", "h1", MatchingClaimedIdentifiers),
        ("h1", "h2", DifferentClaimedIdentifiers),
        ("h2", "h3", PossibleReplayOrCopy),
    ];
    let mutated_hints = [
        ("h1", "h2", Unclassified),
        ("h2", "h1", EqualReportedValues),
        ("h1", "h2", PossibleReplayOrCopy),
        ("h2", "h3", MatchingClaimedIdentifiers),
    ];
    let reversed = [
        ("h2", "h1", MatchingClaimedIdentifiers),
        ("h1", "h2", MatchingClaimedIdentifiers),
        ("h2", "h1", DifferentClaimedIdentifiers),
        ("h3", "h2", PossibleReplayOrCopy),
    ];
    let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Complete)];
    for selection in [
        Selection::Both,
        Selection::WindowsOnly,
        Selection::SelectedWslOnly,
    ] {
        let base = run(&s18_set(&original), RunInventory::Declared(&d), selection);
        for variant in [&[][..], &mutated_hints[..], &reversed[..], &original[..1]] {
            let p = run(&s18_set(variant), RunInventory::Declared(&d), selection);
            assert_eq!(without_relation_outputs(&p, &base), base);
            assert_eq!(p.excluded.0.occurrences, base.excluded.0.occurrences);
            assert_eq!(p.excluded.0.declarations, base.excluded.0.declarations);
        }
    }

    // S19: only excluded.relation_entries changes (1 -> 0) outside `relations`.
    let s19 = |with: bool| {
        let mut set = build(vec![
            win("h1", r#"{"tokens":{"input":100}}"#),
            occ(
                "h2",
                parsed(r#"{"tokens":{"input":7}}"#),
                L,
                ("L1", "S2", "RL"),
                Complete,
            ),
        ]);
        if with {
            relate(&mut set, "h1", "h2", Unclassified);
        }
        let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Complete)];
        run(&set, RunInventory::Declared(&d), Selection::WindowsOnly)
    };
    let (with, without) = (s19(true), s19(false));
    assert_eq!(with.excluded.0.relation_entries, 1);
    assert_eq!(without.excluded.0.relation_entries, 0);
    let mut normalized = with;
    normalized.relations = without.relations;
    assert_ne!(normalized, without);
    normalized.excluded.0.relation_entries = 0;
    assert_eq!(normalized, without);
}

// ---- T4 zero versus unknown ---------------------------------------------------

#[test]
fn t4_valid_zero_contributes_and_each_non_valid_state_only_bins() {
    let p = run(
        &build(vec![win("h1", r#"{"tokens":{"output":0}}"#)]),
        RunInventory::Undeclared,
        Selection::WindowsOnly,
    );
    let c = &sel(&p.windows).components.output;
    assert_eq!(c.value, ReportedClaimSum(0));
    assert_eq!(
        (c.evidence.contributing, c.evidence.field_states.valid_zero),
        (1, 1)
    );
    assert!(!c.evidence.qualifications.contains(MissingClaims));

    let cases: [(&str, FieldStateCounts); 5] = [
        (
            r#"{"tokens":{"input":1}}"#,
            FieldStateCounts { absent: 1, ..Z },
        ),
        (
            r#"{"tokens":{"input":1,"output":null}}"#,
            FieldStateCounts { null: 1, ..Z },
        ),
        (
            r#"{"tokens":{"input":1,"output":"7"}}"#,
            FieldStateCounts { wrong_type: 1, ..Z },
        ),
        (
            r#"{"tokens":{"input":1,"output":-1}}"#,
            FieldStateCounts {
                out_of_range: 1,
                ..Z
            },
        ),
        (
            r#"{"tokens":{"input":1,"output":18446744073709551616}}"#,
            FieldStateCounts {
                out_of_range: 1,
                ..Z
            },
        ),
    ];
    for (frame, bins) in cases {
        let p = run(
            &build(vec![win("h1", frame)]),
            RunInventory::Undeclared,
            Selection::WindowsOnly,
        );
        let c = &sel(&p.windows).components.output;
        assert_eq!(c.value, NoValidClaims, "{frame}");
        assert_eq!(c.evidence.contributing, 0);
        assert_eq!(c.evidence.field_states, bins);
        assert!(c.evidence.qualifications.contains(MissingClaims));
    }
    // Unsupported: cache children without a cache container.
    let c = &sel(&p.windows).components.cache_write;
    assert_eq!(c.value, NoValidClaims);
    assert_eq!(
        c.evidence.field_states,
        FieldStateCounts {
            unsupported: 1,
            ..Z
        }
    );
    assert!(c.evidence.qualifications.contains(MissingClaims));
}

// ---- T5 / T5b component independence ------------------------------------------

fn single(frame: &str) -> Projection {
    let d = [r1(Complete)];
    run(
        &build(vec![win("h1", frame)]),
        RunInventory::Declared(&d),
        Selection::WindowsOnly,
    )
}

#[test]
fn t5_mutating_one_component_changes_only_that_component() {
    let base = single(r#"{"tokens":{"input":5,"output":3,"total":9,"cache":{"read":1}}}"#);
    let changed_input = single(r#"{"tokens":{"input":7,"output":3,"total":9,"cache":{"read":1}}}"#);
    let output_absent = single(r#"{"tokens":{"input":5,"total":9,"cache":{"read":1}}}"#);
    let (b, ci, oa) = (
        &sel(&base.windows).components,
        &sel(&changed_input.windows).components,
        &sel(&output_absent.windows).components,
    );
    assert_eq!(ci.input.value, ReportedClaimSum(7));
    assert_eq!(
        (b.output, b.total, b.cache_read, b.cache_write),
        (ci.output, ci.total, ci.cache_read, ci.cache_write)
    );
    assert_eq!(oa.output.value, NoValidClaims);
    assert_eq!(
        oa.output.evidence.field_states,
        FieldStateCounts { absent: 1, ..Z }
    );
    assert_eq!(
        (b.input, b.total, b.cache_read, b.cache_write),
        (oa.input, oa.total, oa.cache_read, oa.cache_write)
    );
    // B and B' unchanged: eligibility class preserved.
    for p in [&changed_input, &output_absent] {
        assert_eq!(sel(&p.windows).outcomes, sel(&base.windows).outcomes);
        assert_eq!(sel(&p.windows).records, sel(&base.windows).records);
    }
    // A status change (Accepted -> Qualified) changes shared flags only per trigger.
    let qualified = single(r#"{"tokens":{"input":5,"output":null,"total":9,"cache":{"read":1}}}"#);
    let qc = &sel(&qualified.windows).components;
    assert_eq!(qc.input.value, ReportedClaimSum(5));
    assert_eq!(
        qc.input.evidence.qualifications.bits(),
        q(&[FromQualifiedRecords])
    );
    assert_eq!(
        qc.cache_write.evidence.qualifications.bits(),
        q(&[MissingClaims])
    );
    assert_eq!(
        qc.output.evidence.field_states,
        FieldStateCounts { null: 1, ..Z }
    );
}

#[test]
fn t5b_eligibility_change_has_explicit_shared_effects() {
    let frame = r#"{"tokens":{"input":5,"output":3}}"#;
    let mut o = observation(frame);
    o.total = Field::Null; // C6: Null without ExplicitNull.
    let d = [r1(Complete)];
    let base = run(
        &build(vec![win("h1", frame), win("h2", frame)]),
        RunInventory::Declared(&d),
        Selection::WindowsOnly,
    );
    let mutated = run(
        &build(vec![
            win("h1", frame),
            occ(
                "h2",
                ParseResult::Observation(o),
                W,
                ("W1", "S1", "R1"),
                Complete,
            ),
        ]),
        RunInventory::Declared(&d),
        Selection::WindowsOnly,
    );
    let (b, m) = (sel(&base.windows), sel(&mutated.windows));
    assert_eq!(b.outcomes, m.outcomes);
    assert_eq!((b.records.0.eligible, b.records.0.contradictory), (2, 0));
    assert_eq!((m.records.0.eligible, m.records.0.contradictory), (1, 1));
    for (bc, mc) in all(&b.components).into_iter().zip(all(&m.components)) {
        assert_eq!(bc.evidence.eligible_records, 2);
        assert_eq!(mc.evidence.eligible_records, 1);
        assert!(
            mc.evidence
                .qualifications
                .contains(ContradictoryRecordsExcluded)
        );
        assert!(
            !bc.evidence
                .qualifications
                .contains(ContradictoryRecordsExcluded)
        );
    }
    assert_eq!(m.components.input.value, ReportedClaimSum(5));
    assert_eq!(b.components.input.value, ReportedClaimSum(10));
}

// ---- T6 ceiling sum -----------------------------------------------------------

#[test]
fn t6_u64_max_at_the_128_occurrence_ceiling_is_exact() {
    let frame = r#"{"tokens":{"input":18446744073709551615}}"#;
    let set = build((0..128).map(|i| win(&format!("h{i}"), frame)).collect());
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    let c = &sel(&p.windows).components.input;
    assert_eq!(c.value, ReportedClaimSum(2_361_183_241_434_822_606_720));
    assert_eq!(c.evidence.contributing, 128);
    assert_eq!(
        p.computer.components.input.value,
        ReportedClaimSum(2_361_183_241_434_822_606_720)
    );
}

// ---- T7 contradiction screen ----------------------------------------------------

fn constructed_contradictions() -> Vec<(ParseResult, Status)> {
    let accepted = || observation(r#"{"tokens":{"input":1}}"#);
    let mut v = Vec::new();
    let ParseResult::Observation(c1) = scenarios_s14b() else {
        unreachable!()
    };
    let mut c5 = c1.clone();
    c5.status = Status::Qualified;
    let mut c2 = accepted();
    c2.input = Field::Absent;
    let mut c3 = observation(r#"{"tokens":{"input":100,"output":null}}"#);
    c3.status = Status::Accepted;
    let mut c4 = accepted();
    c4.status = Status::Qualified;
    let mut c6 = accepted();
    c6.session_id = Field::Null;
    let mut c7 = accepted();
    c7.cache_write = Field::Valid(3);
    for o in [c1, c2, c3, c4, c5, c6, c7] {
        let status = o.status;
        v.push((ParseResult::Observation(o), status));
    }
    v
}

fn scenarios_s14b() -> ParseResult {
    let mut o = observation(r#"{"time":{"created":1700000000}}"#);
    o.tokens = Field::Valid(());
    o.input = Field::Valid(100);
    o.output = Field::Absent;
    o.total = Field::Absent;
    o.cache = Field::Absent;
    ParseResult::Observation(o)
}

#[test]
fn t7_constructed_contradictions_are_excluded_but_keep_original_outcome() {
    for (result, status) in constructed_contradictions() {
        let d = [r1(Complete)];
        let p = run(
            &build(vec![
                win("h0", r#"{"tokens":{"input":4}}"#),
                occ("h1", result, W, ("W1", "S1", "R1"), Complete),
            ]),
            RunInventory::Declared(&d),
            Selection::WindowsOnly,
        );
        let w = sel(&p.windows);
        let expected = match status {
            Status::Accepted => outcomes(2, 0, 0, 0),
            Status::Qualified => outcomes(1, 1, 0, 0),
            Status::Incomplete => outcomes(1, 0, 1, 0),
        };
        assert_eq!(w.outcomes.0, expected);
        assert_eq!((w.records.0.eligible, w.records.0.contradictory), (1, 1));
        for c in all(&w.components) {
            assert_eq!(c.evidence.eligible_records, 1);
            assert!(
                c.evidence
                    .qualifications
                    .contains(ContradictoryRecordsExcluded)
            );
        }
        assert_eq!(w.components.input.value, ReportedClaimSum(4));
    }
}

#[test]
fn t7_parser_produced_observations_are_never_contradictory() {
    let frames = [
        r#"{"tokens":{"input":0}}"#,
        r#"{"tokens":{"total":500}}"#,
        r#"{"tokens":{"input":100,"output":null}}"#,
        r#"{"tokens":{"input":1,"output":"x","cache":{"read":2,"write":-1}}}"#,
        r#"{"tokens":null}"#,
        r#"{"tokens":[1]}"#,
        r#"{"tokens":{"cache":null,"input":4}}"#,
        r#"{"tokens":{"cache":{"read":0,"write":0,"x":1}}}"#,
        r#"{"tokens":{"reasoning":5}}"#,
        r#"{"time":{"created":1700000000}}"#,
        r#"{"time":"bad","tokens":{"input":2}}"#,
        r#"{"id":null,"sessionID":7,"modelID":"m","tokens":{"input":2}}"#,
        r#"{}"#,
    ];
    let set = build(
        frames
            .iter()
            .enumerate()
            .map(|(i, f)| win(&format!("h{i}"), f))
            .collect(),
    );
    let p = run(&set, RunInventory::Undeclared, Selection::WindowsOnly);
    assert_eq!(sel(&p.windows).records.0.contradictory, 0);
}

// ---- T8 outcome conservation ----------------------------------------------------

#[test]
fn t8_outcomes_sum_to_stored_occurrences_in_every_scope_and_computer() {
    let ParseResult::Observation(mut c4) = parsed(r#"{"tokens":{"input":1}}"#) else {
        unreachable!()
    };
    c4.status = Status::Qualified;
    let set = build(vec![
        win("a", r#"{"tokens":{"input":1}}"#),
        win("b", r#"{"tokens":{"input":1,"output":null}}"#),
        win("c", r#"{}"#),
        occ("d", rejected(), W, ("W1", "S1", "R1"), Complete),
        occ(
            "e",
            ParseResult::Observation(c4),
            L,
            ("L1", "S1", "R1"),
            Partial,
        ),
        occ("f", rejected(), L, ("L1", "S1", "R1"), Partial),
    ]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    assert_eq!(sel(&p.windows).outcomes.0, outcomes(1, 1, 1, 1));
    assert_eq!(sel(&p.selected_wsl).outcomes.0, outcomes(0, 1, 0, 1));
    assert_eq!(p.computer.outcomes.0, outcomes(1, 2, 1, 2));
    assert_eq!(p.computer.stored_occurrences, Exact(6));
    assert_eq!(p.computer.records.0.contradictory, 1);
}

// ---- T9 coverage table ----------------------------------------------------------

fn coverage(set: &SyntheticSet, inventory: RunInventory<'_>) -> RunSummary {
    sel(&run(set, inventory, Selection::WindowsOnly).windows).runs
}

#[test]
fn t9_coverage_rows_and_raw_versus_resolved() {
    // Row 1 via each trigger.
    let r = coverage(
        &build(vec![
            win("h1", r#"{"tokens":{"input":1}}"#),
            occ(
                "h2",
                parsed(r#"{"tokens":{"input":1}}"#),
                W,
                ("W1", "S1", "R1"),
                Failed,
            ),
        ]),
        RunInventory::Declared(&[r1(Complete)]),
    );
    assert_eq!(r.coverage, CoverageState::Inconsistent);
    assert!(r.qualifications.contains(FailedRun) && r.qualifications.contains(RunStatusConflict));
    let r = coverage(
        &build(vec![occ(
            "h1",
            parsed(r#"{"tokens":{"input":1}}"#),
            W,
            ("W1", "S1", "R1"),
            NotCollected,
        )]),
        RunInventory::Undeclared,
    );
    assert_eq!(r.coverage, CoverageState::Inconsistent);
    assert_eq!(r.resolved, status(1, 0, 0, 0, 0));
    assert_eq!(
        r.qualifications.bits(),
        q(&[
            NotCollectedRun,
            RecordWithoutReportedTraversal,
            RunInventoryUndeclared
        ])
    );
    // Row 1 beats row 3: the only key is Complete but undeclared.
    let other = [decl(W, ("W1", "S1", "R2"), Complete)];
    let r = coverage(
        &build(vec![win("h1", r#"{"tokens":{"input":1}}"#)]),
        RunInventory::Declared(&other),
    );
    assert_eq!(r.coverage, CoverageState::Inconsistent);
    assert_eq!((r.run_keys, r.undeclared_record_keys), (2, 1));
    assert_eq!(r.resolved, status(0, 2, 0, 0, 0));
    assert_eq!(r.qualifications.bits(), q(&[UndeclaredRunInRecords]));
    // Row 2.
    assert_eq!(
        coverage(&build(vec![]), RunInventory::Undeclared).coverage,
        CoverageState::NoEvidence
    );
    // Row 3.
    let r = coverage(&build(vec![win("h1", r#"{}"#)]), RunInventory::Undeclared);
    assert_eq!(r.coverage, CoverageState::ReportedComplete);
    // Row 4: only NotCollected/Unavailable keys.
    let d = [r1(NotCollected), decl(W, ("W2", "S2", "R2"), Unavailable)];
    let r = coverage(&build(vec![]), RunInventory::Declared(&d));
    assert_eq!(r.coverage, CoverageState::NoTraversalReported);
    assert_eq!(r.resolved, status(1, 0, 0, 0, 1));
    // Row 5: Complete with Partial, Failed alone, Complete with NotCollected.
    for d in [
        vec![r1(Complete), decl(W, ("W1", "S2", "R2"), Partial)],
        vec![r1(Failed)],
        vec![r1(Complete), decl(W, ("W1", "S2", "R2"), NotCollected)],
    ] {
        let r = coverage(&build(vec![]), RunInventory::Declared(&d));
        assert_eq!(r.coverage, CoverageState::ReportedIncomplete);
        assert_eq!(r.conflicting_keys, 0);
    }
}

// ---- T10 Computer table via public input, both kind orders ---------------------

#[derive(Clone, Copy)]
enum Side {
    Sum(u64),
    NoValid,
    Indeterminate,
}

fn computer_case(kind_a: abf_observation::EnvironmentKind, a: Side, b: Side) -> Projection {
    let kind_b = if kind_a == W { L } else { W };
    let mut occs = Vec::new();
    let mut decls = Vec::new();
    for (i, (kind, side)) in [(kind_a, a), (kind_b, b)].into_iter().enumerate() {
        let key = (if kind == W { "W1" } else { "L1" }, "S1", "R1");
        match side {
            Side::Sum(n) => {
                let frame = format!(r#"{{"tokens":{{"input":{n}}}}}"#);
                occs.push(occ(&format!("h{i}"), parsed(&frame), kind, key, Complete));
                decls.push(decl(kind, key, Complete));
            }
            Side::NoValid => decls.push(decl(kind, key, Complete)),
            Side::Indeterminate => {}
        }
    }
    run(
        &build(occs),
        RunInventory::Declared(&decls),
        Selection::Both,
    )
}

#[test]
fn t10_computer_value_table_every_reachable_row_both_kind_orders() {
    use Side::*;
    let rows = [
        (
            Sum(3),
            Sum(4),
            ReportedClaimSum(7),
            2,
            q(&[CrossEnvironmentSum]),
        ),
        (
            Sum(3),
            NoValid,
            ReportedClaimSum(3),
            1,
            q(&[EnvironmentWithoutClaims]),
        ),
        (
            Sum(3),
            Indeterminate,
            ReportedClaimSum(3),
            1,
            q(&[EnvironmentIndeterminate]),
        ),
        (NoValid, NoValid, NoValidClaims, 0, 0),
        (
            NoValid,
            Indeterminate,
            NoValidClaims,
            0,
            q(&[EnvironmentIndeterminate]),
        ),
        (
            Indeterminate,
            Indeterminate,
            indeterminate(AllEnvironmentsIndeterminate),
            0,
            q(&[EnvironmentIndeterminate]),
        ),
        (
            Sum(0),
            NoValid,
            ReportedClaimSum(0),
            1,
            q(&[EnvironmentWithoutClaims]),
        ),
    ];
    for (a, b, value, contributing, flags) in rows {
        let wl = computer_case(W, a, b);
        let lw = computer_case(L, a, b);
        for p in [&wl, &lw] {
            let c = &p.computer.components.input;
            assert_eq!(c.value, value);
            assert_eq!(c.evidence.contributing, contributing);
            assert_eq!(c.evidence.eligible_records, contributing);
            assert_eq!(c.evidence.qualifications.bits(), flags);
        }
        assert_eq!(wl.computer.components, lw.computer.components);
    }
}

// ---- T11 rejected records ------------------------------------------------------

#[test]
fn t11a_rejected_record_preserves_existing_eligible_sum() {
    let d = [r1(Complete)];
    let before = run(
        &build(vec![win("h1", r#"{"tokens":{"input":100}}"#)]),
        RunInventory::Declared(&d),
        Selection::Both,
    );
    let after = run(
        &build(vec![
            win("h1", r#"{"tokens":{"input":100}}"#),
            occ("h2", rejected(), W, ("W1", "S1", "R1"), Complete),
        ]),
        RunInventory::Declared(&d),
        Selection::Both,
    );
    let (b, a) = (sel(&before.windows), sel(&after.windows));
    check(
        &a.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        FieldStateCounts {
            valid_positive: 1,
            ..Z
        },
        &[RejectedRecordsPresent],
    );
    assert_eq!((b.outcomes.0.rejected, a.outcomes.0.rejected), (0, 1));
    for (bc, ac) in all(&b.components).into_iter().zip(all(&a.components)) {
        assert_eq!(bc.value, ac.value);
        assert_eq!(
            (
                bc.evidence.contributing,
                bc.evidence.zero_valued,
                bc.evidence.field_states
            ),
            (
                ac.evidence.contributing,
                ac.evidence.zero_valued,
                ac.evidence.field_states
            )
        );
    }
}

#[test]
fn t11b_rejected_complete_record_turns_indeterminate_into_no_valid_claims() {
    let empty = run(
        &build(vec![]),
        RunInventory::Undeclared,
        Selection::WindowsOnly,
    );
    let w = sel(&empty.windows);
    assert_eq!(w.runs.coverage, CoverageState::NoEvidence);
    for c in all(&w.components) {
        check(
            c,
            indeterminate(NoRunEvidence),
            (0, 0, 0),
            Z,
            &[RunInventoryUndeclared],
        );
    }
    let p = run(
        &build(vec![occ("h1", rejected(), W, ("W1", "S1", "R1"), Complete)]),
        RunInventory::Undeclared,
        Selection::WindowsOnly,
    );
    let w = sel(&p.windows);
    assert_eq!(w.outcomes.0.rejected, 1);
    assert_eq!(
        (w.runs.run_keys, w.runs.resolved),
        (1, status(0, 1, 0, 0, 0))
    );
    assert_eq!(w.runs.raw_record_claims, status(0, 1, 0, 0, 0));
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    for c in all(&w.components) {
        check(
            c,
            NoValidClaims,
            (0, 0, 0),
            Z,
            &[RunInventoryUndeclared, RejectedRecordsPresent],
        );
    }
    assert_eq!(p.computer.components, w.components);
    assert!(!has_numeric(&p));
}

// ---- T12 empty set never numeric -----------------------------------------------

#[test]
fn t12_empty_set_never_yields_a_numeric_value() {
    let set = build(vec![]);
    let mut inventories: Vec<Vec<RunDeclaration>> = vec![vec![]];
    for kind in [W, L] {
        for s in [NotCollected, Complete, Partial, Failed, Unavailable] {
            inventories.push(vec![decl(kind, ("E1", "S1", "R1"), s)]);
        }
    }
    for selection in [
        Selection::WindowsOnly,
        Selection::SelectedWslOnly,
        Selection::Both,
    ] {
        assert!(!has_numeric(&run(
            &set,
            RunInventory::Undeclared,
            selection
        )));
        for d in &inventories {
            assert!(!has_numeric(&run(
                &set,
                RunInventory::Declared(d),
                selection
            )));
        }
    }
}

// ---- T13 input validation ------------------------------------------------------

#[test]
fn t13_invalid_inventories_return_fixed_codes() {
    let set = build(vec![]);
    for second in [Complete, Failed] {
        let d = [
            r1(Complete),
            decl(L, ("W1", "S1", "R1"), Partial),
            r1(second),
        ];
        assert_eq!(
            project(&set, RunInventory::Declared(&d), Selection::Both),
            Err(Code::DuplicateRunDeclaration)
        );
        // Unselected duplicates are still invalid input.
        assert_eq!(
            project(&set, RunInventory::Declared(&d), Selection::SelectedWslOnly),
            Err(Code::DuplicateRunDeclaration)
        );
    }
    let many: Vec<_> = (0..129)
        .map(|i| decl(W, ("W1", "S1", &format!("R{i}")), Complete))
        .collect();
    assert_eq!(
        project(&set, RunInventory::Declared(&many), Selection::Both),
        Err(Code::RunInventoryCapacity)
    );
    let p = run(
        &set,
        RunInventory::Declared(&many[..MAX_RUN_DECLARATIONS]),
        Selection::Both,
    );
    let w = sel(&p.windows);
    assert_eq!(w.runs.run_keys, 128);
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    // Capacity is checked before duplicates.
    let mut dup = many.clone();
    dup[1] = dup[0].clone();
    assert_eq!(
        project(&set, RunInventory::Declared(&dup), Selection::Both),
        Err(Code::RunInventoryCapacity)
    );
}

// ---- T14 relation partition ----------------------------------------------------

#[test]
fn t14_partition_is_disjoint_and_cross_entries_count_once() {
    let mut set = build(vec![
        win("a", r#"{"tokens":{"input":1}}"#),
        win("b", r#"{"tokens":{"input":1}}"#),
        occ(
            "x",
            parsed(r#"{"tokens":{"input":1}}"#),
            L,
            ("L1", "S1", "R1"),
            Complete,
        ),
        occ(
            "y",
            parsed(r#"{"tokens":{"input":1}}"#),
            L,
            ("L1", "S1", "R1"),
            Complete,
        ),
    ]);
    relate(&mut set, "a", "b", EqualReportedValues);
    relate(&mut set, "x", "y", EqualReportedValues);
    relate(&mut set, "y", "x", Unclassified);
    relate(&mut set, "a", "x", MatchingClaimedIdentifiers);
    for (selection, part, excluded, wi, li, referenced) in [
        (Selection::Both, (1, 2, 1, 4), 0, 2, 3, 4),
        (Selection::WindowsOnly, (1, 0, 0, 1), 3, 1, 0, 2),
        (Selection::SelectedWslOnly, (0, 2, 0, 2), 2, 0, 2, 2),
    ] {
        let g = run(&set, RunInventory::Undeclared, selection).relations;
        assert_eq!(
            g.selected,
            AssertionPartition {
                windows_within: part.0,
                selected_wsl_within: part.1,
                cross_environment: part.2,
                total: part.3
            }
        );
        assert_eq!(g.set_global_entries, Exact(4));
        assert_eq!(g.excluded_entries, Exact(excluded));
        assert_eq!(
            (
                g.windows_incident_entries.0,
                g.selected_wsl_incident_entries.0
            ),
            (wi, li)
        );
        assert_eq!(g.referenced_occurrences, Exact(referenced));
        assert_eq!(
            g.verification,
            abf_identity_relations::Verification::Unverified
        );
    }
}

// ---- T15 unknown components ----------------------------------------------------

#[test]
fn t15_unknown_components_flag_is_independent_of_eligibility() {
    // Incomplete, unsupported-only (S14c).
    let p = single(r#"{"tokens":{"reasoning":5}}"#);
    assert!(all(&sel(&p.windows).components).iter().all(|c| {
        c.evidence
            .qualifications
            .contains(UnknownComponentsDiscarded)
    }));
    // Contradictory: Qualified-by-parser record forced to Accepted (C3).
    let mut o = observation(r#"{"tokens":{"input":1,"reasoning":5}}"#);
    assert_eq!(o.status, Status::Qualified);
    o.status = Status::Accepted;
    let d = [r1(Complete)];
    let p = run(
        &build(vec![occ(
            "h1",
            ParseResult::Observation(o),
            W,
            ("W1", "S1", "R1"),
            Complete,
        )]),
        RunInventory::Declared(&d),
        Selection::WindowsOnly,
    );
    let w = sel(&p.windows);
    assert_eq!(
        (
            w.records.0.contradictory,
            w.records.0.unsupported_component_observations
        ),
        (1, 1)
    );
    for c in all(&w.components) {
        check(
            c,
            NoValidClaims,
            (0, 0, 0),
            Z,
            &[UnknownComponentsDiscarded, ContradictoryRecordsExcluded],
        );
    }
    // A parser rejection never triggers it.
    let p = run(
        &build(vec![occ("h1", rejected(), W, ("W1", "S1", "R1"), Complete)]),
        RunInventory::Undeclared,
        Selection::WindowsOnly,
    );
    assert!(
        !sel(&p.windows)
            .components
            .input
            .evidence
            .qualifications
            .contains(UnknownComponentsDiscarded)
    );
}

// ---- T16 reference multiplicity -------------------------------------------------

#[test]
fn t16_ref_multiplicity_includes_zero_record_declarations() {
    let set = build(vec![occ(
        "h1",
        parsed(r#"{"tokens":{"input":10}}"#),
        L,
        ("L1", "S1", "RL1"),
        Complete,
    )]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    assert_eq!(sel(&p.selected_wsl).environment_refs, RefMultiplicity::One);
    let d = [
        decl(L, ("L1", "S1", "RL1"), Complete),
        decl(L, ("L2", "S2", "RL2"), NotCollected),
    ];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    assert_eq!(
        sel(&p.selected_wsl).environment_refs,
        RefMultiplicity::Multiple(2)
    );
    assert_eq!(sel(&p.selected_wsl).stored_occurrences, Exact(1));
}

// ---- T17 privacy ---------------------------------------------------------------

#[test]
fn t17_debug_output_contains_no_input_derived_text() {
    let frame = r#"{"id":"zzsecretmsg","sessionID":"zzsecretsession","modelID":"zzsecretmodel","tokens":{"input":1}}"#;
    let mut set = build(vec![
        occ(
            "zzsecrethandle1",
            parsed(frame),
            W,
            ("zzsecretenv", "zzsecretsrc", "zzsecretrun"),
            Complete,
        ),
        occ(
            "zzsecrethandle2",
            parsed(frame),
            L,
            ("zzsecretenv", "zzsecretsrc", "zzsecretrun"),
            Partial,
        ),
    ]);
    relate(
        &mut set,
        "zzsecrethandle1",
        "zzsecrethandle2",
        MatchingClaimedIdentifiers,
    );
    let d = [decl(
        W,
        ("zzsecretenv", "zzsecretsrc", "zzsecretrun"),
        Complete,
    )];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let text = format!(
        "{p:?} {:?} {:?}",
        Code::DuplicateRunDeclaration,
        RunInventory::Declared(&d)
    );
    assert!(!text.contains("zzsecret"), "input text leaked");
    let dup = [d[0].clone(), d[0].clone()];
    let err = project(&set, RunInventory::Declared(&dup), Selection::Both).unwrap_err();
    assert!(!format!("{err:?}").contains("zzsecret"));
}

// ---- T18 determinism -----------------------------------------------------------

#[test]
fn t18_repeated_calls_are_identical() {
    let mut set = s18_set(&[("h1", "h3", PossibleReplayOrCopy)]);
    relate(&mut set, "h3", "h1", PossibleReplayOrCopy);
    let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Partial)];
    let first = run(&set, RunInventory::Declared(&d), Selection::Both);
    for _ in 0..8 {
        let again = run(&set, RunInventory::Declared(&d), Selection::Both);
        assert_eq!(again, first);
        assert_eq!(format!("{again:?}"), format!("{first:?}"));
    }
}

// ---- T19 type-level ------------------------------------------------------------

#[test]
fn t19_physical_event_and_usage_types_have_no_numeric_variant() {
    // Exhaustive single-arm matches: adding any variant would fail to compile.
    let p = run(
        &build(vec![win("h1", r#"{"tokens":{"input":1}}"#)]),
        RunInventory::Undeclared,
        Selection::Both,
    );
    for (e, u) in [
        (sel(&p.windows).physical_events, sel(&p.windows).usage),
        (p.computer.physical_events, p.computer.usage),
    ] {
        match e {
            PhysicalEventCount::Unresolved => {}
        }
        match u {
            UsageTotal::Unsupported => {}
        }
    }
    match sel(&p.windows).components.input.evidence.identity {
        IdentityBasis::Unresolved => {}
    }
    match sel(&p.windows).runs.traversal {
        TraversalBasis::CallerReported => {}
    }
    assert_eq!(size_of::<PhysicalEventCount>(), 0);
    assert_eq!(size_of::<UsageTotal>(), 0);
}
