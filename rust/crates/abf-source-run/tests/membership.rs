use abf_source_run::*;

fn scope(kind: EnvironmentKind, env: &str, family: &str, layout: &str) -> CatalogScope {
    CatalogScope::new(kind, env, family, layout).unwrap()
}
fn windows(family: &str, layout: &str) -> CatalogScope {
    scope(EnvironmentKind::Windows, "w1", family, layout)
}
fn catalog(scopes: &[CatalogScope]) -> CatalogSnapshot {
    let mut b = CatalogBuilder::new();
    for (i, s) in scopes.iter().enumerate() {
        b.add(s.clone(), &format!("s{i}")).unwrap();
    }
    b.build()
}
fn refs(selection: Selection) -> SelectedRefs {
    SelectedRefs {
        windows: matches!(selection, Selection::WindowsOnly | Selection::Both)
            .then(|| OpaqueRef::new("w1").unwrap()),
        selected_wsl: matches!(selection, Selection::SelectedWslOnly | Selection::Both)
            .then(|| OpaqueRef::new("l1").unwrap()),
    }
}
fn execution(
    scopes: &[CatalogScope],
    c: &CatalogSnapshot,
    attempt: &str,
    finalization: &str,
    sources_per_member: usize,
    records_per_member: usize,
) -> FinalizedExecution {
    let attempt = AttemptRef::new(attempt).unwrap();
    let mut frames = Vec::new();
    let mut push = |payload| {
        frames.push(ReaderFrame {
            attempt: attempt.clone(),
            frame_seq: frames.len() as u64 + 1,
            payload,
        });
    };
    for (member, s) in scopes.iter().enumerate() {
        let tag = c.scope_tag(s).unwrap();
        push(Payload::Enumeration {
            member,
            second: false,
            outcome: RawEnumeration::Closed(sources_per_member as u64),
            sources: (1..=sources_per_member)
                .map(|n| format!("{tag}-i{n}"))
                .collect(),
            observed_at: None,
        });
        for n in 0..records_per_member {
            let bytes = br#"{"tokens":{"input":100}}"#.to_vec();
            push(Payload::Source {
                source: format!("{tag}-i1"),
                source_seq: n as u64 + 1,
                event: SourceEvent::Record {
                    result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
                    bytes,
                },
            });
        }
    }
    finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt,
                finalization: FinalizationId::new(finalization).unwrap(),
                members: scopes.to_vec(),
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: frames,
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(1, 2),
        },
        c,
    )
    .unwrap()
}
fn rejection(
    selection: Selection,
    selected: SelectedRefs,
    members: &[&FinalizedMember],
    c: &CatalogSnapshot,
    code: AdaptCode,
) {
    assert_eq!(
        ProjectionMembership::validate(selection, selected, members, c).unwrap_err(),
        code
    );
}

#[test]
fn a1_requires_exact_selected_refs_for_every_selection() {
    let c = catalog(&[]);
    for selection in [
        Selection::WindowsOnly,
        Selection::SelectedWslOnly,
        Selection::Both,
    ] {
        assert!(ProjectionMembership::validate(selection, refs(selection), &[], &c).is_ok());
        rejection(
            selection,
            SelectedRefs {
                windows: None,
                selected_wsl: None,
            },
            &[],
            &c,
            AdaptCode::SelectionRefMismatch,
        );
    }
    rejection(
        Selection::WindowsOnly,
        refs(Selection::Both),
        &[],
        &c,
        AdaptCode::SelectionRefMismatch,
    );
    rejection(
        Selection::SelectedWslOnly,
        refs(Selection::Both),
        &[],
        &c,
        AdaptCode::SelectionRefMismatch,
    );
}

#[test]
fn a1_precedes_a2_and_a2_precedes_a3_in_both_directions() {
    for (selection, s) in [
        (
            Selection::WindowsOnly,
            scope(EnvironmentKind::SelectedWsl, "old", "f1", "v1"),
        ),
        (
            Selection::SelectedWslOnly,
            scope(EnvironmentKind::Windows, "old", "f1", "v1"),
        ),
    ] {
        let c = catalog(std::slice::from_ref(&s));
        let x = execution(&[s], &c, "a1", "z1", 1, 0);
        let members = [&x.members()[0]];
        rejection(
            selection,
            SelectedRefs {
                windows: None,
                selected_wsl: None,
            },
            &members,
            &c,
            AdaptCode::SelectionRefMismatch,
        );
        rejection(
            selection,
            refs(selection),
            &members,
            &c,
            AdaptCode::MemberKindNotSelected,
        );
    }
}

