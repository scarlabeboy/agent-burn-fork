use crate::*;
use std::collections::BTreeSet;

pub(crate) fn bump(v: &mut u32) -> Result<(), FinalizeCode> {
    *v = v.checked_add(1).ok_or(FinalizeCode::CountOverflow)?;
    Ok(())
}
pub(crate) fn narrow(v: u64) -> Result<u32, FinalizeCode> {
    u32::try_from(v).map_err(|_| FinalizeCode::CountOverflow)
}
pub(crate) fn capacity(len: usize, limit: usize) -> Result<(), FinalizeCode> {
    if len > limit {
        Err(FinalizeCode::CapacityExceeded)
    } else {
        Ok(())
    }
}
pub(crate) fn interrupt(p: ProcessEnd) -> InterruptReason {
    match p {
        ProcessEnd::KilledTimeout => InterruptReason::Timeout,
        ProcessEnd::KilledCancel => InterruptReason::Cancelled,
        ProcessEnd::TransportLost => InterruptReason::TransportLost,
        _ => InterruptReason::ReaderTerminated,
    }
}
fn partial(i: InterruptReason) -> PartialReason {
    match i {
        InterruptReason::ReaderTerminated => PartialReason::ReaderTerminated,
        InterruptReason::Timeout => PartialReason::Timeout,
        InterruptReason::TransportLost => PartialReason::TransportLost,
        InterruptReason::Cancelled => PartialReason::Cancelled,
    }
}
fn no_traversal(k: TerminalKind) -> bool {
    matches!(
        k,
        TerminalKind::Skipped(_) | TerminalKind::OpenFailed(_) | TerminalKind::Vanished
    )
}

pub(crate) struct Inputs {
    pub claims: Vec<TerminalClaimView>,
    pub q: BTreeSet<u32>,
    pub records: u32,
    pub act: bool,
    pub opening: bool,
    pub conflict: bool,
    pub reuse: bool,
    pub run_end: bool,
    pub process: ProcessEnd,
}

