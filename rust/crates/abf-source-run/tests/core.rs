use abf_source_run::*;

fn scope(family: &str) -> CatalogScope {
    CatalogScope::new(EnvironmentKind::Windows, "w1", family, "v1").unwrap()
}
fn catalog() -> CatalogSnapshot {
    let mut b = CatalogBuilder::new();
    b.add(scope("f1"), "f1v1").unwrap();
    b.add(scope("f2"), "f2v1").unwrap();
    b.build()
}
fn frame(seq: u64, payload: Payload) -> ReaderFrame {
    ReaderFrame {
        attempt: AttemptRef::new("att1").unwrap(),
        frame_seq: seq,
        payload,
    }
}
fn listing(member: usize, second: bool, outcome: RawEnumeration, sources: &[&str]) -> Payload {
    Payload::Enumeration {
        member,
        second,
        outcome,
        sources: sources.iter().map(|s| s.to_string()).collect(),
        observed_at: None,
    }
}
fn initial(sources: &[&str]) -> Payload {
    listing(
        0,
        false,
        RawEnumeration::Closed(sources.len() as u64),
        sources,
    )
}
fn record(source: &str, seq: u64, variant: u8) -> Payload {
    let input = match variant {
        1 => 100,
        2 => 90,
        3 => 7,
        n => u32::from(n),
    };
    let bytes = format!("{{\"tokens\":{{\"input\":{input}}}}}").into_bytes();
    let result = abf_parser::parse(&bytes, None, abf_parser::Limits::MAX);
    Payload::Source {
        source: source.into(),
        source_seq: seq,
        event: SourceEvent::Record { bytes, result },
    }
}
fn event(source: &str, seq: u64, event: SourceEvent) -> Payload {
    Payload::Source {
        source: source.into(),
        source_seq: seq,
        event,
    }
}
fn end(source: &str, k: u64) -> Payload {
    event(
        source,
        k + 1,
        SourceEvent::End {
            records: k,
            change: ChangeObservation::NoChangeObserved,
        },
    )
}
fn raw(frames: Vec<ReaderFrame>) -> RawExecutionLog {
    RawExecutionLog {
        manifest: ExecutionManifest {
            attempt: AttemptRef::new("att1").unwrap(),
            finalization: FinalizationId::new("z1").unwrap(),
            members: vec![scope("f1"), scope("f2")],
            retry_of: None,
            subset: RetrySubset::AllExpected,
        },
        pre_barrier: frames,
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        distribution_matches: true,
        times: TimeObservations::new(9, 1),
    }
}
fn run(frames: Vec<ReaderFrame>) -> FinalizedExecution {
    finalize(raw(frames), &catalog()).unwrap()
}
fn baseline() -> Vec<ReaderFrame> {
    vec![
        frame(1, initial(&["f1v1-i1", "f1v1-i2"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, end("f1v1-i2", 0)),
        frame(5, Payload::RunEnd),
    ]
}

#[test]
fn r01_r19_r44_r73_complete_and_control_frames_retain_only_records() {
    let x = run(baseline());
    let m = &x.members()[0];
    assert_eq!(
        m.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(m.records().len(), 1);
    assert_eq!(m.sources().len(), 2);
    assert_eq!(m.sources()[0].1.status(), SourceRunStatus::Complete);
    assert_eq!(m.sources()[0].1.matched(), &[10]);
    assert_eq!(m.second_enumeration(), &SecondEnumeration::NotPerformed);
    assert!(m.qualifiers().is_empty());
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    assert_eq!(x.dispositions().len(), 5);
}
#[test]
fn r14a_r53b_r61b_replays_quarantine_lowest_index_only_and_controls_never_records() {
    let mut f = baseline();
    f.push(f[1].clone());
    f.push(f[2].clone());
    let x = run(f);
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(
        x.dispositions()[5],
        Disposition::Quarantined(QuarantineCode::ExactReplay)
    );
    assert_eq!(
        x.dispositions()[6],
        Disposition::Quarantined(QuarantineCode::ExactReplay)
    );
    assert_eq!(x.members()[0].delivery().replays_quarantined, 2);
    assert_eq!(x.members()[0].sources()[0].1.claims().len(), 1);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(
        x.members()[0].qualifiers(),
        vec![Qualifier::TransportReplayQuarantined]
    );
    assert_eq!(x.members()[1].delivery().replays_quarantined, 0);
}
#[test]
fn r14b_r49_r50_r52_conflicts_mark_sources_without_retaining_variants() {
    let mut f = baseline();
    f.push(frame(2, record("f1v1-i1", 1, 2)));
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(m.records().len(), 0);
    assert_eq!(m.delivery().seq_conflicts, 1);
    assert_eq!(m.delivery().global_seq_conflicts, 0);
    assert_eq!(
        m.sources()[0].1.outcome(),
        SourceOutcome::PartialRead {
            records: 0,
            reason: PartialReason::DeliveryConflict
        }
    );
    assert_eq!(m.sources()[0].1.matched(), &[4, 8]);
    assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(
        x.dispositions()[1],
        Disposition::Quarantined(QuarantineCode::SequenceConflict)
    );
    assert_eq!(
        x.dispositions()[5],
        Disposition::Quarantined(QuarantineCode::SequenceConflict)
    );
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
}
#[test]
fn r98a_r98b_ambiguous_initials_never_select_a_population_for_every_arrival_order() {
    for first in [initial(&[]), initial(&["f1v1-i1"])] {
        let f = [
            frame(1, first),
            frame(2, initial(&["f1v1-i1"])),
            frame(3, record("f1v1-i1", 1, 1)),
            frame(4, end("f1v1-i1", 1)),
            frame(5, Payload::RunEnd),
        ];
        for order in permutations(f.len()) {
            let x = run(order.iter().map(|&i| f[i].clone()).collect());
            let m = &x.members()[0];
            assert_eq!(
                m.enumeration(),
                EnumerationOutcome::Failed(EnumFailure::AmbiguousInitial)
            );
            assert_eq!(m.delivery().initial_enum_identities, 2);
            assert_eq!(m.delivery().unexpected_source, 2);
            assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
            assert_eq!(m.cause(), VerdictRule::MemberEnumerationAnomaly);
            assert!(m.sources().is_empty());
            assert!(m.records().is_empty());
            assert!(m.qualifiers().is_empty());
            assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
        }
    }
}
fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn visit(prefix: Vec<usize>, n: usize, out: &mut Vec<Vec<usize>>) {
        if prefix.len() == n {
            out.push(prefix);
            return;
        }
        for i in 0..n {
            if !prefix.contains(&i) {
                let mut next = prefix.clone();
                next.push(i);
                visit(next, n, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(Vec::new(), n, &mut out);
    out
}
#[test]
fn r98c_r62_global_exact_replay_is_one_initial_and_never_global_inconsistency() {
    let f = [
        frame(1, initial(&[])),
        frame(1, initial(&[])),
        frame(2, Payload::RunEnd),
    ];
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        assert_eq!(x.members()[0].delivery().initial_enum_identities, 1);
        assert_eq!(x.members()[0].delivery().global_replays, 1);
        assert_eq!(
            x.members()[0].verdict(),
            CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration)
        );
        assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
        assert_eq!(
            x.members()[1].qualifiers(),
            vec![Qualifier::TransportReplayQuarantined]
        );
    }
}
#[test]
fn r98d_r60_conflicting_initial_is_global_and_keeps_its_precedence() {
    let f = [
        frame(1, initial(&[])),
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, Payload::RunEnd),
    ];
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        assert_eq!(
            x.members()[0].enumeration(),
            EnumerationOutcome::Failed(EnumFailure::DeliveryConflict)
        );
        assert_eq!(x.members()[0].delivery().initial_enum_identities, 0);
        assert_eq!(x.members()[0].records().len(), 0);
        for m in x.members() {
            assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            assert_eq!(m.delivery().global_seq_conflicts, 1);
        }
    }
}
#[test]
fn r99a_r99b_r99d_rule_1b_precedes_noattempt_and_unavailability_but_one_second_is_ordinary() {
    for state in [
        None,
        Some(RawEnumeration::ScopeUnavailable(
            UnavailableReason::Unavailable,
        )),
    ] {
        for count in [1, 2] {
            let mut f = Vec::new();
            if let Some(e) = state {
                f.push(frame(1, listing(0, false, e, &[])));
            }
            for _ in 0..count {
                f.push(frame(
                    f.len() as u64 + 1,
                    listing(0, true, RawEnumeration::Closed(0), &[]),
                ));
            }
            f.push(frame(f.len() as u64 + 1, Payload::RunEnd));
            for order in permutations(f.len()) {
                let x = run(order.iter().map(|&i| f[i].clone()).collect());
                let m = &x.members()[0];
                let expected = match (count, state) {
                    (2, _) => CoverageVerdict::Inconsistent,
                    (_, None) => CoverageVerdict::NoAttempt,
                    _ => CoverageVerdict::ScopeUnavailable(UnavailableReason::Unavailable),
                };
                assert_eq!(m.verdict(), expected);
                assert_eq!(
                    m.second_enumeration(),
                    &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
                );
                assert_eq!(m.delivery().second_enum_identities, count);
                assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
            }
        }
    }
}
#[test]
fn r99g_erratum_f3_variants_are_quarantined_and_rule_1a_precedes_1b() {
    let f = [
        frame(1, listing(0, true, RawEnumeration::Closed(0), &[])),
        frame(2, listing(0, true, RawEnumeration::Closed(0), &[])),
        frame(3, listing(1, false, RawEnumeration::Closed(0), &[])),
        frame(3, listing(1, false, RawEnumeration::RootAbsent, &[])),
        frame(4, Payload::RunEnd),
    ];
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        for (i, &j) in order
            .iter()
            .enumerate()
            .filter(|(_, j)| **j == 2 || **j == 3)
        {
            assert_eq!(f[j].frame_seq, 3);
            assert_eq!(
                x.dispositions()[i],
                Disposition::Quarantined(QuarantineCode::SequenceConflict)
            );
        }
        assert_eq!(x.members()[0].delivery().second_enum_identities, 2);
        for m in x.members() {
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
        }
    }
}

#[test]
fn s1_through_s15_literal_outcomes_statuses_and_record_conservation() {
    let a = "f1v1-i1";
    let cases = vec![
        (
            vec![
                record(a, 1, 1),
                event(a, 2, SourceEvent::Skipped(StopReason::Budget)),
                event(a, 3, SourceEvent::OpenFailed(OpenFailure::PermissionDenied)),
            ],
            true,
            ProcessEnd::ExitedNormally,
            1,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::ConflictingTerminals,
                records: 1,
            },
            SourceRunStatus::Unavailable,
            1,
        ),
        (
            vec![record(a, 1, 1), event(a, 2, SourceEvent::Vanished)],
            true,
            ProcessEnd::ExitedNormally,
            2,
            SourceOutcome::TerminalContradiction {
                claim: NoTraversalClaim::Vanished,
                records: 1,
            },
            SourceRunStatus::Unavailable,
            1,
        ),
        (
            vec![event(a, 1, SourceEvent::Skipped(StopReason::Budget))],
            true,
            ProcessEnd::ExitedNormally,
            3,
            SourceOutcome::NotAttempted(StopReason::Budget),
            SourceRunStatus::NotCollected,
            0,
        ),
        (
            vec![end(a, 0), end(a, 0)],
            true,
            ProcessEnd::ExitedNormally,
            5,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::ConflictingTerminals,
                records: 0,
            },
            SourceRunStatus::Partial,
            0,
        ),
        (
            vec![record(a, 1, 1), record(a, 1, 1), end(a, 1)],
            true,
            ProcessEnd::ExitedNormally,
            6,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::SourceSeqReused,
                records: 2,
            },
            SourceRunStatus::Partial,
            2,
        ),
        (
            vec![
                record(a, 1, 1),
                event(
                    a,
                    2,
                    SourceEvent::End {
                        records: 1,
                        change: ChangeObservation::NoChangeObserved,
                    },
                ),
                record(a, 3, 2),
            ],
            true,
            ProcessEnd::ExitedNormally,
            7,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::RecordsAfterEnd,
                records: 2,
            },
            SourceRunStatus::Partial,
            2,
        ),
        (
            vec![record(a, 1, 1), end(a, 3)],
            true,
            ProcessEnd::ExitedNormally,
            8,
            SourceOutcome::PartialRead {
                reason: PartialReason::DeliveryLoss,
                records: 1,
            },
            SourceRunStatus::Partial,
            1,
        ),
        (
            vec![
                record(a, 1, 1),
                record(a, 2, 1),
                event(
                    a,
                    3,
                    SourceEvent::End {
                        records: 1,
                        change: ChangeObservation::NoChangeObserved,
                    },
                ),
            ],
            true,
            ProcessEnd::ExitedNormally,
            9,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::CountExceedsEnd,
                records: 2,
            },
            SourceRunStatus::Partial,
            2,
        ),
        (
            vec![record(a, 1, 1), end(a, 1)],
            true,
            ProcessEnd::ExitedNormally,
            10,
            SourceOutcome::TraversedToObservedEnd {
                records: 1,
                change: ChangeObservation::NoChangeObserved,
            },
            SourceRunStatus::Complete,
            1,
        ),
        (
            vec![
                record(a, 1, 1),
                event(a, 2, SourceEvent::ReadError(PartialReason::Io)),
            ],
            true,
            ProcessEnd::ExitedNormally,
            11,
            SourceOutcome::PartialRead {
                reason: PartialReason::Io,
                records: 1,
            },
            SourceRunStatus::Partial,
            1,
        ),
        (
            vec![event(a, 1, SourceEvent::OpenAttempt)],
            true,
            ProcessEnd::ExitedNormally,
            12,
            SourceOutcome::AnomalousSequence {
                kind: SequenceAnomaly::MissingEndMarker,
                records: 0,
            },
            SourceRunStatus::Partial,
            0,
        ),
        (
            vec![record(a, 1, 1)],
            false,
            ProcessEnd::Crashed,
            13,
            SourceOutcome::PartialRead {
                reason: PartialReason::ReaderTerminated,
                records: 1,
            },
            SourceRunStatus::Failed,
            1,
        ),
        (
            vec![event(a, 1, SourceEvent::OpenAttempt)],
            false,
            ProcessEnd::Crashed,
            14,
            SourceOutcome::OpenInterrupted {
                reason: InterruptReason::ReaderTerminated,
            },
            SourceRunStatus::Failed,
            0,
        ),
        (
            vec![],
            false,
            ProcessEnd::KilledTimeout,
            15,
            SourceOutcome::Unaccounted,
            SourceRunStatus::NotCollected,
            0,
        ),
    ];
    for (payloads, run_end, process, row, outcome, status, retained) in cases {
        let mut f = vec![frame(1, initial(&[a]))];
        for p in payloads {
            f.push(frame(f.len() as u64 + 1, p));
        }
        if run_end {
            f.push(frame(f.len() as u64 + 1, Payload::RunEnd));
        }
        let mut r = raw(f);
        r.process = process;
        let x = finalize(r, &catalog()).unwrap();
        let m = &x.members()[0];
        let s = &m.sources()[0].1;
        assert_eq!(s.outcome(), outcome, "row {row}");
        assert_eq!(s.status(), status, "row {row}");
        assert_eq!(s.matched()[0], row);
        assert_eq!(m.records().len(), retained);
    }
}
#[test]
fn r03_to_r07_r43_r95b_inventory_facts_do_not_create_sources_or_records() {
    for (e, v) in [
        (
            RawEnumeration::Closed(0),
            CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration),
        ),
        (
            RawEnumeration::RootAbsent,
            CoverageVerdict::NoSourcesFound(NoSourcesReason::RootAbsent),
        ),
        (
            RawEnumeration::RootUnresolved,
            CoverageVerdict::InventoryUnknown,
        ),
        (
            RawEnumeration::LayoutUnrecognized,
            CoverageVerdict::InventoryUnknown,
        ),
        (
            RawEnumeration::Failed(EnumFailure::PermissionDenied),
            CoverageVerdict::InventoryUnknown,
        ),
        (
            RawEnumeration::Partial(0, StopReason::Budget),
            CoverageVerdict::InventoryIncomplete,
        ),
    ] {
        let mut r = raw(vec![frame(1, listing(0, false, e, &[]))]);
        r.process = ProcessEnd::Crashed;
        let x = finalize(r, &catalog()).unwrap();
        let m = &x.members()[0];
        assert_eq!(m.verdict(), v);
        assert!(m.sources().is_empty());
        assert!(m.records().is_empty());
        assert!(!m.run_end_frame());
    }
}
#[test]
fn r74_to_r82_r97_relisting_is_relative_and_does_not_downgrade() {
    let cases = vec![
        (
            RawEnumeration::Closed(3),
            vec!["f1v1-i1", "f1v1-i2", "f1v1-i3"],
            Qualifier::NewSourcesObserved {
                count: 1,
                basis: CountBasis::Exact,
            },
        ),
        (
            RawEnumeration::Closed(2),
            vec!["f1v1-i1", "f1v1-i2"],
            Qualifier::SecondEnumerationNoNewSources,
        ),
        (
            RawEnumeration::Closed(1),
            vec!["f1v1-i1"],
            Qualifier::SecondEnumerationNoNewSources,
        ),
        (
            RawEnumeration::Partial(2, StopReason::Budget),
            vec!["f1v1-i1", "f1v1-i3"],
            Qualifier::NewSourcesObserved {
                count: 1,
                basis: CountBasis::LowerBound,
            },
        ),
        (
            RawEnumeration::Partial(1, StopReason::Budget),
            vec!["f1v1-i1"],
            Qualifier::SecondEnumerationIncomplete,
        ),
        (
            RawEnumeration::Failed(EnumFailure::PermissionDenied),
            vec![],
            Qualifier::SecondEnumerationUnusable,
        ),
    ];
    for (outcome, refs, q) in cases {
        let mut f = baseline();
        f.push(frame(6, listing(0, true, outcome, &refs)));
        let x = run(f);
        let m = &x.members()[0];
        assert_eq!(
            m.verdict(),
            CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
        );
        assert_eq!(m.sources().len(), 2);
        assert_eq!(m.records().len(), 1);
        assert_eq!(m.qualifiers(), vec![q]);
    }
}
#[test]
fn r100a_to_r100e_ordered_second_unusable_codes_and_d2_facts() {
    for initial_outcome in [
        None,
        Some(RawEnumeration::Failed(EnumFailure::PermissionDenied)),
        Some(RawEnumeration::Closed(0)),
    ] {
        let mut f = Vec::new();
        if let Some(e) = initial_outcome {
            f.push(frame(1, listing(0, false, e, &[])));
        }
        let seq = f.len() as u64 + 1;
        f.push(frame(seq, listing(0, true, RawEnumeration::Closed(0), &[])));
        f.push(frame(
            seq,
            listing(0, true, RawEnumeration::RootAbsent, &[]),
        ));
        f.push(frame(seq + 1, Payload::RunEnd));
        for order in permutations(f.len()) {
            let x = run(order.iter().map(|&i| f[i].clone()).collect());
            let m = &x.members()[0];
            let code = match initial_outcome {
                Some(RawEnumeration::Closed(_)) => SecondUnusable::DeliveryConflict,
                _ => SecondUnusable::NoInitialSet,
            };
            assert_eq!(m.second_enumeration(), &SecondEnumeration::Unusable(code));
            assert!(m.delivery().second_enum_conflict);
            assert_eq!(m.delivery().second_enum_identities, 0);
            assert_eq!(m.delivery().global_seq_conflicts, 1);
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
        }
    }
}
#[test]
fn r65_unattributable_gap_marks_untouched_member_and_r64c_mismatch_is_never_grouped() {
    let x = run(vec![frame(1, initial(&[])), frame(3, Payload::RunEnd)]);
    assert_eq!(x.members()[1].delivery().unattributable_gaps, 1);
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::Inconsistent);
    let mut r = raw(vec![
        frame(1, initial(&[])),
        frame(1, initial(&["f1v1-i1"])),
    ]);
    r.distribution_matches = false;
    let x = finalize(r, &catalog()).unwrap();
    for m in x.members() {
        assert_eq!(
            m.verdict(),
            CoverageVerdict::ScopeUnavailable(UnavailableReason::DistributionMismatch)
        );
        assert_eq!(m.delivery().global_seq_conflicts, 0);
    }
    assert!(
        x.dispositions()
            .iter()
            .all(|d| *d == Disposition::Quarantined(QuarantineCode::DistributionMismatch))
    );
}

