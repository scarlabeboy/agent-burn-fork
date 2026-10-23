use abf_reader_fixture::*;
use abf_source_run::*;
fn setup() -> (CatalogSnapshot, CatalogScope, AttemptRef, SourceRef) {
    let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope.clone(), "f1v1").unwrap();
    (
        b.build(),
        scope,
        AttemptRef::new("att1").unwrap(),
        SourceRef::new("f1v1-i1").unwrap(),
    )
}
fn step(n: u64, event: Event<'_>) -> Step<'_> {
    Step {
        frame_seq: n,
        event,
    }
}
#[test]
fn enumeration_fidelity() {
    let (c, s, a, _) = setup();
    for outcome in [
        RawEnumeration::Closed(0),
        RawEnumeration::Partial(0, StopReason::Budget),
        RawEnumeration::Failed(EnumFailure::Io),
        RawEnumeration::RootAbsent,
        RawEnumeration::RootUnresolved,
        RawEnumeration::LayoutUnrecognized,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::NotAttempted,
    ] {
        let script = [step(
            1,
            Event::Enumeration {
                outcome,
                sources: &[],
            },
        )];
        let out = emit(&a, &c, &s, &script).unwrap();
        assert_eq!(out.frames().len(), 1);
        assert!(
            matches!(&out.frames()[0].payload,Payload::Enumeration{outcome:o,sources,..} if *o==outcome && sources.is_empty())
        );
    }
}
fn initial<'a>(sources: &'a [SourceRef]) -> Step<'a> {
    step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Closed(u64::try_from(sources.len()).unwrap()),
            sources,
        },
    )
}
fn op<'a>(n: u64, s: &'a SourceRef, q: u64, action: Action) -> Step<'a> {
    step(
        n,
        Event::Source {
            source: s,
            source_seq: q,
            action,
        },
    )
}
fn rec<'a>(n: u64, s: &'a SourceRef, q: u64, bytes: &'a [u8]) -> Step<'a> {
    step(
        n,
        Event::Record {
            source: s,
            source_seq: q,
            bytes,
        },
    )
}
#[test]
fn parser_conservation() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    for bytes in [
        b"{\"tokens\":{\"input\":100}}".as_slice(),
        b"{broken",
        b"{}",
    ] {
        let script = [
            initial(&sources),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 2, Action::OpenSuccess),
            rec(4, &r, 1, bytes),
            op(5, &r, 4, Action::End(ChangeObservation::Grew)),
            step(6, Event::RunEnd),
        ];
        let o = emit(&a, &c, &s, &script).unwrap();
        assert_eq!(o.frames().len(), 6);
        assert!(
            matches!(&o.frames()[3].payload,Payload::Source{event:SourceEvent::Record{bytes:b,result},..} if b==bytes && *result==abf_parser::parse(bytes,None,abf_parser::Limits::MAX))
        );
        assert!(matches!(
            &o.frames()[4].payload,
            Payload::Source {
                event: SourceEvent::End {
                    records: 1,
                    change: ChangeObservation::Grew
                },
                ..
            }
        ));
    }
}
#[test]
fn transitions_and_failures() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    for action in [Action::OpenFail(OpenFailure::Locked), Action::Vanished] {
        let script = [
            initial(&sources),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 2, action),
        ];
        assert_eq!(emit(&a, &c, &s, &script).unwrap().frames().len(), 3);
    }
    for count in [0, 1] {
        let mut script = vec![
            initial(&sources),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 2, Action::OpenSuccess),
        ];
        if count == 1 {
            script.push(rec(4, &r, 1, b"{}"));
        }
        script.push(op(5, &r, 3 + count, Action::ReadError(PartialReason::Io)));
        assert_eq!(
            emit(&a, &c, &s, &script).unwrap().frames().len(),
            usize::try_from(4 + count).unwrap()
        );
    }
    let script = [
        initial(&sources),
        op(2, &r, 1, Action::Skip(StopReason::Budget)),
    ];
    assert_eq!(emit(&a, &c, &s, &script).unwrap().frames().len(), 2);
}
#[test]
fn cancellation_points() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let prefix = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 2, Action::OpenSuccess),
        rec(4, &r, 1, b"{}"),
    ];
    for cut in [1, 2, 4] {
        let mut script = prefix[..cut].to_vec();
        script.push(step(99, Event::Cancel));
        script.extend_from_slice(&prefix[cut..]);
        script.push(rec(100, &r, 4, b"later"));
        let o = emit(&a, &c, &s, &script).unwrap();
        assert!(o.cancelled());
        assert_eq!(o.frames().len(), cut);
    }
}
#[test]
fn invalid_transitions() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let scripts = vec![
        vec![rec(1, &r, 1, b"{}")],
        vec![initial(&sources), rec(2, &r, 1, b"{}")],
        vec![initial(&sources), op(2, &r, 1, Action::OpenSuccess)],
        vec![initial(&sources), step(2, Event::RunEnd)],
        vec![
            initial(&sources),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 2, Action::End(ChangeObservation::NoChangeObserved)),
        ],
        vec![initial(&sources), op(2, &r, 0, Action::OpenAttempt)],
        vec![initial(&sources), initial(&sources)],
        vec![step(
            1,
            Event::Replay {
                original: 0,
                replacement: None,
            },
        )],
        vec![
            initial(&sources),
            step(
                2,
                Event::Replay {
                    original: 0,
                    replacement: Some(b"{}"),
                },
            ),
        ],
    ];
    for script in scripts {
        assert_eq!(emit(&a, &c, &s, &script).unwrap_err(), Error::InvalidScript);
    }
    let bad = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Failed(EnumFailure::Io),
            sources: &sources,
        },
    )];
    assert_eq!(emit(&a, &c, &s, &bad).unwrap_err(), Error::InvalidScript);
}
#[test]
fn delivery_identity() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let script = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 2, Action::OpenSuccess),
        rec(4, &r, 1, b"{}"),
        rec(5, &r, 2, b"{}"),
        step(
            90,
            Event::Replay {
                original: 3,
                replacement: None,
            },
        ),
        step(
            91,
            Event::Replay {
                original: 3,
                replacement: Some(b"{bad"),
            },
        ),
    ];
    let x = emit(&a, &c, &s, &script).unwrap();
    let y = emit(&a, &c, &s, &script).unwrap();
    assert_eq!(x.frames(), y.frames());
    assert_eq!(x.frames()[3], x.frames()[5]);
    assert_ne!(x.frames()[3], x.frames()[4]);
    assert_eq!(x.frames()[3].frame_seq, x.frames()[6].frame_seq);
    assert_ne!(x.frames()[3], x.frames()[6]);
    assert!(format!("{script:?}{x:?}").contains("redacted"));
}
#[test]
fn bounds() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    for n in [MAX_STEPS - 1, MAX_STEPS, MAX_STEPS + 1] {
        let script = vec![step(1, Event::Cancel); n];
        let x = emit(&a, &c, &s, &script);
        assert_eq!(x.is_ok(), n <= MAX_STEPS);
    }
    for n in [MAX_RECORDS - 1, MAX_RECORDS, MAX_RECORDS + 1] {
        let mut script = vec![
            initial(&sources),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 2, Action::OpenSuccess),
        ];
        for i in 0..n {
            script.push(rec(
                u64::try_from(i + 4).unwrap(),
                &r,
                u64::try_from(i + 1).unwrap(),
                b"{}",
            ));
        }
        assert_eq!(emit(&a, &c, &s, &script).is_ok(), n <= MAX_RECORDS);
    }
    for n in [MAX_BYTES - 1, MAX_BYTES, MAX_BYTES + 1] {
        let b = vec![b' '; n];
        let script = [
            step(1, Event::Cancel),
            rec(2, &r, 1, &b[..MAX_RECORD_BYTES]),
            rec(3, &r, 2, &b[MAX_RECORD_BYTES..]),
        ];
        assert_eq!(emit(&a, &c, &s, &script).is_ok(), n <= MAX_BYTES);
    }
    for n in [MAX_RECORD_BYTES - 1, MAX_RECORD_BYTES, MAX_RECORD_BYTES + 1] {
        let b = vec![b' '; n];
        let script = [step(1, Event::Cancel), rec(2, &r, 1, &b)];
        assert_eq!(emit(&a, &c, &s, &script).is_ok(), n <= MAX_RECORD_BYTES);
    }
}
#[test]
fn identity_overflow() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    for n in [u64::from(u32::MAX) + 1, u64::MAX, 0] {
        assert_eq!(
            emit(
                &a,
                &c,
                &s,
                &[step(
                    n,
                    Event::Enumeration {
                        outcome: RawEnumeration::Closed(0),
                        sources: &[]
                    }
                )]
            )
            .unwrap_err(),
            Error::InvalidScript
        );
    }
    assert_eq!(
        emit(
            &a,
            &c,
            &s,
            &[initial(&sources), op(2, &r, u64::MAX, Action::OpenAttempt)]
        )
        .unwrap_err(),
        Error::InvalidScript
    );
}
#[test]
fn total_bytes_are_bounded_independently_of_per_record_limit() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    for n in [MAX_BYTES - 1, MAX_BYTES, MAX_BYTES + 1] {
        let first = vec![b' '; MAX_RECORD_BYTES];
        let second = vec![b' '; (n - MAX_RECORD_BYTES).min(MAX_RECORD_BYTES)];
        let extra = vec![b' '; n.saturating_sub(2 * MAX_RECORD_BYTES)];
        let mut script = vec![
            initial(&list),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 1, Action::OpenSuccess),
            rec(4, &r, 1, &first),
            rec(5, &r, 2, &second),
        ];
        if !extra.is_empty() {
            script.push(rec(6, &r, 3, &extra));
        }
        let out = emit(&a, &c, &s, &script);
        assert_eq!(out.is_ok(), n <= MAX_BYTES);
    }
}
#[test]
fn record_replay_counts_toward_budgets() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    for n in [MAX_RECORDS - 1, MAX_RECORDS, MAX_RECORDS + 1] {
        let mut script = vec![
            initial(&list),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 1, Action::OpenSuccess),
            rec(4, &r, 1, b"{}"),
        ];
        for _ in 1..n {
            script.push(step(
                999,
                Event::Replay {
                    original: 3,
                    replacement: None,
                },
            ));
        }
        assert_eq!(emit(&a, &c, &s, &script).is_ok(), n <= MAX_RECORDS);
    }
    let b = vec![b' '; MAX_RECORD_BYTES];
    let script = [
        initial(&list),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        rec(4, &r, 1, &b),
        step(
            5,
            Event::Replay {
                original: 3,
                replacement: None,
            },
        ),
        step(
            6,
            Event::Replay {
                original: 3,
                replacement: Some(b"x"),
            },
        ),
    ];
    assert_eq!(
        emit(&a, &c, &s, &script).unwrap_err(),
        Error::BudgetExceeded
    );
}
#[test]
fn enumeration_bound_and_binding() {
    let (c, s, a, _) = setup();
    for n in [255, 256, 257] {
        let list: Vec<_> = (1..=n)
            .map(|i| SourceRef::new(&format!("f1v1-t{i}")).unwrap())
            .collect();
        assert_eq!(emit(&a, &c, &s, &[initial(&list)]).is_ok(), n <= 256);
    }
    let foreign = SourceRef::new("other-i1").unwrap();
    assert_eq!(
        emit(&a, &c, &s, &[initial(&[foreign])]).unwrap_err(),
        Error::InvalidScript
    );
    let r = SourceRef::new("f1v1-i1").unwrap();
    assert_eq!(
        emit(&a, &c, &s, &[initial(&[r.clone(), r])]).unwrap_err(),
        Error::InvalidScript
    );
    assert_eq!(
        emit(&a, &CatalogBuilder::new().build(), &s, &[]).unwrap_err(),
        Error::InvalidScript
    );
}
#[test]
fn active_step_boundary_and_no_fabricated_end() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    for n in [MAX_STEPS - 1, MAX_STEPS, MAX_STEPS + 1] {
        let mut script = vec![initial(&list)];
        for _ in 1..n {
            script.push(step(
                999,
                Event::Replay {
                    original: 0,
                    replacement: None,
                },
            ));
        }
        let out = emit(&a, &c, &s, &script);
        assert_eq!(out.is_ok(), n <= MAX_STEPS);
    }
    let script = [initial(&list), op(2, &r, 1, Action::OpenAttempt)];
    let out = emit(&a, &c, &s, &script).unwrap();
    assert!(!out.cancelled());
    assert_eq!(out.frames().len(), 2);
    assert!(!out.frames().iter().any(|f| matches!(
        f.payload,
        Payload::RunEnd
            | Payload::Source {
                event: SourceEvent::End { .. },
                ..
            }
    )));
}
#[test]
fn contents_never_assign_delivery_or_source_identity_and_debug_is_redacted() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    let bytes = b"{\"source\":\"private_marker\",\"attempt\":\"other\",\"tokens\":{\"input\":1}}";
    let script = [
        initial(&list),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        rec(77, &r, 1, bytes),
    ];
    let out = emit(&a, &c, &s, &script).unwrap();
    let f = &out.frames()[3];
    assert_eq!(f.attempt, a);
    assert_eq!(f.frame_seq, 77);
    assert!(matches!(&f.payload,Payload::Source{source,source_seq:1,..} if source==r.as_str()));
    let debug = format!("{script:?} {out:?} {:?}", Error::InvalidScript);
    assert!(!debug.contains("private_marker"));
    assert!(!debug.contains(r.as_str()));
}
#[test]
fn partial_listing_and_invalid_terminal_transitions() {
    let (c, s, a, r) = setup();
    let list = [r.clone()];
    let script = [step(
        1,
        Event::Enumeration {
            outcome: RawEnumeration::Partial(1, StopReason::Budget),
            sources: &list,
        },
    )];
    let out = emit(&a, &c, &s, &script).unwrap();
    assert!(
        matches!(&out.frames()[0].payload,Payload::Enumeration{outcome:RawEnumeration::Partial(1,StopReason::Budget),sources,..} if sources.len()==1)
    );
    let scripts = [
        vec![
            initial(&list),
            op(2, &r, 1, Action::Skip(StopReason::Cancelled)),
            op(3, &r, 1, Action::OpenAttempt),
        ],
        vec![
            initial(&list),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 1, Action::OpenSuccess),
            op(4, &r, 1, Action::ReadError(PartialReason::Io)),
            rec(5, &r, 1, b"{}"),
        ],
        vec![
            initial(&list),
            op(2, &r, 1, Action::OpenAttempt),
            op(3, &r, 1, Action::OpenSuccess),
            rec(4, &r, 2, b"{}"),
        ],
    ];
    for script in scripts {
        assert_eq!(emit(&a, &c, &s, &script).unwrap_err(), Error::InvalidScript);
    }
}
#[test]
fn impossible_replay_indices_and_counts_fail_without_panicking() {
    let (c, s, a, _) = setup();
    assert_eq!(
        emit(
            &a,
            &c,
            &s,
            &[step(
                1,
                Event::Replay {
                    original: usize::MAX,
                    replacement: None
                }
            )]
        )
        .unwrap_err(),
        Error::InvalidScript
    );
    assert_eq!(
        emit(
            &a,
            &c,
            &s,
            &[step(
                1,
                Event::Enumeration {
                    outcome: RawEnumeration::Closed(u64::MAX),
                    sources: &[]
                }
            )]
        )
        .unwrap_err(),
        Error::InvalidScript
    );
}

