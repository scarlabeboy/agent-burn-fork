mod support;
use abf_identity_relations::{Code, Limits, Occurrence, SyntheticSet};
use abf_observation::{EnvironmentKind, Inconsistency, SourceRunStatus};
use abf_parser::{Code as ParserCode, Diagnostic, Field, ParseResult, Status};
use support::*;

// Behavior sketch: preserve occurrences; reject duplicate handles atomically;
// retain statuses/rejections/diagnostics; bound collections; validate explicit hints;
// redact claims and retain deterministic insertion order.
#[test]
fn equal_claims_are_independent_occurrences_without_automatic_hints() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("one"), ordinary())).unwrap();
    set.insert(Occurrence::new(id("two"), ordinary())).unwrap();
    assert_eq!(set.occurrences().len(), 2);
    assert_eq!(set.occurrences()[0].handle().as_str(), "one");
    assert_eq!(set.occurrences()[1].handle().as_str(), "two");
    for occurrence in set.occurrences() {
        let o = occurrence.envelope().parser_result().observation().unwrap();
        assert_eq!(o.message_id, Field::Valid("fictional_message".into()));
        assert_eq!(o.session_id, Field::Valid("fictional_session".into()));
        assert_eq!(o.input, Field::Valid(7));
        assert_eq!(o.total, Field::Valid(3));
    }
    assert!(set.relations().is_empty());
}

#[test]
fn duplicate_handle_returns_code_only_and_keeps_first_intact() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("same"), ordinary())).unwrap();
    let failed = envelope(
        b"broken",
        EnvironmentKind::SelectedWsl,
        SourceRunStatus::Failed,
    );
    let error = set.insert(Occurrence::new(id("same"), failed)).unwrap_err();
    let code: Code = error;
    assert_eq!(code, Code::DuplicateHandle);
    assert_eq!(set.occurrences()[0].envelope(), &ordinary());
    assert_eq!(set.occurrences().len(), 1);
    assert_eq!(
        set.occurrences()[0].envelope().source_status(),
        SourceRunStatus::Complete
    );
    assert!(set.relations().is_empty());
}

#[test]
fn windows_and_selected_wsl_stay_separate_with_equal_source_claims() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    for (handle, kind) in [
        ("win", EnvironmentKind::Windows),
        ("wsl", EnvironmentKind::SelectedWsl),
    ] {
        set.insert(Occurrence::new(
            id(handle),
            envelope(b"{}", kind, SourceRunStatus::Complete),
        ))
        .unwrap();
    }
    assert_eq!(set.occurrences().len(), 2);
    assert_eq!(
        set.occurrences()[0].envelope().context().environment_kind(),
        EnvironmentKind::Windows
    );
    assert_eq!(
        set.occurrences()[1].envelope().context().environment_kind(),
        EnvironmentKind::SelectedWsl
    );
    assert!(set.relations().is_empty());
}

#[test]
fn rejected_records_and_every_source_status_remain_inspectable() {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    let cases = [
        (
            SourceRunStatus::NotCollected,
            Some(Inconsistency::RecordWithoutReportedTraversal),
        ),
        (SourceRunStatus::Complete, None),
        (SourceRunStatus::Partial, None),
        (SourceRunStatus::Failed, None),
        (
            SourceRunStatus::Unavailable,
            Some(Inconsistency::RecordWithoutReportedTraversal),
        ),
    ];
    for (i, (status, _)) in cases.iter().enumerate() {
        set.insert(Occurrence::new(
            id(&format!("r{i}")),
            envelope(b"[]", EnvironmentKind::Windows, *status),
        ))
        .unwrap();
    }
    for (occurrence, (status, inconsistency)) in set.occurrences().iter().zip(cases) {
        assert_eq!(
            occurrence.envelope().parser_result(),
            &ParseResult::Rejected(ParserCode::RootShape)
        );
        assert_eq!(occurrence.envelope().source_status(), status);
        assert_eq!(occurrence.envelope().inconsistency(), inconsistency);
    }
    assert_eq!(set.occurrences().len(), 5);
}

#[test]
fn fields_diagnostics_and_owned_string_addresses_survive_insertion() {
    let e = envelope(br#"{"id":"fictional_unique","modelID":false,"time":{"created":-7},"tokens":{"input":18446744073709551615,"output":null,"total":0,"other":2}}"#, EnvironmentKind::SelectedWsl, SourceRunStatus::Partial);
    let pointer = match &e.parser_result().observation().unwrap().message_id {
        Field::Valid(s) => s.as_ptr(),
        _ => panic!("fixture"),
    };
    let context_pointer = e.context().source_ref().as_str().as_ptr();
    let expected = e.clone();
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("kept"), e)).unwrap();
    let retained = set.occurrences()[0].envelope();
    assert_eq!(retained, &expected);
    assert_eq!(
        retained.context().source_ref().as_str().as_ptr(),
        context_pointer
    );
    let o = retained.parser_result().observation().unwrap();
    match &o.message_id {
        Field::Valid(s) => assert_eq!(s.as_ptr(), pointer),
        _ => panic!("lost claim"),
    }
    assert_eq!(o.created, Field::Valid(-7));
    assert_eq!(o.input, Field::Valid(u64::MAX));
    assert_eq!(o.output, Field::Null);
    assert_eq!(o.total, Field::Valid(0));
    assert_eq!(o.status, Status::Qualified);
    assert!(o.diagnostics.contains(Diagnostic::InvalidField));
    assert!(o.diagnostics.contains(Diagnostic::ExplicitNull));
    assert!(o.diagnostics.contains(Diagnostic::UnsupportedComponent));
    assert_eq!(o.diagnostics.bits(), 7);
}
