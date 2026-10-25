use abf_catalog_plan_fixture::*;
use abf_reader_fixture::{Action, Event, Step};
use abf_source_run::*;
use abf_supervisor_fixture::*;

fn plan() -> FixturePlan {
    plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families: &["f1"],
        },
        &[FixtureDescriptor {
            id: "d",
            schema: 1,
            revision: "r1",
            scope: CatalogScope::new(EnvironmentKind::Windows, "fictionalW", "f1", "fixture-v1")
                .unwrap(),
            rule: "rule",
            scope_tag: "scope1",
            approved_classes: &[RootClass::Configured],
            overlaps: None,
        }],
        &[FictionalCandidate {
            id: "c",
            family: "f1",
            namespace: "fictionalW",
            revision: "r1",
            class: RootClass::Configured,
            locator: r"C:\Fixture\Root",
            recognition: Some(&[RecognitionClaim {
                descriptor: "d",
                descriptor_revision: "r1",
                rule: "rule",
            }]),
        }],
        &[FictionalGrant {
            candidate: "c",
            family: "f1",
            namespace: "fictionalW",
            candidate_revision: "r1",
            class: RootClass::Configured,
            descriptor: "d",
            descriptor_revision: "r1",
        }],
    )
    .unwrap()
}
fn step(frame_seq: u64, event: Event<'_>) -> Step<'_> {
    Step { frame_seq, event }
}
fn facts() -> SyntheticFacts {
    SyntheticFacts {
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        times: TimeObservations::new(1, 2),
    }
}
fn complete<'a>(source: &'a SourceRef, sources: &'a [SourceRef]) -> Vec<Step<'a>> {
    vec![
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(1),
                sources,
            },
        ),
        step(
            2,
            Event::Source {
                source,
                source_seq: 1,
                action: Action::OpenAttempt,
            },
        ),
        step(
            3,
            Event::Source {
                source,
                source_seq: 1,
                action: Action::OpenSuccess,
            },
        ),
        step(
            4,
            Event::Record {
                source,
                source_seq: 1,
                bytes: br#"{"tokens":{"input":37}}"#,
            },
        ),
        step(
            5,
            Event::Source {
                source,
                source_seq: 2,
                action: Action::End(ChangeObservation::NoChangeObserved),
            },
        ),
        step(6, Event::RunEnd),
    ]
}
#[test]
fn success_preserves_literal_claims_and_provenance() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    assert_eq!(
        s.run(&complete(&source, &sources), None, facts(), false)
            .code(),
        Code::Completed
    );
    assert_eq!(s.availability(), Availability::Fresh);
    let good = s.last_good().unwrap();
    assert_eq!(good.projection_error(), None);
    let x = good.execution();
    assert_eq!(x.manifest().attempt.as_str(), "a1");
    assert_eq!(x.manifest().finalization.as_str(), "z1");
    assert_eq!(
        x.members()[0].scope().environment_kind(),
        EnvironmentKind::Windows
    );
    assert_eq!(x.members()[0].records()[0].source(), &source);
    assert_eq!(x.members()[0].attestation(), AttestationBasis::Unattested);
    let projection = good.project().unwrap();
    let windows = projection.projection().windows.selected().unwrap();
    assert_eq!(
        windows.components.input.value,
        abf_counting::ComponentValue::ReportedClaimSum(37)
    );
    assert_eq!(windows.usage, abf_counting::UsageTotal::Unsupported);
}

