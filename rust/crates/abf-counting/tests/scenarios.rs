//! T1: contract §9 decision matrix S01–S20, with independent expected values.
mod support;

use abf_counting::*;
use abf_identity_relations::UnverifiedHint::*;
use abf_parser::{Field, ParseResult, Status};
use support::*;

use ComponentValue::{NoValidClaims, ReportedClaimSum};
use IndeterminateReason::*;

const ACCEPTED_1: OutcomeCounts = OutcomeCounts {
    accepted: 1,
    qualified: 0,
    incomplete: 0,
    rejected: 0,
};

fn absent(n: u32) -> FieldStateCounts {
    FieldStateCounts { absent: n, ..Z }
}
fn unsupported(n: u32) -> FieldStateCounts {
    FieldStateCounts {
        unsupported: n,
        ..Z
    }
}
fn positive(n: u32) -> FieldStateCounts {
    FieldStateCounts {
        valid_positive: n,
        ..Z
    }
}

/// Default single-record Windows case (common defaults of §9).
fn default_case(frame: &str) -> Projection {
    let set = build(vec![win("h1", frame)]);
    let d = [r1(Complete)];
    run(&set, RunInventory::Declared(&d), Selection::Both)
}

fn assert_default_reported_complete(w: &EnvironmentResult) {
    assert_eq!(w.environment_refs, RefMultiplicity::One);
    assert_eq!(w.runs.inventory, InventoryBasis::Declared);
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    assert_eq!(w.runs.run_keys, 1);
    assert_eq!(w.runs.conflicting_keys, 0);
    assert_eq!(w.runs.resolved, status(0, 1, 0, 0, 0));
    assert!(w.runs.qualifications.is_empty());
}

// ABF106 (ABF105-F01): independently specified T1 expectations. Every value
// below is written as a literal from contract §9 / §8.3-§8.5; none is computed
// from the projection under test.

/// Expected scope-level facts (A, B, B', refs and run summary F) for one scope.
struct ScopeExp<'a> {
    refs: RefMultiplicity,
    stored: u32,
    outcomes: OutcomeCounts,
    records: RecordDiagnostics,
    inventory: InventoryBasis,
    coverage: CoverageState,
    run_keys: u32,
    conflicting: u32,
    resolved: StatusCounts,
    raw_records: StatusCounts,
    raw_declared: StatusCounts,
    undeclared_keys: u32,
    run_flags: &'a [Qualification],
}

fn rec(
    eligible: u32,
    contradictory: u32,
    unsupported: u32,
    without_traversal: u32,
) -> RecordDiagnostics {
    RecordDiagnostics {
        eligible,
        contradictory,
        unsupported_component_observations: unsupported,
        records_without_reported_traversal: without_traversal,
    }
}

/// §9 common default: one Accepted Windows record, run R1 Complete, declared.
fn win_one() -> ScopeExp<'static> {
    ScopeExp {
        refs: RefMultiplicity::One,
        stored: 1,
        outcomes: ACCEPTED_1,
        records: rec(1, 0, 0, 0),
        inventory: InventoryBasis::Declared,
        coverage: CoverageState::ReportedComplete,
        run_keys: 1,
        conflicting: 0,
        resolved: status(0, 1, 0, 0, 0),
        raw_records: status(0, 1, 0, 0, 0),
        raw_declared: status(0, 1, 0, 0, 0),
        undeclared_keys: 0,
        run_flags: &[],
    }
}

/// A scope with no occurrences and no declarations (§9 default for WSL).
fn empty_exp(undeclared: bool) -> ScopeExp<'static> {
    ScopeExp {
        refs: RefMultiplicity::None,
        stored: 0,
        outcomes: outcomes(0, 0, 0, 0),
        records: rec(0, 0, 0, 0),
        inventory: if undeclared {
            InventoryBasis::Undeclared
        } else {
            InventoryBasis::Declared
        },
        coverage: CoverageState::NoEvidence,
        run_keys: 0,
        conflicting: 0,
        resolved: status(0, 0, 0, 0, 0),
        raw_records: status(0, 0, 0, 0, 0),
        raw_declared: status(0, 0, 0, 0, 0),
        undeclared_keys: 0,
        run_flags: if undeclared {
            &[RunInventoryUndeclared]
        } else {
            &[]
        },
    }
}

fn assert_scope(r: &EnvironmentResult, kind: abf_observation::EnvironmentKind, e: &ScopeExp<'_>) {
    assert_eq!(r.kind, kind, "kind");
    assert_eq!(r.environment_refs, e.refs, "environment_refs");
    assert_eq!(r.stored_occurrences, Exact(e.stored), "stored");
    assert_eq!(r.outcomes, Exact(e.outcomes), "original outcomes");
    assert_eq!(r.records, Exact(e.records), "record diagnostics");
    let s = &r.runs;
    assert_eq!(s.inventory, e.inventory, "inventory");
    assert_eq!(s.traversal, TraversalBasis::CallerReported);
    assert_eq!(s.coverage, e.coverage, "coverage");
    assert_eq!(s.run_keys, e.run_keys, "run_keys");
    assert_eq!(s.conflicting_keys, e.conflicting, "conflicting_keys");
    assert_eq!(s.resolved, e.resolved, "resolved");
    assert_eq!(s.raw_record_claims, e.raw_records, "raw_record_claims");
    assert_eq!(s.raw_declared_claims, e.raw_declared, "raw_declared_claims");
    assert_eq!(
        s.undeclared_record_keys, e.undeclared_keys,
        "undeclared keys"
    );
    assert_eq!(
        s.qualifications.bits(),
        q(e.run_flags),
        "run flags {:?}",
        s.qualifications
    );
    assert_eq!(r.physical_events, PhysicalEventCount::Unresolved);
    assert_eq!(r.usage, UsageTotal::Unsupported);
}

/// An empty scope: run summary and the five `Indeterminate(NoRunEvidence)` rows.
fn assert_empty(r: &EnvironmentResult, kind: abf_observation::EnvironmentKind, undeclared: bool) {
    assert_scope(r, kind, &empty_exp(undeclared));
    let flags: &[Qualification] = if undeclared {
        &[RunInventoryUndeclared]
    } else {
        &[]
    };
    assert_empty_scope(r, flags);
}

