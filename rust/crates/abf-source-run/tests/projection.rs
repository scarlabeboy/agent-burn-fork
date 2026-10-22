use abf_source_run::*;

#[test]
fn empty_membership_has_declared_no_evidence_and_keeps_selection() {
    let catalog = CatalogBuilder::new().build();
    let membership = ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: None,
        },
        &[],
        &catalog,
    )
    .unwrap();
    let paired = project_with_companion(&membership).unwrap();
    assert_eq!(paired.selection(), Selection::WindowsOnly);
    assert_eq!(paired.members().count(), 0);
    let w = paired.projection().windows.selected().unwrap();
    assert_eq!(w.runs.inventory, abf_counting::InventoryBasis::Declared);
    assert_eq!(w.runs.coverage, abf_counting::CoverageState::NoEvidence);
    assert_eq!(
        w.components.input.value,
        abf_counting::ComponentValue::Indeterminate(
            abf_counting::IndeterminateReason::NoRunEvidence
        )
    );
}

fn scope() -> CatalogScope {
    CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap()
}
fn catalog() -> CatalogSnapshot {
    let mut b = CatalogBuilder::new();
    b.add(scope(), "f1v1").unwrap();
    b.build()
}
fn frame(n: u64, payload: Payload) -> ReaderFrame {
    ReaderFrame {
        attempt: AttemptRef::new("att1").unwrap(),
        frame_seq: n,
        payload,
    }
}
fn initial(outcome: RawEnumeration, sources: &[&str]) -> Payload {
    Payload::Enumeration {
        member: 0,
        second: false,
        outcome,
        sources: sources.iter().map(|s| s.to_string()).collect(),
        observed_at: None,
    }
}
fn event(source: &str, seq: u64, event: SourceEvent) -> Payload {
    Payload::Source {
        source: source.into(),
        source_seq: seq,
        event,
    }
}
fn record(source: &str, seq: u64, input: u32) -> Payload {
    let bytes = format!("{{\"tokens\":{{\"input\":{input}}}}}").into_bytes();
    let result = abf_parser::parse(&bytes, None, abf_parser::Limits::MAX);
    event(source, seq, SourceEvent::Record { bytes, result })
}
fn end(source: &str, seq: u64, count: u64) -> Payload {
    event(
        source,
        seq,
        SourceEvent::End {
            records: count,
            change: ChangeObservation::NoChangeObserved,
        },
    )
}
fn raw(payloads: Vec<Payload>, process: ProcessEnd) -> RawExecutionLog {
    RawExecutionLog {
        manifest: ExecutionManifest {
            attempt: AttemptRef::new("att1").unwrap(),
            finalization: FinalizationId::new("z1").unwrap(),
            members: vec![scope()],
            retry_of: None,
            subset: RetrySubset::AllExpected,
        },
        pre_barrier: payloads
            .into_iter()
            .enumerate()
            .map(|(i, p)| frame(i as u64 + 1, p))
            .collect(),
        process,
        barrier: Barrier::Drained,
        distribution_matches: true,
        times: TimeObservations::new(9, 1),
    }
}
fn membership<'a>(x: &'a FinalizedExecution, c: &CatalogSnapshot) -> ProjectionMembership<'a> {
    ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: None,
        },
        &x.members().iter().collect::<Vec<_>>(),
        c,
    )
    .unwrap()
}
const A: &str = "f1v1-i1";
const B: &str = "f1v1-i2";
fn baseline() -> Vec<Payload> {
    vec![
        initial(RawEnumeration::Closed(2), &[A, B]),
        record(A, 1, 100),
        end(A, 2, 1),
        end(B, 1, 0),
        Payload::RunEnd,
    ]
}
use abf_counting::{ComponentValue as Value, CoverageState as Cov, Qualification as Q};

#[test]
fn r01_literal_zero_record_declaration_and_conservation() {
    let c = catalog();
    let x = finalize(raw(baseline(), ProcessEnd::ExitedNormally), &c).unwrap();
    let m = membership(&x, &c);
    let p = project_with_companion(&m).unwrap();
    let w = p.projection().windows.selected().unwrap();
    assert_eq!(w.stored_occurrences.0, 1);
    assert_eq!(w.runs.run_keys, 2);
    assert_eq!(w.runs.raw_declared_claims.complete, 2);
    assert_eq!(w.runs.raw_record_claims.complete, 1);
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(100));
    assert_eq!(
        p.members().next().unwrap().verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(p.selected_refs(), m.selected_refs());
}

// Literals come from V1.7, independently of the adapter's generated inputs.
#[test]
fn r03_to_r07_r21_r31_r43_r95b_inventory_states_never_fabricate_a_zero_or_key() {
    for enumeration in [
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
        RawEnumeration::Closed(0),
        RawEnumeration::RootAbsent,
        RawEnumeration::RootUnresolved,
        RawEnumeration::LayoutUnrecognized,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::NotAttempted,
    ] {
        let c = catalog();
        let x = finalize(
            raw(vec![initial(enumeration, &[])], ProcessEnd::Crashed),
            &c,
        )
        .unwrap();
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(w.runs.run_keys, 0);
        assert_eq!(
            w.runs.raw_declared_claims,
            abf_counting::StatusCounts::default()
        );
        assert_eq!(w.stored_occurrences.0, 0);
        assert_eq!(w.runs.coverage, Cov::NoEvidence);
        assert_eq!(
            w.components.input.value,
            Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence)
        );
        assert!(w.components.input.evidence.qualifications.is_empty());
    }
}

#[test]
fn r36_r37_status_is_identical_on_records_and_declarations_without_repair() {
    for (terminal, status, flag) in [
        (
            SourceEvent::Skipped(StopReason::Cancelled),
            SourceRunStatus::NotCollected,
            Q::NotCollectedRun,
        ),
        (
            SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
            SourceRunStatus::Unavailable,
            Q::UnavailableRun,
        ),
        (
            SourceEvent::Vanished,
            SourceRunStatus::Unavailable,
            Q::UnavailableRun,
        ),
    ] {
        let c = catalog();
        let x = finalize(
            raw(
                vec![
                    initial(RawEnumeration::Closed(1), &[A]),
                    record(A, 1, 100),
                    event(A, 2, terminal),
                    Payload::RunEnd,
                ],
                ProcessEnd::ExitedNormally,
            ),
            &c,
        )
        .unwrap();
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        assert_eq!(x.members()[0].sources()[0].1.status(), status);
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(w.stored_occurrences.0, 1);
        assert_eq!(w.runs.run_keys, 1);
        assert_eq!(w.runs.raw_record_claims, w.runs.raw_declared_claims);
        assert_eq!(w.records.0.records_without_reported_traversal, 1);
        assert_eq!(w.runs.coverage, Cov::Inconsistent);
        assert_eq!(w.components.input.value, Value::ReportedClaimSum(100));
        assert!(w.components.input.evidence.qualifications.contains(flag));
        assert!(
            w.components
                .input
                .evidence
                .qualifications
                .contains(Q::RecordWithoutReportedTraversal)
        );
    }
}

#[test]
fn r08_r11_m5_never_omit_untraversed_source() {
    for (terminal, status, cov, value) in [
        (
            SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
            SourceRunStatus::Unavailable,
            Cov::ReportedIncomplete,
            100,
        ),
        (
            SourceEvent::ReadError(PartialReason::Io),
            SourceRunStatus::Partial,
            Cov::ReportedIncomplete,
            100,
        ),
        (
            SourceEvent::Skipped(StopReason::Cancelled),
            SourceRunStatus::NotCollected,
            Cov::ReportedIncomplete,
            100,
        ),
        (
            SourceEvent::ReadError(PartialReason::Cancelled),
            SourceRunStatus::Partial,
            Cov::ReportedIncomplete,
            100,
        ),
    ] {
        let c = catalog();
        let x = finalize(
            raw(
                vec![
                    initial(RawEnumeration::Closed(2), &[A, B]),
                    record(A, 1, 100),
                    end(A, 2, 1),
                    event(B, 1, terminal),
                    Payload::RunEnd,
                ],
                ProcessEnd::ExitedNormally,
            ),
            &c,
        )
        .unwrap();
        assert_eq!(x.members()[0].sources()[1].1.status(), status);
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(w.runs.run_keys, 2);
        assert_eq!(w.runs.resolved.complete, 1);
        assert_eq!(w.runs.coverage, cov);
        assert_eq!(w.components.input.value, Value::ReportedClaimSum(value));
    }
}

