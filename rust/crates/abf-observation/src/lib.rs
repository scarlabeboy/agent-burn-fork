//! Experimental synthetic-only observation boundary; no collection or normalization.
#![forbid(unsafe_code)]

mod opencode;
mod provenance;

pub use opencode::{AdapterCode, ObservationEnvelope, from_opencode};
pub use provenance::{ContextCode, DeclaredContext, EnvironmentKind, Harness, OpaqueRef, ParserId};

/// Caller-reported traversal, independent of every record's parser outcome.
/// None of these variants certifies complete usage or field reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRunStatus {
    /// No reader attempt is reported.
    NotCollected,
    /// Traversal is reported complete, including any rejected records.
    Complete,
    /// Some traversal is reported; records remain qualified by this status.
    Partial,
    /// Source access is reported unavailable; no traversal is reported.
    Unavailable,
    /// Reader failure is reported; earlier records may still exist.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inconsistency {
    /// Any record result (even rejection) conflicts with NotCollected/Unavailable.
    RecordWithoutReportedTraversal,
}
