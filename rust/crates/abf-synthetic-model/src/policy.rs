use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    DispositionUnconfigured,
    DispositionDenied,
    DispositionConflictDenied,
    DispositionPolicyIncompatible,
    DispositionCompatibilityUnproven,
    Unauthorized,
    Ineligible,
    ProducerUnverified,
    TemporalNewnessUnproven,
    ReplacementDenied,
    DuplicateLiveObjectGrant,
    DeclinedSuppressed,
    ManualSuppressionOverflow,
    ContinuityLost,
    Exhausted,
    Fenced,
    Retired,
    Replay,
    BindingMismatch,
    RangeInvalid,
    RecoveryUnavailable,
    LineageUnknown,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Disposition(pub Option<bool>);

/// All values are injected synthetic policy parameters, never production defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct Policy {
    pub present: Disposition,
    pub reencounter: Disposition,
    pub unknown: Disposition,
    pub compatibility_proven: bool,
    pub allow_unverified: bool,
    pub require_newness: bool,
    pub allow_replacement: bool,
    pub automatic: bool,
    pub manual: bool,
    pub retirement: bool,
}

impl Policy {
    /// Configuration guards apply even to consent presentation, before credit/disclosure.
    pub fn validate_configuration(self) -> Result<(), Reason> {
        if !self.compatibility_proven {
            return Err(Reason::DispositionCompatibilityUnproven);
        }
        if self.unknown.0 == Some(true) && self.reencounter.0 != Some(true) {
            return Err(Reason::DispositionPolicyIncompatible);
        }
        Ok(())
    }
    /// Returns an unordered applicable fixed-code set (M2F-R02).
    /// A missing hint always applies both R and U (M2F-R01).
    pub fn dispositions(self, present: bool, retained: bool) -> Result<(), Vec<Reason>> {
        let mut codes = Vec::new();
        if !self.compatibility_proven {
            codes.push(Reason::DispositionCompatibilityUnproven);
        }
        if self.unknown.0 == Some(true) && self.reencounter.0 != Some(true) {
            codes.push(Reason::DispositionPolicyIncompatible);
        }
        let terms = [
            (present, self.present.0),
            (true, self.reencounter.0),
            (!retained, self.unknown.0),
        ];
        let denied = terms
            .iter()
            .any(|&(applies, term)| applies && term == Some(false));
        let allowed = terms
            .iter()
            .any(|&(applies, term)| applies && term == Some(true));
        if terms
            .iter()
            .any(|&(applies, term)| applies && term.is_none())
        {
            codes.push(Reason::DispositionUnconfigured);
        }
        if denied {
            codes.push(if allowed {
                Reason::DispositionConflictDenied
            } else {
                Reason::DispositionDenied
            });
        }
        if codes.is_empty() { Ok(()) } else { Err(codes) }
    }
}
