mod support;
use abf_identity_relations::{Code, Limits, Occurrence, SyntheticSet, UnverifiedHint};
use abf_observation::{EnvironmentKind, SourceRunStatus};
use support::*;

#[test]
fn debug_of_every_new_data_type_redacts_all_synthetic_claims() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("fictional_private_handle"), envelope(br#"{"id":"fictional_private_message","sessionID":"fictional_private_session","modelID":"fictional_private_model","providerID":"fictional_private_provider","prompt":"fictional_raw_prompt","tokens":{"input":8}}"#, EnvironmentKind::Windows, SourceRunStatus::Failed))).unwrap();
    set.insert(Occurrence::new(id("fictional_second_handle"), ordinary()))
        .unwrap();
    set.assert_relation(
        id("fictional_private_handle"),
        id("fictional_second_handle"),
        UnverifiedHint::MatchingClaimedIdentifiers,
    )
    .unwrap();
    let debug = [
        format!("{set:?}"),
        format!("{:?}", set.occurrences()[0]),
        format!("{:?}", set.relations()[0]),
    ];
    for output in debug {
        for claim in [
            "fictional_private",
            "fictional_second",
            "fictional_raw",
            "fictional_parser_claim",
            "fictional_context_claim",
            "fictional_env",
            "fictional_source",
            "fictional_run",
            "fictional_message",
            "fictional_session",
            "{\"id\"",
        ] {
            assert!(!output.contains(claim), "claim exposed: {claim}");
        }
        assert!(output.contains("[redacted]"));
    }
    assert_eq!(
        set.occurrences()[0]
            .envelope()
            .parser_result()
            .observation()
            .unwrap()
            .message_id,
        abf_parser::Field::Valid("fictional_private_message".into())
    );
}

#[test]
fn rejected_insert_error_contains_only_code_and_leaves_empty_set() {
    let mut set = SyntheticSet::new(Limits {
        occurrences: 0,
        relations: 0,
    })
    .unwrap();
    let err = set
        .insert(Occurrence::new(id("fictional_private_handle"), ordinary()))
        .unwrap_err();
    assert_eq!(format!("{err:?}"), "OccurrenceCapacity");
    let code: Code = err;
    assert_eq!(code, Code::OccurrenceCapacity);
    assert!(set.occurrences().is_empty());
    assert!(set.relations().is_empty());
}

#[test]
fn all_error_codes_have_fixed_metadata_free_debug() {
    for (code, text) in [
        (Code::InvalidLimits, "InvalidLimits"),
        (Code::DuplicateHandle, "DuplicateHandle"),
        (Code::OccurrenceCapacity, "OccurrenceCapacity"),
        (Code::RelationCapacity, "RelationCapacity"),
        (Code::SelfLink, "SelfLink"),
        (Code::MissingEndpoint, "MissingEndpoint"),
        (Code::AllocationFailure, "AllocationFailure"),
    ] {
        assert_eq!(format!("{code:?}"), text);
    }
}
