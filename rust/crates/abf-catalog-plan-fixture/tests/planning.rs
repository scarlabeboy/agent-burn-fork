use abf_catalog_plan_fixture::*;
use abf_source_run::{CatalogScope, EnvironmentKind};

fn scope(family: &str, layout: &str) -> CatalogScope {
    CatalogScope::new(EnvironmentKind::Windows, "fictionalW", family, layout).unwrap()
}
fn descriptor(family: &str) -> FixtureDescriptor<'_> {
    FixtureDescriptor {
        id: family,
        schema: 1,
        revision: "r1",
        scope: scope(family, "fixture-v1"),
        rule: "rule1",
        scope_tag: family,
        approved_classes: &[RootClass::Configured],
        overlaps: None,
    }
}
fn candidate<'a>(id: &'a str, family: &'a str, locator: &'a str) -> FictionalCandidate<'a> {
    FictionalCandidate {
        id,
        family,
        namespace: "fictionalW",
        revision: "c1",
        class: RootClass::Configured,
        locator,
        recognition: None,
    }
}
fn grant<'a>(id: &'a str, family: &'a str) -> FictionalGrant<'a> {
    FictionalGrant {
        candidate: id,
        family,
        namespace: "fictionalW",
        candidate_revision: "c1",
        class: RootClass::Configured,
        descriptor: family,
        descriptor_revision: "r1",
    }
}
fn recognition(descriptor: &str) -> RecognitionClaim<'_> {
    RecognitionClaim {
        descriptor,
        descriptor_revision: "r1",
        rule: "rule1",
    }
}
fn run<'a>(
    families: &[&str],
    d: &[FixtureDescriptor<'a>],
    c: &[FictionalCandidate<'a>],
    g: &[FictionalGrant<'a>],
) -> Result<FixturePlan, PlanCode> {
    plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalW",
            families,
        },
        d,
        c,
        g,
    )
}
#[test]
fn state_matrix_never_infers_inventory() {
    let d = [descriptor("f1")];
    let mut c = candidate("candidate1", "f1", r"C:\Fixture\Root");
    let g = [grant("candidate1", "f1")];
    let p = run(&["f1"], &d, &[], &[]).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Unresolved);
    assert!(p.decisions()[0].missing().candidate);
    assert_eq!(p.catalog().scopes().count(), 0);
    let p = run(&["f1"], &d, std::slice::from_ref(&c), &[]).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Unresolved);
    assert!(p.decisions()[0].missing().grant);
    assert!(p.decisions()[0].missing().recognition);
    let p = run(&["f1"], &d, std::slice::from_ref(&c), &g).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Unresolved);
    c.recognition = Some(&[]);
    let p = run(&["f1"], &d, std::slice::from_ref(&c), &g).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Unrecognized);
    assert_eq!(p.catalog().scopes().count(), 0);
    let claims = [recognition("f1")];
    c.recognition = Some(&claims);
    let p = run(&["f1"], &d, std::slice::from_ref(&c), &[]).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Unresolved);
    let p = run(&["f1"], &d, &[c], &g).unwrap();
    assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
    assert_eq!(p.catalog().scopes().count(), 1);
    assert_eq!(p.requirements().len(), 8);
    assert_eq!(p.outcome(), PlanOutcome::RequestedWork);
    let p = run(&[], &d, &[], &[]).unwrap();
    assert_eq!(p.outcome(), PlanOutcome::NoSelectedWork);
    assert!(p.decisions().is_empty());
    assert_eq!(p.catalog().scopes().count(), 0);
}