#[test]
fn r13_r13b_r66_r86_r98h_r99e_late_frames_only_weaken_assessment() {
    let x = run(baseline());
    let mut log = PostFinalizationLog::bind(&x);
    let before = log.snapshot();
    assert_eq!(
        assess(&x.members()[0], &before).unwrap().ceiling,
        PresentationCeiling::Original
    );
    let frames = vec![
        frame(6, record("f1v1-i1", 2, 2)),
        baseline()[1].clone(),
        frame(2, record("f1v1-i1", 1, 3)),
        frame(7, listing(0, true, RawEnumeration::Closed(1), &["f1v1-i3"])),
        frame(8, initial(&[])),
    ];
    for f in frames {
        log.append(f, SupervisorInstant::synthetic(0)).unwrap();
    }
    let d = log.snapshot();
    assert_eq!(d.entries().len(), 5);
    assert_eq!(d.entries()[0].relation, IdentityRelation::NewIdentity);
    assert_eq!(d.entries()[0].attribution, Attribution::Member(0));
    assert_eq!(d.entries()[0].outcome, Some(OutcomeClass::Accepted));
    assert_eq!(
        d.entries()[1].relation,
        IdentityRelation::ReplayOfAdmittedIdentity
    );
    assert_eq!(
        d.entries()[2].relation,
        IdentityRelation::ConflictsWithAdmittedIdentity
    );
    assert_eq!(d.entries()[3].kind, FrameKind::SecondEnumeration);
    assert_eq!(d.entries()[4].kind, FrameKind::InitialEnumeration);
    let a = assess(&x.members()[0], &d).unwrap();
    assert_eq!(
        a.verdict,
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(a.ceiling, PresentationCeiling::WeakenedByLateDelivery);
    assert_eq!(
        a.post,
        PostFinalization::Observed {
            records: 3,
            other: 2,
            conflicts: 1,
            unattributed: 2
        }
    );
    assert_eq!(before.entries().len(), 0);
    assert_eq!(before.counts().records, 0);
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(x.members()[0].delivery().initial_enum_identities, 1);
    assert_eq!(
        x.members()[0].second_enumeration(),
        &SecondEnumeration::NotPerformed
    );
    assert!(x.members()[0].qualifiers().is_empty());
}
#[test]
fn n16_metadata_is_retained_but_all_opaque_debug_is_redacted() {
    let result = abf_parser::parse(
        br#"{"id":"private_marker","tokens":{"input":100}}"#,
        None,
        abf_parser::Limits::MAX,
    );
    let mut f = baseline();
    f[1].payload = Payload::Source {
        source: "f1v1-i1".into(),
        source_seq: 1,
        event: SourceEvent::Record {
            bytes: b"private_marker".to_vec(),
            result: result.clone(),
        },
    };
    let x = run(f);
    assert_eq!(x.members()[0].records()[0].parse_result(), &result);
    for text in [
        format!("{x:?}"),
        format!("{:?}", x.members()[0]),
        format!("{:?}", x.members()[0].records()[0]),
        format!("{:?}", x.delivery_evidence()),
        format!("{:?}", x.manifest()),
        format!("{:?}", x.members()[0].scope()),
        format!("{:?}", PostFinalizationLog::bind(&x)),
        format!("{:?}", x.times()),
    ] {
        assert!(!text.contains("private_marker"));
        assert!(!text.contains("100"));
        assert!(!text.contains("f1v1"));
    }
}
#[test]
fn diagnostics_binding_and_capacity_are_fixed_codes_not_payloads() {
    let x = run(baseline());
    let mut log = PostFinalizationLog::bind(&x);
    for n in 0..=MAX_DIAGNOSTIC_ENTRIES {
        log.append(
            frame(100 + n as u64, record("f1v1-i1", 1, 1)),
            SupervisorInstant::synthetic(1),
        )
        .unwrap();
    }
    let d = log.snapshot();
    assert_eq!(d.entries().len(), 64);
    assert!(d.capacity_reached());
    assert!(!d.count_overflow());
    assert_eq!(d.counts().records, 65);
    let mut r = raw(baseline());
    r.manifest.finalization = FinalizationId::new("z2").unwrap();
    let other = finalize(r, &catalog()).unwrap();
    assert_eq!(
        assess(&other.members()[0], &d),
        Err(AssessCode::IdentityMismatch)
    );
    let mut wrong = frame(1, Payload::RunEnd);
    wrong.attempt = AttemptRef::new("other").unwrap();
    assert_eq!(
        log.append(wrong, SupervisorInstant::synthetic(0)),
        Err(DiagnosticsCode::ForeignFrame)
    );
    assert_eq!(log.snapshot().counts().records, 65);
}

#[test]
fn r08_r09_r10_r11_r12_r16_r17_r42b_r56_r57_read_progress_and_interruptions() {
    for (process, reason, status) in [
        (
            ProcessEnd::Crashed,
            InterruptReason::ReaderTerminated,
            SourceRunStatus::Failed,
        ),
        (
            ProcessEnd::KilledTimeout,
            InterruptReason::Timeout,
            SourceRunStatus::Failed,
        ),
        (
            ProcessEnd::TransportLost,
            InterruptReason::TransportLost,
            SourceRunStatus::Failed,
        ),
        (
            ProcessEnd::KilledCancel,
            InterruptReason::Cancelled,
            SourceRunStatus::Partial,
        ),
    ] {
        let mut r = raw(vec![
            frame(1, initial(&["f1v1-i1"])),
            frame(2, event("f1v1-i1", 1, SourceEvent::OpenAttempt)),
        ]);
        r.process = process;
        let x = finalize(r, &catalog()).unwrap();
        let m = &x.members()[0];
        assert_eq!(
            m.sources()[0].1.outcome(),
            SourceOutcome::OpenInterrupted { reason }
        );
        assert_eq!(m.sources()[0].1.status(), status);
        assert_eq!(m.verdict(), CoverageVerdict::NotCovered);
    }
    let mut r = raw(baseline());
    r.pre_barrier.pop();
    r.process = ProcessEnd::Crashed;
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CompletionUnconfirmed
    );
    assert_eq!(x.members()[0].records().len(), 1);
    for change in [
        ChangeObservation::Grew,
        ChangeObservation::ShrankOrReplaced,
        ChangeObservation::NotObservable,
    ] {
        let mut f = baseline();
        f[2].payload = event("f1v1-i1", 2, SourceEvent::End { records: 1, change });
        let x = run(f);
        let m = &x.members()[0];
        assert_eq!(
            m.verdict(),
            match change {
                ChangeObservation::NotObservable =>
                    CoverageVerdict::CoveredWithinEnumeration { expected: 2 },
                _ => CoverageVerdict::PartiallyCovered,
            }
        );
        assert_eq!(m.records().len(), 1);
    }
    for terminal in [
        SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
        SourceEvent::Vanished,
        SourceEvent::Skipped(StopReason::Cancelled),
    ] {
        let mut f = baseline();
        f[3].payload = event("f1v1-i2", 1, terminal);
        let x = run(f);
        assert_eq!(x.members()[0].verdict(), CoverageVerdict::PartiallyCovered);
        assert_eq!(x.members()[0].records().len(), 1);
    }
}
#[test]
fn r32_r83_unexpected_sources_are_never_promoted_by_second_listing() {
    let mut f = baseline();
    f.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    f.push(frame(7, record("f1v1-i3", 1, 1)));
    f.push(frame(7, record("f1v1-i3", 1, 1)));
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(m.records().len(), 1);
    assert_eq!(m.sources().len(), 2);
    assert_eq!(m.delivery().unexpected_source, 2);
    assert_eq!(m.delivery().replays_quarantined, 0);
    assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(
        x.dispositions()[6],
        Disposition::Quarantined(QuarantineCode::UnexpectedSource)
    );
    assert_eq!(
        x.dispositions()[7],
        Disposition::Quarantined(QuarantineCode::UnexpectedSource)
    );
}
#[test]
fn r51_r64a_r64b_global_conflicts_mark_all_named_sources_without_repair() {
    for state in [
        RawEnumeration::NotAttempted,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::RootUnresolved,
    ] {
        let f = [
            frame(1, initial(&["f1v1-i1"])),
            frame(2, listing(1, false, state, &[])),
            frame(3, record("f1v1-i1", 1, 1)),
            frame(3, record("f2v1-unexpected", 1, 1)),
            frame(4, end("f1v1-i1", 1)),
            frame(5, Payload::RunEnd),
        ];
        for order in permutations(f.len()) {
            let x = run(order.iter().map(|&i| f[i].clone()).collect());
            for m in x.members() {
                assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
                assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
                assert_eq!(m.delivery().global_seq_conflicts, 1);
                assert_eq!(m.delivery().unexpected_source, 0);
                assert!(m.records().is_empty());
            }
            assert!(x.members()[0].sources()[0].1.conflict_marked());
        }
    }
}
#[test]
fn r98f_r99f_r100c_initial_and_second_control_identity_interactions() {
    let f = [
        frame(1, initial(&[])),
        frame(2, initial(&["f1v1-i1"])),
        frame(3, listing(0, true, RawEnumeration::Closed(0), &[])),
        frame(4, listing(0, true, RawEnumeration::Closed(1), &["f1v1-i1"])),
        frame(5, Payload::RunEnd),
    ];
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        let m = &x.members()[0];
        assert_eq!(
            m.enumeration(),
            EnumerationOutcome::Failed(EnumFailure::AmbiguousInitial)
        );
        assert_eq!(
            m.second_enumeration(),
            &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
        );
        assert_eq!(m.delivery().initial_enum_identities, 2);
        assert_eq!(m.delivery().second_enum_identities, 2);
        assert_eq!(m.cause(), VerdictRule::MemberEnumerationAnomaly);
    }
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(1, listing(0, true, RawEnumeration::Closed(1), &["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, Payload::RunEnd),
    ];
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(
        m.enumeration(),
        EnumerationOutcome::Failed(EnumFailure::DeliveryConflict)
    );
    assert!(m.delivery().second_enum_conflict);
    assert_eq!(m.delivery().initial_enum_identities, 0);
    assert_eq!(m.delivery().second_enum_identities, 0);
    assert_eq!(
        m.second_enumeration(),
        &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
    );
    assert_eq!(m.delivery().unexpected_source, 2);
    assert!(m.records().is_empty());
}
#[test]
fn r84_r85_r87_r100e_second_conflict_precedes_ambiguity_and_never_reads_variants() {
    let mut f = baseline();
    let se = frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    );
    f.push(se.clone());
    f.push(se);
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(m.delivery().second_enum_identities, 1);
    assert_eq!(m.delivery().global_replays, 1);
    assert_eq!(
        m.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(
        m.qualifiers(),
        vec![
            Qualifier::TransportReplayQuarantined,
            Qualifier::NewSourcesObserved {
                count: 1,
                basis: CountBasis::Exact
            }
        ]
    );
    let mut f = baseline();
    f.push(frame(6, listing(0, true, RawEnumeration::Closed(0), &[])));
    f.push(frame(7, listing(0, true, RawEnumeration::Closed(0), &[])));
    let x = run(f.clone());
    assert_eq!(
        x.members()[0].second_enumeration(),
        &SecondEnumeration::Unusable(SecondUnusable::Ambiguous)
    );
    assert_eq!(
        x.members()[0].cause(),
        VerdictRule::MemberEnumerationAnomaly
    );
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    f.push(frame(8, listing(0, true, RawEnumeration::Closed(0), &[])));
    f.push(frame(8, listing(0, true, RawEnumeration::RootAbsent, &[])));
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(
        m.second_enumeration(),
        &SecondEnumeration::Unusable(SecondUnusable::DeliveryConflict)
    );
    assert_eq!(m.delivery().second_enum_identities, 2);
    assert!(m.delivery().second_enum_conflict);
    assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
    assert_eq!(m.records().len(), 1);
}
#[test]
fn r68_r71_r88_r97d_r97e_listing_integrity_checks_precede_ambiguity() {
    for second in [false, true] {
        for (outcome, sources, code) in [
            (
                RawEnumeration::Closed(2),
                vec!["f1v1-i1", "f1v1-i1"],
                FinalizeCode::DuplicateSource,
            ),
            (
                RawEnumeration::Closed(1),
                vec!["f2v1-i1"],
                FinalizeCode::SourceScopeMismatch,
            ),
            (
                RawEnumeration::Closed(3),
                vec!["f1v1-i1", "f1v1-i2"],
                FinalizeCode::EnumerationCountMismatch,
            ),
            (
                RawEnumeration::RootAbsent,
                vec!["f1v1-i1"],
                FinalizeCode::EnumerationCountMismatch,
            ),
            (
                RawEnumeration::Closed(1),
                vec!["bad/path"],
                FinalizeCode::InvalidSourceRef,
            ),
        ] {
            let mut f = baseline();
            f.push(frame(6, listing(0, second, outcome, &sources)));
            assert_eq!(finalize(raw(f), &catalog()).unwrap_err(), code);
        }
    }
    let mut f = baseline();
    f.push(frame(
        6,
        listing(0, true, RawEnumeration::Closed(1), &["f1v1-unissued"]),
    ));
    let x = run(f);
    assert_eq!(
        x.members()[0].qualifiers(),
        vec![Qualifier::NewSourcesObserved {
            count: 1,
            basis: CountBasis::Exact
        }]
    );
}
#[test]
fn raw_validation_foreign_manifest_barrier_capacity_and_representation_codes() {
    let mut r = raw(baseline());
    r.manifest.members.clear();
    assert_eq!(
        finalize(r, &catalog()).unwrap_err(),
        FinalizeCode::InvalidManifest
    );
    let mut r = raw(baseline());
    r.manifest.members.push(scope("f1"));
    assert_eq!(
        finalize(r, &catalog()).unwrap_err(),
        FinalizeCode::InvalidManifest
    );
    let mut r = raw(baseline());
    r.pre_barrier[0].attempt = AttemptRef::new("other").unwrap();
    assert_eq!(
        finalize(r, &catalog()).unwrap_err(),
        FinalizeCode::ForeignFrame
    );
    let mut r = raw(baseline());
    r.barrier = Barrier::NotDrained;
    assert_eq!(
        finalize(r, &catalog()).unwrap_err(),
        FinalizeCode::InvalidBarrier
    );
    let mut r = raw(baseline());
    r.pre_barrier[0].frame_seq = 4294967296;
    assert_eq!(
        finalize(r, &catalog()).unwrap_err(),
        FinalizeCode::CountOverflow
    );
    let mut f = baseline();
    f[1].payload = record("f1v1-i1", 4294967296, 1);
    assert_eq!(
        finalize(raw(f), &catalog()).unwrap_err(),
        FinalizeCode::CountOverflow
    );
    let mut f = baseline();
    f[1].payload = record("f1v1-i1", 4294967295, 1);
    assert!(finalize(raw(f), &catalog()).is_ok());
    let mut f = baseline();
    f[4].frame_seq = 4294967295;
    assert!(finalize(raw(f), &catalog()).is_ok());
    let mut f = baseline();
    f[4].frame_seq = 0;
    assert!(finalize(raw(f), &catalog()).is_ok());
}

#[test]
fn source_seq_zero_is_reconciled_as_a_sequence_fact_not_a_representation_overflow() {
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 0, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(
        x.members()[0].sources()[0].1.outcome(),
        SourceOutcome::PartialRead {
            records: 1,
            reason: PartialReason::DeliveryLoss
        }
    );
}

#[test]
fn r88_r101e_list_frame_and_record_byte_bounds_are_not_truncation() {
    for n in [255usize, 256, 257] {
        let refs: Vec<_> = (0..n).map(|k| format!("f1v1-label{k}")).collect();
        let refs: Vec<_> = refs.iter().map(String::as_str).collect();
        let mut f = baseline();
        f.push(frame(
            6,
            listing(0, true, RawEnumeration::Closed(n as u64), &refs),
        ));
        match n {
            257 => assert_eq!(
                finalize(raw(f), &catalog()).unwrap_err(),
                FinalizeCode::CapacityExceeded
            ),
            _ => {
                let x = run(f);
                let SecondEnumeration::Performed { new_sources, .. } =
                    x.members()[0].second_enumeration()
                else {
                    panic!("expected usable listing")
                };
                assert_eq!(new_sources.len(), n);
            }
        }
    }
    for n in [MAX_FRAMES - 1, MAX_FRAMES, MAX_FRAMES + 1] {
        let r = raw(vec![frame(1, initial(&[])); n]);
        match n {
            4097 => assert_eq!(
                finalize(r, &catalog()).unwrap_err(),
                FinalizeCode::CapacityExceeded
            ),
            _ => {
                let x = finalize(r, &catalog()).unwrap();
                assert_eq!(x.dispositions().len(), n);
                assert_eq!(x.members()[0].delivery().global_replays, n as u32 - 1);
            }
        }
    }
    for n in [MAX_RECORD_BYTES - 1, MAX_RECORD_BYTES, MAX_RECORD_BYTES + 1] {
        let mut f = baseline();
        f[1].payload = Payload::Source {
            source: "f1v1-i1".into(),
            source_seq: 1,
            event: SourceEvent::Record {
                bytes: vec![1; n],
                result: abf_parser::ParseResult::Rejected(abf_parser::Code::MalformedJson),
            },
        };
        assert_eq!(finalize(raw(f), &catalog()).is_ok(), n <= 65536);
    }
}
#[test]
fn r49_r50_r52_r53_no_traversal_and_conflict_precedence_have_literal_matched_rows() {
    for (terminal, outcome, matched, status) in [
        (
            end("f1v1-i1", 0),
            SourceOutcome::PartialRead {
                records: 0,
                reason: PartialReason::DeliveryConflict,
            },
            vec![4, 10],
            SourceRunStatus::Partial,
        ),
        (
            event("f1v1-i1", 2, SourceEvent::Skipped(StopReason::Budget)),
            SourceOutcome::NotAttempted(StopReason::Budget),
            vec![3],
            SourceRunStatus::NotCollected,
        ),
    ] {
        let f = vec![
            frame(1, initial(&["f1v1-i1"])),
            frame(2, record("f1v1-i1", 1, 1)),
            frame(2, record("f1v1-i1", 1, 2)),
            frame(3, terminal),
            frame(4, Payload::RunEnd),
        ];
        let x = run(f);
        let r = &x.members()[0].sources()[0].1;
        assert_eq!(r.outcome(), outcome);
        assert_eq!(r.matched(), matched);
        assert_eq!(r.status(), status);
        assert!(r.conflict_marked());
        assert!(x.members()[0].records().is_empty());
        assert_eq!(x.members()[0].delivery().seq_conflicts, 1);
    }
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(2, end("f1v1-i1", 0)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(x.members()[0].sources()[0].1.claims().len(), 1);
    assert_eq!(x.members()[0].sources()[0].1.matched(), &[4, 8]);
    assert_eq!(
        x.delivery_evidence()[2].terminal(),
        Some(TerminalKind::End {
            records: 0,
            change: ChangeObservation::NoChangeObserved
        })
    );
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, end("f1v1-i1", 0)),
        frame(2, end("f1v1-i1", 0)),
        frame(3, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(x.members()[0].sources()[0].1.claims().len(), 1);
    assert_eq!(x.members()[0].sources()[0].1.matched(), &[10]);
    assert!(x.members()[0].records().is_empty());
}
#[test]
fn r20_r34_r59_r95_n24_distinct_identities_and_zero_claims_are_retained_verbatim() {
    let accepted = abf_parser::parse(br#"{"tokens":{"input":0}}"#, None, abf_parser::Limits::MAX);
    assert_eq!(
        accepted.observation().unwrap().input,
        abf_parser::Field::Valid(0)
    );
    let payload = |seq| Payload::Source {
        source: "f1v1-i1".into(),
        source_seq: seq,
        event: SourceEvent::Record {
            bytes: br#"{"tokens":{"input":0}}"#.to_vec(),
            result: accepted.clone(),
        },
    };
    let x = run(vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, payload(1)),
        frame(3, payload(2)),
        frame(4, end("f1v1-i1", 2)),
        frame(5, Payload::RunEnd),
    ]);
    assert_eq!(x.members()[0].records().len(), 2);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(x.members()[0].delivery().replays_quarantined, 0);
    assert!(
        x.members()[0]
            .records()
            .iter()
            .all(|r| r.parse_result() == &accepted)
    );
    let x = run(vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, payload(1)),
        frame(3, payload(1)),
        frame(4, end("f1v1-i1", 1)),
        frame(5, Payload::RunEnd),
    ]);
    assert_eq!(x.members()[0].records().len(), 2);
    assert_eq!(
        x.members()[0].sources()[0].1.outcome(),
        SourceOutcome::AnomalousSequence {
            kind: SequenceAnomaly::SourceSeqReused,
            records: 2
        }
    );
}
#[test]
fn r80_r81_r82_initial_inventory_controls_second_qualification() {
    for (initial_outcome, initial_sources, expected_verdict, qualifier) in [
        (
            RawEnumeration::Partial(1, StopReason::Budget),
            vec!["f1v1-i1"],
            CoverageVerdict::InventoryIncomplete,
            Qualifier::NewSourcesObserved {
                count: 1,
                basis: CountBasis::Exact,
            },
        ),
        (
            RawEnumeration::Closed(0),
            vec![],
            CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration),
            Qualifier::NewSourcesObserved {
                count: 2,
                basis: CountBasis::Exact,
            },
        ),
        (
            RawEnumeration::Failed(EnumFailure::PermissionDenied),
            vec![],
            CoverageVerdict::InventoryUnknown,
            Qualifier::SecondEnumerationUnusable,
        ),
    ] {
        let f = vec![
            frame(1, listing(0, false, initial_outcome, &initial_sources)),
            frame(
                2,
                listing(0, true, RawEnumeration::Closed(2), &["f1v1-i1", "f1v1-i2"]),
            ),
            frame(3, Payload::RunEnd),
        ];
        let x = run(f);
        let m = &x.members()[0];
        assert_eq!(m.verdict(), expected_verdict);
        assert_eq!(m.qualifiers(), vec![qualifier]);
        assert_eq!(m.sources().len(), initial_sources.len());
        assert!(m.records().is_empty());
    }
}
#[test]
fn r97a_r97b_listing_and_finalization_never_call_or_advance_issuer() {
    let c = catalog();
    let mut i = FixtureRefIssuer::new(&c);
    let s = scope("f1");
    let a = i.create_instance(&s).unwrap();
    let b = i.create_instance(&s).unwrap();
    let refs = [a.as_str(), b.as_str()];
    let init = initial(&refs);
    let second = listing(0, true, RawEnumeration::Closed(2), &refs);
    assert_eq!(i.instance_counter(&s).unwrap(), 2);
    let x = run(vec![
        frame(1, init),
        frame(2, end(a.as_str(), 0)),
        frame(3, end(b.as_str(), 0)),
        frame(4, second),
        frame(5, Payload::RunEnd),
    ]);
    assert_eq!(
        x.members()[0].qualifiers(),
        vec![Qualifier::SecondEnumerationNoNewSources]
    );
    assert_eq!(i.instance_counter(&s).unwrap(), 2);
    assert_eq!(i.registry_size(&s).unwrap(), 2);
    assert_eq!(i.create_instance(&s).unwrap().as_str(), "f1v1-i3");
}

