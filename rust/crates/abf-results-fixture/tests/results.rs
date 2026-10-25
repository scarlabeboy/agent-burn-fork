use abf_catalog_plan_fixture::*;
use abf_collection_boundary::identity::{EnvironmentIdentity, OpaqueId, WslDistroName};
use abf_reader_fixture::{Action, Event, Step};
use abf_results_fixture::*;
use abf_selected_wsl_fixture::{Delivery, Request, Transport};
use abf_source_run::*;
use abf_supervisor_fixture::{Availability, Code, Supervisor, SyntheticFacts};

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
            scope_tag: "win",
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
fn wsl(name: &str) -> EnvironmentIdentity {
    EnvironmentIdentity::wsl(
        OpaqueId::new("fictionalWsl").unwrap(),
        WslDistroName::new(name).unwrap(),
    )
}
fn transport(env: &EnvironmentIdentity) -> (Transport, CatalogSnapshot) {
    let scope = CatalogScope::new(
        EnvironmentKind::SelectedWsl,
        "fictionalWsl",
        "f1",
        "fixture-v1",
    )
    .unwrap();
    let mut catalog = CatalogBuilder::new();
    catalog.add(scope.clone(), "wsl").unwrap();
    let catalog = catalog.build();
    (
        Transport::new(std::slice::from_ref(env), catalog.clone(), scope).unwrap(),
        catalog,
    )
}
fn facts() -> SyntheticFacts {
    SyntheticFacts {
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        times: TimeObservations::new(11, 19),
    }
}
fn step(frame_seq: u64, event: Event<'_>) -> Step<'_> {
    Step { frame_seq, event }
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
fn request<'a>(env: &'a EnvironmentIdentity, script: &'a [Step<'a>]) -> Request<'a> {
    Request {
        target: env,
        responding_environment: env,
        script,
        delivery: Delivery::Complete,
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        times: TimeObservations::new(101, 109),
    }
}

#[test]
fn windows_only_requires_no_wsl_capability_and_preserves_fresh_evidence() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("win-i1").unwrap();
    let sources = [source.clone()];
    supervisor.run(&complete(&source, &sources), None, facts(), false);
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    assert!(results.selected_wsl().is_none());
    let windows = results.windows().unwrap();
    assert_eq!(windows.source().availability(), Availability::Fresh);
    assert_eq!(windows.freshness(), Freshness::Fresh);
    assert_eq!(windows.source().latest().unwrap().code(), Code::Completed);
    let latest = windows.latest().unwrap();
    assert!(std::ptr::eq(
        latest.evidence(),
        windows.last_good().unwrap().evidence()
    ));
    assert!(std::ptr::eq(
        latest.evidence(),
        supervisor.latest().unwrap().snapshot().unwrap()
    ));
    assert!(std::ptr::eq(
        windows.last_good().unwrap().evidence(),
        supervisor.last_good().unwrap()
    ));
    assert_eq!(
        latest.evidence().execution().times(),
        &TimeObservations::new(11, 19)
    );
    assert_eq!(
        latest.assessment().as_ref().unwrap().ceiling,
        PresentationCeiling::Original
    );
    assert_eq!(
        latest
            .projection()
            .as_ref()
            .unwrap()
            .projection()
            .windows
            .selected()
            .unwrap()
            .stored_occurrences
            .0,
        1
    );
    assert_eq!(
        latest.evidence().execution().members()[0].records()[0].source(),
        &source
    );
    assert_eq!(
        latest.evidence().execution().members()[0].attestation(),
        AttestationBasis::Unattested
    );
}

