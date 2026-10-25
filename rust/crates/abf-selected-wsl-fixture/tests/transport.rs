use abf_collection_boundary::identity::{EnvironmentIdentity, OpaqueId, WslDistroName};
use abf_reader_fixture::{Action, Event, Step};
use abf_selected_wsl_fixture::*;
use abf_source_run::*;

fn environment(name: &str) -> EnvironmentIdentity {
    EnvironmentIdentity::wsl(
        OpaqueId::new("fictionalWsl").unwrap(),
        WslDistroName::new(name).unwrap(),
    )
}
fn setup() -> (EnvironmentIdentity, CatalogSnapshot, CatalogScope) {
    let env = environment("Fictional-Ubuntu");
    let scope = CatalogScope::new(
        EnvironmentKind::SelectedWsl,
        "fictionalWsl",
        "f1",
        "fixture-v1",
    )
    .unwrap();
    let mut builder = CatalogBuilder::new();
    builder.add(scope.clone(), "wsl").unwrap();
    (env, builder.build(), scope)
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
        times: TimeObservations::new(1, 2),
    }
}
#[test]
fn exactly_one_explicit_wsl_identity_is_required() {
    let (env, catalog, scope) = setup();
    assert!(matches!(
        Transport::new(&[], catalog.clone(), scope.clone()),
        Err(Error::SingleSelectedDistributionRequired)
    ));
    assert!(matches!(
        Transport::new(&[env.clone(), env.clone()], catalog.clone(), scope.clone()),
        Err(Error::SingleSelectedDistributionRequired)
    ));
    let windows = EnvironmentIdentity::windows(OpaqueId::new("fictionalWsl").unwrap());
    assert!(matches!(
        Transport::new(&[windows], catalog.clone(), scope.clone()),
        Err(Error::SingleSelectedDistributionRequired)
    ));
    assert!(Transport::new(&[env], catalog, scope).is_ok());
}
#[test]
fn windows_only_and_pre_cancel_make_no_calls_or_issue_identities() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    assert!(matches!(
        transport.exchange(Selection::WindowsOnly, request(&env, &[]), false),
        Err(Error::WindowsOnly)
    ));
    assert!(matches!(
        transport.exchange(Selection::Both, request(&env, &[]), true),
        Err(Error::CancelledBeforeCall)
    ));
    assert_eq!(transport.calls(), 0);
    let response = transport
        .exchange(Selection::SelectedWslOnly, request(&env, &[]), false)
        .unwrap();
    assert_eq!(response.identity().0.as_str(), "a1");
    assert_eq!(response.identity().1.as_str(), "z1");
}
#[test]
fn another_distribution_with_the_same_environment_label_is_refused() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let other = environment("Fictional-Other");
    assert!(matches!(
        transport.exchange(Selection::Both, request(&other, &[]), false),
        Err(Error::DistributionMismatch)
    ));
    let mut reply = request(&env, &[]);
    reply.responding_environment = &other;
    assert!(matches!(
        transport.exchange(Selection::Both, reply, false),
        Err(Error::DistributionMismatch)
    ));
    assert_eq!(transport.calls(), 0);
}
#[test]
fn completed_response_preserves_provenance_and_original_catalog_authority() {
    let (env, catalog, scope) = setup();
    let mut transport =
        Transport::new(std::slice::from_ref(&env), catalog.clone(), scope.clone()).unwrap();
    let source = SourceRef::new("wsl-i1").unwrap();
    let sources = [source.clone()];
    let script = complete(&source, &sources);
    let response = transport
        .exchange(Selection::Both, request(&env, &script), false)
        .unwrap();
    assert_eq!(response.environment(), &env);
    assert_eq!(response.delivery(), Delivery::Complete);
    assert!(!response.cancelled());
    let execution = response.execution().unwrap();
    let member = &execution.members()[0];
    assert_eq!(member.scope(), &scope);
    assert_eq!(member.identity(), response.identity());
    assert_eq!(member.records()[0].source(), &source);
    assert_eq!(member.records()[0].source_seq(), 1);
    assert_eq!(member.records()[0].frame_seq(), 4);
    assert_eq!(member.attestation(), AttestationBasis::Unattested);
    assert_eq!(
        member.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert!(member.run_end_frame());
    let selected = SelectedRefs {
        windows: None,
        selected_wsl: Some(scope.environment_ref().clone()),
    };
    assert!(
        ProjectionMembership::validate(
            Selection::SelectedWslOnly,
            selected.clone(),
            &[member],
            &catalog
        )
        .is_ok()
    );
    assert!(response.project().is_ok());
    let mut rebuilt = CatalogBuilder::new();
    rebuilt.add(scope, "wsl").unwrap();
    assert!(matches!(
        ProjectionMembership::validate(
            Selection::SelectedWslOnly,
            selected,
            &[member],
            &rebuilt.build()
        ),
        Err(AdaptCode::CatalogAuthorityMismatch)
    ));
    assert_eq!(transport.calls(), 1);
}
#[test]
fn interrupted_delivery_retains_exact_prefix_without_fabricated_completion() {
    for (delivery, process, cancelled) in [
        (
            Delivery::Failed { after: 4 },
            ProcessEnd::TransportLost,
            false,
        ),
        (
            Delivery::Incomplete { after: 4 },
            ProcessEnd::TransportLost,
            false,
        ),
        (
            Delivery::Cancelled { after: 4 },
            ProcessEnd::KilledCancel,
            true,
        ),
    ] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let source = SourceRef::new("wsl-i1").unwrap();
        let sources = [source.clone()];
        let script = complete(&source, &sources);
        let mut input = request(&env, &script);
        input.delivery = delivery;
        let response = transport
            .exchange(Selection::SelectedWslOnly, input, false)
            .unwrap();
        assert_eq!(response.delivery(), delivery);
        assert_eq!(response.cancelled(), cancelled);
        let x = response.execution().unwrap();
        assert_eq!(x.dispositions().len(), 4);
        let member = &x.members()[0];
        assert_eq!(member.records().len(), 1);
        assert_eq!(member.records()[0].frame_seq(), 4);
        assert_eq!(member.records()[0].source(), &source);
        assert_eq!(member.process(), process);
        assert!(!member.run_end_frame());
        assert_eq!(member.verdict(), CoverageVerdict::PartiallyCovered);
        assert_eq!(response.environment(), &env);
        assert!(response.project().is_ok());
    }
}
#[test]
fn failure_before_any_delivery_preserves_g4_no_attempt_not_empty_success() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let mut input = request(&env, &[]);
    input.delivery = Delivery::Failed { after: 0 };
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert_eq!(
        response.execution().unwrap().members()[0].verdict(),
        CoverageVerdict::NoAttempt
    );
}
#[test]
fn incomplete_cutoff_outside_emitted_deliveries_is_refused() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let mut input = request(&env, &[]);
    input.delivery = Delivery::Incomplete { after: usize::MAX };
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert_eq!(response.execution().unwrap_err(), Error::DeliveryIndex);
    assert_eq!(response.identity().0.as_str(), "a1");
}
#[test]
fn cancellation_and_delivery_loss_do_not_bypass_suffix_resource_preflight() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let source = SourceRef::new("wsl-i1").unwrap();
    let oversized = vec![b'x'; abf_reader_fixture::MAX_RECORD_BYTES + 1];
    let script = [
        step(1, Event::Cancel),
        step(
            2,
            Event::Record {
                source: &source,
                source_seq: 1,
                bytes: &oversized,
            },
        ),
    ];
    let mut input = request(&env, &script);
    input.delivery = Delivery::Failed { after: 0 };
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert_eq!(
        response.execution().unwrap_err(),
        Error::Reader(abf_reader_fixture::Error::BudgetExceeded)
    );
    assert_eq!(response.identity().1.as_str(), "z1");
}
#[test]
fn s1_cancel_remains_primary_and_projection_refusal_retains_full_evidence() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let sources: Vec<_> = (1..=129)
        .map(|n| SourceRef::new(&format!("wsl-i{n}")).unwrap())
        .collect();
    let script = [
        step(
            1,
            Event::Enumeration {
                outcome: RawEnumeration::Closed(129),
                sources: &sources,
            },
        ),
        step(2, Event::Cancel),
    ];
    let mut input = request(&env, &script);
    input.process = ProcessEnd::Crashed;
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert!(response.cancelled());
    assert_eq!(
        response.execution().unwrap().members()[0].process(),
        ProcessEnd::KilledCancel
    );
    assert_eq!(
        response.execution().unwrap().members()[0].sources().len(),
        129
    );
    assert!(matches!(
        response.project(),
        Err(ProjectionError::Admission(AdaptCode::DeclarationCeiling))
    ));
    assert!(response.cancelled());
}
#[test]
fn supplied_undrained_barrier_is_never_upgraded() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let mut input = request(&env, &[]);
    input.delivery = Delivery::Cancelled { after: 0 };
    input.barrier = Barrier::NotDrained;
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert!(response.cancelled());
    assert_eq!(
        response.execution().unwrap_err(),
        Error::Finalize(FinalizeCode::InvalidBarrier)
    );
}
#[test]
fn scope_kind_environment_and_membership_must_match_the_selection() {
    let (env, catalog, scope) = setup();
    let windows =
        CatalogScope::new(EnvironmentKind::Windows, "fictionalWsl", "f1", "fixture-v1").unwrap();
    assert!(matches!(
        Transport::new(std::slice::from_ref(&env), catalog.clone(), windows),
        Err(Error::ScopeBinding)
    ));
    let foreign =
        CatalogScope::new(EnvironmentKind::SelectedWsl, "other", "f1", "fixture-v1").unwrap();
    assert!(matches!(
        Transport::new(std::slice::from_ref(&env), catalog, foreign),
        Err(Error::ScopeBinding)
    ));
    assert!(matches!(
        Transport::new(&[env], CatalogBuilder::new().build(), scope),
        Err(Error::ScopeBinding)
    ));
}
#[test]
fn identical_explicit_input_sequences_produce_identical_evidence() {
    let (env, catalog, scope) = setup();
    let mut a = Transport::new(std::slice::from_ref(&env), catalog.clone(), scope.clone()).unwrap();
    let mut b = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let source = SourceRef::new("wsl-i1").unwrap();
    let sources = [source.clone()];
    let mut script = complete(&source, &sources);
    script.push(step(
        0,
        Event::Replay {
            original: 3,
            replacement: None,
        },
    ));
    for delivery in [
        Delivery::Complete,
        Delivery::Incomplete { after: 4 },
        Delivery::Cancelled { after: 4 },
    ] {
        let mut first = request(&env, &script);
        first.delivery = delivery;
        let mut second = request(&env, &script);
        second.delivery = delivery;
        let x = a.exchange(Selection::Both, first, false).unwrap();
        let y = b.exchange(Selection::Both, second, false).unwrap();
        assert_eq!(x.identity(), y.identity());
        assert_eq!(x.environment(), y.environment());
        assert_eq!(x.delivery(), y.delivery());
        assert_eq!(x.cancelled(), y.cancelled());
        let xe = x.execution().unwrap();
        let ye = y.execution().unwrap();
        assert_eq!(xe.times(), ye.times());
        assert_eq!(xe.dispositions(), ye.dispositions());
        assert_eq!(xe.delivery_evidence().len(), ye.delivery_evidence().len());
        for (left, right) in xe.delivery_evidence().iter().zip(ye.delivery_evidence()) {
            assert_eq!(left.frame_seq(), right.frame_seq());
            assert_eq!(left.source(), right.source());
            assert_eq!(left.source_seq(), right.source_seq());
            assert_eq!(left.terminal(), right.terminal());
            assert_eq!(left.enumeration(), right.enumeration());
            assert_eq!(left.enumerated_at(), right.enumerated_at());
        }
        let xm = &xe.members()[0];
        let ym = &ye.members()[0];
        assert_eq!(xm.records(), ym.records());
        assert_eq!(xm.sources(), ym.sources());
        assert_eq!(xm.verdict(), ym.verdict());
        assert_eq!(xm.delivery(), ym.delivery());
        assert_eq!(
            x.project().unwrap().projection().selected_wsl,
            y.project().unwrap().projection().selected_wsl
        );
    }
    assert_eq!(a.calls(), 3);
    assert_eq!(b.calls(), 3);
}
#[test]
fn steps_are_bounded_even_when_transport_cancels_before_delivery() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    let mut script = vec![step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(0),
            sources: &[],
        },
    )];
    script.extend((1..abf_reader_fixture::MAX_STEPS).map(|_| {
        step(
            0,
            Event::Replay {
                original: 0,
                replacement: None,
            },
        )
    }));
    let response = transport
        .exchange(Selection::SelectedWslOnly, request(&env, &script), false)
        .unwrap();
    assert_eq!(response.execution().unwrap().dispositions().len(), 512);
    script.push(step(
        0,
        Event::Replay {
            original: 0,
            replacement: None,
        },
    ));
    let mut input = request(&env, &script);
    input.delivery = Delivery::Cancelled { after: 0 };
    let refused = transport.exchange(Selection::Both, input, false).unwrap();
    assert_eq!(
        refused.execution().unwrap_err(),
        Error::Reader(abf_reader_fixture::Error::BudgetExceeded)
    );
}
#[test]
fn record_size_and_cumulative_bytes_are_bounded_before_truncation() {
    for sizes in [vec![65536], vec![65536, 65536]] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let source = SourceRef::new("wsl-i1").unwrap();
        let sources = [source.clone()];
        let payloads: Vec<_> = sizes.iter().map(|n| vec![b'x'; *n]).collect();
        let mut script = complete(&source, &sources);
        script.truncate(3);
        script.extend(payloads.iter().enumerate().map(|(i, bytes)| {
            step(
                i as u64 + 4,
                Event::Record {
                    source: &source,
                    source_seq: i as u64 + 1,
                    bytes,
                },
            )
        }));
        let accepted = transport
            .exchange(Selection::Both, request(&env, &script), false)
            .unwrap();
        assert_eq!(
            accepted.execution().unwrap().members()[0].records().len(),
            sizes.len()
        );
    }
    for sizes in [vec![65537], vec![65536, 65536, 1]] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let source = SourceRef::new("wsl-i1").unwrap();
        let payloads: Vec<_> = sizes.iter().map(|n| vec![b'x'; *n]).collect();
        let mut script = vec![step(1, Event::Cancel)];
        script.extend(payloads.iter().enumerate().map(|(i, bytes)| {
            step(
                i as u64 + 2,
                Event::Record {
                    source: &source,
                    source_seq: i as u64 + 1,
                    bytes,
                },
            )
        }));
        let mut input = request(&env, &script);
        input.delivery = Delivery::Incomplete { after: 0 };
        let refused = transport.exchange(Selection::Both, input, false).unwrap();
        assert_eq!(
            refused.execution().unwrap_err(),
            Error::Reader(abf_reader_fixture::Error::BudgetExceeded)
        );
    }
}
#[test]
fn identities_are_never_reused_and_issuer_exhaustion_prevents_more_calls() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    for n in 1..=MAX_EXECUTIONS_PER_ISSUER {
        let response = transport
            .exchange(Selection::Both, request(&env, &[]), false)
            .unwrap();
        assert_eq!(response.identity().0.as_str(), format!("a{n}"));
        assert_eq!(response.identity().1.as_str(), format!("z{n}"));
    }
    assert!(matches!(
        transport.exchange(Selection::Both, request(&env, &[]), false),
        Err(Error::Identity(RefCode::CounterExhausted))
    ));
    assert_eq!(transport.calls(), 1024);
}
#[test]
fn payload_bearing_debug_output_is_redacted() {
    let (env, catalog, scope) = setup();
    let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
    assert_eq!(format!("{transport:?}"), "Transport([redacted])");
    let input = request(&env, &[]);
    assert_eq!(format!("{input:?}"), "Request<'_>([redacted])");
    let response = transport.exchange(Selection::Both, input, false).unwrap();
    assert_eq!(format!("{response:?}"), "Response([redacted])");
}

