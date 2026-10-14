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
//! abf_synthetic_model::simulate(abf_synthetic_model::Limits { caps: [20; 12] }, |mut model| {
//!     model.retire(r);
//! });
//! ```
//! There is no arbitrary View converter or historical-to-current conversion.
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! fn bypass(v: abf_synthetic_model::View) -> FreshReceiverReadout {
//!     v.into()
//! }
//! ```
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! fn reuse(h: HistoricalInspection) -> FreshReceiverReadout { h.into() }
//! ```
//! Descriptive readouts cannot be supplied as fixture inputs. These examples
//! prove only the illustrated type restrictions, not native security guarantees.
//! Public DTOs can be freely forged and are never credentials.
//! ```
//! use abf_synthetic_adapter::*;
//! let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
//! r.contract_version = 999; // legitimate descriptive-data mutation
//! let _: SyntheticReadout = r;
//! ```
//! ```compile_fail
//! use abf_synthetic_adapter::*;
//! let r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
//! run_synthetic_fixture(r);
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
    pub subject: Subject,
    pub phase: Phase,
    pub receiver: u32,
    pub data: QualifiedData,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverUse {
    Authorized(FreshReceiverReadout),
    Refused {
        subject: Subject,
        phase: Phase,
        receiver: u32,
        reason: Reason,
    },
}
/// Descriptive projection subject, never a collection source or permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Environment(FixtureEnvironment),
    Computer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Outstanding,
    Exhausted,
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
    pub subject: Subject,
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
#[derive(Clone, Debug, PartialEq, Eq)]
struct Capture<'id> {
    observation: m::Observation<'id>,
    object: u32,
    capture: u32,
    acceptance_ordinal: usize,
    lineage: m::Lineage,
    range: m::Range,
    time: u64,
}
/// Retains the exact handles submitted to derive/computer, independently of
/// presentation sidecars. Closed capture() is the sole successful accept path.
struct Projection<'id> {
    handle: m::Derived<'id>,
    captures: Vec<Capture<'id>>,
    subject: Subject,
}
fn canonical<'id>(captures: &[Capture<'id>]) -> Vec<Capture<'id>> {
    let mut unique: Vec<Capture<'_>> = Vec::new();
    for c in captures {
        if let Some(prior) = unique.iter().find(|x| x.observation == c.observation) {
            assert_eq!(prior, c, "conflicting handle binding");
        } else {
            unique.push(c.clone());
        }
    }
    unique.sort_unstable_by_key(|c| c.acceptance_ordinal);
    assert!(
        unique
            .windows(2)
            .all(|w| w[0].acceptance_ordinal < w[1].acceptance_ordinal),
        "ambiguous acceptance order"
    );
    unique
}
fn qualified<'id>(
    view: m::View,
    projection: &Projection<'id>,
    captures: &[Capture<'id>],
) -> QualifiedData {
    let unique = canonical(captures);
    assert_eq!(
        unique, projection.captures,
        "sidecars do not match contributing handles"
    );
    assert_eq!(view.observations, unique.len(), "observation count");
    assert_eq!(view.ranges.len(), unique.len(), "range count");
    for ((env, range, time), c) in view.ranges.iter().zip(&unique) {
        assert_eq!(
            (*env, *range, *time),
            (c.lineage.environment, c.range, c.time),
            "capture tuple mismatch"
        );
    }
    let mut environments: Vec<_> = unique.iter().map(|c| c.lineage.environment).collect();
    environments.sort_unstable();
    environments.dedup();
    assert_eq!(view.environments, environments, "environment set mismatch");
    // M3A accept appends a fresh observation; derive/computer sort its indices.
    // Acceptance ordinals are recorded with the returned handle, never inferred
    // from payload, label, range or time. Projection retains the contributor set.
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
                principal: c.lineage.principal,
                harness: c.lineage.harness,
                directory: c.lineage.directory,
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
    d: &Projection<'id>,
    captures: &[Capture<'id>],
    phase: Phase,
) {
    let inspection = record(r, OperationCode::View, model.view(d.handle))
        .map(|v| HistoricalInspection {
            data: qualified(v, d, captures),
        })
        .map_err(reason);
    r.historical.push(HistoricalReadout {
        subject: d.subject,
        phase,
        inspection,
    });
}
fn receiver_use<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    d: &Projection<'id>,
    captures: &[Capture<'id>],
    receiver: u32,
    phase: Phase,
) {
    // Convert immediately after successful P, with no intervening model transition.
    let output = match model.use_result(d.handle, receiver) {
        Ok(v) => ReceiverUse::Authorized(FreshReceiverReadout {
            subject: d.subject,
            phase,
            receiver,
            data: qualified(v, d, captures),
        }),
        Err(e) => ReceiverUse::Refused {
            subject: d.subject,
            phase,
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
#[derive(Clone, Copy)]
struct FixtureObject<'id> {
    handle: m::Object<'id>,
    id: u32,
    lineage: m::Lineage,
}
fn setup<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    env: u32,
) -> (FixtureObject<'id>, m::Grant<'id>) {
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
    (
        FixtureObject {
            handle: o,
            id: env,
            lineage: lineage(env),
        },
        g,
    )
}
fn capture<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    o: FixtureObject<'id>,
    g: m::Grant<'id>,
    ids: (u32, u32),
    wrong: bool,
) -> Capture<'id> {
    assert_eq!(o.id, ids.0, "fixture object binding");
    let op = record(r, OperationCode::Begin, model.begin(g, o.handle, RANGE, 10)).unwrap();
    record(
        r,
        OperationCode::ReturnRead,
        model.return_read(op, o.handle, 42),
    )
    .unwrap();
    if wrong {
        let _ = record(r, OperationCode::Release, model.release(op, 99));
    }
    let released = record(r, OperationCode::Release, model.release(op, 11)).unwrap();
    if wrong {
        let _ = record(r, OperationCode::Accept, model.accept(released, 99));
    }
    let observation = record(r, OperationCode::Accept, model.accept(released, 11)).unwrap();
    // Count only successful accepts, including this one; failed accepts never
    // append an observation. All accepts in this closed adapter pass here.
    let acceptance_ordinal = r
        .operations
        .iter()
        .filter(|o| o.operation == OperationCode::Accept && o.outcome == Outcome::Succeeded)
        .count()
        - 1;
    Capture {
        acceptance_ordinal,
        lineage: o.lineage,
        range: RANGE,
        time: 0,
        observation,
        object: ids.0,
        capture: ids.1,
    }
}
fn derive<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    captures: &[Capture<'id>],
) -> Projection<'id> {
    let obs: Vec<_> = captures.iter().map(|c| c.observation).collect();
    let handle = record(r, OperationCode::Derive, model.derive(&obs)).unwrap();
    let captures = canonical(captures);
    assert!(
        captures
            .iter()
            .all(|c| c.lineage.environment == captures[0].lineage.environment),
        "closed environment projection"
    );
    Projection {
        handle,
        subject: Subject::Environment(environment(captures[0].lineage.environment)),
        captures,
    }
}
fn computer<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    inputs: &[&Projection<'id>],
) -> Projection<'id> {
    let handles: Vec<_> = inputs.iter().map(|d| d.handle).collect();
    let handle = record(r, OperationCode::Computer, model.computer(&handles)).unwrap();
    let captures: Vec<_> = inputs
        .iter()
        .flat_map(|d| d.captures.iter().cloned())
        .collect();
    Projection {
        handle,
        captures: canonical(&captures),
        subject: Subject::Computer,
    }
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
            let d = computer(r, model, &[&da, &db]);
            (d, alloc::vec![a.clone(), b])
        }
        SyntheticCase::RepeatedHandleAndEqualCapture => {
            let b = capture(r, model, o, g, (1, 2), false);
            let db = derive(r, model, &[a.clone(), a.clone(), b.clone()]);
            let d = computer(r, model, &[&da, &da, &db]);
            (d, alloc::vec![a.clone(), b])
        }
        _ => {
            let d = computer(r, model, &[&da]);
            (d, alloc::vec![a.clone()])
        }
    };
    inspect(r, model, &d, &captures, Phase::Qualified);
    if case == SyntheticCase::ReceiverAndRevocation {
        snapshot(r, model, Phase::Qualified);
        receiver_use(r, model, &d, &captures, 99, Phase::Qualified);
        snapshot(r, model, Phase::Qualified);
    }
    receiver_use(r, model, &d, &captures, 11, Phase::Qualified);
    match case {
        SyntheticCase::ReceiverAndRevocation => {
            record(r, OperationCode::Retire, model.retire(g)).unwrap();
            let _ = record(r, OperationCode::Derive, model.derive(&[a.observation]));
            let _ = record(r, OperationCode::Computer, model.computer(&[da.handle]));
            receiver_use(r, model, &d, &captures, 11, Phase::Revoked);
            inspect(r, model, &d, &captures, Phase::Revoked);
        }
        SyntheticCase::DowngradeAndLineageLoss => {
            record(
                r,
                OperationCode::Downgrade,
                model.downgrade(a.observation, true, true),
            )
            .unwrap();
            inspect(r, model, &da, &captures, Phase::Downgraded);
            inspect(r, model, &d, &captures, Phase::Downgraded);
            receiver_use(r, model, &d, &captures, 11, Phase::Downgraded);
            record(r, OperationCode::LoseLineage, model.lose_lineage(da.handle)).unwrap();
            inspect(r, model, &d, &captures, Phase::LineageLost);
            receiver_use(r, model, &d, &captures, 11, Phase::LineageLost);
            let _ = record(r, OperationCode::Computer, model.computer(&[d.handle]));
        }
        _ => {}
    }
}
fn outstanding<'id>(
    r: &mut SyntheticReadout,
    model: &mut m::Model<'id>,
    o: FixtureObject<'id>,
    g: m::Grant<'id>,
) {
    let op = record(r, OperationCode::Begin, model.begin(g, o.handle, RANGE, 10)).unwrap();
    snapshot(r, model, Phase::Outstanding);
    for _ in 0..3 {
        let _ = record(r, OperationCode::Begin, model.begin(g, o.handle, RANGE, 10));
    }
    snapshot(r, model, Phase::Exhausted);
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
    record(
        r,
        OperationCode::ReturnRead,
        model.return_read(op, o.handle, 42),
    )
    .unwrap();
    snapshot(r, model, Phase::Returned);
}