#[test]
fn failure_keeps_last_good_separate_and_marks_stale() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    let failed = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
                sources: &[],
            },
        ),
        step(2, Event::RunEnd),
    ];
    assert_eq!(
        s.run(&failed, None, facts(), false).code(),
        Code::Degraded(CoverageVerdict::ScopeUnavailable(
            UnavailableReason::Unavailable
        ))
    );
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
    assert_eq!(
        s.latest()
            .unwrap()
            .snapshot()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a2"
    );
    assert_eq!(
        s.last_good().unwrap().execution().members()[0]
            .records()
            .len(),
        1
    );
}
#[test]
fn cancellation_preserves_prefix_without_completion_at_each_poll() {
    for at in [1, 2, 4] {
        let mut s = Supervisor::new(plan()).unwrap();
        let source = SourceRef::new("scope1-i1").unwrap();
        let sources = [source.clone()];
        let mut script = complete(&source, &sources);
        script.insert(at, step(0, Event::Cancel));
        let report = s.run(&script, None, facts(), false);
        assert_eq!(report.code(), Code::Cancelled);
        let member = &report.snapshot().unwrap().execution().members()[0];
        assert!(!member.run_end_frame());
        assert_eq!(member.process(), ProcessEnd::KilledCancel);
        assert_eq!(member.records().len(), usize::from(at == 4));
        assert_eq!(s.availability(), Availability::Unavailable);
        assert!(s.last_good().is_none());
    }
}
#[test]
fn pre_cancel_does_not_consume_identity_or_run_invalid_input() {
    let mut s = Supervisor::new(plan()).unwrap();
    assert_eq!(
        s.run(&[step(0, Event::RunEnd)], None, facts(), true).code(),
        Code::Cancelled
    );
    assert!(s.latest().unwrap().snapshot().is_none());
    let empty = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(0),
                sources: &[],
            },
        ),
        step(2, Event::RunEnd),
    ];
    s.run(&empty, None, facts(), false);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}
