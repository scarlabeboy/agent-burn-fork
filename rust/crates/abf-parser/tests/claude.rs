use abf_parser::*;

fn obs(text: &str) -> ClaudeObservation {
    parse_claude(text.as_bytes(), None, Limits::MAX)
        .observation()
        .unwrap()
        .clone()
}
fn rejected(text: &str) -> Code {
    match parse_claude(text.as_bytes(), None, Limits::MAX) {
        ClaudeParseResult::Rejected(code) => code,
        other => panic!("expected rejection, got {other:?}"),
    }
}
fn s(text: &str) -> Field<Box<str>> {
    Field::Valid(text.into())
}

const FULL: &str = r#"{"type":"assistant","sessionId":"fict-session","requestId":"fict-req","timestamp":"2026-01-02T03:04:05.678Z","cwd":"/fictional/path","message":{"id":"fict-msg","model":"fict-model","role":"assistant","content":[{"type":"text","text":"FICTIONAL-SENTINEL-CONTENT"}],"usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"service_tier":"standard"}}}"#;

#[test]
fn valid_record_keeps_claims_only() {
    let o = obs(FULL);
    assert_eq!(o.session_id, s("fict-session"));
    assert_eq!(o.request_id, s("fict-req"));
    assert_eq!(o.timestamp, s("2026-01-02T03:04:05.678Z"));
    assert_eq!(o.message_id, s("fict-msg"));
    assert_eq!(o.model, s("fict-model"));
    assert_eq!(o.input, Field::Valid(10));
    assert_eq!(o.output, Field::Valid(20));
    assert_eq!(o.cache_read, Field::Valid(4));
    assert_eq!(o.cache_creation_flat, Field::Valid(3));
    assert_eq!(o.cache_creation_detail, Field::Absent);
    assert_eq!(o.cache_creation(), Field::Valid(3));
    assert_eq!(o.status, Status::Accepted);
    assert_eq!(o.diagnostics.bits(), 0);
}

