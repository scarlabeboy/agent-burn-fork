//! §3 and §8.3: one environment scope. Five components are accumulated
//! independently from eligible records only; no cross-component arithmetic.

use abf_identity_relations::SyntheticSet;
use abf_observation::{EnvironmentKind, OpaqueRef};
use abf_parser::{Diagnostic, Field, ParseResult};

use crate::EnvironmentResult;
use crate::classify::{self, Eligibility, Outcome};
use crate::runs::Runs;
use crate::{
    Code, ComponentEvidence, ComponentResult, ComponentValue, Components, CoverageState, Exact,
    FieldStateCounts, IdentityBasis, IndeterminateReason, InventoryBasis, OutcomeCounts,
    PhysicalEventCount, Qualification, Qualifications, RecordDiagnostics, RefMultiplicity,
    RunInventory, TraversalBasis, UsageTotal, bump,
};

/// Exact evidence plus a checked `u128` claim accumulator for one component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Accumulator {
    eligible_records: u32,
    contributing: u32,
    zero_valued: u32,
    field_states: FieldStateCounts,
    local: Qualifications,
    /// `None` once a checked addition has failed; never reset, clamped or wrapped.
    sum: Option<u128>,
}

impl Accumulator {
    pub(crate) fn new() -> Self {
        Self {
            eligible_records: 0,
            contributing: 0,
            zero_valued: 0,
            field_states: FieldStateCounts::default(),
            local: Qualifications::empty(),
            sum: Some(0),
        }
    }

    /// Test-only injection point for the unreachable overflow branch (T6b).
    #[cfg(test)]
    pub(crate) fn seeded(sum: u128) -> Self {
        Self {
            sum: Some(sum),
            ..Self::new()
        }
    }

    /// One eligible record's field for this component.
    pub(crate) fn observe(&mut self, field: &Field<u64>, qualified: bool) -> Result<(), Code> {
        bump(&mut self.eligible_records)?;
        let bins = &mut self.field_states;
        let bin = match field {
            Field::Valid(n) => {
                bump(&mut self.contributing)?;
                self.local
                    .set_if(Qualification::FromQualifiedRecords, qualified);
                self.sum = self.sum.and_then(|s| s.checked_add(u128::from(*n)));
                if *n == 0 {
                    bump(&mut self.zero_valued)?;
                    &mut bins.valid_zero
                } else {
                    &mut bins.valid_positive
                }
            }
            other => {
                self.local.insert(Qualification::MissingClaims);
                match other {
                    Field::Absent => &mut bins.absent,
                    Field::Null => &mut bins.null,
                    Field::WrongType => &mut bins.wrong_type,
                    Field::OutOfRange => &mut bins.out_of_range,
                    _ => &mut bins.unsupported,
                }
            }
        };
        bump(bin)
    }

    /// §8.3 value selection, first match. Evidence is retained for every variant.
    pub(crate) fn finish(
        self,
        shared: Qualifications,
        stored_occurrences: u32,
        coverage: CoverageState,
    ) -> ComponentResult {
        let value = match self.sum {
            None => ComponentValue::Overflow,
            Some(sum) if self.contributing != 0 => ComponentValue::ReportedClaimSum(sum),
            Some(_) if stored_occurrences != 0 => ComponentValue::NoValidClaims,
            Some(_) => match coverage {
                CoverageState::ReportedComplete
                | CoverageState::ReportedIncomplete
                | CoverageState::Inconsistent => ComponentValue::NoValidClaims,
                CoverageState::NoEvidence => {
                    ComponentValue::Indeterminate(IndeterminateReason::NoRunEvidence)
                }
                CoverageState::NoTraversalReported => {
                    ComponentValue::Indeterminate(IndeterminateReason::NoTraversalReported)
                }
            },
        };
        ComponentResult {
            value,
            evidence: ComponentEvidence {
                eligible_records: self.eligible_records,
                contributing: self.contributing,
                zero_valued: self.zero_valued,
                field_states: self.field_states,
                qualifications: self.local.union(shared),
                identity: IdentityBasis::Unresolved,
                traversal: TraversalBasis::CallerReported,
            },
        }
    }
}

fn refs_multiplicity(refs: &[&OpaqueRef]) -> Result<RefMultiplicity, Code> {
    let mut distinct = 0u32;
    for (i, r) in refs.iter().enumerate() {
        if !refs[..i].contains(r) {
            bump(&mut distinct)?;
        }
    }
    Ok(match distinct {
        0 => RefMultiplicity::None,
        1 => RefMultiplicity::One,
        n => RefMultiplicity::Multiple(n),
    })
}