#[test]
fn errors_are_exact_and_never_replace_reliable_data() {
    let mut s = Supervisor::new(plan()).unwrap();
    let empty = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(0),
                sources: &[],
            },
        ),
        step(2, Event::RunEnd),
    ];
    s.run(&empty, None, facts(), false);
    let invalid = [step(1, Event::RunEnd)];
    assert_eq!(
        s.run(&invalid, None, facts(), false).code(),
        Code::Reader(abf_reader_fixture::Error::InvalidScript)
    );
    assert_eq!(
        s.run(
            &empty,
            None,
            SyntheticFacts {
                barrier: Barrier::NotDrained,
                ..facts()
            },
            false
        )
        .code(),
        Code::Finalize(FinalizeCode::InvalidBarrier)
    );
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}
#[test]
fn delayed_frames_only_weaken_diagnostics_and_do_not_reopen_snapshot() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let script = complete(&source, &sources);
    let report = s.run(
        &script,
        Some(4),
        SyntheticFacts {
            process: ProcessEnd::KilledTimeout,
            ..facts()
        },
        false,
    );
    assert_eq!(
        report.code(),
        Code::Degraded(CoverageVerdict::PartiallyCovered)
    );
    let snapshot = report.snapshot().unwrap();
    assert_eq!(snapshot.execution().members()[0].records().len(), 1);
    assert!(!snapshot.execution().members()[0].run_end_frame());
    assert_eq!(snapshot.diagnostics().entries().len(), 2);
    assert_eq!(
        snapshot
            .project()
            .unwrap()
            .assess(&[snapshot.diagnostics().clone()])
            .unwrap()
            .members()[0]
            .ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert!(s.last_good().is_none());
}
#[test]
fn reader_budget_and_barrier_index_refusals_are_explicit() {
    let mut s = Supervisor::new(plan()).unwrap();
    let script = vec![step(0, Event::Cancel); abf_reader_fixture::MAX_STEPS + 1];
    assert_eq!(
        s.run(&script, None, facts(), false).code(),
        Code::Reader(abf_reader_fixture::Error::BudgetExceeded)
    );
    assert_eq!(
        s.run(&[], Some(1), facts(), false).code(),
        Code::BarrierIndex
    );
    assert_eq!(s.availability(), Availability::Unavailable);
}
#[test]
fn independent_replays_are_deterministic_and_have_windows_only_membership() {
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let script = complete(&source, &sources);
    let mut a = Supervisor::new(plan()).unwrap();
    let mut b = Supervisor::new(plan()).unwrap();
    a.run(&script, None, facts(), false);
    b.run(&script, None, facts(), false);
    let a = a.last_good().unwrap();
    let b = b.last_good().unwrap();
    assert_eq!(a.execution().manifest(), b.execution().manifest());
    let facts = |x: &Snapshot| {
        x.execution()
            .delivery_evidence()
            .iter()
            .map(|d| {
                (
                    d.frame_seq(),
                    d.source().cloned(),
                    d.source_seq(),
                    d.terminal(),
                    d.enumeration(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(facts(a), facts(b));
    let p = a.project().unwrap();
    assert_eq!(p.selection(), Selection::WindowsOnly);
    assert_eq!(p.selected_refs().selected_wsl, None);
    assert_eq!(p.projection(), b.project().unwrap().projection());
}

#[test]
fn projection_refusal_retains_finalized_evidence_without_truncation() {
    let mut s = Supervisor::new(plan()).unwrap();
    let sources: Vec<_> = (1..=129)
        .map(|i| SourceRef::new(&format!("scope1-i{i}")).unwrap())
        .collect();
    let script = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(129),
            sources: &sources,
        },
    )];
    let report = s.run(&script, None, facts(), false);
    assert_eq!(report.code(), Code::Adapt(AdaptCode::DeclarationCeiling));
    assert_eq!(
        report.snapshot().unwrap().projection_error(),
        Some(AdaptCode::DeclarationCeiling)
    );
    assert_eq!(
        report.snapshot().unwrap().execution().members()[0]
            .sources()
            .len(),
        129
    );
    assert_eq!(s.availability(), Availability::Unavailable);
}
#[test]
fn attempt_issuance_is_bounded_and_never_resets() {
    let mut s = Supervisor::new(plan()).unwrap();
    let empty = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(0),
                sources: &[],
            },
        ),
        step(2, Event::RunEnd),
    ];
    for n in 1..=MAX_EXECUTIONS_PER_ISSUER {
        assert_eq!(s.run(&empty, None, facts(), false).code(), Code::Completed);
        assert_eq!(
            s.last_good()
                .unwrap()
                .execution()
                .manifest()
                .attempt
                .as_str(),
            format!("a{n}")
        );
    }
    assert_eq!(
        s.run(&empty, None, facts(), false).code(),
        Code::Identity(RefCode::CounterExhausted)
    );
    assert_eq!(
        s.run(&empty, None, facts(), false).code(),
        Code::Identity(RefCode::CounterExhausted)
    );
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1024"
    );
}
#[test]
fn maximal_script_has_bounded_late_diagnostics_without_false_freshness() {
    for length in [
        abf_reader_fixture::MAX_STEPS - 1,
        abf_reader_fixture::MAX_STEPS,
    ] {
        let mut s = Supervisor::new(plan()).unwrap();
        let mut script = vec![
            step(
                1,
                Event::Enumeration {
                    outcome: RawEnumeration::Closed(0),
                    sources: &[],
                },
            ),
            step(2, Event::RunEnd),
        ];
        script.extend((2..length).map(|_| {
            step(
                0,
                Event::Replay {
                    original: 0,
                    replacement: None,
                },
            )
        }));
        let report = s.run(&script, Some(2), facts(), false);
        assert_eq!(
            report.code(),
            Code::Degraded(CoverageVerdict::NoSourcesFound(
                NoSourcesReason::EmptyEnumeration
            ))
        );
        let d = report.snapshot().unwrap().diagnostics();
        assert_eq!(d.entries().len(), MAX_DIAGNOSTIC_ENTRIES);
        assert!(d.capacity_reached());
        assert!(!d.count_overflow());
        assert_eq!(d.counts().other as usize, length - 2);
        assert!(s.last_good().is_none());
    }
}
#[test]
fn no_work_and_unresolved_plans_cannot_become_empty_success() {
    let empty = plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families: &[],
        },
        &[],
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(
        Supervisor::new(empty).unwrap_err(),
        Code::SingleConditionalWindowsScopeRequired
    );
    let unresolved = plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families: &["f1"],
        },
        &[FixtureDescriptor {
            id: "d",
            schema: 1,
            revision: "r1",
            scope: CatalogScope::new(EnvironmentKind::Windows, "fictionalW", "f1", "fixture-v1")
                .unwrap(),
            rule: "rule",
            scope_tag: "scope1",
            approved_classes: &[RootClass::Configured],
            overlaps: None,
        }],
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(
        Supervisor::new(unresolved).unwrap_err(),
        Code::SingleConditionalWindowsScopeRequired
    );
}
#[test]
fn crash_missing_run_end_and_failed_inventory_never_publish_fresh_zero() {
    for outcome in [
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
        RawEnumeration::RootAbsent,
        RawEnumeration::RootUnresolved,
        RawEnumeration::NotAttempted,
    ] {
        let mut s = Supervisor::new(plan()).unwrap();
        let script = [
            step(
                1,
                Event::Enumeration {
                    outcome,
                    sources: &[],
                },
            ),
            step(2, Event::RunEnd),
        ];
        assert_ne!(s.run(&script, None, facts(), false).code(), Code::Completed);
        assert_eq!(s.availability(), Availability::Unavailable);
        assert!(s.last_good().is_none());
    }
    for process in [
        ProcessEnd::Crashed,
        ProcessEnd::KilledTimeout,
        ProcessEnd::ExitedWithError,
        ProcessEnd::Unknown,
    ] {
        let mut s = Supervisor::new(plan()).unwrap();
        let script = [
            step(
                1,
                Event::Enumeration {
                    outcome: RawEnumeration::Closed(0),
                    sources: &[],
                },
            ),
            step(2, Event::RunEnd),
        ];
        assert_ne!(
            s.run(&script, None, SyntheticFacts { process, ..facts() }, false)
                .code(),
            Code::Completed
        );
        assert!(s.last_good().is_none());
    }
    let mut s = Supervisor::new(plan()).unwrap();
    let script = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(0),
            sources: &[],
        },
    )];
    assert_ne!(s.run(&script, None, facts(), false).code(), Code::Completed);
}
#[test]
fn debug_and_actionable_diagnostics_do_not_expose_fixture_payloads() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    let debug = format!(
        "{s:?} {:?} {:?}",
        s.latest().unwrap(),
        s.last_good().unwrap()
    );
    assert!(!debug.contains("fictionalW"));
    assert!(!debug.contains("scope1"));
    assert!(!debug.contains("tokens"));
    assert!(
        Code::Finalize(FinalizeCode::InvalidBarrier)
            .action()
            .contains("drain")
    );
}