#[test]
fn cancel_after_run_end_is_invalid() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let script = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        op(4, &r, 1, Action::End(ChangeObservation::NoChangeObserved)),
        step(5, Event::RunEnd),
        step(6, Event::Cancel),
    ];
    assert_eq!(emit(&a, &c, &s, &script).unwrap_err(), Error::InvalidScript);
}

#[test]
fn exact_replay_after_run_end_remains_legal() {
    let (c, s, a, _) = setup();
    let script = [
        initial(&[]),
        step(2, Event::RunEnd),
        step(
            99,
            Event::Replay {
                original: 1,
                replacement: None,
            },
        ),
    ];
    let out = emit(&a, &c, &s, &script).unwrap();
    assert!(!out.cancelled());
    assert_eq!(out.frames().len(), 3);
    assert_eq!(out.frames()[1], out.frames()[2]);
    assert_eq!(out.frames()[2].frame_seq, 2);
    assert!(matches!(out.frames()[2].payload, Payload::RunEnd));
}

#[test]
fn cancel_first_emits_no_completion_and_preserves_suffix_preflight() {
    let (c, s, a, r) = setup();
    let script = [
        step(1, Event::Cancel),
        rec(2, &r, 0, b"{}"),
        step(3, Event::RunEnd),
    ];
    let out = emit(&a, &c, &s, &script).unwrap();
    assert!(out.cancelled());
    assert!(out.frames().is_empty());
    let invalid_replay = [
        step(1, Event::Cancel),
        step(
            2,
            Event::Replay {
                original: 0,
                replacement: None,
            },
        ),
    ];
    assert_eq!(
        emit(&a, &c, &s, &invalid_replay).unwrap_err(),
        Error::InvalidScript
    );
    let oversized = vec![0; MAX_RECORD_BYTES + 1];
    let over_budget = [step(1, Event::Cancel), rec(2, &r, 1, &oversized)];
    assert_eq!(
        emit(&a, &c, &s, &over_budget).unwrap_err(),
        Error::BudgetExceeded
    );
}

