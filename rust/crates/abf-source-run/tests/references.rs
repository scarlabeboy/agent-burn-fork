use abf_source_run::*;

#[test]
fn r68_r69_r97_creation_listing_and_rejection_counters() {
    let mut b = CatalogBuilder::new();
    let s = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
    b.add(s.clone(), "f1v1").unwrap();
    assert_eq!(b.add(s.clone(), "f1v1"), Err(RefCode::DuplicateScopeTag));
    let catalog = b.build();
    let mut issuer = FixtureRefIssuer::new(&catalog);
    assert_eq!(issuer.create_instance(&s).unwrap().as_str(), "f1v1-i1");
    assert_eq!(issuer.create_instance(&s).unwrap().as_str(), "f1v1-i2");
    assert_eq!(
        issuer
            .create_instance_labelled(&s, "sessions")
            .unwrap()
            .as_str(),
        "f1v1-sessions"
    );
    assert_eq!(issuer.instance_counter(&s).unwrap(), 2);
    assert_eq!(
        issuer.create_instance_labelled(&s, "sessions"),
        Err(RefCode::DuplicateInstanceTag)
    );
    assert_eq!(
        issuer.create_instance_labelled(&s, "i007"),
        Err(RefCode::ReservedCounterForm)
    );
    assert_eq!(issuer.create_instance(&s).unwrap().as_str(), "f1v1-i3");
    assert_eq!(issuer.begin_execution().unwrap().as_str(), "a1");
    assert_eq!(
        issuer.begin_execution_labelled("att1").unwrap().as_str(),
        "att1"
    );
    assert_eq!(
        issuer.begin_execution_labelled("att1"),
        Err(RefCode::DuplicateAttemptRef)
    );
    assert_eq!(
        issuer.begin_execution_labelled("z3"),
        Err(RefCode::ReservedCounterForm)
    );
    assert_eq!(issuer.begin_execution().unwrap().as_str(), "a2");
    assert_eq!(issuer.issue_finalization().unwrap().as_str(), "z1");
    issuer.reset();
    assert_eq!(issuer.create_instance(&s).unwrap().as_str(), "f1v1-i1");
    assert_eq!(issuer.begin_execution().unwrap().as_str(), "a1");
    assert_eq!(issuer.issue_finalization().unwrap().as_str(), "z1");
}