#[test]
fn equal_record_bytes_with_distinct_ordinals_remain_distinct() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    script[4] = step(
        5,
        Event::Record {
            source: &source,
            source_seq: 2,
            bytes: br#"{"tokens":{"input":37}}"#,
        },
    );
    script[5] = step(
        6,
        Event::Source {
            source: &source,
            source_seq: 3,
            action: Action::End(ChangeObservation::NoChangeObserved),
        },
    );
    script.push(step(7, Event::RunEnd));
    assert_eq!(s.run(&script, None, facts(), false).code(), Code::Completed);
    let good = s.last_good().unwrap();
    assert_eq!(good.execution().members()[0].records().len(), 2);
    assert_eq!(
        good.project()
            .unwrap()
            .projection()
            .windows
            .selected()
            .unwrap()
            .components
            .input
            .value,
        abf_counting::ComponentValue::ReportedClaimSum(74)
    );
}
#[test]
fn cancellation_never_bypasses_suffix_budgets_and_keeps_prior_good() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    assert_eq!(
        s.run(&[step(0, Event::Cancel)], None, facts(), false)
            .code(),
        Code::Cancelled
    );
    assert_eq!(s.availability(), Availability::Stale);
    let bytes = vec![b'x'; abf_reader_fixture::MAX_RECORD_BYTES + 1];
    let script = [
        step(0, Event::Cancel),
        step(
            1,
            Event::Record {
                source: &source,
                source_seq: 1,
                bytes: &bytes,
            },
        ),
    ];
    assert_eq!(
        s.run(&script, None, facts(), false).code(),
        Code::Reader(abf_reader_fixture::Error::BudgetExceeded)
    );
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}
#[test]
fn open_failure_and_partial_read_keep_their_exact_source_diagnostics() {
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let mut s = Supervisor::new(plan()).unwrap();
    let failed = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(1),
                sources: &sources,
            },
        ),
        step(
            2,
            Event::Source {
                source: &source,
                source_seq: 1,
                action: Action::OpenAttempt,
            },
        ),
        step(
            3,
            Event::Source {
                source: &source,
                source_seq: 1,
                action: Action::OpenFail(OpenFailure::PermissionDenied),
            },
        ),
        step(4, Event::RunEnd),
    ];
    let r = s.run(&failed, None, facts(), false);
    assert_eq!(r.code(), Code::Degraded(CoverageVerdict::NotCovered));
    assert_eq!(
        r.snapshot().unwrap().execution().members()[0].sources()[0]
            .1
            .outcome(),
        SourceOutcome::OpenFailed(OpenFailure::PermissionDenied)
    );
    let mut partial = complete(&source, &sources);
    partial[4] = step(
        5,
        Event::Source {
            source: &source,
            source_seq: 2,
            action: Action::ReadError(PartialReason::Io),
        },
    );
    let r = s.run(&partial, None, facts(), false);
    assert_eq!(r.code(), Code::Degraded(CoverageVerdict::PartiallyCovered));
    assert_eq!(
        r.snapshot().unwrap().execution().members()[0].sources()[0]
            .1
            .outcome(),
        SourceOutcome::PartialRead {
            records: 1,
            reason: PartialReason::Io
        }
    );
}