#[test]
fn r21_r23_r24_r31_r44_r48_r67_r89_environment_and_member_verdicts_are_independent() {
    let mut f = baseline();
    f.push(frame(
        6,
        listing(1, false, RawEnumeration::RootUnresolved, &[]),
    ));
    let x = run(f);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::InventoryUnknown);
    let mut r = raw(baseline());
    r.manifest.members =
        vec![CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap()];
    let mut b = CatalogBuilder::new();
    b.add(r.manifest.members[0].clone(), "f1v1").unwrap();
    let c = b.build();
    let wsl = finalize(r.clone(), &c).unwrap();
    assert_eq!(
        wsl.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(wsl.members()[0].records().len(), 1);
    let mut f = baseline();
    f.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    let windows = run(f);
    assert_eq!(
        windows.members()[0].qualifiers(),
        vec![Qualifier::NewSourcesObserved {
            count: 1,
            basis: CountBasis::Exact
        }]
    );
    assert!(wsl.members()[0].qualifiers().is_empty());
    let global = run(vec![
        frame(1, initial(&[])),
        frame(1, listing(0, false, RawEnumeration::RootAbsent, &[])),
    ]);
    assert_eq!(global.members()[0].verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(wsl.members()[0].delivery().global_seq_conflicts, 0);
    r.pre_barrier = vec![frame(
        1,
        listing(
            0,
            false,
            RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
            &[],
        ),
    )];
    let unavailable = finalize(r, &c).unwrap();
    assert_eq!(
        unavailable.members()[0].verdict(),
        CoverageVerdict::ScopeUnavailable(UnavailableReason::Unavailable)
    );
    assert_eq!(wsl.members()[0].records().len(), 1);
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(
            2,
            listing(1, false, RawEnumeration::Closed(1), &["f2v1-i1"]),
        ),
        frame(3, record("f1v1-i1", 1, 1)),
        frame(4, end("f1v1-i1", 1)),
        frame(
            5,
            event("f2v1-i1", 1, SourceEvent::Skipped(StopReason::Budget)),
        ),
        frame(6, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NotCovered);
    assert_eq!(
        x.members()[1].sources()[0].1.status(),
        SourceRunStatus::NotCollected
    );
}
#[test]
fn r09_r11_r25_n18_failed_records_and_retry_subset_are_never_repaired_or_erased() {
    let mut f = baseline();
    f[3].payload = record("f1v1-i2", 1, 2);
    f.pop();
    let mut r = raw(f);
    r.process = ProcessEnd::Crashed;
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(x.members()[0].records().len(), 2);
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::PartiallyCovered);
    assert_eq!(
        x.members()[0].sources()[1].1.status(),
        SourceRunStatus::Failed
    );
    let mut r = raw(baseline());
    r.manifest.attempt = AttemptRef::new("att2").unwrap();
    r.manifest.retry_of = Some(AttemptRef::new("att1").unwrap());
    r.manifest.subset = RetrySubset::OnlyNonCoveredSources;
    for f in &mut r.pre_barrier {
        f.attempt = r.manifest.attempt.clone();
    }
    let retry = finalize(r, &catalog()).unwrap();
    assert_eq!(retry.manifest().subset, RetrySubset::OnlyNonCoveredSources);
    assert_eq!(retry.manifest().retry_of.as_ref().unwrap().as_str(), "att1");
    assert_eq!(x.members()[0].records().len(), 2);
}
#[derive(PartialEq, Eq)]
struct Signature {
    scope: CatalogScope,
    enumeration: EnumerationOutcome,
    sources: Vec<(SourceRef, SourceReconciliation)>,
    records: Vec<(SourceRef, u32, u32, abf_parser::ParseResult)>,
    delivery: DeliveryIntegrity,
    second: SecondEnumeration,
    verdict: CoverageVerdict,
    cause: VerdictRule,
    qualifiers: Vec<Qualifier>,
}
impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signature([redacted])")
    }
}
fn signature(x: &FinalizedExecution) -> Vec<Signature> {
    let mut result: Vec<_> = x
        .members()
        .iter()
        .map(|m| Signature {
            scope: m.scope().clone(),
            enumeration: m.enumeration(),
            sources: m.sources().to_vec(),
            records: m
                .records()
                .iter()
                .map(|r| {
                    (
                        r.source().clone(),
                        r.source_seq(),
                        r.frame_seq(),
                        r.parse_result().clone(),
                    )
                })
                .collect(),
            delivery: m.delivery(),
            second: m.second_enumeration().clone(),
            verdict: m.verdict(),
            cause: m.cause(),
            qualifiers: m.qualifiers(),
        })
        .collect();
    result.sort_by(|a, b| a.scope.family().as_str().cmp(b.scope.family().as_str()));
    result
}
#[test]
fn n14_n15_permutation_conservation_and_member_order_cover_admission_and_reconciliation() {
    let mut cases = vec![baseline()];
    for extra in [
        record("f1v1-i1", 1, 1),
        record("f1v1-i1", 1, 2),
        event("f1v1-i1", 2, SourceEvent::Skipped(StopReason::Budget)),
        end("f1v1-i1", 1),
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ] {
        let mut f = baseline();
        f.push(frame(2, extra));
        cases.push(f);
    }
    let mut f = baseline();
    f.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    cases.push(f);
    for frames in cases {
        let reference = signature(&run(frames.clone()));
        for order in permutations(frames.len()) {
            let f: Vec<_> = order.iter().map(|&i| frames[i].clone()).collect();
            let x = run(f.clone());
            assert_eq!(signature(&x), reference);
            let admitted_records = f
                .iter()
                .zip(x.dispositions())
                .filter(|(f, d)| {
                    **d == Disposition::Admitted
                        && matches!(
                            f.payload,
                            Payload::Source {
                                event: SourceEvent::Record { .. },
                                ..
                            }
                        )
                })
                .count();
            assert_eq!(
                x.members().iter().map(|m| m.records().len()).sum::<usize>(),
                admitted_records
            );
            let mut r = raw(f);
            r.manifest.members.swap(0, 1);
            for frame in &mut r.pre_barrier {
                if let Payload::Enumeration { member, .. } = &mut frame.payload {
                    *member = 1 - *member;
                }
            }
            let swapped = finalize(r, &catalog()).unwrap();
            assert_eq!(signature(&swapped), reference);
        }
    }
}

