//! §6.3: Computer is derived from the selected scopes of one set. It has no
//! run summary or coverage of its own and never removes an occurrence.

use abf_observation::EnvironmentKind;

use crate::{
    Code, ComponentEvidence, ComponentResult, ComponentValue, Components, ComputerResult,
    EnvironmentResult, Exact, FieldStateCounts, IdentityBasis, IndeterminateReason, OutcomeCounts,
    PerEnvironmentRuns, PhysicalEventCount, Qualification, RecordDiagnostics, Selection,
    TraversalBasis, UsageTotal, add,
};

fn field_states(w: &FieldStateCounts, l: &FieldStateCounts) -> Result<FieldStateCounts, Code> {
    Ok(FieldStateCounts {
        valid_zero: add(w.valid_zero, l.valid_zero)?,
        valid_positive: add(w.valid_positive, l.valid_positive)?,
        absent: add(w.absent, l.absent)?,
        null: add(w.null, l.null)?,
        wrong_type: add(w.wrong_type, l.wrong_type)?,
        out_of_range: add(w.out_of_range, l.out_of_range)?,
        unsupported: add(w.unsupported, l.unsupported)?,
    })
}

fn outcomes(w: &OutcomeCounts, l: &OutcomeCounts) -> Result<OutcomeCounts, Code> {
    Ok(OutcomeCounts {
        accepted: add(w.accepted, l.accepted)?,
        qualified: add(w.qualified, l.qualified)?,
        incomplete: add(w.incomplete, l.incomplete)?,
        rejected: add(w.rejected, l.rejected)?,
    })
}

fn records(w: &RecordDiagnostics, l: &RecordDiagnostics) -> Result<RecordDiagnostics, Code> {
    Ok(RecordDiagnostics {
        eligible: add(w.eligible, l.eligible)?,
        contradictory: add(w.contradictory, l.contradictory)?,
        unsupported_component_observations: add(
            w.unsupported_component_observations,
            l.unsupported_component_observations,
        )?,
        records_without_reported_traversal: add(
            w.records_without_reported_traversal,
            l.records_without_reported_traversal,
        )?,
    })
}

/// Symmetric value table for `Both`. Depends only on component values.
fn value(w: ComponentValue, l: ComponentValue) -> ComponentValue {
    use ComponentValue::*;
    match (w, l) {
        (Overflow, _) | (_, Overflow) => Overflow,
        (ReportedClaimSum(a), ReportedClaimSum(b)) => {
            a.checked_add(b).map_or(Overflow, ReportedClaimSum)
        }
        (ReportedClaimSum(a), _) | (_, ReportedClaimSum(a)) => ReportedClaimSum(a),
        (NoValidClaims, _) | (_, NoValidClaims) => NoValidClaims,
        (Indeterminate(_), Indeterminate(_)) => {
            Indeterminate(IndeterminateReason::AllEnvironmentsIndeterminate)
        }
    }
}

/// One component under `Both`. Evidence is summed exactly for every value row.
pub(crate) fn combine(w: &ComponentResult, l: &ComponentResult) -> Result<ComponentResult, Code> {
    let (we, le) = (&w.evidence, &l.evidence);
    let mut q = we.qualifications.union(le.qualifications);
    q.set_if(
        Qualification::CrossEnvironmentSum,
        we.contributing != 0 && le.contributing != 0,
    );
    q.set_if(
        Qualification::EnvironmentWithoutClaims,
        (w.value == ComponentValue::NoValidClaims && le.contributing != 0)
            || (l.value == ComponentValue::NoValidClaims && we.contributing != 0),
    );
    q.set_if(
        Qualification::EnvironmentIndeterminate,
        matches!(w.value, ComponentValue::Indeterminate(_))
            || matches!(l.value, ComponentValue::Indeterminate(_)),
    );
    Ok(ComponentResult {
        value: value(w.value, l.value),
        evidence: ComponentEvidence {
            eligible_records: add(we.eligible_records, le.eligible_records)?,
            contributing: add(we.contributing, le.contributing)?,
            zero_valued: add(we.zero_valued, le.zero_valued)?,
            field_states: field_states(&we.field_states, &le.field_states)?,
            qualifications: q,
            identity: IdentityBasis::Unresolved,
            traversal: TraversalBasis::CallerReported,
        },
    })
}

/// Single selection: the selected scope unchanged; no Computer-only flag.
pub(crate) fn single(selection: Selection, r: &EnvironmentResult) -> ComputerResult {
    let runs = Some(r.runs);
    ComputerResult {
        selection,
        stored_occurrences: r.stored_occurrences,
        outcomes: r.outcomes,
        records: r.records,
        per_environment: match r.kind {
            EnvironmentKind::Windows => PerEnvironmentRuns {
                windows: runs,
                selected_wsl: None,
            },
            EnvironmentKind::SelectedWsl => PerEnvironmentRuns {
                windows: None,
                selected_wsl: runs,
            },
        },
        components: r.components,
        physical_events: PhysicalEventCount::Unresolved,
        usage: UsageTotal::Unsupported,
    }
}

