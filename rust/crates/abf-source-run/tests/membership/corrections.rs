//! ABF108D synthetic corrections to the ABF108C W1–W9 witnesses.
use super::*;

fn pair_catalog(scopes: &[CatalogScope], overlap: bool) -> CatalogSnapshot {
    let mut b = CatalogBuilder::new();
    for (i, s) in scopes.iter().enumerate() {
        b.add(s.clone(), &format!("s{i}")).unwrap();
    }
    if overlap {
        b.declare_overlap(&scopes[0], &scopes[1]).unwrap();
    }
    b.build()
}

#[test]
fn w1_authoritative_overlap_cannot_be_removed_by_substitution() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = pair_catalog(&scopes, true);
    let x = execution(&scopes, &c, "a1", "z1", 1, 1);
    let members = [&x.members()[0], &x.members()[1]];
    let before = (
        x.dispositions().to_vec(),
        x.members()[0].records().to_vec(),
        x.members()[1].sources().to_vec(),
    );
    for authentic in [&c, &c.clone()] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &members,
            authentic,
            AdaptCode::DeclaredCatalogOverlap,
        );
    }
    let mut retagged = CatalogBuilder::new();
    retagged.add(scopes[0].clone(), "new0").unwrap();
    retagged.add(scopes[1].clone(), "new1").unwrap();
    for substitute in [
        pair_catalog(&scopes, false),
        CatalogBuilder::new().build(),
        retagged.build(),
        catalog(&scopes[..1]),
        pair_catalog(&scopes, true),
    ] {
        for order in [members, [members[1], members[0]]] {
            rejection(
                Selection::WindowsOnly,
                refs(Selection::WindowsOnly),
                &order,
                &substitute,
                AdaptCode::CatalogAuthorityMismatch,
            );
        }
    }
    assert_eq!(
        before,
        (
            x.dispositions().to_vec(),
            x.members()[0].records().to_vec(),
            x.members()[1].sources().to_vec()
        )
    );
}

#[test]
fn authority_clone_survives_original_drop_and_shares_multiple_finalizations() {
    let scopes = [windows("z", "v1"), windows("a", "v1")];
    let c = catalog(&scopes);
    let clone = c.clone();
    let x = execution(&scopes[..1], &c, "a1", "z1", 1, 1);
    let y = execution(&scopes[1..], &clone, "a2", "z2", 1, 1);
    drop(c);
    for order in [
        [&x.members()[0], &y.members()[0]],
        [&y.members()[0], &x.members()[0]],
    ] {
        let m = ProjectionMembership::validate(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &order,
            &clone,
        )
        .unwrap();
        assert!(std::ptr::eq(m.members()[0], &y.members()[0]));
        assert!(std::ptr::eq(m.members()[1], &x.members()[0]));
    }
    // An independently rebuilt, value-identical catalog is not an authority clone.
    let rebuilt = catalog(&scopes);
    assert_eq!(clone, rebuilt);
    rejection(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[&x.members()[0]],
        &rebuilt,
        AdaptCode::CatalogAuthorityMismatch,
    );
}

#[test]
fn independent_authorities_cannot_be_combined_even_across_environments() {
    for other in [
        windows("f2", "v1"),
        scope(EnvironmentKind::SelectedWsl, "l1", "f1", "v1"),
    ] {
        let scopes = [windows("f1", "v1"), other];
        let c1 = catalog(std::slice::from_ref(&scopes[0]));
        let c2 = catalog(std::slice::from_ref(&scopes[1]));
        let replacement = catalog(&scopes);
        let x = execution(&scopes[..1], &c1, "a1", "z1", 1, 1);
        let y = execution(&scopes[1..], &c2, "a2", "z2", 1, 1);
        for c in [&c1, &c2, &replacement] {
            for members in [
                [&x.members()[0], &y.members()[0]],
                [&y.members()[0], &x.members()[0]],
            ] {
                rejection(
                    Selection::Both,
                    refs(Selection::Both),
                    &members,
                    c,
                    AdaptCode::CatalogAuthorityMismatch,
                );
            }
        }
    }
}

#[test]
fn empty_membership_still_accepts_valid_refs_with_any_catalog() {
    for c in [
        CatalogBuilder::new().build(),
        catalog(&[windows("f1", "v1")]),
    ] {
        for s in [
            Selection::WindowsOnly,
            Selection::SelectedWslOnly,
            Selection::Both,
        ] {
            assert!(
                ProjectionMembership::validate(s, refs(s), &[], &c)
                    .unwrap()
                    .members()
                    .is_empty()
            );
        }
    }
}