#[test]
fn replacement_record_bytes_bind_the_actual_parser_result() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let original_bytes = b"{\"tokens\":{\"input\":7}}";
    let prefix = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        op(3, &r, 1, Action::OpenSuccess),
        rec(4, &r, 1, original_bytes),
    ];
    let original = emit(&a, &c, &s, &prefix).unwrap();
    for replacement in [b"{\"tokens\":{\"input\":9}}".as_slice(), b"{malformed"] {
        let mut script = prefix.to_vec();
        script.push(step(
            999,
            Event::Replay {
                original: 3,
                replacement: Some(replacement),
            },
        ));
        let out = emit(&a, &c, &s, &script).unwrap();
        assert_eq!(out.frames().len(), 5);
        assert_eq!(out.frames()[3], original.frames()[3]);
        assert!(
            matches!(&out.frames()[3].payload, Payload::Source { event: SourceEvent::Record { bytes, result }, .. } if bytes.as_slice() == original_bytes && *result == abf_parser::parse(original_bytes, None, abf_parser::Limits::MAX))
        );
        let replay = &out.frames()[4];
        assert_eq!(replay.attempt, a);
        assert_eq!(replay.frame_seq, original.frames()[3].frame_seq);
        assert!(
            matches!(&replay.payload, Payload::Source { source, source_seq: 1, event: SourceEvent::Record { bytes, .. } } if source == r.as_str() && bytes.as_slice() == replacement)
        );
        // Boolean comparisons deliberately avoid printing bytes or ParseResult on failure.
        assert!(
            matches!(&replay.payload, Payload::Source { event: SourceEvent::Record { result, .. }, .. } if *result == abf_parser::parse(replacement, None, abf_parser::Limits::MAX))
        );
        assert!(
            matches!(&replay.payload, Payload::Source { event: SourceEvent::Record { result, .. }, .. } if *result != abf_parser::parse(original_bytes, None, abf_parser::Limits::MAX))
        );
    }
}

