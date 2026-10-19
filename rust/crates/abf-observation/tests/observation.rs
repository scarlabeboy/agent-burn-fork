use abf_observation::*;
use abf_parser::{Code, Field, Limits, ParseResult, Status, parse};

fn context(kind: EnvironmentKind) -> DeclaredContext {
    let id = || OpaqueRef::new("fictional").unwrap();
    DeclaredContext::new(kind, id(), id(), id(), Some(id()), Some(0))
}
fn adapt(result: ParseResult, source: SourceRunStatus) -> ObservationEnvelope {
    from_opencode(result, context(EnvironmentKind::Windows), source).unwrap()
}

#[test]
fn preserves_full_parser_results_for_all_field_states() {
    for key in [
        "input",
        "output",
        "total",
        "read",
        "write",
        "id",
        "sessionID",
        "modelID",
        "providerID",
        "created",
        "tokens",
        "cache",
        "time",
    ] {
        for literal in [
            "null",
            "false",
            "[]",
            "{}",
            "-1",
            "0",
            "18446744073709551615",
            "18446744073709551616",
            "\"fictional\"",
        ] {
            let frame = match key {
                "read" | "write" => format!("{{\"tokens\":{{\"cache\":{{\"{key}\":{literal}}}}}}}"),
                "input" | "output" | "total" | "cache" => {
                    format!("{{\"tokens\":{{\"{key}\":{literal}}}}}")
                }
                "created" => format!("{{\"time\":{{\"created\":{literal}}}}}"),
                _ => format!("{{\"{key}\":{literal}}}"),
            };
            let original = parse(frame.as_bytes(), Some("parser_marker"), Limits::MAX);
            let envelope = adapt(original.clone(), SourceRunStatus::Partial);
            assert_eq!(envelope.parser_result(), &original);
            assert_eq!(envelope.source_status(), SourceRunStatus::Partial);
        }
    }
    let absent = parse(b"{}", None, Limits::MAX);
    assert_eq!(
        adapt(absent.clone(), SourceRunStatus::Complete).parser_result(),
        &absent
    );
}

#[test]
fn total_only_zero_max_and_missing_claims_are_not_imputed() {
    for total in [0, u64::MAX] {
        let frame = format!("{{\"tokens\":{{\"total\":{total}}}}}");
        let e = adapt(
            parse(frame.as_bytes(), None, Limits::MAX),
            SourceRunStatus::Partial,
        );
        let o = e.parser_result().observation().unwrap();
        assert_eq!(o.total, Field::Valid(total));
        assert_eq!(o.output, Field::Absent);
        assert_eq!(o.input, Field::Absent);
        assert_eq!(o.model_id, Field::Absent);
        assert_eq!(o.provider_id, Field::Absent);
        assert_eq!(o.time, Field::Absent);
        assert_eq!(o.created, Field::Unsupported);
        assert_eq!(o.status, Status::Accepted);
    }
}