#[test]
fn complete_delivery_preserves_supplied_termination_without_confirming_completion() {
    for (process, cancelled) in [
        (ProcessEnd::KilledCancel, true),
        (ProcessEnd::TransportLost, false),
    ] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let source = SourceRef::new("wsl-i1").unwrap();
        let sources = [source.clone()];
        let script = complete(&source, &sources);
        let mut input = request(&env, &script);
        input.process = process;
        let response = transport.exchange(Selection::Both, input, false).unwrap();
        let execution = response.execution().unwrap();
        let member = &execution.members()[0];
        assert_eq!(member.process(), process);
        assert_eq!(response.cancelled(), cancelled);
        assert_eq!(response.delivery(), Delivery::Complete);
        assert_eq!(execution.dispositions().len(), 6);
        assert!(member.run_end_frame());
        assert_eq!(member.records().len(), 1);
        assert_eq!(member.verdict(), CoverageVerdict::CompletionUnconfirmed);
        let projection = response.project().unwrap();
        assert_eq!(projection.selection(), Selection::SelectedWslOnly);
        assert_eq!(projection.members().count(), 1);
        assert_eq!(
            projection.members().next().unwrap().verdict(),
            CoverageVerdict::CompletionUnconfirmed
        );
        assert_eq!(
            projection
                .projection()
                .selected_wsl
                .selected()
                .unwrap()
                .stored_occurrences
                .0,
            1
        );
    }
}

