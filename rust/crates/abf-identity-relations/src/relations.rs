use abf_observation::OpaqueRef;

/// Caller assertions only. No field comparison, truth check, authenticity check
/// or physical-event attestation is performed for ANY variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnverifiedHint {
    /// Caller asserts equal claimed identifier text; does not assert event identity.
    /// Which identifier(s) the caller meant is deliberately unspecified here.
    MatchingClaimedIdentifiers,
    /// Caller asserts different claimed identifier text. May conflict with another hint.
    DifferentClaimedIdentifiers,
    /// Caller asserts equal reported values; no fields are compared or summed.
    EqualReportedValues,
    PossibleReplayOrCopy,
    Unclassified,
}

/// No verified or proven-physical-event state is expressible in this gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    Unverified,
}

/// One ordered assertion. Repetition and reversed endpoint order remain distinct
/// entries, without graph traversal, transitive meaning or deduplication effects.
#[derive(Debug)]
pub struct RelationAssertion {
    from: OpaqueRef,
    to: OpaqueRef,
    hint: UnverifiedHint,
}

impl RelationAssertion {
    pub(crate) fn new(from: OpaqueRef, to: OpaqueRef, hint: UnverifiedHint) -> Self {
        Self { from, to, hint }
    }

    /// Explicit local claim access; not automatically safe for logging/export.
    pub fn from(&self) -> &OpaqueRef {
        &self.from
    }
    /// Endpoint order is preserved, not a claim of causal or directional identity.
    pub fn to(&self) -> &OpaqueRef {
        &self.to
    }
    pub fn hint(&self) -> UnverifiedHint {
        self.hint
    }
    pub fn verification(&self) -> Verification {
        Verification::Unverified
    }
}
