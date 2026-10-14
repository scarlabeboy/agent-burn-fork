use abf_synthetic_adapter::*;

#[test]
fn default_is_denied_without_successful_empty_output() {
    let r = run_synthetic_fixture(SyntheticCase::default());
    assert_eq!(r.case, SyntheticCase::DenyAllDefault);
    assert!(r.historical.is_empty());
    assert!(r.receiver_uses.is_empty());
    assert_eq!(
        r.operations.last().unwrap().outcome,
        Outcome::Refused(Reason::DispositionCompatibilityUnproven)
    );
    assert_eq!(r.account, ProviderAccount::NotModeled);
    assert_eq!(r.snapshots.last().unwrap().counters.admissions, 0);
}

const BASELINE: Qualification = Qualification {
    inventory_partial: true,
    byte_partial: true,
    version_unproven: true,
    producer_unverified: true,
    identity_unresolved: true,
    stale: false,
    disputed: false,
    revoked: false,
};
const CASES: [SyntheticCase; 6] = [
    SyntheticCase::DenyAllDefault,
    SyntheticCase::TwoEnvironmentQualifiedProjection,
    SyntheticCase::RepeatedHandleAndEqualCapture,
    SyntheticCase::ReceiverAndRevocation,
    SyntheticCase::DowngradeAndLineageLoss,
    SyntheticCase::ExhaustionAndOutstandingRead,
];
fn history(r: &SyntheticReadout, index: usize) -> &QualifiedData {
    &r.historical[index].inspection.as_ref().unwrap().data
}
#[test]
fn two_environments_preserve_uncertainty_and_equal_captures() {
    let r = run_synthetic_fixture(SyntheticCase::TwoEnvironmentQualifiedProjection);
    let d = history(&r, 0);
    assert_eq!(d.qualification, BASELINE);
    assert_eq!(d.observation_count, 2);
    assert_eq!(d.symbolic_sampled_extent, 8);
    assert_eq!(
        d.environments,
        vec![
            FixtureEnvironment {
                id: 1,
                label: EnvironmentLabel::SimulatedWindows
            },
            FixtureEnvironment {
                id: 2,
                label: EnvironmentLabel::SimulatedSelectedWsl
            }
        ]
    );
    assert_eq!(
        d.captures
            .iter()
            .map(|c| (
                c.fixture_object,
                c.fixture_capture,
                c.environment.id,
                c.principal,
                c.harness,
                c.directory,
                c.symbolic_capture_time,
                c.range
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                1,
                1,
                1,
                7,
                3,
                9,
                0,
                SampleRange {
                    start: 0,
                    length: 4,
                    domain_end: 64
                }
            ),
            (
                2,
                2,
                2,
                7,
                3,
                9,
                0,
                SampleRange {
                    start: 0,
                    length: 4,
                    domain_end: 64
                }
            )
        ]
    );
    assert!(matches!(
        &r.receiver_uses[0],
        ReceiverUse::Authorized(FreshReceiverReadout { receiver: 11, .. })
    ));
}
#[test]
fn repeated_handles_collapse_but_equal_captures_remain() {
    let r = run_synthetic_fixture(SyntheticCase::RepeatedHandleAndEqualCapture);
    let d = history(&r, 0);
    assert_eq!(d.observation_count, 2);
    assert_eq!(d.symbolic_sampled_extent, 4);
    assert_eq!(
        d.captures
            .iter()
            .map(|c| (c.fixture_object, c.fixture_capture, c.symbolic_capture_time))
            .collect::<Vec<_>>(),
        vec![(1, 1, 0), (1, 2, 0)]
    );
    assert_eq!(d.qualification, BASELINE);
}
#[test]
fn wrong_receivers_and_retirement_cannot_use_history() {
    let r = run_synthetic_fixture(SyntheticCase::ReceiverAndRevocation);
    assert_eq!(
        r.receiver_uses[0],
        ReceiverUse::Refused {
            receiver: 99,
            reason: Reason::Unauthorized
        }
    );
    assert!(matches!(&r.receiver_uses[1], ReceiverUse::Authorized(_)));
    assert_eq!(
        r.receiver_uses[2],
        ReceiverUse::Refused {
            receiver: 11,
            reason: Reason::Retired
        }
    );
    assert_eq!(r.snapshots[0].counters, r.snapshots[1].counters);
    let q = Qualification {
        stale: true,
        revoked: true,
        ..BASELINE
    };
    assert_eq!(history(&r, 1).qualification, q);
    assert_eq!(history(&r, 1).observation_count, 1);
    for op in [
        OperationCode::Release,
        OperationCode::Accept,
        OperationCode::UseResult,
    ] {
        assert!(r.operations.contains(&OperationOutcome {
            operation: op,
            outcome: Outcome::Refused(Reason::Unauthorized)
        }));
    }
    for op in [
        OperationCode::Derive,
        OperationCode::Computer,
        OperationCode::UseResult,
    ] {
        assert!(r.operations.contains(&OperationOutcome {
            operation: op,
            outcome: Outcome::Refused(Reason::Retired)
        }));
    }
    assert_eq!(r.snapshots.last().unwrap().counters.uses, 2);
}
#[test]
fn downgrade_is_preserved_and_lineage_loss_withholds() {
    let r = run_synthetic_fixture(SyntheticCase::DowngradeAndLineageLoss);
    let q = Qualification {
        stale: true,
        disputed: true,
        ..BASELINE
    };
    assert_eq!(history(&r, 1).qualification, q);
    assert_eq!(history(&r, 2).qualification, q);
    assert!(
        matches!(&r.receiver_uses[1],ReceiverUse::Authorized(FreshReceiverReadout{data,..}) if data.qualification==q)
    );
    assert_eq!(r.historical[3].inspection, Err(Reason::LineageUnknown));
    assert_eq!(
        r.receiver_uses[2],
        ReceiverUse::Refused {
            receiver: 11,
            reason: Reason::LineageUnknown
        }
    );
    assert!(r.operations.contains(&OperationOutcome {
        operation: OperationCode::Computer,
        outcome: Outcome::Refused(Reason::LineageUnknown)
    }));
}
#[test]
fn exhaustion_deadline_and_return_accounting_are_distinct() {
    let r = run_synthetic_fixture(SyntheticCase::ExhaustionAndOutstandingRead);
    assert!(r.receiver_uses.is_empty());
    assert!(r.historical.is_empty());
    assert_eq!(
        r.operations
            .iter()
            .filter(|o| **o
                == OperationOutcome {
                    operation: OperationCode::Begin,
                    outcome: Outcome::Refused(Reason::Exhausted)
                })
            .count(),
        3
    );
    assert_eq!(r.snapshots[0].held, r.snapshots[1].held);
    assert_eq!(r.snapshots[0].spent, r.snapshots[1].spent);
    assert_eq!(r.snapshots[0].counters, r.snapshots[1].counters);
    assert_eq!(r.snapshots[1].held, r.snapshots[2].held);
    assert_eq!(&r.snapshots[2].held[..3], &[5, 1, 1]);
    assert_eq!(&r.snapshots[3].held[..3], &[5, 1, 1]);
    assert_eq!(r.snapshots[3].counters.post_samples, 0);
    assert_eq!(&r.snapshots[4].held[..3], &[0, 0, 0]);
    assert_eq!(r.snapshots[4].counters.post_samples, 2);
    assert!(r.operations.contains(&OperationOutcome {
        operation: OperationCode::Restart,
        outcome: Outcome::Refused(Reason::RecoveryUnavailable)
    }));
    assert!(r.operations.contains(&OperationOutcome {
        operation: OperationCode::Release,
        outcome: Outcome::Refused(Reason::Fenced)
    }));
    assert_eq!(r.snapshots.last().unwrap().counters.reads, 1);
}
#[test]
fn every_case_is_deterministic_finite_and_synthetic() {
    for case in CASES {
        let r = run_synthetic_fixture(case);
        assert_eq!(r, run_synthetic_fixture(case));
        assert_eq!(r.origin, Origin::SyntheticOnly);
        assert_eq!(r.contract_version, 1);
        assert_eq!(r.account, ProviderAccount::NotModeled);
        for s in &r.snapshots {
            for i in 0..12 {
                assert!(s.held[i] <= s.peak[i]);
                assert!(s.peak[i] <= r.fixture_caps[i]);
                assert!(s.spent[i] <= r.fixture_caps[i]);
            }
        }
        for h in &r.historical {
            if let Ok(h) = &h.inspection {
                assert_eq!(h.data.origin, Origin::SyntheticOnly);
            }
        }
    }
}
#[test]
fn default_never_admits_reads_prompts_releases_or_uses() {
    let r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
    let c = r.snapshots.last().unwrap().counters;
    assert_eq!(
        (c.admissions, c.reads, c.prompts, c.releases, c.uses),
        (0, 0, 0, 0, 0)
    );
}

