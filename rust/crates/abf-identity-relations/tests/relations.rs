mod support;
use abf_identity_relations::{
    Code, Limits, Occurrence, SyntheticSet, UnverifiedHint, Verification,
};
use abf_observation::{EnvironmentKind, SourceRunStatus};
use support::*;

fn pair() -> SyntheticSet {
    let mut set = SyntheticSet::new(Limits::MAX).unwrap();
    set.insert(Occurrence::new(id("a"), ordinary())).unwrap();
    set.insert(Occurrence::new(
        id("b"),
        envelope(
            br#"{"id":"different_message","tokens":{"input":99}}"#,
            EnvironmentKind::SelectedWsl,
            SourceRunStatus::Partial,
        ),
    ))
    .unwrap();
    set
}

#[test]
fn explicit_cross_environment_assertion_stays_unverified_even_when_claim_is_false() {
    let mut set = pair();
    let before: Vec<_> = set
        .occurrences()
        .iter()
        .map(|o| o.envelope().clone())
        .collect();
    assert!(set.relations().is_empty());
    set.assert_relation(id("a"), id("b"), UnverifiedHint::MatchingClaimedIdentifiers)
        .unwrap();
    let relation = &set.relations()[0];
    assert_eq!(relation.from().as_str(), "a");
    assert_eq!(relation.to().as_str(), "b");
    assert_eq!(relation.hint(), UnverifiedHint::MatchingClaimedIdentifiers);
    assert_eq!(relation.verification(), Verification::Unverified);
    assert_eq!(set.occurrences().len(), 2);
    for (o, expected) in set.occurrences().iter().zip(&before) {
        assert_eq!(o.envelope(), expected);
    }
    assert_eq!(
        set.occurrences()[0].envelope().context().environment_kind(),
        EnvironmentKind::Windows
    );
    assert_eq!(
        set.occurrences()[1].envelope().context().environment_kind(),
        EnvironmentKind::SelectedWsl
    );
}

#[test]
fn missing_either_endpoint_is_rejected_without_changing_occurrences_or_relations() {
    let mut set = pair();
    set.assert_relation(id("a"), id("b"), UnverifiedHint::Unclassified)
        .unwrap();
    for (from, to) in [("missing", "a"), ("a", "missing"), ("x", "y")] {
        assert_eq!(
            set.assert_relation(id(from), id(to), UnverifiedHint::EqualReportedValues),
            Err(Code::MissingEndpoint)
        );
        assert_eq!(set.relations().len(), 1);
        assert_eq!(set.relations()[0].hint(), UnverifiedHint::Unclassified);
        assert_eq!(set.occurrences().len(), 2);
        assert_eq!(set.occurrences()[0].envelope(), &ordinary());
    }
}

#[test]
fn self_link_has_fixed_precedence_over_missing_endpoint() {
    let mut set = pair();
    for handle in ["a", "missing"] {
        assert_eq!(
            set.assert_relation(id(handle), id(handle), UnverifiedHint::Unclassified),
            Err(Code::SelfLink)
        );
    }
    assert!(set.relations().is_empty());
    assert_eq!(set.occurrences().len(), 2);
}

#[test]
fn repeats_reversed_order_and_conflicting_assertions_are_preserved_in_order() {
    let mut set = pair();
    let assertions = [
        ("a", "b", UnverifiedHint::MatchingClaimedIdentifiers),
        ("a", "b", UnverifiedHint::MatchingClaimedIdentifiers),
        ("b", "a", UnverifiedHint::MatchingClaimedIdentifiers),
        ("a", "b", UnverifiedHint::DifferentClaimedIdentifiers),
        ("a", "b", UnverifiedHint::EqualReportedValues),
        ("a", "b", UnverifiedHint::PossibleReplayOrCopy),
        ("a", "b", UnverifiedHint::Unclassified),
    ];
    let before: Vec<_> = set
        .occurrences()
        .iter()
        .map(|o| o.envelope().clone())
        .collect();
    for (from, to, hint) in assertions {
        set.assert_relation(id(from), id(to), hint).unwrap();
    }
    assert_eq!(set.relations().len(), 7);
    for (r, (from, to, hint)) in set.relations().iter().zip(assertions) {
        assert_eq!(r.from().as_str(), from);
        assert_eq!(r.to().as_str(), to);
        assert_eq!(r.hint(), hint);
        assert_eq!(r.verification(), Verification::Unverified);
    }
    assert_eq!(set.occurrences().len(), 2);
    for (o, expected) in set.occurrences().iter().zip(&before) {
        assert_eq!(o.envelope(), expected);
    }
}

#[test]
fn deterministic_operation_sequences_preserve_explicit_endpoint_order() {
    let apply = || {
        let mut set = pair();
        for (a, b) in [("b", "a"), ("a", "b"), ("b", "a")] {
            set.assert_relation(id(a), id(b), UnverifiedHint::PossibleReplayOrCopy)
                .unwrap();
        }
        set
    };
    let first = apply();
    let second = apply();
    let actual: Vec<_> = first
        .relations()
        .iter()
        .map(|r| {
            (
                r.from().as_str(),
                r.to().as_str(),
                r.hint(),
                r.verification(),
            )
        })
        .collect();
    let repeated: Vec<_> = second
        .relations()
        .iter()
        .map(|r| {
            (
                r.from().as_str(),
                r.to().as_str(),
                r.hint(),
                r.verification(),
            )
        })
        .collect();
    assert_eq!(actual, repeated);
    assert_eq!(
        actual.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
        [("b", "a"), ("a", "b"), ("b", "a")]
    );
    for (a, b) in first.occurrences().iter().zip(second.occurrences()) {
        assert_eq!(a.handle(), b.handle());
        assert_eq!(a.envelope(), b.envelope());
    }
}

#[test]
fn relations_can_reference_rejected_parser_occurrences() {
    let mut set = pair();
    set.insert(Occurrence::new(
        id("rejected"),
        envelope(
            b"[]",
            EnvironmentKind::Windows,
            SourceRunStatus::Unavailable,
        ),
    ))
    .unwrap();
    set.assert_relation(id("a"), id("rejected"), UnverifiedHint::EqualReportedValues)
        .unwrap();
    assert_eq!(set.relations()[0].verification(), Verification::Unverified);
    assert_eq!(
        set.occurrences()[2].envelope().parser_result(),
        &abf_parser::ParseResult::Rejected(abf_parser::Code::RootShape)
    );
    assert_eq!(
        set.occurrences()[2].envelope().source_status(),
        SourceRunStatus::Unavailable
    );
}
