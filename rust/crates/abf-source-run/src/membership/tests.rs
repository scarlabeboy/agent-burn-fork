use super::*;
use crate::*;

fn fixture() -> (CatalogSnapshot, FinalizedExecution) {
    let scopes: Vec<_> = ["f1", "f2", "f3"]
        .into_iter()
        .map(|f| CatalogScope::new(EnvironmentKind::Windows, "w1", f, "v1").unwrap())
        .collect();
    let mut b = CatalogBuilder::new();
    for (i, s) in scopes.iter().enumerate() {
        b.add(s.clone(), &format!("s{i}")).unwrap();
    }
    let c = b.build();
    let attempt = AttemptRef::new("a1").unwrap();
    let x = finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: attempt.clone(),
                finalization: FinalizationId::new("z1").unwrap(),
                members: scopes,
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: (0..3)
                .map(|member| ReaderFrame {
                    attempt: attempt.clone(),
                    frame_seq: member as u64 + 1,
                    payload: Payload::Enumeration {
                        member,
                        second: false,
                        outcome: RawEnumeration::Closed(1),
                        sources: vec![format!("s{member}-i1")],
                        observed_at: None,
                    },
                })
                .collect(),
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(0, 1),
        },
        &c,
    )
    .unwrap();
    (c, x)
}
fn selected() -> SelectedRefs {
    SelectedRefs {
        windows: Some(OpaqueRef::new("w1").unwrap()),
        selected_wsl: None,
    }
}
fn refuse(c: &CatalogSnapshot, members: &[&FinalizedMember], code: AdaptCode) {
    assert_eq!(
        ProjectionMembership::validate(Selection::WindowsOnly, selected(), members, c).unwrap_err(),
        code
    );
}

#[test]
fn a7_missing_manifest_member_is_refused_before_a8_collision() {
    let (c, x) = fixture();
    let mut a = x.members()[0].clone();
    let mut b = x.members()[1].clone();
    // Private negative witness only: no public finalization can omit its member.
    a.manifest.members = vec![x.members()[2].scope.clone()];
    b.manifest = a.manifest.clone();
    b.sources[0].0 = a.sources[0].0.clone();
    for members in [[&a, &b], [&b, &a]] {
        refuse(&c, &members, AdaptCode::MemberNotInManifest);
    }
}

#[test]
fn a7_reused_attempt_precedes_missing_manifest_member_and_a8() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    b.manifest.members.clear();
    b.sources[0].0 = a.sources[0].0.clone();
    for members in [[a, &b], [&b, a]] {
        refuse(&c, &members, AdaptCode::AttemptRefReused);
    }
}

#[test]
fn a7_rejects_different_lineage_in_carried_manifest() {
    let (c, x) = fixture();
    let mut b = x.members()[1].clone();
    b.manifest.retry_of = Some(AttemptRef::new("a2").unwrap());
    refuse(&c, &[&x.members()[0], &b], AdaptCode::AttemptRefReused);
}

#[test]
fn a8_faulty_generator_collision_precedes_a8b() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    // Matrix R47/R72: emulate the disabled tag-uniqueness guard privately.
    // Never expose a way to forge these sources through the public API.
    b.sources[0].0 = a.sources[0].0.clone();
    for members in [[a, &b], [&b, a]] {
        refuse(&c, &members, AdaptCode::RunKeyCollision);
    }
}

#[test]
fn a8b_faulty_generator_distinct_attempts_keep_independent_namespace_guard() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    b.sources[0].0 = a.sources[0].0.clone();
    b.attempt = AttemptRef::new("a2").unwrap();
    b.manifest.attempt = b.attempt.clone();
    for members in [[a, &b], [&b, a]] {
        refuse(&c, &members, AdaptCode::SourceRefNamespaceConflict);
    }
}

#[test]
fn a2_is_a_global_pass_before_a3_for_every_permutation() {
    let (c, x) = fixture();
    let mut a = x.members()[0].clone();
    let mut b = x.members()[1].clone();
    a.scope = CatalogScope::new(EnvironmentKind::Windows, "old", "f1", "v1").unwrap();
    b.scope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f2", "v1").unwrap();
    for members in [[&a, &b], [&b, &a]] {
        refuse(&c, &members, AdaptCode::MemberKindNotSelected);
    }
}

#[test]
fn a4_is_a_global_pass_before_a5_for_every_permutation() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    b.scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v2").unwrap();
    let all = [a, &b, a];
    for p in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        refuse(&c, &p.map(|i| all[i]), AdaptCode::DuplicateScopeMember);
    }
}