#[test]
fn duplicate_original_frame_identity_is_invalid_but_replay_reuses_it() {
    let (c, s, a, r) = setup();
    let sources = [r.clone()];
    let duplicates = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        op(2, &r, 1, Action::OpenSuccess),
    ];
    assert_eq!(
        emit(&a, &c, &s, &duplicates).unwrap_err(),
        Error::InvalidScript
    );
    let replay = [
        initial(&sources),
        op(2, &r, 1, Action::OpenAttempt),
        step(
            2,
            Event::Replay {
                original: 1,
                replacement: None,
            },
        ),
    ];
    let out = emit(&a, &c, &s, &replay).unwrap();
    assert_eq!(out.frames().len(), 3);
    assert_eq!(out.frames()[1], out.frames()[2]);
}

#[test]
fn cancellation_preserves_exact_prefix_with_an_otherwise_valid_second_source_tail() {
    let (c, s, a, first) = setup();
    let second = SourceRef::new("f1v1-i2").unwrap();
    let sources = [first.clone(), second.clone()];
    let prefix = [
        initial(&sources),
        op(2, &first, 1, Action::OpenAttempt),
        op(3, &first, 1, Action::OpenSuccess),
        rec(4, &first, 1, b"{\"tokens\":{\"input\":7}}"),
        op(
            5,
            &first,
            2,
            Action::End(ChangeObservation::NoChangeObserved),
        ),
    ];
    let tail = [
        op(6, &second, 1, Action::OpenAttempt),
        op(7, &second, 1, Action::OpenSuccess),
        rec(8, &second, 1, b"{\"tokens\":{\"input\":9}}"),
        op(
            9,
            &second,
            2,
            Action::End(ChangeObservation::NoChangeObserved),
        ),
        step(10, Event::RunEnd),
    ];
    let mut uninterrupted = prefix.to_vec();
    uninterrupted.extend_from_slice(&tail);
    assert_eq!(emit(&a, &c, &s, &uninterrupted).unwrap().frames().len(), 10);
    let expected = emit(&a, &c, &s, &prefix).unwrap();
    let mut script = prefix.to_vec();
    script.push(step(99, Event::Cancel));
    script.extend_from_slice(&tail);
    let out = emit(&a, &c, &s, &script).unwrap();
    // This assertion must directly reject consistent emit-after-Cancel regressions.
    assert_eq!(out.frames().len(), 5);
    assert!(out.cancelled());
    assert_eq!(out.frames(), expected.frames());
    assert!(
        !out.frames()
            .iter()
            .any(|f| matches!(&f.payload, Payload::RunEnd)
                || matches!(&f.payload, Payload::Source { source, .. } if source == second.as_str())
                || f.frame_seq > 5)
    );
    assert!(matches!(
        &out.frames()[4].payload,
        Payload::Source {
            event: SourceEvent::End { records: 1, .. },
            ..
        }
    ));
    // Fictional supervisor facts only: these do not attest actual process/drain behavior.
    let execution = finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: a,
                finalization: FinalizationId::new("z1").unwrap(),
                members: vec![s],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: out.into_frames(),
            process: ProcessEnd::KilledCancel,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(1, 2),
        },
        &c,
    )
    .unwrap();
    let member = &execution.members()[0];
    assert_eq!(member.records().len(), 1);
    assert_eq!(
        member.sources()[0].1.outcome(),
        SourceOutcome::TraversedToObservedEnd {
            records: 1,
            change: ChangeObservation::NoChangeObserved
        }
    );
    assert_eq!(member.sources()[1].1.outcome(), SourceOutcome::Unaccounted);
    assert_eq!(member.verdict(), CoverageVerdict::PartiallyCovered);
    let membership = ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: None,
        },
        &[member],
        &c,
    )
    .unwrap();
    let paired = project_with_companion(&membership).unwrap();
    let windows = paired.projection().windows.selected().unwrap();
    assert_eq!(windows.stored_occurrences.0, 1);
    assert_eq!(windows.runs.run_keys, 2);
    assert_eq!(
        windows.components.input.value,
        abf_counting::ComponentValue::ReportedClaimSum(7)
    );
}

