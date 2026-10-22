use crate::{
    AttemptRef, CatalogScope, DeliveryEvidence, FinalizationId, SourceRef, SourceRunStatus,
};
use abf_parser::ParseResult;

pub const MAX_FRAMES: usize = 4096;
pub const MAX_QUARANTINE: usize = MAX_FRAMES;
pub const MAX_DIAGNOSTIC_ENTRIES: usize = 64;
pub const MAX_RECORD_BYTES: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalizeCode {
    InvalidManifest,
    DuplicateSource,
    SourceScopeMismatch,
    EnumerationCountMismatch,
    ForeignFrame,
    CountOverflow,
    CapacityExceeded,
    InvalidSourceRef,
    InvalidBarrier,
    InvalidParserResult,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumFailure {
    PermissionDenied,
    Io,
    DeliveryConflict,
    AmbiguousInitial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Budget,
    Cancelled,
    Timeout,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    Unavailable,
    DistributionMismatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenFailure {
    PermissionDenied,
    Io,
    Locked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartialReason {
    DeliveryConflict,
    DeliveryLoss,
    Io,
    Budget,
    ReaderTerminated,
    Timeout,
    TransportLost,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptReason {
    ReaderTerminated,
    Timeout,
    TransportLost,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeObservation {
    NoChangeObserved,
    NotObservable,
    Grew,
    ShrankOrReplaced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessEnd {
    ExitedNormally,
    ExitedWithError,
    Crashed,
    KilledTimeout,
    KilledCancel,
    TransportLost,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetrySubset {
    AllExpected,
    OnlyNonCoveredSources,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Barrier {
    Drained,
    AbandonedWithoutReader,
    NotDrained,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SupervisorInstant(u64);
impl SupervisorInstant {
    pub fn synthetic(value: u64) -> Self {
        Self(value)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ReaderInstant(u64);
impl ReaderInstant {
    pub fn synthetic(value: u64) -> Self {
        Self(value)
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct TimeObservations {
    start: SupervisorInstant,
    barrier: SupervisorInstant,
}
impl TimeObservations {
    /// No clock is read, no ordering or skew is inferred.
    pub fn new(start: u64, barrier: u64) -> Self {
        Self {
            start: SupervisorInstant(start),
            barrier: SupervisorInstant(barrier),
        }
    }
    pub fn start(&self) -> SupervisorInstant {
        self.start
    }
    pub fn barrier(&self) -> SupervisorInstant {
        self.barrier
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawEnumeration {
    NotAttempted,
    ScopeUnavailable(UnavailableReason),
    RootUnresolved,
    RootAbsent,
    LayoutUnrecognized,
    Failed(EnumFailure),
    Partial(u64, StopReason),
    Closed(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumerationOutcome {
    NotAttempted,
    ScopeUnavailable(UnavailableReason),
    RootUnresolved,
    RootAbsent,
    LayoutUnrecognized,
    Failed(EnumFailure),
    Partial { listed: u32, reason: StopReason },
    Closed { listed: u32 },
}
impl EnumerationOutcome {
    pub(crate) fn has_set(self) -> bool {
        matches!(self, Self::Partial { .. } | Self::Closed { .. })
    }
}
#[derive(Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Inline bounded parser result; no per-frame box allocation.
pub enum SourceEvent {
    Record {
        bytes: Vec<u8>,
        result: ParseResult,
    },
    OpenAttempt,
    OpenSuccess,
    Skipped(StopReason),
    OpenFailed(OpenFailure),
    Vanished,
    End {
        records: u64,
        change: ChangeObservation,
    },
    ReadError(PartialReason),
}
#[derive(Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Bounded frame representation, matching the parser carrier.
pub enum Payload {
    Enumeration {
        member: usize,
        second: bool,
        outcome: RawEnumeration,
        sources: Vec<String>,
        observed_at: Option<ReaderInstant>,
    },
    Source {
        source: String,
        source_seq: u64,
        event: SourceEvent,
    },
    RunEnd,
}
impl Payload {
    pub(crate) fn source(&self) -> Option<&str> {
        match self {
            Self::Source { source, .. } => Some(source),
            _ => None,
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct ReaderFrame {
    pub attempt: AttemptRef,
    pub frame_seq: u64,
    pub payload: Payload,
}
#[derive(Clone, PartialEq, Eq)]
pub struct ExecutionManifest {
    pub attempt: AttemptRef,
    pub finalization: FinalizationId,
    pub members: Vec<CatalogScope>,
    pub retry_of: Option<AttemptRef>,
    pub subset: RetrySubset,
}
#[derive(Clone)]
pub struct RawExecutionLog {
    pub manifest: ExecutionManifest,
    pub pre_barrier: Vec<ReaderFrame>,
    pub process: ProcessEnd,
    pub barrier: Barrier,
    pub distribution_matches: bool,
    pub times: TimeObservations,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuarantineCode {
    ExactReplay,
    SequenceConflict,
    UnexpectedSource,
    DistributionMismatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Admitted,
    Quarantined(QuarantineCode),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeliveryIndex(pub(crate) u32);
impl DeliveryIndex {
    pub fn value(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoTraversalClaim {
    Skipped,
    OpenFailed,
    Vanished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceAnomaly {
    ConflictingTerminals,
    SourceSeqReused,
    RecordsAfterEnd,
    CountExceedsEnd,
    MissingEndMarker,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOutcome {
    NotAttempted(StopReason),
    OpenFailed(OpenFailure),
    Vanished,
    PartialRead {
        records: u32,
        reason: PartialReason,
    },
    TraversedToObservedEnd {
        records: u32,
        change: ChangeObservation,
    },
    Unaccounted,
    TerminalContradiction {
        claim: NoTraversalClaim,
        records: u32,
    },
    AnomalousSequence {
        kind: SequenceAnomaly,
        records: u32,
    },
    OpenInterrupted {
        reason: InterruptReason,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKind {
    Skipped(StopReason),
    OpenFailed(OpenFailure),
    Vanished,
    End {
        records: u32,
        change: ChangeObservation,
    },
    ReadError(PartialReason),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalClaimView {
    pub kind: TerminalKind,
    pub frame_seq: u32,
    pub source_seq: u32,
}
#[derive(Clone, PartialEq, Eq)]
pub struct SourceReconciliation {
    pub(crate) outcome: SourceOutcome,
    pub(crate) status: SourceRunStatus,
    pub(crate) claims: Vec<TerminalClaimView>,
    pub(crate) matched: Vec<u8>,
    pub(crate) conflict: bool,
    pub(crate) progress: bool,
    pub(crate) missing: u32,
}
impl SourceReconciliation {
    pub fn outcome(&self) -> SourceOutcome {
        self.outcome
    }
    pub fn status(&self) -> SourceRunStatus {
        self.status
    }
    pub fn claims(&self) -> &[TerminalClaimView] {
        &self.claims
    }
    pub fn matched(&self) -> &[u8] {
        &self.matched
    }
    pub fn conflict_marked(&self) -> bool {
        self.conflict
    }
    pub fn read_progress(&self) -> bool {
        self.progress
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeliveryIntegrity {
    pub replays_quarantined: u32,
    pub seq_conflicts: u32,
    pub global_seq_conflicts: u32,
    pub global_replays: u32,
    pub attributable_gaps: u32,
    pub unattributable_gaps: u32,
    pub unexpected_source: u32,
    pub initial_enum_identities: u32,
    pub second_enum_identities: u32,
    pub second_enum_conflict: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondUnusable {
    NoInitialSet,
    DeliveryConflict,
    Ambiguous,
    NotListed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondListing {
    Closed { listed: u32 },
    Partial { listed: u32 },
}
#[derive(Clone, PartialEq, Eq)]
pub enum SecondEnumeration {
    NotPerformed,
    Performed {
        listing: SecondListing,
        new_sources: Vec<SourceRef>,
    },
    Unusable(SecondUnusable),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountBasis {
    Exact,
    LowerBound,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qualifier {
    TransportReplayQuarantined,
    ChangeNotObservable,
    SourceRewritten,
    NewSourcesObserved { count: u32, basis: CountBasis },
    SecondEnumerationNoNewSources,
    SecondEnumerationIncomplete,
    SecondEnumerationUnusable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoSourcesReason {
    RootAbsent,
    EmptyEnumeration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageVerdict {
    NotSelected,
    NoAttempt,
    ScopeUnavailable(UnavailableReason),
    Inconsistent,
    InventoryUnknown,
    NoSourcesFound(NoSourcesReason),
    InventoryIncomplete,
    NotCovered,
    PartiallyCovered,
    CompletionUnconfirmed,
    CoveredWithinEnumeration { expected: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerdictRule {
    ExecutionGlobalAnomaly,
    MemberEnumerationAnomaly,
    NoAttempt,
    ScopeUnavailable,
    LocalAnomaly,
    InventoryUnknown,
    RootAbsent,
    PartialInventory,
    EmptyInventory,
    TraversalComplete,
    NoReadProgress,
    SomeReadProgress,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttestationBasis {
    Unattested,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RetainedRecord {
    pub(crate) source: SourceRef,
    pub(crate) source_seq: u32,
    pub(crate) frame_seq: u32,
    pub(crate) delivery: DeliveryIndex,
    pub(crate) result: ParseResult,
}
impl RetainedRecord {
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn source_seq(&self) -> u32 {
        self.source_seq
    }
    pub fn frame_seq(&self) -> u32 {
        self.frame_seq
    }
    pub fn delivery(&self) -> DeliveryIndex {
        self.delivery
    }
    /// Explicit access for future adaptation. Never print the parser result.
    pub fn parse_result(&self) -> &ParseResult {
        &self.result
    }
}
/// Immutable finalized evidence, including its bound catalog authority.
/// ```compile_fail,E0616
/// use abf_source_run::{FinalizedMember, CatalogSnapshot};
/// fn rebind(member: &mut FinalizedMember, catalog: CatalogSnapshot) {
///     member.catalog = catalog;
/// }
/// ```
#[derive(Clone)]
pub struct FinalizedMember {
    pub(crate) catalog: crate::CatalogSnapshot,
    pub(crate) manifest: ExecutionManifest,
    pub(crate) scope: CatalogScope,
    pub(crate) enumeration: EnumerationOutcome,
    pub(crate) sources: Vec<(SourceRef, SourceReconciliation)>,
    pub(crate) records: Vec<RetainedRecord>,
    pub(crate) delivery: DeliveryIntegrity,
    pub(crate) second: SecondEnumeration,
    pub(crate) verdict: CoverageVerdict,
    pub(crate) cause: VerdictRule,
    pub(crate) run_end: bool,
    pub(crate) attempt: AttemptRef,
    pub(crate) finalization: FinalizationId,
    pub(crate) process: ProcessEnd,
    pub(crate) retry_of: Option<AttemptRef>,
    pub(crate) subset: RetrySubset,
}
impl FinalizedMember {
    pub fn scope(&self) -> &CatalogScope {
        &self.scope
    }
    pub fn enumeration(&self) -> EnumerationOutcome {
        self.enumeration
    }
    pub fn sources(&self) -> &[(SourceRef, SourceReconciliation)] {
        &self.sources
    }
    pub fn records(&self) -> &[RetainedRecord] {
        &self.records
    }
    pub fn delivery(&self) -> DeliveryIntegrity {
        self.delivery
    }
    pub fn second_enumeration(&self) -> &SecondEnumeration {
        &self.second
    }
    pub fn verdict(&self) -> CoverageVerdict {
        self.verdict
    }
    pub fn cause(&self) -> VerdictRule {
        self.cause
    }
    pub fn run_end_frame(&self) -> bool {
        self.run_end
    }
    pub fn process(&self) -> ProcessEnd {
        self.process
    }
    pub fn lineage(&self) -> (Option<&AttemptRef>, RetrySubset) {
        (self.retry_of.as_ref(), self.subset)
    }
    pub fn identity(&self) -> (&AttemptRef, &FinalizationId) {
        (&self.attempt, &self.finalization)
    }
    pub fn attestation(&self) -> AttestationBasis {
        AttestationBasis::Unattested
    }
    pub fn qualifiers(&self) -> Vec<Qualifier> {
        let mut q = Vec::new();
        if self.delivery.replays_quarantined > 0 || self.delivery.global_replays > 0 {
            q.push(Qualifier::TransportReplayQuarantined);
        }
        if self.sources.iter().any(|s| {
            matches!(
                s.1.outcome,
                SourceOutcome::TraversedToObservedEnd {
                    change: ChangeObservation::NotObservable,
                    ..
                }
            )
        }) {
            q.push(Qualifier::ChangeNotObservable);
        }
        if self.sources.iter().any(|s| {
            matches!(
                s.1.outcome,
                SourceOutcome::TraversedToObservedEnd {
                    change: ChangeObservation::ShrankOrReplaced,
                    ..
                }
            )
        }) {
            q.push(Qualifier::SourceRewritten);
        }
        match &self.second {
            SecondEnumeration::NotPerformed => {}
            SecondEnumeration::Unusable(_) => q.push(Qualifier::SecondEnumerationUnusable),
            SecondEnumeration::Performed {
                listing,
                new_sources,
            } => {
                let closed = matches!(listing, SecondListing::Closed { .. });
                q.push(if !new_sources.is_empty() {
                    Qualifier::NewSourcesObserved {
                        count: new_sources.len() as u32,
                        basis: if closed {
                            CountBasis::Exact
                        } else {
                            CountBasis::LowerBound
                        },
                    }
                } else if closed {
                    Qualifier::SecondEnumerationNoNewSources
                } else {
                    Qualifier::SecondEnumerationIncomplete
                });
            }
        }
        q
    }
}
pub struct FinalizedExecution {
    pub(crate) unbound_unexpected_source_deliveries: u32,
    pub(crate) manifest: ExecutionManifest,
    pub(crate) members: Vec<FinalizedMember>,
    pub(crate) dispositions: Vec<Disposition>,
    pub(crate) identities: Vec<(u32, crate::evidence::IdentityPayload)>,
    pub(crate) deliveries: Vec<DeliveryEvidence>,
    pub(crate) times: TimeObservations,
}
impl FinalizedExecution {
    /// Pre-barrier unbound UnexpectedSource delivery instances (immutable layer 1).
    pub fn unbound_unexpected_source_deliveries(&self) -> u32 {
        self.unbound_unexpected_source_deliveries
    }
    pub fn members(&self) -> &[FinalizedMember] {
        &self.members
    }
    pub fn dispositions(&self) -> &[Disposition] {
        &self.dispositions
    }
    pub fn manifest(&self) -> &ExecutionManifest {
        &self.manifest
    }
    pub fn times(&self) -> &TimeObservations {
        &self.times
    }
    /// Fixed claim evidence includes quarantined variants, never their parser payloads.
    pub fn delivery_evidence(&self) -> &[DeliveryEvidence] {
        &self.deliveries
    }
}
redacted!(
    SupervisorInstant,
    ReaderInstant,
    TimeObservations,
    SourceEvent,
    Payload,
    ReaderFrame,
    ExecutionManifest,
    RawExecutionLog,
    DeliveryIndex,
    SourceReconciliation,
    SecondEnumeration,
    RetainedRecord,
    FinalizedMember,
    FinalizedExecution
);