#[test]
fn fixed_validation_codes_and_no_partial_plan() {
    let d = descriptor("f1");
    let c = candidate("c1", "f1", r"C:\Fixture\Root");
    let g = grant("c1", "f1");
    let mut bad = d.clone();
    bad.schema = 2;
    assert_eq!(
        run(&["f1"], &[bad], &[], &[]).unwrap_err(),
        PlanCode::UnsupportedSchema
    );
    let mut bad = d.clone();
    bad.scope = scope("f1", "future");
    assert_eq!(
        run(&["f1"], &[bad], &[], &[]).unwrap_err(),
        PlanCode::UnsupportedLayout
    );
    assert_eq!(
        run(&["unknown"], std::slice::from_ref(&d), &[], &[]).unwrap_err(),
        PlanCode::UnknownFamily
    );
    let mut bad = d.clone();
    bad.scope = CatalogScope::new(
        EnvironmentKind::SelectedWsl,
        "fictionalW",
        "f1",
        "fixture-v1",
    )
    .unwrap();
    assert_eq!(
        run(&["f1"], &[bad], &[], &[]).unwrap_err(),
        PlanCode::NamespaceMismatch
    );
    let mut bad = c.clone();
    bad.namespace = "foreign";
    assert_eq!(
        run(&["f1"], std::slice::from_ref(&d), &[bad], &[]).unwrap_err(),
        PlanCode::NamespaceMismatch
    );
    assert_eq!(
        run(&["f1"], &[d.clone(), d.clone()], &[], &[]).unwrap_err(),
        PlanCode::DuplicateDescriptor
    );
    let mut same_scope = d.clone();
    same_scope.id = "other";
    same_scope.scope_tag = "other";
    assert_eq!(
        run(&["f1"], &[d.clone(), same_scope], &[], &[]).unwrap_err(),
        PlanCode::DuplicateDescriptor
    );
    assert_eq!(
        run(&["f1", "f1"], std::slice::from_ref(&d), &[], &[]).unwrap_err(),
        PlanCode::DuplicateRequest
    );
    let mut tag = descriptor("f2");
    tag.scope_tag = "f1";
    assert_eq!(
        run(&["f1"], &[d.clone(), tag], &[], &[]).unwrap_err(),
        PlanCode::DuplicateTag
    );
    assert_eq!(
        run(
            &["f1"],
            std::slice::from_ref(&d),
            &[c.clone(), c.clone()],
            &[]
        )
        .unwrap_err(),
        PlanCode::DuplicateCandidate
    );
    assert_eq!(
        run(
            &["f1"],
            std::slice::from_ref(&d),
            std::slice::from_ref(&c),
            &[g.clone(), g.clone()]
        )
        .unwrap_err(),
        PlanCode::DuplicateGrant
    );
    for bad in [
        FictionalGrant {
            namespace: "foreign",
            ..g.clone()
        },
        FictionalGrant {
            candidate_revision: "old",
            ..g.clone()
        },
        FictionalGrant {
            descriptor_revision: "old",
            ..g.clone()
        },
        FictionalGrant {
            family: "other",
            ..g.clone()
        },
        FictionalGrant {
            class: RootClass::Relocated,
            ..g.clone()
        },
        FictionalGrant {
            candidate: "other",
            ..g.clone()
        },
        FictionalGrant {
            descriptor: "other",
            ..g.clone()
        },
    ] {
        assert_eq!(
            run(
                &["f1"],
                std::slice::from_ref(&d),
                std::slice::from_ref(&c),
                &[bad]
            )
            .unwrap_err(),
            PlanCode::GrantBinding
        );
    }
    let mut bad = c.clone();
    bad.class = RootClass::Default;
    assert_eq!(
        run(&["f1"], std::slice::from_ref(&d), &[bad], &[]).unwrap_err(),
        PlanCode::ClassDenied
    );
    let mut bad = d.clone();
    bad.overlaps = Some("missing");
    assert_eq!(
        run(&["f1"], &[bad], &[], &[]).unwrap_err(),
        PlanCode::IdentifierBinding
    );
    let matches = [recognition("f1"), recognition("f1")];
    let mut bad = c.clone();
    bad.recognition = Some(&matches);
    assert_eq!(
        run(&["f1"], std::slice::from_ref(&d), &[bad], &[g]).unwrap_err(),
        PlanCode::AmbiguousRecognition
    );
    let mut c2 = c.clone();
    c2.id = "c2";
    c2.locator = r"D:\Fixture\Root";
    assert_eq!(
        run(&["f1"], std::slice::from_ref(&d), &[c.clone(), c2], &[]).unwrap_err(),
        PlanCode::MultipleRoots
    );
    let matches = [RecognitionClaim {
        rule: "foreign",
        ..recognition("f1")
    }];
    let mut bad = c;
    bad.recognition = Some(&matches);
    assert_eq!(
        run(&["f1"], &[d], &[bad], &[]).unwrap_err(),
        PlanCode::RecognitionBinding
    );
}

#[test]
fn unselected_input_is_still_structurally_validated() {
    let d = [descriptor("f1")];
    let bad = candidate("unselected", "f1", r"C:relative");
    assert_eq!(
        run(&[], &d, &[bad], &[]).unwrap_err(),
        PlanCode::InvalidLocator
    );
    let bad = candidate("bad!", "f1", r"C:\Fixture\Root");
    assert_eq!(
        run(&[], &d, &[bad], &[]).unwrap_err(),
        PlanCode::InvalidShape
    );
    let bad = FictionalGrant {
        candidate_revision: "stale",
        ..grant("c1", "f1")
    };
    let c = candidate("c1", "f1", r"C:\Fixture\Root");
    assert_eq!(
        run(&[], &d, &[c], &[bad]).unwrap_err(),
        PlanCode::GrantBinding
    );
}

