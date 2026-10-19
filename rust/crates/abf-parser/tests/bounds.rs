use abf_parser::*;

#[test]
fn frame_limit_precedes_decoding() {
    let mut frame = vec![b' '; 65536];
    frame[..2].copy_from_slice(b"{}");
    assert!(parse(&frame, None, Limits::MAX).observation().is_some());
    frame.push(255);
    assert_eq!(
        parse(&frame, None, Limits::MAX),
        ParseResult::Rejected(Code::FrameBudget)
    );
}

#[test]
fn depth_limit_applies_to_ignored_objects_and_arrays() {
    for (open, close) in [("[", "]"), ("{\"x\":", "}")] {
        for (levels, accepted) in [(15, true), (16, false)] {
            let text = format!(
                "{{\"ignored\":{}0{}}}",
                open.repeat(levels),
                close.repeat(levels)
            );
            assert_eq!(
                !matches!(
                    parse(text.as_bytes(), None, Limits::MAX),
                    ParseResult::Rejected(_)
                ),
                accepted
            );
        }
    }
}

#[test]
fn combined_entry_budget_includes_all_ignored_content() {
    for (count, accepted) in [(255, true), (256, false)] {
        let text = format!("{{\"ignored\":[{}]}}", vec!["0"; count].join(","));
        assert_eq!(
            !matches!(
                parse(text.as_bytes(), None, Limits::MAX),
                ParseResult::Rejected(_)
            ),
            accepted
        );
    }
    for (count, accepted) in [(256, true), (257, false)] {
        let members: Vec<_> = (0..count).map(|i| format!("\"k{i}\":0")).collect();
        let text = format!("{{{}}}", members.join(","));
        assert_eq!(
            !matches!(
                parse(text.as_bytes(), None, Limits::MAX),
                ParseResult::Rejected(_)
            ),
            accepted
        );
    }
}

#[test]
fn metadata_and_correlation_limits_use_decoded_utf8_bytes() {
    for value in [
        "a".repeat(256),
        "é".repeat(128),
        "\\u0061".repeat(256),
        "\\ud83d\\ude00".repeat(64),
    ] {
        let input = format!("{{\"id\":\"{value}\"}}");
        assert!(
            parse(input.as_bytes(), None, Limits::MAX)
                .observation()
                .is_some()
        );
    }
    for value in [
        "a".repeat(257),
        "é".repeat(129),
        "\\u0061".repeat(257),
        "\\ud83d\\ude00".repeat(65),
    ] {
        let input = format!("{{\"id\":\"{value}\"}}");
        assert_eq!(
            parse(input.as_bytes(), None, Limits::MAX),
            ParseResult::Rejected(Code::StringBudget)
        );
    }
    assert!(
        parse(b"{}", Some(&"x".repeat(256)), Limits::MAX)
            .observation()
            .is_some()
    );
    assert_eq!(
        parse(b"{}", Some(&"x".repeat(257)), Limits::MAX),
        ParseResult::Rejected(Code::StringBudget)
    );
}

#[test]
fn ignored_strings_are_bounded_by_the_frame_not_metadata_limit() {
    let text = format!("{{\"ignored\":\"{}\"}}", "x".repeat(65000));
    assert!(
        parse(text.as_bytes(), None, Limits::MAX)
            .observation()
            .is_some()
    );
    let escaped = format!("{{\"ignored\":\"{}\"}}", "\\u0061".repeat(10000));
    assert!(
        parse(escaped.as_bytes(), None, Limits::MAX)
            .observation()
            .is_some()
    );
}

