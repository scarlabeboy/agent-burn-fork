use abf_reader_fixture::*;
use abf_source_run::*;
fn setup() -> (CatalogSnapshot, CatalogScope, AttemptRef, SourceRef) {
    let s = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(s.clone(), "f1v1").unwrap();
    (
        b.build(),
        s,
        AttemptRef::new("att1").unwrap(),
        SourceRef::new("f1v1-i1").unwrap(),
    )
}
fn st(n: u64, event: Event<'_>) -> Step<'_> {
    Step {
        frame_seq: n,
        event,
    }
}
fn init(s: &[SourceRef]) -> Step<'_> {
    st(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(u64::try_from(s.len()).unwrap()),
            sources: s,
        },
    )
}
fn op(n: u64, r: &SourceRef, q: u64, action: Action) -> Step<'_> {
    st(
        n,
        Event::Source {
            source: r,
            source_seq: q,
            action,
        },
    )
}
fn rec<'a>(n: u64, r: &'a SourceRef, q: u64, bytes: &'a [u8]) -> Step<'a> {
    st(
        n,
        Event::Record {
            source: r,
            source_seq: q,
            bytes,
        },
    )
}
// These are injected fictional supervisor facts, never claims made by the emitter.
fn finish(
    c: &CatalogSnapshot,
    s: CatalogScope,
    a: AttemptRef,
    frames: Vec<ReaderFrame>,
    process: ProcessEnd,
) -> FinalizedExecution {
    finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: a,
                finalization: FinalizationId::new("z1").unwrap(),
                members: vec![s],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: frames,
            process,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(1, 2),
        },
        c,
    )
    .unwrap()
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
#[test]
fn complete_literal_and_zero_record_conservation() {
    let (c, s, a, r) = setup();
    let r2 = SourceRef::new("f1v1-i2").unwrap();
    let list = [r.clone(), r2.clone()];
    let script = [
        init(&list),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        rec(4, &r, 1, b"{\"tokens\":{\"input\":100}}"),
        op(5, &r, 2, Action::End(ChangeObservation::NoChangeObserved)),
        op(6, &r2, 1, Action::OpenAttempt),
        op(7, &r2, 1, Action::OpenSuccess),
        op(8, &r2, 1, Action::End(ChangeObservation::NoChangeObserved)),
        st(9, Event::RunEnd),
    ];
    let out = emit(&a, &c, &s, &script).unwrap();
    let x = finish(&c, s, a, out.into_frames(), ProcessEnd::ExitedNormally);
    let member = &x.members()[0];
    assert_eq!(
        member.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(member.records().len(), 1);
    assert_eq!(member.sources().len(), 2);
    assert_eq!(member.sources()[0].1.matched(), &[10]);
    let m = membership(&x, &c);
    let p = project_with_companion(&m).unwrap();
    let w = p.projection().windows.selected().unwrap();
    assert_eq!(w.stored_occurrences.0, 1);
    assert_eq!(w.runs.run_keys, 2);
    assert_eq!(w.runs.raw_declared_claims.complete, 2);
    assert_eq!(
        w.components.input.value,
        abf_counting::ComponentValue::ReportedClaimSum(100)
    );
    assert_eq!(
        w.runs.coverage,
        abf_counting::CoverageState::ReportedComplete
    );
    let before = *p.projection();
    let mut d = PostFinalizationLog::bind(&x);
    d.append(
        ReaderFrame {
            attempt: x.manifest().attempt.clone(),
            frame_seq: 99,
            payload: Payload::RunEnd,
        },
        SupervisorInstant::synthetic(3),
    )
    .unwrap();
    p.assess(&[d.snapshot()]).unwrap();
    assert_eq!(*p.projection(), before);
}
#[test]
fn inventory_never_zero() {
    for outcome in [
        RawEnumeration::Closed(0),
        RawEnumeration::Partial(0, StopReason::Budget),
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
        RawEnumeration::RootAbsent,
        RawEnumeration::RootUnresolved,
        RawEnumeration::LayoutUnrecognized,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::NotAttempted,
    ] {
        let (c, s, a, _) = setup();
        let script = [st(
            1,
            Event::Enumeration {
                outcome,
                sources: &[],
            },
        )];
        let frames = emit(&a, &c, &s, &script).unwrap().into_frames();
        let x = finish(&c, s, a, frames, ProcessEnd::Crashed);
        let m = membership(&x, &c);
        let p = project_with_companion(&m).unwrap();
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(w.runs.run_keys, 0);
        assert_eq!(w.stored_occurrences.0, 0);
        assert_eq!(
            w.components.input.value,
            abf_counting::ComponentValue::Indeterminate(
                abf_counting::IndeterminateReason::NoRunEvidence
            )
        );
    }
}
#[test]
fn outcome_classes_are_g4_owned() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    let cases = vec![
        (
            vec![init(&list), op(2, &r, 1, Action::Skip(StopReason::Budget))],
            3,
            SourceOutcome::NotAttempted(StopReason::Budget),
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::OpenFail(OpenFailure::PermissionDenied)),
            ],
            3,
            SourceOutcome::OpenFailed(OpenFailure::PermissionDenied),
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::Vanished),
            ],
            3,
            SourceOutcome::Vanished,
        ),
        (vec![init(&list)], 15, SourceOutcome::Unaccounted),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                st(9, Event::Cancel),
            ],
            14,
            SourceOutcome::OpenInterrupted {
                reason: InterruptReason::Cancelled,
            },
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::OpenSuccess),
                rec(4, &r, 1, b"{}"),
                st(9, Event::Cancel),
            ],
            13,
            SourceOutcome::PartialRead {
                records: 1,
                reason: PartialReason::Cancelled,
            },
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::OpenSuccess),
                op(4, &r, 1, Action::ReadError(PartialReason::Io)),
            ],
            11,
            SourceOutcome::PartialRead {
                records: 0,
                reason: PartialReason::Io,
            },
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::OpenSuccess),
                rec(4, &r, 1, b"{}"),
                op(5, &r, 2, Action::ReadError(PartialReason::Io)),
            ],
            11,
            SourceOutcome::PartialRead {
                records: 1,
                reason: PartialReason::Io,
            },
        ),
        (
            vec![
                init(&list),
                op(2, &r, 1, Action::OpenAttempt),
                op(3, &r, 1, Action::OpenSuccess),
                op(4, &r, 1, Action::End(ChangeObservation::ShrankOrReplaced)),
            ],
            10,
            SourceOutcome::TraversedToObservedEnd {
                records: 0,
                change: ChangeObservation::ShrankOrReplaced,
            },
        ),
    ];
    for (script, class, outcome) in cases {
        let frames = emit(&a, &c, &s, &script).unwrap().into_frames();
        let retained = frames
            .iter()
            .filter(|f| {
                matches!(
                    f.payload,
                    Payload::Source {
                        event: SourceEvent::Record { .. },
                        ..
                    }
                )
            })
            .count();
        let x = finish(&c, s.clone(), a.clone(), frames, ProcessEnd::KilledCancel);
        let source = &x.members()[0].sources()[0].1;
        assert_eq!(source.matched()[0], class);
        assert_eq!(source.outcome(), outcome);
        let m = membership(&x, &c);
        let p = project_with_companion(&m).unwrap();
        assert_eq!(
            usize::try_from(p.projection().computer.stored_occurrences.0).unwrap(),
            retained
        );
        assert_eq!(p.projection().windows.selected().unwrap().runs.run_keys, 1);
    }
}
#[test]
fn replay_quarantine_and_distinct_records() {
    for replacement in [None, Some(b"{changed".as_slice())] {
        let (c, s, a, r) = setup();
        let list = [r.clone()];
        let script = [
            init(&list),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 1, Action::OpenSuccess),
            rec(4, &r, 1, b"{\"tokens\":{\"input\":100}}"),
            rec(5, &r, 2, b"{\"tokens\":{\"input\":100}}"),
            st(
                6,
                Event::Replay {
                    original: 3,
                    replacement,
                },
            ),
            op(7, &r, 3, Action::End(ChangeObservation::NoChangeObserved)),
            st(8, Event::RunEnd),
        ];
        let out = emit(&a, &c, &s, &script).unwrap();
        let x = finish(&c, s, a, out.into_frames(), ProcessEnd::ExitedNormally);
        let member = &x.members()[0];
        let expected = if replacement.is_some() { 1 } else { 2 };
        assert_eq!(member.records().len(), expected);
        let m = membership(&x, &c);
        let p = project_with_companion(&m).unwrap();
        let w = p.projection().windows.selected().unwrap();
        assert_eq!(usize::try_from(w.stored_occurrences.0).unwrap(), expected);
        assert_eq!(w.runs.run_keys, 1);
        if replacement.is_some() {
            assert_eq!(member.sources()[0].1.matched()[0], 4);
            assert_eq!(member.delivery().seq_conflicts, 1);
            assert_eq!(member.verdict(), CoverageVerdict::Inconsistent);
            assert_eq!(
                w.components.input.value,
                abf_counting::ComponentValue::ReportedClaimSum(100)
            );
            assert_eq!(
                x.dispositions()[3],
                Disposition::Quarantined(QuarantineCode::SequenceConflict)
            );
        } else {
            assert_eq!(member.sources()[0].1.matched()[0], 10);
            assert_eq!(member.delivery().replays_quarantined, 1);
            assert_eq!(
                w.components.input.value,
                abf_counting::ComponentValue::ReportedClaimSum(200)
            );
            assert_eq!(
                x.dispositions()[5],
                Disposition::Quarantined(QuarantineCode::ExactReplay)
            );
        }
    }
}
#[test]
fn malformed_record_is_retained_downstream() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    let script = [
        init(&list),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        rec(4, &r, 1, b"{broken"),
        op(5, &r, 2, Action::End(ChangeObservation::NoChangeObserved)),
        st(6, Event::RunEnd),
    ];
    let x = finish(
        &c,
        s.clone(),
        a.clone(),
        emit(&a, &c, &s, &script).unwrap().into_frames(),
        ProcessEnd::ExitedNormally,
    );
    assert_eq!(
        x.members()[0].records()[0].parse_result(),
        &abf_parser::parse(b"{broken", None, abf_parser::Limits::MAX)
    );
    let m = membership(&x, &c);
    let p = project_with_companion(&m).unwrap();
    assert_eq!(p.projection().computer.stored_occurrences.0, 1);
    assert_eq!(p.projection().windows.selected().unwrap().runs.run_keys, 1);
}
#[test]
fn refusal_has_no_partial_paired_projection() {
    let (c, s, a, _) = setup();
    let list: Vec<_> = (1..=129)
        .map(|i| SourceRef::new(&format!("f1v1-i{i}")).unwrap())
        .collect();
    let script = [init(&list)];
    let x = finish(
        &c,
        s.clone(),
        a.clone(),
        emit(&a, &c, &s, &script).unwrap().into_frames(),
        ProcessEnd::Crashed,
    );
    let admission = ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: None,
        },
        &x.members().iter().collect::<Vec<_>>(),
        &c,
    );
    assert_eq!(admission.unwrap_err(), AdaptCode::DeclarationCeiling);
}
