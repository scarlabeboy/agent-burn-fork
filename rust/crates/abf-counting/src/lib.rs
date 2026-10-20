//! Experimental synthetic-only counting and projection boundary (G3B, contract G3A V2.2).
//! Pure computation over one caller-owned `SyntheticSet`; no I/O, collection,
//! deduplication, pricing or persistence. Reported claim sums are never usage.
#![forbid(unsafe_code)]

mod classify;
mod computer;
mod relations;
mod runs;
mod scope;

use abf_identity_relations::{SyntheticSet, Verification};
use abf_observation::{EnvironmentKind, OpaqueRef, SourceRunStatus};

/// R2 ceiling on caller-reported run declarations per inventory.
pub const MAX_RUN_DECLARATIONS: usize = 128;

/// Fixed codes for invalid inputs or resources only. Inconsistent evidence is
/// always represented inside `Ok(Projection)`. No input text is attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    DuplicateRunDeclaration,
    RunInventoryCapacity,
    CountOverflow,
    AllocationFailure,
}

/// Caller-reported synthetic run inventory (R1). `Declared` means only that the
/// caller supplied a list; it is never proof that the list is exhaustive.
#[derive(Clone, Copy, Debug)]
pub enum RunInventory<'a> {
    Undeclared,
    Declared(&'a [RunDeclaration]),
}

/// One caller-reported run claim, unverified like `DeclaredContext`.
/// The run key is (`environment_kind`, `environment_ref`, `source_ref`, `run_ref`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunDeclaration {
    pub environment_kind: EnvironmentKind,
    pub environment_ref: OpaqueRef,
    pub source_ref: OpaqueRef,
    pub run_ref: OpaqueRef,
    pub status: SourceRunStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    WindowsOnly,
    SelectedWslOnly,
    Both,
}

impl Selection {
    fn includes(self, kind: EnvironmentKind) -> bool {
        matches!(
            (self, kind),
            (Self::Both, _)
                | (Self::WindowsOnly, EnvironmentKind::Windows)
                | (Self::SelectedWslOnly, EnvironmentKind::SelectedWsl)
        )
    }
}

/// Structural storage fact; never a usage or physical-event claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exact<T>(pub T);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection {
    pub windows: ScopeResult,
    pub selected_wsl: ScopeResult,
    pub computer: ComputerResult,
    /// Domain G only: the sole carrier of relation-hint information.
    pub relations: RelationDiagnostics,
    pub excluded: Exact<ExcludedCounts>,
}

/// Evidence excluded by selection; counted so nothing disappears silently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExcludedCounts {
    /// Relation-independent.
    pub occurrences: u32,
    /// Relation-independent.
    pub declarations: u32,
    /// Stored assertion entries only; the one relation-dependent excluded count.
    pub relation_entries: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Fixed-size result; no extra allocation.
pub enum ScopeResult {
    NotSelected,
    Selected(EnvironmentResult),
}

impl ScopeResult {
    pub fn selected(&self) -> Option<&EnvironmentResult> {
        match self {
            Self::Selected(r) => Some(r),
            Self::NotSelected => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentResult {
    pub kind: EnvironmentKind,
    pub environment_refs: RefMultiplicity,
    /// Domain A: stored occurrences, not events.
    pub stored_occurrences: Exact<u32>,
    /// Domain B: original parser outcomes, unchanged.
    pub outcomes: Exact<OutcomeCounts>,
    /// Domain B': overlapping record diagnostics, not outcomes.
    pub records: Exact<RecordDiagnostics>,
    /// Domain F: caller-reported run evidence.
    pub runs: RunSummary,
    /// Domains C and C'.
    pub components: Components<ComponentResult>,
    /// Domain D: never computable.
    pub physical_events: PhysicalEventCount,
    /// Domain D': never computable.
    pub usage: UsageTotal,
}

/// Derived from the selected scopes of one set. Never a source, run or environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputerResult {
    pub selection: Selection,
    pub stored_occurrences: Exact<u32>,
    pub outcomes: Exact<OutcomeCounts>,
    pub records: Exact<RecordDiagnostics>,
    /// Computer has no run summary or coverage of its own.
    pub per_environment: PerEnvironmentRuns,
    pub components: Components<ComponentResult>,
    pub physical_events: PhysicalEventCount,
    pub usage: UsageTotal,
}

/// Five independent components. No cross-component arithmetic is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Components<T> {
    pub input: T,
    pub output: T,
    pub total: T,
    pub cache_read: T,
    pub cache_write: T,
}

impl<T> Components<T> {
    fn from_array([input, output, total, cache_read, cache_write]: [T; 5]) -> Self {
        Self {
            input,
            output,
            total,
            cache_read,
            cache_write,
        }
    }
}

/// `None` = environment not selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerEnvironmentRuns {
    pub windows: Option<RunSummary>,
    pub selected_wsl: Option<RunSummary>,
}