pub(crate) fn reconcile(i: Inputs) -> SourceReconciliation {
    let n: Vec<_> = i.claims.iter().filter(|c| no_traversal(c.kind)).collect();
    let t: Vec<_> = i.claims.iter().filter(|c| !no_traversal(c.kind)).collect();
    let has_n = !n.is_empty();
    let empty_c = i.claims.is_empty();
    let end = match t.as_slice() {
        [c] => match c.kind {
            TerminalKind::End { records, change } => Some((records, change, c.source_seq)),
            _ => None,
        },
        _ => None,
    };
    // Count distinct values in 1..k without iterating a fictional billion-element range.
    let missing = end.map_or(0, |(k, _, _)| {
        k - i.q.iter().filter(|&&v| v >= 1 && v <= k).count() as u32
    });
    let flags = [
        has_n && i.claims.len() >= 2,
        has_n && i.claims.len() == 1 && i.act,
        has_n && i.claims.len() == 1 && !i.act,
        !has_n && i.conflict,
        !has_n && t.len() >= 2,
        !has_n && i.reuse,
        !has_n && end.is_some_and(|(_, _, seq)| i.q.iter().any(|&v| v > seq)),
        !has_n && end.is_some_and(|(k, _, _)| i.q.len() < (k as usize) || missing > 0),
        !has_n && end.is_some_and(|(k, _, _)| i.q.len() > k as usize),
        !has_n && end.is_some_and(|(k, _, _)| missing == 0 && i.q.len() == k as usize),
        !has_n && matches!(t.as_slice(),[c] if matches!(c.kind,TerminalKind::ReadError(_))),
        empty_c && (i.act || i.opening) && i.run_end,
        empty_c && i.act && !i.run_end,
        empty_c && i.opening && !i.act && !i.run_end,
        empty_c && !i.act && !i.opening,
    ];
    let matched: Vec<u8> = flags
        .iter()
        .enumerate()
        .filter(|(_, v)| **v)
        .map(|(n, _)| n as u8 + 1)
        .collect();
    let first = matched[0];
    let anomaly = |kind| SourceOutcome::AnomalousSequence {
        kind,
        records: i.records,
    };
    let no_status = if n
        .iter()
        .any(|c| matches!(c.kind, TerminalKind::OpenFailed(_) | TerminalKind::Vanished))
    {
        SourceRunStatus::Unavailable
    } else {
        SourceRunStatus::NotCollected
    };
    let interrupted_status = if i.process == ProcessEnd::KilledCancel {
        SourceRunStatus::Partial
    } else {
        SourceRunStatus::Failed
    };
    let outcome = match first {
        1 | 5 => anomaly(SequenceAnomaly::ConflictingTerminals),
        2 => SourceOutcome::TerminalContradiction {
            claim: match n[0].kind {
                TerminalKind::Skipped(_) => NoTraversalClaim::Skipped,
                TerminalKind::OpenFailed(_) => NoTraversalClaim::OpenFailed,
                _ => NoTraversalClaim::Vanished,
            },
            records: i.records,
        },
        3 => match n[0].kind {
            TerminalKind::Skipped(r) => SourceOutcome::NotAttempted(r),
            TerminalKind::OpenFailed(r) => SourceOutcome::OpenFailed(r),
            _ => SourceOutcome::Vanished,
        },
        4 => SourceOutcome::PartialRead {
            records: i.records,
            reason: PartialReason::DeliveryConflict,
        },
        6 => anomaly(SequenceAnomaly::SourceSeqReused),
        7 => anomaly(SequenceAnomaly::RecordsAfterEnd),
        8 => SourceOutcome::PartialRead {
            records: i.records,
            reason: PartialReason::DeliveryLoss,
        },
        9 => anomaly(SequenceAnomaly::CountExceedsEnd),
        10 => {
            let (k, change, _) = end.expect("S10");
            SourceOutcome::TraversedToObservedEnd { records: k, change }
        }
        11 => {
            let TerminalKind::ReadError(reason) = t[0].kind else {
                unreachable!()
            };
            SourceOutcome::PartialRead {
                records: i.records,
                reason,
            }
        }
        12 => anomaly(SequenceAnomaly::MissingEndMarker),
        13 => SourceOutcome::PartialRead {
            records: i.records,
            reason: partial(interrupt(i.process)),
        },
        14 => SourceOutcome::OpenInterrupted {
            reason: interrupt(i.process),
        },
        15 => SourceOutcome::Unaccounted,
        _ => unreachable!(),
    };
    let status = match first {
        1..=3 => no_status,
        10 => match end.expect("S10").1 {
            ChangeObservation::NoChangeObserved | ChangeObservation::NotObservable => {
                SourceRunStatus::Complete
            }
            _ => SourceRunStatus::Partial,
        },
        13 | 14 => interrupted_status,
        15 => SourceRunStatus::NotCollected,
        _ => SourceRunStatus::Partial,
    };
    SourceReconciliation {
        outcome,
        status,
        claims: i.claims,
        matched,
        conflict: i.conflict,
        progress: i.act,
        missing,
    }
}

