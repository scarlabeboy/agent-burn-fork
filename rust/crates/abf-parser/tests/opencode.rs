use abf_parser::*;

#[test]
fn preserves_raw_components_without_total_imputation() {
    let result = parse(
        br#"{"tokens":{"input":100,"output":50,"cache":{"read":10,"write":20}}}"#,
        None,
        Limits::MAX,
    );
    let observation = result.observation().unwrap();
    assert_eq!(observation.input, Field::Valid(100));
    assert_eq!(observation.output, Field::Valid(50));
    assert_eq!(observation.cache_read, Field::Valid(10));
    assert_eq!(observation.cache_write, Field::Valid(20));
    assert_eq!(observation.total, Field::Absent);
}

fn obs(text: &str) -> Observation {
    parse(text.as_bytes(), None, Limits::MAX)
        .observation()
        .unwrap()
        .clone()
}

#[test]
fn each_component_preserves_all_states() {
    let cases = [
        ("null", Field::Null),
        ("false", Field::WrongType),
        ("\"7\"", Field::WrongType),
        ("-1", Field::OutOfRange),
        ("1.5", Field::WrongType),
        ("1e0", Field::WrongType),
        ("18446744073709551616", Field::OutOfRange),
        ("0", Field::Valid(0)),
        ("7", Field::Valid(7)),
        ("18446744073709551615", Field::Valid(u64::MAX)),
    ];
    for field in ["input", "output", "total", "read", "write"] {
        for (literal, expected) in &cases {
            let text = match field {
                "read" | "write" => {
                    format!("{{\"tokens\":{{\"cache\":{{\"{field}\":{literal}}}}}}}")
                }
                _ => format!("{{\"tokens\":{{\"{field}\":{literal}}}}}"),
            };
            let o = obs(&text);
            let actual = match field {
                "input" => o.input,
                "output" => o.output,
                "total" => o.total,
                "read" => o.cache_read,
                _ => o.cache_write,
            };
            assert_eq!(&actual, expected, "{field} {literal}");
        }
    }
    let o = obs("{\"tokens\":{\"cache\":{}}}");
    assert_eq!(
        [o.input, o.output, o.total, o.cache_read, o.cache_write],
        [const { Field::Absent }; 5]
    );
}

#[test]
fn total_is_independent_and_zero_is_valid() {
    for total in [0, 1, 123, 999, u64::MAX] {
        let o = obs(&format!(
            "{{\"tokens\":{{\"input\":100,\"output\":0,\"total\":{total}}}}}"
        ));
        assert_eq!(o.total, Field::Valid(total));
        assert_eq!(o.output, Field::Valid(0));
        assert_eq!(o.input, Field::Valid(100));
    }
    let o = obs("{\"tokens\":{\"total\":123}}");
    assert_eq!(o.output, Field::Absent);
    assert_eq!(obs("{\"tokens\":{\"output\":0}}").status, Status::Accepted);
}

#[test]
fn containers_never_become_zero_usage() {
    for (text, expected) in [
        ("{}", Field::Absent),
        ("{\"tokens\":null}", Field::Null),
        ("{\"tokens\":[]}", Field::WrongType),
        ("{\"tokens\":3}", Field::WrongType),
    ] {
        let o = obs(text);
        assert_eq!(o.tokens, expected);
        assert_eq!(o.input, Field::Unsupported);
        assert_eq!(o.status, Status::Incomplete);
        assert!(o.diagnostics.contains(Diagnostic::IncompleteTokens));
    }
    for (literal, expected) in [
        ("null", Field::Null),
        ("false", Field::WrongType),
        ("[]", Field::WrongType),
    ] {
        let o = obs(&format!("{{\"tokens\":{{\"cache\":{literal}}}}}"));
        assert_eq!(o.cache, expected);
        assert_eq!(o.cache_read, Field::Unsupported);
    }
    assert_eq!(obs("{\"tokens\":{}}").status, Status::Incomplete);
}

