//! Fictional fixtures and independent invariant checks shared by test targets.
#![allow(dead_code)]

use abf_counting::*;
use abf_identity_relations::{Limits as SetLimits, Occurrence, SyntheticSet, UnverifiedHint};
use abf_observation::{
    DeclaredContext, EnvironmentKind, OpaqueRef, SourceRunStatus, from_opencode,
};
use abf_parser::{Code as ParserCode, Limits, ParseResult, parse};

pub use EnvironmentKind::{SelectedWsl as L, Windows as W};
pub use Qualification::*;
pub use SourceRunStatus::{Complete, Failed, NotCollected, Partial, Unavailable};

pub const Z: FieldStateCounts = FieldStateCounts {
    valid_zero: 0,
    valid_positive: 0,
    absent: 0,
    null: 0,
    wrong_type: 0,
    out_of_range: 0,
    unsupported: 0,
};

pub fn id(text: &str) -> OpaqueRef {
    OpaqueRef::new(text).unwrap()
}

pub fn parsed(frame: &str) -> ParseResult {
    parse(frame.as_bytes(), None, Limits::MAX)
}

/// A genuine parser rejection from a fictional malformed frame.
pub fn rejected() -> ParseResult {
    let r = parsed(r#"{"tokens":}"#);
    assert_eq!(r, ParseResult::Rejected(ParserCode::MalformedJson));
    r
}

pub fn occ(
    handle: &str,
    result: ParseResult,
    kind: EnvironmentKind,
    run: (&str, &str, &str),
    status: SourceRunStatus,
) -> Occurrence {
    let context = DeclaredContext::new(kind, id(run.0), id(run.1), id(run.2), None, None);
    Occurrence::new(id(handle), from_opencode(result, context, status).unwrap())
}

/// Default Windows record: W1/S1/R1, Complete.
pub fn win(handle: &str, frame: &str) -> Occurrence {
    occ(handle, parsed(frame), W, ("W1", "S1", "R1"), Complete)
}

pub fn decl(
    kind: EnvironmentKind,
    run: (&str, &str, &str),
    status: SourceRunStatus,
) -> RunDeclaration {
    RunDeclaration {
        environment_kind: kind,
        environment_ref: id(run.0),
        source_ref: id(run.1),
        run_ref: id(run.2),
        status,
    }
}

pub fn r1(status: SourceRunStatus) -> RunDeclaration {
    decl(W, ("W1", "S1", "R1"), status)
}

pub fn build(occurrences: Vec<Occurrence>) -> SyntheticSet {
    let mut set = SyntheticSet::new(SetLimits::MAX).unwrap();
    for o in occurrences {
        set.insert(o).unwrap();
    }
    set
}

pub fn relate(set: &mut SyntheticSet, from: &str, to: &str, hint: UnverifiedHint) {
    set.assert_relation(id(from), id(to), hint).unwrap();
}

pub fn run(set: &SyntheticSet, inventory: RunInventory<'_>, selection: Selection) -> Projection {
    let p = project(set, inventory, selection).unwrap();
    invariants(&p);
    p
}

pub fn sel(scope: &ScopeResult) -> &EnvironmentResult {
    scope.selected().expect("scope selected")
}

pub fn all<T>(c: &Components<T>) -> [&T; 5] {
    [&c.input, &c.output, &c.total, &c.cache_read, &c.cache_write]
}

pub fn q(list: &[Qualification]) -> u32 {
    list.iter().fold(0, |acc, f| acc | *f as u32)
}

/// Value, (eligible, contributing, zero_valued), bins and complete flag list.
pub fn check(
    c: &ComponentResult,
    value: ComponentValue,
    counts: (u32, u32, u32),
    bins: FieldStateCounts,
    flags: &[Qualification],
) {
    assert_eq!(c.value, value);
    let e = &c.evidence;
    assert_eq!(
        (e.eligible_records, e.contributing, e.zero_valued),
        counts,
        "evidence counts"
    );
    assert_eq!(e.field_states, bins);
    assert_eq!(
        e.qualifications.bits(),
        q(flags),
        "flags {:?}",
        e.qualifications
    );
}

pub fn status(
    not_collected: u32,
    complete: u32,
    partial: u32,
    failed: u32,
    unavailable: u32,
) -> StatusCounts {
    StatusCounts {
        not_collected,
        complete,
        partial,
        failed,
        unavailable,
    }
}

pub fn outcomes(accepted: u32, qualified: u32, incomplete: u32, rejected: u32) -> OutcomeCounts {
    OutcomeCounts {
        accepted,
        qualified,
        incomplete,
        rejected,
    }
}

pub fn indeterminate(reason: IndeterminateReason) -> ComponentValue {
    ComponentValue::Indeterminate(reason)
}

/// Contract default: Selected WSL has no occurrences and no declarations.
pub fn assert_empty_scope(r: &EnvironmentResult, extra: &[Qualification]) {
    assert_eq!(r.environment_refs, RefMultiplicity::None);
    assert_eq!(r.stored_occurrences, Exact(0));
    assert_eq!(r.runs.coverage, CoverageState::NoEvidence);
    assert_eq!(r.runs.run_keys, 0);
    for c in all(&r.components) {
        check(
            c,
            indeterminate(IndeterminateReason::NoRunEvidence),
            (0, 0, 0),
            Z,
            extra,
        );
    }
}

/// Default-case Computer under `Both` with an empty Selected WSL: the Windows
/// value and evidence, plus `EnvironmentIndeterminate`.
pub fn assert_computer_is_windows_plus_indeterminate(p: &Projection) {
    let w = sel(&p.windows);
    for (c, wc) in all(&p.computer.components)
        .into_iter()
        .zip(all(&w.components))
    {
        let expected = match wc.value {
            ComponentValue::Indeterminate(_) => {
                indeterminate(IndeterminateReason::AllEnvironmentsIndeterminate)
            }
            v => v,
        };
        assert_eq!(c.value, expected);
        assert_eq!(c.evidence.eligible_records, wc.evidence.eligible_records);
        assert_eq!(c.evidence.contributing, wc.evidence.contributing);
        assert_eq!(c.evidence.zero_valued, wc.evidence.zero_valued);
        assert_eq!(c.evidence.field_states, wc.evidence.field_states);
        assert_eq!(
            c.evidence.qualifications.bits(),
            wc.evidence.qualifications.bits() | EnvironmentIndeterminate as u32
        );
    }
}

pub fn assert_no_relations(p: &Projection) {
    let g = &p.relations;
    assert_eq!(g.set_global_entries, Exact(0));
    assert_eq!(g.selected, AssertionPartition::default());
    assert_eq!(g.excluded_entries, Exact(0));
    assert_eq!(g.selected_by_hint, HintCounts::default());
    assert_eq!(g.referenced_occurrences, Exact(0));
    assert!(!g.hints_present && !g.cross_environment_hints_present);
}

pub fn has_numeric(p: &Projection) -> bool {
    let mut results: Vec<&ComponentResult> = all(&p.computer.components).to_vec();
    for s in [&p.windows, &p.selected_wsl] {
        if let ScopeResult::Selected(r) = s {
            results.extend(all(&r.components));
        }
    }
    results
        .iter()
        .any(|c| matches!(c.value, ComponentValue::ReportedClaimSum(_)))
}

fn sum_bins(b: &FieldStateCounts) -> u32 {
    b.valid_zero
        + b.valid_positive
        + b.absent
        + b.null
        + b.wrong_type
        + b.out_of_range
        + b.unsupported
}

fn sum_status(s: &StatusCounts) -> u32 {
    s.not_collected + s.complete + s.partial + s.failed + s.unavailable
}

fn sum_outcomes(o: &OutcomeCounts) -> u32 {
    o.accepted + o.qualified + o.incomplete + o.rejected
}

fn component_invariants(c: &ComponentResult, eligible: u32) {
    let e = &c.evidence;
    assert_eq!(sum_bins(&e.field_states), e.eligible_records);
    assert_eq!(e.eligible_records, eligible);
    assert_eq!(
        e.field_states.valid_zero + e.field_states.valid_positive,
        e.contributing
    );
    assert_eq!(e.field_states.valid_zero, e.zero_valued);
    assert_eq!(e.identity, IdentityBasis::Unresolved);
    assert_eq!(e.traversal, TraversalBasis::CallerReported);
    if let ComponentValue::ReportedClaimSum(_) = c.value {
        assert!(e.contributing >= 1, "numeric value requires a contributor");
    }
    if e.contributing >= 1 {
        assert!(matches!(
            c.value,
            ComponentValue::ReportedClaimSum(_) | ComponentValue::Overflow
        ));
    }
}

/// T8, §8.3/§8.4 invariants and D/D' for every scope, Computer and G.
pub fn invariants(p: &Projection) {
    for s in [&p.windows, &p.selected_wsl] {
        let ScopeResult::Selected(r) = s else {
            continue;
        };
        assert_eq!(sum_outcomes(&r.outcomes.0), r.stored_occurrences.0);
        for c in all(&r.components) {
            component_invariants(c, r.records.0.eligible);
        }
        let runs = &r.runs;
        assert_eq!(
            runs.run_keys,
            runs.conflicting_keys + sum_status(&runs.resolved)
        );
        assert_eq!(sum_status(&runs.raw_record_claims), r.stored_occurrences.0);
        assert_eq!(runs.traversal, TraversalBasis::CallerReported);
        assert_eq!(r.physical_events, PhysicalEventCount::Unresolved);
        assert_eq!(r.usage, UsageTotal::Unsupported);
    }
    let c = &p.computer;
    assert_eq!(sum_outcomes(&c.outcomes.0), c.stored_occurrences.0);
    for r in all(&c.components) {
        component_invariants(r, c.records.0.eligible);
    }
    assert_eq!(
        c.per_environment.windows,
        p.windows.selected().map(|r| r.runs)
    );
    assert_eq!(
        c.per_environment.selected_wsl,
        p.selected_wsl.selected().map(|r| r.runs)
    );
    assert_eq!(c.physical_events, PhysicalEventCount::Unresolved);
    assert_eq!(c.usage, UsageTotal::Unsupported);
    let g = &p.relations;
    let s = &g.selected;
    assert_eq!(
        s.windows_within + s.selected_wsl_within + s.cross_environment,
        s.total
    );
    assert_eq!(g.set_global_entries.0, s.total + g.excluded_entries.0);
    let h = &g.selected_by_hint;
    assert_eq!(
        h.matching_claimed_identifiers
            + h.different_claimed_identifiers
            + h.equal_reported_values
            + h.possible_replay_or_copy
            + h.unclassified,
        s.total
    );
    assert_eq!(
        g.windows_incident_entries.0,
        s.windows_within + s.cross_environment
    );
    assert_eq!(
        g.selected_wsl_incident_entries.0,
        s.selected_wsl_within + s.cross_environment
    );
    assert_eq!(g.hints_present, s.total >= 1);
    assert_eq!(g.cross_environment_hints_present, s.cross_environment >= 1);
    assert_eq!(p.excluded.0.relation_entries, g.excluded_entries.0);
}

/// Projection with the two relation-dependent locations normalized (§5.2).
pub fn without_relation_outputs(p: &Projection, reference: &Projection) -> Projection {
    let mut p = *p;
    p.relations = reference.relations;
    p.excluded.0.relation_entries = reference.excluded.0.relation_entries;
    p
}