pub(crate) fn verdict(
    m: &FinalizedMember,
    unbound_unexpected_source_deliveries: u32,
) -> (CoverageVerdict, VerdictRule) {
    use CoverageVerdict as V;
    use VerdictRule as R;
    let d = m.delivery;
    if unbound_unexpected_source_deliveries > 0
        || d.global_seq_conflicts > 0
        || d.unattributable_gaps > 0
    {
        return (V::Inconsistent, R::ExecutionGlobalAnomaly);
    }
    if d.initial_enum_identities > 1 || d.second_enum_identities > 1 {
        return (V::Inconsistent, R::MemberEnumerationAnomaly);
    }
    if m.enumeration == EnumerationOutcome::NotAttempted
        && m.sources.is_empty()
        && d.unexpected_source == 0
    {
        return (V::NoAttempt, R::NoAttempt);
    }
    if let EnumerationOutcome::ScopeUnavailable(r) = m.enumeration {
        return (V::ScopeUnavailable(r), R::ScopeUnavailable);
    }
    if d.unexpected_source > 0
        || d.seq_conflicts > 0
        || m.sources.iter().any(|s| {
            matches!(
                s.1.outcome,
                SourceOutcome::TerminalContradiction { .. }
                    | SourceOutcome::AnomalousSequence { .. }
                    | SourceOutcome::PartialRead {
                        reason: PartialReason::DeliveryConflict,
                        ..
                    }
            )
        })
    {
        return (V::Inconsistent, R::LocalAnomaly);
    }
    match m.enumeration {
        EnumerationOutcome::RootUnresolved
        | EnumerationOutcome::LayoutUnrecognized
        | EnumerationOutcome::Failed(_) => (V::InventoryUnknown, R::InventoryUnknown),
        EnumerationOutcome::RootAbsent => (
            V::NoSourcesFound(NoSourcesReason::RootAbsent),
            R::RootAbsent,
        ),
        EnumerationOutcome::Partial { .. } => (V::InventoryIncomplete, R::PartialInventory),
        EnumerationOutcome::Closed { listed: 0 } => (
            V::NoSourcesFound(NoSourcesReason::EmptyEnumeration),
            R::EmptyInventory,
        ),
        _ => {
            if m.sources.iter().all(|s| {
                matches!(
                    s.1.outcome,
                    SourceOutcome::TraversedToObservedEnd {
                        change: ChangeObservation::NoChangeObserved
                            | ChangeObservation::NotObservable,
                        ..
                    }
                )
            }) {
                (
                    if m.run_end && m.process == ProcessEnd::ExitedNormally {
                        V::CoveredWithinEnumeration {
                            expected: m.sources.len() as u32,
                        }
                    } else {
                        V::CompletionUnconfirmed
                    },
                    R::TraversalComplete,
                )
            } else if m.sources.iter().all(|s| !s.1.progress) {
                (V::NotCovered, R::NoReadProgress)
            } else {
                (V::PartiallyCovered, R::SomeReadProgress)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn n28_bounded_stage_two_domain_is_total_and_complete_only_from_s10() {
        let kinds = [
            TerminalKind::Skipped(StopReason::Budget),
            TerminalKind::OpenFailed(OpenFailure::PermissionDenied),
            TerminalKind::Vanished,
            TerminalKind::End {
                records: 0,
                change: ChangeObservation::NoChangeObserved,
            },
            TerminalKind::End {
                records: 1,
                change: ChangeObservation::NoChangeObserved,
            },
            TerminalKind::End {
                records: 2,
                change: ChangeObservation::NoChangeObserved,
            },
            TerminalKind::ReadError(PartialReason::Io),
        ];
        let mut sets = vec![Vec::new()];
        for &a in &kinds {
            sets.push(vec![a]);
            for &b in &kinds {
                sets.push(vec![a, b]);
            }
        }
        let mut checked = 0u32;
        for set in sets {
            for mask in 0..8 {
                for flags in 0..32 {
                    for process in [
                        ProcessEnd::ExitedNormally,
                        ProcessEnd::Crashed,
                        ProcessEnd::KilledCancel,
                    ] {
                        let claims = set
                            .iter()
                            .enumerate()
                            .map(|(i, &kind)| TerminalClaimView {
                                kind,
                                frame_seq: i as u32 + 1,
                                source_seq: 3,
                            })
                            .collect();
                        let q: BTreeSet<u32> =
                            (1..=3).filter(|v| mask & (1 << (v - 1)) != 0).collect();
                        let result = reconcile(Inputs {
                            claims,
                            q: q.clone(),
                            records: q.len() as u32,
                            act: flags & 1 != 0,
                            opening: flags & 2 != 0,
                            conflict: flags & 4 != 0,
                            reuse: flags & 8 != 0,
                            run_end: flags & 16 != 0,
                            process,
                        });
                        assert!(!result.matched.is_empty());
                        assert!((1..=15).contains(&result.matched[0]));
                        assert_eq!(
                            result.status == SourceRunStatus::Complete,
                            result.matched[0] == 10
                        );
                        assert_eq!(result.claims.len(), set.len());
                        let has_no_traversal = set.iter().any(|k| {
                            matches!(
                                k,
                                TerminalKind::Skipped(_)
                                    | TerminalKind::OpenFailed(_)
                                    | TerminalKind::Vanished
                            )
                        });
                        if has_no_traversal {
                            assert!(result.matched[0] <= 3);
                            assert_eq!(
                                result.status,
                                if set.iter().any(|k| matches!(
                                    k,
                                    TerminalKind::OpenFailed(_) | TerminalKind::Vanished
                                )) {
                                    SourceRunStatus::Unavailable
                                } else {
                                    SourceRunStatus::NotCollected
                                }
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 43776);
    }
}