/// Distinct declared `environment_ref` values; equality is not environment identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefMultiplicity {
    None,
    One,
    Multiple(u32),
}

/// Original parser outcomes only; the four bins sum to stored occurrences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutcomeCounts {
    pub accepted: u32,
    pub qualified: u32,
    pub incomplete: u32,
    pub rejected: u32,
}

/// Overlapping diagnostics, not a fifth outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordDiagnostics {
    /// Eligibility class `Eligible` (§4.1).
    pub eligible: u32,
    /// Failed the partial C1–C7 screen; passing it authenticates nothing.
    pub contradictory: u32,
    pub unsupported_component_observations: u32,
    pub records_without_reported_traversal: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentResult {
    pub value: ComponentValue,
    /// Present and exact for every value variant.
    pub evidence: ComponentEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentValue {
    /// Exact sum of eligible Valid claims of one component. Not usage, not deduplicated.
    ReportedClaimSum(u128),
    /// No eligible Valid claim. Not zero usage.
    NoValidClaims,
    /// No basis for a numeric statement. Not emptiness.
    Indeterminate(IndeterminateReason),
    /// Sum not representable; evidence remains exact.
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndeterminateReason {
    NoRunEvidence,
    NoTraversalReported,
    AllEnvironmentsIndeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentEvidence {
    pub eligible_records: u32,
    pub contributing: u32,
    pub zero_valued: u32,
    pub field_states: FieldStateCounts,
    pub qualifications: Qualifications,
    pub identity: IdentityBasis,
    pub traversal: TraversalBasis,
}

/// Field states of eligible records only. A zero bin is a storage fact, never a zero claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FieldStateCounts {
    pub valid_zero: u32,
    pub valid_positive: u32,
    pub absent: u32,
    pub null: u32,
    pub wrong_type: u32,
    pub out_of_range: u32,
    pub unsupported: u32,
}

/// Single variant: no verified identity state exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityBasis {
    Unresolved,
}

/// Single variant: no attested traversal state exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraversalBasis {
    CallerReported,
}

/// Single variant: a physical-event count is not computable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalEventCount {
    Unresolved,
}

/// Single variant: physical-event usage is not computable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageTotal {
    Unsupported,
}

/// Caller-reported run evidence for one scope. `ReportedComplete` covers
/// represented run keys only and never certifies an exhaustive scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub inventory: InventoryBasis,
    pub traversal: TraversalBasis,
    pub coverage: CoverageState,
    /// Represented keys: records ∪ declarations.
    pub run_keys: u32,
    /// In no resolved bin.
    pub conflicting_keys: u32,
    pub resolved: StatusCounts,
    /// One per selected envelope.
    pub raw_record_claims: StatusCounts,
    /// One per selected declaration.
    pub raw_declared_claims: StatusCounts,
    /// Only under `Declared`.
    pub undeclared_record_keys: u32,
    /// Group R flags only.
    pub qualifications: Qualifications,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryBasis {
    Undeclared,
    /// A list was supplied; not exhaustive.
    Declared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageState {
    NoEvidence,
    NoTraversalReported,
    ReportedComplete,
    ReportedIncomplete,
    Inconsistent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub not_collected: u32,
    pub complete: u32,
    pub partial: u32,
    pub failed: u32,
    pub unavailable: u32,
}

/// Storage counts of unverified hints. Confers no identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationDiagnostics {
    pub set_global_entries: Exact<u32>,
    pub selected: AssertionPartition,
    /// Equals `Projection.excluded.relation_entries`.
    pub excluded_entries: Exact<u32>,
    pub selected_by_hint: HintCounts,
    /// Non-additive: never an assertion count.
    pub windows_incident_entries: Exact<u32>,
    /// Non-additive: never an assertion count.
    pub selected_wsl_incident_entries: Exact<u32>,
    pub referenced_occurrences: Exact<u32>,
    pub hints_present: bool,
    pub cross_environment_hints_present: bool,
    pub verification: Verification,
}

/// Disjoint bins of selected stored assertion entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssertionPartition {
    pub windows_within: u32,
    pub selected_wsl_within: u32,
    pub cross_environment: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HintCounts {
    pub matching_claimed_identifiers: u32,
    pub different_claimed_identifiers: u32,
    pub equal_reported_values: u32,
    pub possible_replay_or_copy: u32,
    pub unclassified: u32,
}