#[test]
fn fixed_terminal_accounting_table() {
    // Independent counts from the declared transition sequences, not conversion.
    let expected = [
        (
            SyntheticCase::DenyAllDefault,
            [0, 0, 0, 0, 0, 0, 1],
            3,
            0,
            3,
        ),
        (
            SyntheticCase::TwoEnvironmentQualifiedProjection,
            [2, 0, 2, 3, 2, 4, 2],
            25,
            6,
            15,
        ),
        (
            SyntheticCase::RepeatedHandleAndEqualCapture,
            [1, 0, 2, 3, 2, 4, 1],
            22,
            7,
            11,
        ),
        (
            SyntheticCase::ReceiverAndRevocation,
            [1, 0, 1, 2, 1, 2, 1],
            13,
            3,
            8,
        ),
        (
            SyntheticCase::DowngradeAndLineageLoss,
            [1, 0, 1, 3, 1, 2, 1],
            14,
            3,
            8,
        ),
        (
            SyntheticCase::ExhaustionAndOutstandingRead,
            [1, 0, 0, 0, 1, 2, 1],
            8,
            0,
            5,
        ),
    ];
    for (case, counters, work, retained, registry) in expected {
        let r = run_synthetic_fixture(case);
        let s = r.snapshots.last().unwrap();
        let c = s.counters;
        assert_eq!(
            [
                c.admissions,
                c.prompts,
                c.releases,
                c.uses,
                c.reads,
                c.post_samples,
                c.discoveries
            ],
            counters,
            "{case:?}"
        );
        assert_eq!(s.spent[7], work, "{case:?}");
        assert_eq!(s.held[6], retained, "{case:?}");
        assert_eq!(s.spent[11], registry, "{case:?}");
    }
}