#[test]
fn metadata_claims_are_preserved_without_defaults() {
    let o = obs(
        r#"{"id":"m","sessionID":"s","modelID":"k2p6","providerID":"test","time":{"created":-1},"tokens":{"input":0}}"#,
    );
    assert_eq!(o.message_id, Field::Valid(Box::from("m")));
    assert_eq!(o.session_id, Field::Valid(Box::from("s")));
    assert_eq!(o.model_id, Field::Valid(Box::from("k2p6")));
    assert_eq!(o.provider_id, Field::Valid(Box::from("test")));
    assert_eq!(o.created, Field::Valid(-1));
    let absent = obs("{\"tokens\":{\"input\":1}}");
    assert_eq!(absent.message_id, Field::Absent);
    assert_eq!(absent.session_id, Field::Absent);
    assert_eq!(absent.model_id, Field::Absent);
    assert_eq!(absent.provider_id, Field::Absent);
    assert_eq!(absent.time, Field::Absent);
    assert_eq!(absent.created, Field::Unsupported);
    for key in ["id", "sessionID", "modelID", "providerID"] {
        for literal in ["null", "7", "[]", "{}", "true"] {
            let o = obs(&format!(
                "{{\"{key}\":{literal},\"tokens\":{{\"input\":0}}}}"
            ));
            let fields = [o.message_id, o.session_id, o.model_id, o.provider_id];
            let index = ["id", "sessionID", "modelID", "providerID"]
                .iter()
                .position(|x| *x == key)
                .unwrap();
            assert_eq!(
                fields[index],
                match literal {
                    "null" => Field::Null,
                    _ => Field::WrongType,
                }
            );
        }
    }
}

#[test]
fn time_is_raw_signed_integer_or_explicit_invalid_state() {
    for (value, expected) in [
        ("-9223372036854775808", Field::Valid(i64::MIN)),
        ("9223372036854775807", Field::Valid(i64::MAX)),
        ("0", Field::Valid(0)),
        ("9223372036854775808", Field::OutOfRange),
        ("-9223372036854775809", Field::OutOfRange),
        ("null", Field::Null),
        ("\"0\"", Field::WrongType),
        ("1.1", Field::WrongType),
    ] {
        assert_eq!(
            obs(&format!("{{\"time\":{{\"created\":{value}}}}}")).created,
            expected
        );
    }
    assert_eq!(obs("{\"time\":{}}").created, Field::Absent);
    for value in ["0", "null", "[]", "false"] {
        let o = obs(&format!("{{\"time\":{value}}}"));
        assert_eq!(o.created, Field::Unsupported);
    }
}

#[test]
fn malformed_frames_have_coded_rejections() {
    for input in [
        b"".as_slice(),
        b"{",
        b"{\"x\":\"",
        b"{\"x\":\"\\",
        b"{]",
        b"{} {}",
        b"{}x",
        b"[]",
        b"null",
        b"1",
        b"\xff",
        b"{\"x\":01}",
        b"{\"x\":+1}",
        b"{\"x\":1.}",
        b"{\"x\":1e}",
        b"{\"x\":NaN}",
        b"{\"x\":\"\\uD800\"}",
        b"{\"x\":\"\\uDC00\"}",
        b"{\"x\":\"\n\"}",
        b"{\"x\":true,}",
        b"{\"x\":[1,]}",
    ] {
        assert!(
            matches!(parse(input, None, Limits::MAX), ParseResult::Rejected(_)),
            "{input:?}"
        );
    }
    assert_eq!(
        parse(b"{", None, Limits::MAX),
        ParseResult::Rejected(Code::IncompleteJson)
    );
    assert_eq!(
        parse(b"\xff", None, Limits::MAX),
        ParseResult::Rejected(Code::InvalidUtf8)
    );
    assert_eq!(
        parse(b"[]", None, Limits::MAX),
        ParseResult::Rejected(Code::RootShape)
    );
}