#[test]
fn maximal_enumeration_replays_are_bounded_by_steps_and_source_ref_rules() {
    let (_, s, a, _) = setup();
    let tag = "f123456789012345";
    let mut builder = CatalogBuilder::new();
    builder.add(s.clone(), tag).unwrap();
    let c = builder.build();
    let instance_prefix = "x".repeat(43);
    let sources: Vec<_> = (0..MAX_INSTANCES_PER_SCOPE)
        .map(|i| SourceRef::new(&format!("{tag}-{instance_prefix}{i:04}")).unwrap())
        .collect();
    assert_eq!(sources.len(), 256);
    assert!(sources.iter().all(|r| r.as_str().len() == 64));
    let mut script = vec![initial(&sources)];
    script.resize(
        MAX_STEPS,
        step(
            999,
            Event::Replay {
                original: 0,
                replacement: None,
            },
        ),
    );
    let out = emit(&a, &c, &s, &script).unwrap();
    assert!(!out.cancelled());
    assert_eq!(out.frames().len(), MAX_STEPS);
    let mut copied_names = 0usize;
    let mut character_bytes = 0usize;
    for frame in out.frames() {
        assert_eq!(frame, &out.frames()[0]);
        assert_eq!(frame.attempt, a);
        assert_eq!(frame.frame_seq, 1);
        let Payload::Enumeration { sources: names, .. } = &frame.payload else {
            panic!("expected enumeration");
        };
        assert!(
            names
                .iter()
                .map(String::as_str)
                .eq(sources.iter().map(SourceRef::as_str))
        );
        copied_names = copied_names.checked_add(names.len()).unwrap();
        character_bytes = character_bytes
            .checked_add(names.iter().map(String::len).sum::<usize>())
            .unwrap();
    }
    assert_eq!(copied_names, 131072);
    assert_eq!(character_bytes, 8388608);
    // There are no records: MAX_BYTES is not a source-name byte budget.
    assert!(character_bytes > MAX_BYTES);
    script.push(step(
        999,
        Event::Replay {
            original: 0,
            replacement: None,
        },
    ));
    assert_eq!(
        emit(&a, &c, &s, &script).unwrap_err(),
        Error::BudgetExceeded
    );
}
