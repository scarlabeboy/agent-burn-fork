//! §4.3: raw status claims, per-run-key resolution and scope coverage.
//! Groups caller-reported claims by value; proves nothing about any run.

use abf_observation::{DeclaredContext, OpaqueRef, SourceRunStatus};

use crate::{
    Code, CoverageState, InventoryBasis, Qualification, Qualifications, RunDeclaration, RunSummary,
    StatusCounts, TraversalBasis, bump,
};

pub(crate) fn same_key(a: &RunDeclaration, b: &RunDeclaration) -> bool {
    a.environment_kind == b.environment_kind
        && a.environment_ref == b.environment_ref
        && a.source_ref == b.source_ref
        && a.run_ref == b.run_ref
}

fn bit(status: SourceRunStatus) -> u8 {
    match status {
        SourceRunStatus::NotCollected => 1,
        SourceRunStatus::Complete => 2,
        SourceRunStatus::Partial => 4,
        SourceRunStatus::Unavailable => 8,
        SourceRunStatus::Failed => 16,
    }
}

fn slot(counts: &mut StatusCounts, status: SourceRunStatus) -> &mut u32 {
    match status {
        SourceRunStatus::NotCollected => &mut counts.not_collected,
        SourceRunStatus::Complete => &mut counts.complete,
        SourceRunStatus::Partial => &mut counts.partial,
        SourceRunStatus::Unavailable => &mut counts.unavailable,
        SourceRunStatus::Failed => &mut counts.failed,
    }
}

const STATUSES: [SourceRunStatus; 5] = [
    SourceRunStatus::NotCollected,
    SourceRunStatus::Complete,
    SourceRunStatus::Partial,
    SourceRunStatus::Unavailable,
    SourceRunStatus::Failed,
];

/// One represented run key within a single environment kind.
struct Key<'a> {
    environment_ref: &'a OpaqueRef,
    source_ref: &'a OpaqueRef,
    run_ref: &'a OpaqueRef,
    /// Set of distinct raw statuses; never an ordered or weighted choice.
    statuses: u8,
    declared: bool,
    recorded: bool,
}

/// Accumulates one scope's raw claims. The environment kind is fixed by the caller.
pub(crate) struct Runs<'a> {
    keys: Vec<Key<'a>>,
    raw_record: StatusCounts,
    raw_declared: StatusCounts,
}

impl<'a> Runs<'a> {
    /// `capacity` = selected occurrences + selected declarations (≤ 256).
    pub(crate) fn new(capacity: usize) -> Result<Self, Code> {
        let mut keys = Vec::new();
        keys.try_reserve_exact(capacity)
            .map_err(|_| Code::AllocationFailure)?;
        Ok(Self {
            keys,
            raw_record: StatusCounts::default(),
            raw_declared: StatusCounts::default(),
        })
    }

    fn key(
        &mut self,
        environment_ref: &'a OpaqueRef,
        source_ref: &'a OpaqueRef,
        run_ref: &'a OpaqueRef,
    ) -> Result<&mut Key<'a>, Code> {
        let found = self.keys.iter().position(|k| {
            k.environment_ref == environment_ref
                && k.source_ref == source_ref
                && k.run_ref == run_ref
        });
        let index = match found {
            Some(i) => i,
            None => {
                self.keys
                    .try_reserve(1)
                    .map_err(|_| Code::AllocationFailure)?;
                self.keys.push(Key {
                    environment_ref,
                    source_ref,
                    run_ref,
                    statuses: 0,
                    declared: false,
                    recorded: false,
                });
                self.keys.len() - 1
            }
        };
        self.keys.get_mut(index).ok_or(Code::AllocationFailure)
    }

    pub(crate) fn record(
        &mut self,
        context: &'a DeclaredContext,
        status: SourceRunStatus,
    ) -> Result<(), Code> {
        bump(slot(&mut self.raw_record, status))?;
        let key = self.key(
            context.environment_ref(),
            context.source_ref(),
            context.run_ref(),
        )?;
        key.statuses |= bit(status);
        key.recorded = true;
        Ok(())
    }

    pub(crate) fn declare(&mut self, d: &'a RunDeclaration) -> Result<(), Code> {
        bump(slot(&mut self.raw_declared, d.status))?;
        let key = self.key(&d.environment_ref, &d.source_ref, &d.run_ref)?;
        key.statuses |= bit(d.status);
        key.declared = true;
        Ok(())
    }

    pub(crate) fn finish(
        self,
        inventory: InventoryBasis,
        records_without_reported_traversal: u32,
    ) -> Result<RunSummary, Code> {
        let mut run_keys = 0;
        let mut conflicting_keys = 0;
        let mut undeclared_record_keys = 0;
        let mut resolved = StatusCounts::default();
        for key in &self.keys {
            bump(&mut run_keys)?;
            match STATUSES.iter().find(|s| bit(**s) == key.statuses) {
                Some(status) => bump(slot(&mut resolved, *status))?,
                // Two or more distinct statuses: no status is chosen.
                None => bump(&mut conflicting_keys)?,
            }
            if inventory == InventoryBasis::Declared && key.recorded && !key.declared {
                bump(&mut undeclared_record_keys)?;
            }
        }
        let raw = |status| {
            let mut r = self.raw_record;
            let mut d = self.raw_declared;
            *slot(&mut r, status) != 0 || *slot(&mut d, status) != 0
        };
        let mut q = Qualifications::empty();
        q.set_if(Qualification::PartialRun, raw(SourceRunStatus::Partial));
        q.set_if(Qualification::FailedRun, raw(SourceRunStatus::Failed));
        q.set_if(
            Qualification::NotCollectedRun,
            raw(SourceRunStatus::NotCollected),
        );
        q.set_if(
            Qualification::UnavailableRun,
            raw(SourceRunStatus::Unavailable),
        );
        q.set_if(
            Qualification::RecordWithoutReportedTraversal,
            records_without_reported_traversal != 0,
        );
        q.set_if(Qualification::RunStatusConflict, conflicting_keys != 0);
        q.set_if(
            Qualification::RunInventoryUndeclared,
            inventory == InventoryBasis::Undeclared,
        );
        q.set_if(
            Qualification::UndeclaredRunInRecords,
            undeclared_record_keys != 0,
        );
        // §4.3 table, first matching row wins.
        let coverage = if q.contains(Qualification::RunStatusConflict)
            || q.contains(Qualification::RecordWithoutReportedTraversal)
            || q.contains(Qualification::UndeclaredRunInRecords)
        {
            CoverageState::Inconsistent
        } else if run_keys == 0 {
            CoverageState::NoEvidence
        } else if resolved.complete == run_keys {
            CoverageState::ReportedComplete
        } else if resolved.complete == 0 && resolved.partial == 0 && resolved.failed == 0 {
            CoverageState::NoTraversalReported
        } else {
            CoverageState::ReportedIncomplete
        };
        Ok(RunSummary {
            inventory,
            traversal: TraversalBasis::CallerReported,
            coverage,
            run_keys,
            conflicting_keys,
            resolved,
            raw_record_claims: self.raw_record,
            raw_declared_claims: self.raw_declared,
            undeclared_record_keys,
            qualifications: q,
        })
    }
}