#[test]
fn output_budget_and_diagnostic_saturation_are_bounded() {
    let input = format!(
        r#"{{"id":"{}","sessionID":"{}","modelID":"{}","providerID":"{}","time":false,"tokens":{{"input":null,"output":[],"total":-1,"cache":null,{}}}}}"#,
        "a".repeat(256),
        "b".repeat(256),
        "c".repeat(256),
        "d".repeat(256),
        (0..200)
            .map(|i| format!("\"unknown{i}\":null"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let result = parse(input.as_bytes(), Some(&"m".repeat(256)), Limits::MAX);
    let size = result.retained_bytes().unwrap();
    assert!(size <= 4096);
    let exact = Limits {
        result_bytes: size,
        ..Limits::MAX
    };
    assert_eq!(
        parse(input.as_bytes(), Some(&"m".repeat(256)), exact),
        result
    );
    assert_eq!(
        parse(
            input.as_bytes(),
            Some(&"m".repeat(256)),
            Limits {
                result_bytes: size - 1,
                ..exact
            }
        ),
        ParseResult::Rejected(Code::ResultBudget)
    );
    assert!(
        result
            .observation()
            .unwrap()
            .diagnostics
            .bits()
            .count_ones()
            <= 4
    );
}

#[test]
fn invalid_limits_and_overflow_cannot_expand_hard_caps() {
    for limits in [
        Limits {
            frame_bytes: usize::MAX,
            ..Limits::MAX
        },
        Limits {
            depth: usize::MAX,
            ..Limits::MAX
        },
        Limits {
            entries: usize::MAX,
            ..Limits::MAX
        },
        Limits {
            string_bytes: usize::MAX,
            ..Limits::MAX
        },
        Limits {
            result_bytes: usize::MAX,
            ..Limits::MAX
        },
    ] {
        assert_eq!(
            parse(b"{}", None, limits),
            ParseResult::Rejected(Code::InvalidLimits)
        );
    }
    let result = parse(br#"{"tokens":{"input":18446744073709551615,"output":18446744073709551615,"cache":{"read":18446744073709551615,"write":18446744073709551615},"total":18446744073709551615}}"#, None, Limits::MAX);
    assert_eq!(result.observation().unwrap().total, Field::Valid(u64::MAX));
}

#[test]
fn smaller_explicit_budgets_are_enforced() {
    assert_eq!(
        parse(
            b"{}",
            None,
            Limits {
                frame_bytes: 1,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::FrameBudget)
    );
    assert_eq!(
        parse(
            b"{}",
            None,
            Limits {
                depth: 0,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::DepthBudget)
    );
    assert_eq!(
        parse(
            b"{\"x\":0}",
            None,
            Limits {
                entries: 0,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::EntryBudget)
    );
    assert_eq!(
        parse(
            b"{\"id\":\"x\"}",
            None,
            Limits {
                string_bytes: 0,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::StringBudget)
    );
}

#[test]
fn duplicate_stress_and_long_numbers_remain_coded() {
    let text = format!("{{\"x\":0,{}}}", vec!["\"x\":0"; 250].join(","));
    assert_eq!(
        parse(text.as_bytes(), None, Limits::MAX),
        ParseResult::Rejected(Code::DuplicateKey)
    );
    let text = format!("{{\"tokens\":{{\"input\":{}}}}}", "9".repeat(60000));
    assert_eq!(
        parse(text.as_bytes(), None, Limits::MAX)
            .observation()
            .unwrap()
            .input,
        Field::OutOfRange
    );
}

#[test]
fn deterministic_generated_bytes_never_panic_or_exceed_output_budget() {
    let mut seed = 0x9596_1234_u64;
    for length in 0..512 {
        let bytes: Vec<_> = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as u8
            })
            .collect();
        let a = parse(&bytes, None, Limits::MAX);
        assert_eq!(a, parse(&bytes, None, Limits::MAX));
        assert!(a.retained_bytes().unwrap() <= 4096);
    }
}

#[test]
fn generated_structured_variations_preserve_unrelated_claims() {
    for n in 0..256 {
        let input = format!(
            "{{\"ignored\":[null,{{\"input\":{n}}}],\"tokens\":{{\"input\":{n},\"output\":0,\"total\":1}}}}"
        );
        let result = parse(input.as_bytes(), None, Limits::MAX);
        let o = result.observation().unwrap();
        assert_eq!(o.input, Field::Valid(n));
        assert_eq!(o.output, Field::Valid(0));
        assert_eq!(o.total, Field::Valid(1));
    }
}

#[test]
fn result_ceiling_and_fixed_layout_are_explicit() {
    let result = parse(b"{}", None, Limits::MAX);
    let inline = size_of::<ParseResult>();
    assert_eq!(result.retained_bytes(), Some(inline));
    assert!(inline.checked_add(5 * 256).unwrap() <= 4096);
    assert!(
        parse(
            b"{}",
            None,
            Limits {
                result_bytes: 4096,
                ..Limits::MAX
            }
        )
        .observation()
        .is_some()
    );
    assert_eq!(
        parse(
            b"{}",
            None,
            Limits {
                result_bytes: 4097,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::InvalidLimits)
    );
    assert_eq!(
        parse(
            b"{}",
            None,
            Limits {
                result_bytes: inline - 1,
                ..Limits::MAX
            }
        ),
        ParseResult::Rejected(Code::InvalidLimits)
    );
}

#[test]
fn generated_ascii_and_single_byte_mutations_are_deterministic() {
    let baseline = br#"{"id":"synthetic-\ud83d\ude00","time":{"created":-1},"tokens":{"input":0,"cache":{"read":1}},"ignored":[true,null,{"x":"\\"}]}"#;
    for end in 0..baseline.len() {
        assert!(matches!(
            parse(&baseline[..end], None, Limits::MAX),
            ParseResult::Rejected(_)
        ));
    }
    for index in 0..baseline.len() {
        for byte in 0..=127u8 {
            let mut mutated = baseline.to_vec();
            mutated[index] = byte;
            let result = parse(&mutated, None, Limits::MAX);
            assert_eq!(result, parse(&mutated, None, Limits::MAX));
            assert!(result.retained_bytes().unwrap() <= 4096);
        }
    }
    let mut seed = 96u64;
    for length in 0..512 {
        let text: Vec<_> = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((seed >> 32) & 127) as u8
            })
            .collect();
        assert_eq!(
            parse(&text, None, Limits::MAX),
            parse(&text, None, Limits::MAX)
        );
    }
}