#[test]
fn a3_precedes_duplicate_scope_and_rejects_old_environment_refs() {
    for (selection, kind) in [
        (Selection::WindowsOnly, EnvironmentKind::Windows),
        (Selection::SelectedWslOnly, EnvironmentKind::SelectedWsl),
    ] {
        let s = scope(kind, "old", "f1", "v1");
        let c = catalog(std::slice::from_ref(&s));
        let x = execution(&[s], &c, "a1", "z1", 1, 0);
        rejection(
            selection,
            refs(selection),
            &[&x.members()[0], &x.members()[0]],
            &c,
            AdaptCode::EnvironmentRefNotSelected,
        );
    }
}

#[test]
fn a4_rejects_repeated_members_scans_and_retries_before_family_overlap() {
    let s = windows("f1", "v1");
    let c = catalog(std::slice::from_ref(&s));
    let x = execution(std::slice::from_ref(&s), &c, "a1", "z1", 1, 1);
    let y = execution(std::slice::from_ref(&s), &c, "a2", "z2", 1, 1);
    let retry = finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: AttemptRef::new("a3").unwrap(),
                finalization: FinalizationId::new("z3").unwrap(),
                members: vec![s],
                retry_of: Some(AttemptRef::new("a1").unwrap()),
                subset: RetrySubset::OnlyNonCoveredSources,
            },
            pre_barrier: vec![],
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(2, 3),
        },
        &c,
    )
    .unwrap();
    let alone = ProjectionMembership::validate(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[&retry.members()[0]],
        &c,
    )
    .unwrap();
    assert_eq!(
        alone.members()[0].lineage().1,
        RetrySubset::OnlyNonCoveredSources
    );
    assert_eq!(alone.members()[0].lineage().0.unwrap().as_str(), "a1");
    for other in [&x.members()[0], &y.members()[0], &retry.members()[0]] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&x.members()[0], other],
            &c,
            AdaptCode::DuplicateScopeMember,
        );
    }
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(y.members()[0].records().len(), 1);
}

#[test]
fn a5_rejects_layout_variants_before_declared_overlap() {
    let scopes = [windows("f1", "v1"), windows("f1", "v2")];
    let mut b = CatalogBuilder::new();
    b.add(scopes[0].clone(), "s0").unwrap();
    b.add(scopes[1].clone(), "s1").unwrap();
    b.declare_overlap(&scopes[0], &scopes[1]).unwrap();
    let c = b.build();
    let x = execution(&scopes, &c, "a1", "z1", 1, 1);
    rejection(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[&x.members()[0], &x.members()[1]],
        &c,
        AdaptCode::FamilyOverlap,
    );
    let y = execution(&scopes[1..], &c, "a2", "z2", 1, 1);
    rejection(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[&x.members()[0], &y.members()[0]],
        &c,
        AdaptCode::FamilyOverlap,
    );
}

#[test]
fn a6_declared_overlap_is_symmetric_and_precedes_attempt_reuse() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let mut b = CatalogBuilder::new();
    assert_eq!(
        b.declare_overlap(&scopes[0], &scopes[1]),
        Err(RefCode::UnknownScope)
    );
    b.add(scopes[0].clone(), "s0").unwrap();
    b.add(scopes[1].clone(), "s1").unwrap();
    b.declare_overlap(&scopes[1], &scopes[0]).unwrap();
    let c = b.build();
    let x = execution(&scopes[..1], &c, "a1", "z1", 1, 0);
    let y = execution(&scopes[1..], &c, "a1", "z2", 1, 0);
    for members in [
        [&x.members()[0], &y.members()[0]],
        [&y.members()[0], &x.members()[0]],
    ] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &members,
            &c,
            AdaptCode::DeclaredCatalogOverlap,
        );
    }
}

#[test]
fn a7_allows_shared_execution_but_rejects_different_manifests_or_finalizations() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = catalog(&scopes);
    let x = execution(&scopes, &c, "a1", "z1", 1, 1);
    assert!(
        ProjectionMembership::validate(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&x.members()[0], &x.members()[1]],
            &c
        )
        .is_ok()
    );
    for y in [
        execution(&scopes, &c, "a1", "z2", 1, 1),
        execution(&scopes[1..], &c, "a1", "z1", 1, 1),
        execution(
            &[scopes[1].clone(), scopes[0].clone()],
            &c,
            "a1",
            "z1",
            1,
            1,
        ),
    ] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[
                &x.members()[0],
                y.members()
                    .iter()
                    .find(|m| m.scope() == &scopes[1])
                    .unwrap(),
            ],
            &c,
            AdaptCode::AttemptRefReused,
        );
    }
}