#[cfg(test)]
mod association_tests {
    use super::*;
    #[test]
    fn reversed_equal_sidecars_preserve_handle_binding() {
        m::simulate(m::Limits { caps: CAPS }, |mut model| {
            let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
            let (o, g) = setup(&mut r, &mut model, 1);
            let a = capture(&mut r, &mut model, o, g, (1, 1), false);
            let b = capture(&mut r, &mut model, o, g, (1, 2), false);
            let d = derive(&mut r, &mut model, &[a.clone(), a.clone(), b.clone()]);
            let data = qualified(model.view(d.handle).unwrap(), &d, &[b, a]);
            assert_eq!(
                data.captures
                    .iter()
                    .map(|c| c.fixture_capture)
                    .collect::<Vec<_>>(),
                alloc::vec![1, 2]
            );
        });
    }
    #[test]
    fn reversed_two_environment_sidecars_preserve_handle_binding() {
        m::simulate(m::Limits { caps: CAPS }, |mut model| {
            let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
            let (o, g) = setup(&mut r, &mut model, 1);
            let a = capture(&mut r, &mut model, o, g, (1, 1), false);
            let (o2, g2) = setup(&mut r, &mut model, 2);
            let b = capture(&mut r, &mut model, o2, g2, (2, 2), false);
            let da = derive(&mut r, &mut model, core::slice::from_ref(&a));
            let db = derive(&mut r, &mut model, core::slice::from_ref(&b));
            let d = computer(&mut r, &mut model, &[&db, &da, &da]);
            let data = qualified(model.view(d.handle).unwrap(), &d, &[b, a]);
            assert_eq!(
                data.captures
                    .iter()
                    .map(|c| (c.fixture_capture, c.environment.id))
                    .collect::<Vec<_>>(),
                alloc::vec![(1, 1), (2, 2)]
            );
        });
    }
    #[test]
    fn repeated_presented_handles_collapse() {
        m::simulate(m::Limits { caps: CAPS }, |mut model| {
            let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
            let (o, g) = setup(&mut r, &mut model, 1);
            let a = capture(&mut r, &mut model, o, g, (1, 1), false);
            let b = capture(&mut r, &mut model, o, g, (1, 2), false);
            let d = derive(&mut r, &mut model, &[b.clone(), a.clone(), a.clone()]);
            let data = qualified(
                model.view(d.handle).unwrap(),
                &d,
                &[b.clone(), a.clone(), b, a],
            );
            assert_eq!(data.observation_count, 2);
            assert_eq!(
                data.captures
                    .iter()
                    .map(|c| c.fixture_capture)
                    .collect::<Vec<_>>(),
                alloc::vec![1, 2]
            );
        });
    }
    fn mismatch(change: impl FnOnce(&mut Vec<Capture<'_>>, &mut m::View)) {
        m::simulate(m::Limits { caps: CAPS }, |mut model| {
            let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
            let (o, g) = setup(&mut r, &mut model, 1);
            let a = capture(&mut r, &mut model, o, g, (1, 1), false);
            let b = capture(&mut r, &mut model, o, g, (1, 2), false);
            let d = derive(&mut r, &mut model, core::slice::from_ref(&a));
            let mut sidecars = alloc::vec![a, b];
            sidecars.pop();
            let mut view = model.view(d.handle).unwrap();
            change(&mut sidecars, &mut view);
            let _ = qualified(view, &d, &sidecars);
        });
    }
    #[test]
    #[should_panic(expected = "sidecars do not match contributing handles")]
    fn missing_mapping_fails_closed() {
        mismatch(|c, _| c.clear());
    }
    #[test]
    #[should_panic(expected = "sidecars do not match contributing handles")]
    fn altered_capture_identity_fails_closed() {
        mismatch(|c, _| c[0].capture = 2);
    }
    #[test]
    #[should_panic(expected = "sidecars do not match contributing handles")]
    fn altered_ordinal_fails_closed() {
        mismatch(|c, _| c[0].acceptance_ordinal = 1);
    }
    #[test]
    #[should_panic(expected = "capture tuple mismatch")]
    fn altered_environment_fails_closed() {
        mismatch(|_, v| v.ranges[0].0 = 2);
    }
    #[test]
    #[should_panic(expected = "capture tuple mismatch")]
    fn altered_range_fails_closed() {
        mismatch(|_, v| v.ranges[0].1.length = 3);
    }
    #[test]
    #[should_panic(expected = "capture tuple mismatch")]
    fn altered_time_fails_closed() {
        mismatch(|_, v| v.ranges[0].2 = 1);
    }
    #[test]
    #[should_panic(expected = "observation count")]
    fn altered_observation_count_fails_closed() {
        mismatch(|_, v| v.observations = 2);
    }
    #[test]
    #[should_panic(expected = "range count")]
    fn missing_range_fails_closed() {
        mismatch(|_, v| v.ranges.clear());
    }
    #[test]
    #[should_panic(expected = "sidecars do not match contributing handles")]
    fn same_count_noncontributing_handle_fails_closed() {
        m::simulate(m::Limits { caps: CAPS }, |mut model| {
            let mut r = run_synthetic_fixture(SyntheticCase::DenyAllDefault);
            let (o, g) = setup(&mut r, &mut model, 1);
            let a = capture(&mut r, &mut model, o, g, (1, 1), false);
            let b = capture(&mut r, &mut model, o, g, (1, 2), false);
            let d = derive(&mut r, &mut model, &[a]);
            let _ = qualified(model.view(d.handle).unwrap(), &d, &[b]);
        });
    }
}