#[test]
fn valid_zero_is_distinct_from_absent_and_null() {
    let o = obs(r#"{"message":{"usage":{"input_tokens":0,"output_tokens":null}}}"#);
    assert_eq!(o.input, Field::Valid(0));
    assert_eq!(o.output, Field::Null);
    assert_eq!(o.cache_read, Field::Absent);
    assert_eq!(o.status, Status::Qualified);
    assert!(o.diagnostics.contains(Diagnostic::ExplicitNull));
}

#[test]
fn every_numeric_component_preserves_all_states() {
    let cases = [
        ("null", Field::Null),
        ("false", Field::WrongType),
        ("\"7\"", Field::WrongType),
        ("-1", Field::OutOfRange),
        ("-0", Field::OutOfRange),
        ("1.5", Field::WrongType),
        ("1e2", Field::WrongType),
        ("18446744073709551616", Field::OutOfRange),
        ("0", Field::Valid(0)),
        ("18446744073709551615", Field::Valid(u64::MAX)),
    ];
    for key in [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ] {
        for (literal, expected) in &cases {
            let o = obs(&format!(
                r#"{{"message":{{"usage":{{"{key}":{literal}}}}}}}"#
            ));
            let actual = match key {
                "input_tokens" => o.input,
                "output_tokens" => o.output,
                "cache_read_input_tokens" => o.cache_read,
                _ => o.cache_creation_flat,
            };
            assert_eq!(&actual, expected, "{key} {literal}");
        }
    }
    for key in ["ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"] {
        let o = obs(&format!(
            r#"{{"message":{{"usage":{{"cache_creation":{{"{key}":-1}}}}}}}}"#
        ));
        let actual = if key.contains("5m") {
            o.cache_creation_5m
        } else {
            o.cache_creation_1h
        };
        assert_eq!(actual, Field::OutOfRange);
    }
}

#[test]
fn invalid_fields_qualify_and_no_token_is_incomplete() {
    let o = obs(r#"{"message":{"usage":{"input_tokens":-5,"output_tokens":2}}}"#);
    assert_eq!(o.status, Status::Qualified);
    assert!(o.diagnostics.contains(Diagnostic::InvalidField));
    let o = obs(r#"{"message":{"usage":{"input_tokens":-5}}}"#);
    assert_eq!(o.status, Status::Incomplete);
    assert!(o.diagnostics.contains(Diagnostic::IncompleteTokens));
    let o = obs("{}");
    assert_eq!(o.status, Status::Incomplete);
    assert_eq!(o.message, Field::Absent);
    assert_eq!(o.usage, Field::Unsupported);
    assert_eq!(o.input, Field::Unsupported);
    assert_eq!(o.model, Field::Unsupported);
    assert_eq!(o.session_id, Field::Absent);
}

#[test]
fn container_states_are_distinct() {
    assert_eq!(obs(r#"{"message":null}"#).message, Field::Null);
    assert_eq!(obs(r#"{"message":[]}"#).message, Field::WrongType);
    assert_eq!(obs(r#"{"message":{}}"#).usage, Field::Absent);
    assert_eq!(obs(r#"{"message":{"usage":null}}"#).usage, Field::Null);
    let o = obs(r#"{"message":{"usage":"x"}}"#);
    assert_eq!(o.usage, Field::WrongType);
    assert_eq!(o.input, Field::Unsupported);
    assert_eq!(obs(r#"{"message":{"usage":{}}}"#).input, Field::Absent);
    // Wrong-typed metadata
    let o =
        obs(r#"{"sessionId":7,"requestId":null,"timestamp":{},"message":{"id":[],"model":true}}"#);
    assert_eq!(o.session_id, Field::WrongType);
    assert_eq!(o.request_id, Field::Null);
    assert_eq!(o.timestamp, Field::WrongType);
    assert_eq!(o.message_id, Field::WrongType);
    assert_eq!(o.model, Field::WrongType);
    // Empty strings are valid claims; interpretation is left to the consumer.
    assert_eq!(obs(r#"{"requestId":""}"#).request_id, s(""));
}

#[test]
fn cache_creation_variants_are_not_double_counted() {
    let both = obs(
        r#"{"message":{"usage":{"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200}}}}"#,
    );
    assert_eq!(both.cache_creation_flat, Field::Valid(300));
    assert_eq!(both.cache_creation_5m, Field::Valid(100));
    assert_eq!(both.cache_creation_1h, Field::Valid(200));
    assert_eq!(both.cache_creation(), Field::Valid(300));
    // Disagreeing claims: the breakdown alone is used, flat is never added.
    let disagree = obs(
        r#"{"message":{"usage":{"cache_creation_input_tokens":999,"cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_1h_input_tokens":2}}}}"#,
    );
    assert_eq!(disagree.cache_creation(), Field::Valid(3));
    // One child only
    let one = obs(
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":{"ephemeral_1h_input_tokens":2}}}}"#,
    );
    assert_eq!(one.cache_creation(), Field::Valid(2));
    // Empty breakdown falls back to the flat claim
    let empty =
        obs(r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":{}}}}"#);
    assert_eq!(empty.cache_creation(), Field::Valid(9));
    // Invalid child / container is surfaced, flat is not substituted
    let bad_child = obs(
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":{"ephemeral_5m_input_tokens":"x","ephemeral_1h_input_tokens":2}}}}"#,
    );
    assert_eq!(bad_child.cache_creation(), Field::WrongType);
    let null_detail =
        obs(r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":null}}}"#);
    assert_eq!(null_detail.cache_creation(), Field::Null);
    let wrong_detail =
        obs(r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":5}}}"#);
    assert_eq!(wrong_detail.cache_creation(), Field::WrongType);
    // Breakdown overflow
    let overflow = obs(&format!(
        r#"{{"message":{{"usage":{{"cache_creation":{{"ephemeral_5m_input_tokens":{m},"ephemeral_1h_input_tokens":1}}}}}}}}"#,
        m = u64::MAX
    ));
    assert_eq!(overflow.cache_creation(), Field::OutOfRange);
    assert!(overflow.diagnostics.contains(Diagnostic::InvalidField));
    // Breakdown alone makes a record usable
    let only = obs(r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":5}}}}"#);
    assert_eq!(only.status, Status::Accepted);
}

#[test]
fn unknown_token_components_qualify_but_other_usage_keys_do_not() {
    let extra = obs(
        r#"{"message":{"usage":{"input_tokens":1,"service_tier":"x","speed":"fast","server_tool_use":{"web_search_requests":2}}}}"#,
    );
    assert_eq!(extra.status, Status::Accepted);
    let unknown = obs(r#"{"message":{"usage":{"input_tokens":1,"future_tokens":4}}}"#);
    assert_eq!(unknown.status, Status::Qualified);
    assert!(
        unknown
            .diagnostics
            .contains(Diagnostic::UnsupportedComponent)
    );
    let breakdown = obs(
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_24h_input_tokens":4}}}}"#,
    );
    assert!(
        breakdown
            .diagnostics
            .contains(Diagnostic::UnsupportedComponent)
    );
}

#[test]
fn malformed_frames_are_rejected_with_codes() {
    assert_eq!(rejected(""), Code::IncompleteJson);
    assert_eq!(rejected("{"), Code::IncompleteJson);
    assert_eq!(rejected(r#"{"a":}"#), Code::MalformedJson);
    assert_eq!(rejected("[]"), Code::RootShape);
    assert_eq!(rejected("null"), Code::RootShape);
    assert_eq!(rejected("{} {}"), Code::TrailingData);
    assert_eq!(rejected("{}x"), Code::TrailingData);
    assert_eq!(
        parse_claude(b"{\"a\":\"\xff\"}", None, Limits::MAX),
        ClaudeParseResult::Rejected(Code::InvalidUtf8)
    );
    // Surrounding whitespace, including a trailing JSONL newline, is allowed.
    assert!(
        parse_claude(b"  {}\n", None, Limits::MAX)
            .observation()
            .is_some()
    );
}

#[test]
fn duplicate_keys_are_rejected_everywhere() {
    for text in [
        r#"{"sessionId":"a","sessionId":"b"}"#,
        r#"{"message":{"id":"a","id":"b"}}"#,
        r#"{"message":{"usage":{"input_tokens":1,"input_tokens":2}}}"#,
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_5m_input_tokens":1}}}}"#,
        r#"{"message":{"model":"a"},"message":{"model":"b"}}"#,
        r#"{"message":{"content":[{"type":"x","type":"y"}]}}"#,
        r#"{"sessionId":"a","sessionId":"b"}"#,
    ] {
        assert_eq!(rejected(text), Code::DuplicateKey, "{text}");
    }
}

#[test]
fn size_bounds_are_explicit_and_bounded() {
    let big = format!(r#"{{"message":{{"content":"{}"}}}}"#, "x".repeat(70_000));
    assert_eq!(rejected(&big), Code::FrameBudget);
    let limits = Limits {
        frame_bytes: FULL.len() - 1,
        ..Limits::MAX
    };
    assert_eq!(
        parse_claude(FULL.as_bytes(), None, limits),
        ClaudeParseResult::Rejected(Code::FrameBudget)
    );
    let exact = Limits {
        frame_bytes: FULL.len(),
        ..Limits::MAX
    };
    assert!(
        parse_claude(FULL.as_bytes(), None, exact)
            .observation()
            .is_some()
    );
    let long = "y".repeat(257);
    assert_eq!(
        rejected(&format!(r#"{{"sessionId":"{long}"}}"#)),
        Code::StringBudget
    );
    assert_eq!(
        rejected(&format!(r#"{{"message":{{"model":"{long}"}}}}"#)),
        Code::StringBudget
    );
    // Content strings are not retained, so a long content string below the frame ceiling is fine.
    assert!(
        parse_claude(
            format!(r#"{{"message":{{"content":"{long}"}}}}"#).as_bytes(),
            None,
            Limits::MAX
        )
        .observation()
        .is_some()
    );
    assert_eq!(
        parse_claude(b"{}", Some(&long), Limits::MAX),
        ClaudeParseResult::Rejected(Code::StringBudget)
    );
    let deep = format!("{}1{}", r#"{"a":"#.repeat(17), "}".repeat(17));
    assert_eq!(rejected(&deep), Code::DepthBudget);
    let many = format!(
        r#"{{"message":{{"content":[{}]}}}}"#,
        vec!["0"; 300].join(",")
    );
    assert_eq!(rejected(&many), Code::EntryBudget);
    // Limits can only be lowered.
    let raised = Limits {
        frame_bytes: Limits::MAX.frame_bytes + 1,
        ..Limits::MAX
    };
    assert_eq!(
        parse_claude(b"{}", None, raised),
        ClaudeParseResult::Rejected(Code::InvalidLimits)
    );
    let tiny = Limits {
        result_bytes: 1,
        ..Limits::MAX
    };
    assert_eq!(
        parse_claude(b"{}", None, tiny),
        ClaudeParseResult::Rejected(Code::InvalidLimits)
    );
}

#[test]
fn maximum_retained_result_fits_the_ceiling() {
    let max = "m".repeat(256);
    let text = format!(
        r#"{{"sessionId":"{max}","requestId":"{max}","timestamp":"{max}","message":{{"id":"{max}","model":"{max}","usage":{{"input_tokens":1}}}}}}"#
    );
    let result = parse_claude(text.as_bytes(), Some(&max), Limits::MAX);
    assert!(result.retained_bytes().unwrap() <= Limits::MAX.result_bytes);
    let exact = Limits {
        result_bytes: result.retained_bytes().unwrap(),
        ..Limits::MAX
    };
    assert!(
        parse_claude(text.as_bytes(), Some(&max), exact)
            .observation()
            .is_some()
    );
    let below = Limits {
        result_bytes: exact.result_bytes - 1,
        ..Limits::MAX
    };
    assert_eq!(
        parse_claude(text.as_bytes(), Some(&max), below),
        ClaudeParseResult::Rejected(Code::ResultBudget)
    );
}

#[test]
fn content_and_identifiers_are_not_disclosed() {
    let result = parse_claude(FULL.as_bytes(), Some("fict-correlation"), Limits::MAX);
    let o = result.observation().unwrap();
    for text in [format!("{result:?}"), format!("{o:?}")] {
        for secret in [
            "FICTIONAL-SENTINEL-CONTENT",
            "/fictional/path",
            "fict-session",
            "fict-req",
            "fict-msg",
            "fict-model",
            "fict-correlation",
            "2026-01-02",
        ] {
            assert!(!text.contains(secret), "Debug leaked {secret}");
        }
    }
    // Retained claims are limited to allowlisted metadata: content never appears in the result.
    assert_eq!(
        result.retained_bytes().unwrap(),
        std::mem::size_of::<ClaudeParseResult>()
            + "assistant".len()
            + "fict-correlation".len()
            + "fict-session".len()
            + "fict-req".len()
            + "2026-01-02T03:04:05.678Z".len()
            + "fict-msg".len()
            + "fict-model".len()
    );
    // Content-only unknown keys cannot supply allowlisted claims.
    let o = obs(
        r#"{"content":{"sessionId":"inner"},"tool":{"requestId":"inner"},"message":{"content":{"id":"inner","usage":{"input_tokens":9}}}}"#,
    );
    assert_eq!(o.session_id, Field::Absent);
    assert_eq!(o.request_id, Field::Absent);
    assert_eq!(o.message_id, Field::Absent);
    assert_eq!(o.usage, Field::Absent);
    assert_eq!(o.input, Field::Unsupported);
}

#[test]
fn deterministic_and_independent_results() {
    assert_eq!(
        parse_claude(FULL.as_bytes(), None, Limits::MAX),
        parse_claude(FULL.as_bytes(), None, Limits::MAX)
    );
    // Equal bytes are equal claims, not an identity or dedupe verdict.
    let a = parse_claude(FULL.as_bytes(), Some("a"), Limits::MAX);
    let b = parse_claude(FULL.as_bytes(), Some("b"), Limits::MAX);
    assert_ne!(a, b);
}

#[test]
fn cache_creation_is_unsupported_when_usage_is_unavailable() {
    for text in [
        "{}",
        r#"{"message":null}"#,
        r#"{"message":{}}"#,
        r#"{"message":{"usage":null}}"#,
        r#"{"message":{"usage":"x"}}"#,
    ] {
        let o = obs(text);
        assert_eq!(o.cache_creation(), Field::Unsupported, "{text}");
        assert_eq!(o.input, Field::Unsupported, "{text}");
    }
    // An available usage object without the claim is Absent, not Unsupported.
    assert_eq!(
        obs(r#"{"message":{"usage":{}}}"#).cache_creation(),
        Field::Absent
    );
}