#[test]
fn authority_mismatch_follows_a5_but_precedes_a6_a7_and_a9() {
    let scopes = [
        windows("f1", "v1"),
        windows("f1", "v2"),
        windows("f2", "v1"),
    ];
    let c = catalog(&scopes);
    let replacement = pair_catalog(&scopes, true);
    let x = execution(&scopes[..1], &c, "a1", "z1", 1, 129);
    let y = execution(&scopes[1..2], &c, "a1", "z2", 1, 0);
    let z = execution(&scopes[2..], &c, "a1", "z3", 1, 0);
    let a = &x.members()[0];
    let b = &y.members()[0];
    let d = &z.members()[0];
    rejection(
        Selection::WindowsOnly,
        refs(Selection::Both),
        &[a, b],
        &replacement,
        AdaptCode::SelectionRefMismatch,
    );
    rejection(
        Selection::SelectedWslOnly,
        refs(Selection::SelectedWslOnly),
        &[a, b],
        &replacement,
        AdaptCode::MemberKindNotSelected,
    );
    rejection(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("old").unwrap()),
            selected_wsl: None,
        },
        &[a, b],
        &replacement,
        AdaptCode::EnvironmentRefNotSelected,
    );
    rejection(
        Selection::WindowsOnly,
        refs(Selection::WindowsOnly),
        &[a, a],
        &replacement,
        AdaptCode::DuplicateScopeMember,
    );
    for order in [[a, b], [b, a]] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &order,
            &replacement,
            AdaptCode::FamilyOverlap,
        );
    }
    let overlapping_replacement = pair_catalog(&[scopes[0].clone(), scopes[2].clone()], true);
    for order in [[a, d], [d, a]] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &order,
            &overlapping_replacement,
            AdaptCode::CatalogAuthorityMismatch,
        );
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &order,
            &c,
            AdaptCode::AttemptRefReused,
        );
    }
}

fn with_lineage(
    c: &CatalogSnapshot,
    scopes: &[CatalogScope],
    retry: Option<&str>,
    subset: RetrySubset,
) -> FinalizedExecution {
    finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: AttemptRef::new("a2").unwrap(),
                finalization: FinalizationId::new("z2").unwrap(),
                members: scopes.to_vec(),
                retry_of: retry.map(|s| AttemptRef::new(s).unwrap()),
                subset,
            },
            pre_barrier: vec![],
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(1, 2),
        },
        c,
    )
    .unwrap()
}

#[test]
fn w8_public_finalization_preserves_retry_lineage_in_manifest_consistency() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = catalog(&scopes);
    let x = with_lineage(&c, &scopes, None, RetrySubset::AllExpected);
    for y in [
        with_lineage(&c, &scopes, Some("a1"), RetrySubset::AllExpected),
        with_lineage(&c, &scopes, None, RetrySubset::OnlyNonCoveredSources),
    ] {
        for order in [
            [&x.members()[0], &y.members()[1]],
            [&y.members()[1], &x.members()[0]],
        ] {
            rejection(
                Selection::WindowsOnly,
                refs(Selection::WindowsOnly),
                &order,
                &c,
                AdaptCode::AttemptRefReused,
            );
        }
    }
    let retry = with_lineage(&c, &scopes, Some("a1"), RetrySubset::OnlyNonCoveredSources);
    assert!(
        ProjectionMembership::validate(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &[&retry.members()[0], &retry.members()[1]],
            &c
        )
        .is_ok()
    );
}

#[test]
fn w9_cross_environment_declared_overlap_uses_one_common_authority() {
    let scopes = [
        windows("f1", "v1"),
        scope(EnvironmentKind::SelectedWsl, "l1", "f2", "v1"),
    ];
    let c = pair_catalog(&scopes, true);
    let x = execution(&scopes[..1], &c, "a1", "z1", 1, 1);
    let y = execution(&scopes[1..], &c.clone(), "a2", "z2", 1, 1);
    for members in [
        [&x.members()[0], &y.members()[0]],
        [&y.members()[0], &x.members()[0]],
    ] {
        rejection(
            Selection::Both,
            refs(Selection::Both),
            &members,
            &c,
            AdaptCode::DeclaredCatalogOverlap,
        );
    }
}