#[test]
fn duplicates_are_rejected_everywhere_including_escaped_collisions() {
    for text in [
        r#"{"id":1,"id":2}"#,
        r#"{"tokens":{"input":0,"input":1}}"#,
        r#"{"tokens":{"cache":{"read":0,"read":1}}}"#,
        r#"{"tokens":{},"to\u006bens":{}}"#,
        r#"{"ignored":{"a":0,"\u0061":0}}"#,
        r#"{"ignored":[{"a":0,"a":0}]}"#,
        r#"{"ignored":{"😀":0,"\ud83d\ude00":1}}"#,
    ] {
        assert_eq!(
            parse(text.as_bytes(), None, Limits::MAX),
            ParseResult::Rejected(Code::DuplicateKey)
        );
    }
}

#[test]
fn unknown_token_components_qualify_without_raw_diagnostics() {
    for text in [
        r#"{"tokens":{"input":1,"reasoning":42}}"#,
        r#"{"tokens":{"input":1,"cache":{"fictional-secret/path":0}}}"#,
    ] {
        let o = obs(text);
        assert_eq!(o.status, Status::Qualified);
        assert!(o.diagnostics.contains(Diagnostic::UnsupportedComponent));
        let diagnostic = format!("{:?}", o.diagnostics);
        assert!(!diagnostic.contains("fictional"));
        assert!(!diagnostic.contains("reasoning"));
    }
}

#[test]
fn ignored_content_cannot_supply_metadata_or_leak_to_diagnostics() {
    let o = obs(
        r#"{"content":{"id":"fake","modelID":"fake","tokens":{"input":100},"secret":"fictional-token-/home/private"},"tokens":{"output":0}}"#,
    );
    assert_eq!(o.message_id, Field::Absent);
    assert_eq!(o.input, Field::Absent);
    assert_eq!(o.output, Field::Valid(0));
    assert!(!format!("{o:?}").contains("fictional"));
}

#[test]
fn calls_are_independent_and_deterministic() {
    let input = br#"{"id":"equal","tokens":{"input":0}}"#;
    let a = parse(input, Some("synthetic-a"), Limits::MAX);
    let b = parse(input, Some("synthetic-b"), Limits::MAX);
    assert_ne!(a, b);
    assert_eq!(a, parse(input, Some("synthetic-a"), Limits::MAX));
    assert_eq!(
        a.observation().unwrap().correlation.as_deref(),
        Some("synthetic-a")
    );
    assert_eq!(
        obs("{\"tokens\":{\"input\":1,\"output\":2},\"id\":\"a\"}"),
        obs(" \n{ \"id\":\"a\", \"tokens\": {\"output\":2,\"input\":1}}\t")
    );
    assert_eq!(
        obs("{\"tokens\":{\"input\":999,\"output\":2}}").output,
        Field::Valid(2)
    );
}

#[test]
fn json_string_grammar_accepts_standard_escapes_and_rejects_invalid_ones() {
    let o = obs(r#"{"id":"\"\\\/\b\f\n\r\t\u0000\u0061\u00e9\ud83d\ude00"}"#);
    assert_eq!(
        o.message_id,
        Field::Valid(Box::from("\"\\/\u{8}\u{c}\n\r\t\0aé😀"))
    );
    for input in [
        r#"{"id":"\q"}"#,
        r#"{"id":"\u000g"}"#,
        r#"{"id":"\ud800\u0000"}"#,
        r#"{"id":"\udfff"}"#,
        r#"{"id":"\u0"}"#,
        r#"{"id":"\ud800"}"#,
        "\u{feff}{}",
        "\u{a0}{}",
        r#"{"x":.1}"#,
        r#"{"x":1e+}"#,
        r#"{"x":-01}"#,
    ] {
        assert!(matches!(
            parse(input.as_bytes(), None, Limits::MAX),
            ParseResult::Rejected(_)
        ));
    }
    assert_eq!(obs(r#"{"tokens":{"input":-0}}"#).input, Field::OutOfRange);
}