#[test]
fn accepted_fictional_locator_examples() {
    for text in [
        r"C:\Fixture\Root",
        "z:/Fixture/Root",
        r"D:\Fixture/Root-1_2.3",
        r"C:\Fixture Names\Root",
        r"C:\Fixture\COM0",
        r"C:\Fixture\COM10",
        r"C:\Fixture\Console",
    ] {
        assert!(
            run(
                &["f1"],
                &[descriptor("f1")],
                &[candidate("c1", "f1", text)],
                &[]
            )
            .is_ok(),
            "fictional example rejected"
        );
    }
}
#[test]
fn refused_fictional_locator_examples() {
    for text in [
        "",
        "C:",
        r"C:\",
        "C:relative",
        r"\Fixture\Root",
        "relative",
        r"C:\Fixture\.\Root",
        r"C:\Fixture\..\Root",
        r"C:\\Fixture\Root",
        r"C:\Fixture\Root\",
        r"\\server\share\Root",
        r"\\wsl$\Fictional\Root",
        r"\\?\C:\Fixture\Root",
        r"\\.\C:\Fixture\Root",
        r"C:\Fixture\Root:stream",
        r"C:\Fixture\CON",
        r"C:\Fixture\aux.json",
        r"C:\Fixture\lPt9.txt",
        r"C:\Fixture\COM1.foo",
        r"C:\Fixture\Root.",
        "C:\\Fixture\\Root ",
        "C:/Fixture/Rööt",
        "C:/Fixture/R\u{1}",
        "1:/Fixture/Root",
        "CC:/Fixture/Root",
        "C:/Fixture/R*",
        "C:/Fixture/R?",
        "C:/Fixture/R%",
        "C:/Fixture/R|",
        "C:/Fixture/R<",
        "C:/Fixture/R\"",
        "C:/Fixture/R\u{7f}",
    ] {
        assert!(
            matches!(
                run(
                    &["f1"],
                    &[descriptor("f1")],
                    &[candidate("c1", "f1", text)],
                    &[]
                ),
                Err(PlanCode::InvalidShape | PlanCode::InvalidLocator)
            ),
            "fictional invalid example accepted"
        );
    }
}
#[test]
fn alias_and_overlap_are_refusals_but_sibling_prefix_is_not_containment() {
    let d = [descriptor("f1"), descriptor("f2")];
    let c1 = candidate("c1", "f1", r"C:\Fixture\Root");
    for locator in [
        r"c:\fixture\root",
        "C:/Fixture/Root",
        r"C:\Fixture\Root\Child",
    ] {
        let c2 = candidate("c2", "f2", locator);
        let expected = if locator.ends_with("Child") {
            PlanCode::OverlapConflict
        } else {
            PlanCode::AliasCollision
        };
        assert_eq!(
            run(&["f1", "f2"], &d, &[c1.clone(), c2], &[]).unwrap_err(),
            expected
        );
    }
    let c2 = candidate("c2", "f2", r"C:\Fixture\RootSibling");
    assert!(run(&["f1", "f2"], &d, &[c1, c2], &[]).is_ok());
    let mut overlap = descriptor("f1");
    overlap.overlaps = Some("f2");
    assert_eq!(
        run(&["f1", "f2"], &[overlap, descriptor("f2")], &[], &[]).unwrap_err(),
        PlanCode::OverlapConflict
    );
}
#[test]
fn unique_layout_match_is_allowed_but_multi_layout_is_refused() {
    let mut d2 = descriptor("f1");
    d2.id = "layout2";
    d2.scope_tag = "layout2";
    d2.scope = scope("f1", "fixture-v2");
    let d = [descriptor("f1"), d2];
    let claims = [recognition("f1")];
    let mut c = candidate("c1", "f1", r"C:\Fixture\Root");
    c.recognition = Some(&claims);
    assert_eq!(
        run(&["f1"], &d, std::slice::from_ref(&c), &[grant("c1", "f1")])
            .unwrap()
            .catalog()
            .scopes()
            .count(),
        1
    );
    let claims = [recognition("f1"), recognition("layout2")];
    c.recognition = Some(&claims);
    assert_eq!(
        run(&["f1"], &d, &[c], &[]).unwrap_err(),
        PlanCode::AmbiguousRecognition
    );
}
#[test]
fn logical_output_and_refusals_are_order_independent() {
    let d = [descriptor("f2"), descriptor("f1")];
    let m1 = [recognition("f1")];
    let m2 = [recognition("f2")];
    let c = [
        FictionalCandidate {
            recognition: Some(&m2),
            ..candidate("c2", "f2", r"D:\Fixture\Root")
        },
        FictionalCandidate {
            recognition: Some(&m1),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        },
    ];
    let g = [grant("c2", "f2"), grant("c1", "f1")];
    let p = run(&["f2", "f1"], &d, &c, &g).unwrap();
    let q = run(
        &["f1", "f2"],
        &[d[1].clone(), d[0].clone()],
        &[c[1].clone(), c[0].clone()],
        &[g[1].clone(), g[0].clone()],
    )
    .unwrap();
    assert_eq!(p.decisions(), q.decisions());
    assert_eq!(p.decisions()[0].family(), "f1");
    assert_eq!(p.catalog(), q.catalog()); // content equality is NOT authority equality
    assert_eq!(
        p.catalog().scope_tag(&scope("f1", "fixture-v1")),
        Some("f1")
    );
    let mut a = d[0].clone();
    a.schema = 2;
    let mut b = d[1].clone();
    b.scope = scope("f1", "unsupported");
    assert_eq!(
        run(&[], &[a.clone(), b.clone()], &[], &[]).unwrap_err(),
        PlanCode::UnsupportedSchema
    );
    assert_eq!(
        run(&[], &[b, a], &[], &[]).unwrap_err(),
        PlanCode::UnsupportedSchema
    );
    let mut bad1 = c[0].clone();
    bad1.locator = "C:bad";
    let mut bad2 = c[1].clone();
    bad2.class = RootClass::Default;
    for cs in [[bad1.clone(), bad2.clone()], [bad2, bad1]] {
        assert_eq!(
            run(&["f1", "f2"], &d, &cs, &g).unwrap_err(),
            PlanCode::InvalidLocator
        );
    }
}

