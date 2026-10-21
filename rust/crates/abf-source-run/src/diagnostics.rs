use crate::reconcile::{bump, narrow};
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticsCode {
    ForeignFrame,
    CountOverflow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssessCode {
    IdentityMismatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Record,
    OpenAttempt,
    OpenSuccess,
    Skipped,
    OpenFailed,
    Vanished,
    End,
    ReadError,
    InitialEnumeration,
    SecondEnumeration,
    RunEnd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeClass {
    Accepted,
    Qualified,
    Incomplete,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityRelation {
    ReplayOfAdmittedIdentity,
    ConflictsWithAdmittedIdentity,
    NewIdentity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribution {
    Member(u32),
    Unattributed,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticEntry {
    pub kind: FrameKind,
    pub frame_seq: u32,
    pub attribution: Attribution,
    pub relation: IdentityRelation,
    pub outcome: Option<OutcomeClass>,
    pub arrival: SupervisorInstant,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticCounts {
    pub records: u32,
    pub other: u32,
    pub conflicts: u32,
    pub unattributed: u32,
}
impl DiagnosticCounts {
    fn append(&mut self, entry: DiagnosticEntry) -> bool {
        let mut next = *self;
        let result = (|| {
            if entry.kind == FrameKind::Record {
                bump(&mut next.records)?;
            } else {
                bump(&mut next.other)?;
            }
            if entry.relation == IdentityRelation::ConflictsWithAdmittedIdentity {
                bump(&mut next.conflicts)?;
            }
            if entry.attribution == Attribution::Unattributed {
                bump(&mut next.unattributed)?;
            }
            Ok::<_, FinalizeCode>(())
        })();
        if result.is_err() {
            return false;
        }
        *self = next;
        true
    }
}

pub struct PostFinalizationLog<'a> {
    execution: &'a FinalizedExecution,
    entries: Vec<DiagnosticEntry>,
    total: DiagnosticCounts,
    per_member: Vec<DiagnosticCounts>,
    unattributed: DiagnosticCounts,
    capacity_reached: bool,
    count_overflow: bool,
}
impl<'a> PostFinalizationLog<'a> {
    /// Supervisor-owned, in-memory diagnostics; binding grants no evidence authority.
    pub fn bind(execution: &'a FinalizedExecution) -> Self {
        Self {
            execution,
            entries: Vec::new(),
            total: DiagnosticCounts::default(),
            per_member: vec![DiagnosticCounts::default(); execution.members.len()],
            unattributed: DiagnosticCounts::default(),
            capacity_reached: false,
            count_overflow: false,
        }
    }
    /// Consumes and drops the late payload; stores fixed facts only. Foreign attempts
    /// and unrepresentable identities are rejected without changing the log.
    pub fn append(
        &mut self,
        frame: ReaderFrame,
        arrival: SupervisorInstant,
    ) -> Result<(), DiagnosticsCode> {
        if frame.attempt != self.execution.manifest.attempt {
            return Err(DiagnosticsCode::ForeignFrame);
        }
        let frame_seq = narrow(frame.frame_seq).map_err(|_| DiagnosticsCode::CountOverflow)?;
        let admitted = self
            .execution
            .identities
            .iter()
            .find(|(seq, _)| *seq == frame_seq)
            .map(|(_, p)| p);
        let parse_equal = match &frame.payload {
            Payload::Source {
                event: SourceEvent::Record { result, .. },
                ..
            } => self
                .execution
                .members
                .iter()
                .flat_map(|m| &m.records)
                .find(|r| r.frame_seq == frame_seq)
                .is_some_and(|r| &r.result == result),
            _ => true,
        };
        let relation = match admitted {
            None => IdentityRelation::NewIdentity,
            Some(p) if p.matches(&frame.payload) && parse_equal => {
                IdentityRelation::ReplayOfAdmittedIdentity
            }
            Some(_) => IdentityRelation::ConflictsWithAdmittedIdentity,
        };
        let attribution = frame
            .payload
            .source()
            .and_then(|s| {
                self.execution
                    .members
                    .iter()
                    .position(|m| m.sources.iter().any(|e| e.0.as_str() == s))
            })
            .map_or(Attribution::Unattributed, |n| Attribution::Member(n as u32));
        let (kind, outcome) = classify(&frame.payload);
        let entry = DiagnosticEntry {
            kind,
            outcome,
            frame_seq,
            relation,
            attribution,
            arrival,
        };
        if !self.count_overflow {
            let mut total = self.total;
            let destination = match attribution {
                Attribution::Member(n) => &mut self.per_member[n as usize],
                Attribution::Unattributed => &mut self.unattributed,
            };
            let mut local = *destination;
            if total.append(entry) && local.append(entry) {
                self.total = total;
                *destination = local;
            } else {
                self.count_overflow = true;
            }
        }
        if self.entries.len() < MAX_DIAGNOSTIC_ENTRIES {
            self.entries.push(entry);
        } else {
            self.capacity_reached = true;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
            attempt: self.execution.manifest.attempt.clone(),
            finalization: self.execution.manifest.finalization.clone(),
            scopes: self
                .execution
                .members
                .iter()
                .map(|m| m.scope.clone())
                .collect(),
            entries: self.entries.clone(),
            total: self.total,
            per_member: self.per_member.clone(),
            unattributed: self.unattributed,
            capacity_reached: self.capacity_reached,
            count_overflow: self.count_overflow,
        }
    }
}
fn classify(p: &Payload) -> (FrameKind, Option<OutcomeClass>) {
    (
        match p {
            Payload::RunEnd => FrameKind::RunEnd,
            Payload::Enumeration { second: true, .. } => FrameKind::SecondEnumeration,
            Payload::Enumeration { second: false, .. } => FrameKind::InitialEnumeration,
            Payload::Source { event, .. } => match event {
                SourceEvent::Record { result, .. } => {
                    return (
                        FrameKind::Record,
                        Some(match result {
                            abf_parser::ParseResult::Rejected(_) => OutcomeClass::Rejected,
                            abf_parser::ParseResult::Observation(o) => match o.status {
                                abf_parser::Status::Accepted => OutcomeClass::Accepted,
                                abf_parser::Status::Qualified => OutcomeClass::Qualified,
                                abf_parser::Status::Incomplete => OutcomeClass::Incomplete,
                            },
                        }),
                    );
                }
                SourceEvent::OpenAttempt => FrameKind::OpenAttempt,
                SourceEvent::OpenSuccess => FrameKind::OpenSuccess,
                SourceEvent::Skipped(_) => FrameKind::Skipped,
                SourceEvent::OpenFailed(_) => FrameKind::OpenFailed,
                SourceEvent::Vanished => FrameKind::Vanished,
                SourceEvent::End { .. } => FrameKind::End,
                SourceEvent::ReadError(_) => FrameKind::ReadError,
            },
        },
        None,
    )
}
#[derive(Clone)]
pub struct DiagnosticsSnapshot {
    attempt: AttemptRef,
    finalization: FinalizationId,
    scopes: Vec<CatalogScope>,
    entries: Vec<DiagnosticEntry>,
    total: DiagnosticCounts,
    per_member: Vec<DiagnosticCounts>,
    unattributed: DiagnosticCounts,
    capacity_reached: bool,
    count_overflow: bool,
}
impl DiagnosticsSnapshot {
    pub fn entries(&self) -> &[DiagnosticEntry] {
        &self.entries
    }
    /// Exact while count_overflow is false; otherwise explicitly a lower bound.
    pub fn counts(&self) -> DiagnosticCounts {
        self.total
    }
    pub fn capacity_reached(&self) -> bool {
        self.capacity_reached
    }
    pub fn count_overflow(&self) -> bool {
        self.count_overflow
    }
    pub fn count_basis(&self) -> CountBasis {
        if self.count_overflow {
            CountBasis::LowerBound
        } else {
            CountBasis::Exact
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationCeiling {
    Original,
    WeakenedByLateDelivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostFinalization {
    NoneObserved,
    Observed {
        records: u32,
        other: u32,
        conflicts: u32,
        unattributed: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentAssessment {
    pub verdict: CoverageVerdict,
    pub post: PostFinalization,
    pub ceiling: PresentationCeiling,
    pub count_basis: CountBasis,
}
/// Attribution limits a member's assessment to its own late source facts plus
/// unattributed deliveries. Never recomputes the original verdict or source set.
pub fn assess(
    member: &FinalizedMember,
    diagnostics: &DiagnosticsSnapshot,
) -> Result<CurrentAssessment, AssessCode> {
    if member.attempt != diagnostics.attempt || member.finalization != diagnostics.finalization {
        return Err(AssessCode::IdentityMismatch);
    }
    let index = diagnostics
        .scopes
        .iter()
        .position(|s| s == &member.scope)
        .ok_or(AssessCode::IdentityMismatch)?;
    let a = diagnostics.per_member[index];
    let b = diagnostics.unattributed;
    // Both are disjoint subcounts of the representable total, or marked lower bounds.
    let c = DiagnosticCounts {
        records: a
            .records
            .checked_add(b.records)
            .ok_or(AssessCode::IdentityMismatch)?,
        other: a
            .other
            .checked_add(b.other)
            .ok_or(AssessCode::IdentityMismatch)?,
        conflicts: a
            .conflicts
            .checked_add(b.conflicts)
            .ok_or(AssessCode::IdentityMismatch)?,
        unattributed: b.unattributed,
    };
    let observed = c.records > 0 || c.other > 0 || diagnostics.count_overflow;
    Ok(CurrentAssessment {
        verdict: member.verdict,
        post: if observed {
            PostFinalization::Observed {
                records: c.records,
                other: c.other,
                conflicts: c.conflicts,
                unattributed: c.unattributed,
            }
        } else {
            PostFinalization::NoneObserved
        },
        ceiling: if observed {
            PresentationCeiling::WeakenedByLateDelivery
        } else {
            PresentationCeiling::Original
        },
        count_basis: diagnostics.count_basis(),
    })
}
redacted!(DiagnosticEntry, DiagnosticsSnapshot);
impl std::fmt::Debug for PostFinalizationLog<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostFinalizationLog([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn execution() -> FinalizedExecution {
        let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v").unwrap();
        let mut b = CatalogBuilder::new();
        b.add(scope.clone(), "f1v1").unwrap();
        let catalog = b.build();
        finalize(
            RawExecutionLog {
                manifest: ExecutionManifest {
                    attempt: AttemptRef::new("a1").unwrap(),
                    finalization: FinalizationId::new("z1").unwrap(),
                    members: vec![scope],
                    retry_of: None,
                    subset: RetrySubset::AllExpected,
                },
                pre_barrier: Vec::new(),
                process: ProcessEnd::Crashed,
                barrier: Barrier::Drained,
                distribution_matches: true,
                times: TimeObservations::new(0, 0),
            },
            &catalog,
        )
        .unwrap()
    }
    #[test]
    fn r101f_maximum_is_exact_then_actual_overflow_is_terminal_and_marked_lower_bound() {
        let x = execution();
        let mut log = PostFinalizationLog::bind(&x);
        log.total.other = 4294967294;
        log.unattributed.other = 4294967294;
        let late = || ReaderFrame {
            attempt: AttemptRef::new("a1").unwrap(),
            frame_seq: 4294967295,
            payload: Payload::RunEnd,
        };
        log.append(late(), SupervisorInstant::synthetic(0)).unwrap();
        let first = log.snapshot();
        assert_eq!(first.counts().other, 4294967295);
        assert!(!first.count_overflow());
        assert_eq!(first.count_basis(), CountBasis::Exact);
        assert!(!first.capacity_reached());
        log.append(late(), SupervisorInstant::synthetic(0)).unwrap();
        let second = log.snapshot();
        assert_eq!(second.counts().other, 4294967295);
        assert!(second.count_overflow());
        assert_eq!(second.count_basis(), CountBasis::LowerBound);
        assert!(!second.capacity_reached());
        assert_eq!(
            assess(&x.members()[0], &second).unwrap().count_basis,
            CountBasis::LowerBound
        );
        log.append(late(), SupervisorInstant::synthetic(0)).unwrap();
        assert!(log.snapshot().count_overflow());
        assert_eq!(log.snapshot().counts(), second.counts());
        assert!(!first.count_overflow());
    }
    #[test]
    fn r101f_capacity_minus_one_at_and_plus_one_are_distinct_from_overflow() {
        let x = execution();
        let mut log = PostFinalizationLog::bind(&x);
        for n in 0..MAX_DIAGNOSTIC_ENTRIES - 1 {
            log.append(
                ReaderFrame {
                    attempt: AttemptRef::new("a1").unwrap(),
                    frame_seq: n as u64,
                    payload: Payload::RunEnd,
                },
                SupervisorInstant::synthetic(0),
            )
            .unwrap();
        }
        assert_eq!(log.snapshot().entries().len(), 63);
        assert!(!log.snapshot().capacity_reached());
        log.append(
            ReaderFrame {
                attempt: AttemptRef::new("a1").unwrap(),
                frame_seq: 64,
                payload: Payload::RunEnd,
            },
            SupervisorInstant::synthetic(0),
        )
        .unwrap();
        assert_eq!(log.snapshot().entries().len(), 64);
        assert!(!log.snapshot().capacity_reached());
        log.append(
            ReaderFrame {
                attempt: AttemptRef::new("a1").unwrap(),
                frame_seq: 65,
                payload: Payload::RunEnd,
            },
            SupervisorInstant::synthetic(0),
        )
        .unwrap();
        assert_eq!(log.snapshot().entries().len(), 64);
        assert!(log.snapshot().capacity_reached());
        assert!(!log.snapshot().count_overflow());
        assert_eq!(log.snapshot().counts().other, 65);
    }
}
