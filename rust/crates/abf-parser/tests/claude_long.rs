use abf_parser::*;

const LIM: LongLimits = LongLimits::MAX;

fn long(text: &str) -> ClaudeParseResult {
    parse_claude_long(text.as_bytes(), None, LIM)
}
fn obs(text: &str) -> ClaudeObservation {
    long(text).observation().unwrap().clone()
}
fn rejected(text: &str) -> Code {
    match long(text) {
        ClaudeParseResult::Rejected(code) => code,
        other => panic!("expected rejection, got {other:?}"),
    }
}
fn record_with_content(content: &str) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"fict-session","requestId":"fict-req","timestamp":"2026-01-02T03:04:05.678Z","message":{{"id":"fict-msg","model":"fict-model","content":{content},"usage":{{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}}}}}}"#
    )
}

#[test]
fn long_text_content_beyond_the_old_budget_keeps_small_claims() {
    let text = record_with_content(&format!(
        r#"[{{"type":"text","text":"{}"}}]"#,
        "FICTIONAL-CONTENT ".repeat(20_000)
    ));
    assert!(text.len() > 300_000);
    // The tree-based parser cannot take this record; the long parser can.
    assert_eq!(
        parse_claude(text.as_bytes(), None, Limits::MAX),
        ClaudeParseResult::Rejected(Code::FrameBudget)
    );
    let o = obs(&text);
    assert_eq!(o.input, Field::Valid(10));
    assert_eq!(o.output, Field::Valid(20));
    assert_eq!(o.cache_read, Field::Valid(4));
    assert_eq!(o.cache_creation(), Field::Valid(3));
    assert_eq!(o.session_id, Field::Valid("fict-session".into()));
    assert_eq!(o.model, Field::Valid("fict-model".into()));
    assert_eq!(o.status, Status::Accepted);
}

