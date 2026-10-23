//! Explicit test-only adapter. Planning supplies no enumeration or supervisor facts.
use abf_catalog_plan_fixture::*;
use abf_reader_fixture::{Action, Event, Step, emit};
use abf_source_run::*;

fn planned(locator: &str) -> FixturePlan {
    let d = FixtureDescriptor {
        id: "descriptor1",
        schema: 1,
        revision: "r1",
        scope: CatalogScope::new(EnvironmentKind::Windows, "fictionalW", "f1", "fixture-v1")
            .unwrap(),
        rule: "rule1",
        scope_tag: "scope1",
        approved_classes: &[RootClass::Configured],
        overlaps: None,
    };
    let m = [RecognitionClaim {
        descriptor: "descriptor1",
        descriptor_revision: "r1",
        rule: "rule1",
    }];
    let c = FictionalCandidate {
        id: "candidate1",
        family: "f1",
        namespace: "fictionalW",
        revision: "c1",
        class: RootClass::Configured,
        locator,
        recognition: Some(&m),
    };
    let g = FictionalGrant {
        candidate: "candidate1",
        family: "f1",
        namespace: "fictionalW",
        candidate_revision: "c1",
        class: RootClass::Configured,
        descriptor: "descriptor1",
        descriptor_revision: "r1",
    };
    plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families: &["f1"],
        },
        &[d],
        &[c],
        &[g],
    )
    .unwrap()
}
// Passing this check permits only explicit synthetic test setup, never actual reading.
fn adapter_scope(p: &FixturePlan) -> Result<&CatalogScope, &'static str> {
    let decision = p.decisions().first().ok_or("no conditional scope")?;
    if decision.state() != DecisionState::Conditional {
        return Err("no conditional scope");
    }
    decision.conditional_scope().ok_or("no conditional scope")
}
fn membership<'a>(
    x: &'a FinalizedExecution,
    catalog: &CatalogSnapshot,
) -> Result<ProjectionMembership<'a>, AdaptCode> {
    ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("fictionalW").unwrap()),
            selected_wsl: None,
        },
        &x.members().iter().collect::<Vec<_>>(),
        catalog,
    )
}
// All process/time/drain/distribution values below are injected fictional supervisor claims.
fn finish(
    c: &CatalogSnapshot,
    s: CatalogScope,
    a: AttemptRef,
    z: FinalizationId,
    frames: Vec<ReaderFrame>,
) -> FinalizedExecution {
    finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: a,
                finalization: z,
                members: vec![s],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: frames,
            process: ProcessEnd::ExitedNormally,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(1, 2),
        },
        c,
    )
    .unwrap()
}
fn step(frame_seq: u64, event: Event<'_>) -> Step<'_> {
    Step { frame_seq, event }
}
#[test]
fn literal_record_conservation_and_catalog_allocation_authority() {
    let p = planned(r"C:\Fixture\Root");
    let scope = adapter_scope(&p).unwrap();
    let mut issuer = FixtureRefIssuer::new(p.catalog());
    let source = issuer.create_instance(scope).unwrap();
    assert_eq!(source.as_str(), "scope1-i1");
    let attempt = issuer.begin_execution().unwrap();
    assert_eq!(attempt.as_str(), "a1");
    let finalization = issuer.issue_finalization().unwrap();
    assert_eq!(finalization.as_str(), "z1");
    let sources = [source.clone()];
    let script = [
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
                action: Action::OpenSuccess,
            },
        ),
        step(
            4,
            Event::Record {
                source: &source,
                source_seq: 1,
                bytes: b"{\"tokens\":{\"input\":37}}",
            },
        ),
        step(
            5,
            Event::Source {
                source: &source,
                source_seq: 2,
                action: Action::End(ChangeObservation::NoChangeObserved),
            },
        ),
        step(6, Event::RunEnd),
    ];
    let frames = emit(&attempt, p.catalog(), scope, &script)
        .unwrap()
        .into_frames();
    let clone = p.clone();
    let x = finish(
        clone.catalog(),
        scope.clone(),
        attempt,
        finalization,
        frames,
    );
    assert_eq!(x.members().len(), 1);
    assert_eq!(x.members()[0].sources().len(), 1);
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(x.members()[0].attestation(), AttestationBasis::Unattested);
    let m = membership(&x, p.catalog()).unwrap();
    let paired = project_with_companion(&m).unwrap();
    let w = paired.projection().windows.selected().unwrap();
    assert_eq!(w.stored_occurrences.0, 1);
    assert_eq!(w.runs.run_keys, 1);
    assert_eq!(w.runs.raw_declared_claims.complete, 1);
    assert_eq!(w.runs.inventory, abf_counting::InventoryBasis::Declared);
    assert_eq!(
        w.runs.coverage,
        abf_counting::CoverageState::ReportedComplete
    );
    assert_eq!(
        w.components.input.value,
        abf_counting::ComponentValue::ReportedClaimSum(37)
    );
    assert_eq!(w.usage, abf_counting::UsageTotal::Unsupported);
    assert_eq!(
        w.physical_events,
        abf_counting::PhysicalEventCount::Unresolved
    );
    let diagnostics = PostFinalizationLog::bind(&x).snapshot();
    assert_eq!(paired.assess(&[diagnostics]).unwrap().members().len(), 1);
    let rebuilt = planned(r"C:\Fixture\Root");
    assert_eq!(p.catalog(), rebuilt.catalog());
    assert_eq!(
        membership(&x, rebuilt.catalog()).unwrap_err(),
        AdaptCode::CatalogAuthorityMismatch
    );
    // Locator relocation cannot derive or preserve a source/attempt/finalization identity.
    // Separate fixture issuance remains caller-owned and independent of locator contents.
    let relocated = planned(r"D:\OtherFixture\Root");
    let mut separate = FixtureRefIssuer::new(relocated.catalog());
    assert_eq!(
        separate
            .create_instance(adapter_scope(&relocated).unwrap())
            .unwrap()
            .as_str(),
        "scope1-i1"
    );
    assert_eq!(separate.begin_execution().unwrap().as_str(), "a1");
    assert_eq!(separate.issue_finalization().unwrap().as_str(), "z1");
}
#[test]
fn explicit_empty_success_failed_and_unavailable_enumerations_remain_distinct() {
    let p = planned(r"C:\Fixture\Root");
    let scope = adapter_scope(&p).unwrap();
    let mut issuer = FixtureRefIssuer::new(p.catalog());
    let cases = [
        (
            RawEnumeration::Closed(0),
            CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration),
        ),
        (
            RawEnumeration::Failed(EnumFailure::PermissionDenied),
            CoverageVerdict::InventoryUnknown,
        ),
        (
            RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
            CoverageVerdict::ScopeUnavailable(UnavailableReason::Unavailable),
        ),
        (
            RawEnumeration::RootUnresolved,
            CoverageVerdict::InventoryUnknown,
        ),
    ];
    for (outcome, expected) in cases {
        let attempt = issuer.begin_execution().unwrap();
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
        let frames = emit(&attempt, p.catalog(), scope, &script)
            .unwrap()
            .into_frames();
        let x = finish(
            p.catalog(),
            scope.clone(),
            attempt,
            issuer.issue_finalization().unwrap(),
            frames,
        );
        assert_eq!(x.members()[0].verdict(), expected);
        let m = membership(&x, p.catalog()).unwrap();
        let paired = project_with_companion(&m).unwrap();
        let w = paired.projection().windows.selected().unwrap();
        assert_eq!(w.stored_occurrences.0, 0);
        assert_eq!(w.runs.run_keys, 0);
        assert_eq!(w.runs.raw_declared_claims.complete, 0);
        assert_eq!(
            w.components.input.value,
            abf_counting::ComponentValue::Indeterminate(
                abf_counting::IndeterminateReason::NoRunEvidence
            )
        );
        assert_eq!(w.usage, abf_counting::UsageTotal::Unsupported);
    }
}
#[test]
fn unresolved_unrecognized_no_work_and_refusal_do_not_create_enumerations() {
    let d = FixtureDescriptor {
        id: "descriptor1",
        schema: 1,
        revision: "r1",
        scope: CatalogScope::new(EnvironmentKind::Windows, "fictionalW", "f1", "fixture-v1")
            .unwrap(),
        rule: "rule1",
        scope_tag: "scope1",
        approved_classes: &[RootClass::Configured],
        overlaps: None,
    };
    let r = WindowsRequest {
        namespace: "fictionalW",
        families: &["f1"],
    };
    let unresolved = plan_windows_fixture(&r, std::slice::from_ref(&d), &[], &[]).unwrap();
    assert!(adapter_scope(&unresolved).is_err());
    assert_eq!(unresolved.catalog().scopes().count(), 0);
    // Even a caller-supplied Closed(0) script cannot be emitted against an unresolved scope.
    let attempt = AttemptRef::new("testAttempt").unwrap();
    let script = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(0),
            sources: &[],
        },
    )];
    assert_eq!(
        emit(&attempt, unresolved.catalog(), &d.scope, &script).unwrap_err(),
        abf_reader_fixture::Error::InvalidScript
    );
    let c = FictionalCandidate {
        id: "candidate1",
        family: "f1",
        namespace: "fictionalW",
        revision: "c1",
        class: RootClass::Configured,
        locator: r"C:\Fixture\Root",
        recognition: Some(&[]),
    };
    let unrecognized = plan_windows_fixture(&r, std::slice::from_ref(&d), &[c], &[]).unwrap();
    assert!(adapter_scope(&unrecognized).is_err());
    let empty = plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families: &[],
        },
        std::slice::from_ref(&d),
        &[],
        &[],
    )
    .unwrap();
    assert!(adapter_scope(&empty).is_err());
    let bad = FixtureDescriptor { schema: 2, ..d };
    assert!(matches!(
        plan_windows_fixture(&r, &[bad], &[], &[]),
        Err(PlanCode::UnsupportedSchema)
    ));
}