/// `Both`: checked sums of exact counts; per-component table above.
pub(crate) fn both(w: &EnvironmentResult, l: &EnvironmentResult) -> Result<ComputerResult, Code> {
    let (wc, lc) = (&w.components, &l.components);
    Ok(ComputerResult {
        selection: Selection::Both,
        stored_occurrences: Exact(add(w.stored_occurrences.0, l.stored_occurrences.0)?),
        outcomes: Exact(outcomes(&w.outcomes.0, &l.outcomes.0)?),
        records: Exact(records(&w.records.0, &l.records.0)?),
        per_environment: PerEnvironmentRuns {
            windows: Some(w.runs),
            selected_wsl: Some(l.runs),
        },
        components: Components {
            input: combine(&wc.input, &lc.input)?,
            output: combine(&wc.output, &lc.output)?,
            total: combine(&wc.total, &lc.total)?,
            cache_read: combine(&wc.cache_read, &lc.cache_read)?,
            cache_write: combine(&wc.cache_write, &lc.cache_write)?,
        },
        physical_events: PhysicalEventCount::Unresolved,
        usage: UsageTotal::Unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Qualifications;
    use ComponentValue::*;

    fn result(value: ComponentValue, contributing: u32) -> ComponentResult {
        ComponentResult {
            value,
            evidence: ComponentEvidence {
                eligible_records: contributing,
                contributing,
                zero_valued: 0,
                field_states: FieldStateCounts {
                    valid_positive: contributing,
                    ..FieldStateCounts::default()
                },
                qualifications: Qualifications::empty(),
                identity: IdentityBasis::Unresolved,
                traversal: TraversalBasis::CallerReported,
            },
        }
    }

    #[test]
    fn value_table_every_row_both_orders_with_summed_evidence() {
        let ind = Indeterminate(IndeterminateReason::NoRunEvidence);
        let all = Indeterminate(IndeterminateReason::AllEnvironmentsIndeterminate);
        let rows = [
            (
                result(ReportedClaimSum(3), 1),
                result(ReportedClaimSum(4), 2),
                ReportedClaimSum(7),
            ),
            (
                result(ReportedClaimSum(3), 1),
                result(NoValidClaims, 0),
                ReportedClaimSum(3),
            ),
            (
                result(ReportedClaimSum(3), 1),
                result(ind, 0),
                ReportedClaimSum(3),
            ),
            (
                result(NoValidClaims, 0),
                result(NoValidClaims, 0),
                NoValidClaims,
            ),
            (result(NoValidClaims, 0), result(ind, 0), NoValidClaims),
            (result(ind, 0), result(ind, 0), all),
            (
                result(Overflow, 2),
                result(ReportedClaimSum(1), 1),
                Overflow,
            ),
            (result(Overflow, 2), result(NoValidClaims, 0), Overflow),
            (result(Overflow, 2), result(ind, 0), Overflow),
            (result(Overflow, 2), result(Overflow, 1), Overflow),
            (
                result(ReportedClaimSum(u128::MAX), 1),
                result(ReportedClaimSum(1), 1),
                Overflow,
            ),
        ];
        for (w, l, expected) in rows {
            let a = combine(&w, &l).unwrap();
            let b = combine(&l, &w).unwrap();
            assert_eq!(a, b);
            assert_eq!(a.value, expected);
            let contributing = w.evidence.contributing + l.evidence.contributing;
            assert_eq!(a.evidence.contributing, contributing);
            assert_eq!(a.evidence.eligible_records, contributing);
            assert_eq!(a.evidence.field_states.valid_positive, contributing);
        }
    }

    #[test]
    fn computer_only_flags_follow_their_canonical_triggers() {
        let ind = Indeterminate(IndeterminateReason::NoTraversalReported);
        let q = |w: ComponentResult, l: ComponentResult| {
            combine(&w, &l).unwrap().evidence.qualifications
        };
        let both = q(
            result(ReportedClaimSum(1), 1),
            result(ReportedClaimSum(0), 1),
        );
        assert!(both.contains(Qualification::CrossEnvironmentSum));
        assert!(!both.contains(Qualification::EnvironmentWithoutClaims));
        let without = q(result(ReportedClaimSum(1), 1), result(NoValidClaims, 0));
        assert_eq!(
            without.bits(),
            Qualification::EnvironmentWithoutClaims as u32
        );
        let indet = q(result(NoValidClaims, 0), result(ind, 0));
        assert_eq!(indet.bits(), Qualification::EnvironmentIndeterminate as u32);
        assert!(q(result(NoValidClaims, 0), result(NoValidClaims, 0)).is_empty());
    }
}
