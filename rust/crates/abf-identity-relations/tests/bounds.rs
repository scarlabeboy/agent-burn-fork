mod support;
use abf_identity_relations::{Code, Limits, Occurrence, SyntheticSet};
use abf_observation::{ContextCode, OpaqueRef};
use support::*;

#[test]
fn occurrence_hard_ceiling_accepts_128_then_rejects_overflow_atomically() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    for n in 0..128 {
        set.insert(Occurrence::new(id(&format!("o{n}")), ordinary()))
            .unwrap();
    }
    let error = set
        .insert(Occurrence::new(id("overflow"), ordinary()))
        .unwrap_err();
    assert_eq!(error, Code::OccurrenceCapacity);
    assert_eq!(set.occurrences().len(), 128);
    for (n, o) in set.occurrences().iter().enumerate() {
        assert_eq!(o.handle().as_str(), format!("o{n}"));
        assert_eq!(o.envelope(), &ordinary());
    }
    assert!(set.relations().is_empty());
}

#[test]
fn lowered_occurrence_ceiling_is_enforced_without_replacing_first() {
    let mut set = SyntheticSet::new(Limits {
        occurrences: 1,
        relations: 0,
    })
    .unwrap();
    set.insert(Occurrence::new(id("one"), ordinary())).unwrap();
    assert_eq!(
        set.insert(Occurrence::new(id("two"), ordinary()))
            .unwrap_err(),
        Code::OccurrenceCapacity
    );
    assert_eq!(set.occurrences().len(), 1);
    assert_eq!(set.occurrences()[0].handle().as_str(), "one");
}

#[test]
fn zero_occurrence_limit_rejects_first_insertion() {
    let mut set = SyntheticSet::new(Limits {
        occurrences: 0,
        relations: 0,
    })
    .unwrap();
    assert_eq!(
        set.insert(Occurrence::new(id("one"), ordinary()))
            .unwrap_err(),
        Code::OccurrenceCapacity
    );
    assert!(set.occurrences().is_empty());
}

#[test]
fn raised_or_usize_max_limits_are_rejected_before_allocation() {
    for limits in [
        Limits {
            occurrences: 129,
            relations: 256,
        },
        Limits {
            occurrences: 128,
            relations: 257,
        },
        Limits {
            occurrences: usize::MAX,
            relations: 0,
        },
        Limits {
            occurrences: 0,
            relations: usize::MAX,
        },
    ] {
        assert_eq!(SyntheticSet::new(limits).unwrap_err(), Code::InvalidLimits);
    }
}

#[test]
fn handle_validation_reuses_existing_exact_codes_and_64_byte_ceiling() {
    assert_eq!(OpaqueRef::new(""), Err(ContextCode::Empty));
    assert_eq!(OpaqueRef::new(&"x".repeat(65)), Err(ContextCode::TooLong));
    for text in ["a/b", "a\\b", "a.b", "a b", "a\n", "é", "\0"] {
        assert_eq!(OpaqueRef::new(text), Err(ContextCode::Malformed));
    }
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    let text = "x".repeat(64);
    set.insert(Occurrence::new(id(&text), ordinary())).unwrap();
    assert_eq!(set.occurrences()[0].handle().as_str(), text);
}

#[test]
fn generated_reference_inputs_have_independent_expected_codes_without_panics() {
    for scalar in 0..=255u8 {
        for length in [0, 1, 63, 64, 65, 256] {
            let text = char::from(scalar).to_string().repeat(length);
            let expected = match text.len() {
                0 => Some(ContextCode::Empty),
                65.. => Some(ContextCode::TooLong),
                _ => match scalar {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => None,
                    _ => Some(ContextCode::Malformed),
                },
            };
            assert_eq!(OpaqueRef::new(&text).err(), expected);
        }
    }
}

#[test]
fn assertion_ceiling_counts_repeats_and_rejects_257th_atomically() {
    use abf_identity_relations::{UnverifiedHint, Verification};
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("a"), ordinary())).unwrap();
    set.insert(Occurrence::new(id("b"), ordinary())).unwrap();
    for _ in 0..256 {
        set.assert_relation(id("a"), id("b"), UnverifiedHint::EqualReportedValues)
            .unwrap();
    }
    assert_eq!(
        set.assert_relation(
            id("b"),
            id("a"),
            UnverifiedHint::DifferentClaimedIdentifiers
        ),
        Err(Code::RelationCapacity)
    );
    assert_eq!(set.relations().len(), 256);
    for r in set.relations() {
        assert_eq!(r.from().as_str(), "a");
        assert_eq!(r.to().as_str(), "b");
        assert_eq!(r.hint(), UnverifiedHint::EqualReportedValues);
        assert_eq!(r.verification(), Verification::Unverified);
    }
    assert_eq!(set.occurrences().len(), 2);
    for o in set.occurrences() {
        assert_eq!(o.envelope(), &ordinary());
    }
}

#[test]
fn lowered_and_zero_relation_limits_do_not_affect_occurrences() {
    use abf_identity_relations::UnverifiedHint;
    for limit in [0, 1, 3] {
        let mut set = SyntheticSet::new(Limits {
            occurrences: 2,
            relations: limit,
        })
        .unwrap();
        set.insert(Occurrence::new(id("a"), ordinary())).unwrap();
        set.insert(Occurrence::new(id("b"), ordinary())).unwrap();
        for _ in 0..limit {
            set.assert_relation(id("a"), id("b"), UnverifiedHint::Unclassified)
                .unwrap();
        }
        assert_eq!(
            set.assert_relation(id("a"), id("b"), UnverifiedHint::Unclassified),
            Err(Code::RelationCapacity)
        );
        assert_eq!(set.relations().len(), limit);
        assert_eq!(set.occurrences().len(), 2);
        for o in set.occurrences() {
            assert_eq!(o.envelope(), &ordinary());
        }
    }
}