pub(crate) fn compute(
    set: &SyntheticSet,
    inventory: RunInventory<'_>,
    kind: EnvironmentKind,
) -> Result<EnvironmentResult, Code> {
    let occurrences = set
        .occurrences()
        .iter()
        .map(|o| o.envelope())
        .filter(|e| e.context().environment_kind() == kind);
    let (basis, declarations): (_, &[_]) = match inventory {
        RunInventory::Undeclared => (InventoryBasis::Undeclared, &[]),
        RunInventory::Declared(d) => (InventoryBasis::Declared, d),
    };
    let declarations = declarations.iter().filter(|d| d.environment_kind == kind);
    let capacity = occurrences
        .clone()
        .count()
        .checked_add(declarations.clone().count())
        .ok_or(Code::CountOverflow)?;

    let mut refs = Vec::new();
    refs.try_reserve_exact(capacity)
        .map_err(|_| Code::AllocationFailure)?;
    let mut runs = Runs::new(capacity)?;
    let mut stored = 0u32;
    let mut outcomes = OutcomeCounts::default();
    let mut records = RecordDiagnostics::default();
    let mut acc = [Accumulator::new(); 5];

    for envelope in occurrences {
        bump(&mut stored)?;
        let result = envelope.parser_result();
        let outcome = classify::outcome(result);
        bump(match outcome {
            Outcome::Accepted => &mut outcomes.accepted,
            Outcome::Qualified => &mut outcomes.qualified,
            Outcome::Incomplete => &mut outcomes.incomplete,
            Outcome::Rejected => &mut outcomes.rejected,
        })?;
        if let ParseResult::Observation(o) = result {
            if o.diagnostics.contains(Diagnostic::UnsupportedComponent) {
                bump(&mut records.unsupported_component_observations)?;
            }
            match classify::eligibility(result) {
                Eligibility::Eligible => {
                    bump(&mut records.eligible)?;
                    let qualified = outcome == Outcome::Qualified;
                    let fields = [&o.input, &o.output, &o.total, &o.cache_read, &o.cache_write];
                    for (a, field) in acc.iter_mut().zip(fields) {
                        a.observe(field, qualified)?;
                    }
                }
                Eligibility::Contradictory(_) => bump(&mut records.contradictory)?,
                Eligibility::Incomplete | Eligibility::Rejected => {}
            }
        }
        if envelope.inconsistency().is_some() {
            bump(&mut records.records_without_reported_traversal)?;
        }
        refs.push(envelope.context().environment_ref());
        runs.record(envelope.context(), envelope.source_status())?;
    }
    for d in declarations {
        refs.push(&d.environment_ref);
        runs.declare(d)?;
    }

    let environment_refs = refs_multiplicity(&refs)?;
    let runs = runs.finish(basis, records.records_without_reported_traversal)?;
    let mut shared = runs.qualifications;
    shared.set_if(
        Qualification::UnknownComponentsDiscarded,
        records.unsupported_component_observations != 0,
    );
    shared.set_if(
        Qualification::IncompleteRecordsPresent,
        outcomes.incomplete != 0,
    );
    shared.set_if(
        Qualification::RejectedRecordsPresent,
        outcomes.rejected != 0,
    );
    shared.set_if(
        Qualification::ContradictoryRecordsExcluded,
        records.contradictory != 0,
    );
    shared.set_if(
        Qualification::EnvironmentRefAmbiguous,
        matches!(environment_refs, RefMultiplicity::Multiple(_)),
    );
    Ok(EnvironmentResult {
        kind,
        environment_refs,
        stored_occurrences: Exact(stored),
        outcomes: Exact(outcomes),
        records: Exact(records),
        runs,
        components: Components::from_array(acc.map(|a| a.finish(shared, stored, runs.coverage))),
        physical_events: PhysicalEventCount::Unresolved,
        usage: UsageTotal::Unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_overflow_yields_overflow_with_unchanged_exact_evidence() {
        let fields = [Field::Valid(u64::MAX), Field::Valid(0), Field::Absent];
        let mut plain = Accumulator::new();
        let mut injected = Accumulator::seeded(u128::MAX - 1);
        for f in &fields {
            plain.observe(f, false).unwrap();
            injected.observe(f, false).unwrap();
        }
        let shared = Qualifications::empty();
        let plain = plain.finish(shared, 3, CoverageState::ReportedComplete);
        let injected = injected.finish(shared, 3, CoverageState::ReportedComplete);
        assert_eq!(
            plain.value,
            ComponentValue::ReportedClaimSum(u128::from(u64::MAX))
        );
        assert_eq!(injected.value, ComponentValue::Overflow);
        assert_eq!(injected.evidence, plain.evidence);
        assert_eq!(injected.evidence.contributing, 2);
        assert_eq!(injected.evidence.zero_valued, 1);
        assert_eq!(injected.evidence.field_states.valid_positive, 1);
        assert_eq!(injected.evidence.field_states.valid_zero, 1);
        assert_eq!(injected.evidence.field_states.absent, 1);
        assert!(
            injected
                .evidence
                .qualifications
                .contains(Qualification::MissingClaims)
        );
    }

    #[test]
    fn overflow_is_sticky_and_never_reset_by_later_claims() {
        let mut a = Accumulator::seeded(u128::MAX);
        a.observe(&Field::Valid(1), false).unwrap();
        a.observe(&Field::Valid(0), false).unwrap();
        let r = a.finish(Qualifications::empty(), 2, CoverageState::NoEvidence);
        assert_eq!(r.value, ComponentValue::Overflow);
        assert_eq!(r.evidence.contributing, 2);
    }

    #[test]
    fn exact_boundary_sum_is_not_overflow() {
        let mut a = Accumulator::seeded(u128::MAX - u128::from(u64::MAX));
        a.observe(&Field::Valid(u64::MAX), false).unwrap();
        let r = a.finish(Qualifications::empty(), 1, CoverageState::ReportedComplete);
        assert_eq!(r.value, ComponentValue::ReportedClaimSum(u128::MAX));
    }
}