/// Computer counts A/B/B' and the selection it was projected with.
fn assert_computer(
    p: &Projection,
    selection: Selection,
    stored: u32,
    original: OutcomeCounts,
    records: RecordDiagnostics,
) {
    let k = &p.computer;
    assert_eq!(k.selection, selection);
    assert_eq!(k.stored_occurrences, Exact(stored));
    assert_eq!(k.outcomes, Exact(original));
    assert_eq!(k.records, Exact(records));
}

/// §9 common defaults for `Selection::Both` with an empty Selected WSL: the
/// Windows scope, the empty WSL scope, Computer counts, no exclusions, no G.
fn assert_defaults(p: &Projection, w: &ScopeExp<'_>) {
    assert_scope(sel(&p.windows), W, w);
    assert_empty(sel(&p.selected_wsl), L, false);
    assert_computer(p, Selection::Both, w.stored, w.outcomes, w.records);
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(p);
}

/// (value, (eligible, contributing, zero_valued), bins, complete flag list)
type Comp<'a> = (
    ComponentValue,
    (u32, u32, u32),
    FieldStateCounts,
    &'a [Qualification],
);

fn sum_comp(v: u128, n: u32, flags: &[Qualification]) -> Comp<'_> {
    (ReportedClaimSum(v), (n, n, 0), positive(n), flags)
}
fn abs_comp(n: u32, flags: &[Qualification]) -> Comp<'_> {
    (NoValidClaims, (n, 0, 0), absent(n), flags)
}
fn uns_comp(n: u32, flags: &[Qualification]) -> Comp<'_> {
    (NoValidClaims, (n, 0, 0), unsupported(n), flags)
}

/// Order: input, output, total, cache_read, cache_write.
fn assert_components(c: &Components<ComponentResult>, e: [Comp<'_>; 5]) {
    for (r, (v, counts, bins, flags)) in all(c).into_iter().zip(e) {
        check(r, v, counts, bins, flags);
    }
}

#[test]
fn s01_explicit_zero_input_is_a_claim_not_missing() {
    let p = default_case(r#"{"tokens":{"input":0}}"#);
    let w = sel(&p.windows);
    assert_eq!(w.stored_occurrences, Exact(1));
    assert_eq!(w.outcomes, Exact(ACCEPTED_1));
    assert_eq!(w.records.0.eligible, 1);
    assert_default_reported_complete(w);
    let c = &w.components;
    let zero = FieldStateCounts { valid_zero: 1, ..Z };
    check(&c.input, ReportedClaimSum(0), (1, 1, 1), zero, &[]);
    check(
        &c.output,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims],
    );
    check(
        &c.total,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims],
    );
    check(
        &c.cache_read,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );
    check(
        &c.cache_write,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );
    assert_empty_scope(sel(&p.selected_wsl), &[]);
    let k = &p.computer.components;
    check(
        &k.input,
        ReportedClaimSum(0),
        (1, 1, 1),
        zero,
        &[EnvironmentIndeterminate],
    );
    check(
        &k.output,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims, EnvironmentIndeterminate],
    );
    assert_computer_is_windows_plus_indeterminate(&p);
    assert_no_relations(&p);
    assert_ne!(c.input.value, NoValidClaims);

    assert_defaults(&p, &win_one());
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            (
                ReportedClaimSum(0),
                (1, 1, 1),
                zero,
                &[EnvironmentIndeterminate],
            ),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s02_positive_input_claim() {
    let p = default_case(r#"{"tokens":{"input":100}}"#);
    let w = sel(&p.windows);
    check(
        &w.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &[],
    );
    assert_computer_is_windows_plus_indeterminate(&p);
    assert_eq!(p.computer.components.input.value, ReportedClaimSum(100));

    // §9 S02 in full: S01's shape with input = 100.
    assert_defaults(&p, &win_one());
    assert_components(
        &w.components,
        [
            sum_comp(100, 1, &[]),
            abs_comp(1, &[MissingClaims]),
            abs_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
        ],
    );
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[EnvironmentIndeterminate]),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s03_equal_claimed_ids_are_two_stored_occurrences() {
    let frame = r#"{"id":"msg-1","tokens":{"input":100}}"#;
    let set = build(vec![win("h1", frame), win("h2", frame)]);
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.stored_occurrences, Exact(2));
    assert_eq!(w.outcomes.0.accepted, 2);
    check(
        &w.components.input,
        ReportedClaimSum(200),
        (2, 2, 0),
        positive(2),
        &[],
    );
    assert_eq!(w.physical_events, PhysicalEventCount::Unresolved);
    assert_no_relations(&p);

    let two = ScopeExp {
        stored: 2,
        outcomes: outcomes(2, 0, 0, 0),
        records: rec(2, 0, 0, 0),
        raw_records: status(0, 2, 0, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &two);
    assert_components(
        &w.components,
        [
            sum_comp(200, 2, &[]),
            abs_comp(2, &[MissingClaims]),
            abs_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
        ],
    );
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(200, 2, &[EnvironmentIndeterminate]),
            abs_comp(2, miss_k),
            abs_comp(2, miss_k),
            uns_comp(2, miss_k),
            uns_comp(2, miss_k),
        ],
    );
}