#[test]
fn both_keeps_distinct_authorities_and_colliding_local_attempt_labels() {
    let plan = plan();
    let windows_catalog = plan.catalog().clone();
    let mut supervisor = Supervisor::new(plan).unwrap();
    let source = SourceRef::new("win-i1").unwrap();
    let sources = [source.clone()];
    supervisor.run(&complete(&source, &sources), None, facts(), false);
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let wsl_source = SourceRef::new("wsl-i1").unwrap();
    let wsl_sources = [wsl_source.clone()];
    let script = complete(&wsl_source, &wsl_sources);
    let response = transport
        .exchange(Selection::Both, request(&env, &script), false)
        .unwrap();
    let results = present(Inputs::Both {
        windows: &supervisor,
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    assert_eq!(results.selection(), Selection::Both);
    let win = results.windows().unwrap().latest().unwrap();
    let wsl = results.selected_wsl().unwrap();
    assert_eq!(wsl.freshness(), Freshness::Unestablished);
    assert_eq!(wsl.selected(), &env);
    assert_eq!(
        win.evidence().execution().members()[0].identity(),
        response.identity()
    );
    assert_eq!(
        wsl.projection()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .selection(),
        Selection::SelectedWslOnly
    );
    assert_eq!(
        win.projection().as_ref().unwrap().selection(),
        Selection::WindowsOnly
    );
    assert_eq!(
        response.execution().unwrap().times(),
        &TimeObservations::new(101, 109)
    );
    assert!(matches!(
        ProjectionMembership::validate(
            Selection::Both,
            SelectedRefs {
                windows: Some(OpaqueRef::new("fictionalW").unwrap()),
                selected_wsl: Some(OpaqueRef::new("fictionalWsl").unwrap())
            },
            &[
                &win.evidence().execution().members()[0],
                &response.execution().unwrap().members()[0]
            ],
            &windows_catalog,
        ),
        Err(AdaptCode::CatalogAuthorityMismatch)
    ));
    assert_eq!(transport.calls(), 1);
}

#[test]
fn selected_wsl_requires_one_explicit_matching_distribution_even_without_response() {
    let supervisor = Supervisor::new(plan()).unwrap();
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let response = transport
        .exchange(Selection::SelectedWslOnly, request(&env, &[]), false)
        .unwrap();
    for selected in [
        vec![],
        vec![env.clone(), env.clone()],
        vec![EnvironmentIdentity::windows(
            OpaqueId::new("fictionalWsl").unwrap(),
        )],
    ] {
        for input in [
            Inputs::SelectedWsl {
                selected: &selected,
                response: Ok(&response),
            },
            Inputs::Both {
                windows: &supervisor,
                selected: &selected,
                response: Ok(&response),
            },
        ] {
            assert!(matches!(
                present(input),
                Err(Error::ExplicitSelectedWslRequired)
            ));
        }
    }
    let other = [wsl("Other-Ubuntu")];
    for input in [
        Inputs::SelectedWsl {
            selected: &other,
            response: Ok(&response),
        },
        Inputs::Both {
            windows: &supervisor,
            selected: &other,
            response: Ok(&response),
        },
    ] {
        assert!(matches!(present(input), Err(Error::EnvironmentMismatch)));
    }
    assert!(matches!(
        present(Inputs::SelectedWsl {
            selected: &[],
            response: Err(abf_selected_wsl_fixture::Error::CancelledBeforeCall)
        }),
        Err(Error::ExplicitSelectedWslRequired)
    ));
}

#[test]
fn projection_and_complete_delivery_do_not_upgrade_missing_collection_completion() {
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let source = SourceRef::new("wsl-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    script.truncate(4);
    let response = transport
        .exchange(Selection::SelectedWslOnly, request(&env, &script), false)
        .unwrap();
    let results = present(Inputs::SelectedWsl {
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    assert!(results.windows().is_none());
    let wsl = results.selected_wsl().unwrap();
    assert!(wsl.projection().as_ref().unwrap().is_ok());
    assert_eq!(wsl.response().unwrap().delivery(), Delivery::Complete);
    assert_eq!(
        wsl.response().unwrap().execution().unwrap().members()[0].verdict(),
        CoverageVerdict::PartiallyCovered
    );
    assert_eq!(wsl.freshness(), Freshness::Unestablished);
}

#[test]
fn stale_windows_keeps_good_data_and_latest_unavailable_attempt_separate() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("win-i1").unwrap();
    let sources = [source.clone()];
    supervisor.run(&complete(&source, &sources), None, facts(), false);
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
    supervisor.run(&failed, None, facts(), false);
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    let windows = results.windows().unwrap();
    assert_eq!(windows.freshness(), Freshness::Stale);
    assert_eq!(windows.source().availability(), Availability::Stale);
    assert_eq!(
        windows.latest().unwrap().evidence().execution().members()[0].verdict(),
        CoverageVerdict::ScopeUnavailable(UnavailableReason::Unavailable)
    );
    let good = windows.last_good().unwrap();
    assert_eq!(
        good.evidence().execution().manifest().attempt.as_str(),
        "a1"
    );
    assert_eq!(
        windows
            .source()
            .latest()
            .unwrap()
            .identity()
            .unwrap()
            .0
            .as_str(),
        "a2"
    );
    assert_eq!(
        good.projection()
            .as_ref()
            .unwrap()
            .projection()
            .windows
            .selected()
            .unwrap()
            .stored_occurrences
            .0,
        1
    );
}

#[test]
fn unattempted_windows_unavailable_is_distinct_from_wsl_unestablished() {
    let supervisor = Supervisor::new(plan()).unwrap();
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    let windows = results.windows().unwrap();
    assert_eq!(windows.source().availability(), Availability::Unavailable);
    assert_eq!(windows.freshness(), Freshness::Unavailable);
    assert!(windows.source().latest().is_none());
    assert!(windows.latest().is_none());
    assert!(windows.last_good().is_none());
    let env = wsl("Fictional-Ubuntu");
    let results = present(Inputs::SelectedWsl {
        selected: std::slice::from_ref(&env),
        response: Err(abf_selected_wsl_fixture::Error::CancelledBeforeCall),
    })
    .unwrap();
    assert_eq!(
        results.selected_wsl().unwrap().freshness(),
        Freshness::Unestablished
    );
}

#[test]
fn pre_cancel_preserves_windows_staleness_without_inventing_attempt_identity() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("win-i1").unwrap();
    let sources = [source.clone()];
    supervisor.run(&complete(&source, &sources), None, facts(), false);
    supervisor.run(&[], None, facts(), true);
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    let windows = results.windows().unwrap();
    assert_eq!(windows.freshness(), Freshness::Stale);
    assert_eq!(windows.source().latest().unwrap().code(), Code::Cancelled);
    assert!(windows.source().latest().unwrap().identity().is_none());
    assert!(windows.latest().is_none());
    assert_eq!(
        windows
            .last_good()
            .unwrap()
            .evidence()
            .execution()
            .manifest()
            .attempt
            .as_str(),
        "a1"
    );
}

#[test]
fn late_windows_diagnostics_weaken_assessment_despite_successful_projection() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let source = SourceRef::new("win-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    script.push(step(
        0,
        Event::Replay {
            original: 3,
            replacement: None,
        },
    ));
    supervisor.run(&script, Some(6), facts(), false);
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    let windows = results.windows().unwrap();
    assert_eq!(windows.source().availability(), Availability::Unavailable);
    assert!(windows.last_good().is_none());
    let latest = windows.latest().unwrap();
    assert!(latest.projection().is_ok());
    assert_eq!(
        latest.evidence().execution().members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(
        latest.assessment().as_ref().unwrap().ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
    assert!(matches!(
        latest.assessment().as_ref().unwrap().post,
        PostFinalization::Observed { records: 1, .. }
    ));
}

#[test]
fn wsl_interrupted_prefix_and_cancellation_preserve_exact_response_evidence() {
    for (delivery, cancelled) in [
        (Delivery::Failed { after: 4 }, false),
        (Delivery::Incomplete { after: 4 }, false),
        (Delivery::Cancelled { after: 4 }, true),
    ] {
        let env = wsl("Fictional-Ubuntu");
        let (mut transport, _) = transport(&env);
        let source = SourceRef::new("wsl-i1").unwrap();
        let sources = [source.clone()];
        let script = complete(&source, &sources);
        let mut input = request(&env, &script);
        input.delivery = delivery;
        let response = transport.exchange(Selection::Both, input, false).unwrap();
        let results = present(Inputs::SelectedWsl {
            selected: std::slice::from_ref(&env),
            response: Ok(&response),
        })
        .unwrap();
        let wsl = results.selected_wsl().unwrap();
        let retained = wsl.response().unwrap();
        assert!(std::ptr::eq(retained, &response));
        assert_eq!(retained.delivery(), delivery);
        assert_eq!(retained.cancelled(), cancelled);
        assert_eq!(
            retained.execution().unwrap().members()[0].verdict(),
            CoverageVerdict::PartiallyCovered
        );
        assert_eq!(
            retained.execution().unwrap().members()[0].records()[0].source(),
            &source
        );
        assert_eq!(
            retained.execution().unwrap().members()[0].records()[0].frame_seq(),
            4
        );
        assert!(wsl.projection().as_ref().unwrap().is_ok());
        assert_eq!(wsl.freshness(), Freshness::Unestablished);
        assert_eq!(transport.calls(), 1);
    }
}

#[test]
fn refused_wsl_finalization_keeps_cancellation_identity_and_exact_error() {
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let mut input = request(&env, &[]);
    input.delivery = Delivery::Cancelled { after: 0 };
    input.barrier = Barrier::NotDrained;
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    let results = present(Inputs::SelectedWsl {
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    let wsl = results.selected_wsl().unwrap();
    assert!(wsl.response().unwrap().cancelled());
    assert_eq!(wsl.response().unwrap().identity().0.as_str(), "a1");
    assert!(matches!(
        wsl.projection(),
        Some(Err(abf_selected_wsl_fixture::ProjectionError::Execution(
            abf_selected_wsl_fixture::Error::Finalize(FinalizeCode::InvalidBarrier)
        )))
    ));
}

#[test]
fn cancelled_projection_refusal_preserves_all_129_sources_in_each_environment() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let windows_sources: Vec<_> = (1..=129)
        .map(|n| SourceRef::new(&format!("win-i{n}")).unwrap())
        .collect();
    let script = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(129),
                sources: &windows_sources,
            },
        ),
        step(2, Event::Cancel),
    ];
    supervisor.run(&script, None, facts(), false);
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let wsl_sources: Vec<_> = (1..=129)
        .map(|n| SourceRef::new(&format!("wsl-i{n}")).unwrap())
        .collect();
    let script = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(129),
                sources: &wsl_sources,
            },
        ),
        step(2, Event::Cancel),
    ];
    let response = transport
        .exchange(Selection::Both, request(&env, &script), false)
        .unwrap();
    let results = present(Inputs::Both {
        windows: &supervisor,
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    let windows = results.windows().unwrap();
    assert_eq!(windows.source().latest().unwrap().code(), Code::Cancelled);
    assert!(matches!(
        windows.latest().unwrap().projection(),
        Err(AdaptCode::DeclarationCeiling)
    ));
    assert_eq!(
        windows.latest().unwrap().evidence().execution().members()[0]
            .sources()
            .len(),
        129
    );
    assert!(windows.latest().unwrap().assessment().is_ok());
    let wsl = results.selected_wsl().unwrap();
    assert!(wsl.response().unwrap().cancelled());
    assert!(matches!(
        wsl.projection(),
        Some(Err(abf_selected_wsl_fixture::ProjectionError::Admission(
            AdaptCode::DeclarationCeiling
        )))
    ));
    assert_eq!(
        wsl.response().unwrap().execution().unwrap().members()[0]
            .sources()
            .len(),
        129
    );
}

#[test]
fn both_preserves_fresh_or_stale_windows_when_wsl_is_unavailable_before_a_call() {
    let env = wsl("Fictional-Ubuntu");
    for (pre_cancel, expected_freshness, good_attempt) in [
        (false, Freshness::Fresh, "a2"),
        (true, Freshness::Stale, "a1"),
    ] {
        let mut supervisor = Supervisor::new(plan()).unwrap();
        let source = SourceRef::new("win-i1").unwrap();
        let sources = [source.clone()];
        let script = complete(&source, &sources);
        supervisor.run(&script, None, facts(), false);
        supervisor.run(&script, None, facts(), pre_cancel);
        for error in [
            abf_selected_wsl_fixture::Error::DistributionMismatch,
            abf_selected_wsl_fixture::Error::CancelledBeforeCall,
        ] {
            let results = present(Inputs::Both {
                windows: &supervisor,
                selected: std::slice::from_ref(&env),
                response: Err(error),
            })
            .unwrap();
            let windows = results.windows().unwrap();
            assert_eq!(windows.freshness(), expected_freshness);
            let good = windows.last_good().unwrap();
            assert!(std::ptr::eq(
                good.evidence(),
                supervisor.last_good().unwrap()
            ));
            assert_eq!(
                good.evidence().execution().manifest().attempt.as_str(),
                good_attempt
            );
            assert_eq!(
                good.projection()
                    .as_ref()
                    .unwrap()
                    .projection()
                    .windows
                    .selected()
                    .unwrap()
                    .stored_occurrences
                    .0,
                1
            );
            assert_eq!(
                windows.source().latest().unwrap().identity().is_none(),
                pre_cancel
            );
            let wsl = results.selected_wsl().unwrap();
            assert_eq!(wsl.response().unwrap_err(), error);
            assert!(wsl.projection().is_none());
            assert_eq!(wsl.freshness(), Freshness::Unestablished);
        }
    }
}

#[test]
fn repeated_presentation_is_deterministic_and_makes_no_exchanges() {
    let mut supervisor = Supervisor::new(plan()).unwrap();
    let win_source = SourceRef::new("win-i1").unwrap();
    let win_sources = [win_source.clone()];
    supervisor.run(&complete(&win_source, &win_sources), None, facts(), false);
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let source = SourceRef::new("wsl-i1").unwrap();
    let sources = [source.clone()];
    let script = complete(&source, &sources);
    let response = transport
        .exchange(Selection::Both, request(&env, &script), false)
        .unwrap();
    let a = present(Inputs::Both {
        windows: &supervisor,
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    let b = present(Inputs::Both {
        windows: &supervisor,
        selected: std::slice::from_ref(&env),
        response: Ok(&response),
    })
    .unwrap();
    assert_eq!(a.selection(), b.selection());
    assert_eq!(
        a.selected_wsl()
            .unwrap()
            .projection()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .projection()
            .selected_wsl,
        b.selected_wsl()
            .unwrap()
            .projection()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .projection()
            .selected_wsl
    );
    assert_eq!(
        a.windows().unwrap().freshness(),
        b.windows().unwrap().freshness()
    );
    let first = a.windows().unwrap().latest().unwrap();
    let second = b.windows().unwrap().latest().unwrap();
    assert_eq!(
        first.projection().as_ref().unwrap().projection().windows,
        second.projection().as_ref().unwrap().projection().windows
    );
    assert_eq!(
        first
            .projection()
            .as_ref()
            .unwrap()
            .projection()
            .windows
            .selected()
            .unwrap()
            .stored_occurrences
            .0,
        1
    );
    assert_eq!(first.assessment(), second.assessment());
    assert!(std::ptr::eq(first.evidence(), second.evidence()));
    assert_eq!(a.windows().unwrap().freshness(), Freshness::Fresh);
    assert_eq!(transport.calls(), 1);
}

#[test]
fn presentation_debug_redacts_evidence_and_selection() {
    let supervisor = Supervisor::new(plan()).unwrap();
    let input = Inputs::WindowsOnly(&supervisor);
    assert_eq!(format!("{input:?}"), "Inputs([redacted])");
    let results = present(input).unwrap();
    assert_eq!(format!("{results:?}"), "Results([redacted])");
    assert_eq!(
        format!("{:?}", results.windows().unwrap()),
        "WindowsView([redacted])"
    );
    let env = wsl("Fictional-Ubuntu");
    let results = present(Inputs::SelectedWsl {
        selected: std::slice::from_ref(&env),
        response: Err(abf_selected_wsl_fixture::Error::CancelledBeforeCall),
    })
    .unwrap();
    assert_eq!(
        format!("{:?}", results.selected_wsl().unwrap()),
        "WslView([redacted])"
    );
}

#[test]
fn wsl_failure_after_a_covered_response_does_not_invent_last_good_or_freshness() {
    let env = wsl("Fictional-Ubuntu");
    let (mut transport, _) = transport(&env);
    let source = SourceRef::new("wsl-i1").unwrap();
    let sources = [source.clone()];
    let script = complete(&source, &sources);
    let good = transport
        .exchange(Selection::SelectedWslOnly, request(&env, &script), false)
        .unwrap();
    let selected = std::slice::from_ref(&env);
    let first = present(Inputs::SelectedWsl {
        selected,
        response: Ok(&good),
    })
    .unwrap();
    assert_eq!(
        first
            .selected_wsl()
            .unwrap()
            .response()
            .unwrap()
            .execution()
            .unwrap()
            .members()[0]
            .verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    let mut input = request(&env, &script);
    input.delivery = Delivery::Failed { after: 0 };
    let failed = transport
        .exchange(Selection::SelectedWslOnly, input, false)
        .unwrap();
    let second = present(Inputs::SelectedWsl {
        selected,
        response: Ok(&failed),
    })
    .unwrap();
    let wsl = second.selected_wsl().unwrap();
    assert_eq!(wsl.response().unwrap().identity().0.as_str(), "a2");
    assert_eq!(
        wsl.response().unwrap().execution().unwrap().members()[0].verdict(),
        CoverageVerdict::NoAttempt
    );
    assert_eq!(wsl.freshness(), Freshness::Unestablished);
    assert!(wsl.projection().as_ref().unwrap().is_ok());
    assert_eq!(
        first
            .selected_wsl()
            .unwrap()
            .response()
            .unwrap()
            .identity()
            .0
            .as_str(),
        "a1"
    );
    assert_eq!(transport.calls(), 2);
}

#[test]
fn wsl_selections_reject_only_contradictory_windows_only_error() {
    let supervisor = Supervisor::new(plan()).unwrap();
    let env = wsl("Fictional-Ubuntu");
    let selected = std::slice::from_ref(&env);
    for input in [
        Inputs::SelectedWsl {
            selected,
            response: Err(abf_selected_wsl_fixture::Error::WindowsOnly),
        },
        Inputs::Both {
            windows: &supervisor,
            selected,
            response: Err(abf_selected_wsl_fixture::Error::WindowsOnly),
        },
    ] {
        assert!(matches!(present(input), Err(Error::SelectionMismatch)));
    }
    for error in [
        abf_selected_wsl_fixture::Error::SingleSelectedDistributionRequired,
        abf_selected_wsl_fixture::Error::ScopeBinding,
        abf_selected_wsl_fixture::Error::CancelledBeforeCall,
        abf_selected_wsl_fixture::Error::DistributionMismatch,
        abf_selected_wsl_fixture::Error::DeliveryIndex,
        abf_selected_wsl_fixture::Error::Identity(RefCode::CounterExhausted),
        abf_selected_wsl_fixture::Error::Reader(abf_reader_fixture::Error::BudgetExceeded),
        abf_selected_wsl_fixture::Error::Finalize(FinalizeCode::InvalidBarrier),
    ] {
        for input in [
            Inputs::SelectedWsl {
                selected,
                response: Err(error),
            },
            Inputs::Both {
                windows: &supervisor,
                selected,
                response: Err(error),
            },
        ] {
            let results = present(input).unwrap();
            let wsl = results.selected_wsl().unwrap();
            assert_eq!(wsl.response().unwrap_err(), error);
            assert!(wsl.projection().is_none());
        }
    }
}