#[test]
fn failure_precedence_at_full_limits_is_deterministic_and_preserves_evidence() {
    use abf_identity_relations::UnverifiedHint;
    let mut set = SyntheticSet::new(Limits {
        occurrences: 2,
        relations: 1,
    })
    .unwrap();
    set.insert(Occurrence::new(id("a"), ordinary())).unwrap();
    set.insert(Occurrence::new(id("b"), ordinary())).unwrap();
    set.assert_relation(id("a"), id("b"), UnverifiedHint::Unclassified)
        .unwrap();
    assert_eq!(
        set.insert(Occurrence::new(id("a"), ordinary()))
            .unwrap_err(),
        Code::DuplicateHandle
    );
    assert_eq!(
        set.assert_relation(id("missing"), id("missing"), UnverifiedHint::Unclassified),
        Err(Code::SelfLink)
    );
    assert_eq!(
        set.assert_relation(id("a"), id("missing"), UnverifiedHint::Unclassified),
        Err(Code::MissingEndpoint)
    );
    assert_eq!(set.relations().len(), 1);
    assert_eq!(set.relations()[0].from().as_str(), "a");
    assert_eq!(set.relations()[0].to().as_str(), "b");
    assert_eq!(set.occurrences().len(), 2);
    for o in set.occurrences() {
        assert_eq!(o.envelope(), &ordinary());
    }
}

#[test]
fn maximum_payloads_and_both_collection_ceilings_coexist_without_numeric_changes() {
    use abf_identity_relations::UnverifiedHint;
    use abf_observation::{DeclaredContext, EnvironmentKind, SourceRunStatus, from_opencode};
    use abf_parser::{Field, Limits as ParserLimits, parse};
    let text = "x".repeat(256);
    let frame = format!(
        r#"{{"id":"{text}","sessionID":"{text}","modelID":"{text}","providerID":"{text}","time":{{"created":-9223372036854775808}},"tokens":{{"input":18446744073709551615,"output":18446744073709551615,"total":0,"cache":{{"read":18446744073709551615,"write":18446744073709551615}}}}}}"#
    );
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    let handles: Vec<_> = (0..128).map(|n| format!("{n:064}")).collect();
    for handle in &handles {
        let ctx = DeclaredContext::new(
            EnvironmentKind::Windows,
            id(&"e".repeat(64)),
            id(&"s".repeat(64)),
            id(&"r".repeat(64)),
            Some(id(&"c".repeat(64))),
            Some(u64::MAX),
        );
        let e = from_opencode(
            parse(frame.as_bytes(), Some(&text), ParserLimits::MAX),
            ctx,
            SourceRunStatus::Partial,
        )
        .unwrap();
        set.insert(Occurrence::new(id(handle), e)).unwrap();
    }
    assert!(set.relations().is_empty());
    for _ in 0..256 {
        set.assert_relation(
            id(&handles[0]),
            id(&handles[127]),
            UnverifiedHint::EqualReportedValues,
        )
        .unwrap();
    }
    assert_eq!(
        set.insert(Occurrence::new(id("overflow"), ordinary()))
            .unwrap_err(),
        Code::OccurrenceCapacity
    );
    assert_eq!(
        set.assert_relation(
            id(&handles[0]),
            id(&handles[127]),
            UnverifiedHint::Unclassified
        ),
        Err(Code::RelationCapacity)
    );
    assert_eq!(set.occurrences().len(), 128);
    assert_eq!(set.relations().len(), 256);
    for (o, handle) in set.occurrences().iter().zip(&handles) {
        assert_eq!(o.handle().as_str(), handle);
        let parsed = o.envelope().parser_result().observation().unwrap();
        assert_eq!(parsed.message_id, Field::Valid(text.clone().into()));
        assert_eq!(parsed.session_id, Field::Valid(text.clone().into()));
        assert_eq!(parsed.model_id, Field::Valid(text.clone().into()));
        assert_eq!(parsed.provider_id, Field::Valid(text.clone().into()));
        assert_eq!(parsed.correlation.as_deref(), Some(text.as_str()));
        assert_eq!(parsed.created, Field::Valid(i64::MIN));
        assert_eq!(parsed.input, Field::Valid(u64::MAX));
        assert_eq!(parsed.output, Field::Valid(u64::MAX));
        assert_eq!(parsed.cache_read, Field::Valid(u64::MAX));
        assert_eq!(parsed.cache_write, Field::Valid(u64::MAX));
        assert_eq!(parsed.total, Field::Valid(0));
        assert_eq!(o.envelope().source_status(), SourceRunStatus::Partial);
        assert_eq!(o.envelope().context().ordinal(), Some(u64::MAX));
    }
    for r in set.relations() {
        assert_eq!(r.from().as_str(), handles[0]);
        assert_eq!(r.to().as_str(), handles[127]);
        assert_eq!(r.hint(), UnverifiedHint::EqualReportedValues);
    }
}