fn s04(with_entry: bool) -> Projection {
    let mut set = build(vec![
        occ(
            "h1",
            parsed(r#"{"tokens":{"input":100}}"#),
            W,
            ("W1", "S1", "RW"),
            Complete,
        ),
        occ(
            "h2",
            parsed(r#"{"tokens":{"input":100}}"#),
            L,
            ("L1", "S2", "RL"),
            Complete,
        ),
    ]);
    if with_entry {
        relate(&mut set, "h1", "h2", PossibleReplayOrCopy);
    }
    let d = [
        decl(W, ("W1", "S1", "RW"), Complete),
        decl(L, ("L1", "S2", "RL"), Complete),
    ];
    run(&set, RunInventory::Declared(&d), Selection::Both)
}

#[test]
fn s04_cross_environment_hint_never_deduplicates() {
    let p = s04(true);
    for scope in [&p.windows, &p.selected_wsl] {
        check(
            &sel(scope).components.input,
            ReportedClaimSum(100),
            (1, 1, 0),
            positive(1),
            &[],
        );
    }
    check(
        &p.computer.components.input,
        ReportedClaimSum(200),
        (2, 2, 0),
        positive(2),
        &[CrossEnvironmentSum],
    );
    let g = &p.relations;
    assert_eq!(g.set_global_entries, Exact(1));
    assert_eq!(
        g.selected,
        AssertionPartition {
            windows_within: 0,
            selected_wsl_within: 0,
            cross_environment: 1,
            total: 1
        }
    );
    assert_eq!(g.excluded_entries, Exact(0));
    assert_eq!(g.selected_by_hint.possible_replay_or_copy, 1);
    assert_eq!(g.windows_incident_entries, Exact(1));
    assert_eq!(g.selected_wsl_incident_entries, Exact(1));
    assert_eq!(g.referenced_occurrences, Exact(2));
    assert!(g.hints_present && g.cross_environment_hints_present);

    // Run summaries, counts and the remaining components (RW and RL Complete).
    assert_scope(sel(&p.windows), W, &win_one());
    assert_scope(sel(&p.selected_wsl), L, &win_one());
    assert_computer(
        &p,
        Selection::Both,
        2,
        outcomes(2, 0, 0, 0),
        rec(2, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    for scope in [&p.windows, &p.selected_wsl] {
        assert_components(
            &sel(scope).components,
            [
                sum_comp(100, 1, &[]),
                abs_comp(1, &[MissingClaims]),
                abs_comp(1, &[MissingClaims]),
                uns_comp(1, &[MissingClaims]),
                uns_comp(1, &[MissingClaims]),
            ],
        );
    }
    assert_components(
        &p.computer.components,
        [
            sum_comp(200, 2, &[CrossEnvironmentSum]),
            abs_comp(2, &[MissingClaims]),
            abs_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
        ],
    );

    let without = s04(false);
    assert_no_relations(&without);
    assert_eq!(without_relation_outputs(&p, &without), without);
    assert_eq!(
        without.computer.components.input.value,
        ReportedClaimSum(200)
    );
}

#[test]
fn s05_partial_wsl_run_propagates_to_computer() {
    let set = build(vec![
        occ(
            "h1",
            parsed(r#"{"tokens":{"input":100}}"#),
            W,
            ("W1", "S1", "RW"),
            Complete,
        ),
        occ(
            "h2",
            parsed(r#"{"tokens":{"input":40}}"#),
            L,
            ("L1", "S2", "RL"),
            Partial,
        ),
    ]);
    let d = [
        decl(W, ("W1", "S1", "RW"), Complete),
        decl(L, ("L1", "S2", "RL"), Partial),
    ];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let l = sel(&p.selected_wsl);
    assert_eq!(l.runs.coverage, CoverageState::ReportedIncomplete);
    assert_eq!(l.runs.resolved, status(0, 0, 1, 0, 0));
    assert_eq!(l.runs.qualifications.bits(), q(&[PartialRun]));
    check(
        &l.components.input,
        ReportedClaimSum(40),
        (1, 1, 0),
        positive(1),
        &[PartialRun],
    );
    check(
        &p.computer.components.input,
        ReportedClaimSum(140),
        (2, 2, 0),
        positive(2),
        &[PartialRun, CrossEnvironmentSum],
    );

    // Full S05: Windows RW Complete, WSL RL Partial; no relations, no exclusions.
    assert_scope(sel(&p.windows), W, &win_one());
    let partial_wsl = ScopeExp {
        coverage: CoverageState::ReportedIncomplete,
        resolved: status(0, 0, 1, 0, 0),
        raw_records: status(0, 0, 1, 0, 0),
        raw_declared: status(0, 0, 1, 0, 0),
        run_flags: &[PartialRun],
        ..win_one()
    };
    assert_scope(l, L, &partial_wsl);
    assert_computer(
        &p,
        Selection::Both,
        2,
        outcomes(2, 0, 0, 0),
        rec(2, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
    assert_components(
        &sel(&p.windows).components,
        [
            sum_comp(100, 1, &[]),
            abs_comp(1, &[MissingClaims]),
            abs_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
        ],
    );
    assert_components(
        &l.components,
        [
            sum_comp(40, 1, &[PartialRun]),
            abs_comp(1, &[MissingClaims, PartialRun]),
            abs_comp(1, &[MissingClaims, PartialRun]),
            uns_comp(1, &[MissingClaims, PartialRun]),
            uns_comp(1, &[MissingClaims, PartialRun]),
        ],
    );
    let miss_k: &[Qualification] = &[MissingClaims, PartialRun];
    assert_components(
        &p.computer.components,
        [
            sum_comp(140, 2, &[PartialRun, CrossEnvironmentSum]),
            abs_comp(2, miss_k),
            abs_comp(2, miss_k),
            uns_comp(2, miss_k),
            uns_comp(2, miss_k),
        ],
    );
}

#[test]
fn s06_rejected_record_adds_no_claim_and_complete_is_not_all_parsed() {
    let set = build(vec![
        win("h1", r#"{"tokens":{"input":100}}"#),
        occ("h2", rejected(), W, ("W1", "S1", "R1"), Complete),
    ]);
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.stored_occurrences, Exact(2));
    assert_eq!(w.outcomes, Exact(outcomes(1, 0, 0, 1)));
    assert_eq!(w.records.0.eligible, 1);
    check(
        &w.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &[RejectedRecordsPresent],
    );
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    assert_eq!(w.runs.raw_record_claims, status(0, 2, 0, 0, 0));

    // Full S06: the rejected record adds no claim, zero or bin; B' eligible 1.
    let scope = ScopeExp {
        stored: 2,
        outcomes: outcomes(1, 0, 0, 1),
        records: rec(1, 0, 0, 0),
        raw_records: status(0, 2, 0, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
    assert_components(
        &w.components,
        [
            sum_comp(100, 1, &[RejectedRecordsPresent]),
            abs_comp(1, &[MissingClaims, RejectedRecordsPresent]),
            abs_comp(1, &[MissingClaims, RejectedRecordsPresent]),
            uns_comp(1, &[MissingClaims, RejectedRecordsPresent]),
            uns_comp(1, &[MissingClaims, RejectedRecordsPresent]),
        ],
    );
    let miss_k: &[Qualification] = &[
        MissingClaims,
        RejectedRecordsPresent,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[RejectedRecordsPresent, EnvironmentIndeterminate]),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s07_failed_run_retains_earlier_claims() {
    let set = build(vec![occ(
        "h1",
        parsed(r#"{"tokens":{"input":100}}"#),
        W,
        ("W1", "S1", "R1"),
        Failed,
    )]);
    let d = [r1(Failed)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    check(
        &w.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &[FailedRun],
    );
    assert_eq!(w.runs.coverage, CoverageState::ReportedIncomplete);
    assert_eq!(w.runs.resolved, status(0, 0, 0, 1, 0));

    // Full S07: Failed retains the earlier eligible claim; flag on all five rows.
    let scope = ScopeExp {
        coverage: CoverageState::ReportedIncomplete,
        resolved: status(0, 0, 0, 1, 0),
        raw_records: status(0, 0, 0, 1, 0),
        raw_declared: status(0, 0, 0, 1, 0),
        run_flags: &[FailedRun],
        ..win_one()
    };
    assert_defaults(&p, &scope);
    assert_components(
        &w.components,
        [
            sum_comp(100, 1, &[FailedRun]),
            abs_comp(1, &[MissingClaims, FailedRun]),
            abs_comp(1, &[MissingClaims, FailedRun]),
            uns_comp(1, &[MissingClaims, FailedRun]),
            uns_comp(1, &[MissingClaims, FailedRun]),
        ],
    );
    let miss_k: &[Qualification] = &[MissingClaims, FailedRun, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[FailedRun, EnvironmentIndeterminate]),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

fn s08_declared(status_: abf_observation::SourceRunStatus, flag: Qualification) {
    let set = build(vec![]);
    let d = [r1(status_)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.runs.run_keys, 1);
    assert_eq!(w.runs.coverage, CoverageState::NoTraversalReported);
    assert_eq!(w.runs.qualifications.bits(), q(&[flag]));
    let expected = match status_ {
        NotCollected => status(1, 0, 0, 0, 0),
        _ => status(0, 0, 0, 0, 1),
    };
    assert_eq!(w.runs.resolved, expected);
    assert_eq!(w.runs.raw_declared_claims, expected);
    for c in all(&w.components) {
        check(c, indeterminate(NoTraversalReported), (0, 0, 0), Z, &[flag]);
    }
    for c in all(&p.computer.components) {
        check(
            c,
            indeterminate(AllEnvironmentsIndeterminate),
            (0, 0, 0),
            Z,
            &[flag, EnvironmentIndeterminate],
        );
    }
    assert!(!has_numeric(&p));

    // Full S08(a)/(b): a declaration alone contributes the ref and the run key.
    let scope = ScopeExp {
        stored: 0,
        outcomes: outcomes(0, 0, 0, 0),
        records: rec(0, 0, 0, 0),
        coverage: CoverageState::NoTraversalReported,
        resolved: expected,
        raw_records: status(0, 0, 0, 0, 0),
        raw_declared: expected,
        run_flags: &[flag],
        ..win_one()
    };
    assert_scope(w, W, &scope);
    assert_empty(sel(&p.selected_wsl), L, false);
    assert_computer(
        &p,
        Selection::Both,
        0,
        outcomes(0, 0, 0, 0),
        rec(0, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
}

#[test]
fn s08a_declared_not_collected_is_never_zero() {
    s08_declared(NotCollected, NotCollectedRun);
}

#[test]
fn s08b_declared_unavailable_is_never_zero() {
    s08_declared(Unavailable, UnavailableRun);
}

#[test]
fn s08c_undeclared_without_records_is_no_evidence() {
    let set = build(vec![]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    for scope in [&p.windows, &p.selected_wsl] {
        let r = sel(scope);
        assert_eq!(r.runs.inventory, InventoryBasis::Undeclared);
        assert_eq!(r.runs.qualifications.bits(), q(&[RunInventoryUndeclared]));
        assert_empty_scope(r, &[RunInventoryUndeclared]);
    }
    assert_empty(sel(&p.windows), W, true);
    assert_empty(sel(&p.selected_wsl), L, true);
    assert_computer(
        &p,
        Selection::Both,
        0,
        outcomes(0, 0, 0, 0),
        rec(0, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
    for c in all(&p.computer.components) {
        check(
            c,
            indeterminate(AllEnvironmentsIndeterminate),
            (0, 0, 0),
            Z,
            &[RunInventoryUndeclared, EnvironmentIndeterminate],
        );
    }
}

#[test]
fn s09_record_under_unavailable_run_is_inconsistent_but_retained() {
    let set = build(vec![occ(
        "h1",
        parsed(r#"{"tokens":{"input":100}}"#),
        W,
        ("W1", "S1", "R1"),
        Unavailable,
    )]);
    let d = [r1(Unavailable)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.records.0.records_without_reported_traversal, 1);
    assert_eq!(w.runs.resolved, status(0, 0, 0, 0, 1));
    assert_eq!(w.runs.conflicting_keys, 0);
    assert_eq!(w.runs.coverage, CoverageState::Inconsistent);
    let flags = [UnavailableRun, RecordWithoutReportedTraversal];
    assert_eq!(w.runs.qualifications.bits(), q(&flags));
    check(
        &w.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &flags,
    );

    // Full S09: original Accepted outcome kept; the status is not repaired.
    let scope = ScopeExp {
        records: rec(1, 0, 0, 1),
        coverage: CoverageState::Inconsistent,
        resolved: status(0, 0, 0, 0, 1),
        raw_records: status(0, 0, 0, 0, 1),
        raw_declared: status(0, 0, 0, 0, 1),
        run_flags: &flags,
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let miss: &[Qualification] = &[
        MissingClaims,
        UnavailableRun,
        RecordWithoutReportedTraversal,
    ];
    assert_components(
        &w.components,
        [
            sum_comp(100, 1, &flags),
            abs_comp(1, miss),
            abs_comp(1, miss),
            uns_comp(1, miss),
            uns_comp(1, miss),
        ],
    );
    let miss_k: &[Qualification] = &[
        MissingClaims,
        UnavailableRun,
        RecordWithoutReportedTraversal,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [
            sum_comp(
                100,
                1,
                &[
                    UnavailableRun,
                    RecordWithoutReportedTraversal,
                    EnvironmentIndeterminate,
                ],
            ),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s10_total_only_never_infers_input_or_output() {
    let p = default_case(r#"{"tokens":{"total":500}}"#);
    let w = sel(&p.windows);
    assert_eq!(w.outcomes, Exact(ACCEPTED_1));
    let c = &w.components;
    check(&c.total, ReportedClaimSum(500), (1, 1, 0), positive(1), &[]);
    check(
        &c.input,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims],
    );
    check(
        &c.output,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims],
    );
    check(
        &c.cache_read,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );
    check(
        &c.cache_write,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );

    // Full S10: Windows scope defaults and the Computer rows (no total split).
    assert_defaults(&p, &win_one());
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            sum_comp(500, 1, &[EnvironmentIndeterminate]),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s11_five_independent_components() {
    let p = default_case(
        r#"{"tokens":{"input":100,"output":50,"cache":{"read":30,"write":10},"total":190}}"#,
    );
    let c = &sel(&p.windows).components;
    for (r, v) in all(c).into_iter().zip([100, 50, 190, 30, 10]) {
        check(r, ReportedClaimSum(v), (1, 1, 0), positive(1), &[]);
    }

    // Full S11: scope defaults and the five Computer rows, same literals.
    assert_defaults(&p, &win_one());
    let k: &[Qualification] = &[EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, k),
            sum_comp(50, 1, k),
            sum_comp(190, 1, k),
            sum_comp(30, 1, k),
            sum_comp(10, 1, k),
        ],
    );
}

#[test]
fn s12_u64_max_claims_sum_exactly_in_u128() {
    let frame = r#"{"tokens":{"input":18446744073709551615}}"#;
    let set = build(vec![win("h1", frame), win("h2", frame), win("h3", frame)]);
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    check(
        &sel(&p.windows).components.input,
        ReportedClaimSum(55_340_232_221_128_654_845),
        (3, 3, 0),
        positive(3),
        &[],
    );

    // Full S12: three Accepted records, one declared Complete run, no overflow.
    let scope = ScopeExp {
        stored: 3,
        outcomes: outcomes(3, 0, 0, 0),
        records: rec(3, 0, 0, 0),
        raw_records: status(0, 3, 0, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let w = sel(&p.windows);
    assert_components(
        &w.components,
        [
            sum_comp(55_340_232_221_128_654_845, 3, &[]),
            abs_comp(3, &[MissingClaims]),
            abs_comp(3, &[MissingClaims]),
            uns_comp(3, &[MissingClaims]),
            uns_comp(3, &[MissingClaims]),
        ],
    );
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(55_340_232_221_128_654_845, 3, &[EnvironmentIndeterminate]),
            abs_comp(3, miss_k),
            abs_comp(3, miss_k),
            uns_comp(3, miss_k),
            uns_comp(3, miss_k),
        ],
    );
    // No Overflow anywhere: 3 × (2^64 − 1) fits u128 exactly.
    assert!(
        all(&w.components)
            .into_iter()
            .chain(all(&p.computer.components))
            .all(|c| c.value != ComponentValue::Overflow)
    );
}

#[test]
fn s13_qualified_record_claims_are_retained_and_flagged() {
    let p = default_case(r#"{"tokens":{"input":100,"output":null}}"#);
    let w = sel(&p.windows);
    assert_eq!(w.outcomes, Exact(outcomes(0, 1, 0, 0)));
    let c = &w.components;
    check(
        &c.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &[FromQualifiedRecords],
    );
    check(
        &c.output,
        NoValidClaims,
        (1, 0, 0),
        FieldStateCounts { null: 1, ..Z },
        &[MissingClaims],
    );
    check(
        &c.total,
        NoValidClaims,
        (1, 0, 0),
        absent(1),
        &[MissingClaims],
    );
    check(
        &c.cache_read,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );
    check(
        &c.cache_write,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );

    // Full S13: original Qualified outcome; the record stays eligible (B' 1).
    assert_defaults(
        &p,
        &ScopeExp {
            outcomes: outcomes(0, 1, 0, 0),
            ..win_one()
        },
    );
    let null_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[FromQualifiedRecords, EnvironmentIndeterminate]),
            (
                NoValidClaims,
                (1, 0, 0),
                FieldStateCounts { null: 1, ..Z },
                null_k,
            ),
            abs_comp(1, null_k),
            uns_comp(1, null_k),
            uns_comp(1, null_k),
        ],
    );
}

#[test]
fn s14a_parser_incomplete_record() {
    let p = default_case(r#"{"time":{"created":1700000000}}"#);
    let w = sel(&p.windows);
    assert_eq!(w.outcomes, Exact(outcomes(0, 0, 1, 0)));
    assert_eq!((w.records.0.eligible, w.records.0.contradictory), (0, 0));
    for c in all(&w.components) {
        check(c, NoValidClaims, (0, 0, 0), Z, &[IncompleteRecordsPresent]);
    }

    // Full S14a: IncompleteTokens only; nothing unsupported, nothing contradictory.
    let scope = ScopeExp {
        outcomes: outcomes(0, 0, 1, 0),
        records: rec(0, 0, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let k: &[Qualification] = &[IncompleteRecordsPresent, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [(NoValidClaims, (0, 0, 0), Z, k); 5],
    );
}

/// S14b: constructed Incomplete with a Valid token, diagnostics copied from (a).
pub fn s14b_result() -> ParseResult {
    let ParseResult::Observation(mut o) = parsed(r#"{"time":{"created":1700000000}}"#) else {
        panic!("fixture");
    };
    assert_eq!(o.status, Status::Incomplete);
    o.tokens = Field::Valid(());
    o.input = Field::Valid(100);
    o.output = Field::Absent;
    o.total = Field::Absent;
    o.cache = Field::Absent;
    assert_eq!(o.cache_read, Field::Unsupported);
    ParseResult::Observation(o)
}

#[test]
fn s14b_constructed_contradiction_keeps_original_outcome() {
    let set = build(vec![occ(
        "h1",
        s14b_result(),
        W,
        ("W1", "S1", "R1"),
        Complete,
    )]);
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.outcomes, Exact(outcomes(0, 0, 1, 0)));
    assert_eq!((w.records.0.eligible, w.records.0.contradictory), (0, 1));
    for c in all(&w.components) {
        check(
            c,
            NoValidClaims,
            (0, 0, 0),
            Z,
            &[IncompleteRecordsPresent, ContradictoryRecordsExcluded],
        );
    }
    // Full S14b: original Incomplete kept; contradictory 1, eligible 0.
    let scope = ScopeExp {
        outcomes: outcomes(0, 0, 1, 0),
        records: rec(0, 1, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let k: &[Qualification] = &[
        IncompleteRecordsPresent,
        ContradictoryRecordsExcluded,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [(NoValidClaims, (0, 0, 0), Z, k); 5],
    );
    // The raw Valid(100) stays in the stored observation.
    let o = set.occurrences()[0]
        .envelope()
        .parser_result()
        .observation()
        .unwrap();
    assert_eq!(o.input, Field::Valid(100));
}

#[test]
fn s14c_unsupported_only_component() {
    let p = default_case(r#"{"tokens":{"reasoning":5}}"#);
    let w = sel(&p.windows);
    assert_eq!(w.outcomes, Exact(outcomes(0, 0, 1, 0)));
    assert_eq!(w.records.0.unsupported_component_observations, 1);
    for c in all(&w.components) {
        check(
            c,
            NoValidClaims,
            (0, 0, 0),
            Z,
            &[IncompleteRecordsPresent, UnknownComponentsDiscarded],
        );
    }

    // Full S14c: original Incomplete; unsupported observation 1, eligible 0.
    let scope = ScopeExp {
        outcomes: outcomes(0, 0, 1, 0),
        records: rec(0, 0, 1, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let k: &[Qualification] = &[
        IncompleteRecordsPresent,
        UnknownComponentsDiscarded,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [(NoValidClaims, (0, 0, 0), Z, k); 5],
    );
}

#[test]
fn s15_inconsistent_looking_components_are_not_validated() {
    let p = default_case(r#"{"tokens":{"total":10,"input":100,"output":50}}"#);
    let c = &sel(&p.windows).components;
    check(&c.total, ReportedClaimSum(10), (1, 1, 0), positive(1), &[]);
    check(&c.input, ReportedClaimSum(100), (1, 1, 0), positive(1), &[]);
    check(&c.output, ReportedClaimSum(50), (1, 1, 0), positive(1), &[]);
    check(
        &c.cache_read,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );
    check(
        &c.cache_write,
        NoValidClaims,
        (1, 0, 0),
        unsupported(1),
        &[MissingClaims],
    );

    // Full S15: values shown as reported, no conflict flag, Windows defaults.
    assert_defaults(&p, &win_one());
    let miss_k: &[Qualification] = &[MissingClaims, EnvironmentIndeterminate];
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[EnvironmentIndeterminate]),
            sum_comp(50, 1, &[EnvironmentIndeterminate]),
            sum_comp(10, 1, &[EnvironmentIndeterminate]),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}

#[test]
fn s16a_empty_set_undeclared() {
    let set = build(vec![]);
    let p = run(&set, RunInventory::Undeclared, Selection::Both);
    for scope in [&p.windows, &p.selected_wsl] {
        assert_empty_scope(sel(scope), &[RunInventoryUndeclared]);
    }
    assert_empty(sel(&p.windows), W, true);
    assert_empty(sel(&p.selected_wsl), L, true);
    assert_computer(
        &p,
        Selection::Both,
        0,
        outcomes(0, 0, 0, 0),
        rec(0, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
    for c in all(&p.computer.components) {
        check(
            c,
            indeterminate(AllEnvironmentsIndeterminate),
            (0, 0, 0),
            Z,
            &[RunInventoryUndeclared, EnvironmentIndeterminate],
        );
    }
}

#[test]
fn s16b_declared_complete_without_records_has_no_numeric_zero() {
    let set = build(vec![]);
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let w = sel(&p.windows);
    assert_eq!(w.runs.run_keys, 1);
    assert_eq!(w.runs.coverage, CoverageState::ReportedComplete);
    for c in all(&w.components) {
        check(c, NoValidClaims, (0, 0, 0), Z, &[]);
    }
    assert_empty_scope(sel(&p.selected_wsl), &[]);
    for c in all(&p.computer.components) {
        check(c, NoValidClaims, (0, 0, 0), Z, &[EnvironmentIndeterminate]);
    }
    assert!(!has_numeric(&p));

    // Full S16(b): one represented, declared-Complete key and no records.
    let scope = ScopeExp {
        stored: 0,
        outcomes: outcomes(0, 0, 0, 0),
        records: rec(0, 0, 0, 0),
        raw_records: status(0, 0, 0, 0, 0),
        ..win_one()
    };
    assert_defaults(&p, &scope);
}

#[test]
fn s16c_declared_empty_list_is_not_a_complete_scan() {
    let set = build(vec![]);
    let p = run(&set, RunInventory::Declared(&[]), Selection::Both);
    for scope in [&p.windows, &p.selected_wsl] {
        let r = sel(scope);
        assert_eq!(r.runs.inventory, InventoryBasis::Declared);
        assert_empty_scope(r, &[]);
    }
    for c in all(&p.computer.components) {
        check(
            c,
            indeterminate(AllEnvironmentsIndeterminate),
            (0, 0, 0),
            Z,
            &[EnvironmentIndeterminate],
        );
    }

    // Full S16(c): `Declared(&[])` is still a Declared inventory, with no flags.
    assert_empty(sel(&p.windows), W, false);
    assert_empty(sel(&p.selected_wsl), L, false);
    assert_computer(
        &p,
        Selection::Both,
        0,
        outcomes(0, 0, 0, 0),
        rec(0, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
}

pub fn s17_set() -> abf_identity_relations::SyntheticSet {
    build(vec![
        win("h1", r#"{"tokens":{"input":100}}"#),
        occ(
            "h2",
            parsed(r#"{"tokens":{"input":100}}"#),
            W,
            ("W1", "S1", "R1"),
            Failed,
        ),
    ])
}

#[test]
fn s17_conflicting_run_statuses_are_not_arbitrated() {
    let set = s17_set();
    let d = [r1(Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let r = &sel(&p.windows).runs;
    assert_eq!((r.run_keys, r.conflicting_keys), (1, 1));
    assert_eq!(r.resolved, status(0, 0, 0, 0, 0));
    assert_eq!(r.raw_record_claims, status(0, 1, 0, 1, 0));
    assert_eq!(r.raw_declared_claims, status(0, 1, 0, 0, 0));
    assert_eq!(r.coverage, CoverageState::Inconsistent);
    let flags = [RunStatusConflict, FailedRun];
    assert_eq!(r.qualifications.bits(), q(&flags));
    check(
        &sel(&p.windows).components.input,
        ReportedClaimSum(200),
        (2, 2, 0),
        positive(2),
        &flags,
    );

    // Full S17: both records Accepted; no status chosen, raw Failed survives.
    let scope = ScopeExp {
        stored: 2,
        outcomes: outcomes(2, 0, 0, 0),
        records: rec(2, 0, 0, 0),
        coverage: CoverageState::Inconsistent,
        conflicting: 1,
        resolved: status(0, 0, 0, 0, 0),
        raw_records: status(0, 1, 0, 1, 0),
        raw_declared: status(0, 1, 0, 0, 0),
        run_flags: &flags,
        ..win_one()
    };
    assert_defaults(&p, &scope);
    let miss: &[Qualification] = &[MissingClaims, RunStatusConflict, FailedRun];
    assert_components(
        &sel(&p.windows).components,
        [
            sum_comp(200, 2, &flags),
            abs_comp(2, miss),
            abs_comp(2, miss),
            uns_comp(2, miss),
            uns_comp(2, miss),
        ],
    );
    let miss_k: &[Qualification] = &[
        MissingClaims,
        RunStatusConflict,
        FailedRun,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [
            sum_comp(
                200,
                2,
                &[RunStatusConflict, FailedRun, EnvironmentIndeterminate],
            ),
            abs_comp(2, miss_k),
            abs_comp(2, miss_k),
            uns_comp(2, miss_k),
            uns_comp(2, miss_k),
        ],
    );
}

#[test]
fn s18_repeated_reversed_conflicting_hints_are_separate_entries() {
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
    relate(&mut set, "h1", "h2", MatchingClaimedIdentifiers);
    relate(&mut set, "h2", "h1", MatchingClaimedIdentifiers);
    relate(&mut set, "h1", "h2", DifferentClaimedIdentifiers);
    relate(&mut set, "h2", "h3", PossibleReplayOrCopy);
    let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let ws = &sel(&p.windows).components.input;
    check(ws, ReportedClaimSum(200), (2, 2, 0), positive(2), &[]);
    let ls = &sel(&p.selected_wsl).components.input;
    check(ls, ReportedClaimSum(100), (1, 1, 0), positive(1), &[]);
    check(
        &p.computer.components.input,
        ReportedClaimSum(300),
        (3, 3, 0),
        positive(3),
        &[CrossEnvironmentSum],
    );
    let g = &p.relations;
    assert_eq!(g.set_global_entries, Exact(4));
    assert_eq!(
        g.selected,
        AssertionPartition {
            windows_within: 3,
            selected_wsl_within: 0,
            cross_environment: 1,
            total: 4
        }
    );
    assert_eq!(g.excluded_entries, Exact(0));
    assert_eq!(
        g.selected_by_hint,
        HintCounts {
            matching_claimed_identifiers: 2,
            different_claimed_identifiers: 1,
            equal_reported_values: 0,
            possible_replay_or_copy: 1,
            unclassified: 0
        }
    );
    assert_eq!(g.windows_incident_entries, Exact(4));
    assert_eq!(g.selected_wsl_incident_entries, Exact(1));
    assert_ne!(
        g.windows_incident_entries.0 + g.selected_wsl_incident_entries.0,
        g.selected.total
    );
    assert_eq!(g.referenced_occurrences, Exact(3));
    assert!(g.hints_present && g.cross_environment_hints_present);
    assert_eq!(p.computer.stored_occurrences, Exact(3));

    // Full S18: hints touch only G; scopes, counts and other rows are as without.
    let w_scope = ScopeExp {
        stored: 2,
        outcomes: outcomes(2, 0, 0, 0),
        records: rec(2, 0, 0, 0),
        raw_records: status(0, 2, 0, 0, 0),
        ..win_one()
    };
    assert_scope(sel(&p.windows), W, &w_scope);
    assert_scope(sel(&p.selected_wsl), L, &win_one());
    assert_computer(
        &p,
        Selection::Both,
        3,
        outcomes(3, 0, 0, 0),
        rec(3, 0, 0, 0),
    );
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_components(
        &sel(&p.windows).components,
        [
            sum_comp(200, 2, &[]),
            abs_comp(2, &[MissingClaims]),
            abs_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
            uns_comp(2, &[MissingClaims]),
        ],
    );
    assert_components(
        &sel(&p.selected_wsl).components,
        [
            sum_comp(100, 1, &[]),
            abs_comp(1, &[MissingClaims]),
            abs_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
        ],
    );
    assert_components(
        &p.computer.components,
        [
            sum_comp(300, 3, &[CrossEnvironmentSum]),
            abs_comp(3, &[MissingClaims]),
            abs_comp(3, &[MissingClaims]),
            uns_comp(3, &[MissingClaims]),
            uns_comp(3, &[MissingClaims]),
        ],
    );
}

#[test]
fn s19_windows_only_excludes_and_counts_wsl_evidence() {
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
    relate(&mut set, "h1", "h2", Unclassified);
    let d = [r1(Complete), decl(L, ("L1", "S2", "RL"), Complete)];
    let p = run(&set, RunInventory::Declared(&d), Selection::WindowsOnly);
    assert_eq!(p.selected_wsl, ScopeResult::NotSelected);
    assert_eq!(
        p.excluded,
        Exact(ExcludedCounts {
            occurrences: 1,
            declarations: 1,
            relation_entries: 1
        })
    );
    let w = sel(&p.windows);
    check(
        &p.computer.components.input,
        ReportedClaimSum(100),
        (1, 1, 0),
        positive(1),
        &[],
    );
    assert_eq!(p.computer.components, w.components);
    assert_eq!(p.computer.stored_occurrences, Exact(1));
    assert_eq!(p.computer.per_environment.selected_wsl, None);
    assert_eq!(p.computer.per_environment.windows, Some(w.runs));
    let g = &p.relations;
    assert_eq!(g.set_global_entries, Exact(1));
    assert_eq!(g.selected.total, 0);
    assert_eq!(g.excluded_entries, Exact(1));
    assert!(!g.hints_present);

    // Full S19: Windows scope, single-selection Computer rows and empty selected G.
    assert_scope(w, W, &win_one());
    assert_computer(&p, Selection::WindowsOnly, 1, ACCEPTED_1, rec(1, 0, 0, 0));
    assert_components(
        &w.components,
        [
            sum_comp(100, 1, &[]),
            abs_comp(1, &[MissingClaims]),
            abs_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
        ],
    );
    assert_components(
        &p.computer.components,
        [
            sum_comp(100, 1, &[]),
            abs_comp(1, &[MissingClaims]),
            abs_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
            uns_comp(1, &[MissingClaims]),
        ],
    );
    assert_eq!(g.selected, AssertionPartition::default());
    assert_eq!(g.selected_by_hint, HintCounts::default());
    assert_eq!(g.windows_incident_entries, Exact(0));
    assert_eq!(g.selected_wsl_incident_entries, Exact(0));
    assert_eq!(g.referenced_occurrences, Exact(0));
    assert!(!g.cross_environment_hints_present);
}

pub fn s20_inputs() -> (abf_identity_relations::SyntheticSet, [RunDeclaration; 2]) {
    let set = build(vec![occ(
        "h1",
        parsed(r#"{"tokens":{"input":10}}"#),
        L,
        ("L1", "S1", "RL1"),
        Complete,
    )]);
    let d = [
        decl(L, ("L1", "S1", "RL1"), Complete),
        decl(L, ("L2", "S2", "RL2"), NotCollected),
    ];
    (set, d)
}

#[test]
fn s20_zero_record_declaration_participates_in_ref_multiplicity() {
    let (set, d) = s20_inputs();
    let p = run(&set, RunInventory::Declared(&d), Selection::Both);
    let l = sel(&p.selected_wsl);
    assert_eq!(l.environment_refs, RefMultiplicity::Multiple(2));
    assert_eq!(l.runs.run_keys, 2);
    assert_eq!(l.runs.resolved, status(1, 1, 0, 0, 0));
    assert_eq!(l.runs.coverage, CoverageState::ReportedIncomplete);
    let flags = [EnvironmentRefAmbiguous, NotCollectedRun];
    check(
        &l.components.input,
        ReportedClaimSum(10),
        (1, 1, 0),
        positive(1),
        &flags,
    );
    assert_eq!(l.outcomes, Exact(ACCEPTED_1));
    let w = sel(&p.windows);
    assert_eq!(w.runs.coverage, CoverageState::NoEvidence);
    assert_empty_scope(w, &[]);
    check(
        &p.computer.components.input,
        ReportedClaimSum(10),
        (1, 1, 0),
        positive(1),
        &[
            EnvironmentRefAmbiguous,
            NotCollectedRun,
            EnvironmentIndeterminate,
        ],
    );

    // Full S20: counts, run summaries, the other four rows, and no exclusions.
    let wsl = ScopeExp {
        refs: RefMultiplicity::Multiple(2),
        run_keys: 2,
        coverage: CoverageState::ReportedIncomplete,
        resolved: status(1, 1, 0, 0, 0),
        raw_declared: status(1, 1, 0, 0, 0),
        run_flags: &[NotCollectedRun],
        ..win_one()
    };
    assert_scope(l, L, &wsl);
    assert_empty(w, W, false);
    assert_computer(&p, Selection::Both, 1, ACCEPTED_1, rec(1, 0, 0, 0));
    assert_eq!(p.excluded, Exact(ExcludedCounts::default()));
    assert_no_relations(&p);
    let miss: &[Qualification] = &[MissingClaims, EnvironmentRefAmbiguous, NotCollectedRun];
    assert_components(
        &l.components,
        [
            sum_comp(10, 1, &flags),
            abs_comp(1, miss),
            abs_comp(1, miss),
            uns_comp(1, miss),
            uns_comp(1, miss),
        ],
    );
    let miss_k: &[Qualification] = &[
        MissingClaims,
        EnvironmentRefAmbiguous,
        NotCollectedRun,
        EnvironmentIndeterminate,
    ];
    assert_components(
        &p.computer.components,
        [
            sum_comp(
                10,
                1,
                &[
                    EnvironmentRefAmbiguous,
                    NotCollectedRun,
                    EnvironmentIndeterminate,
                ],
            ),
            abs_comp(1, miss_k),
            abs_comp(1, miss_k),
            uns_comp(1, miss_k),
            uns_comp(1, miss_k),
        ],
    );
}