fn listed_execution(
    c: &CatalogSnapshot,
    scopes: &[CatalogScope],
    identity: (&str, &str),
    member: usize,
    initial: &[&str],
    second: Option<&[&str]>,
) -> FinalizedExecution {
    let attempt = AttemptRef::new(identity.0).unwrap();
    let enumeration = |frame_seq, second, sources: &[&str]| ReaderFrame {
        attempt: attempt.clone(),
        frame_seq,
        payload: Payload::Enumeration {
            member,
            second,
            outcome: RawEnumeration::Closed(sources.len() as u64),
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            observed_at: None,
        },
    };
    let mut frames = vec![enumeration(1, false, initial)];
    frames.extend(second.map(|s| enumeration(2, true, s)));
    finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt,
                finalization: FinalizationId::new(identity.1).unwrap(),
                members: scopes.to_vec(),
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: frames,
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(0, 1),
        },
        c,
    )
    .unwrap()
}

#[test]
fn w2_second_listing_collision_under_independent_catalogs_is_refused_at_authority() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c1 = catalog(&scopes[..1]);
    let c2 = catalog(&scopes[1..]);
    let replacement = catalog(&scopes);
    let x = listed_execution(&c1, &scopes[..1], ("a1", "z1"), 0, &["s0-A"], None);
    for y in [
        listed_execution(
            &c2,
            &scopes[1..],
            ("a2", "z2"),
            0,
            &["s0-B"],
            Some(&["s0-B", "s0-A"]),
        ),
        listed_execution(&c2, &scopes[1..], ("a2", "z2"), 0, &["s0-A"], None),
    ] {
        for c in [&c1, &c2, &replacement] {
            for members in [
                [&x.members()[0], &y.members()[0]],
                [&y.members()[0], &x.members()[0]],
            ] {
                rejection(
                    Selection::WindowsOnly,
                    refs(Selection::WindowsOnly),
                    &members,
                    c,
                    AdaptCode::CatalogAuthorityMismatch,
                );
            }
        }
    }
}

#[test]
fn w3_swapped_catalog_tags_cannot_reach_a8_through_public_admission() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c1 = catalog(&scopes);
    let mut b = CatalogBuilder::new();
    b.add(scopes[0].clone(), "s1").unwrap();
    b.add(scopes[1].clone(), "s0").unwrap();
    let c2 = b.build();
    let x = listed_execution(&c1, &scopes, ("a1", "z1"), 0, &["s0-A"], None);
    let y = listed_execution(&c2, &scopes, ("a1", "z1"), 1, &["s0-A"], None);
    for c in [&c1, &c2] {
        for members in [
            [&x.members()[0], &y.members()[1]],
            [&y.members()[1], &x.members()[0]],
        ] {
            rejection(
                Selection::WindowsOnly,
                refs(Selection::WindowsOnly),
                &members,
                c,
                AdaptCode::CatalogAuthorityMismatch,
            );
        }
    }
}

#[test]
fn authority_is_a_whole_membership_pass_before_any_declared_overlap_pair() {
    let scopes = [
        windows("f1", "v1"),
        windows("f2", "v1"),
        windows("f3", "v1"),
    ];
    let c = pair_catalog(&scopes, true);
    let other = catalog(&scopes[2..]);
    let x = execution(&scopes[..2], &c, "a1", "z1", 1, 1);
    let y = execution(&scopes[2..], &other, "a2", "z2", 1, 1);
    let all = [&x.members()[0], &x.members()[1], &y.members()[0]];
    for p in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        rejection(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &p.map(|i| all[i]),
            &c,
            AdaptCode::CatalogAuthorityMismatch,
        );
    }
}

#[test]
fn common_authority_second_listing_repetition_keeps_initial_population() {
    let scopes = [windows("f1", "v1"), windows("f2", "v1")];
    let c = catalog(&scopes);
    let x = listed_execution(
        &c,
        &scopes,
        ("a1", "z1"),
        0,
        &["s0-A"],
        Some(&["s0-A", "s0-B"]),
    );
    let y = listed_execution(
        &c,
        &scopes,
        ("a1", "z1"),
        1,
        &["s1-A"],
        Some(&["s1-A", "s1-B"]),
    );
    for members in [
        [&x.members()[0], &y.members()[1]],
        [&y.members()[1], &x.members()[0]],
    ] {
        let m = ProjectionMembership::validate(
            Selection::WindowsOnly,
            refs(Selection::WindowsOnly),
            &members,
            &c,
        )
        .unwrap();
        assert_eq!(
            m.members().iter().map(|m| m.sources().len()).sum::<usize>(),
            2
        );
        assert!(m.members().iter().all(|m| m.records().is_empty()));
        assert!(m.members().iter().all(|m| matches!(m.second_enumeration(), SecondEnumeration::Performed { new_sources, .. } if new_sources.len() == 1)));
    }
}
