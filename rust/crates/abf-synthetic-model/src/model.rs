use crate::{Limits, Policy, Reason, Resources};
use alloc::vec::Vec;
use core::marker::PhantomData;

type Brand<'id> = PhantomData<&'id mut &'id ()>;
macro_rules! capability {
    ($($name:ident),+) => {$ (
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name<'id> { index: usize, brand: Brand<'id> }
        impl $name<'_> { fn new(index: usize) -> Self { Self { index, brand: PhantomData } } }
    )+};
}
capability!(
    Object,
    Scope,
    Proposal,
    Grant,
    Operation,
    Observation,
    Derived
);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Released<'id> {
    index: usize,
    brand: Brand<'id>,
    payload: u32,
}
impl Released<'_> {
    pub fn payload(self) -> u32 {
        self.payload
    }
}
impl core::fmt::Debug for Released<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Released(opaque)")
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Prompt {
    pub name_hint: u32,
    pub producer_unverified: bool,
}
impl core::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Prompt(untrusted hint redacted)")
    }
}

/// Synthetic authorization intent, not object, account or producer identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lineage {
    pub environment: u32,
    pub principal: u32,
    pub harness: u32,
    pub directory: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct ScopeSpec {
    pub lineage: Lineage,
    pub policy: Policy,
    pub policy_version: u32,
    pub metadata: bool,
    pub display: bool,
    pub bytes: bool,
    pub release: bool,
    pub use_results: bool,
    pub receiver: u32,
    pub display_receiver: u32,
    pub valid_until: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: u32,
    pub length: u32,
    pub domain_end: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Qualification {
    pub inventory_partial: bool,
    pub byte_partial: bool,
    pub version_unproven: bool,
    pub producer_unverified: bool,
    pub identity_unresolved: bool,
    pub stale: bool,
    pub disputed: bool,
    pub revoked: bool,
}
impl Qualification {
    fn inherit(&mut self, q: Self) {
        self.inventory_partial |= q.inventory_partial;
        self.byte_partial |= q.byte_partial;
        self.version_unproven |= q.version_unproven;
        self.producer_unverified |= q.producer_unverified;
        self.identity_unresolved |= q.identity_unresolved;
        self.stale |= q.stale;
        self.disputed |= q.disputed;
        self.revoked |= q.revoked;
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub qualification: Qualification,
    pub observations: usize,
    pub sampled_bytes: u32,
    pub environments: Vec<u32>,
    pub ranges: Vec<(u32, Range, u64)>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub admissions: u32,
    pub prompts: u32,
    pub releases: u32,
    pub uses: u32,
    pub reads: u32,
    pub post_samples: u32,
    pub discoveries: u32,
}
/// Deliberately supplied witness classes at a synthetic independent boundary.
/// These enums are not native evidence, signatures, PIDs, or self-attestations.
#[derive(Clone, Copy, Debug)]
pub enum IndependentEvidence {
    SelfReport,
    PartialInventory,
    OwnersOnly,
    CompleteDrain,
}

struct Obj {
    lineage: Lineage,
    hint: u32,
    eligible: bool,
    producer: bool,
    holder: u32,
    sampled_size: u32,
}
struct ScopeRow {
    spec: ScopeSpec,
    range_end: u32,
    epoch: u32,
    active: bool,
    owners: bool,
}
struct ProposalRow {
    scope: usize,
    object: usize,
    present: bool,
    replacement: bool,
    held: bool,
    prompted: bool,
}
struct GrantRow {
    scope: usize,
    object: usize,
    active: bool,
    owner: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpState {
    Admitted,
    Staged,
    Released,
    Accepted,
    Disposed,
}
struct OpRow {
    grant: usize,
    object: usize,
    range: Range,
    deadline: u64,
    state: OpState,
    fenced: bool,
    copies: u32,
    pair: bool,
    stage: bool,
    payload: u32,
    capture: u64,
}
struct ObsRow {
    op: usize,
    qualification: Qualification,
    revision: u32,
}
struct DerivedRow {
    observations: Vec<usize>,
    lineage_known: bool,
}

/// Run one branded synthetic universe. Authority cannot be constructed or moved
/// between universes by callers; all IDs have private fields and invariant brands.
///
/// ```compile_fail
/// use abf_synthetic_model::Grant;
/// let forged = Grant { index: 0, brand: core::marker::PhantomData };
/// ```
/// ```compile_fail
/// use abf_synthetic_model::*;
/// let escaped = simulate(Limits { caps: [10; 12] }, |mut m| {
///     m.fixture_object(Lineage { environment: 1, principal: 1, harness: 1, directory: 1 }, 1, true, false).unwrap()
/// });
/// ```
pub fn simulate<R>(limits: Limits, f: impl for<'id> FnOnce(Model<'id>) -> R) -> R {
    f(Model {
        limits,
        resources: Resources::default(),
        counters: Counters::default(),
        objects: Vec::new(),
        scopes: Vec::new(),
        proposals: Vec::new(),
        grants: Vec::new(),
        ops: Vec::new(),
        observations: Vec::new(),
        derived: Vec::new(),
        hints: Vec::new(),
        declines: Vec::new(),
        overflow: Vec::new(),
        consent_continuity: true,
        now: 0,
        broker: 1,
        holder: 1,
        fenced: false,
        quiescent: false,
        brand: PhantomData,
    })
}

pub struct Model<'id> {
    limits: Limits,
    resources: Resources,
    counters: Counters,
    objects: Vec<Obj>,
    scopes: Vec<ScopeRow>,
    proposals: Vec<ProposalRow>,
    grants: Vec<GrantRow>,
    ops: Vec<OpRow>,
    observations: Vec<ObsRow>,
    derived: Vec<DerivedRow>,
    hints: Vec<(Lineage, u32)>,
    declines: Vec<(Lineage, u32)>,
    overflow: Vec<Lineage>,
    consent_continuity: bool,
    now: u64,
    broker: u32,
    holder: u32,
    fenced: bool,
    quiescent: bool,
    brand: Brand<'id>,
}
impl<'id> Model<'id> {
    fn charge(&mut self, costs: &[(usize, u32)]) -> Result<(), Reason> {
        self.resources.reserve(self.limits, costs)
    }
    fn scope_live(&self, s: usize) -> Result<(), Reason> {
        if self.fenced {
            return Err(Reason::Fenced);
        }
        let row = &self.scopes[s];
        if !row.active || row.epoch != self.broker || self.now >= row.spec.valid_until {
            return Err(Reason::Retired);
        }
        Ok(())
    }
    fn grant_live(&self, g: usize) -> Result<(), Reason> {
        self.scope_live(self.grants[g].scope)?;
        if !self.grants[g].active {
            return Err(Reason::Retired);
        }
        Ok(())
    }
    fn suppression(&self, lineage: Lineage, hint: u32) -> Result<(), Reason> {
        if !self.consent_continuity {
            return Err(Reason::ContinuityLost);
        }
        if self.overflow.contains(&lineage) {
            return Err(Reason::ManualSuppressionOverflow);
        }
        if self.declines.contains(&(lineage, hint)) {
            return Err(Reason::DeclinedSuppressed);
        }
        Ok(())
    }
    pub fn resources(&self) -> Resources {
        self.resources
    }
    pub fn counters(&self) -> Counters {
        self.counters
    }
}

mod consent;
mod lifecycle;
mod qualification;
mod ranges;