#[test]
fn r98g_r99c_ambiguous_member_never_changes_a_covered_peer() {
    for second in [false, true] {
        let f = vec![
            frame(1, listing(0, false, RawEnumeration::Closed(0), &[])),
            frame(
                2,
                listing(0, second, RawEnumeration::Closed(1), &["f1v1-i1"]),
            ),
            frame(3, listing(0, true, RawEnumeration::Closed(0), &[])),
            frame(
                4,
                listing(1, false, RawEnumeration::Closed(1), &["f2v1-i1"]),
            ),
            frame(5, record("f2v1-i1", 1, 1)),
            frame(6, end("f2v1-i1", 1)),
            frame(7, Payload::RunEnd),
        ];
        let reference = signature(&run(f.clone()));
        for order in permutations(f.len()) {
            let x = run(order.iter().map(|&i| f[i].clone()).collect());
            assert_eq!(signature(&x), reference);
            assert_eq!(x.members()[0].verdict(), CoverageVerdict::Inconsistent);
            assert_eq!(
                x.members()[1].verdict(),
                CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
            );
            assert_eq!(x.members()[1].records().len(), 1);
            assert!(x.members()[1].qualifiers().is_empty());
            assert_eq!(x.members()[1].delivery().global_seq_conflicts, 0);
        }
    }
}
#[test]
fn r19_parser_rejections_are_retained_without_changing_traversal() {
    let rejected = Payload::Source {
        source: "f1v1-i1".into(),
        source_seq: 2,
        event: SourceEvent::Record {
            bytes: b"{".to_vec(),
            result: abf_parser::ParseResult::Rejected(abf_parser::Code::MalformedJson),
        },
    };
    let x = run(vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, rejected),
        frame(4, end("f1v1-i1", 2)),
        frame(5, Payload::RunEnd),
    ]);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(x.members()[0].records().len(), 2);
    assert_eq!(
        x.members()[0].records()[0]
            .parse_result()
            .observation()
            .unwrap()
            .input,
        abf_parser::Field::Valid(100)
    );
    assert_eq!(
        x.members()[0].records()[1].parse_result(),
        &abf_parser::ParseResult::Rejected(abf_parser::Code::MalformedJson)
    );
}
#[test]
fn r99e_noinitial_late_seconds_never_change_noattempt_or_initial_authority() {
    let x = run(vec![
        frame(1, listing(0, true, RawEnumeration::Closed(0), &[])),
        frame(2, Payload::RunEnd),
    ]);
    let before = signature(&x);
    let mut log = PostFinalizationLog::bind(&x);
    for n in [3, 4] {
        log.append(
            frame(n, listing(0, true, RawEnumeration::Closed(0), &[])),
            SupervisorInstant::synthetic(n),
        )
        .unwrap();
    }
    assert_eq!(signature(&x), before);
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::NoAttempt);
    assert_eq!(x.members()[0].delivery().second_enum_identities, 1);
    assert_eq!(log.snapshot().entries().len(), 2);
    assert_eq!(
        assess(&x.members()[0], &log.snapshot()).unwrap().ceiling,
        PresentationCeiling::WeakenedByLateDelivery
    );
}
#[test]
fn opaque_reader_times_and_subset_lineage_remain_inspectable_on_member_evidence() {
    let mut f = baseline();
    if let Payload::Enumeration { observed_at, .. } = &mut f[0].payload {
        *observed_at = Some(ReaderInstant::synthetic(123));
    }
    let mut r = raw(f);
    r.manifest.subset = RetrySubset::OnlyNonCoveredSources;
    r.manifest.retry_of = Some(AttemptRef::new("previous").unwrap());
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(
        x.delivery_evidence()[0].enumerated_at(),
        Some(ReaderInstant::synthetic(123))
    );
    assert_eq!(
        x.members()[0].lineage().1,
        RetrySubset::OnlyNonCoveredSources
    );
    assert_eq!(x.members()[0].lineage().0.unwrap().as_str(), "previous");
}