#[test]
fn independent_authorities_are_refused_before_namespace_and_a9() {
    let s1 = windows("f1", "v1");
    let s2 = windows("f2", "v1");
    // Independent catalogs can legitimately finalize the same textual tag.
    let c1 = catalog(std::slice::from_ref(&s1));
    let c2 = catalog(std::slice::from_ref(&s2));
    let c = catalog(&[s1.clone(), s2.clone()]);
    let x = execution(&[s1], &c1, "a1", "z1", 1, 65);
    let y = execution(&[s2], &c2, "a2", "z2", 1, 64);
    for members in [
        [&x.members()[0], &y.members()[0]],
        [&y.members()[0], &x.members()[0]],
    ] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &members,
            &c,
            AdaptCode::CatalogAuthorityMismatch,
        );
    }
    assert_eq!(x.members()[0].records().len(), 65);
    assert_eq!(y.members()[0].records().len(), 64);
}

#[test]
fn a9_records_and_declarations_have_independent_127_128_129_boundaries() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = catalog(&scopes);
    for n in [127, 128] {
        for (sources, records) in [(1, n), (n, 0)] {
            let x = execution(&scopes[..1], &c, "a1", "z1", sources, records);
            let m = ProjectionMembership::validate(
                Selection::WindowsOnly,
                refs(Selection::WindowsOnly),
                &[&x.members()[0]],
                &c,
            )
            .unwrap();
            assert_eq!(m.members().len(), 1);
        }
    }
    for (sources, records, code) in [
        (1, 129, AdaptCode::OccurrenceCeiling),
        (129, 0, AdaptCode::DeclarationCeiling),
        (129, 129, AdaptCode::OccurrenceCeiling),
    ] {
        let x = execution(&scopes[..1], &c, "a1", "z1", sources, records);
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&x.members()[0]],
            &c,
            code,
        );
        assert_eq!(x.members()[0].records().len(), records);
        assert_eq!(x.members()[0].sources().len(), sources);
    }
    for (sources, records, code) in [
        (1, 65, AdaptCode::OccurrenceCeiling),
        (65, 0, AdaptCode::DeclarationCeiling),
    ] {
        let x = execution(&scopes, &c, "a1", "z1", sources, records);
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&x.members()[0], &x.members()[1]],
            &c,
            code,
        );
    }
}

#[test]
fn a10_canonical_order_is_independent_of_all_input_permutations() {
    let w = [windows("z", "v1"), windows("a", "v1")];
    let l = scope(EnvironmentKind::SelectedWsl, "l1", "a", "v1");
    let c = catalog(&[w[0].clone(), w[1].clone(), l.clone()]);
    let x = execution(&w, &c, "a1", "z1", 1, 1);
    let y = execution(&[l], &c, "a2", "z2", 1, 1);
    let all = [&x.members()[0], &x.members()[1], &y.members()[0]];
    for p in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let m = ProjectionMembership::validate(
            Selection::Both,
            refs(Selection::Both),
            &p.map(|i| all[i]),
            &c,
        )
        .unwrap();
        assert_eq!(m.selection(), Selection::Both);
        assert_eq!(m.selected_refs(), &refs(Selection::Both));
        assert!(std::ptr::eq(m.members()[0], all[1]));
        assert!(std::ptr::eq(m.members()[1], all[0]));
        assert!(std::ptr::eq(m.members()[2], all[2]));
    }
}

#[test]
fn a5_allows_same_family_in_distinct_environments_under_common_authority() {
    let w = windows("f1", "v1");
    let l = scope(EnvironmentKind::SelectedWsl, "l1", "f1", "v1");
    let c = catalog(&[w.clone(), l.clone()]);
    let x = execution(&[w], &c, "a1", "z1", 1, 1);
    let y = execution(&[l], &c.clone(), "a2", "z2", 1, 1);
    let m = ProjectionMembership::validate(
        Selection::Both,
        refs(Selection::Both),
        &[&x.members()[0], &y.members()[0]],
        &c,
    )
    .unwrap();
    assert_eq!(m.members().len(), 2);
}