#[test]
fn r13_late_delivery_weakens_member_assessment_without_changing_projection() {
    let c = catalog();
    let x = finalize(raw(baseline(), ProcessEnd::ExitedNormally), &c).unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let before = *p.projection();
    let mut log = PostFinalizationLog::bind(&x);
    let original = p.assess(&[log.snapshot()]).unwrap();
    assert_eq!(original.members()[0].ceiling, PresentationCeiling::Original);
    log.append(frame(6, record(A, 3, 7)), SupervisorInstant::synthetic(10))
        .unwrap();
    let assessed = p.assess(&[log.snapshot()]).unwrap();
    assert_eq!(
        assessed.members()[0].ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert_eq!(
        assessed.members()[0].verdict,
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(
        assessed.members()[0].post,
        PostFinalization::Observed {
            records: 1,
            other: 0,
            conflicts: 0,
            unattributed: 0
        }
    );
    assert_eq!(p.projection(), &before);
    assert_eq!(
        project_with_companion(&membership(&x, &c))
            .unwrap()
            .projection(),
        &before
    );
}

fn direct(
    declarations: &[(&str, SourceRunStatus)],
    records: &[(&str, u32, SourceRunStatus)],
) -> abf_counting::Projection {
    use abf_identity_relations::{Limits, Occurrence, SyntheticSet};
    use abf_observation::{DeclaredContext, from_opencode};
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    for (i, (source, input, status)) in records.iter().enumerate() {
        let json = format!("{{\"tokens\":{{\"input\":{input}}}}}");
        let result = abf_parser::parse(json.as_bytes(), None, abf_parser::Limits::MAX);
        let context = DeclaredContext::new(
            EnvironmentKind::Windows,
            OpaqueRef::new("w1").unwrap(),
            OpaqueRef::new(source).unwrap(),
            OpaqueRef::new("att1").unwrap(),
            None,
            None,
        );
        set.insert(Occurrence::new(
            OpaqueRef::new(&format!("independent{i}")).unwrap(),
            from_opencode(result, context, *status).unwrap(),
        ))
        .unwrap();
    }
    let declarations: Vec<_> = declarations
        .iter()
        .map(|(source, status)| abf_counting::RunDeclaration {
            environment_kind: EnvironmentKind::Windows,
            environment_ref: OpaqueRef::new("w1").unwrap(),
            source_ref: OpaqueRef::new(source).unwrap(),
            run_ref: OpaqueRef::new("att1").unwrap(),
            status: *status,
        })
        .collect();
    abf_counting::project(
        &set,
        abf_counting::RunInventory::Declared(&declarations),
        Selection::WindowsOnly,
    )
    .unwrap()
}

fn literal_case(
    payloads: Vec<Payload>,
    process: ProcessEnd,
    declarations: &[(&str, SourceRunStatus)],
    records: &[(&str, u32, SourceRunStatus)],
    coverage: Cov,
    value: Value,
    flags: u32,
) {
    literal_raw(
        raw(payloads, process),
        declarations,
        records,
        coverage,
        value,
        flags,
    );
}

fn literal_raw(
    raw: RawExecutionLog,
    declarations: &[(&str, SourceRunStatus)],
    records: &[(&str, u32, SourceRunStatus)],
    coverage: Cov,
    value: Value,
    flags: u32,
) {
    let c = catalog();
    let x = finalize(raw, &c).unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let w = p.projection().windows.selected().unwrap();
    assert_eq!(w.stored_occurrences.0, records.len() as u32);
    assert_eq!(w.runs.run_keys, declarations.len() as u32);
    assert_eq!(w.runs.coverage, coverage);
    assert_eq!(w.components.input.value, value);
    assert_eq!(w.components.input.evidence.qualifications.bits(), flags);
    assert_eq!(*p.projection(), direct(declarations, records));
    assert_eq!(x.members()[0].records().len(), records.len());
}

#[test]
fn r02_r12_r16_r20_r95_literals_and_independent_differential() {
    use SourceRunStatus::{Complete, Partial};
    literal_case(
        vec![
            initial(RawEnumeration::Partial(1, StopReason::Budget), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete)],
        &[(A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
        ],
        ProcessEnd::Crashed,
        &[(A, Complete)],
        &[(A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
    for change in [
        ChangeObservation::Grew,
        ChangeObservation::ShrankOrReplaced,
        ChangeObservation::NotObservable,
    ] {
        let (status, cov, flags) = match change {
            ChangeObservation::NotObservable => (Complete, Cov::ReportedComplete, 0),
            _ => (Partial, Cov::ReportedIncomplete, Q::PartialRun as u32),
        };
        literal_case(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                event(A, 2, SourceEvent::End { records: 1, change }),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
            &[(A, status)],
            &[(A, 100, status)],
            cov,
            Value::ReportedClaimSum(100),
            flags,
        );
    }
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 0),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete)],
        &[(A, 0, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(0),
        0,
    );
}

#[test]
fn r09_r42b_r56_failed_reads_and_interrupted_opens_keep_exact_values() {
    use SourceRunStatus::{Complete, Failed, Partial};
    literal_case(
        vec![
            initial(RawEnumeration::Closed(2), &[A, B]),
            record(A, 1, 100),
            end(A, 2, 1),
            record(B, 1, 40),
        ],
        ProcessEnd::Crashed,
        &[(A, Complete), (B, Failed)],
        &[(A, 100, Complete), (B, 40, Failed)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(140),
        Q::FailedRun as u32,
    );
    for (process, status, flag) in [
        (ProcessEnd::Crashed, Failed, Q::FailedRun),
        (ProcessEnd::KilledTimeout, Failed, Q::FailedRun),
        (ProcessEnd::TransportLost, Failed, Q::FailedRun),
        (ProcessEnd::KilledCancel, Partial, Q::PartialRun),
    ] {
        literal_case(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                event(A, 1, SourceEvent::OpenAttempt),
            ],
            process,
            &[(A, status)],
            &[],
            Cov::ReportedIncomplete,
            Value::NoValidClaims,
            flag as u32,
        );
        literal_case(
            vec![initial(RawEnumeration::Closed(1), &[A]), record(A, 1, 100)],
            process,
            &[(A, status)],
            &[(A, 100, status)],
            Cov::ReportedIncomplete,
            Value::ReportedClaimSum(100),
            flag as u32,
        );
    }
}

#[test]
fn r10_r54b_no_traversal_is_indeterminate_without_records() {
    use SourceRunStatus::{NotCollected, Unavailable};
    literal_case(
        vec![initial(RawEnumeration::Closed(2), &[A, B])],
        ProcessEnd::KilledTimeout,
        &[(A, NotCollected), (B, NotCollected)],
        &[],
        Cov::NoTraversalReported,
        Value::Indeterminate(abf_counting::IndeterminateReason::NoTraversalReported),
        Q::NotCollectedRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            event(A, 1, SourceEvent::Skipped(StopReason::Budget)),
            event(A, 2, SourceEvent::OpenFailed(OpenFailure::Io)),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Unavailable)],
        &[],
        Cov::NoTraversalReported,
        Value::Indeterminate(abf_counting::IndeterminateReason::NoTraversalReported),
        Q::UnavailableRun as u32,
    );
}

#[test]
fn r35_r38_r39_r40_r42_r54_r55_r57_r59_anomalies_keep_literals_and_claims() {
    use SourceRunStatus::{Complete, NotCollected, Partial, Unavailable};
    literal_case(
        vec![
            initial(RawEnumeration::Closed(2), &[A, B]),
            record(A, 1, 10),
            end(A, 2, 1),
            record(B, 1, 100),
            event(B, 2, SourceEvent::Skipped(StopReason::Cancelled)),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete), (B, NotCollected)],
        &[(A, 10, Complete), (B, 100, NotCollected)],
        Cov::Inconsistent,
        Value::ReportedClaimSum(110),
        (Q::NotCollectedRun as u32) | (Q::RecordWithoutReportedTraversal as u32),
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            record(A, 3, 50),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 100, Partial), (A, 50, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(150),
        Q::PartialRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            end(A, 3, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 100, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(100),
        Q::PartialRun as u32,
    );
    for first in [
        end(A, 2, 1),
        event(A, 2, SourceEvent::Skipped(StopReason::Budget)),
    ] {
        literal_case(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                first,
                event(A, 3, SourceEvent::OpenFailed(OpenFailure::PermissionDenied)),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
            &[(A, Unavailable)],
            &[(A, 100, Unavailable)],
            Cov::Inconsistent,
            Value::ReportedClaimSum(100),
            (Q::UnavailableRun as u32) | (Q::RecordWithoutReportedTraversal as u32),
        );
    }
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 100, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(100),
        Q::PartialRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            end(A, 1, 0),
            end(A, 2, 0),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[],
        Cov::ReportedIncomplete,
        Value::NoValidClaims,
        Q::PartialRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            event(A, 1, SourceEvent::OpenAttempt),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[],
        Cov::ReportedIncomplete,
        Value::NoValidClaims,
        Q::PartialRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            record(A, 1, 100),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 100, Partial), (A, 100, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(200),
        Q::PartialRun as u32,
    );
}

#[test]
fn r15_delivery_loss_and_n24_equal_content_distinct_identities_are_not_merged() {
    use SourceRunStatus::{Complete, Partial};
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 10),
            record(A, 2, 20),
            end(A, 4, 3),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 10, Partial), (A, 20, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(30),
        Q::PartialRun as u32,
    );
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            record(A, 2, 100),
            end(A, 3, 2),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete)],
        &[(A, 100, Complete), (A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(200),
        0,
    );
}