#[test]
fn substituted_catalog_is_refused_without_changing_finalized_bindings() {
    let (c, x) = fixture();
    let m = &x.members()[0];
    let mut b = CatalogBuilder::new();
    b.add(m.scope.clone(), "other").unwrap();
    let different_tags = b.build();
    refuse(&different_tags, &[m], AdaptCode::CatalogAuthorityMismatch);
    assert_eq!(m.sources()[0].0.as_str(), "s0-i1");
    assert_eq!(c.scope_tag(&m.scope), Some("s0"));
}

// These deliberately private faulty-generator witnesses preserve one genuine
// authority. A correctly built shared catalog has unique tags, so public W2/W3
// cannot reach namespace/key collisions after ABF108D D1.
fn performed(new_sources: Vec<SourceRef>) -> SecondEnumeration {
    SecondEnumeration::Performed {
        listing: SecondListing::Closed {
            listed: new_sources.len() as u32,
        },
        new_sources,
    }
}

#[test]
fn second_enumeration_cross_scope_reuse_is_refused_independently_of_a8() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    b.second = performed(vec![a.sources[0].0.clone()]);
    for members in [[a, &b], [&b, a]] {
        refuse(&c, &members, AdaptCode::SourceRefNamespaceConflict);
    }
    assert_eq!(a.sources().len(), 1);
    assert_eq!(b.sources().len(), 1);
    assert!(b.records().is_empty());
}

#[test]
fn second_enumeration_to_second_enumeration_reuse_is_refused() {
    let (c, x) = fixture();
    let mut a = x.members()[0].clone();
    let mut b = x.members()[1].clone();
    let shared = SourceRef::new("s0-new").unwrap();
    a.second = performed(vec![shared.clone()]);
    b.second = performed(vec![shared]);
    for members in [[&a, &b], [&b, &a]] {
        refuse(&c, &members, AdaptCode::SourceRefNamespaceConflict);
    }
}

#[test]
fn same_member_listed_repetition_is_not_a_cross_scope_conflict() {
    let (c, x) = fixture();
    let mut a = x.members()[0].clone();
    let b = &x.members()[1];
    let own = a.sources[0].0.clone();
    a.second = performed(vec![own.clone(), own]);
    let m =
        ProjectionMembership::validate(Selection::WindowsOnly, selected(), &[&a, b], &c).unwrap();
    assert_eq!(m.members().len(), 2);
    assert_eq!(m.members()[0].sources().len(), 1);
}

#[test]
fn second_namespace_conflict_precedes_a9_but_does_not_create_run_keys() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    b.second = performed(vec![a.sources[0].0.clone()]);
    // Seed 129 records privately: A8b must win, not the later A9 ceiling.
    let record = RetainedRecord {
        source: b.sources[0].0.clone(),
        source_seq: 1,
        frame_seq: 1,
        delivery: DeliveryIndex(0),
        result: abf_parser::parse(b"{}", None, abf_parser::Limits::MAX),
    };
    b.records = vec![record; 129];
    for members in [[a, &b], [&b, a]] {
        refuse(&c, &members, AdaptCode::SourceRefNamespaceConflict);
    }
    assert_eq!(b.records().len(), 129);
}

#[test]
fn initial_and_second_source_text_can_repeat_across_environment_kinds() {
    let (c, x) = fixture();
    let a = &x.members()[0];
    let mut b = x.members()[1].clone();
    // Defensive key-dimension witness, using a private scope mutation only.
    b.scope = CatalogScope::new(EnvironmentKind::SelectedWsl, "l1", "f1", "v1").unwrap();
    b.attempt = AttemptRef::new("a2").unwrap();
    b.manifest.attempt = b.attempt.clone();
    b.manifest.members = vec![b.scope.clone()];
    b.sources[0].0 = a.sources[0].0.clone();
    b.second = performed(vec![SourceRef::new("s0-new").unwrap()]);
    let mut a = a.clone();
    a.second = performed(vec![SourceRef::new("s0-new").unwrap()]);
    let selected = SelectedRefs {
        windows: Some(OpaqueRef::new("w1").unwrap()),
        selected_wsl: Some(OpaqueRef::new("l1").unwrap()),
    };
    assert!(ProjectionMembership::validate(Selection::Both, selected, &[&b, &a], &c).is_ok());
}
