//! Synthetic occurrences and caller assertions only; no physical-event identity policy.
#![forbid(unsafe_code)]

mod relations;
pub use relations::{RelationAssertion, UnverifiedHint, Verification};

use abf_observation::{ObservationEnvelope, OpaqueRef};

/// Fixed codes. No source text or opaque claims are attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    InvalidLimits,
    DuplicateHandle,
    OccurrenceCapacity,
    RelationCapacity,
    SelfLink,
    MissingEndpoint,
    AllocationFailure,
}

/// Research ceilings, independent of parser/observation payload ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub occurrences: usize,
    pub relations: usize,
}

impl Limits {
    pub const MAX: Self = Self {
        occurrences: 128,
        relations: 256,
    };
}

/// One local occurrence, including rejected records. Neither handle nor value
/// equality establishes a physical event, persistent key or source identity.
/// No Clone implementation: ownership moves without implicit payload copies.
#[derive(Debug)]
pub struct Occurrence {
    handle: OpaqueRef,
    envelope: ObservationEnvelope,
}

impl Occurrence {
    pub fn new(handle: OpaqueRef, envelope: ObservationEnvelope) -> Self {
        Self { handle, envelope }
    }

    /// Explicit claim access; not automatically safe for logging or export.
    pub fn handle(&self) -> &OpaqueRef {
        &self.handle
    }

    /// Original envelope, moved intact. Parser-result Debug can expose metadata.
    pub fn envelope(&self) -> &ObservationEnvelope {
        &self.envelope
    }
}

/// Bounded, insertion-ordered synthetic set. Occurrence counts are storage
/// lengths only, never counts of unique physical events. No mutable access/removal.
#[derive(Debug)]
pub struct SyntheticSet {
    limits: Limits,
    occurrences: Vec<Occurrence>,
    relations: Vec<RelationAssertion>,
}

impl SyntheticSet {
    pub fn new(limits: Limits) -> Result<Self, Code> {
        if limits.occurrences > Limits::MAX.occurrences || limits.relations > Limits::MAX.relations
        {
            return Err(Code::InvalidLimits);
        }
        Ok(Self {
            limits,
            occurrences: Vec::new(),
            relations: Vec::new(),
        })
    }

    pub fn occurrences(&self) -> &[Occurrence] {
        &self.occurrences
    }
    pub fn relations(&self) -> &[RelationAssertion] {
        &self.relations
    }

    /// Store an unverified assertion exactly as supplied. Repeats, reversed
    /// endpoints and conflicting hints each consume one slot. Validation order:
    /// self-link, missing endpoint, capacity, allocation. Never compares payloads.
    /// Endpoint arguments are consumed even on error; occurrences are untouched.
    pub fn assert_relation(
        &mut self,
        from: OpaqueRef,
        to: OpaqueRef,
        hint: UnverifiedHint,
    ) -> Result<(), Code> {
        if from == to {
            return Err(Code::SelfLink);
        }
        if !self.occurrences.iter().any(|o| o.handle == from)
            || !self.occurrences.iter().any(|o| o.handle == to)
        {
            return Err(Code::MissingEndpoint);
        }
        if self
            .relations
            .len()
            .checked_add(1)
            .is_none_or(|n| n > self.limits.relations)
        {
            return Err(Code::RelationCapacity);
        }
        self.relations
            .try_reserve_exact(1)
            .map_err(|_| Code::AllocationFailure)?;
        self.relations.push(RelationAssertion::new(from, to, hint));
        Ok(())
    }

    /// Duplicate check precedes capacity check, including at the limit.
    /// All fallible work precedes push; existing occurrences stay intact.
    /// The supplied candidate is consumed on success or failure. Errors contain
    /// only a code, never the candidate's claims. Existing stored data is intact.
    pub fn insert(&mut self, occurrence: Occurrence) -> Result<(), Code> {
        self.prepare_occurrence(&occurrence)?;
        self.occurrences.push(occurrence);
        Ok(())
    }

    fn prepare_occurrence(&mut self, occurrence: &Occurrence) -> Result<(), Code> {
        if self
            .occurrences
            .iter()
            .any(|o| o.handle == occurrence.handle)
        {
            return Err(Code::DuplicateHandle);
        }
        if self
            .occurrences
            .len()
            .checked_add(1)
            .is_none_or(|n| n > self.limits.occurrences)
        {
            return Err(Code::OccurrenceCapacity);
        }
        self.occurrences
            .try_reserve_exact(1)
            .map_err(|_| Code::AllocationFailure)
    }
}