#[test]
fn r14a_r14b_r49_r50_r51_r52_r53_r53b_pq_literal_projections() {
    use SourceRunStatus::{Complete, NotCollected, Partial};
    let mut replay = raw(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    replay.pre_barrier.push(replay.pre_barrier[1].clone());
    literal_raw(
        replay.clone(),
        &[(A, Complete)],
        &[(A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
    replay.pre_barrier.last_mut().unwrap().payload = record(A, 1, 90);
    literal_raw(
        replay,
        &[(A, Partial)],
        &[],
        Cov::ReportedIncomplete,
        Value::NoValidClaims,
        Q::PartialRun as u32,
    );
    for terminal in [
        end(A, 1, 0),
        event(A, 1, SourceEvent::Skipped(StopReason::Budget)),
    ] {
        let skipped = matches!(
            terminal,
            Payload::Source {
                event: SourceEvent::Skipped(_),
                ..
            }
        );
        let mut r = raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                terminal,
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        );
        r.pre_barrier.push(frame(2, record(A, 1, 90)));
        let (status, cov, value, flag) = if skipped {
            (
                NotCollected,
                Cov::NoTraversalReported,
                Value::Indeterminate(abf_counting::IndeterminateReason::NoTraversalReported),
                Q::NotCollectedRun,
            )
        } else {
            (
                Partial,
                Cov::ReportedIncomplete,
                Value::NoValidClaims,
                Q::PartialRun,
            )
        };
        literal_raw(r, &[(A, status)], &[], cov, value, flag as u32);
    }
    let mut r = raw(
        vec![
            initial(RawEnumeration::Closed(2), &[A, B]),
            record(A, 1, 100),
            end(A, 2, 1),
            end(B, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    r.pre_barrier.push(frame(2, record(B, 1, 100)));
    literal_raw(
        r,
        &[(A, Partial), (B, Partial)],
        &[],
        Cov::ReportedIncomplete,
        Value::NoValidClaims,
        Q::PartialRun as u32,
    );
    let mut r = raw(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    r.pre_barrier.push(frame(2, end(A, 1, 0)));
    literal_raw(
        r,
        &[(A, Partial)],
        &[],
        Cov::ReportedIncomplete,
        Value::NoValidClaims,
        Q::PartialRun as u32,
    );
    for with_record in [false, true] {
        let mut p = vec![initial(RawEnumeration::Closed(1), &[A])];
        if with_record {
            p.push(record(A, 1, 100));
        }
        p.push(end(A, 2, u64::from(with_record)));
        let mut r = raw(p, ProcessEnd::ExitedNormally);
        r.pre_barrier.push(r.pre_barrier.last().unwrap().clone());
        r.pre_barrier.push(frame(4, Payload::RunEnd));
        let records = if with_record {
            vec![(A, 100, Complete)]
        } else {
            vec![]
        };
        let value = if with_record {
            Value::ReportedClaimSum(100)
        } else {
            Value::NoValidClaims
        };
        literal_raw(
            r,
            &[(A, Complete)],
            &records,
            Cov::ReportedComplete,
            value,
            0,
        );
    }
}

fn second(outcome: RawEnumeration, sources: &[&str]) -> Payload {
    let mut p = initial(outcome, sources);
    let Payload::Enumeration { second, .. } = &mut p else {
        unreachable!()
    };
    *second = true;
    p
}

#[test]
fn r73_to_r87_r97_second_listings_and_protocol_anomalies_leave_base_g3b_values() {
    use SourceRunStatus::Complete;
    for se in [
        second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"]),
        second(RawEnumeration::Closed(2), &[A, B]),
        second(RawEnumeration::Closed(1), &[A]),
        second(
            RawEnumeration::Partial(2, StopReason::Budget),
            &[A, "f1v1-i3"],
        ),
        second(RawEnumeration::Partial(1, StopReason::Budget), &[A]),
        second(RawEnumeration::Failed(EnumFailure::PermissionDenied), &[]),
    ] {
        let mut p = baseline();
        p.push(se);
        literal_case(
            p,
            ProcessEnd::ExitedNormally,
            &[(A, Complete), (B, Complete)],
            &[(A, 100, Complete)],
            Cov::ReportedComplete,
            Value::ReportedClaimSum(100),
            0,
        );
    }
    // R80, R81, R82: the first enumeration, never the second, owns declarations.
    literal_case(
        vec![
            initial(RawEnumeration::Partial(1, StopReason::Budget), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            second(RawEnumeration::Closed(2), &[A, B]),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete)],
        &[(A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
    for e in [
        RawEnumeration::Closed(0),
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
    ] {
        literal_case(
            vec![
                initial(e, &[]),
                second(RawEnumeration::Closed(1), &[A]),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
            &[],
            &[],
            Cov::NoEvidence,
            Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence),
            0,
        );
    }
    let mut p = baseline();
    p.push(second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"]));
    p.push(record("f1v1-i3", 1, 50));
    literal_case(
        p,
        ProcessEnd::ExitedNormally,
        &[(A, Complete), (B, Complete)],
        &[(A, 100, Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
    for variant in [0, 1, 2] {
        let mut r = raw(baseline(), ProcessEnd::ExitedNormally);
        r.pre_barrier.push(frame(
            6,
            second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"]),
        ));
        let (seq, payload) = match variant {
            0 => (6, second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"])),
            1 => (6, second(RawEnumeration::Closed(2), &[A, B])),
            _ => (7, second(RawEnumeration::Closed(2), &[A, B])),
        };
        r.pre_barrier.push(frame(seq, payload));
        literal_raw(
            r,
            &[(A, Complete), (B, Complete)],
            &[(A, 100, Complete)],
            Cov::ReportedComplete,
            Value::ReportedClaimSum(100),
            0,
        );
    }
}

#[test]
fn r98_r99_r100_conflicted_enumerations_never_fabricate_keys_or_claims() {
    let none = Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence);
    // R98a/b: two initial identities with equal or unequal listings.
    for first in [
        initial(RawEnumeration::Closed(0), &[]),
        initial(RawEnumeration::Closed(1), &[A]),
    ] {
        literal_case(
            vec![
                first,
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                end(A, 2, 1),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
            &[],
            &[],
            Cov::NoEvidence,
            none,
            0,
        );
    }
    // R99a/b/d(ii,iii), R100a/d: no initial set, regardless of second anomaly.
    for e in [
        RawEnumeration::NotAttempted,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
    ] {
        for conflict in [false, true] {
            let mut r = raw(
                vec![
                    initial(e, &[]),
                    second(RawEnumeration::Closed(0), &[]),
                    Payload::RunEnd,
                ],
                ProcessEnd::ExitedNormally,
            );
            r.pre_barrier.push(frame(
                if conflict { 2 } else { 4 },
                second(RawEnumeration::RootAbsent, &[]),
            ));
            literal_raw(r, &[], &[], Cov::NoEvidence, none, 0);
        }
    }
    // R98d, R100c: initial D2, optionally mixed with a second variant.
    for second_kind in [false, true] {
        let mut r = raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                end(A, 2, 1),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        );
        let variant = if second_kind {
            second(RawEnumeration::Closed(1), &[A])
        } else {
            initial(RawEnumeration::Closed(0), &[])
        };
        r.pre_barrier.push(frame(1, variant));
        literal_raw(r, &[], &[], Cov::NoEvidence, none, 0);
    }
    // R100e: two admitted second identities and one D2 leave R01 verbatim.
    let mut r = raw(baseline(), ProcessEnd::ExitedNormally);
    r.pre_barrier.extend([
        frame(6, second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"])),
        frame(7, second(RawEnumeration::Closed(2), &[A, B])),
        frame(8, second(RawEnumeration::Closed(1), &[A])),
        frame(8, second(RawEnumeration::Closed(0), &[])),
    ]);
    literal_raw(
        r,
        &[
            (A, SourceRunStatus::Complete),
            (B, SourceRunStatus::Complete),
        ],
        &[(A, 100, SourceRunStatus::Complete)],
        Cov::ReportedComplete,
        Value::ReportedClaimSum(100),
        0,
    );
}

#[test]
fn r102_r103_unexpected_and_unbound_deliveries_never_enter_numeric_projection() {
    use SourceRunStatus::Complete;
    let mut builder = CatalogBuilder::new();
    builder.add(scope(), "f1v1").unwrap();
    let peer = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    builder.add(peer.clone(), "f2v1").unwrap();
    builder
        .add(
            CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap(),
            "l1f1v1",
        )
        .unwrap();
    let c = builder.build();
    for source in ["f1v1-i9", "f2v1-i1", "l1f1v1-i1", "unk9-i1"] {
        for covered in [false, true] {
            for peers in [false, true] {
                let payloads = if covered {
                    vec![
                        initial(RawEnumeration::Closed(1), &[A]),
                        record(A, 1, 100),
                        end(A, 2, 1),
                        record(source, 1, 50),
                        Payload::RunEnd,
                    ]
                } else {
                    vec![
                        initial(RawEnumeration::Closed(0), &[]),
                        record(source, 1, 50),
                        Payload::RunEnd,
                    ]
                };
                let mut r = raw(payloads, ProcessEnd::ExitedNormally);
                if peers {
                    r.manifest.members.push(peer.clone());
                }
                let x = finalize(r, &c).unwrap();
                let paired = project_with_companion(&membership(&x, &c)).unwrap();
                let w = paired.projection().windows.selected().unwrap();
                let (decls, records, cov, value) = if covered {
                    (
                        vec![(A, Complete)],
                        vec![(A, 100, Complete)],
                        Cov::ReportedComplete,
                        Value::ReportedClaimSum(100),
                    )
                } else {
                    (
                        vec![],
                        vec![],
                        Cov::NoEvidence,
                        Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence),
                    )
                };
                assert_eq!(*paired.projection(), direct(&decls, &records));
                assert_eq!(w.runs.coverage, cov);
                assert_eq!(w.components.input.value, value);
                assert_eq!(w.runs.run_keys, u32::from(covered));
                assert_eq!(w.stored_occurrences.0, u32::from(covered));
                assert!(w.components.input.evidence.qualifications.is_empty());
            }
        }
    }
    // R102g/h: repeated unexpected deliveries, distinct identities and D2 variants.
    for mode in [0, 1, 2] {
        let mut r = raw(
            vec![
                initial(RawEnumeration::Closed(0), &[]),
                record("unk9-i1", 1, 50),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        );
        r.pre_barrier.push(frame(
            if mode == 1 { 4 } else { 2 },
            record("unk9-i1", 1, if mode == 2 { 20 } else { 50 }),
        ));
        let x = finalize(r, &c).unwrap();
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        assert_eq!(*p.projection(), direct(&[], &[]));
    }
    // R103a-k adapter population: no INITIAL plus record, control, second,
    // global conflict, gap, mismatch or late evidence still has no key or value.
    for event in [
        record("f1v1-i9", 1, 50),
        event("f1v1-i9", 1, SourceEvent::OpenAttempt),
        end("f1v1-i9", 1, 0),
        Payload::RunEnd,
    ] {
        for count in [0, 1, 2] {
            let mut payloads = vec![event.clone()];
            payloads.extend(std::iter::repeat_n(
                second(RawEnumeration::Closed(0), &[]),
                count,
            ));
            payloads.push(Payload::RunEnd);
            let mut r = raw(payloads, ProcessEnd::ExitedNormally);
            r.manifest.members.push(peer.clone());
            let x = finalize(r, &c).unwrap();
            let p = project_with_companion(&membership(&x, &c)).unwrap();
            assert_eq!(*p.projection(), direct(&[], &[]));
        }
    }
}

#[test]
fn r19_rejected_records_are_stored_and_zero_claims_remain_distinct_from_no_claims() {
    let bytes = b"not json".to_vec();
    let rejected = event(
        A,
        2,
        SourceEvent::Record {
            result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
            bytes,
        },
    );
    let c = catalog();
    let x = finalize(
        raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                rejected,
                end(A, 3, 2),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        ),
        &c,
    )
    .unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let w = p.projection().windows.selected().unwrap();
    assert_eq!(w.stored_occurrences.0, 2);
    assert_eq!(
        w.outcomes.0,
        abf_counting::OutcomeCounts {
            accepted: 1,
            rejected: 1,
            ..Default::default()
        }
    );
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(100));
    assert_eq!(
        w.components.input.evidence.qualifications.bits(),
        Q::RejectedRecordsPresent as u32
    );
    let x = finalize(
        raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 0),
                end(A, 2, 1),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        ),
        &c,
    )
    .unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let e = p.projection().windows.selected().unwrap().components.input;
    assert_eq!(e.value, Value::ReportedClaimSum(0));
    assert_eq!(e.evidence.contributing, 1);
    assert_eq!(e.evidence.zero_valued, 1);
    assert_eq!(
        p.projection().computer.physical_events,
        abf_counting::PhysicalEventCount::Unresolved
    );
    assert_eq!(
        p.projection().computer.usage,
        abf_counting::UsageTotal::Unsupported
    );
}

#[test]
fn n14_n15_permutations_and_listing_order_keep_projection_identical() {
    let c = catalog();
    let r = raw(baseline(), ProcessEnd::ExitedNormally);
    let x = finalize(r.clone(), &c).unwrap();
    let expected = *project_with_companion(&membership(&x, &c))
        .unwrap()
        .projection();
    fn permutations(
        frames: &mut [ReaderFrame],
        at: usize,
        c: &CatalogSnapshot,
        r: &RawExecutionLog,
        expected: &abf_counting::Projection,
    ) {
        if at == frames.len() {
            let mut r = r.clone();
            r.pre_barrier = frames.to_vec();
            let x = finalize(r, c).unwrap();
            assert_eq!(
                project_with_companion(&membership(&x, c))
                    .unwrap()
                    .projection(),
                expected
            );
            return;
        }
        for i in at..frames.len() {
            frames.swap(i, at);
            permutations(frames, at + 1, c, r, expected);
            frames.swap(i, at);
        }
    }
    permutations(&mut r.pre_barrier.clone(), 0, &c, &r, &expected);
    let mut r = r;
    let Payload::Enumeration { sources, .. } = &mut r.pre_barrier[0].payload else {
        unreachable!()
    };
    sources.reverse();
    let x = finalize(r, &c).unwrap();
    assert_eq!(
        project_with_companion(&membership(&x, &c))
            .unwrap()
            .projection(),
        &expected
    );
}

#[test]
fn n12_n40_n41_capacity_127_128_129_is_independent_and_refusal_has_no_result() {
    for n in [127, 128, 129] {
        for declarations_only in [false, true] {
            let sources: Vec<String> = (1..=if declarations_only { n } else { 1 })
                .map(|i| format!("f1v1-i{i}"))
                .collect();
            let names: Vec<&str> = sources.iter().map(String::as_str).collect();
            let mut p = vec![initial(RawEnumeration::Closed(names.len() as u64), &names)];
            if !declarations_only {
                p.extend((1..=n).map(|i| record(A, i as u64, 1)));
            }
            let c = catalog();
            let x = finalize(raw(p, ProcessEnd::Crashed), &c).unwrap();
            let mut calls = 0;
            let result = ProjectionMembership::validate(
                Selection::WindowsOnly,
                SelectedRefs {
                    windows: Some(OpaqueRef::new("w1").unwrap()),
                    selected_wsl: None,
                },
                &[&x.members()[0]],
                &c,
            )
            .map(|m| {
                calls += 1;
                project_with_companion(&m).unwrap()
            });
            if n == 129 {
                assert_eq!(
                    result.unwrap_err(),
                    if declarations_only {
                        AdaptCode::DeclarationCeiling
                    } else {
                        AdaptCode::OccurrenceCeiling
                    }
                );
                assert_eq!(calls, 0);
            } else {
                let p = result.unwrap();
                let w = p.projection().windows.selected().unwrap();
                assert_eq!(calls, 1);
                assert_eq!(
                    w.runs.run_keys,
                    if declarations_only { n as u32 } else { 1 }
                );
                assert_eq!(
                    w.stored_occurrences.0,
                    if declarations_only { 0 } else { n as u32 }
                );
                assert_eq!(
                    w.components.input.value,
                    if declarations_only {
                        Value::Indeterminate(abf_counting::IndeterminateReason::NoTraversalReported)
                    } else {
                        Value::ReportedClaimSum(n as u128)
                    }
                );
            }
            assert_eq!(
                x.members()[0].records().len(),
                if declarations_only { 0 } else { n }
            );
            assert_eq!(
                x.members()[0].sources().len(),
                if declarations_only { n } else { 1 }
            );
        }
    }
}

fn external_set(records: &[(&str, &str, SourceRunStatus)]) -> abf_identity_relations::SyntheticSet {
    use abf_identity_relations::{Limits, Occurrence, SyntheticSet};
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    for (i, (source, attempt, status)) in records.iter().enumerate() {
        let result = abf_parser::parse(br#"{"tokens":{"input":1}}"#, None, abf_parser::Limits::MAX);
        let context = abf_observation::DeclaredContext::new(
            EnvironmentKind::Windows,
            OpaqueRef::new("w1").unwrap(),
            OpaqueRef::new(source).unwrap(),
            OpaqueRef::new(attempt).unwrap(),
            None,
            None,
        );
        let e = abf_observation::from_opencode(result, context, *status).unwrap();
        set.insert(Occurrence::new(
            OpaqueRef::new(&format!("raw{i}")).unwrap(),
            e,
        ))
        .unwrap();
    }
    set
}
fn declaration(
    source: &str,
    attempt: &str,
    status: SourceRunStatus,
) -> abf_counting::RunDeclaration {
    abf_counting::RunDeclaration {
        environment_kind: EnvironmentKind::Windows,
        environment_ref: OpaqueRef::new("w1").unwrap(),
        source_ref: OpaqueRef::new(source).unwrap(),
        run_ref: OpaqueRef::new(attempt).unwrap(),
        status,
    }
}

#[test]
fn ce1_ce2_ce3_ce4_ce7_raw_g3b_distinguishes_collision_and_repeated_scan_hazards() {
    use SourceRunStatus::{Complete, Failed};
    use abf_counting::{RunInventory, project};
    let set = external_set(&[(A, "att1", Complete), (A, "att1", Complete)]);
    let p = project(&set, RunInventory::Undeclared, Selection::WindowsOnly).unwrap();
    let w = p.windows.selected().unwrap();
    assert_eq!(w.runs.run_keys, 1);
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(2));
    assert_eq!(
        project(
            &set,
            RunInventory::Declared(&[
                declaration(A, "att1", Complete),
                declaration(A, "att1", Complete)
            ]),
            Selection::WindowsOnly
        ),
        Err(abf_counting::Code::DuplicateRunDeclaration)
    );
    // CE2: incompatible statuses, one reused key, retained claims still summed.
    let set = external_set(&[(A, "att1", Complete), (A, "att1", Failed)]);
    let p = project(&set, RunInventory::Undeclared, Selection::WindowsOnly).unwrap();
    let w = p.windows.selected().unwrap();
    assert_eq!(w.runs.coverage, Cov::Inconsistent);
    assert_eq!(w.runs.conflicting_keys, 1);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(2));
    // CE4: a forged Complete declaration is accepted by raw G3B without G4 authority.
    let set = external_set(&[]);
    let p = project(
        &set,
        RunInventory::Declared(&[declaration(A, "att9", Complete)]),
        Selection::WindowsOnly,
    )
    .unwrap();
    let w = p.windows.selected().unwrap();
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::NoValidClaims);
    // CE7: two different keys need no key-collision signal. A4/A5 are separate guards.
    let set = external_set(&[(A, "att1", Complete), (A, "att2", Complete)]);
    let p = project(
        &set,
        RunInventory::Declared(&[
            declaration(A, "att1", Complete),
            declaration(A, "att2", Complete),
        ]),
        Selection::WindowsOnly,
    )
    .unwrap();
    let w = p.windows.selected().unwrap();
    assert_eq!(w.runs.run_keys, 2);
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(2));
    assert!(w.components.input.evidence.qualifications.is_empty());
}

#[test]
fn ce5_omitting_untraversed_declaration_falsely_promotes_coverage() {
    use SourceRunStatus::{Complete, NotCollected};
    let set = external_set(&[(A, "att1", Complete)]);
    let p = abf_counting::project(
        &set,
        abf_counting::RunInventory::Declared(&[
            declaration(A, "att1", Complete),
            declaration(B, "att1", NotCollected),
        ]),
        Selection::WindowsOnly,
    )
    .unwrap();
    assert_eq!(
        p.windows.selected().unwrap().runs.coverage,
        Cov::ReportedIncomplete
    );
    let p = abf_counting::project(
        &set,
        abf_counting::RunInventory::Declared(&[declaration(A, "att1", Complete)]),
        Selection::WindowsOnly,
    )
    .unwrap();
    assert_eq!(
        p.windows.selected().unwrap().runs.coverage,
        Cov::ReportedComplete
    );
}

#[test]
fn ce6_ce8_ce9_n1_reference_consumer_respects_companion_and_assessment() {
    fn may_present_complete(p: &PairedProjection<'_>, assessment: &PairedAssessment) -> bool {
        p.members().all(|m| {
            matches!(
                m.verdict(),
                CoverageVerdict::CoveredWithinEnumeration { .. }
            )
        }) && !assessment.members().is_empty()
            && assessment
                .members()
                .iter()
                .all(|a| a.ceiling == PresentationCeiling::Original)
    }
    for payloads in [
        vec![
            initial(RawEnumeration::Partial(1, StopReason::Budget), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
            Payload::RunEnd,
        ],
        vec![initial(
            RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
            &[],
        )],
        vec![initial(RawEnumeration::Closed(0), &[])],
    ] {
        let c = catalog();
        let x = finalize(raw(payloads, ProcessEnd::ExitedNormally), &c).unwrap();
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        assert!(!may_present_complete(
            &p,
            &p.assess(&[PostFinalizationLog::bind(&x).snapshot()])
                .unwrap()
        ));
    }
    let c = catalog();
    let x = finalize(raw(baseline(), ProcessEnd::ExitedNormally), &c).unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let mut log = PostFinalizationLog::bind(&x);
    assert!(may_present_complete(
        &p,
        &p.assess(&[log.snapshot()]).unwrap()
    ));
    log.append(
        frame(6, record(A, 3, 999)),
        SupervisorInstant::synthetic(99),
    )
    .unwrap();
    assert!(!may_present_complete(
        &p,
        &p.assess(&[log.snapshot()]).unwrap()
    ));
}

fn environment_execution(
    scope: CatalogScope,
    c: &CatalogSnapshot,
    attempt: &str,
    finalization: &str,
    available: bool,
) -> FinalizedExecution {
    let source = format!("{}-i1", c.scope_tag(&scope).unwrap());
    let mut r = raw(
        if available {
            vec![
                initial(RawEnumeration::Closed(1), &[&source]),
                record(&source, 1, 100),
                end(&source, 2, 1),
                Payload::RunEnd,
            ]
        } else {
            vec![initial(
                RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
                &[],
            )]
        },
        ProcessEnd::ExitedNormally,
    );
    r.manifest.members = vec![scope];
    r.manifest.attempt = AttemptRef::new(attempt).unwrap();
    r.manifest.finalization = FinalizationId::new(finalization).unwrap();
    for frame in &mut r.pre_barrier {
        frame.attempt = r.manifest.attempt.clone();
    }
    finalize(r, c).unwrap()
}

#[test]
fn r21_r24_x7_x11_environment_selection_and_independent_assessments() {
    let wscope = scope();
    let lscope = CatalogScope::new(EnvironmentKind::SelectedWsl, "w1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(wscope.clone(), "f1v1").unwrap();
    b.add(lscope.clone(), "l1f1v1").unwrap();
    let c = b.build();
    for available in [false, true] {
        let w = environment_execution(wscope.clone(), &c, "win", "z1", true);
        let l = environment_execution(lscope.clone(), &c, "linux", "z2", available);
        let refs = SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: Some(OpaqueRef::new("w1").unwrap()),
        };
        for order in [
            vec![&w.members()[0], &l.members()[0]],
            vec![&l.members()[0], &w.members()[0]],
        ] {
            let membership =
                ProjectionMembership::validate(Selection::Both, refs.clone(), &order, &c).unwrap();
            let p = project_with_companion(&membership).unwrap();
            assert_eq!(p.members().count(), 2);
            assert_eq!(p.selection(), Selection::Both);
            assert_eq!(p.selected_refs(), &refs);
            let g = p.projection();
            assert_eq!(
                g.windows.selected().unwrap().components.input.value,
                Value::ReportedClaimSum(100)
            );
            assert_eq!(
                g.selected_wsl.selected().unwrap().components.input.value,
                if available {
                    Value::ReportedClaimSum(100)
                } else {
                    Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence)
                }
            );
            assert_eq!(
                g.computer.components.input.value,
                Value::ReportedClaimSum(if available { 200 } else { 100 })
            );
            assert!(
                g.computer
                    .components
                    .input
                    .evidence
                    .qualifications
                    .contains(if available {
                        Q::CrossEnvironmentSum
                    } else {
                        Q::EnvironmentIndeterminate
                    })
            );
            assert_eq!(
                g.computer.components.input.evidence.identity,
                abf_counting::IdentityBasis::Unresolved
            );
            assert_eq!(
                g.computer.physical_events,
                abf_counting::PhysicalEventCount::Unresolved
            );
            // An independent raw set uses the explicit catalog labels and attempts.
            let mut set =
                abf_identity_relations::SyntheticSet::new(abf_identity_relations::Limits::MAX)
                    .unwrap();
            let mut decl = Vec::new();
            let mut specs = vec![(EnvironmentKind::Windows, "f1v1-i1", "win")];
            if available {
                specs.push((EnvironmentKind::SelectedWsl, "l1f1v1-i1", "linux"));
            }
            for (i, (kind, source, attempt)) in specs.into_iter().enumerate() {
                let context = abf_observation::DeclaredContext::new(
                    kind,
                    OpaqueRef::new("w1").unwrap(),
                    OpaqueRef::new(source).unwrap(),
                    OpaqueRef::new(attempt).unwrap(),
                    None,
                    None,
                );
                let result = abf_parser::parse(
                    br#"{"tokens":{"input":100}}"#,
                    None,
                    abf_parser::Limits::MAX,
                );
                set.insert(abf_identity_relations::Occurrence::new(
                    OpaqueRef::new(&format!("e{i}")).unwrap(),
                    abf_observation::from_opencode(result, context, SourceRunStatus::Complete)
                        .unwrap(),
                ))
                .unwrap();
                decl.push(abf_counting::RunDeclaration {
                    environment_kind: kind,
                    environment_ref: OpaqueRef::new("w1").unwrap(),
                    source_ref: OpaqueRef::new(source).unwrap(),
                    run_ref: OpaqueRef::new(attempt).unwrap(),
                    status: SourceRunStatus::Complete,
                });
            }
            assert_eq!(
                *g,
                abf_counting::project(
                    &set,
                    abf_counting::RunInventory::Declared(&decl),
                    Selection::Both
                )
                .unwrap()
            );
            if available {
                let before = *g;
                set.assert_relation(
                    OpaqueRef::new("e0").unwrap(),
                    OpaqueRef::new("e1").unwrap(),
                    abf_identity_relations::UnverifiedHint::PossibleReplayOrCopy,
                )
                .unwrap();
                let with_hint = abf_counting::project(
                    &set,
                    abf_counting::RunInventory::Declared(&decl),
                    Selection::Both,
                )
                .unwrap();
                assert_eq!(with_hint.computer, before.computer);
                assert_eq!(with_hint.windows, before.windows);
                assert_eq!(with_hint.selected_wsl, before.selected_wsl);
                assert_eq!(
                    with_hint.relations.verification,
                    abf_identity_relations::Verification::Unverified
                );
            }
            let mut wlog = PostFinalizationLog::bind(&w);
            let llog = PostFinalizationLog::bind(&l);
            let mut late = frame(99, record(A, 9, 900));
            late.attempt = AttemptRef::new("win").unwrap();
            wlog.append(late, SupervisorInstant::synthetic(99)).unwrap();
            let assessed = p.assess(&[llog.snapshot(), wlog.snapshot()]).unwrap();
            assert_eq!(
                assessed.members()[0].ceiling,
                PresentationCeiling::WeakenedByLateDelivery
            );
            assert_eq!(assessed.members()[1].ceiling, PresentationCeiling::Original);
            assert_eq!(p.assess(&[]).unwrap_err(), AssessCode::IdentityMismatch);
            assert_eq!(
                p.assess(&[wlog.snapshot(), wlog.snapshot(), llog.snapshot()])
                    .unwrap_err(),
                AssessCode::IdentityMismatch
            );
        }
    }
}

#[test]
fn r22_r23_r64c_mismatch_and_selected_wsl_only_preserve_scope_selection() {
    let lscope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(lscope.clone(), "l1f1v1").unwrap();
    let c = b.build();
    let x = environment_execution(lscope.clone(), &c, "att1", "z1", true);
    let m = ProjectionMembership::validate(
        Selection::SelectedWslOnly,
        SelectedRefs {
            windows: None,
            selected_wsl: Some(OpaqueRef::new("l1").unwrap()),
        },
        &[&x.members()[0]],
        &c,
    )
    .unwrap();
    let p = project_with_companion(&m).unwrap();
    assert_eq!(
        p.projection().windows,
        abf_counting::ScopeResult::NotSelected
    );
    assert_eq!(
        p.projection().computer.components.input.value,
        Value::ReportedClaimSum(100)
    );
    assert_eq!(
        p.projection()
            .selected_wsl
            .selected()
            .unwrap()
            .environment_refs,
        abf_counting::RefMultiplicity::One
    );
    let mut r = raw(
        vec![
            initial(RawEnumeration::Closed(1), &["l1f1v1-i1"]),
            record("unk9-i1", 1, 100),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    r.manifest.members = vec![lscope];
    r.distribution_matches = false;
    r.pre_barrier.push(frame(2, record("unk9-i1", 1, 90)));
    let x = finalize(r, &c).unwrap();
    let m = ProjectionMembership::validate(
        Selection::SelectedWslOnly,
        SelectedRefs {
            windows: None,
            selected_wsl: Some(OpaqueRef::new("l1").unwrap()),
        },
        &[&x.members()[0]],
        &c,
    )
    .unwrap();
    let p = project_with_companion(&m).unwrap();
    let l = p.projection().selected_wsl.selected().unwrap();
    assert_eq!(l.runs.run_keys, 0);
    assert_eq!(l.runs.coverage, Cov::NoEvidence);
    assert_eq!(l.stored_occurrences.0, 0);
    assert_eq!(
        l.components.input.value,
        Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence)
    );
    assert!(l.components.input.evidence.qualifications.is_empty());
}

#[test]
fn r44_r47_r48_r92_r93_disjoint_families_and_unknown_member_keep_exact_populations() {
    let peer = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope(), "f1v1").unwrap();
    b.add(peer.clone(), "f2v1").unwrap();
    let c = b.build();
    for kind in [0, 1, 2] {
        let mut p = vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            end(A, 2, 1),
        ];
        let mut listing = initial(
            if kind == 2 {
                RawEnumeration::RootUnresolved
            } else {
                RawEnumeration::Closed(1)
            },
            if kind == 2 { &[] } else { &["f2v1-i1"] },
        );
        let Payload::Enumeration { member, .. } = &mut listing else {
            unreachable!()
        };
        *member = 1;
        p.push(listing);
        if kind == 0 {
            p.extend([record("f2v1-i1", 1, 100), end("f2v1-i1", 2, 1)]);
        }
        if kind == 1 {
            p.push(event(
                "f2v1-i1",
                1,
                SourceEvent::Skipped(StopReason::Budget),
            ));
        }
        p.push(Payload::RunEnd);
        let mut r = raw(p, ProcessEnd::ExitedNormally);
        r.manifest.members.push(peer.clone());
        let x = finalize(r, &c).unwrap();
        let m = membership(&x, &c);
        let paired = project_with_companion(&m).unwrap();
        let w = paired.projection().windows.selected().unwrap();
        assert_eq!(w.runs.run_keys, if kind == 2 { 1 } else { 2 });
        assert_eq!(w.stored_occurrences.0, if kind == 0 { 2 } else { 1 });
        assert_eq!(
            w.components.input.value,
            Value::ReportedClaimSum(if kind == 0 { 200 } else { 100 })
        );
        assert_eq!(
            w.runs.coverage,
            if kind == 1 {
                Cov::ReportedIncomplete
            } else {
                Cov::ReportedComplete
            }
        );
        let (decls, records) = match kind {
            0 => (
                vec![
                    (A, SourceRunStatus::Complete),
                    ("f2v1-i1", SourceRunStatus::Complete),
                ],
                vec![
                    (A, 100, SourceRunStatus::Complete),
                    ("f2v1-i1", 100, SourceRunStatus::Complete),
                ],
            ),
            1 => (
                vec![
                    (A, SourceRunStatus::Complete),
                    ("f2v1-i1", SourceRunStatus::NotCollected),
                ],
                vec![(A, 100, SourceRunStatus::Complete)],
            ),
            _ => (
                vec![(A, SourceRunStatus::Complete)],
                vec![(A, 100, SourceRunStatus::Complete)],
            ),
        };
        assert_eq!(*paired.projection(), direct(&decls, &records));
        assert_eq!(
            paired.members().map(|m| m.records().len()).sum::<usize>(),
            w.stored_occurrences.0 as usize
        );
        // Both member input orders keep companion and numeric order canonical.
        let reversed = ProjectionMembership::validate(
            Selection::WindowsOnly,
            m.selected_refs().clone(),
            &[&x.members()[1], &x.members()[0]],
            &c,
        )
        .unwrap();
        assert_eq!(
            project_with_companion(&reversed).unwrap().projection(),
            paired.projection()
        );
    }
}

#[test]
fn r60_r62_r64a_r64b_r64d_r65_global_anomalies_only_change_declared_source_status_when_marked() {
    let peer = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope(), "f1v1").unwrap();
    b.add(peer.clone(), "f2v1").unwrap();
    let c = b.build();
    let empty = direct(&[], &[]);
    for mode in [0, 1, 2, 3] {
        let mut r = raw(vec![], ProcessEnd::Crashed);
        r.manifest.members.push(peer.clone());
        r.pre_barrier = match mode {
            0 => vec![
                frame(1, initial(RawEnumeration::Closed(0), &[])),
                frame(1, initial(RawEnumeration::RootAbsent, &[])),
            ],
            1 => vec![
                frame(1, initial(RawEnumeration::Closed(0), &[])),
                frame(1, initial(RawEnumeration::Closed(0), &[])),
                frame(2, Payload::RunEnd),
            ],
            2 => vec![
                frame(1, initial(RawEnumeration::Closed(0), &[])),
                frame(3, Payload::RunEnd),
            ],
            _ => vec![],
        };
        let x = finalize(r, &c).unwrap();
        assert_eq!(
            project_with_companion(&membership(&x, &c))
                .unwrap()
                .projection(),
            &empty
        );
    }
    for e in [
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::RootUnresolved,
    ] {
        let mut other = initial(e, &[]);
        let Payload::Enumeration { member, .. } = &mut other else {
            unreachable!()
        };
        *member = 1;
        let mut r = raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                other,
                record(A, 1, 100),
                end(A, 2, 1),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        );
        r.manifest.members.push(peer.clone());
        r.pre_barrier.push(frame(3, record("f1v1-i9", 1, 90)));
        let x = finalize(r, &c).unwrap();
        let p = project_with_companion(&membership(&x, &c)).unwrap();
        assert_eq!(
            *p.projection(),
            direct(&[(A, SourceRunStatus::Partial)], &[])
        );
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(w.runs.coverage, Cov::ReportedIncomplete);
        assert_eq!(w.components.input.value, Value::NoValidClaims);
        assert_eq!(
            w.components.input.evidence.qualifications.bits(),
            Q::PartialRun as u32
        );
    }
}

#[test]
fn m3_authoritative_s9_s11_and_no_record_s2_mapping_is_total() {
    use SourceRunStatus::{NotCollected, Partial};
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            record(A, 1, 100),
            record(A, 2, 50),
            end(A, 3, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Partial)],
        &[(A, 100, Partial), (A, 50, Partial)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(150),
        Q::PartialRun as u32,
    );
    // S11's table column is authoritative, including reasons whose summary
    // spelling also occurs in S13. An explicit ReadError is a different row.
    for reason in [
        PartialReason::DeliveryConflict,
        PartialReason::DeliveryLoss,
        PartialReason::Io,
        PartialReason::Budget,
        PartialReason::ReaderTerminated,
        PartialReason::Timeout,
        PartialReason::TransportLost,
        PartialReason::Cancelled,
    ] {
        literal_case(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                event(A, 2, SourceEvent::ReadError(reason)),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
            &[(A, Partial)],
            &[(A, 100, Partial)],
            Cov::ReportedIncomplete,
            Value::ReportedClaimSum(100),
            Q::PartialRun as u32,
        );
    }
    literal_case(
        vec![
            initial(RawEnumeration::Closed(1), &[A]),
            event(A, 1, SourceEvent::OpenSuccess),
            event(A, 2, SourceEvent::Skipped(StopReason::Budget)),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, NotCollected)],
        &[],
        Cov::NoTraversalReported,
        Value::Indeterminate(abf_counting::IndeterminateReason::NoTraversalReported),
        Q::NotCollectedRun as u32,
    );
}

#[test]
fn r102f_literally_preserves_expected_claims_and_marks_only_named_source() {
    use SourceRunStatus::{Complete, Partial};
    for names_expected in [false, true] {
        let mut r = raw(
            vec![
                initial(RawEnumeration::Closed(1), &[A]),
                record(A, 1, 100),
                end(A, 2, 1),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        );
        r.pre_barrier[3].frame_seq = 5;
        r.pre_barrier.push(frame(
            4,
            record(if names_expected { A } else { "unknown-i1" }, 2, 7),
        ));
        r.pre_barrier.push(frame(4, record("unk9-i1", 1, 50)));
        let status = if names_expected { Partial } else { Complete };
        literal_raw(
            r,
            &[(A, status)],
            &[(A, 100, status)],
            if names_expected {
                Cov::ReportedIncomplete
            } else {
                Cov::ReportedComplete
            },
            Value::ReportedClaimSum(100),
            if names_expected {
                Q::PartialRun as u32
            } else {
                0
            },
        );
    }
}

#[test]
fn paired_assessment_rejects_unrelated_diagnostics_and_shares_one_snapshot_per_execution() {
    let peer = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope(), "f1v1").unwrap();
    b.add(peer.clone(), "f2v1").unwrap();
    let c = b.build();
    let mut r = raw(baseline(), ProcessEnd::ExitedNormally);
    r.manifest.members.push(peer);
    let x = finalize(r, &c).unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let y = environment_execution(scope(), &c, "other", "z2", true);
    let mut log = PostFinalizationLog::bind(&x);
    let foreign = PostFinalizationLog::bind(&y).snapshot();
    assert_eq!(
        p.assess(std::slice::from_ref(&foreign)).unwrap_err(),
        AssessCode::IdentityMismatch
    );
    assert_eq!(
        p.assess(&[log.snapshot(), foreign]).unwrap_err(),
        AssessCode::IdentityMismatch
    );
    let initial = p.assess(&[log.snapshot()]).unwrap();
    assert_eq!(initial.members().len(), 2);
    assert_eq!(initial.members()[0].ceiling, PresentationCeiling::Original);
    assert_eq!(initial.members()[1].ceiling, PresentationCeiling::Original);
    let before = *p.projection();
    log.append(
        frame(6, record(A, 3, 99999)),
        SupervisorInstant::synthetic(10),
    )
    .unwrap();
    let attributed = p.assess(&[log.snapshot()]).unwrap();
    assert_eq!(
        attributed.members()[0].ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert_eq!(
        attributed.members()[1].ceiling,
        PresentationCeiling::Original
    );
    // A late SECOND control is unattributed; it changes neither frozen inventory
    // nor numeric values and weakens both assessments of its execution (R86).
    log.append(
        frame(7, second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"])),
        SupervisorInstant::synthetic(11),
    )
    .unwrap();
    let late = p.assess(&[log.snapshot()]).unwrap();
    assert_eq!(
        late.members()[1].ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert_eq!(p.projection(), &before);
    assert_eq!(
        project_with_companion(&membership(&x, &c))
            .unwrap()
            .projection(),
        &before
    );
    assert_eq!(
        p.members().next().unwrap().second_enumeration(),
        &SecondEnumeration::NotPerformed
    );
}

#[test]
fn r67_global_anomaly_and_other_environment_are_paired_without_cross_effects() {
    let wscope = scope();
    let lscope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(wscope.clone(), "f1v1").unwrap();
    b.add(lscope.clone(), "l1f1v1").unwrap();
    let c = b.build();
    let mut wr = raw(
        vec![initial(RawEnumeration::Closed(0), &[])],
        ProcessEnd::Crashed,
    );
    wr.pre_barrier
        .push(frame(1, initial(RawEnumeration::RootAbsent, &[])));
    let w = finalize(wr, &c).unwrap();
    let mut lr = raw(
        vec![
            initial(RawEnumeration::Closed(2), &["l1f1v1-i1", "l1f1v1-i2"]),
            record("l1f1v1-i1", 1, 100),
            end("l1f1v1-i1", 2, 1),
            end("l1f1v1-i2", 1, 0),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    lr.manifest.members = vec![lscope];
    lr.manifest.attempt = AttemptRef::new("e2").unwrap();
    lr.manifest.finalization = FinalizationId::new("z2").unwrap();
    for frame in &mut lr.pre_barrier {
        frame.attempt = lr.manifest.attempt.clone();
    }
    let l = finalize(lr, &c).unwrap();
    let m = ProjectionMembership::validate(
        Selection::Both,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: Some(OpaqueRef::new("l1").unwrap()),
        },
        &[&w.members()[0], &l.members()[0]],
        &c,
    )
    .unwrap();
    let p = project_with_companion(&m).unwrap();
    assert_eq!(
        p.projection().windows.selected().unwrap().runs.coverage,
        Cov::NoEvidence
    );
    assert_eq!(
        p.projection()
            .windows
            .selected()
            .unwrap()
            .components
            .input
            .value,
        Value::Indeterminate(abf_counting::IndeterminateReason::NoRunEvidence)
    );
    assert_eq!(
        p.projection()
            .selected_wsl
            .selected()
            .unwrap()
            .components
            .input
            .value,
        Value::ReportedClaimSum(100)
    );
    assert_eq!(
        p.projection()
            .selected_wsl
            .selected()
            .unwrap()
            .runs
            .run_keys,
        2
    );
    assert_eq!(
        p.projection().computer.components.input.value,
        Value::ReportedClaimSum(100)
    );
    assert!(
        p.projection()
            .computer
            .components
            .input
            .evidence
            .qualifications
            .contains(Q::EnvironmentIndeterminate)
    );
    assert_eq!(
        p.members().next().unwrap().verdict(),
        CoverageVerdict::Inconsistent
    );
    assert_eq!(
        p.members().nth(1).unwrap().verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
}

#[test]
fn r99c_three_complete_declarations_and_two_claims_survive_local_second_ambiguity() {
    let peer = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope(), "f1v1").unwrap();
    b.add(peer.clone(), "f2v1").unwrap();
    let c = b.build();
    let mut p = baseline();
    let mut other = initial(RawEnumeration::Closed(1), &["f2v1-i1"]);
    let Payload::Enumeration { member, .. } = &mut other else {
        unreachable!()
    };
    *member = 1;
    p.extend([
        other,
        record("f2v1-i1", 1, 100),
        end("f2v1-i1", 2, 1),
        second(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"]),
        second(RawEnumeration::Closed(2), &[A, B]),
    ]);
    let mut r = raw(p, ProcessEnd::ExitedNormally);
    r.manifest.members.push(peer);
    let x = finalize(r, &c).unwrap();
    let p = project_with_companion(&membership(&x, &c)).unwrap();
    let w = p.projection().windows.selected().unwrap();
    assert_eq!(w.runs.run_keys, 3);
    assert_eq!(w.runs.raw_declared_claims.complete, 3);
    assert_eq!(w.runs.raw_record_claims.complete, 2);
    assert_eq!(w.stored_occurrences.0, 2);
    assert_eq!(w.runs.coverage, Cov::ReportedComplete);
    assert_eq!(w.components.input.value, Value::ReportedClaimSum(200));
    assert!(w.components.input.evidence.qualifications.is_empty());
    assert_eq!(
        *p.projection(),
        direct(
            &[
                (A, SourceRunStatus::Complete),
                (B, SourceRunStatus::Complete),
                ("f2v1-i1", SourceRunStatus::Complete)
            ],
            &[
                (A, 100, SourceRunStatus::Complete),
                ("f2v1-i1", 100, SourceRunStatus::Complete)
            ]
        )
    );
}

#[test]
fn r25_subset_retry_preserves_individual_100_and_50_without_combining_or_scope_overclaim() {
    let c = catalog();
    let first = finalize(
        raw(
            vec![
                initial(RawEnumeration::Closed(2), &[A, B]),
                record(A, 1, 100),
                end(A, 2, 1),
                event(B, 1, SourceEvent::OpenFailed(OpenFailure::PermissionDenied)),
                Payload::RunEnd,
            ],
            ProcessEnd::ExitedNormally,
        ),
        &c,
    )
    .unwrap();
    let mut r = raw(
        vec![
            initial(RawEnumeration::Closed(1), &[B]),
            record(B, 1, 50),
            end(B, 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    r.manifest.attempt = AttemptRef::new("att2").unwrap();
    r.manifest.finalization = FinalizationId::new("z2").unwrap();
    r.manifest.retry_of = Some(AttemptRef::new("att1").unwrap());
    r.manifest.subset = RetrySubset::OnlyNonCoveredSources;
    for frame in &mut r.pre_barrier {
        frame.attempt = r.manifest.attempt.clone();
    }
    let retry = finalize(r, &c).unwrap();
    let old = project_with_companion(&membership(&first, &c)).unwrap();
    let new = project_with_companion(&membership(&retry, &c)).unwrap();
    assert_eq!(
        old.projection().computer.components.input.value,
        Value::ReportedClaimSum(100)
    );
    assert_eq!(
        old.projection().windows.selected().unwrap().runs.coverage,
        Cov::ReportedIncomplete
    );
    assert_eq!(
        new.projection().computer.components.input.value,
        Value::ReportedClaimSum(50)
    );
    assert_eq!(
        new.projection().windows.selected().unwrap().runs.coverage,
        Cov::ReportedComplete
    );
    assert_eq!(
        new.members().next().unwrap().lineage().1,
        RetrySubset::OnlyNonCoveredSources
    );
    let permits_scope_coverage = new.members().all(|m| {
        matches!(
            m.verdict(),
            CoverageVerdict::CoveredWithinEnumeration { .. }
        ) && m.lineage().1 == RetrySubset::AllExpected
    });
    assert!(!permits_scope_coverage);
    assert_eq!(
        ProjectionMembership::validate(
            Selection::WindowsOnly,
            SelectedRefs {
                windows: Some(OpaqueRef::new("w1").unwrap()),
                selected_wsl: None
            },
            &[&first.members()[0], &retry.members()[0]],
            &c
        )
        .unwrap_err(),
        AdaptCode::DuplicateScopeMember
    );
    assert_eq!(
        old.projection().computer.components.input.value,
        Value::ReportedClaimSum(100)
    );
}

#[test]
fn r45_only_selected_environment_is_admitted_and_projects_literal_40() {
    let oldscope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    let newscope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l2", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(oldscope.clone(), "old").unwrap();
    b.add(newscope.clone(), "new").unwrap();
    let c = b.build();
    let old = environment_execution(oldscope, &c, "e7", "z1", true);
    let mut r = raw(
        vec![
            initial(RawEnumeration::Closed(1), &["new-i1"]),
            record("new-i1", 1, 40),
            end("new-i1", 2, 1),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
    );
    r.manifest.members = vec![newscope];
    r.manifest.attempt = AttemptRef::new("e8").unwrap();
    r.manifest.finalization = FinalizationId::new("z2").unwrap();
    for frame in &mut r.pre_barrier {
        frame.attempt = r.manifest.attempt.clone();
    }
    let new = finalize(r, &c).unwrap();
    let refs = SelectedRefs {
        windows: None,
        selected_wsl: Some(OpaqueRef::new("l2").unwrap()),
    };
    assert_eq!(
        ProjectionMembership::validate(
            Selection::SelectedWslOnly,
            refs.clone(),
            &[&old.members()[0], &new.members()[0]],
            &c
        )
        .unwrap_err(),
        AdaptCode::EnvironmentRefNotSelected
    );
    let m =
        ProjectionMembership::validate(Selection::SelectedWslOnly, refs, &[&new.members()[0]], &c)
            .unwrap();
    let p = project_with_companion(&m).unwrap();
    let l = p.projection().selected_wsl.selected().unwrap();
    assert_eq!(l.components.input.value, Value::ReportedClaimSum(40));
    assert_eq!(l.environment_refs, abf_counting::RefMultiplicity::One);
    assert_eq!(l.runs.run_keys, 1);
}

#[test]
fn r11_literal_three_source_complete_partial_not_collected_sum_is_10() {
    use SourceRunStatus::{Complete, NotCollected, Partial};
    literal_case(
        vec![
            initial(RawEnumeration::Closed(3), &[A, B, "f1v1-i3"]),
            record(A, 1, 10),
            end(A, 2, 1),
            event(B, 1, SourceEvent::OpenSuccess),
            event(B, 2, SourceEvent::ReadError(PartialReason::Cancelled)),
            Payload::RunEnd,
        ],
        ProcessEnd::ExitedNormally,
        &[(A, Complete), (B, Partial), ("f1v1-i3", NotCollected)],
        &[(A, 10, Complete)],
        Cov::ReportedIncomplete,
        Value::ReportedClaimSum(10),
        (Q::PartialRun as u32) | (Q::NotCollectedRun as u32),
    );
}