#[test]
fn issued_identity_survives_reader_or_drain_refusal() {
    let mut s = Supervisor::new(plan()).unwrap();
    let r = s.run(&[step(0, Event::RunEnd)], None, facts(), false);
    let (attempt, finalization) = r.identity().unwrap();
    assert_eq!(attempt.as_str(), "a1");
    assert_eq!(finalization.as_str(), "z1");
    assert!(r.snapshot().is_none());
    let empty = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(0),
                sources: &[],
            },
        ),
        step(2, Event::RunEnd),
    ];
    let r = s.run(
        &empty,
        None,
        SyntheticFacts {
            barrier: Barrier::NotDrained,
            ..facts()
        },
        false,
    );
    assert_eq!(r.identity().unwrap().0.as_str(), "a2");
    assert_eq!(r.identity().unwrap().1.as_str(), "z2");
    assert!(r.snapshot().is_none());
}

#[test]
fn cancel_with_projection_refusal_preserves_primary_outcome_and_prior_good() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    let oversized: Vec<_> = (1..=129)
        .map(|i| SourceRef::new(&format!("scope1-i{i}")).unwrap())
        .collect();
    let script = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(129),
                sources: &oversized,
            },
        ),
        step(0, Event::Cancel),
    ];
    let report = s.run(
        &script,
        None,
        SyntheticFacts {
            process: ProcessEnd::Crashed,
            ..facts()
        },
        false,
    );
    assert_eq!(report.code(), Code::Cancelled);
    let snapshot = report.snapshot().unwrap();
    assert_eq!(
        snapshot.projection_error(),
        Some(AdaptCode::DeclarationCeiling)
    );
    assert_eq!(
        snapshot.project().unwrap_err(),
        AdaptCode::DeclarationCeiling
    );
    assert_eq!(snapshot.execution().members()[0].sources().len(), 129);
    assert_eq!(
        snapshot.execution().members()[0].process(),
        ProcessEnd::KilledCancel
    );
    assert!(!snapshot.execution().members()[0].run_end_frame());
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
    assert_eq!(
        s.last_good().unwrap().execution().members()[0]
            .records()
            .len(),
        1
    );
}

#[test]
fn cancel_with_projection_refusal_without_prior_good_remains_unavailable() {
    let mut s = Supervisor::new(plan()).unwrap();
    let sources: Vec<_> = (1..=129)
        .map(|i| SourceRef::new(&format!("scope1-i{i}")).unwrap())
        .collect();
    let script = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(129),
                sources: &sources,
            },
        ),
        step(0, Event::Cancel),
    ];
    let report = s.run(&script, None, facts(), false);
    assert_eq!(report.code(), Code::Cancelled);
    assert_eq!(
        report.snapshot().unwrap().projection_error(),
        Some(AdaptCode::DeclarationCeiling)
    );
    assert_eq!(report.identity().unwrap().0.as_str(), "a1");
    assert_eq!(s.availability(), Availability::Unavailable);
    assert!(s.last_good().is_none());
}