#[test]
fn collection_limits_boundary_minus_one_boundary_plus_one() {
    for n in [15, 16, 17] {
        let labels: Vec<_> = (0..n).map(|i| format!("f{i}")).collect();
        let d: Vec<_> = labels.iter().map(|f| descriptor(f)).collect();
        let req: Vec<_> = labels.iter().map(String::as_str).collect();
        let locators: Vec<_> = (0..n).map(|i| format!("C:/Fixture/Root{i}")).collect();
        let c: Vec<_> = labels
            .iter()
            .zip(&locators)
            .map(|(f, l)| candidate(f, f, l))
            .collect();
        let g: Vec<_> = labels.iter().map(|f| grant(f, f)).collect();
        for result in [
            run(&[], &d, &[], &[]),
            run(&req, &d[..n.min(16)], &[], &[]),
            run(&[], &d[..n.min(16)], &c, &[]),
            run(&[], &d[..n.min(16)], &c[..n.min(16)], &g),
        ] {
            assert_eq!(
                result.as_ref().err().copied(),
                (n == 17).then_some(PlanCode::Budget)
            );
        }
    }
}
#[test]
fn recognition_limits_one_two_three() {
    let d = [descriptor("f1")];
    for (n, expected) in [
        (1, None),
        (2, Some(PlanCode::AmbiguousRecognition)),
        (3, Some(PlanCode::Budget)),
    ] {
        let claims = vec![recognition("f1"); n];
        let mut c = candidate("c1", "f1", r"C:\Fixture\Root");
        c.recognition = Some(&claims);
        assert_eq!(
            run(&["f1"], &d, &[c], &[grant("c1", "f1")])
                .as_ref()
                .err()
                .copied(),
            expected
        );
    }
}
#[test]
fn locator_and_component_limits() {
    for n in [511, 512, 513] {
        let text = format!("C:/{}", "R".repeat(n - 3));
        assert_eq!(
            run(
                &["f1"],
                &[descriptor("f1")],
                &[candidate("c1", "f1", &text)],
                &[]
            )
            .as_ref()
            .err()
            .copied(),
            (n == 513).then_some(PlanCode::Budget)
        );
    }
    for n in [15, 16, 17] {
        let text = format!("C:/{}", vec!["R"; n].join("/"));
        assert_eq!(
            run(
                &["f1"],
                &[descriptor("f1")],
                &[candidate("c1", "f1", &text)],
                &[]
            )
            .as_ref()
            .err()
            .copied(),
            (n == 17).then_some(PlanCode::Budget)
        );
    }
}
#[test]
fn total_locator_bytes_8191_8192_8193_are_not_truncated() {
    let labels: Vec<_> = (0..16).map(|i| format!("f{i}")).collect();
    let d: Vec<_> = labels.iter().map(|f| descriptor(f)).collect();
    for total in [8191, 8192, 8193] {
        let mut text: Vec<_> = (0..16).map(|_| format!("C:/{}", "R".repeat(509))).collect();
        text[0] = format!("C:/{}", "R".repeat(total - 15 * 512 - 3));
        let c: Vec<_> = labels
            .iter()
            .zip(&text)
            .map(|(f, l)| candidate(f, f, l))
            .collect();
        assert_eq!(
            run(&[], &d, &c, &[]).as_ref().err().copied(),
            (total == 8193).then_some(PlanCode::Budget)
        );
    }
}
#[test]
fn opaque_tag_and_class_shape_bounds_and_privacy() {
    let secret = "FictionalSecret";
    let mut d = descriptor(secret);
    let c = candidate(secret, secret, r"C:\FictionalSecret\Root");
    let g = grant(secret, secret);
    let request = WindowsRequest {
        namespace: "fictionalW",
        families: &[secret],
    };
    let claim = recognition(secret);
    let p = plan_windows_fixture(
        &request,
        std::slice::from_ref(&d),
        std::slice::from_ref(&c),
        std::slice::from_ref(&g),
    )
    .unwrap();
    for debug in [
        format!("{d:?}"),
        format!("{c:?}"),
        format!("{g:?}"),
        format!("{request:?}"),
        format!("{claim:?}"),
        format!("{p:?}"),
        format!("{:?}", p.decisions()),
        format!("{:?}", PlanCode::GrantBinding),
    ] {
        assert!(!debug.contains(secret));
        assert!(!debug.contains("C:"));
    }
    d.scope_tag = "bad_tag";
    assert_eq!(
        run(&[], &[d], &[], &[]).unwrap_err(),
        PlanCode::InvalidShape
    );
    for length in [63, 64, 65] {
        let id = "X".repeat(length);
        let c = candidate(&id, secret, r"C:\Fixture\Root");
        assert_eq!(
            run(&[], &[descriptor(secret)], &[c], &[])
                .as_ref()
                .err()
                .copied(),
            (length == 65).then_some(PlanCode::InvalidShape)
        );
    }
    for length in [15, 16, 17] {
        let tag = "X".repeat(length);
        let mut d = descriptor(secret);
        d.scope_tag = &tag;
        assert_eq!(
            run(&[], &[d], &[], &[]).as_ref().err().copied(),
            (length == 17).then_some(PlanCode::InvalidShape)
        );
    }
}