#[test]
fn r68_n34_tag_opaque_and_reference_length_boundaries() {
    for n in [15, 16, 17] {
        let mut b = CatalogBuilder::new();
        let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
        assert_eq!(b.add(scope, &"s".repeat(n)).is_ok(), n <= 16);
    }
    for n in [46, 47, 48] {
        let mut b = CatalogBuilder::new();
        let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
        b.add(scope.clone(), "abcdefghijklmnop").unwrap();
        let mut i = FixtureRefIssuer::new(&b.build());
        let r = i.create_instance_labelled(&scope, &"x".repeat(n));
        match n {
            46 => assert_eq!(r.unwrap().as_str().len(), 63),
            47 => assert_eq!(r.unwrap().as_str().len(), 64),
            _ => assert_eq!(r, Err(RefCode::InvalidTag)),
        }
    }
    for bad in ["", "a.b", "a/b", "a\\b", "a:b", "a b", "a-b", "a_b"] {
        let mut b = CatalogBuilder::new();
        assert_eq!(
            b.add(
                CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v").unwrap(),
                bad
            ),
            Err(RefCode::InvalidTag)
        );
    }
    for n in [63, 64, 65] {
        assert_eq!(OpaqueRef::new(&"a".repeat(n)).is_ok(), n <= 64);
        assert_eq!(AttemptRef::new(&"a".repeat(n)).is_ok(), n <= 64);
    }
    assert_eq!(OpaqueRef::new(""), Err(abf_observation::ContextCode::Empty));
    assert_eq!(
        OpaqueRef::new(&"a".repeat(65)),
        Err(abf_observation::ContextCode::TooLong)
    );
    for bad in [
        "f-i0", "f-i01", "f-i257", "f--i1", "f-i1-x", "f/1-i1", "f1-", "-i1",
    ] {
        assert_eq!(SourceRef::new(bad), Err(RefCode::InvalidTag));
    }
    assert_eq!(
        CatalogScope::new(EnvironmentKind::Windows, r"\\wsl.localhost\x", "f", "v"),
        Err(RefCode::InvalidTag)
    );
}
#[test]
fn r69_r101d_configured_exhaustion_is_not_arithmetic_overflow() {
    let mut b = CatalogBuilder::new();
    let s = CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v").unwrap();
    b.add(s.clone(), "f1v1").unwrap();
    let mut i = FixtureRefIssuer::new(&b.build());
    for n in 1..MAX_INSTANCES_PER_SCOPE {
        assert_eq!(
            i.create_instance(&s).unwrap().as_str(),
            format!("f1v1-i{n}")
        );
    }
    assert_eq!(i.instance_counter(&s).unwrap(), 255);
    assert_eq!(i.create_instance(&s).unwrap().as_str(), "f1v1-i256");
    assert_eq!(i.create_instance(&s), Err(RefCode::CounterExhausted));
    assert_eq!(i.instance_counter(&s).unwrap(), 256);
    for _ in 1..MAX_EXECUTIONS_PER_ISSUER {
        i.begin_execution().unwrap();
        i.issue_finalization().unwrap();
    }
    assert_eq!(i.execution_counter(), 1023);
    assert_eq!(i.finalization_counter(), 1023);
    assert_eq!(i.begin_execution().unwrap().as_str(), "a1024");
    assert_eq!(i.issue_finalization().unwrap().as_str(), "z1024");
    assert_eq!(i.begin_execution(), Err(RefCode::CounterExhausted));
    assert_eq!(i.issue_finalization(), Err(RefCode::CounterExhausted));
    assert_eq!(i.execution_counter(), 1024);
    assert_eq!(i.finalization_counter(), 1024);
}
#[test]
fn r28_r71_r72_namespace_catalog_guard_and_reset_boundaries() {
    let s1 = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
    let s2 = CatalogScope::new(EnvironmentKind::Windows, "w1", "f2", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(s1.clone(), "f1v1").unwrap();
    assert_eq!(b.add(s2.clone(), "f1v1"), Err(RefCode::DuplicateScopeTag));
    b.add(s2.clone(), "f2v1").unwrap();
    let mut i = FixtureRefIssuer::new(&b.build());
    let a = i.create_instance_labelled(&s1, "sessions").unwrap();
    let other = i.create_instance_labelled(&s2, "sessions").unwrap();
    assert_ne!(a, other);
    assert_eq!(a.as_str(), "f1v1-sessions");
    assert_eq!(other.as_str(), "f2v1-sessions");
    let attempt = i.begin_execution().unwrap();
    let finalization = i.issue_finalization().unwrap();
    assert_ne!(a.as_str(), attempt.as_str());
    assert_ne!(attempt.as_str(), finalization.as_str());
    assert_ne!(a.as_str(), finalization.as_str());
    for label in ["i7", "i007"] {
        assert_eq!(
            i.create_instance_labelled(&s1, label),
            Err(RefCode::ReservedCounterForm)
        );
    }
    assert_eq!(
        i.create_instance_labelled(&s1, "i7x").unwrap().as_str(),
        "f1v1-i7x"
    );
    assert_eq!(i.instance_counter(&s1).unwrap(), 0);
}
#[test]
fn catalog_member_capacity_minus_one_at_and_plus_one() {
    let mut b = CatalogBuilder::new();
    for n in 0..MAX_MEMBERS - 1 {
        b.add(
            CatalogScope::new(EnvironmentKind::Windows, "w1", &format!("f{n}"), "v1").unwrap(),
            &format!("s{n}"),
        )
        .unwrap();
    }
    b.add(
        CatalogScope::new(EnvironmentKind::Windows, "w1", "last", "v1").unwrap(),
        "last",
    )
    .unwrap();
    assert_eq!(
        b.add(
            CatalogScope::new(EnvironmentKind::Windows, "w1", "extra", "v1").unwrap(),
            "extra"
        ),
        Err(RefCode::CapacityExceeded)
    );
    assert_eq!(b.build().scopes().count(), 16);
}

#[test]
fn abf107n_labelled_attempt_registry_limit_never_advances_counter_form() {
    let mut builder = CatalogBuilder::new();
    let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f1", "v1").unwrap();
    builder.add(scope, "f1v1").unwrap();
    let mut issuer = FixtureRefIssuer::new(&builder.build());
    for n in 0..1023 {
        issuer
            .begin_execution_labelled(&format!("label{n}"))
            .unwrap();
    }
    assert_eq!(issuer.execution_counter(), 0);
    assert_eq!(
        issuer.begin_execution_labelled("last").unwrap().as_str(),
        "last"
    );
    assert_eq!(issuer.execution_counter(), 0);
    assert_eq!(
        issuer.begin_execution_labelled("extra"),
        Err(RefCode::CounterExhausted)
    );
    assert_eq!(issuer.execution_counter(), 0);
    assert_eq!(issuer.begin_execution().unwrap().as_str(), "a1");
    assert_eq!(issuer.execution_counter(), 1);
}