#[test]
fn a9_exact_aggregate_boundaries_do_not_apply_per_member() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = catalog(&scopes);
    for total in [127, 128, 129] {
        for declarations in [false, true] {
            let [a, b] = [64, total - 64];
            let x = execution(
                &scopes[..1],
                &c,
                "a1",
                "z1",
                if declarations { a } else { 1 },
                if declarations { 0 } else { a },
            );
            let y = execution(
                &scopes[1..],
                &c,
                "a2",
                "z2",
                if declarations { b } else { 1 },
                if declarations { 0 } else { b },
            );
            for members in [
                [&x.members()[0], &y.members()[0]],
                [&y.members()[0], &x.members()[0]],
            ] {
                let result = ProjectionMembership::validate(
                    Selection::WindowsOnly,
                    refs(Selection::WindowsOnly),
                    &members,
                    &c,
                );
                match total {
                    127 | 128 => assert_eq!(result.unwrap().members().len(), 2),
                    129 => assert_eq!(
                        result.unwrap_err(),
                        if declarations {
                            AdaptCode::DeclarationCeiling
                        } else {
                            AdaptCode::OccurrenceCeiling
                        }
                    ),
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[test]
fn admitted_empty_unknown_and_unavailable_snapshots_preserve_their_verdicts() {
    let s = windows("f1", "v1");
    let c = catalog(std::slice::from_ref(&s));
    for outcome in [
        RawEnumeration::NotAttempted,
        RawEnumeration::RootAbsent,
        RawEnumeration::RootUnresolved,
        RawEnumeration::ScopeUnavailable(UnavailableReason::Unavailable),
        RawEnumeration::Closed(0),
    ] {
        let attempt = AttemptRef::new("a1").unwrap();
        let x = finalize(
            RawExecutionLog {
                manifest: ExecutionManifest {
                    attempt: attempt.clone(),
                    finalization: FinalizationId::new("z1").unwrap(),
                    members: vec![s.clone()],
                    retry_of: None,
                    subset: RetrySubset::AllExpected,
                },
                pre_barrier: vec![ReaderFrame {
                    attempt,
                    frame_seq: 1,
                    payload: Payload::Enumeration {
                        member: 0,
                        second: false,
                        outcome,
                        sources: vec![],
                        observed_at: None,
                    },
                }],
                process: ProcessEnd::Crashed,
                barrier: Barrier::Drained,
                distribution_matches: true,
                times: TimeObservations::new(1, 2),
            },
            &c,
        )
        .unwrap();
        let before = x.members()[0].verdict();
        let m = ProjectionMembership::validate(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&x.members()[0]],
            &c,
        )
        .unwrap();
        assert_eq!(m.members()[0].verdict(), before);
        assert!(m.members()[0].records().is_empty());
        assert!(m.members()[0].sources().is_empty());
    }
}

#[test]
fn new_admission_debug_output_redacts_synthetic_refs() {
    let s = scope(
        EnvironmentKind::Windows,
        "secret_environment",
        "secret_family",
        "v1",
    );
    let c = catalog(std::slice::from_ref(&s));
    let x = execution(&[s], &c, "secret_attempt", "z1", 1, 1);
    let selected = SelectedRefs {
        windows: Some(OpaqueRef::new("secret_environment").unwrap()),
        selected_wsl: None,
    };
    assert!(!format!("{selected:?}").contains("secret"));
    let m =
        ProjectionMembership::validate(Selection::WindowsOnly, selected, &[&x.members()[0]], &c)
            .unwrap();
    assert!(!format!("{m:?}").contains("secret"));
    assert_eq!(
        format!("{:?}", AdaptCode::AttemptRefReused),
        "AttemptRefReused"
    );
}

#[test]
fn only_retained_records_and_initial_expected_sources_count_toward_a9() {
    let s = windows("f1", "v1");
    let c = catalog(std::slice::from_ref(&s));
    let attempt = AttemptRef::new("a1").unwrap();
    let frame = |frame_seq, payload| ReaderFrame {
        attempt: attempt.clone(),
        frame_seq,
        payload,
    };
    let bytes = br#"{"tokens":{"input":100}}"#.to_vec();
    let record = frame(
        2,
        Payload::Source {
            source: "s0-i1".into(),
            source_seq: 1,
            event: SourceEvent::Record {
                result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
                bytes,
            },
        },
    );
    let mut frames = vec![frame(
        1,
        Payload::Enumeration {
            member: 0,
            second: false,
            outcome: RawEnumeration::Closed(1),
            sources: vec!["s0-i1".into()],
            observed_at: None,
        },
    )];
    frames.extend(std::iter::repeat_n(record, 129));
    frames.push(frame(
        3,
        Payload::Enumeration {
            member: 0,
            second: true,
            outcome: RawEnumeration::Closed(129),
            sources: (1..=129).map(|n| format!("s0-i{n}")).collect(),
            observed_at: None,
        },
    ));
    let x = finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt,
                finalization: FinalizationId::new("z1").unwrap(),
                members: vec![s],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: frames,
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(0, 1),
        },
        &c,
    )
    .unwrap();
    assert_eq!(x.members()[0].records().len(), 1);
    assert_eq!(x.members()[0].sources().len(), 1);
    assert_eq!(x.members()[0].delivery().replays_quarantined, 128);
    assert!(
        matches!(x.members()[0].second_enumeration(), SecondEnumeration::Performed { new_sources, .. } if new_sources.len() == 128)
    );
    let m = ProjectionMembership::validate(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[&x.members()[0]],
        &c,
    )
    .unwrap();
    assert_eq!(m.members()[0].records().len(), 1);
    assert_eq!(m.members()[0].sources().len(), 1);
}

#[path = "membership/corrections.rs"]
mod corrections;