#[test]
fn late_delivery_alone_prevents_completed_despite_normal_exit_and_run_end() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    s.run(&script, None, facts(), false);
    script.push(step(
        0,
        Event::Replay {
            original: 3,
            replacement: None,
        },
    ));
    let report = s.run(&script, Some(6), facts(), false);
    assert_eq!(
        report.code(),
        Code::Degraded(CoverageVerdict::CoveredWithinEnumeration { expected: 1 })
    );
    let snapshot = report.snapshot().unwrap();
    let member = &snapshot.execution().members()[0];
    assert_eq!(
        member.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(member.process(), ProcessEnd::ExitedNormally);
    assert!(member.run_end_frame());
    assert_eq!(member.records().len(), 1);
    assert_eq!(snapshot.projection_error(), None);
    assert_eq!(snapshot.diagnostics().counts().records, 1);
    assert_eq!(snapshot.diagnostics().entries().len(), 1);
    assert_eq!(
        assess(member, snapshot.diagnostics()).unwrap().ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}

#[test]
fn abandoned_without_reader_accepts_empty_evidence_without_becoming_fresh() {
    let mut s = Supervisor::new(plan()).unwrap();
    let report = s.run(
        &[],
        None,
        SyntheticFacts {
            barrier: Barrier::AbandonedWithoutReader,
            ..facts()
        },
        false,
    );
    assert_eq!(report.code(), Code::Degraded(CoverageVerdict::NoAttempt));
    let snapshot = report.snapshot().unwrap();
    assert_eq!(snapshot.execution().delivery_evidence().len(), 0);
    assert_eq!(
        snapshot.execution().members()[0].enumeration(),
        EnumerationOutcome::NotAttempted
    );
    assert_eq!(snapshot.projection_error(), None);
    assert!(snapshot.diagnostics().entries().is_empty());
    assert_eq!(s.availability(), Availability::Unavailable);
    assert!(s.last_good().is_none());

    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    assert_eq!(s.availability(), Availability::Fresh);
    let report = s.run(
        &[],
        None,
        SyntheticFacts {
            barrier: Barrier::AbandonedWithoutReader,
            ..facts()
        },
        false,
    );
    assert_eq!(report.code(), Code::Degraded(CoverageVerdict::NoAttempt));
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a2"
    );
}

#[test]
fn abandoned_without_reader_refuses_nonempty_pre_barrier_and_keeps_prior_good() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    s.run(&complete(&source, &sources), None, facts(), false);
    let script = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(0),
            sources: &[],
        },
    )];
    let report = s.run(
        &script,
        None,
        SyntheticFacts {
            barrier: Barrier::AbandonedWithoutReader,
            ..facts()
        },
        false,
    );
    assert_eq!(report.code(), Code::Finalize(FinalizeCode::InvalidBarrier));
    assert!(report.snapshot().is_none());
    assert_eq!(report.identity().unwrap().0.as_str(), "a2");
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}

#[test]
fn cancel_with_barrier_after_routes_emitted_suffix_only_to_diagnostics() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    s.run(&script, None, facts(), false);
    script.insert(4, step(0, Event::Cancel));
    let report = s.run(&script, Some(3), facts(), false);
    assert_eq!(report.code(), Code::Cancelled);
    let snapshot = report.snapshot().unwrap();
    let member = &snapshot.execution().members()[0];
    assert_eq!(member.process(), ProcessEnd::KilledCancel);
    assert!(!member.run_end_frame());
    assert!(member.records().is_empty());
    assert_eq!(snapshot.execution().delivery_evidence().len(), 3);
    assert_eq!(snapshot.projection_error(), None);
    assert_eq!(snapshot.diagnostics().counts().records, 1);
    assert_eq!(snapshot.diagnostics().counts().other, 0);
    assert_eq!(snapshot.diagnostics().entries().len(), 1);
    assert_eq!(
        assess(member, snapshot.diagnostics()).unwrap().ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
    assert_eq!(
        s.last_good().unwrap().execution().members()[0]
            .records()
            .len(),
        1
    );
}

#[test]
fn cancel_with_out_of_range_barrier_counts_only_emitted_prefix() {
    let mut s = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("scope1-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    s.run(&script, None, facts(), false);
    script.insert(4, step(0, Event::Cancel));
    // The original End/RunEnd suffix is suppressed; only four deliveries exist.
    let report = s.run(&script, Some(5), facts(), false);
    assert_eq!(report.code(), Code::BarrierIndex);
    assert!(report.snapshot().is_none());
    assert_eq!(report.identity().unwrap().0.as_str(), "a2");
    assert_eq!(s.availability(), Availability::Stale);
    assert_eq!(
        s.last_good()
            .unwrap()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}
