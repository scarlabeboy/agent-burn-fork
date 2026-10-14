#![no_std]
#![forbid(unsafe_code)]
//! Closed in-memory synthetic scenarios. Every output is descriptive, forgeable
//! data, never a credential, native identity, or permission to perform work.
//!
//! Native input and arbitrary policy are not accepted.
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! run_synthetic_fixture("/native/history");
//! ```
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! run_synthetic_fixture(SyntheticCase::DenyAllDefault, abf_synthetic_model::Policy::default());
//! ```
//! Model capabilities cannot be constructed or extracted through this API.
//! ```compile_fail
//! use abf_synthetic_adapter::{Grant, Model, simulate};
//! ```
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! let r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
//! let capability = r.grant;
//! ```
//! There is no arbitrary View converter or historical-to-current conversion.
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! fn bypass(v: abf_synthetic_model::View) -> FreshReceiverReadout {
//!     FreshReceiverReadout::from_view(v)
//! }
//! ```
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! fn reuse(h: HistoricalInspection) -> FreshReceiverReadout { h.into() }
//! ```
//! No filesystem, process, network, credential or UI operation is exposed.
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! run_synthetic_fixture(SyntheticCase::DenyAllDefault).collect("/native/history");
//! ```
extern crate alloc;
use abf_synthetic_model as m;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyntheticCase {
    #[default]
    DenyAllDefault,
    TwoEnvironmentQualifiedProjection,
    RepeatedHandleAndEqualCapture,
    ReceiverAndRevocation,
    DowngradeAndLineageLoss,
    ExhaustionAndOutstandingRead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    SyntheticOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderAccount {
    NotModeled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentLabel {
    SimulatedWindows,
    SimulatedSelectedWsl,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixtureEnvironment {
    pub id: u32,
    pub label: EnvironmentLabel,
}
/// Exact M3A refusal codes, without native status claims.
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
fn reason(value: m::Reason) -> Reason {
    match value {
        m::Reason::DispositionUnconfigured => Reason::DispositionUnconfigured,
        m::Reason::DispositionDenied => Reason::DispositionDenied,
        m::Reason::DispositionConflictDenied => Reason::DispositionConflictDenied,
        m::Reason::DispositionPolicyIncompatible => Reason::DispositionPolicyIncompatible,
        m::Reason::DispositionCompatibilityUnproven => Reason::DispositionCompatibilityUnproven,
        m::Reason::Unauthorized => Reason::Unauthorized,
        m::Reason::Ineligible => Reason::Ineligible,
        m::Reason::ProducerUnverified => Reason::ProducerUnverified,
        m::Reason::TemporalNewnessUnproven => Reason::TemporalNewnessUnproven,
        m::Reason::ReplacementDenied => Reason::ReplacementDenied,
        m::Reason::DuplicateLiveObjectGrant => Reason::DuplicateLiveObjectGrant,
        m::Reason::DeclinedSuppressed => Reason::DeclinedSuppressed,
        m::Reason::ManualSuppressionOverflow => Reason::ManualSuppressionOverflow,
        m::Reason::ContinuityLost => Reason::ContinuityLost,
        m::Reason::Exhausted => Reason::Exhausted,
        m::Reason::Fenced => Reason::Fenced,
        m::Reason::Retired => Reason::Retired,
        m::Reason::Replay => Reason::Replay,
        m::Reason::BindingMismatch => Reason::BindingMismatch,
        m::Reason::RangeInvalid => Reason::RangeInvalid,
        m::Reason::RecoveryUnavailable => Reason::RecoveryUnavailable,
        m::Reason::LineageUnknown => Reason::LineageUnknown,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleRange {
    pub start: u32,
    pub length: u32,
    pub domain_end: u32,
}
/// Fixture sidecars only: neither logical events nor verified native provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureAssociation {
    pub fixture_object: u32,
    pub fixture_capture: u32,
    pub environment: FixtureEnvironment,
    pub principal: u32,
    pub harness: u32,
    pub directory: u32,
    pub range: SampleRange,
    pub symbolic_capture_time: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedData {
    pub origin: Origin,
    pub qualification: Qualification,
    pub observation_count: usize,
    pub symbolic_sampled_extent: u32,
    pub environments: Vec<FixtureEnvironment>,
    pub captures: Vec<CaptureAssociation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalInspection {
    pub data: QualifiedData,
}
/// Successful use at this sequence point; never reusable authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshReceiverReadout {
    pub receiver: u32,
    pub data: QualifiedData,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverUse {
    Authorized(FreshReceiverReadout),
    Refused { receiver: u32, reason: Reason },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Qualified,
    Revoked,
    Downgraded,
    LineageLost,
    Deadline,
    SessionLoss,
    Returned,
    Completed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalReadout {
    pub phase: Phase,
    pub inspection: Result<HistoricalInspection, Reason>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationCode {
    FixtureObject,
    Approve,
    Discover,
    Admit,
    RequestPrompt,
    Begin,
    ReturnRead,
    Release,
    Accept,
    Derive,
    Computer,
    View,
    UseResult,
    Retire,
    Downgrade,
    LoseLineage,
    Step,
    SessionLoss,
    Restart,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Refused(Reason),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationOutcome {
    pub operation: OperationCode,
    pub outcome: Outcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelCounters {
    pub admissions: u32,
    pub prompts: u32,
    pub releases: u32,
    pub uses: u32,
    pub reads: u32,
    pub post_samples: u32,
    pub discoveries: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceSnapshot {
    pub phase: Phase,
    pub held: [u32; 12],
    pub spent: [u32; 12],
    pub peak: [u32; 12],
    pub counters: ModelCounters,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticReadout {
    pub origin: Origin,
    pub contract_version: u32,
    pub case: SyntheticCase,
    pub account: ProviderAccount,
    /// Finite scenario parameters, not production budgets or RSS guarantees.
    pub fixture_caps: [u32; 12],
    pub historical: Vec<HistoricalReadout>,
    pub receiver_uses: Vec<ReceiverUse>,
    pub operations: Vec<OperationOutcome>,
    pub snapshots: Vec<ResourceSnapshot>,
}
const CAPS: [u32; 12] = [20, 8, 8, 8, 8, 8, 80, 300, 4, 4, 2, 200];
const RANGE: m::Range = m::Range {
    start: 0,
    length: 4,
    domain_end: 64,
};
fn lineage(environment: u32) -> m::Lineage {
    m::Lineage {
        environment,
        principal: 7,
        harness: 3,
        directory: 9,
    }
}
fn spec(environment: u32, policy: m::Policy) -> m::ScopeSpec {
    m::ScopeSpec {
        lineage: lineage(environment),
        policy,
        policy_version: 1,
        metadata: true,
        display: true,
        bytes: true,
        release: true,
        use_results: true,
        receiver: 11,
        display_receiver: 11,
        valid_until: 100,
    }
}
fn permit() -> m::Policy {
    m::Policy {
        present: m::Disposition(Some(true)),
        reencounter: m::Disposition(Some(true)),
        unknown: m::Disposition(Some(true)),
        compatibility_proven: true,
        allow_unverified: true,
        automatic: true,
        manual: true,
        retirement: true,
        ..m::Policy::default()
    }
}
fn environment(id: u32) -> FixtureEnvironment {
    FixtureEnvironment {
        id,
        label: match id {
            1 => EnvironmentLabel::SimulatedWindows,
            2 => EnvironmentLabel::SimulatedSelectedWsl,
            _ => unreachable!("closed fixture environments"),
        },
    }
}
fn record<T>(
    r: &mut SyntheticReadout,
    operation: OperationCode,
    result: Result<T, m::Reason>,
) -> Result<T, m::Reason> {
    r.operations.push(OperationOutcome {
        operation,
        outcome: match &result {
            Ok(_) => Outcome::Succeeded,
            Err(e) => Outcome::Refused(reason(*e)),
        },
    });
    result
}
fn snapshot(r: &mut SyntheticReadout, model: &m::Model<'_>, phase: Phase) {
    let v = model.resources();
    let c = model.counters();
    r.snapshots.push(ResourceSnapshot {
        phase,
        held: v.held,
        spent: v.spent,
        peak: v.peak,
        counters: ModelCounters {
            admissions: c.admissions,
            prompts: c.prompts,
            releases: c.releases,
            uses: c.uses,
            reads: c.reads,
            post_samples: c.post_samples,
            discoveries: c.discoveries,
        },
    });
}
/// Private handle-keyed association; handles cannot leave simulate.
#[derive(Clone)]
struct Capture<'id> {
    observation: m::Observation<'id>,
    object: u32,
    capture: u32,
}
fn qualified(view: m::View, captures: &[Capture<'_>]) -> QualifiedData {
    let mut unique = Vec::<Capture<'_>>::new();
    for c in captures {
        if !unique.iter().any(|x| x.observation == c.observation) {
            unique.push(c.clone());
        }
    }
    assert_eq!(view.observations, unique.len());
    assert_eq!(view.ranges.len(), unique.len());
    // Fixed wiring supplies captures in M3A observation creation order.
    let q = view.qualification;
    QualifiedData {
        origin: Origin::SyntheticOnly,
        qualification: Qualification {
            inventory_partial: q.inventory_partial,
            byte_partial: q.byte_partial,
            version_unproven: q.version_unproven,
            producer_unverified: q.producer_unverified,
            identity_unresolved: q.identity_unresolved,
            stale: q.stale,
            disputed: q.disputed,
            revoked: q.revoked,
        },
        observation_count: view.observations,
        symbolic_sampled_extent: view.sampled_bytes,
        environments: view.environments.into_iter().map(environment).collect(),
        captures: view
            .ranges
            .into_iter()
            .zip(unique)
            .map(|((env, range, time), c)| CaptureAssociation {
                fixture_object: c.object,
                fixture_capture: c.capture,
                environment: environment(env),
                principal: 7,
                harness: 3,
                directory: 9,
                range: SampleRange {
                    start: range.start,
                    length: range.length,
                    domain_end: range.domain_end,
                },
                symbolic_capture_time: time,
            })
            .collect(),
    }
}
fn inspect<'id>(
    r: &mut SyntheticReadout,
    model: &m::Model<'id>,
    d: m::Derived<'id>,
    captures: &[Capture<'id>],
    phase: Phase,
) {
    let inspection = record(r, OperationCode::View, model.view(d))
        .map(|v| HistoricalInspection {
            data: qualified(v, captures),
        })
        .map_err(reason);
    r.historical.push(HistoricalReadout { phase, inspection });
}
fn receiver_use<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    d: m::Derived<'id>,
    captures: &[Capture<'id>],
    receiver: u32,
) {
    // Convert immediately after successful P, with no intervening model transition.
    let output = match model.use_result(d, receiver) {
        Ok(v) => ReceiverUse::Authorized(FreshReceiverReadout {
            receiver,
            data: qualified(v, captures),
        }),
        Err(e) => ReceiverUse::Refused {
            receiver,
            reason: reason(e),
        },
    };
    r.operations.push(OperationOutcome {
        operation: OperationCode::UseResult,
        outcome: match &output {
            ReceiverUse::Authorized(_) => Outcome::Succeeded,
            ReceiverUse::Refused { reason, .. } => Outcome::Refused(*reason),
        },
    });
    r.receiver_uses.push(output);
}
fn setup<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    env: u32,
) -> (m::Object<'id>, m::Grant<'id>) {
    let o = record(
        r,
        OperationCode::FixtureObject,
        model.fixture_object(lineage(env), 1, true, false),
    )
    .unwrap();
    let s = record(
        r,
        OperationCode::Approve,
        model.approve(spec(env, permit())),
    )
    .unwrap();
    let p = record(
        r,
        OperationCode::Discover,
        model.discover(s, o, true, false),
    )
    .unwrap();
    let g = record(r, OperationCode::Admit, model.admit(p, false)).unwrap();
    (o, g)
}
fn capture<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    o: m::Object<'id>,
    g: m::Grant<'id>,
    ids: (u32, u32),
    wrong: bool,
) -> Capture<'id> {
    let op = record(r, OperationCode::Begin, model.begin(g, o, RANGE, 10)).unwrap();
    record(r, OperationCode::ReturnRead, model.return_read(op, o, 42)).unwrap();
    if wrong {
        let _ = record(r, OperationCode::Release, model.release(op, 99));
    }
    let released = record(r, OperationCode::Release, model.release(op, 11)).unwrap();
    if wrong {
        let _ = record(r, OperationCode::Accept, model.accept(released, 99));
    }
    let observation = record(r, OperationCode::Accept, model.accept(released, 11)).unwrap();
    Capture {
        observation,
        object: ids.0,
        capture: ids.1,
    }
}
fn derive<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    captures: &[Capture<'id>],
) -> m::Derived<'id> {
    let obs: Vec<_> = captures.iter().map(|c| c.observation).collect();
    record(r, OperationCode::Derive, model.derive(&obs)).unwrap()
}
/// Every invocation constructs a fresh synchronous model universe with fixed facts.
/// Nothing returned can be supplied back to this API.
pub fn run_synthetic_fixture(case: SyntheticCase) -> SyntheticReadout {
    let mut caps = CAPS;
    if case == SyntheticCase::ExhaustionAndOutstandingRead {
        caps[1] = 1;
    }
    m::simulate(m::Limits { caps }, |mut model| {
        let mut r = SyntheticReadout {
            origin: Origin::SyntheticOnly,
            contract_version: 1,
            case,
            account: ProviderAccount::NotModeled,
            fixture_caps: caps,
            historical: Vec::new(),
            receiver_uses: Vec::new(),
            operations: Vec::new(),
            snapshots: Vec::new(),
        };
        match case {
            SyntheticCase::DenyAllDefault => {
                let o = record(
                    &mut r,
                    OperationCode::FixtureObject,
                    model.fixture_object(lineage(1), 1, true, false),
                )
                .unwrap();
                let s = record(
                    &mut r,
                    OperationCode::Approve,
                    model.approve(spec(1, m::Policy::default())),
                )
                .unwrap();
                let p = record(
                    &mut r,
                    OperationCode::Discover,
                    model.discover(s, o, true, false),
                )
                .unwrap();
                let _ = record(
                    &mut r,
                    OperationCode::RequestPrompt,
                    model.request_prompt(p, 11, true),
                );
                let _ = record(&mut r, OperationCode::Admit, model.admit(p, false));
            }
            _ => projection_case(&mut r, &mut model, case),
        }
        snapshot(&mut r, &model, Phase::Completed);
        r
    })
}
fn projection_case<'id>(r: &mut SyntheticReadout, model: &mut m::Model<'id>, case: SyntheticCase) {
    let (o, g) = setup(r, model, 1);
    if case == SyntheticCase::ExhaustionAndOutstandingRead {
        outstanding(r, model, o, g);
        return;
    }
    let a = capture(
        r,
        model,
        o,
        g,
        (1, 1),
        case == SyntheticCase::ReceiverAndRevocation,
    );
    let da = derive(r, model, core::slice::from_ref(&a));
    let (d, captures) = match case {
        SyntheticCase::TwoEnvironmentQualifiedProjection => {
            let (o2, g2) = setup(r, model, 2);
            let b = capture(r, model, o2, g2, (2, 2), false);
            let db = derive(r, model, core::slice::from_ref(&b));
            let d = record(r, OperationCode::Computer, model.computer(&[da, db])).unwrap();
            (d, alloc::vec![a.clone(), b])
        }
        SyntheticCase::RepeatedHandleAndEqualCapture => {
            let b = capture(r, model, o, g, (1, 2), false);
            let db = derive(r, model, &[a.clone(), a.clone(), b.clone()]);
            let d = record(r, OperationCode::Computer, model.computer(&[da, da, db])).unwrap();
            (d, alloc::vec![a.clone(), b])
        }
        _ => {
            let d = record(r, OperationCode::Computer, model.computer(&[da])).unwrap();
            (d, alloc::vec![a.clone()])
        }
    };
    inspect(r, model, d, &captures, Phase::Qualified);
    if case == SyntheticCase::ReceiverAndRevocation {
        snapshot(r, model, Phase::Qualified);
        receiver_use(r, model, d, &captures, 99);
        snapshot(r, model, Phase::Qualified);
    }
    receiver_use(r, model, d, &captures, 11);
    match case {
        SyntheticCase::ReceiverAndRevocation => {
            record(r, OperationCode::Retire, model.retire(g)).unwrap();
            let _ = record(r, OperationCode::Derive, model.derive(&[a.observation]));
            let _ = record(r, OperationCode::Computer, model.computer(&[da]));
            receiver_use(r, model, d, &captures, 11);
            inspect(r, model, d, &captures, Phase::Revoked);
        }
        SyntheticCase::DowngradeAndLineageLoss => {
            record(
                r,
                OperationCode::Downgrade,
                model.downgrade(a.observation, true, true),
            )
            .unwrap();
            inspect(r, model, da, &captures, Phase::Downgraded);
            inspect(r, model, d, &captures, Phase::Downgraded);
            receiver_use(r, model, d, &captures, 11);
            record(r, OperationCode::LoseLineage, model.lose_lineage(da)).unwrap();
            inspect(r, model, d, &captures, Phase::LineageLost);
            receiver_use(r, model, d, &captures, 11);
            let _ = record(r, OperationCode::Computer, model.computer(&[d]));
        }
        _ => {}
    }
}
fn outstanding<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    o: m::Object<'id>,
    g: m::Grant<'id>,
) {
    let op = record(r, OperationCode::Begin, model.begin(g, o, RANGE, 10)).unwrap();
    snapshot(r, model, Phase::Qualified);
    for _ in 0..3 {
        let _ = record(r, OperationCode::Begin, model.begin(g, o, RANGE, 10));
    }
    snapshot(r, model, Phase::Qualified);
    record(r, OperationCode::Step, model.step(10)).unwrap();
    snapshot(r, model, Phase::Deadline);
    model.session_loss();
    r.operations.push(OperationOutcome {
        operation: OperationCode::SessionLoss,
        outcome: Outcome::Succeeded,
    });
    snapshot(r, model, Phase::SessionLoss);
    let _ = record(
        r,
        OperationCode::Restart,
        model.restart(m::IndependentEvidence::CompleteDrain, true),
    );
    let _ = record(r, OperationCode::Release, model.release(op, 11));
    record(r, OperationCode::ReturnRead, model.return_read(op, o, 42)).unwrap();
    snapshot(r, model, Phase::Returned);
}