#[test]
fn negative_time_provider_and_both_correlations_remain_separate_claims() {
    let e = adapt(parse(br#"{"time":{"created":-9223372036854775808},"providerID":"fictional_provider","tokens":{"output":0}}"#, Some("parser_marker"), Limits::MAX), SourceRunStatus::Complete);
    let o = e.parser_result().observation().unwrap();
    assert_eq!(o.created, Field::Valid(i64::MIN));
    assert_eq!(o.provider_id, Field::Valid(Box::from("fictional_provider")));
    assert_eq!(o.correlation.as_deref(), Some("parser_marker"));
    assert_eq!(e.context().correlation().unwrap().as_str(), "fictional");
    assert_eq!(e.context().harness(), Harness::OpenCode);
}

#[test]
fn every_rejection_code_survives_without_numeric_observation() {
    for code in [
        Code::InvalidLimits,
        Code::FrameBudget,
        Code::StringBudget,
        Code::DepthBudget,
        Code::EntryBudget,
        Code::ResultBudget,
        Code::InvalidUtf8,
        Code::IncompleteJson,
        Code::MalformedJson,
        Code::TrailingData,
        Code::RootShape,
        Code::DuplicateKey,
    ] {
        let e = adapt(ParseResult::Rejected(code), SourceRunStatus::Complete);
        assert_eq!(e.parser_result(), &ParseResult::Rejected(code));
        assert!(e.parser_result().observation().is_none());
        assert_eq!(e.source_status(), SourceRunStatus::Complete);
    }
}

#[test]
fn source_status_never_overwrites_record_qualification() {
    for (frame, expected) in [
        (br#"{"tokens":{"input":0}}"#.as_slice(), Status::Accepted),
        (
            br#"{"tokens":{"input":0,"output":null}}"#,
            Status::Qualified,
        ),
        (b"{}", Status::Incomplete),
    ] {
        for source in [
            SourceRunStatus::NotCollected,
            SourceRunStatus::Complete,
            SourceRunStatus::Partial,
            SourceRunStatus::Unavailable,
            SourceRunStatus::Failed,
        ] {
            let e = adapt(parse(frame, None, Limits::MAX), source);
            assert_eq!(e.source_status(), source);
            assert_eq!(e.parser_result().observation().unwrap().status, expected);
            let expected_inconsistency = match source {
                SourceRunStatus::NotCollected | SourceRunStatus::Unavailable => {
                    Some(Inconsistency::RecordWithoutReportedTraversal)
                }
                _ => None,
            };
            assert_eq!(e.inconsistency(), expected_inconsistency);
        }
    }
}

#[test]
fn equal_claims_remain_separate_occurrences_under_permutation() {
    let result = parse(
        br#"{"id":"m","sessionID":"s","tokens":{"input":7}}"#,
        None,
        Limits::MAX,
    );
    let a = from_opencode(
        result.clone(),
        context(EnvironmentKind::Windows),
        SourceRunStatus::Partial,
    )
    .unwrap();
    let b = from_opencode(
        result,
        context(EnvironmentKind::SelectedWsl),
        SourceRunStatus::Complete,
    )
    .unwrap();
    let records = [a.clone(), a.clone(), b.clone()];
    let permuted = [b.clone(), a.clone(), a.clone()];
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], records[1]);
    assert_eq!(records[0], permuted[2]);
    assert_eq!(records[2], permuted[0]);
    assert_ne!(a.context(), b.context());
}

#[test]
fn metadata_is_redacted_in_envelope_debug_and_errors() {
    let result = parse(
        br#"{"modelID":"fictional_secret_sk_123","tokens":{"input":0}}"#,
        Some("fictional_secret"),
        Limits::MAX,
    );
    let e = adapt(result, SourceRunStatus::Complete);
    assert_eq!(format!("{e:?}"), "ObservationEnvelope([redacted])");
    assert_eq!(
        format!("{:?}", AdapterCode::ParserStringBudget),
        "ParserStringBudget"
    );
}

#[test]
fn adapter_revalidates_public_parser_strings_without_reinterpreting_fields() {
    let base = parse(br#"{"tokens":{"input":0}}"#, None, Limits::MAX);
    for length in [0, 256, 257, 4096] {
        for slot in 0..5 {
            let mut result = base.clone();
            let ParseResult::Observation(ref mut o) = result else {
                unreachable!()
            };
            let text: Box<str> = "x".repeat(length).into();
            match slot {
                0 => o.correlation = Some(text),
                1 => o.message_id = Field::Valid(text),
                2 => o.session_id = Field::Valid(text),
                3 => o.model_id = Field::Valid(text),
                _ => o.provider_id = Field::Valid(text),
            }
            let adapted = from_opencode(
                result.clone(),
                context(EnvironmentKind::Windows),
                SourceRunStatus::Partial,
            );
            match length {
                0 | 256 => assert_eq!(adapted.unwrap().parser_result(), &result),
                _ => assert_eq!(adapted, Err(AdapterCode::ParserStringBudget)),
            }
        }
    }
}

#[test]
fn repeated_equivalent_calls_are_deterministic() {
    for byte in 0..=255u8 {
        let frame = [byte; 17];
        let result = parse(&frame, None, Limits::MAX);
        assert_eq!(
            adapt(result.clone(), SourceRunStatus::Failed),
            adapt(result, SourceRunStatus::Failed)
        );
    }
}

#[test]
fn explicit_field_states_are_retained_even_in_manually_constructed_results() {
    let numbers = [
        Field::Absent,
        Field::Null,
        Field::WrongType,
        Field::OutOfRange,
        Field::Unsupported,
        Field::Valid(u64::MAX),
    ];
    let texts = [
        Field::Absent,
        Field::Null,
        Field::WrongType,
        Field::OutOfRange,
        Field::Unsupported,
        Field::Valid(Box::<str>::from("fictional")),
    ];
    let containers = [
        Field::Absent,
        Field::Null,
        Field::WrongType,
        Field::OutOfRange,
        Field::Unsupported,
        Field::Valid(()),
    ];
    let times = [
        Field::Absent,
        Field::Null,
        Field::WrongType,
        Field::OutOfRange,
        Field::Unsupported,
        Field::Valid(-1i64),
    ];
    for index in 0..6 {
        let mut result = parse(b"{}", None, Limits::MAX);
        let ParseResult::Observation(ref mut o) = result else {
            unreachable!()
        };
        o.message_id = texts[index].clone();
        o.session_id = texts[index].clone();
        o.model_id = texts[index].clone();
        o.provider_id = texts[index].clone();
        o.time = containers[index].clone();
        o.tokens = containers[index].clone();
        o.cache = containers[index].clone();
        o.created = times[index].clone();
        o.input = numbers[index].clone();
        o.output = numbers[index].clone();
        o.total = numbers[index].clone();
        o.cache_read = numbers[index].clone();
        o.cache_write = numbers[index].clone();
        assert_eq!(
            adapt(result.clone(), SourceRunStatus::Partial).parser_result(),
            &result
        );
    }
}

#[test]
fn all_diagnostic_bits_and_status_precedence_are_retained() {
    use abf_parser::Diagnostic;
    let result = parse(
        br#"{"modelID":false,"time":null,"tokens":{"input":null,"unknown":7}}"#,
        None,
        Limits::MAX,
    );
    let e = adapt(result, SourceRunStatus::Complete);
    let o = e.parser_result().observation().unwrap();
    assert_eq!(o.status, Status::Incomplete);
    for code in [
        Diagnostic::InvalidField,
        Diagnostic::ExplicitNull,
        Diagnostic::UnsupportedComponent,
        Diagnostic::IncompleteTokens,
    ] {
        assert!(o.diagnostics.contains(code));
    }
    assert_eq!(o.diagnostics.bits(), 15);
}

#[test]
fn rejected_results_also_retain_source_contradictions() {
    for status in [SourceRunStatus::NotCollected, SourceRunStatus::Unavailable] {
        let e = adapt(ParseResult::Rejected(Code::MalformedJson), status);
        assert_eq!(
            e.inconsistency(),
            Some(Inconsistency::RecordWithoutReportedTraversal)
        );
        assert_eq!(
            e.parser_result(),
            &ParseResult::Rejected(Code::MalformedJson)
        );
        assert_eq!(e.source_status(), status);
    }
}

#[test]
fn maximum_bounded_metadata_and_components_move_without_arithmetic() {
    let text = "x".repeat(256);
    let frame = format!(
        r#"{{"id":"{text}","sessionID":"{text}","modelID":"{text}","providerID":"{text}","time":{{"created":9223372036854775807}},"tokens":{{"input":18446744073709551615,"output":18446744073709551615,"total":0,"cache":{{"read":18446744073709551615,"write":18446744073709551615}}}}}}"#
    );
    let result = parse(frame.as_bytes(), Some(&text), Limits::MAX);
    let ParseResult::Observation(ref o) = result else {
        panic!("synthetic maximum rejected")
    };
    let pointer = o.message_id.clone();
    let original_address = match &o.message_id {
        Field::Valid(s) => s.as_ptr(),
        _ => unreachable!(),
    };
    let id = || OpaqueRef::new(&"x".repeat(64)).unwrap();
    let ctx = DeclaredContext::new(
        EnvironmentKind::SelectedWsl,
        id(),
        id(),
        id(),
        Some(id()),
        Some(u64::MAX),
    );
    let e = from_opencode(result, ctx, SourceRunStatus::Partial).unwrap();
    let o = e.parser_result().observation().unwrap();
    assert_eq!(o.message_id, pointer);
    let retained_address = match &o.message_id {
        Field::Valid(s) => s.as_ptr(),
        _ => unreachable!(),
    };
    assert_eq!(retained_address, original_address);
    assert_eq!(
        [&o.input, &o.output, &o.cache_read, &o.cache_write],
        [&Field::Valid(u64::MAX); 4]
    );
    assert_eq!(o.total, Field::Valid(0));
    assert_eq!(o.created, Field::Valid(i64::MAX));
    assert_eq!(e.context().ordinal(), Some(u64::MAX));
    let upper_bound = size_of::<ObservationEnvelope>()
        .checked_add(1280)
        .unwrap()
        .checked_add(256)
        .unwrap();
    assert!(upper_bound <= 4096);
}

#[test]
fn raw_json_and_secret_shaped_unknown_content_do_not_enter_diagnostics() {
    let result = parse(br#"{"prompt":"fictional_secret_sk_123","credentials":{"token":"fictional_oauth_token"},"tokens":null}"#, None, Limits::MAX);
    let e = adapt(result, SourceRunStatus::Partial);
    let o = e.parser_result().observation().unwrap();
    assert_eq!(o.status, Status::Incomplete);
    assert_eq!(format!("{:?}", o.diagnostics), "Diagnostics(10)");
    assert!(!format!("{:?}", e.parser_result()).contains("fictional_secret"));
    assert!(!format!("{:?}", e.parser_result()).contains("fictional_oauth"));
}