#[test]
fn r95_r95b_literal_four_member_inventory_and_real_zero_claim_population() {
    for include_zero in [false, true] {
        let count = if include_zero { 4 } else { 3 };
        let mut b = CatalogBuilder::new();
        let mut scopes = Vec::new();
        for n in 1..=count {
            let s = scope(&format!("f{n}"));
            b.add(s.clone(), &format!("f{n}v1")).unwrap();
            scopes.push(s);
        }
        let catalog = b.build();
        let mut f = vec![
            frame(1, listing(0, false, RawEnumeration::Closed(0), &[])),
            frame(2, listing(1, false, RawEnumeration::RootAbsent, &[])),
            frame(3, listing(2, false, RawEnumeration::RootUnresolved, &[])),
        ];
        if include_zero {
            let bytes = br#"{"tokens":{"input":0}}"#.to_vec();
            let result = abf_parser::parse(&bytes, None, abf_parser::Limits::MAX);
            f.push(frame(
                4,
                listing(3, false, RawEnumeration::Closed(1), &["f4v1-i1"]),
            ));
            f.push(frame(
                5,
                Payload::Source {
                    source: "f4v1-i1".into(),
                    source_seq: 1,
                    event: SourceEvent::Record { bytes, result },
                },
            ));
            f.push(frame(6, end("f4v1-i1", 1)));
        }
        f.push(frame(f.len() as u64 + 1, Payload::RunEnd));
        let mut r = raw(f);
        r.manifest.members = scopes;
        let x = finalize(r, &catalog).unwrap();
        assert_eq!(
            x.members()[0].verdict(),
            CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration)
        );
        assert_eq!(
            x.members()[1].verdict(),
            CoverageVerdict::NoSourcesFound(NoSourcesReason::RootAbsent)
        );
        assert_eq!(x.members()[2].verdict(), CoverageVerdict::InventoryUnknown);
        assert!(
            x.members()[..3]
                .iter()
                .all(|m| m.records().is_empty() && m.sources().is_empty())
        );
        assert_eq!(
            x.members().iter().map(|m| m.records().len()).sum::<usize>(),
            usize::from(include_zero)
        );
        if include_zero {
            assert_eq!(
                x.members()[3].verdict(),
                CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
            );
            assert_eq!(
                x.members()[3].records()[0]
                    .parse_result()
                    .observation()
                    .unwrap()
                    .input,
                abf_parser::Field::Valid(0)
            );
        }
    }
}
#[test]
fn r15_r41_n22_attributable_loss_and_unattributable_gap_have_different_verdicts() {
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(4, record("f1v1-i1", 3, 2)),
        frame(5, end("f1v1-i1", 3)),
        frame(6, Payload::RunEnd),
    ];
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(
        m.sources()[0].1.outcome(),
        SourceOutcome::PartialRead {
            records: 2,
            reason: PartialReason::DeliveryLoss
        }
    );
    assert_eq!(m.delivery().attributable_gaps, 1);
    assert_eq!(m.delivery().unattributable_gaps, 0);
    assert_eq!(m.verdict(), CoverageVerdict::PartiallyCovered);
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(5, Payload::RunEnd),
    ];
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(m.sources()[0].1.status(), SourceRunStatus::Complete);
    assert_eq!(m.delivery().unattributable_gaps, 1);
    assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::Inconsistent);
}
#[test]
fn r34_copied_content_in_distinct_sources_is_never_physically_deduplicated() {
    let f = vec![
        frame(1, initial(&["f1v1-i1", "f1v1-i2"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, record("f1v1-i2", 1, 1)),
        frame(4, end("f1v1-i1", 1)),
        frame(5, end("f1v1-i2", 1)),
        frame(6, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(x.members()[0].records().len(), 2);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_ne!(
        x.members()[0].records()[0].source(),
        x.members()[0].records()[1].source()
    );
    assert_eq!(
        x.members()[0].records()[0].parse_result(),
        x.members()[0].records()[1].parse_result()
    );
}
#[test]
fn r11_cancelled_active_zero_record_source_is_partial_and_keeps_prior_records() {
    let f = vec![
        frame(1, initial(&["f1v1-i1", "f1v1-i2", "f1v1-i3"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, event("f1v1-i2", 1, SourceEvent::OpenSuccess)),
        frame(
            5,
            event("f1v1-i3", 1, SourceEvent::Skipped(StopReason::Cancelled)),
        ),
    ];
    let mut r = raw(f);
    r.process = ProcessEnd::KilledCancel;
    let x = finalize(r, &catalog()).unwrap();
    let m = &x.members()[0];
    assert_eq!(m.verdict(), CoverageVerdict::PartiallyCovered);
    assert_eq!(m.records().len(), 1);
    assert_eq!(
        m.sources()[1].1.outcome(),
        SourceOutcome::PartialRead {
            records: 0,
            reason: PartialReason::Cancelled
        }
    );
    assert_eq!(m.sources()[1].1.status(), SourceRunStatus::Partial);
    assert_eq!(m.sources()[2].1.status(), SourceRunStatus::NotCollected);
}
#[test]
fn r02_r10_partial_inventory_and_unopened_timeout_have_literal_verdicts() {
    let f = vec![
        frame(
            1,
            listing(
                0,
                false,
                RawEnumeration::Partial(1, StopReason::Budget),
                &["f1v1-i1"],
            ),
        ),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::InventoryIncomplete
    );
    assert_eq!(
        x.members()[0].sources()[0].1.status(),
        SourceRunStatus::Complete
    );
    assert_eq!(x.members()[0].records().len(), 1);
    let mut r = raw(vec![frame(1, initial(&["f1v1-i1", "f1v1-i2"]))]);
    r.process = ProcessEnd::KilledTimeout;
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::NotCovered);
    assert_eq!(x.members()[0].sources().len(), 2);
    assert!(
        x.members()[0]
            .sources()
            .iter()
            .all(|s| s.1.outcome() == SourceOutcome::Unaccounted)
    );
    assert!(x.members()[0].records().is_empty());
}
#[test]
fn r35_r36_r37_r40_r42_r54b_no_terminal_status_repair_and_claims_are_preserved() {
    for (terminal, claim, status) in [
        (
            SourceEvent::Skipped(StopReason::Cancelled),
            NoTraversalClaim::Skipped,
            SourceRunStatus::NotCollected,
        ),
        (
            SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
            NoTraversalClaim::OpenFailed,
            SourceRunStatus::Unavailable,
        ),
        (
            SourceEvent::Vanished,
            NoTraversalClaim::Vanished,
            SourceRunStatus::Unavailable,
        ),
    ] {
        let f = vec![
            frame(1, initial(&["f1v1-i1", "f1v1-i2"])),
            frame(2, record("f1v1-i1", 1, 1)),
            frame(3, end("f1v1-i1", 1)),
            frame(4, record("f1v1-i2", 1, 1)),
            frame(5, event("f1v1-i2", 2, terminal)),
            frame(6, Payload::RunEnd),
        ];
        let x = run(f);
        let m = &x.members()[0];
        assert_eq!(m.records().len(), 2);
        assert_eq!(
            m.sources()[1].1.outcome(),
            SourceOutcome::TerminalContradiction { claim, records: 1 }
        );
        assert_eq!(m.sources()[1].1.status(), status);
        assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
    }
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(
            4,
            event(
                "f1v1-i1",
                3,
                SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
            ),
        ),
        frame(5, Payload::RunEnd),
    ];
    let x = run(f);
    let s = &x.members()[0].sources()[0].1;
    assert_eq!(s.status(), SourceRunStatus::Unavailable);
    assert_eq!(s.claims().len(), 2);
    assert!(matches!(
        s.claims()[0].kind,
        TerminalKind::End { records: 1, .. }
    ));
    assert_eq!(
        s.claims()[1].kind,
        TerminalKind::OpenFailed(OpenFailure::PermissionDenied)
    );
    assert_eq!(
        s.outcome(),
        SourceOutcome::AnomalousSequence {
            kind: SequenceAnomaly::ConflictingTerminals,
            records: 1
        }
    );
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(
        x.members()[0].sources()[0].1.outcome(),
        SourceOutcome::AnomalousSequence {
            kind: SequenceAnomaly::MissingEndMarker,
            records: 1
        }
    );
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::Inconsistent);
    let f = vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(
            2,
            event("f1v1-i1", 1, SourceEvent::Skipped(StopReason::Budget)),
        ),
        frame(
            3,
            event(
                "f1v1-i1",
                2,
                SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
            ),
        ),
        frame(4, Payload::RunEnd),
    ];
    let x = run(f);
    assert_eq!(
        x.members()[0].sources()[0].1.status(),
        SourceRunStatus::Unavailable
    );
    assert_eq!(x.members()[0].sources()[0].1.claims().len(), 2);
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::Inconsistent);
    assert!(x.members()[0].records().is_empty());
}
#[test]
fn r51_every_expected_source_named_by_global_d2_is_conflict_marked() {
    let f = vec![
        frame(1, initial(&["f1v1-i1", "f1v1-i2"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(2, record("f1v1-i2", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, end("f1v1-i2", 1)),
        frame(5, Payload::RunEnd),
    ];
    let x = run(f);
    assert!(x.members()[0].records().is_empty());
    assert_eq!(x.members()[0].delivery().global_seq_conflicts, 1);
    for (_, s) in x.members()[0].sources() {
        assert!(s.conflict_marked());
        assert_eq!(
            s.outcome(),
            SourceOutcome::PartialRead {
                records: 0,
                reason: PartialReason::DeliveryConflict
            }
        );
        assert_eq!(s.matched(), &[4, 8]);
    }
}
#[test]
fn r85_r100b_pure_second_d2_never_changes_initial_or_successful_source_outcomes() {
    let mut f = baseline();
    f.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    f.push(frame(
        6,
        listing(0, true, RawEnumeration::Closed(2), &["f1v1-i1", "f1v1-i2"]),
    ));
    let x = run(f);
    let m = &x.members()[0];
    assert_eq!(m.enumeration(), EnumerationOutcome::Closed { listed: 2 });
    assert_eq!(m.sources().len(), 2);
    assert_eq!(m.records().len(), 1);
    assert!(
        m.sources()
            .iter()
            .all(|s| s.1.status() == SourceRunStatus::Complete && !s.1.conflict_marked())
    );
    assert_eq!(m.delivery().second_enum_identities, 0);
    assert!(m.delivery().second_enum_conflict);
    assert_eq!(
        m.second_enumeration(),
        &SecondEnumeration::Unusable(SecondUnusable::DeliveryConflict)
    );
    assert_eq!(
        x.dispositions()[5],
        Disposition::Quarantined(QuarantineCode::SequenceConflict)
    );
    assert_eq!(
        x.dispositions()[6],
        Disposition::Quarantined(QuarantineCode::SequenceConflict)
    );
}

// ABF107Q: V1.6 supersedes ABF107N's documented B/C rejection expectation.
#[test]
fn abf107n_f01_b_c_original_inputs_now_follow_v16() {
    for source in ["f2v1-i1", "outside-i1"] {
        assert!(SourceRef::new(source).is_ok());
        let mut r = raw(vec![
            frame(1, initial(&[])),
            frame(2, record(source, 1, 1)),
            frame(3, Payload::RunEnd),
        ]);
        r.manifest.members.truncate(1);
        let x = finalize(r, &catalog()).unwrap();
        assert_eq!(x.unbound_unexpected_source_deliveries(), 1);
        assert_eq!(x.members()[0].cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(x.members()[0].delivery().unexpected_source, 0);
    }
}

#[test]
fn abf107n_f01_a_d_e_f_supported_boundaries_remain_distinct() {
    let x = run(vec![
        frame(1, initial(&[])),
        frame(2, record("f1v1-extra", 1, 1)),
        frame(3, Payload::RunEnd),
    ]);
    assert_eq!(
        x.dispositions()[1],
        Disposition::Quarantined(QuarantineCode::UnexpectedSource)
    );
    assert_eq!(x.members()[0].delivery().unexpected_source, 1);
    assert_eq!(x.members()[0].verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    assert!(x.members()[0].records().is_empty());
    let bad = raw(vec![frame(1, record("invalid/path", 1, 1))]);
    assert_eq!(
        finalize(bad, &catalog()).unwrap_err(),
        FinalizeCode::InvalidSourceRef
    );
    for second in [false, true] {
        let f = vec![
            frame(1, initial(&[])),
            frame(
                2,
                listing(0, second, RawEnumeration::Closed(1), &["f2v1-i1"]),
            ),
        ];
        assert_eq!(
            finalize(raw(f), &catalog()).unwrap_err(),
            FinalizeCode::SourceScopeMismatch
        );
    }
    for other in ["f2v1-i1", "outside-i1"] {
        let f = [
            frame(1, initial(&["f1v1-i1"])),
            frame(2, record("f1v1-i1", 1, 1)),
            frame(2, record(other, 1, 1)),
            frame(3, end("f1v1-i1", 1)),
            frame(4, Payload::RunEnd),
        ];
        for order in permutations(f.len()) {
            let x = run(order.iter().map(|&i| f[i].clone()).collect());
            assert!(
                x.members()
                    .iter()
                    .all(|m| m.delivery().global_seq_conflicts == 1
                        && m.verdict() == CoverageVerdict::Inconsistent)
            );
            assert_eq!(x.members()[0].sources()[0].1.matched(), &[4, 8]);
            assert!(x.members().iter().all(|m| m.records().is_empty()));
            assert_eq!(
                x.dispositions()
                    .iter()
                    .filter(|&&d| d == Disposition::Quarantined(QuarantineCode::SequenceConflict))
                    .count(),
                2
            );
        }
    }
}

#[test]
fn abf107n_r66a_exact_global_d2_late_controls_preserve_all_snapshot_facts() {
    let mut r = raw(vec![
        frame(1, initial(&[])),
        frame(1, listing(0, false, RawEnumeration::RootAbsent, &[])),
    ]);
    r.process = ProcessEnd::Crashed;
    let x = finalize(r, &catalog()).unwrap();
    let before = signature(&x);
    assert_eq!(
        x.members()[0].enumeration(),
        EnumerationOutcome::Failed(EnumFailure::DeliveryConflict)
    );
    assert_eq!(
        x.members()[1].enumeration(),
        EnumerationOutcome::NotAttempted
    );
    assert!(
        x.dispositions()
            .iter()
            .all(|&d| d == Disposition::Quarantined(QuarantineCode::SequenceConflict))
    );
    for m in x.members() {
        assert_eq!(m.delivery().global_seq_conflicts, 1);
        assert_eq!(m.process(), ProcessEnd::Crashed);
        assert!(!m.run_end_frame());
        assert!(m.sources().is_empty() && m.records().is_empty());
    }
    let mut log = PostFinalizationLog::bind(&x);
    log.append(
        frame(
            1,
            listing(
                1,
                false,
                RawEnumeration::Closed(3),
                &["f2v1-i1", "f2v1-i2", "f2v1-i3"],
            ),
        ),
        SupervisorInstant::synthetic(10),
    )
    .unwrap();
    log.append(frame(1, initial(&[])), SupervisorInstant::synthetic(11))
        .unwrap();
    let d = log.snapshot();
    assert_eq!(d.entries().len(), 2);
    // The D2 identity was never admitted; late variants do not become replays of an admitted identity.
    assert!(
        d.entries()
            .iter()
            .all(|e| e.kind == FrameKind::InitialEnumeration
                && e.relation == IdentityRelation::NewIdentity
                && e.attribution == Attribution::Unattributed)
    );
    for m in x.members() {
        let a = assess(m, &d).unwrap();
        assert_eq!(a.verdict, CoverageVerdict::Inconsistent);
        assert_eq!(a.ceiling, PresentationCeiling::WeakenedByLateDelivery);
        assert_eq!(
            a.post,
            PostFinalization::Observed {
                records: 0,
                other: 2,
                conflicts: 0,
                unattributed: 2
            }
        );
    }
    assert_eq!(signature(&x), before);
}

#[test]
fn abf107n_r66b_exact_pq_global_d1_late_conflict_never_rewrites_verdicts() {
    let x = run(vec![
        frame(1, initial(&[])),
        frame(1, initial(&[])),
        frame(2, Payload::RunEnd),
    ]);
    let before = signature(&x);
    assert_eq!(
        x.dispositions(),
        &[
            Disposition::Admitted,
            Disposition::Quarantined(QuarantineCode::ExactReplay),
            Disposition::Admitted
        ]
    );
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration)
    );
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    for m in x.members() {
        assert_eq!(m.delivery().global_replays, 1);
        assert_eq!(m.delivery().global_seq_conflicts, 0);
        assert_eq!(m.qualifiers(), vec![Qualifier::TransportReplayQuarantined]);
    }
    let mut log = PostFinalizationLog::bind(&x);
    log.append(
        frame(1, listing(0, false, RawEnumeration::RootAbsent, &[])),
        SupervisorInstant::synthetic(3),
    )
    .unwrap();
    let d = log.snapshot();
    assert_eq!(
        d.entries()[0].relation,
        IdentityRelation::ConflictsWithAdmittedIdentity
    );
    for m in x.members() {
        let a = assess(m, &d).unwrap();
        assert_eq!(a.verdict, m.verdict());
        assert_eq!(a.ceiling, PresentationCeiling::WeakenedByLateDelivery);
        assert_eq!(
            a.post,
            PostFinalization::Observed {
                records: 0,
                other: 1,
                conflicts: 1,
                unattributed: 1
            }
        );
    }
    assert_eq!(signature(&x), before);
}

#[test]
fn abf107n_r97b_complete_issuer_creation_listing_counter_witness() {
    let c = catalog();
    let s = scope("f1");
    let mut issuer = FixtureRefIssuer::new(&c);
    let a = issuer.create_instance(&s).unwrap();
    let b = issuer.create_instance(&s).unwrap();
    assert_eq!(issuer.instance_counter(&s).unwrap(), 2);
    let init = initial(&[a.as_str(), b.as_str()]);
    assert_eq!(issuer.instance_counter(&s).unwrap(), 2);
    let new = issuer.create_instance(&s).unwrap();
    assert_eq!(new.as_str(), "f1v1-i3");
    assert_eq!(issuer.instance_counter(&s).unwrap(), 3);
    let second = listing(
        0,
        true,
        RawEnumeration::Closed(3),
        &[a.as_str(), b.as_str(), new.as_str()],
    );
    assert_eq!(issuer.instance_counter(&s).unwrap(), 3);
    let x = run(vec![
        frame(1, init),
        frame(2, record(a.as_str(), 1, 1)),
        frame(3, end(a.as_str(), 1)),
        frame(4, end(b.as_str(), 0)),
        frame(5, second),
        frame(6, Payload::RunEnd),
    ]);
    assert_eq!(issuer.instance_counter(&s).unwrap(), 3);
    assert_eq!(issuer.registry_size(&s).unwrap(), 3);
    assert_eq!(x.members()[0].sources().len(), 2);
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(
        x.members()[0].qualifiers(),
        vec![Qualifier::NewSourcesObserved {
            count: 1,
            basis: CountBasis::Exact
        }]
    );
    let SecondEnumeration::Performed { new_sources, .. } = x.members()[0].second_enumeration()
    else {
        panic!("usable second")
    };
    assert_eq!(new_sources, &[new]);
    assert_eq!(issuer.create_instance(&s).unwrap().as_str(), "f1v1-i4");
    assert_eq!(issuer.instance_counter(&s).unwrap(), 4);
}

fn r98e_frames() -> Vec<ReaderFrame> {
    vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, listing(0, true, RawEnumeration::Closed(1), &["f1v1-i1"])),
        frame(3, record("f1v1-i1", 1, 1)),
        frame(4, end("f1v1-i1", 1)),
        frame(5, Payload::RunEnd),
    ]
}
#[test]
fn abf107n_r98e_f_h_standalone_identity_and_late_authority_witnesses() {
    let f = r98e_frames();
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        let m = &x.members()[0];
        assert_eq!(
            m.verdict(),
            CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
        );
        assert_eq!(m.delivery().initial_enum_identities, 1);
        assert_eq!(m.delivery().second_enum_identities, 1);
        assert_eq!(
            m.qualifiers(),
            vec![Qualifier::SecondEnumerationNoNewSources]
        );
        assert_eq!(m.records().len(), 1);
        assert!(x.dispositions().iter().all(|&d| d == Disposition::Admitted));
        let before = signature(&x);
        let mut log = PostFinalizationLog::bind(&x);
        log.append(frame(6, initial(&[])), SupervisorInstant::synthetic(6))
            .unwrap();
        let d = log.snapshot();
        assert_eq!(d.entries().len(), 1);
        assert_eq!(d.entries()[0].kind, FrameKind::InitialEnumeration);
        assert_eq!(d.entries()[0].relation, IdentityRelation::NewIdentity);
        assert_eq!(
            assess(m, &d).unwrap().ceiling,
            PresentationCeiling::WeakenedByLateDelivery
        );
        assert_eq!(signature(&x), before);
    }
    let f = [
        frame(1, initial(&[])),
        frame(2, initial(&["f1v1-i1"])),
        frame(3, record("f1v1-i1", 1, 1)),
        frame(4, end("f1v1-i1", 1)),
        frame(5, listing(0, true, RawEnumeration::Closed(1), &["f1v1-i1"])),
        frame(6, Payload::RunEnd),
    ];
    for order in permutations(f.len()) {
        let x = run(order.iter().map(|&i| f[i].clone()).collect());
        let m = &x.members()[0];
        assert_eq!(
            m.enumeration(),
            EnumerationOutcome::Failed(EnumFailure::AmbiguousInitial)
        );
        assert_eq!(m.delivery().initial_enum_identities, 2);
        assert_eq!(m.delivery().second_enum_identities, 1);
        assert_eq!(m.delivery().unexpected_source, 2);
        assert_eq!(
            m.second_enumeration(),
            &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
        );
        assert_eq!(m.qualifiers(), vec![Qualifier::SecondEnumerationUnusable]);
        assert!(m.sources().is_empty() && m.records().is_empty());
        assert_eq!(m.cause(), VerdictRule::MemberEnumerationAnomaly);
    }
}

#[test]
fn abf107n_r99e_i_qualified_second_listing_survives_a_late_second_identity() {
    let mut f = baseline();
    f.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    let x = run(f);
    let before = signature(&x);
    let mut log = PostFinalizationLog::bind(&x);
    log.append(
        frame(30, listing(0, true, RawEnumeration::Closed(0), &[])),
        SupervisorInstant::synthetic(30),
    )
    .unwrap();
    let m = &x.members()[0];
    assert_eq!(m.delivery().second_enum_identities, 1);
    assert_eq!(
        m.qualifiers(),
        vec![Qualifier::NewSourcesObserved {
            count: 1,
            basis: CountBasis::Exact
        }]
    );
    assert_eq!(
        m.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert_eq!(
        log.snapshot().entries()[0].kind,
        FrameKind::SecondEnumeration
    );
    assert_eq!(
        log.snapshot().entries()[0].relation,
        IdentityRelation::NewIdentity
    );
    assert_eq!(
        assess(m, &log.snapshot()).unwrap().post,
        PostFinalization::Observed {
            records: 0,
            other: 1,
            conflicts: 0,
            unattributed: 1
        }
    );
    assert_eq!(signature(&x), before);
}

#[test]
fn abf107n_n16_all_public_redacted_types_have_exact_fixed_debug() {
    let catalog = catalog();
    let builder = CatalogBuilder::new();
    let issuer = FixtureRefIssuer::new(&catalog);
    let source = SourceRef::new("f1v1-private_marker").unwrap();
    let attempt = AttemptRef::new("private_marker").unwrap();
    let finalization = FinalizationId::new("z19").unwrap();
    let supervisor = SupervisorInstant::synthetic(123);
    let reader = ReaderInstant::synthetic(456);
    let times = TimeObservations::new(123, 456);
    let bytes = br#"{"id":"private_marker","tokens":{"input":100}}"#.to_vec();
    let result = abf_parser::parse(&bytes, None, abf_parser::Limits::MAX);
    let record_event = SourceEvent::Record { bytes, result };
    let payload = event("f1v1-private_marker", 1, record_event.clone());
    let reader_frame = frame(2, payload.clone());
    let raw_log = raw(baseline());
    let manifest = raw_log.manifest.clone();
    let x = run(baseline());
    let m = &x.members()[0];
    let r = &m.records()[0];
    let index = r.delivery();
    let second = SecondEnumeration::Performed {
        listing: SecondListing::Closed { listed: 1 },
        new_sources: vec![source.clone()],
    };
    let mut log = PostFinalizationLog::bind(&x);
    log.append(reader_frame.clone(), supervisor).unwrap();
    let diagnostics = log.snapshot();
    let cases: [(&str, &dyn std::fmt::Debug); 25] = [
        ("CatalogScope", m.scope()),
        ("SourceRef", &source),
        ("AttemptRef", &attempt),
        ("FinalizationId", &finalization),
        ("CatalogSnapshot", &catalog),
        ("CatalogBuilder", &builder),
        ("FixtureRefIssuer", &issuer),
        ("SupervisorInstant", &supervisor),
        ("ReaderInstant", &reader),
        ("TimeObservations", &times),
        ("SourceEvent", &record_event),
        ("Payload", &payload),
        ("ReaderFrame", &reader_frame),
        ("ExecutionManifest", &manifest),
        ("RawExecutionLog", &raw_log),
        ("DeliveryIndex", &index),
        ("SourceReconciliation", &m.sources()[0].1),
        ("SecondEnumeration", &second),
        ("RetainedRecord", r),
        ("FinalizedMember", m),
        ("FinalizedExecution", &x),
        ("DeliveryEvidence", &x.delivery_evidence()[0]),
        ("DiagnosticEntry", &diagnostics.entries()[0]),
        ("DiagnosticsSnapshot", &diagnostics),
        ("PostFinalizationLog", &log),
    ];
    for (name, value) in cases {
        assert_eq!(format!("{value:?}"), format!("{name}([redacted])"));
    }
    // Public view values intentionally show only fixed codes/counts. They are not evidence constructors.
    assert_eq!(
        format!(
            "{:?}",
            CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
        ),
        "CoveredWithinEnumeration { expected: 2 }"
    );
    assert_eq!(
        format!("{:?}", FinalizeCode::SourceScopeMismatch),
        "SourceScopeMismatch"
    );
    // Explicit accessors are intentionally sensitive, rather than sanitizing the underlying claims.
    assert_eq!(source.as_str(), "f1v1-private_marker");
    let SourceEvent::Record { result, .. } = &record_event else {
        panic!("record")
    };
    assert_eq!(
        result.observation().unwrap().message_id,
        abf_parser::Field::Valid("private_marker".into())
    );
}

fn bounded_orders(n: usize) -> Vec<Vec<usize>> {
    let base: Vec<_> = (0..n).collect();
    let mut orders = vec![base.clone()];
    for shift in 0..n {
        let order: Vec<_> = (0..n).map(|i| (i + shift) % n).collect();
        orders.push(order.clone());
        orders.push(order.into_iter().rev().collect());
    }
    for a in 0..n {
        for b in a + 1..n {
            let mut order = base.clone();
            order.swap(a, b);
            orders.push(order);
        }
    }
    orders.sort();
    orders.dedup();
    orders
}
fn admission_counts(
    master: &[ReaderFrame],
    arrived: &[ReaderFrame],
    x: &FinalizedExecution,
) -> Vec<[usize; 5]> {
    master
        .iter()
        .enumerate()
        .filter(|(i, f)| !master[..*i].contains(f))
        .map(|(_, key)| {
            let mut counts = [0; 5];
            for (f, d) in arrived.iter().zip(x.dispositions()) {
                if f == key {
                    let bin = match d {
                        Disposition::Admitted => 0,
                        Disposition::Quarantined(QuarantineCode::ExactReplay) => 1,
                        Disposition::Quarantined(QuarantineCode::SequenceConflict) => 2,
                        Disposition::Quarantined(QuarantineCode::UnexpectedSource) => 3,
                        Disposition::Quarantined(QuarantineCode::DistributionMismatch) => 4,
                    };
                    counts[bin] += 1;
                }
            }
            counts
        })
        .collect()
}
fn verify_orders(f: Vec<ReaderFrame>, exhaustive: bool) {
    let original = run(f.clone());
    let expected = signature(&original);
    let counts = admission_counts(&f, &f, &original);
    let orders = if exhaustive {
        permutations(f.len())
    } else {
        bounded_orders(f.len())
    };
    for order in orders {
        let arrived: Vec<_> = order.iter().map(|&i| f[i].clone()).collect();
        let x = run(arrived.clone());
        assert_eq!(signature(&x), expected);
        assert_eq!(admission_counts(&f, &arrived, &x), counts);
        let again = run(arrived.clone());
        assert_eq!(signature(&again), expected);
        let mut r = raw(arrived);
        r.manifest.members.swap(0, 1);
        for f in &mut r.pre_barrier {
            if let Payload::Enumeration { member, .. } = &mut f.payload {
                *member = 1 - *member;
            }
        }
        let reordered = finalize(r, &catalog()).unwrap();
        assert_eq!(signature(&reordered), expected);
    }
}
#[test]
fn abf107n_r98g_exact_and_r99c_full_b2_peer_locality_with_declared_order_domains() {
    let f = vec![
        frame(1, initial(&[])),
        frame(2, initial(&["f1v1-i1"])),
        frame(3, record("f1v1-i1", 1, 1)),
        frame(4, end("f1v1-i1", 1)),
        frame(
            5,
            listing(1, false, RawEnumeration::Closed(1), &["f2v1-i1"]),
        ),
        frame(6, record("f2v1-i1", 1, 1)),
        frame(7, end("f2v1-i1", 1)),
        frame(8, Payload::RunEnd),
    ];
    let x = run(f.clone());
    assert_eq!(x.members()[0].delivery().unexpected_source, 2);
    assert!(x.members()[0].records().is_empty());
    assert_eq!(
        x.members()[1].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    verify_orders(f, true); // exactly 8! = 40,320, each repeated and with reversed member order.
    let mut f = baseline();
    f.pop();
    f.extend([
        frame(
            5,
            listing(1, false, RawEnumeration::Closed(1), &["f2v1-i1"]),
        ),
        frame(6, record("f2v1-i1", 1, 1)),
        frame(7, end("f2v1-i1", 1)),
        frame(
            8,
            listing(
                0,
                true,
                RawEnumeration::Closed(3),
                &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
            ),
        ),
        frame(
            9,
            listing(0, true, RawEnumeration::Closed(2), &["f1v1-i1", "f1v1-i2"]),
        ),
        frame(10, Payload::RunEnd),
    ]);
    let x = run(f.clone());
    let a = &x.members()[0];
    let b = &x.members()[1];
    assert_eq!(a.verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(a.cause(), VerdictRule::MemberEnumerationAnomaly);
    assert_eq!(
        a.second_enumeration(),
        &SecondEnumeration::Unusable(SecondUnusable::Ambiguous)
    );
    assert_eq!(a.delivery().second_enum_identities, 2);
    assert_eq!(a.records().len(), 1);
    assert_eq!(b.records().len(), 1);
    assert_eq!(
        b.verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert!(b.qualifiers().is_empty());
    assert!(
        x.members()
            .iter()
            .all(|m| m.delivery().global_seq_conflicts == 0)
    );
    assert_eq!(bounded_orders(10).len(), 65);
    verify_orders(f, false); // declared finite rotations/reversals/transpositions, not 10!.
}
#[test]
fn abf107n_n14_remaining_terminal_second_and_conflict_order_families() {
    for terminal in [
        SourceEvent::Skipped(StopReason::Cancelled),
        SourceEvent::OpenFailed(OpenFailure::PermissionDenied),
        SourceEvent::Vanished,
        SourceEvent::OpenAttempt,
        SourceEvent::OpenSuccess,
        SourceEvent::ReadError(PartialReason::Io),
    ] {
        verify_orders(
            vec![
                frame(1, initial(&["f1v1-i1"])),
                frame(2, record("f1v1-i1", 1, 1)),
                frame(3, event("f1v1-i1", 2, terminal)),
                frame(4, Payload::RunEnd),
            ],
            true,
        );
    }
    for second in [
        RawEnumeration::Closed(0),
        RawEnumeration::Partial(0, StopReason::Budget),
        RawEnumeration::Failed(EnumFailure::PermissionDenied),
    ] {
        let mut f = baseline();
        f.push(frame(6, listing(0, true, second, &[])));
        verify_orders(f, true);
    }
    let mut f = baseline();
    f.push(frame(6, listing(0, true, RawEnumeration::Closed(0), &[])));
    f.push(frame(6, listing(0, true, RawEnumeration::RootAbsent, &[])));
    verify_orders(f, false);
}

#[test]
fn abf107n_parser_retained_budget_at_minus_one_limit_and_plus_one() {
    for target in [4095, 4096, 4097] {
        let bytes = br#"{"tokens":{"input":100}}"#.to_vec();
        let parsed = abf_parser::parse(&bytes, None, abf_parser::Limits::MAX);
        let mut observation = parsed.observation().unwrap().clone();
        observation.message_id = abf_parser::Field::Valid(
            "x".repeat(target - std::mem::size_of::<abf_parser::ParseResult>())
                .into(),
        );
        let result = abf_parser::ParseResult::Observation(observation);
        assert_eq!(result.retained_bytes(), Some(target));
        let mut f = baseline();
        f[1].payload = event("f1v1-i1", 1, SourceEvent::Record { bytes, result });
        let actual = finalize(raw(f), &catalog());
        match target {
            4097 => assert_eq!(actual.unwrap_err(), FinalizeCode::InvalidParserResult),
            _ => assert_eq!(actual.unwrap().members()[0].records().len(), 1),
        }
    }
}

#[test]
fn abf107n_r99c_b_second_ambiguity_never_crosses_environment_executions() {
    let mut windows_frames = baseline();
    windows_frames.push(frame(
        6,
        listing(
            0,
            true,
            RawEnumeration::Closed(3),
            &["f1v1-i1", "f1v1-i2", "f1v1-i3"],
        ),
    ));
    windows_frames.push(frame(
        7,
        listing(0, true, RawEnumeration::Closed(2), &["f1v1-i1", "f1v1-i2"]),
    ));
    let mut w = raw(windows_frames);
    w.manifest.members.truncate(1);
    let windows = finalize(w, &catalog()).unwrap();
    let selected = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(selected.clone(), "f1v1").unwrap();
    let c = b.build();
    let mut l = raw(baseline());
    l.manifest.members = vec![selected];
    l.manifest.attempt = AttemptRef::new("e2").unwrap();
    for f in &mut l.pre_barrier {
        f.attempt = l.manifest.attempt.clone();
    }
    let wsl = finalize(l, &c).unwrap();
    assert_eq!(
        windows.members()[0].verdict(),
        CoverageVerdict::Inconsistent
    );
    assert_eq!(
        windows.members()[0].cause(),
        VerdictRule::MemberEnumerationAnomaly
    );
    assert_eq!(windows.members()[0].delivery().second_enum_identities, 2);
    assert_eq!(windows.members()[0].records().len(), 1);
    assert_eq!(
        wsl.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 2 }
    );
    assert!(wsl.members()[0].qualifiers().is_empty());
    assert_eq!(wsl.members()[0].delivery().second_enum_identities, 0);
    assert_eq!(wsl.members()[0].delivery().global_seq_conflicts, 0);
    assert_eq!(wsl.members()[0].records().len(), 1);
}

// V1.6 UA-7: the same B/C inputs that ABF107N rejected must finalize.
#[test]
fn r102b_c_v16_original_inputs_now_quarantine_without_promoting_sources() {
    for source in ["f2v1-i1", "outside-i1"] {
        let mut r = raw(vec![
            frame(1, initial(&[])),
            frame(2, record(source, 1, 1)),
            frame(3, Payload::RunEnd),
        ]);
        r.manifest.members.truncate(1);
        let x = finalize(r, &catalog()).expect("V1.6 requires a snapshot for valid B/C deliveries");
        assert_eq!(x.members().len(), 1);
        assert_eq!(
            x.dispositions(),
            &[
                Disposition::Admitted,
                Disposition::Quarantined(QuarantineCode::UnexpectedSource),
                Disposition::Admitted
            ]
        );
        let m = &x.members()[0];
        assert_eq!(m.enumeration(), EnumerationOutcome::Closed { listed: 0 });
        assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
        assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(m.delivery().unexpected_source, 0);
        assert!(m.sources().is_empty());
        assert!(m.records().is_empty());
    }
}

#[test]
fn r102f_k_n44_unbound_d2_and_mismatch_evidence_must_drop_private_identity() {
    for mismatch in [false, true] {
        let private = "privateua7-sentinel";
        for payload in [record(private, 1, 50), end(private, 2)] {
            let mut f = baseline();
            f.push(frame(3, payload));
            let mut r = raw(f);
            r.distribution_matches = !mismatch;
            let x = finalize(r, &catalog()).unwrap();
            let evidence = x.delivery_evidence().last().unwrap();
            assert!(
                evidence.source().is_none(),
                "unbound refs must not be retained, even for D2/mismatch"
            );
            assert!(evidence.source_seq().is_none());
            assert!(evidence.terminal().is_none());
            assert!(!format!("{x:?}").contains(private));
        }
    }
}

fn ua7_catalog() -> CatalogSnapshot {
    let mut b = CatalogBuilder::new();
    b.add(scope("f1"), "f1v1").unwrap();
    b.add(scope("f2"), "f2v1").unwrap();
    b.add(
        CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap(),
        "l1f1v1",
    )
    .unwrap();
    b.build()
}
fn ua7_base() -> Vec<ReaderFrame> {
    vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
    ]
}
// Independent G4-I24 oracle: inspect dispositions, then sum the two ownership
// classes in u64. This is never applied to private seeded arithmetic fixtures.
fn ua7_conservation(x: &FinalizedExecution, expected_local: u64, expected_unbound: u32) {
    let dispositions = x
        .dispositions()
        .iter()
        .filter(|d| **d == Disposition::Quarantined(QuarantineCode::UnexpectedSource))
        .count() as u64;
    let local: u64 = x
        .members()
        .iter()
        .map(|m| u64::from(m.delivery().unexpected_source))
        .sum();
    assert_eq!(local, expected_local);
    assert_eq!(x.unbound_unexpected_source_deliveries(), expected_unbound);
    assert_eq!(dispositions, expected_local + u64::from(expected_unbound));
    assert_eq!(
        dispositions,
        local + u64::from(x.unbound_unexpected_source_deliveries())
    );
}
fn ua7_clean_counters(m: &FinalizedMember) {
    assert_eq!(m.delivery().unexpected_source, 0);
    assert_eq!(m.delivery().seq_conflicts, 0);
    assert_eq!(m.delivery().global_seq_conflicts, 0);
    assert_eq!(m.delivery().global_replays, 0);
    assert_eq!(m.delivery().replays_quarantined, 0);
    assert_eq!(m.delivery().unattributable_gaps, 0);
    assert!(m.qualifiers().is_empty());
}
fn ua7_retained_baseline(x: &FinalizedExecution) {
    let baseline = run({
        let mut f = ua7_base();
        f.push(frame(4, Payload::RunEnd));
        f
    });
    let m = x
        .members()
        .iter()
        .find(|m| m.scope() == &scope("f1"))
        .unwrap();
    assert_eq!(m.records(), baseline.members()[0].records());
    assert_eq!(m.records().len(), 1);
    assert_eq!(m.sources()[0].0.as_str(), "f1v1-i1");
    assert_eq!(m.sources()[0].1.matched(), &[10]);
    assert_eq!(m.sources()[0].1.status(), SourceRunStatus::Complete);
    assert_eq!(m.enumeration(), EnumerationOutcome::Closed { listed: 1 });
    assert_eq!(m.process(), ProcessEnd::ExitedNormally);
    assert!(m.run_end_frame());
}
#[test]
fn r102a_member_owned_unexpected_source_remains_local() {
    let x = run(vec![
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, record("f1v1-i9", 1, 50)),
        frame(4, end("f1v1-i1", 1)),
        frame(5, Payload::RunEnd),
    ]);
    ua7_conservation(&x, 1, 0);
    assert_eq!(x.members()[0].cause(), VerdictRule::LocalAnomaly);
    assert_eq!(x.members()[1].verdict(), CoverageVerdict::NoAttempt);
    assert_eq!(x.members()[1].delivery().unexpected_source, 0);
    assert!(x.members()[1].sources().is_empty());
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(x.members()[0].sources()[0].1.matched(), &[10]);
    assert_eq!(
        x.dispositions()[2],
        Disposition::Quarantined(QuarantineCode::UnexpectedSource)
    );
}
#[test]
fn r102b_c_known_unknown_max_length_and_mixed_unbound_are_identical_fixed_facts() {
    let longest = format!("abcdefghij012345-{}", "x".repeat(47));
    assert_eq!(longest.len(), 64);
    for refs in [
        vec!["f2v1-i1"],
        vec!["l1f1v1-i1"],
        vec!["unk9-i1"],
        vec![longest.as_str()],
        vec!["f2v1-i1", "unk9-i1"],
    ] {
        let mut f = vec![frame(1, initial(&[]))];
        for (i, s) in refs.iter().enumerate() {
            f.push(frame(i as u64 + 2, record(s, 1, 50)));
        }
        f.push(frame(refs.len() as u64 + 2, Payload::RunEnd));
        let mut r = raw(f);
        r.manifest.members.truncate(1);
        let x = finalize(r, &ua7_catalog()).unwrap();
        ua7_conservation(&x, 0, refs.len() as u32);
        assert_eq!(x.members().len(), 1);
        let m = &x.members()[0];
        ua7_clean_counters(m);
        assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
        assert_eq!(m.enumeration(), EnumerationOutcome::Closed { listed: 0 });
        assert!(m.sources().is_empty());
        assert!(m.records().is_empty());
        for (i, e) in x
            .delivery_evidence()
            .iter()
            .enumerate()
            .skip(1)
            .take(refs.len())
        {
            assert_eq!(e.frame_seq(), i as u32 + 1);
            assert!(e.source().is_none());
            assert!(e.source_seq().is_none());
            assert!(e.terminal().is_none());
            assert_eq!(
                x.dispositions()[i],
                Disposition::Quarantined(QuarantineCode::UnexpectedSource)
            );
        }
    }
}
#[test]
fn r102d_invalid_source_representation_precedes_d2_and_distribution_abort() {
    let over_tag = "abcdefghij0123456-i1".to_string();
    let over_instance = format!("unk9-{}", "x".repeat(48));
    for invalid in [
        "unk9",
        "a-b-c",
        "unk9-",
        "unk9-i0",
        &over_tag,
        &over_instance,
    ] {
        for (d2, mismatch) in [(false, false), (true, false), (false, true)] {
            let mut f = ua7_base();
            f.push(frame(4, record(invalid, 1, 50)));
            if d2 {
                f.push(frame(4, record("unk9-i1", 1, 51)));
            }
            f.push(frame(5, Payload::RunEnd));
            let mut r = raw(f);
            r.distribution_matches = !mismatch;
            assert_eq!(
                finalize(r, &ua7_catalog()).unwrap_err(),
                FinalizeCode::InvalidSourceRef
            );
        }
    }
}
#[test]
fn r102e_wrong_scope_initial_and_second_listings_remain_errors() {
    for (second, source, extra) in [
        (false, "f2v1-i1", false),
        (true, "l1f1v1-i1", false),
        (false, "unk9-i1", false),
        (false, "f2v1-i1", true),
    ] {
        let mut f = Vec::new();
        if second {
            f.push(frame(1, initial(&["f1v1-i1"])));
        }
        f.push(frame(
            f.len() as u64 + 1,
            listing(0, second, RawEnumeration::Closed(1), &[source]),
        ));
        if extra {
            f.push(frame(2, record("unk9-i1", 1, 50)));
        }
        let mut r = raw(f);
        r.manifest.members.truncate(1);
        assert_eq!(
            finalize(r, &ua7_catalog()).unwrap_err(),
            FinalizeCode::SourceScopeMismatch
        );
    }
}
#[test]
fn r102f_d2_never_counts_unbound_variants_and_keeps_expected_conflict_marks() {
    for (a, b, extra, marked) in [
        ("f1v1-i1", "unk9-i1", false, true),
        ("l1f1v1-i1", "unk9-i1", false, false),
        ("unk9-i1", "unk9-i1", false, false),
        ("unk9-i1", "unk9-i1", true, false),
    ] {
        let mut f = ua7_base();
        f.push(frame(4, record(a, 2, 7)));
        f.push(frame(4, record(b, 1, 50)));
        if extra {
            f.push(frame(5, record("unk9-i1", 1, 50)));
        }
        f.push(frame(if extra { 6 } else { 5 }, Payload::RunEnd));
        let x = finalize(raw(f), &ua7_catalog()).unwrap();
        ua7_conservation(&x, 0, u32::from(extra));
        for m in x.members() {
            assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            assert_eq!(m.delivery().global_seq_conflicts, 1);
            assert_eq!(m.delivery().seq_conflicts, 0);
            assert_eq!(m.delivery().unexpected_source, 0);
        }
        assert_eq!(
            &x.dispositions()[3..5],
            &[Disposition::Quarantined(QuarantineCode::SequenceConflict); 2]
        );
        assert_eq!(x.members()[0].records().len(), 1);
        assert_eq!(
            x.members()[0].records()[0].parse_result(),
            run({
                let mut f = ua7_base();
                f.push(frame(4, Payload::RunEnd));
                f
            })
            .members()[0]
                .records()[0]
                .parse_result()
        );
        assert_eq!(x.members()[0].sources()[0].1.conflict_marked(), marked);
        assert_eq!(
            x.members()[0].sources()[0].1.matched(),
            if marked { &[4, 10][..] } else { &[10][..] }
        );
        for e in x.delivery_evidence().iter().skip(3).take(2) {
            if e.frame_seq() == 4 && e.source().is_some() {
                assert_eq!(e.source().unwrap().as_str(), "f1v1-i1");
            }
        }
    }
}
#[test]
fn r102g_exact_unbound_replays_count_every_instance_expected_replay_stays_pq() {
    for (source, n) in [("unk9-i1", 2), ("unk9-i1", 3), ("f2v1-i1", 2)] {
        let mut f = ua7_base();
        for _ in 0..n {
            f.push(frame(4, record(source, 1, 50)));
        }
        f.push(frame(5, Payload::RunEnd));
        let mut r = raw(f);
        r.manifest.members.truncate(1);
        let x = finalize(r, &ua7_catalog()).unwrap();
        ua7_conservation(&x, 0, n);
        ua7_retained_baseline(&x);
        for m in x.members() {
            ua7_clean_counters(m);
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
        }
        assert_eq!(
            &x.dispositions()[3..3 + n as usize],
            vec![Disposition::Quarantined(QuarantineCode::UnexpectedSource); n as usize]
        );
    }
    let mut f = ua7_base();
    f.push(f[1].clone());
    f.push(frame(4, Payload::RunEnd));
    let x = run(f);
    ua7_conservation(&x, 0, 0);
    assert_eq!(
        x.dispositions()[3],
        Disposition::Quarantined(QuarantineCode::ExactReplay)
    );
    assert_eq!(x.members()[0].delivery().replays_quarantined, 1);
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(
        x.members()[0].qualifiers(),
        vec![Qualifier::TransportReplayQuarantined]
    );
}
#[test]
fn r102h_distinct_identities_and_unbound_controls_are_delivery_instances() {
    for (payloads, local, unbound) in [
        (
            vec![
                record("unk9-i1", 1, 50),
                record("unk9-i1", 1, 50),
                end("unk9-i1", 2),
            ],
            0,
            3,
        ),
        (
            vec![
                record("f2v1-i1", 1, 50),
                record("unk9-i1", 1, 50),
                record("l1f1v1-i1", 1, 50),
            ],
            0,
            3,
        ),
        (
            vec![record("f1v1-i9", 1, 50), record("unk9-i1", 1, 50)],
            1,
            1,
        ),
    ] {
        let mut f = ua7_base();
        for (i, p) in payloads.into_iter().enumerate() {
            f.push(frame(i as u64 + 4, p));
        }
        f.push(frame(f.len() as u64 + 1, Payload::RunEnd));
        let mut r = raw(f);
        r.manifest.members.truncate(1);
        let x = finalize(r, &ua7_catalog()).unwrap();
        ua7_conservation(&x, local, unbound);
        ua7_retained_baseline(&x);
        assert_eq!(x.members()[0].cause(), VerdictRule::ExecutionGlobalAnomaly);
    }
}
#[test]
fn r102i_j_global_rule_1a_precedes_noattempt_availability_inventory_and_rule_1b() {
    for second in [
        None,
        Some(RawEnumeration::ScopeUnavailable(
            UnavailableReason::Unavailable,
        )),
        Some(RawEnumeration::RootUnresolved),
    ] {
        let mut f = vec![frame(1, initial(&["f1v1-i1"]))];
        if let Some(e) = second {
            f.push(frame(2, listing(1, false, e, &[])));
        }
        let seq = f.len() as u64 + 1;
        f.push(frame(seq, record("f1v1-i1", 1, 1)));
        f.push(frame(seq + 1, end("f1v1-i1", 1)));
        f.push(frame(seq + 2, record("unk9-i1", 1, 50)));
        f.push(frame(seq + 3, Payload::RunEnd));
        let x = run(f);
        ua7_conservation(&x, 0, 1);
        for m in x.members() {
            assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
            ua7_clean_counters(m);
        }
        assert_eq!(
            x.members()[1].enumeration(),
            match second {
                None => EnumerationOutcome::NotAttempted,
                Some(RawEnumeration::RootUnresolved) => EnumerationOutcome::RootUnresolved,
                _ => EnumerationOutcome::ScopeUnavailable(UnavailableReason::Unavailable),
            }
        );
        assert!(x.members()[1].sources().is_empty());
        assert!(x.members()[1].records().is_empty());
        assert_eq!(x.members()[0].records().len(), 1);
        assert_eq!(x.members()[0].sources()[0].1.matched(), &[10]);
    }
    let x = run(vec![
        frame(1, initial(&[])),
        frame(2, initial(&[])),
        frame(3, record("unk9-i1", 1, 50)),
        frame(4, Payload::RunEnd),
    ]);
    ua7_conservation(&x, 0, 1);
    assert_eq!(x.members()[0].delivery().initial_enum_identities, 2);
    assert_eq!(x.members()[0].cause(), VerdictRule::ExecutionGlobalAnomaly);
    let untouched = run({
        let mut f = ua7_base();
        f.push(frame(4, Payload::RunEnd));
        f
    });
    assert_eq!(untouched.unbound_unexpected_source_deliveries(), 0);
    assert_eq!(
        untouched.members()[0].verdict(),
        CoverageVerdict::CoveredWithinEnumeration { expected: 1 }
    );
    assert_eq!(untouched.members()[1].verdict(), CoverageVerdict::NoAttempt);
}

#[test]
fn r102k_distribution_mismatch_and_late_unbound_are_layer_separated() {
    let wsl = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    let mut r = raw(vec![
        frame(1, initial(&[])),
        frame(2, record("unk9-i1", 1, 50)),
        frame(3, record("f2v1-i1", 1, 50)),
        frame(4, Payload::RunEnd),
    ]);
    r.manifest.members = vec![wsl];
    r.distribution_matches = false;
    let x = finalize(r, &ua7_catalog()).unwrap();
    ua7_conservation(&x, 0, 0);
    assert_eq!(
        x.dispositions(),
        &[Disposition::Quarantined(QuarantineCode::DistributionMismatch); 4]
    );
    assert_eq!(
        x.members()[0].verdict(),
        CoverageVerdict::ScopeUnavailable(UnavailableReason::DistributionMismatch)
    );
    ua7_clean_counters(&x.members()[0]);
    assert!(x.members()[0].records().is_empty());
    for preexisting in [false, true] {
        let mut f = ua7_base();
        if preexisting {
            f.push(frame(4, record("unk9-i1", 1, 50)));
        }
        f.push(frame(if preexisting { 5 } else { 4 }, Payload::RunEnd));
        let x = run(f);
        let before = (
            x.dispositions().to_vec(),
            x.members()[0].records().to_vec(),
            x.members()[0].verdict(),
            x.members()[0].delivery(),
        );
        let mut log = PostFinalizationLog::bind(&x);
        for (i, s) in ["unk9-i1", "l1f1v1-i1"].iter().enumerate() {
            log.append(
                frame(6 + i as u64, record(s, 1, 50)),
                SupervisorInstant::synthetic(10),
            )
            .unwrap();
            let partial = log.snapshot();
            assert_eq!(partial.entries().len(), i + 1);
            assert_eq!(partial.entries()[i].attribution, Attribution::Unattributed);
            assert_eq!(
                x.unbound_unexpected_source_deliveries(),
                u32::from(preexisting)
            );
            assert_eq!(x.members()[0].verdict(), before.2);
            assert_eq!(x.members()[0].records(), before.1.as_slice());
        }
        let d = log.snapshot();
        assert_eq!(d.entries().len(), 2);
        for e in d.entries() {
            assert_eq!(e.attribution, Attribution::Unattributed);
            assert_eq!(e.relation, IdentityRelation::NewIdentity);
            assert_eq!(e.kind, FrameKind::Record);
            assert_eq!(e.outcome, Some(OutcomeClass::Accepted));
        }
        assert_eq!(
            assess(&x.members()[0], &d).unwrap().ceiling,
            PresentationCeiling::WeakenedByLateDelivery
        );
        assert_eq!(
            x.unbound_unexpected_source_deliveries(),
            u32::from(preexisting)
        );
        assert_eq!(
            (
                x.dispositions().to_vec(),
                x.members()[0].records().to_vec(),
                x.members()[0].verdict(),
                x.members()[0].delivery()
            ),
            before
        );
        assert!(!format!("{log:?}{d:?}").contains("unk9"));
    }
}
#[test]
fn r102l_all_80640_arrival_and_member_orders_have_literal_dispositions_and_conservation() {
    let deliveries = [
        frame(1, initial(&["f1v1-i1"])),
        frame(2, record("f1v1-i1", 1, 1)),
        frame(3, end("f1v1-i1", 1)),
        frame(4, record("unk9-i1", 1, 50)),
        frame(4, record("unk9-i1", 1, 50)),
        frame(5, record("l1f1v1-i1", 1, 20)),
        frame(6, record("unk9-i1", 2, 30)),
        frame(7, Payload::RunEnd),
    ];
    let orders = permutations(8);
    assert_eq!(orders.len(), 40320);
    let mut executed = 0;
    for reverse in [false, true] {
        for order in &orders {
            let mut f: Vec<_> = order.iter().map(|&i| deliveries[i].clone()).collect();
            let mut r = raw(Vec::new());
            if reverse {
                r.manifest.members.reverse();
                for d in &mut f {
                    if let Payload::Enumeration { member, .. } = &mut d.payload {
                        *member = 1;
                    }
                }
            }
            r.pre_barrier = f;
            let x = finalize(r, &ua7_catalog()).unwrap();
            ua7_conservation(&x, 0, 4);
            for (arrival, &original) in order.iter().enumerate() {
                let expected = match original {
                    0 | 1 | 2 | 7 => Disposition::Admitted,
                    _ => Disposition::Quarantined(QuarantineCode::UnexpectedSource),
                };
                assert_eq!(x.dispositions()[arrival], expected);
                assert_eq!(
                    x.delivery_evidence()[arrival].frame_seq(),
                    deliveries[original].frame_seq as u32
                );
            }
            for m in x.members() {
                ua7_clean_counters(m);
                assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
                assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            }
            let m = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f1"))
                .unwrap();
            assert_eq!(m.records().len(), 1);
            assert_eq!(m.records()[0].source().as_str(), "f1v1-i1");
            assert_eq!(m.records()[0].source_seq(), 1);
            assert_eq!(m.records()[0].frame_seq(), 2);
            assert_eq!(
                m.records()[0].parse_result(),
                run({
                    let mut f = ua7_base();
                    f.push(frame(4, Payload::RunEnd));
                    f
                })
                .members()[0]
                    .records()[0]
                    .parse_result()
            );
            assert_eq!(m.sources()[0].1.matched(), &[10]);
            assert_eq!(m.sources()[0].1.status(), SourceRunStatus::Complete);
            let peer = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f2"))
                .unwrap();
            assert_eq!(peer.enumeration(), EnumerationOutcome::NotAttempted);
            assert!(peer.records().is_empty());
            assert!(peer.sources().is_empty());
            executed += 1;
        }
    }
    assert_eq!(executed, 80640);
}
#[test]
fn r102l_mixed_a_b_c_bounded_rotations_and_both_member_orders_conserve() {
    // Eight cyclic arrival rotations, not the exhaustive 8! domain above.
    let mut f = ua7_base();
    f.extend([
        frame(4, record("f1v1-i9", 1, 50)),
        frame(5, record("l1f1v1-i1", 1, 20)),
        frame(6, record("unk9-i1", 1, 30)),
        frame(7, end("unk9-i1", 1)),
        frame(8, Payload::RunEnd),
    ]);
    for reverse in [false, true] {
        for rotation in 0..8 {
            let mut r = raw(f.clone());
            r.pre_barrier.rotate_left(rotation);
            if reverse {
                r.manifest.members.reverse();
                for d in &mut r.pre_barrier {
                    if let Payload::Enumeration { member, .. } = &mut d.payload {
                        *member = 1;
                    }
                }
            }
            let x = finalize(r, &ua7_catalog()).unwrap();
            ua7_conservation(&x, 1, 3);
            for (d, e) in x.dispositions().iter().zip(x.delivery_evidence()) {
                assert_eq!(
                    *d,
                    match e.frame_seq() {
                        4..=7 => Disposition::Quarantined(QuarantineCode::UnexpectedSource),
                        _ => Disposition::Admitted,
                    }
                );
            }
            let m = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f1"))
                .unwrap();
            assert_eq!(m.delivery().unexpected_source, 1);
            assert_eq!(m.records().len(), 1);
            let peer = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f2"))
                .unwrap();
            assert_eq!(peer.delivery().unexpected_source, 0);
            for m in x.members() {
                assert_eq!(m.cause(), VerdictRule::ExecutionGlobalAnomaly);
            }
        }
    }
}
#[test]
fn r102m_unbound_delivery_capacity_minus_one_at_and_plus_one() {
    for n in [MAX_FRAMES - 1, MAX_FRAMES, MAX_FRAMES + 1] {
        let f = (1..=n)
            .map(|i| frame(i as u64, record("unk9-i1", 1, 50)))
            .collect();
        let result = finalize(raw(f), &catalog());
        if n > MAX_FRAMES {
            assert_eq!(result.unwrap_err(), FinalizeCode::CapacityExceeded);
        } else {
            let x = result.unwrap();
            ua7_conservation(&x, 0, n as u32);
            assert_eq!(x.dispositions().len(), n);
            assert!(
                x.members()
                    .iter()
                    .all(|m| m.records().is_empty() && m.sources().is_empty())
            );
        }
    }
}
#[test]
fn n44_privacy_accessible_retention_and_late_replay_keys_keep_only_legitimate_sources() {
    for private in ["unksecret-sentinel", "l1f1v1-sentinel"] {
        for mode in 0..4 {
            let bytes = br#"{"message_id":"private_ua7_payload","tokens":{"input":50}}"#.to_vec();
            let payload = Payload::Source {
                source: private.into(),
                source_seq: 1,
                event: SourceEvent::Record {
                    result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
                    bytes,
                },
            };
            let mut f = ua7_base();
            f.push(frame(4, payload.clone()));
            if mode == 1 {
                f.push(frame(4, record("f1v1-i1", 2, 7)));
            }
            if mode == 2 {
                f.push(frame(4, record(private, 1, 51)));
            }
            f.push(frame(5, Payload::RunEnd));
            let mut r = raw(f);
            r.distribution_matches = mode != 3;
            let x = finalize(r, &ua7_catalog()).unwrap();
            for (i, e) in x.delivery_evidence().iter().enumerate() {
                assert_eq!(
                    e.frame_seq(),
                    if i < 3 {
                        i as u32 + 1
                    } else if i == x.dispositions().len() - 1 {
                        5
                    } else {
                        4
                    }
                );
                if i == 3 || (mode == 2 && i == 4) {
                    assert!(e.source().is_none());
                    assert!(e.source_seq().is_none());
                    assert!(e.terminal().is_none());
                    assert!(e.enumeration().is_none());
                    assert!(e.enumerated_at().is_none());
                }
                if let Some(s) = e.source() {
                    assert!(!s.as_str().contains(private));
                    assert!(!s.as_str().contains("sentinel"));
                }
            }
            for m in x.members() {
                for (s, _) in m.sources() {
                    assert_eq!(s.as_str(), "f1v1-i1");
                }
                for r in m.records() {
                    assert_eq!(r.source().as_str(), "f1v1-i1");
                    assert_eq!(
                        r.parse_result(),
                        run({
                            let mut f = ua7_base();
                            f.push(frame(4, Payload::RunEnd));
                            f
                        })
                        .members()[0]
                            .records()[0]
                            .parse_result()
                    );
                }
                if let SecondEnumeration::Performed { new_sources, .. } = m.second_enumeration() {
                    assert!(new_sources.is_empty());
                }
            }
            let debug = format!(
                "{x:?}{:?}{:?}{:?}",
                x.delivery_evidence(),
                x.members(),
                x.dispositions()
            );
            for secret in [private, "unksecret", "sentinel", "private_ua7_payload"] {
                assert!(!debug.contains(secret));
            }
            let mut log = PostFinalizationLog::bind(&x);
            log.append(frame(4, payload), SupervisorInstant::synthetic(20))
                .unwrap();
            log.append(
                frame(2, record("f1v1-i1", 1, 1)),
                SupervisorInstant::synthetic(21),
            )
            .unwrap();
            let d = log.snapshot();
            assert_eq!(d.entries()[0].relation, IdentityRelation::NewIdentity);
            assert_eq!(d.entries()[0].attribution, Attribution::Unattributed);
            assert_eq!(d.entries()[0].kind, FrameKind::Record);
            assert_eq!(d.entries()[0].outcome, Some(OutcomeClass::Accepted));
            assert_eq!(
                d.entries()[1].relation,
                if mode == 3 {
                    IdentityRelation::NewIdentity
                } else {
                    IdentityRelation::ReplayOfAdmittedIdentity
                }
            );
            assert!(!format!("{log:?}{d:?}{:?}", d.entries()).contains(private));
        }
    }
}

#[test]
fn r102i_unbound_anomaly_preserves_every_legitimate_member_evidence_field() {
    let mut f = ua7_base();
    f.push(frame(4, Payload::RunEnd));
    let before = run(f);
    let mut f = ua7_base();
    f.push(frame(4, record("unk9-i1", 1, 50)));
    f.push(frame(5, Payload::RunEnd));
    let after = run(f);
    ua7_conservation(&after, 0, 1);
    for (a, b) in before.members().iter().zip(after.members()) {
        assert_eq!(a.scope(), b.scope());
        assert_eq!(a.enumeration(), b.enumeration());
        assert_eq!(a.sources(), b.sources());
        assert_eq!(a.records(), b.records());
        assert_eq!(a.delivery(), b.delivery());
        assert_eq!(a.second_enumeration(), b.second_enumeration());
        assert_eq!(a.process(), b.process());
        assert_eq!(a.run_end_frame(), b.run_end_frame());
        assert_eq!(a.identity(), b.identity());
        assert_eq!(a.lineage(), b.lineage());
        assert_eq!(a.attestation(), b.attestation());
        assert_eq!(a.qualifiers(), b.qualifiers());
        assert_eq!(b.verdict(), CoverageVerdict::Inconsistent);
        assert_eq!(b.cause(), VerdictRule::ExecutionGlobalAnomaly);
    }
    // A separate selected WSL execution is outside the Windows evidence owner.
    let mut r = raw(vec![frame(1, initial(&[])), frame(2, Payload::RunEnd)]);
    r.manifest.members =
        vec![CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap()];
    let unrelated = finalize(r, &ua7_catalog()).unwrap();
    assert_eq!(unrelated.unbound_unexpected_source_deliveries(), 0);
    assert_eq!(
        unrelated.members()[0].verdict(),
        CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration)
    );
}

// V1.7 ABF107R-F01: an empty expected set does not erase member-owned
// pre-barrier unexpected delivery evidence. These are literal P-Q outcomes.
#[test]
fn r103a_no_initial_member_owned_record_is_local_anomaly() {
    let mut r = raw(vec![
        frame(1, record("f1v1-i9", 1, 1)),
        frame(2, Payload::RunEnd),
    ]);
    r.manifest.members.truncate(1);
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(
        x.dispositions(),
        &[
            Disposition::Quarantined(QuarantineCode::UnexpectedSource),
            Disposition::Admitted,
        ]
    );
    assert_eq!(x.unbound_unexpected_source_deliveries(), 0);
    let m = &x.members()[0];
    assert_eq!(m.delivery().unexpected_source, 1);
    assert_eq!(m.delivery().global_seq_conflicts, 0);
    assert_eq!(m.delivery().unattributable_gaps, 0);
    assert_eq!(m.delivery().global_replays, 0);
    assert_eq!(m.enumeration(), EnumerationOutcome::NotAttempted);
    assert!(m.sources().is_empty());
    assert_eq!(m.records().len(), 0);
    assert_eq!(m.verdict(), CoverageVerdict::Inconsistent);
    assert_eq!(m.cause(), VerdictRule::LocalAnomaly);
    ua7_conservation(&x, 1, 0);
}

fn f01_member(
    m: &FinalizedMember,
    enumeration: EnumerationOutcome,
    unexpected: u32,
    verdict: CoverageVerdict,
    cause: VerdictRule,
) {
    assert_eq!(m.enumeration(), enumeration);
    assert_eq!(m.delivery().unexpected_source, unexpected);
    assert!(m.sources().is_empty());
    assert_eq!(m.records().len(), 0);
    assert_eq!(m.verdict(), verdict);
    assert_eq!(m.cause(), cause);
}
fn f01_no_global(x: &FinalizedExecution) {
    assert_eq!(x.unbound_unexpected_source_deliveries(), 0);
    for m in x.members() {
        assert_eq!(m.delivery().global_seq_conflicts, 0);
        assert_eq!(m.delivery().unattributable_gaps, 0);
        assert_eq!(m.delivery().global_replays, 0);
        assert_eq!(m.delivery().seq_conflicts, 0);
        assert_eq!(m.delivery().replays_quarantined, 0);
    }
}
#[test]
fn r103b_local_record_preserves_peer_for_all_two_frame_orders_and_both_member_orders() {
    let frames = [frame(1, record("f1v1-i9", 1, 1)), frame(2, Payload::RunEnd)];
    let mut checked = 0;
    for reverse_members in [false, true] {
        for order in permutations(2) {
            let mut r = raw(order.iter().map(|&i| frames[i].clone()).collect());
            if reverse_members {
                r.manifest.members.reverse();
            }
            let x = finalize(r, &catalog()).unwrap();
            assert_eq!(x.members().len(), 2);
            for (arrival, &original) in order.iter().enumerate() {
                assert_eq!(
                    x.dispositions()[arrival],
                    [
                        Disposition::Quarantined(QuarantineCode::UnexpectedSource),
                        Disposition::Admitted,
                    ][original]
                );
            }
            let owner = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f1"))
                .unwrap();
            let peer = x
                .members()
                .iter()
                .find(|m| m.scope() == &scope("f2"))
                .unwrap();
            f01_member(
                owner,
                EnumerationOutcome::NotAttempted,
                1,
                CoverageVerdict::Inconsistent,
                VerdictRule::LocalAnomaly,
            );
            f01_member(
                peer,
                EnumerationOutcome::NotAttempted,
                0,
                CoverageVerdict::NoAttempt,
                VerdictRule::NoAttempt,
            );
            f01_no_global(&x);
            ua7_conservation(&x, 1, 0);
            checked += 1;
        }
    }
    assert_eq!(checked, 4); // Exhaustive only for this two-delivery family.
}
#[test]
fn r103c_no_source_with_zero_or_one_second_identity_remains_noattempt() {
    for second in [false, true] {
        let frames = if second {
            vec![
                frame(1, listing(0, true, RawEnumeration::Closed(0), &[])),
                frame(2, Payload::RunEnd),
            ]
        } else {
            vec![frame(1, Payload::RunEnd)]
        };
        let x = run(frames);
        assert!(x.dispositions().iter().all(|d| *d == Disposition::Admitted));
        for m in x.members() {
            f01_member(
                m,
                EnumerationOutcome::NotAttempted,
                0,
                CoverageVerdict::NoAttempt,
                VerdictRule::NoAttempt,
            );
        }
        assert_eq!(
            x.members()[0].delivery().second_enum_identities,
            u32::from(second)
        );
        assert_eq!(
            x.members()[0].second_enumeration(),
            if second {
                &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
            } else {
                &SecondEnumeration::NotPerformed
            }
        );
        f01_no_global(&x);
        ua7_conservation(&x, 0, 0);
    }
    let mut r = raw(vec![]);
    r.barrier = Barrier::AbandonedWithoutReader;
    let x = finalize(r, &catalog()).unwrap();
    assert!(x.dispositions().is_empty());
    for m in x.members() {
        f01_member(
            m,
            EnumerationOutcome::NotAttempted,
            0,
            CoverageVerdict::NoAttempt,
            VerdictRule::NoAttempt,
        );
    }
}
#[test]
fn r103d_initial_closed_zero_keeps_existing_local_anomaly() {
    let x = run(vec![
        frame(1, initial(&[])),
        frame(2, record("f1v1-i9", 1, 1)),
        frame(3, Payload::RunEnd),
    ]);
    assert_eq!(
        x.dispositions(),
        &[
            Disposition::Admitted,
            Disposition::Quarantined(QuarantineCode::UnexpectedSource),
            Disposition::Admitted,
        ]
    );
    f01_member(
        &x.members()[0],
        EnumerationOutcome::Closed { listed: 0 },
        1,
        CoverageVerdict::Inconsistent,
        VerdictRule::LocalAnomaly,
    );
    f01_member(
        &x.members()[1],
        EnumerationOutcome::NotAttempted,
        0,
        CoverageVerdict::NoAttempt,
        VerdictRule::NoAttempt,
    );
    f01_no_global(&x);
    ua7_conservation(&x, 1, 0);
}
#[test]
fn r103e_no_initial_unbound_b_c_still_trigger_global_rule_1a() {
    for source in ["l1f1v1-i1", "unk9-i1"] {
        let x = finalize(
            raw(vec![
                frame(1, record(source, 1, 1)),
                frame(2, Payload::RunEnd),
            ]),
            &ua7_catalog(),
        )
        .unwrap();
        assert_eq!(
            x.dispositions(),
            &[
                Disposition::Quarantined(QuarantineCode::UnexpectedSource),
                Disposition::Admitted,
            ]
        );
        assert_eq!(x.unbound_unexpected_source_deliveries(), 1);
        for m in x.members() {
            f01_member(
                m,
                EnumerationOutcome::NotAttempted,
                0,
                CoverageVerdict::Inconsistent,
                VerdictRule::ExecutionGlobalAnomaly,
            );
            assert_eq!(m.delivery().global_seq_conflicts, 0);
            assert_eq!(m.delivery().unattributable_gaps, 0);
        }
        ua7_conservation(&x, 0, 1);
    }
}
#[test]
fn r103f_no_initial_member_owned_nonrecord_controls_are_local_anomalies() {
    for payload in [
        event("f1v1-i9", 1, SourceEvent::OpenAttempt),
        end("f1v1-i9", 0),
    ] {
        let frames = [frame(1, payload), frame(2, Payload::RunEnd)];
        for order in permutations(2) {
            let x = run(order.iter().map(|&i| frames[i].clone()).collect());
            let source_index = order.iter().position(|i| *i == 0).unwrap();
            assert_eq!(
                x.dispositions()[source_index],
                Disposition::Quarantined(QuarantineCode::UnexpectedSource)
            );
            assert_eq!(x.dispositions()[1 - source_index], Disposition::Admitted);
            f01_member(
                &x.members()[0],
                EnumerationOutcome::NotAttempted,
                1,
                CoverageVerdict::Inconsistent,
                VerdictRule::LocalAnomaly,
            );
            f01_member(
                &x.members()[1],
                EnumerationOutcome::NotAttempted,
                0,
                CoverageVerdict::NoAttempt,
                VerdictRule::NoAttempt,
            );
            f01_no_global(&x);
            ua7_conservation(&x, 1, 0);
        }
    }
}
#[test]
fn r103g_late_member_owned_unexpected_delivery_cannot_change_original_noattempt() {
    let x = run(vec![frame(1, Payload::RunEnd)]);
    let mut log = PostFinalizationLog::bind(&x);
    log.append(
        frame(2, record("f1v1-i9", 1, 1)),
        SupervisorInstant::synthetic(10),
    )
    .unwrap();
    let d = log.snapshot();
    assert_eq!(d.entries().len(), 1);
    assert_eq!(d.entries()[0].kind, FrameKind::Record);
    assert_eq!(d.entries()[0].attribution, Attribution::Unattributed); // No expected source exists.
    assert_eq!(d.entries()[0].relation, IdentityRelation::NewIdentity);
    for m in x.members() {
        f01_member(
            m,
            EnumerationOutcome::NotAttempted,
            0,
            CoverageVerdict::NoAttempt,
            VerdictRule::NoAttempt,
        );
        let assessment = assess(m, &d).unwrap();
        assert_eq!(
            assessment.ceiling,
            PresentationCeiling::WeakenedByLateDelivery
        );
        assert_eq!(assessment.verdict, CoverageVerdict::NoAttempt);
    }
    assert_eq!(x.dispositions(), &[Disposition::Admitted]);
    f01_no_global(&x);
    ua7_conservation(&x, 0, 0);
}
#[test]
fn r103h_rule_1b_precedes_local_unexpected_evidence_without_initial() {
    // One SECOND plus local evidence reaches rule4; two SECOND identities keep rule1b.
    for count in [1, 2] {
        let mut frames = vec![frame(1, record("f1v1-i9", 1, 1))];
        for i in 0..count {
            frames.push(frame(
                i + 2,
                listing(0, true, RawEnumeration::Closed(0), &[]),
            ));
        }
        frames.push(frame(count + 2, Payload::RunEnd));
        let x = run(frames);
        f01_member(
            &x.members()[0],
            EnumerationOutcome::NotAttempted,
            1,
            CoverageVerdict::Inconsistent,
            if count == 1 {
                VerdictRule::LocalAnomaly
            } else {
                VerdictRule::MemberEnumerationAnomaly
            },
        );
        assert_eq!(
            x.members()[0].delivery().second_enum_identities,
            count as u32
        );
        assert_eq!(
            x.members()[0].second_enumeration(),
            &SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
        );
        f01_member(
            &x.members()[1],
            EnumerationOutcome::NotAttempted,
            0,
            CoverageVerdict::NoAttempt,
            VerdictRule::NoAttempt,
        );
        assert_eq!(
            x.dispositions()[0],
            Disposition::Quarantined(QuarantineCode::UnexpectedSource)
        );
        assert!(
            x.dispositions()[1..]
                .iter()
                .all(|d| *d == Disposition::Admitted)
        );
        f01_no_global(&x);
        ua7_conservation(&x, 1, 0);
    }
}
#[test]
fn r103i_rule_1a_d2_or_gap_precedes_local_evidence_without_initial() {
    for d2 in [true, false] {
        let frames = if d2 {
            vec![
                frame(1, record("f1v1-i9", 1, 1)),
                frame(2, listing(1, false, RawEnumeration::Closed(0), &[])),
                frame(2, listing(1, false, RawEnumeration::RootAbsent, &[])),
                frame(3, Payload::RunEnd),
            ]
        } else {
            vec![frame(1, record("f1v1-i9", 1, 1)), frame(3, Payload::RunEnd)]
        };
        let x = run(frames);
        f01_member(
            &x.members()[0],
            EnumerationOutcome::NotAttempted,
            1,
            CoverageVerdict::Inconsistent,
            VerdictRule::ExecutionGlobalAnomaly,
        );
        assert_eq!(x.members()[1].verdict(), CoverageVerdict::Inconsistent);
        assert_eq!(x.members()[1].cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(x.members()[1].delivery().unexpected_source, 0);
        for m in x.members() {
            assert_eq!(m.delivery().global_seq_conflicts, u32::from(d2));
            assert_eq!(m.delivery().unattributable_gaps, u32::from(!d2));
            assert!(m.sources().is_empty());
            assert!(m.records().is_empty());
        }
        assert_eq!(
            x.dispositions()[0],
            Disposition::Quarantined(QuarantineCode::UnexpectedSource)
        );
        if d2 {
            assert_eq!(
                &x.dispositions()[1..3],
                &[Disposition::Quarantined(QuarantineCode::SequenceConflict); 2]
            );
        }
        ua7_conservation(&x, 1, 0);
    }
}
#[test]
fn r103j_no_initial_owned_d1_counts_instances_but_d2_preserves_global_precedence() {
    for conflict in [false, true] {
        let x = run(vec![
            frame(1, record("f1v1-i9", 1, 1)),
            frame(1, record("f1v1-i9", 1, if conflict { 2 } else { 1 })),
            frame(2, Payload::RunEnd),
        ]);
        let code = if conflict {
            QuarantineCode::SequenceConflict
        } else {
            QuarantineCode::UnexpectedSource
        };
        assert_eq!(&x.dispositions()[..2], &[Disposition::Quarantined(code); 2]);
        assert_eq!(x.dispositions()[2], Disposition::Admitted);
        f01_member(
            &x.members()[0],
            EnumerationOutcome::NotAttempted,
            if conflict { 0 } else { 2 },
            CoverageVerdict::Inconsistent,
            if conflict {
                VerdictRule::ExecutionGlobalAnomaly
            } else {
                VerdictRule::LocalAnomaly
            },
        );
        f01_member(
            &x.members()[1],
            EnumerationOutcome::NotAttempted,
            0,
            if conflict {
                CoverageVerdict::Inconsistent
            } else {
                CoverageVerdict::NoAttempt
            },
            if conflict {
                VerdictRule::ExecutionGlobalAnomaly
            } else {
                VerdictRule::NoAttempt
            },
        );
        assert_eq!(x.unbound_unexpected_source_deliveries(), 0);
        for m in x.members() {
            assert_eq!(m.delivery().global_seq_conflicts, u32::from(conflict));
            assert_eq!(m.delivery().seq_conflicts, 0);
            assert_eq!(m.delivery().global_replays, 0);
            assert_eq!(m.delivery().replays_quarantined, 0);
            assert_eq!(m.delivery().unattributable_gaps, 0);
        }
        ua7_conservation(&x, if conflict { 0 } else { 2 }, 0);
    }
}
#[test]
fn r103k_distribution_mismatch_still_aborts_before_member_owned_counting() {
    let mut r = raw(vec![
        frame(1, record("f1v1-i9", 1, 1)),
        frame(2, Payload::RunEnd),
    ]);
    r.distribution_matches = false;
    let x = finalize(r, &catalog()).unwrap();
    assert_eq!(
        x.dispositions(),
        &[Disposition::Quarantined(QuarantineCode::DistributionMismatch); 2]
    );
    for m in x.members() {
        f01_member(
            m,
            EnumerationOutcome::ScopeUnavailable(UnavailableReason::DistributionMismatch),
            0,
            CoverageVerdict::ScopeUnavailable(UnavailableReason::DistributionMismatch),
            VerdictRule::ScopeUnavailable,
        );
    }
    f01_no_global(&x);
    ua7_conservation(&x, 0, 0);
}
