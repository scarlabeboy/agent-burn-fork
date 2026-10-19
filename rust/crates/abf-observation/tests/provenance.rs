use abf_observation::*;

fn id(s: &str) -> OpaqueRef {
    OpaqueRef::new(s).unwrap()
}

#[test]
fn opaque_bounds_and_code_only_errors() {
    assert_eq!(OpaqueRef::new(""), Err(ContextCode::Empty));
    assert_eq!(OpaqueRef::new(&"x".repeat(65)), Err(ContextCode::TooLong));
    for bad in [
        "/tmp/fiction",
        "C:\\fiction",
        "a.b",
        "a b",
        "a\n",
        "é",
        "🔑",
    ] {
        assert_eq!(OpaqueRef::new(bad), Err(ContextCode::Malformed));
    }
    for size in [1, 63, 64] {
        assert_eq!(
            OpaqueRef::new(&"x".repeat(size)).unwrap().as_str().len(),
            size
        );
    }
    assert_eq!(id("Abc_019-z").as_str(), "Abc_019-z");
    assert_eq!(
        format!("{:?}", id("fictional_secret")),
        "OpaqueRef([redacted])"
    );
}

#[test]
fn environments_are_distinct_even_for_equal_claims() {
    let make = |kind| {
        DeclaredContext::new(
            kind,
            id("e"),
            id("s"),
            id("r"),
            Some(id("c")),
            Some(u64::MAX),
        )
    };
    let windows = make(EnvironmentKind::Windows);
    let wsl = make(EnvironmentKind::SelectedWsl);
    assert_ne!(windows, wsl);
    assert_eq!(windows.environment_kind(), EnvironmentKind::Windows);
    assert_eq!(wsl.environment_kind(), EnvironmentKind::SelectedWsl);
    assert_eq!(windows.environment_ref().as_str(), "e");
    assert_eq!(windows.source_ref().as_str(), "s");
    assert_eq!(windows.run_ref().as_str(), "r");
    assert_eq!(windows.correlation().unwrap().as_str(), "c");
    assert_eq!(windows.ordinal(), Some(u64::MAX));
    assert_eq!(windows.harness(), Harness::OpenCode);
    assert_eq!(windows.parser(), ParserId::AbfOpenCodeV0_1_0);
    assert_eq!(format!("{windows:?}"), "DeclaredContext([redacted])");
}

#[test]
fn generated_malformed_context_is_deterministic_and_never_panics() {
    for byte in 0..=255u8 {
        for length in [0, 1, 64, 65, 256] {
            let value = char::from(byte).to_string().repeat(length);
            let first = OpaqueRef::new(&value);
            assert_eq!(first, OpaqueRef::new(&value));
            assert!(!format!("{first:?}").contains("fictional_secret"));
        }
    }
}

#[test]
fn source_statuses_are_separate_and_exhaustive() {
    fn tag(status: SourceRunStatus) -> u8 {
        match status {
            SourceRunStatus::NotCollected => 0,
            SourceRunStatus::Complete => 1,
            SourceRunStatus::Partial => 2,
            SourceRunStatus::Unavailable => 3,
            SourceRunStatus::Failed => 4,
        }
    }
    assert_eq!(
        [
            SourceRunStatus::NotCollected,
            SourceRunStatus::Complete,
            SourceRunStatus::Partial,
            SourceRunStatus::Unavailable,
            SourceRunStatus::Failed
        ]
        .map(tag),
        [0, 1, 2, 3, 4]
    );
}

/// No Computer source is expressible; adding one breaks the exhaustive match.
#[test]
fn collection_environment_has_only_two_kinds() {
    fn label(kind: EnvironmentKind) -> &'static str {
        match kind {
            EnvironmentKind::Windows => "windows",
            EnvironmentKind::SelectedWsl => "selected-wsl",
        }
    }
    assert_eq!(label(EnvironmentKind::Windows), "windows");
    assert_eq!(label(EnvironmentKind::SelectedWsl), "selected-wsl");
}

#[test]
fn optional_context_claims_stay_absent() {
    let ctx = DeclaredContext::new(
        EnvironmentKind::Windows,
        id("e"),
        id("s"),
        id("r"),
        None,
        None,
    );
    assert_eq!(ctx.correlation(), None);
    assert_eq!(ctx.ordinal(), None);
    let result = abf_parser::parse(b"{}", None, abf_parser::Limits::MAX);
    let envelope = from_opencode(result, ctx.clone(), SourceRunStatus::Partial).unwrap();
    assert_eq!(envelope.context(), &ctx);
    assert_eq!(
        envelope.parser_result().observation().unwrap().correlation,
        None
    );
}
