//! ABF140B: fictional identity evidence, independent of token status.
use abf_parser::*;

fn pair(text: &str) -> (ClaudeObservation, ClaudeObservation) {
    let tree = parse_claude(text.as_bytes(), None, Limits::MAX);
    let long = parse_claude_long(text.as_bytes(), None, LongLimits::MAX);
    (
        tree.observation().unwrap().clone(),
        long.observation().unwrap().clone(),
    )
}

#[test]
fn both_parsers_retain_identity_without_changing_token_status() {
    let (tree, long) = pair(
        r#"{"type":"assistant","uuid":"fictional-record","isSidechain":false,"message":{"usage":{"input_tokens":10}}}"#,
    );
    assert_eq!(tree, long);
    assert_eq!(tree.identity.record_type, Field::Valid("assistant".into()));
    assert_eq!(
        tree.identity.record_uuid,
        Field::Valid("fictional-record".into())
    );
    assert_eq!(tree.identity.is_sidechain, Field::Valid(false));
    assert_eq!(tree.status, Status::Accepted);
    assert_eq!(tree.input, Field::Valid(10));
    assert!(!format!("{:?}", tree.identity).contains("fictional-record"));
}

#[test]
fn identity_states_are_explicit_and_do_not_qualify_token_claims() {
    let cases = [
        ("", Field::Absent, Field::Absent, Field::Absent),
        (
            r#""type":null,"uuid":null,"isSidechain":null,"#,
            Field::Null,
            Field::Null,
            Field::Null,
        ),
        (
            r#""type":7,"uuid":[],"isSidechain":"false","#,
            Field::WrongType,
            Field::WrongType,
            Field::WrongType,
        ),
        (
            r#""type":"","uuid":"","isSidechain":0,"#,
            Field::WrongType,
            Field::WrongType,
            Field::WrongType,
        ),
    ];
    for (fields, kind, uuid, sidechain) in cases {
        let (tree, long) = pair(&format!(
            r#"{{{fields}"message":{{"usage":{{"input_tokens":10}}}}}}"#
        ));
        assert_eq!(tree, long);
        assert_eq!(tree.identity.record_type, kind);
        assert_eq!(tree.identity.record_uuid, uuid);
        assert_eq!(tree.identity.is_sidechain, sidechain);
        assert_eq!(tree.status, Status::Accepted);
        assert_eq!(tree.diagnostics.bits(), 0);
    }
}

#[test]
fn oversized_new_identity_fields_do_not_reject_previously_readable_claims() {
    let text = format!(
        r#"{{"type":"{}","uuid":"{}","message":{{"usage":{{"input_tokens":10}}}}}}"#,
        "a".repeat(257),
        "b".repeat(257)
    );
    let (tree, long) = pair(&text);
    assert_eq!(tree, long);
    assert_eq!(tree.identity.record_type, Field::OutOfRange);
    assert_eq!(tree.identity.record_uuid, Field::OutOfRange);
    assert_eq!(tree.input, Field::Valid(10));
    assert_eq!(tree.status, Status::Accepted);
}

#[test]
fn streaming_duplicate_identity_fields_are_unusable_without_losing_raw_claims() {
    for fields in [
        r#""type":"assistant","type":"assistant","#,
        r#""uuid":"a","u\u0075id":"a","#,
        r#""isSidechain":false,"isSidechain":false,"#,
    ] {
        let text = format!(r#"{{{fields}"message":{{"usage":{{"input_tokens":10}}}}}}"#);
        let parsed = parse_claude_long(text.as_bytes(), None, LongLimits::MAX);
        let o = parsed.observation().unwrap();
        assert!(
            o.identity.record_type == Field::Unsupported
                || o.identity.record_uuid == Field::Unsupported
                || o.identity.is_sidechain == Field::Unsupported
        );
        assert_eq!(o.input, Field::Valid(10));
        assert_eq!(o.status, Status::Accepted);
        assert_eq!(
            parse_claude(text.as_bytes(), None, Limits::MAX),
            ClaudeParseResult::Rejected(Code::DuplicateKey)
        );
    }
}

#[test]
fn retained_identity_bytes_share_the_existing_result_ceiling() {
    let value = "x".repeat(256);
    let text = format!(
        r#"{{"type":"{value}","uuid":"{value}","isSidechain":true,"message":{{"usage":{{"input_tokens":10}}}}}}"#
    );
    let (tree, long) = pair(&text);
    assert_eq!(tree, long);
    let result = parse_claude_long(text.as_bytes(), None, LongLimits::MAX);
    assert_eq!(
        result.retained_bytes(),
        Some(size_of::<ClaudeParseResult>() + 512)
    );
    assert!(result.retained_bytes().unwrap() <= 4096);
    assert_eq!(tree.identity.is_sidechain, Field::Valid(true));
    assert!(!format!("{result:?}").contains(&value));
}
