//! §6.4 domain G: the only module that reads `set.relations()`. Each stored
//! assertion entry is examined once. Nothing is collapsed, closed or trusted.

use abf_identity_relations::{SyntheticSet, UnverifiedHint, Verification};
use abf_observation::{EnvironmentKind, OpaqueRef};

use crate::{AssertionPartition, Code, Exact, HintCounts, RelationDiagnostics, Selection, bump};

fn kind_of(set: &SyntheticSet, handle: &OpaqueRef) -> Option<EnvironmentKind> {
    set.occurrences()
        .iter()
        .find(|o| o.handle() == handle)
        .map(|o| o.envelope().context().environment_kind())
}

pub(crate) fn diagnose(
    set: &SyntheticSet,
    selection: Selection,
) -> Result<RelationDiagnostics, Code> {
    let mut set_global = 0u32;
    let mut excluded = 0u32;
    let mut selected = AssertionPartition::default();
    let mut by_hint = HintCounts::default();
    let mut windows_incident = 0u32;
    let mut wsl_incident = 0u32;
    let mut referenced: Vec<&OpaqueRef> = Vec::new();
    referenced
        .try_reserve_exact(set.occurrences().len())
        .map_err(|_| Code::AllocationFailure)?;

    for entry in set.relations() {
        bump(&mut set_global)?;
        // G2B guarantees both endpoints exist and offers no removal. A missing
        // endpoint is unreachable; it would be counted as excluded, never dropped.
        let kinds = (kind_of(set, entry.from()), kind_of(set, entry.to()));
        let (Some(a), Some(b)) = kinds else {
            bump(&mut excluded)?;
            continue;
        };
        if !selection.includes(a) || !selection.includes(b) {
            bump(&mut excluded)?;
            continue;
        }
        bump(&mut selected.total)?;
        use EnvironmentKind::{SelectedWsl, Windows};
        match (a, b) {
            (Windows, Windows) => {
                bump(&mut selected.windows_within)?;
                bump(&mut windows_incident)?;
            }
            (SelectedWsl, SelectedWsl) => {
                bump(&mut selected.selected_wsl_within)?;
                bump(&mut wsl_incident)?;
            }
            _ => {
                bump(&mut selected.cross_environment)?;
                bump(&mut windows_incident)?;
                bump(&mut wsl_incident)?;
            }
        }
        bump(match entry.hint() {
            UnverifiedHint::MatchingClaimedIdentifiers => &mut by_hint.matching_claimed_identifiers,
            UnverifiedHint::DifferentClaimedIdentifiers => {
                &mut by_hint.different_claimed_identifiers
            }
            UnverifiedHint::EqualReportedValues => &mut by_hint.equal_reported_values,
            UnverifiedHint::PossibleReplayOrCopy => &mut by_hint.possible_replay_or_copy,
            UnverifiedHint::Unclassified => &mut by_hint.unclassified,
        })?;
        // Local handles are unique within one set: each occurrence counted once.
        for handle in [entry.from(), entry.to()] {
            if !referenced.contains(&handle) {
                referenced.push(handle);
            }
        }
    }
    let referenced = u32::try_from(referenced.len()).map_err(|_| Code::CountOverflow)?;
    Ok(RelationDiagnostics {
        set_global_entries: Exact(set_global),
        selected,
        excluded_entries: Exact(excluded),
        selected_by_hint: by_hint,
        windows_incident_entries: Exact(windows_incident),
        selected_wsl_incident_entries: Exact(wsl_incident),
        referenced_occurrences: Exact(referenced),
        hints_present: selected.total != 0,
        cross_environment_hints_present: selected.cross_environment != 0,
        verification: Verification::Unverified,
    })
}