#[test]
fn long_arrays_and_nested_objects_are_skipped() {
    let array = format!("[{}]", vec!["0"; 500_000].join(","));
    assert_eq!(obs(&record_with_content(&array)).input, Field::Valid(10));
    let mut nested = String::new();
    for _ in 0..12 {
        nested.push_str(r#"{"k":["#);
    }
    nested.push('1');
    for _ in 0..12 {
        nested.push_str("]}");
    }
    assert_eq!(obs(&record_with_content(&nested)).output, Field::Valid(20));
    // Escapes, multibyte text and surrogate pairs in skipped strings are accepted.
    let rich = r#""café 😀 \n \"q\" \\ / é 日本""#;
    assert_eq!(obs(&record_with_content(rich)).input, Field::Valid(10));
}

#[test]
fn record_byte_limit_boundaries() {
    let text = record_with_content("\"x\"");
    let exact = LongLimits {
        record_bytes: text.len(),
        ..LIM
    };
    assert!(
        parse_claude_long(text.as_bytes(), None, exact)
            .observation()
            .is_some()
    );
    let below = LongLimits {
        record_bytes: text.len() - 1,
        ..LIM
    };
    assert_eq!(
        parse_claude_long(text.as_bytes(), None, below),
        ClaudeParseResult::Rejected(Code::FrameBudget)
    );
    // The ceiling is checked before any decoding, even for invalid UTF-8.
    let hostile = vec![0xffu8; LIM.record_bytes + 1];
    assert_eq!(
        parse_claude_long(&hostile, None, LIM),
        ClaudeParseResult::Rejected(Code::FrameBudget)
    );
    let raised = LongLimits {
        record_bytes: LIM.record_bytes + 1,
        ..LIM
    };
    assert_eq!(
        parse_claude_long(b"{}", None, raised),
        ClaudeParseResult::Rejected(Code::InvalidLimits)
    );
}

#[test]
fn entry_limit_counts_members_and_elements_exactly() {
    // 1 root member + n elements.
    let n = 1000;
    let text = format!(r#"{{"a":[{}]}}"#, vec!["0"; n].join(","));
    let exact = LongLimits {
        entries: n + 1,
        ..LIM
    };
    assert!(
        parse_claude_long(text.as_bytes(), None, exact)
            .observation()
            .is_some()
    );
    let below = LongLimits { entries: n, ..LIM };
    assert_eq!(
        parse_claude_long(text.as_bytes(), None, below),
        ClaudeParseResult::Rejected(Code::EntryBudget)
    );
    let over = LongLimits {
        entries: LIM.entries + 1,
        ..LIM
    };
    assert_eq!(
        parse_claude_long(b"{}", None, over),
        ClaudeParseResult::Rejected(Code::InvalidLimits)
    );
    let big = format!(r#"{{"a":[{}]}}"#, vec!["0"; LIM.entries].join(","));
    assert_eq!(rejected(&big), Code::EntryBudget);
}

#[test]
fn depth_limit_boundaries_and_hostile_nesting() {
    // Root object is depth 1; `{"a":[[...]]}` with d-1 arrays has depth d.
    let nest = |depth: usize| {
        format!(
            r#"{{"a":{}0{}}}"#,
            "[".repeat(depth - 1),
            "]".repeat(depth - 1)
        )
    };
    assert!(long(&nest(32)).observation().is_some());
    assert_eq!(rejected(&nest(33)), Code::DepthBudget);
    let lowered = LongLimits { depth: 3, ..LIM };
    assert!(
        parse_claude_long(nest(3).as_bytes(), None, lowered)
            .observation()
            .is_some()
    );
    assert_eq!(
        parse_claude_long(nest(4).as_bytes(), None, lowered),
        ClaudeParseResult::Rejected(Code::DepthBudget)
    );
    // Hostile nesting is refused at the limit, with no recursion proportional to input.
    assert_eq!(
        rejected(&format!(r#"{{"a":{}"#, "[".repeat(1_000_000))),
        Code::DepthBudget
    );
    // Path objects count too.
    let deep_message = format!(
        r#"{{"message":{{"usage":{{"x":{}0{}}}}}}}"#,
        "[".repeat(30),
        "]".repeat(30)
    );
    assert_eq!(rejected(&deep_message), Code::DepthBudget);
}

#[test]
fn retained_strings_are_bounded_but_content_strings_are_not() {
    let over = "y".repeat(257);
    assert_eq!(
        rejected(&format!(r#"{{"sessionId":"{over}"}}"#)),
        Code::StringBudget
    );
    assert_eq!(
        rejected(&format!(r#"{{"message":{{"model":"{over}"}}}}"#)),
        Code::StringBudget
    );
    assert_eq!(
        rejected(&format!(r#"{{"requestId":"{}"}}"#, "y".repeat(2_000_000))),
        Code::StringBudget
    );
    let exact = "y".repeat(256);
    assert_eq!(
        obs(&format!(r#"{{"sessionId":"{exact}"}}"#)).session_id,
        Field::Valid(exact.as_str().into())
    );
    // Escapes count by decoded bytes: 256 escaped ASCII characters still fit.
    let escaped = "\\u0041".repeat(256);
    assert_eq!(
        obs(&format!(r#"{{"sessionId":"{escaped}"}}"#)).session_id,
        Field::Valid("A".repeat(256).as_str().into())
    );
    let lowered = LongLimits {
        string_bytes: 4,
        ..LIM
    };
    assert_eq!(
        parse_claude_long(br#"{"sessionId":"abcde"}"#, None, lowered),
        ClaudeParseResult::Rejected(Code::StringBudget)
    );
    assert_eq!(
        parse_claude_long(b"{}", Some("abcde"), lowered),
        ClaudeParseResult::Rejected(Code::StringBudget)
    );
    // Content strings far above any retention limit are fine.
    assert!(
        long(&record_with_content(&format!(
            "\"{}\"",
            "z".repeat(3_000_000)
        )))
        .observation()
        .is_some()
    );
    let tiny = LongLimits {
        result_bytes: 1,
        ..LIM
    };
    assert_eq!(
        parse_claude_long(b"{}", None, tiny),
        ClaudeParseResult::Rejected(Code::InvalidLimits)
    );
}

#[test]
fn result_budget_boundary() {
    let max = "m".repeat(256);
    let text = format!(
        r#"{{"sessionId":"{max}","requestId":"{max}","timestamp":"{max}","message":{{"id":"{max}","model":"{max}","usage":{{"input_tokens":1}}}}}}"#
    );
    let result = parse_claude_long(text.as_bytes(), Some(&max), LIM);
    let needed = result.retained_bytes().unwrap();
    assert!(needed <= LIM.result_bytes);
    assert!(
        parse_claude_long(
            text.as_bytes(),
            Some(&max),
            LongLimits {
                result_bytes: needed,
                ..LIM
            }
        )
        .observation()
        .is_some()
    );
    assert_eq!(
        parse_claude_long(
            text.as_bytes(),
            Some(&max),
            LongLimits {
                result_bytes: needed - 1,
                ..LIM
            }
        ),
        ClaudeParseResult::Rejected(Code::ResultBudget)
    );
}

#[test]
fn hostile_numbers_and_keys_stay_coded() {
    let digits = "9".repeat(1_000_000);
    assert_eq!(
        obs(&format!(
            r#"{{"message":{{"usage":{{"input_tokens":{digits}}}}}}}"#
        ))
        .input,
        Field::OutOfRange
    );
    assert_eq!(
        obs(&format!(
            r#"{{"message":{{"content":{digits},"usage":{{"input_tokens":1}}}}}}"#
        ))
        .input,
        Field::Valid(1)
    );
    // An enormous unknown key is unmatched, not an error, outside the usage object ...
    let key = "k".repeat(100_000);
    assert_eq!(
        obs(&format!(
            r#"{{"{key}":1,"message":{{"usage":{{"input_tokens":1}}}}}}"#
        ))
        .input,
        Field::Valid(1)
    );
    // ... and inside usage it cannot be classified, so it qualifies the record.
    let o = obs(&format!(
        r#"{{"message":{{"usage":{{"input_tokens":1,"{key}":1}}}}}}"#
    ));
    assert_eq!(o.status, Status::Qualified);
    assert!(o.diagnostics.contains(Diagnostic::UnsupportedComponent));
    // Escaped spellings of allowlisted keys still match.
    assert_eq!(
        obs(r#"{"sessionId":"a"}"#).session_id,
        Field::Valid("a".into())
    );
}

#[test]
fn malformed_and_incomplete_records_are_rejected() {
    let full = record_with_content(r#"[{"type":"text","text":"fict é \"x\""}]"#);
    assert!(long(&full).observation().is_some());
    // Every strict prefix (a pending tail) is rejected, never an observation.
    for end in 0..full.len() {
        if !full.is_char_boundary(end) {
            continue;
        }
        assert!(
            matches!(long(&full[..end]), ClaudeParseResult::Rejected(_)),
            "prefix {end} accepted"
        );
    }
    assert_eq!(rejected(""), Code::IncompleteJson);
    assert_eq!(rejected("{"), Code::IncompleteJson);
    assert_eq!(
        rejected(r#"{"message":{"content":"abc"#),
        Code::IncompleteJson
    );
    assert_eq!(
        rejected(r#"{"message":{"content":[1,2,}}}"#),
        Code::MalformedJson
    );
    assert_eq!(
        rejected(r#"{"message":{"content":"bad \x escape"}}"#),
        Code::MalformedJson
    );
    assert_eq!(rejected(r#"{"a":"\ud800"}"#), Code::MalformedJson);
    assert_eq!(rejected(r#"{"a":"\udc00"}"#), Code::MalformedJson);
    assert_eq!(rejected("{\"a\":\"line\nbreak\"}"), Code::MalformedJson);
    assert_eq!(rejected(r#"{"a":01}"#), Code::MalformedJson);
    assert_eq!(rejected(r#"{"a":tru}"#), Code::MalformedJson);
    assert_eq!(rejected(r#"{"a" 1}"#), Code::MalformedJson);
    assert_eq!(rejected(r#"{"a":1} {"b":2}"#), Code::TrailingData);
    assert_eq!(rejected("{}x"), Code::TrailingData);
    assert_eq!(rejected("[1,2]"), Code::RootShape);
    assert_eq!(rejected("null"), Code::RootShape);
    assert_eq!(rejected("[1] 2"), Code::TrailingData);
    assert_eq!(
        parse_claude_long(b"{\"a\":\"\xff\"}", None, LIM),
        ClaudeParseResult::Rejected(Code::InvalidUtf8)
    );
    // A trailing line terminator is ordinary JSON whitespace.
    assert!(long(&format!("{full}\r\n")).observation().is_some());
}

#[test]
fn duplicate_keys_affecting_claims_are_rejected() {
    for text in [
        r#"{"sessionId":"a","sessionId":"b"}"#,
        r#"{"requestId":"a","requestId":"b"}"#,
        r#"{"timestamp":"a","timestamp":"b"}"#,
        r#"{"message":{"id":"a"},"message":{"id":"b"}}"#,
        r#"{"message":{"id":"a","id":"b"}}"#,
        r#"{"message":{"model":"a","model":"b"}}"#,
        r#"{"message":{"usage":{"input_tokens":1},"usage":{"input_tokens":2}}}"#,
        r#"{"message":{"usage":{"input_tokens":1,"input_tokens":2}}}"#,
        r#"{"message":{"usage":{"output_tokens":1,"output_tokens":1}}}"#,
        r#"{"message":{"usage":{"cache_read_input_tokens":1,"cache_read_input_tokens":1}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":1,"cache_creation_input_tokens":1}}}"#,
        r#"{"message":{"usage":{"cache_creation":{},"cache_creation":{}}}}"#,
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_5m_input_tokens":1}}}}"#,
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_1h_input_tokens":1}}}}"#,
        r#"{"sessionId":"a","sessionId":"b"}"#,
        r#"{"message":null,"message":{}}"#,
    ] {
        assert_eq!(rejected(text), Code::DuplicateKey, "{text}");
    }
}

#[test]
fn duplicates_inside_skipped_values_are_a_documented_gap() {
    // The tree parser rejects these; the bounded-memory path cannot afford to.
    let text = r#"{"message":{"content":[{"type":"x","type":"y"}],"usage":{"input_tokens":1}}}"#;
    assert_eq!(
        parse_claude(text.as_bytes(), None, Limits::MAX),
        ClaudeParseResult::Rejected(Code::DuplicateKey)
    );
    assert_eq!(obs(text).input, Field::Valid(1));
    // A claim-shaped key inside skipped content never supplies or collides with a claim.
    let nested = r#"{"sessionId":"top","content":{"sessionId":"inner","sessionId":"inner2"},"message":{"content":{"usage":{"input_tokens":9}}}}"#;
    let o = obs(nested);
    assert_eq!(o.session_id, Field::Valid("top".into()));
    assert_eq!(o.input, Field::Unsupported);
}

#[test]
fn matches_the_tree_parser_within_its_budget() {
    let corpus = [
        "{}",
        r#"{"message":null}"#,
        r#"{"message":[]}"#,
        r#"{"message":{}}"#,
        r#"{"message":{"usage":null}}"#,
        r#"{"message":{"usage":"x"}}"#,
        r#"{"message":{"usage":{}}}"#,
        r#"{"sessionId":7,"requestId":null,"timestamp":{},"message":{"id":[],"model":true}}"#,
        r#"{"requestId":""}"#,
        r#"{"message":{"usage":{"input_tokens":0,"output_tokens":null}}}"#,
        r#"{"message":{"usage":{"input_tokens":-5,"output_tokens":2}}}"#,
        r#"{"message":{"usage":{"input_tokens":-0}}}"#,
        r#"{"message":{"usage":{"input_tokens":1.5,"output_tokens":1e2}}}"#,
        r#"{"message":{"usage":{"input_tokens":18446744073709551615,"output_tokens":18446744073709551616}}}"#,
        r#"{"message":{"usage":{"input_tokens":1,"service_tier":"x","speed":"fast","server_tool_use":{"n":2}}}}"#,
        r#"{"message":{"usage":{"input_tokens":1,"future_tokens":4}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200}}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":{}}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":null}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":5}}}"#,
        r#"{"message":{"usage":{"cache_creation_input_tokens":9,"cache_creation":{"ephemeral_5m_input_tokens":"x","ephemeral_1h_input_tokens":2}}}}"#,
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":18446744073709551615,"ephemeral_1h_input_tokens":1}}}}"#,
        r#"{"message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":1,"ephemeral_24h_input_tokens":4}}}}"#,
        r#"{"message":{"usage":{"input_tokens":1,"input_tokens":2}}}"#,
        r#"{"message":{"id":"a","id":"b"}}"#,
        r#"{"a":}"#,
        r#"{"a":1} x"#,
        "[]",
        "",
        "{",
        "  {\"sessionId\":\"s\"}\n",
    ];
    for text in corpus {
        for correlation in [None, Some("c")] {
            assert_eq!(
                parse_claude_long(text.as_bytes(), correlation, LIM),
                parse_claude(text.as_bytes(), correlation, Limits::MAX),
                "{text}"
            );
        }
    }
}

#[test]
fn content_and_identifiers_are_not_disclosed() {
    let text = record_with_content(
        r#"[{"type":"text","text":"FICTIONAL-SENTINEL-CONTENT","path":"/fictional/path"}]"#,
    );
    let result = parse_claude_long(text.as_bytes(), Some("fict-correlation"), LIM);
    let o = result.observation().unwrap();
    for debug in [format!("{result:?}"), format!("{o:?}")] {
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
            assert!(!debug.contains(secret), "Debug leaked {secret}");
        }
    }
    // Retained bytes are exactly the allowlisted strings: no content is held.
    assert_eq!(
        result.retained_bytes().unwrap(),
        size_of::<ClaudeParseResult>()
            + "assistant".len()
            + "fict-correlation fict-session fict-req 2026-01-02T03:04:05.678Z fict-msg fict-model"
                .len()
            - 5
    );
}

#[test]
fn results_are_deterministic_and_independent() {
    let text = record_with_content("[]");
    assert_eq!(long(&text), long(&text));
    assert_ne!(
        parse_claude_long(text.as_bytes(), Some("a"), LIM),
        parse_claude_long(text.as_bytes(), Some("b"), LIM)
    );
}
