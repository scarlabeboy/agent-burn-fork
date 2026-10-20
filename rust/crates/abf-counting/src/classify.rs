//! §4.1–4.2: original outcome versus derived eligibility. The contradiction
//! screen is conservative and partial; passing it authenticates nothing.

use abf_parser::{Diagnostic, Field, Observation, ParseResult, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Accepted,
    Qualified,
    Incomplete,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Check {
    C1,
    C2,
    C3,
    C4,
    C5,
    C6,
    C7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Eligibility {
    Rejected,
    Contradictory(Check),
    Incomplete,
    Eligible,
}

/// Domain B: read unchanged, never replaced by eligibility.
pub(crate) fn outcome(result: &ParseResult) -> Outcome {
    match result {
        ParseResult::Rejected(_) => Outcome::Rejected,
        ParseResult::Observation(o) => match o.status {
            Status::Accepted => Outcome::Accepted,
            Status::Qualified => Outcome::Qualified,
            Status::Incomplete => Outcome::Incomplete,
        },
    }
}

pub(crate) fn eligibility(result: &ParseResult) -> Eligibility {
    let ParseResult::Observation(o) = result else {
        return Eligibility::Rejected;
    };
    if let Some(check) = contradiction(o) {
        Eligibility::Contradictory(check)
    } else if o.status == Status::Incomplete {
        Eligibility::Incomplete
    } else {
        Eligibility::Eligible
    }
}

fn token_fields(o: &Observation) -> [&Field<u64>; 5] {
    [&o.input, &o.output, &o.total, &o.cache_read, &o.cache_write]
}

/// Field-state discriminants only: C6 never reads metadata values.
fn state<T>(field: &Field<T>) -> Field<()> {
    match field {
        Field::Absent => Field::Absent,
        Field::Null => Field::Null,
        Field::WrongType => Field::WrongType,
        Field::OutOfRange => Field::OutOfRange,
        Field::Unsupported => Field::Unsupported,
        Field::Valid(_) => Field::Valid(()),
    }
}

/// Container/child shape the G1B parser cannot emit.
fn impossible_shape(parent: &Field<()>, children: &[Field<()>]) -> bool {
    let unsupported = |c: &Field<()>| *c == Field::Unsupported;
    match parent {
        Field::Valid(()) => children.iter().any(unsupported),
        _ => !children.iter().all(unsupported),
    }
}

/// All checks are evaluated; returns the first failing check in order.
pub(crate) fn contradiction(o: &Observation) -> Option<Check> {
    let any_valid = token_fields(o).iter().any(|f| matches!(f, Field::Valid(_)));
    let bits = o.diagnostics.bits();
    let has = |d| o.diagnostics.contains(d);
    let fields = [
        state(&o.message_id),
        state(&o.session_id),
        state(&o.model_id),
        state(&o.provider_id),
        o.time.clone(),
        state(&o.created),
        o.tokens.clone(),
        o.cache.clone(),
        state(&o.input),
        state(&o.output),
        state(&o.total),
        state(&o.cache_read),
        state(&o.cache_write),
    ];
    let c6 = fields.iter().any(|f| match f {
        Field::Null => !has(Diagnostic::ExplicitNull),
        Field::WrongType | Field::OutOfRange => !has(Diagnostic::InvalidField),
        _ => false,
    });
    let c7 = impossible_shape(
        &o.tokens,
        &[
            state(&o.input),
            state(&o.output),
            state(&o.total),
            o.cache.clone(),
        ],
    ) || impossible_shape(&o.cache, &[state(&o.cache_read), state(&o.cache_write)])
        || impossible_shape(&o.time, &[state(&o.created)]);
    let checks = [
        (Check::C1, o.status == Status::Incomplete && any_valid),
        (
            Check::C2,
            matches!(o.status, Status::Accepted | Status::Qualified) && !any_valid,
        ),
        (Check::C3, o.status == Status::Accepted && bits != 0),
        (Check::C4, o.status == Status::Qualified && bits == 0),
        (
            Check::C5,
            has(Diagnostic::IncompleteTokens) && o.status != Status::Incomplete,
        ),
        (Check::C6, c6),
        (Check::C7, c7),
    ];
    checks.iter().find(|(_, failed)| *failed).map(|(c, _)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use abf_parser::{Limits, parse};

    fn parsed(frame: &str) -> Observation {
        match parse(frame.as_bytes(), None, Limits::MAX) {
            ParseResult::Observation(o) => o,
            ParseResult::Rejected(code) => panic!("fixture rejected: {code:?}"),
        }
    }

    #[test]
    fn each_constructed_contradiction_reports_its_first_failing_check() {
        let accepted = || parsed(r#"{"tokens":{"input":1}}"#);

        // C1: Incomplete with a Valid token (S14b shape).
        let mut c1 = parsed(r#"{"time":{"created":1700000000}}"#);
        c1.tokens = Field::Valid(());
        c1.input = Field::Valid(100);
        c1.output = Field::Absent;
        c1.total = Field::Absent;
        c1.cache = Field::Absent;
        assert_eq!(contradiction(&c1), Some(Check::C1));

        let mut c2 = accepted();
        c2.input = Field::Absent;
        assert_eq!(contradiction(&c2), Some(Check::C2));

        let mut c3 = parsed(r#"{"tokens":{"input":100,"output":null}}"#);
        assert_eq!(c3.status, Status::Qualified);
        c3.status = Status::Accepted;
        assert_eq!(contradiction(&c3), Some(Check::C3));

        let mut c4 = accepted();
        c4.status = Status::Qualified;
        assert_eq!(contradiction(&c4), Some(Check::C4));

        // C5: IncompleteTokens copied from a parser result, status not Incomplete.
        let mut c5 = c1.clone();
        c5.status = Status::Qualified;
        assert_eq!(contradiction(&c5), Some(Check::C5));

        let mut c6_null = accepted();
        c6_null.output = Field::Null;
        assert_eq!(contradiction(&c6_null), Some(Check::C6));
        let mut c6_wrong = accepted();
        c6_wrong.model_id = Field::WrongType;
        assert_eq!(contradiction(&c6_wrong), Some(Check::C6));
        let mut c6_range = accepted();
        c6_range.created = Field::OutOfRange;
        c6_range.time = Field::Valid(());
        assert_eq!(contradiction(&c6_range), Some(Check::C6));

        let mut c7_cache = accepted();
        c7_cache.cache_read = Field::Absent;
        assert_eq!(contradiction(&c7_cache), Some(Check::C7));
        let mut c7_tokens = accepted();
        c7_tokens.output = Field::Unsupported;
        assert_eq!(contradiction(&c7_tokens), Some(Check::C7));
        let mut c7_time = accepted();
        c7_time.created = Field::Valid(1);
        assert_eq!(contradiction(&c7_time), Some(Check::C7));

        for o in [
            c1, c2, c3, c4, c5, c6_null, c6_wrong, c6_range, c7_cache, c7_tokens, c7_time,
        ] {
            assert!(matches!(
                eligibility(&ParseResult::Observation(o)),
                Eligibility::Contradictory(_)
            ));
        }
    }

    #[test]
    fn parser_produced_fictional_observations_pass_the_screen() {
        for frame in [
            r#"{"tokens":{"input":0}}"#,
            r#"{"tokens":{"total":500}}"#,
            r#"{"tokens":{"input":100,"output":null}}"#,
            r#"{"tokens":{"input":1,"output":"x","cache":{"read":2,"write":-1}}}"#,
            r#"{"tokens":{"input":18446744073709551616,"total":3}}"#,
            r#"{"tokens":null}"#,
            r#"{"tokens":[1]}"#,
            r#"{"tokens":{"cache":null,"input":4}}"#,
            r#"{"tokens":{"reasoning":5}}"#,
            r#"{"time":{"created":1700000000}}"#,
            r#"{"time":"bad","tokens":{"input":2}}"#,
            r#"{"id":null,"sessionID":7,"tokens":{"input":2}}"#,
            r#"{}"#,
        ] {
            let o = parsed(frame);
            assert_eq!(contradiction(&o), None, "{frame}");
        }
    }

    #[test]
    fn a_passing_constructed_incomplete_is_not_authenticated() {
        // §4.2: C5 is one-directional; this constructed state passes the screen.
        let mut o = parsed(r#"{"tokens":{"input":1}}"#);
        o.input = Field::Absent;
        o.status = Status::Incomplete;
        assert_eq!(contradiction(&o), None);
        assert_eq!(
            eligibility(&ParseResult::Observation(o)),
            Eligibility::Incomplete
        );
    }
}