#[test]
fn sixteen_full_conditional_claims_preserve_all_8192_locator_bytes() {
    let labels: Vec<_> = (0..16).map(|i| format!("f{i}")).collect();
    let d: Vec<_> = labels.iter().map(|f| descriptor(f)).collect();
    let families: Vec<_> = labels.iter().map(String::as_str).collect();
    let locators: Vec<_> = (0..16)
        .map(|i| format!("C:/{}", char::from(b'A' + i).to_string().repeat(509)))
        .collect();
    let claims: Vec<_> = labels.iter().map(|f| [recognition(f)]).collect();
    let c: Vec<_> = labels
        .iter()
        .zip(&locators)
        .zip(&claims)
        .map(|((f, l), m)| FictionalCandidate {
            recognition: Some(m),
            ..candidate(f, f, l)
        })
        .collect();
    let g: Vec<_> = labels.iter().map(|f| grant(f, f)).collect();
    let p = run(&families, &d, &c, &g).unwrap();
    assert_eq!(p.namespace(), "fictionalW");
    assert_eq!(p.decisions().len(), 16);
    assert_eq!(p.catalog().scopes().count(), 16);
    assert!(
        p.decisions()
            .iter()
            .all(|d| d.state() == DecisionState::Conditional)
    );
    assert_eq!(
        p.decisions()
            .iter()
            .map(|d| d.root_claim().unwrap().locator().len())
            .sum::<usize>(),
        8192
    );
    for decision in p.decisions() {
        assert_eq!(
            p.catalog().scope_tag(decision.conditional_scope().unwrap()),
            Some(decision.family())
        );
        let root = decision.root_claim().unwrap();
        assert_eq!(root.candidate(), decision.family());
        assert_eq!(root.descriptor(), Some(decision.family()));
        assert_eq!(root.descriptor_revision(), Some("r1"));
        assert_eq!(root.recognition_rule(), Some("rule1"));
        assert_eq!(root.revision(), "c1");
        assert_eq!(root.class(), RootClass::Configured);
        assert!(root.grant_present());
    }
}
#[test]
fn reserved_stems_with_space_before_extension_are_refused() {
    for text in [
        "C:/Fixture/CON .txt",
        "C:/Fixture/aux  .log",
        "C:/Fixture/LPT1 .json",
    ] {
        assert_eq!(
            run(
                &[],
                &[descriptor("f1")],
                &[candidate("c1", "f1", text)],
                &[]
            )
            .unwrap_err(),
            PlanCode::InvalidLocator
        );
    }
}
#[test]
fn classes_require_explicit_matching_claims_even_for_defaults() {
    for class in [
        RootClass::Default,
        RootClass::Configured,
        RootClass::Relocated,
    ] {
        let mut d = descriptor("f1");
        let classes = [class];
        d.approved_classes = &classes;
        let m = [recognition("f1")];
        let c = FictionalCandidate {
            class,
            recognition: Some(&m),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        assert_eq!(
            run(
                &["f1"],
                std::slice::from_ref(&d),
                std::slice::from_ref(&c),
                &[]
            )
            .unwrap()
            .decisions()[0]
                .state(),
            DecisionState::Unresolved
        );
        let g = FictionalGrant {
            class,
            ..grant("c1", "f1")
        };
        let p = run(&["f1"], &[d], &[c], &[g]).unwrap();
        assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
        assert!(p.requirements().contains(&NativeRequirement::ActualConsent));
    }
}
#[test]
fn shape_and_binding_refusals_precede_selection_and_evidence_absence() {
    let mut d = descriptor("f1");
    d.approved_classes = &[];
    assert_eq!(
        run(&[], &[d], &[], &[]).unwrap_err(),
        PlanCode::InvalidShape
    );
    let mut d = descriptor("f1");
    d.approved_classes = &[RootClass::Configured, RootClass::Configured];
    assert_eq!(
        run(&[], &[d], &[], &[]).unwrap_err(),
        PlanCode::InvalidShape
    );
    let mut d = descriptor("f1");
    d.approved_classes = &[
        RootClass::Default,
        RootClass::Configured,
        RootClass::Relocated,
        RootClass::Configured,
    ];
    assert_eq!(run(&[], &[d], &[], &[]).unwrap_err(), PlanCode::Budget);
    let mut c = candidate("c1", "f1", r"C:\Fixture\Root");
    c.family = "unknown";
    assert_eq!(
        run(&[], &[descriptor("f1")], &[c], &[]).unwrap_err(),
        PlanCode::IdentifierBinding
    );
    let m = [RecognitionClaim {
        descriptor_revision: "stale",
        ..recognition("f1")
    }];
    let c = FictionalCandidate {
        recognition: Some(&m),
        ..candidate("c1", "f1", r"C:\Fixture\Root")
    };
    assert_eq!(
        run(&[], &[descriptor("f1")], &[c], &[]).unwrap_err(),
        PlanCode::RecognitionBinding
    );
}
#[test]
fn independent_permutations_preserve_all_bindings_and_claims() {
    let d = [descriptor("f1"), descriptor("f2")];
    let m = [[recognition("f1")], [recognition("f2")]];
    let c = [
        FictionalCandidate {
            recognition: Some(&m[0]),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        },
        FictionalCandidate {
            recognition: Some(&m[1]),
            ..candidate("c2", "f2", r"D:\Fixture\Root")
        },
    ];
    let g = [grant("c1", "f1"), grant("c2", "f2")];
    let expected = run(&["f1", "f2"], &d, &c, &g).unwrap();
    for bits in 0..16 {
        let mut ds = d.clone();
        let mut cs = c.clone();
        let mut gs = g.clone();
        let mut fs = ["f1", "f2"];
        if bits & 1 != 0 {
            ds.reverse();
        }
        if bits & 2 != 0 {
            cs.reverse();
        }
        if bits & 4 != 0 {
            gs.reverse();
        }
        if bits & 8 != 0 {
            fs.reverse();
        }
        let p = run(&fs, &ds, &cs, &gs).unwrap();
        assert_eq!(p.decisions(), expected.decisions());
        assert_eq!(p.catalog(), expected.catalog());
        cs[0].locator = "C:invalid";
        gs[0].candidate_revision = "stale";
        assert_eq!(
            run(&fs, &ds, &cs, &gs).unwrap_err(),
            PlanCode::InvalidLocator
        );
    }
}

#[test]
fn every_nonconditional_decision_is_excluded_even_with_a_unique_match() {
    let d = [descriptor("f1")];
    let claims = [recognition("f1")];
    for recognition in [None, Some(&[][..]), Some(&claims[..])] {
        let c = FictionalCandidate {
            recognition,
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        let p = run(&["f1"], &d, &[c], &[]).unwrap();
        assert_ne!(p.decisions()[0].state(), DecisionState::Conditional);
        assert!(p.decisions()[0].conditional_scope().is_none());
        assert_eq!(p.catalog().scopes().count(), 0);
        assert!(!format!("{:?}", p.decisions()[0].root_claim().unwrap()).contains("Fixture"));
    }
}
#[test]
fn valid_conditional_prefix_cannot_escape_a_later_structural_conflict() {
    let d = [descriptor("f1"), descriptor("f2")];
    let m = [recognition("f1")];
    let valid = FictionalCandidate {
        recognition: Some(&m),
        ..candidate("c1", "f1", r"C:\Fixture\Root")
    };
    let invalid = candidate("c2", "f2", "D:relative");
    let g = [grant("c1", "f1")];
    for cs in [[valid.clone(), invalid.clone()], [invalid, valid]] {
        assert!(matches!(
            run(&["f1", "f2"], &d, &cs, &g),
            Err(PlanCode::InvalidLocator)
        ));
    }
}

#[test]
fn explicit_ambiguous_recognition_is_refused_even_when_unselected() {
    let claims = [recognition("f1"), recognition("f1")];
    let c = FictionalCandidate {
        recognition: Some(&claims),
        ..candidate("c1", "f1", r"C:\Fixture\Root")
    };
    assert_eq!(
        run(&[], &[descriptor("f1")], &[c], &[]).unwrap_err(),
        PlanCode::AmbiguousRecognition
    );
}

#[test]
fn spaces_are_internal_only_in_the_fixture_grammar() {
    for locator in ["C:/ Fixture/Root", "C:/Fixture/ Root"] {
        assert_eq!(
            run(
                &[],
                &[descriptor("f1")],
                &[candidate("c1", "f1", locator)],
                &[]
            )
            .unwrap_err(),
            PlanCode::InvalidLocator
        );
    }
}

#[test]
fn grant_and_recognition_must_agree_on_the_selected_descriptor() {
    let v1 = descriptor("f1");
    let v2 = FixtureDescriptor {
        id: "layout2",
        revision: "r2",
        scope: scope("f1", "fixture-v2"),
        scope_tag: "layout2",
        rule: "rule2",
        ..descriptor("f1")
    };
    let matches_v1 = [recognition("f1")];
    let matches_v2 = [RecognitionClaim {
        descriptor: "layout2",
        descriptor_revision: "r2",
        rule: "rule2",
    }];
    let grant_v1 = grant("c1", "f1");
    let grant_v2 = FictionalGrant {
        descriptor: "layout2",
        descriptor_revision: "r2",
        ..grant("c1", "f1")
    };
    for descriptors in [[v1.clone(), v2.clone()], [v2, v1]] {
        let mismatched = FictionalCandidate {
            recognition: Some(&matches_v1),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        assert_eq!(
            run(
                &["f1"],
                &descriptors,
                &[mismatched],
                std::slice::from_ref(&grant_v2)
            )
            .err(),
            Some(PlanCode::GrantBinding),
            "X1: a grant for another valid descriptor must refuse the whole plan"
        );
        for (claims, matching_grant, layout, tag) in [
            (&matches_v1[..], &grant_v1, "fixture-v1", "f1"),
            (&matches_v2[..], &grant_v2, "fixture-v2", "layout2"),
        ] {
            let c = FictionalCandidate {
                recognition: Some(claims),
                ..candidate("c1", "f1", r"C:\Fixture\Root")
            };
            let p = run(
                &["f1"],
                &descriptors,
                &[c],
                std::slice::from_ref(matching_grant),
            )
            .unwrap();
            assert_eq!(p.decisions().len(), 1);
            assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
            let expected = scope("f1", layout);
            assert!(p.decisions()[0].conditional_scope() == Some(&expected));
            assert_eq!(p.catalog().scopes().count(), 1);
            assert!(p.catalog().scope_tag(&expected) == Some(tag));
        }
    }
}

#[test]
fn recognition_must_bind_both_the_candidate_class_and_family() {
    let approved = descriptor("f1");
    let class_excluding = FixtureDescriptor {
        id: "defaultLayout",
        scope: scope("f1", "fixture-v2"),
        scope_tag: "defaultLayout",
        approved_classes: &[RootClass::Default],
        ..descriptor("f1")
    };
    let excluded_match = [recognition("defaultLayout")];
    for descriptors in [
        [approved.clone(), class_excluding.clone()],
        [class_excluding, approved.clone()],
    ] {
        let c = FictionalCandidate {
            recognition: Some(&excluded_match),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        // No grant: the recognition-specific check must independently refuse.
        assert_eq!(
            run(&["f1"], &descriptors, &[c], &[]).err(),
            Some(PlanCode::RecognitionBinding),
            "X2: another descriptor approving the class cannot validate this recognition"
        );
        let matching = [recognition("f1")];
        let c = FictionalCandidate {
            recognition: Some(&matching),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        let p = run(&["f1"], &descriptors, &[c], &[grant("c1", "f1")]).unwrap();
        assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
        assert!(p.decisions()[0].conditional_scope() == Some(&scope("f1", "fixture-v1")));
        assert_eq!(p.catalog().scopes().count(), 1);
    }

    let foreign = descriptor("f2");
    let foreign_match = [recognition("f2")];
    for descriptors in [[approved.clone(), foreign.clone()], [foreign, approved]] {
        let c = FictionalCandidate {
            recognition: Some(&foreign_match),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        assert_eq!(
            run(&["f1"], &descriptors, &[c], &[]).err(),
            Some(PlanCode::RecognitionBinding),
            "X3: a coherent descriptor of another family cannot validate this recognition"
        );
        let matching = [recognition("f1")];
        let c = FictionalCandidate {
            recognition: Some(&matching),
            ..candidate("c1", "f1", r"C:\Fixture\Root")
        };
        let p = run(&["f1"], &descriptors, &[c], &[grant("c1", "f1")]).unwrap();
        assert_eq!(p.decisions().len(), 1);
        assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
        assert!(p.decisions()[0].conditional_scope() == Some(&scope("f1", "fixture-v1")));
        assert_eq!(p.catalog().scopes().count(), 1);
    }
}

#[test]
fn lexical_ancestor_overlap_refuses_child_first_and_parent_first() {
    let descriptors = [descriptor("f1"), descriptor("f2")];
    let child_match = [recognition("f1")];
    let parent_match = [recognition("f2")];
    let child = FictionalCandidate {
        recognition: Some(&child_match),
        ..candidate("child", "f1", r"C:\Fixture\Root\Child")
    };
    let parent = FictionalCandidate {
        recognition: Some(&parent_match),
        ..candidate("parent", "f2", r"C:\Fixture\Root")
    };
    let grants = [grant("child", "f1"), grant("parent", "f2")];
    for candidates in [
        [child.clone(), parent.clone()],
        [parent.clone(), child.clone()],
    ] {
        assert_eq!(
            run(&["f1", "f2"], &descriptors, &candidates, &grants).err(),
            Some(PlanCode::OverlapConflict),
            "X4: lexical nesting must refuse in either candidate order"
        );
    }
    let sibling = FictionalCandidate {
        locator: r"C:\Fixture\RootSibling",
        ..child
    };
    for candidates in [[sibling.clone(), parent.clone()], [parent, sibling]] {
        let p = run(&["f1", "f2"], &descriptors, &candidates, &grants).unwrap();
        assert_eq!(p.decisions().len(), 2);
        assert!(
            p.decisions()
                .iter()
                .all(|d| d.state() == DecisionState::Conditional)
        );
        assert_eq!(p.catalog().scopes().count(), 2);
        assert!(p.catalog().scope_tag(&scope("f1", "fixture-v1")) == Some("f1"));
        assert!(p.catalog().scope_tag(&scope("f2", "fixture-v1")) == Some("f2"));
    }
}

#[test]
fn declared_overlap_requires_the_peer_family_to_be_requested() {
    let declaring = FixtureDescriptor {
        overlaps: Some("f2"),
        ..descriptor("f1")
    };
    let peer = descriptor("f2");
    let matching = [recognition("f1")];
    let c = FictionalCandidate {
        recognition: Some(&matching),
        ..candidate("c1", "f1", r"C:\Fixture\Root")
    };
    let grants = [grant("c1", "f1")];
    for descriptors in [[declaring.clone(), peer.clone()], [peer, declaring]] {
        let result = run(&["f1"], &descriptors, std::slice::from_ref(&c), &grants);
        assert_eq!(
            result.as_ref().err().copied(),
            None,
            "X5: an unrequested peer must not cause declared-overlap refusal"
        );
        let p = result.unwrap();
        assert_eq!(p.outcome(), PlanOutcome::RequestedWork);
        assert_eq!(p.decisions().len(), 1);
        assert!(p.decisions()[0].family() == "f1");
        assert_eq!(p.decisions()[0].state(), DecisionState::Conditional);
        assert!(p.decisions()[0].conditional_scope() == Some(&scope("f1", "fixture-v1")));
        assert_eq!(p.catalog().scopes().count(), 1);
        assert!(p.catalog().scope_tag(&scope("f1", "fixture-v1")) == Some("f1"));
        assert!(p.catalog().scope_tag(&scope("f2", "fixture-v1")).is_none());
        assert_eq!(
            run(
                &["f1", "f2"],
                &descriptors,
                std::slice::from_ref(&c),
                &grants
            )
            .err(),
            Some(PlanCode::OverlapConflict)
        );
    }
}