#[test]
fn finalizer_refusal_retains_each_accepted_cancellation_indicator() {
    let cancel_script = [step(1, Event::Cancel)];
    for (script, delivery, process) in [
        (&[][..], Delivery::Complete, ProcessEnd::KilledCancel),
        (&cancel_script[..], Delivery::Complete, ProcessEnd::Crashed),
        (
            &[][..],
            Delivery::Cancelled { after: 0 },
            ProcessEnd::ExitedNormally,
        ),
    ] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let mut input = request(&env, script);
        input.delivery = delivery;
        input.process = process;
        input.barrier = Barrier::NotDrained;
        let response = transport.exchange(Selection::Both, input, false).unwrap();
        assert!(response.cancelled());
        assert_eq!(response.delivery(), delivery);
        assert_eq!(response.identity().0.as_str(), "a1");
        assert_eq!(
            response.execution().unwrap_err(),
            Error::Finalize(FinalizeCode::InvalidBarrier)
        );
        assert!(matches!(
            response.project(),
            Err(ProjectionError::Execution(Error::Finalize(
                FinalizeCode::InvalidBarrier
            )))
        ));
    }
}

#[test]
fn reader_refusal_does_not_accept_cancellation_from_unvalidated_facts() {
    let script = vec![step(1, Event::Cancel); abf_reader_fixture::MAX_STEPS + 1];
    for (delivery, process) in [
        (Delivery::Complete, ProcessEnd::KilledCancel),
        (Delivery::Complete, ProcessEnd::ExitedNormally),
        (Delivery::Cancelled { after: 0 }, ProcessEnd::ExitedNormally),
    ] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let mut input = request(&env, &script);
        input.delivery = delivery;
        input.process = process;
        let response = transport.exchange(Selection::Both, input, false).unwrap();
        assert!(!response.cancelled());
        assert_eq!(response.delivery(), delivery);
        assert_eq!(
            response.execution().unwrap_err(),
            Error::Reader(abf_reader_fixture::Error::BudgetExceeded)
        );
        assert!(matches!(
            response.project(),
            Err(ProjectionError::Execution(Error::Reader(
                abf_reader_fixture::Error::BudgetExceeded
            )))
        ));
    }
}

#[test]
fn transport_loss_supersedes_supplied_cancel_without_script_cancellation() {
    for delivery in [
        Delivery::Failed { after: 0 },
        Delivery::Incomplete { after: 0 },
    ] {
        let (env, catalog, scope) = setup();
        let mut transport = Transport::new(std::slice::from_ref(&env), catalog, scope).unwrap();
        let mut input = request(&env, &[]);
        input.delivery = delivery;
        input.process = ProcessEnd::KilledCancel;
        let response = transport.exchange(Selection::Both, input, false).unwrap();
        assert!(!response.cancelled());
        assert_eq!(response.delivery(), delivery);
        assert_eq!(
            response.execution().unwrap().members()[0].process(),
            ProcessEnd::TransportLost
        );
    }
}