/// One qualification flag with exactly one canonical trigger (§8.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Qualification {
    // Group L: component-local.
    MissingClaims = 1 << 0,
    FromQualifiedRecords = 1 << 1,
    // Group S: scope-shared.
    UnknownComponentsDiscarded = 1 << 2,
    IncompleteRecordsPresent = 1 << 3,
    RejectedRecordsPresent = 1 << 4,
    ContradictoryRecordsExcluded = 1 << 5,
    EnvironmentRefAmbiguous = 1 << 6,
    // Group R: run-related scope flags.
    PartialRun = 1 << 7,
    FailedRun = 1 << 8,
    NotCollectedRun = 1 << 9,
    UnavailableRun = 1 << 10,
    RecordWithoutReportedTraversal = 1 << 11,
    RunStatusConflict = 1 << 12,
    RunInventoryUndeclared = 1 << 13,
    UndeclaredRunInRecords = 1 << 14,
    // Group K: Computer-only.
    CrossEnvironmentSum = 1 << 15,
    EnvironmentWithoutClaims = 1 << 16,
    EnvironmentIndeterminate = 1 << 17,
}

/// Fixed bitset. Union only; no flag is ever cleared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Qualifications(u32);

impl Qualifications {
    pub const fn empty() -> Self {
        Self(0)
    }
    pub fn contains(self, flag: Qualification) -> bool {
        self.0 & flag as u32 != 0
    }
    pub fn bits(self) -> u32 {
        self.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub(crate) fn insert(&mut self, flag: Qualification) {
        self.0 |= flag as u32;
    }
    pub(crate) fn set_if(&mut self, flag: Qualification, condition: bool) {
        if condition {
            self.insert(flag);
        }
    }
    pub(crate) fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Checked `u32` increment; overflow is an error, never a wrap or partial result.
pub(crate) fn bump(counter: &mut u32) -> Result<(), Code> {
    *counter = counter.checked_add(1).ok_or(Code::CountOverflow)?;
    Ok(())
}

pub(crate) fn add(a: u32, b: u32) -> Result<u32, Code> {
    a.checked_add(b).ok_or(Code::CountOverflow)
}

fn validate(runs: RunInventory<'_>) -> Result<(), Code> {
    let RunInventory::Declared(declarations) = runs else {
        return Ok(());
    };
    // Capacity first bounds the quadratic duplicate scan below.
    if declarations.len() > MAX_RUN_DECLARATIONS {
        return Err(Code::RunInventoryCapacity);
    }
    for (i, d) in declarations.iter().enumerate() {
        if declarations[..i].iter().any(|e| runs::same_key(d, e)) {
            return Err(Code::DuplicateRunDeclaration);
        }
    }
    Ok(())
}

/// Project one synthetic set. Every occurrence is kept; nothing is deduplicated.
/// Relation assertions affect only `relations` and `excluded.relation_entries`.
pub fn project(
    set: &SyntheticSet,
    runs: RunInventory<'_>,
    selection: Selection,
) -> Result<Projection, Code> {
    validate(runs)?;
    let mut excluded = ExcludedCounts::default();
    for o in set.occurrences() {
        if !selection.includes(o.envelope().context().environment_kind()) {
            bump(&mut excluded.occurrences)?;
        }
    }
    if let RunInventory::Declared(declarations) = runs {
        for d in declarations {
            if !selection.includes(d.environment_kind) {
                bump(&mut excluded.declarations)?;
            }
        }
    }
    let scope = |kind| scope::compute(set, runs, kind);
    let (windows, selected_wsl, computer) = match selection {
        Selection::WindowsOnly => {
            let w = scope(EnvironmentKind::Windows)?;
            let c = computer::single(selection, &w);
            (ScopeResult::Selected(w), ScopeResult::NotSelected, c)
        }
        Selection::SelectedWslOnly => {
            let l = scope(EnvironmentKind::SelectedWsl)?;
            let c = computer::single(selection, &l);
            (ScopeResult::NotSelected, ScopeResult::Selected(l), c)
        }
        Selection::Both => {
            let w = scope(EnvironmentKind::Windows)?;
            let l = scope(EnvironmentKind::SelectedWsl)?;
            let c = computer::both(&w, &l)?;
            (ScopeResult::Selected(w), ScopeResult::Selected(l), c)
        }
    };
    let relations = relations::diagnose(set, selection)?;
    excluded.relation_entries = relations.excluded_entries.0;
    Ok(Projection {
        windows,
        selected_wsl,
        computer,
        relations,
        excluded: Exact(excluded),
    })
}
