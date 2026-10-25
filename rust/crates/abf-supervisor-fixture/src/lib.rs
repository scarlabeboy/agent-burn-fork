//! ABF124B: bounded, synchronous, fictional Windows supervisor.
//! ABF124D preserves cancellation alongside projection refusal diagnostics.
//! No I/O, callbacks, clocks, native authority, WSL capability or persistence.
#![forbid(unsafe_code)]

use abf_catalog_plan_fixture::{DecisionState, FixturePlan};
use abf_reader_fixture::{Step, emit};
use abf_source_run::*;
use std::sync::Arc;

/// Fixed diagnostics preserve dependency errors without embedding input text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Completed,
    Degraded(CoverageVerdict),
    /// Primary outcome for emitted cancellation, including projection refusal.
    Cancelled,
    SingleConditionalWindowsScopeRequired,
    Reader(abf_reader_fixture::Error),
    Identity(RefCode),
    Finalize(FinalizeCode),
    Adapt(AdaptCode),
    Diagnostics(DiagnosticsCode),
    BarrierIndex,
}
impl Code {
    pub fn action(self) -> &'static str {
        match self {
            Self::Completed => "Inspect the immutable synthetic evidence and its qualifiers.",
            Self::Degraded(_) => {
                "Inspect enumeration, source outcomes and late diagnostics; retain prior reliable evidence."
            }
            Self::Cancelled => {
                "Inspect any retained prefix and projection error; a new attempt requires an explicit call."
            }
            Self::SingleConditionalWindowsScopeRequired => {
                "Supply exactly one conditional Windows decision from S2; no plan grants native authority."
            }
            Self::Reader(_) => {
                "Correct the synthetic script or reduce work to S1 ceilings; no partial emission is accepted."
            }
            Self::Identity(_) => {
                "Inspect fixture issuer exhaustion; do not reset or reuse identities to continue this supervisor."
            }
            Self::Finalize(_) => {
                "Correct the fictional process/drain evidence or script; no finalization was accepted."
            }
            Self::Adapt(_) => {
                "Inspect admission/projection refusal; do not filter, truncate or batch the evidence."
            }
            Self::Diagnostics(_) => {
                "Correct late delivery identity/representation; do not reopen the finalization."
            }
            Self::BarrierIndex => {
                "Choose a barrier index within the emitted delivery count, including cancellation suppression."
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Fresh,
    Stale,
    Unavailable,
}
/// Explicit fictional observations, never OS observations or elapsed-time proof.
/// Emitted script cancellation supersedes `process` with fictional KilledCancel,
/// including a supplied crash or timeout. This is model precedence, not a separate
/// native termination observation. It never upgrades the supplied barrier.
pub struct SyntheticFacts {
    pub process: ProcessEnd,
    pub barrier: Barrier,
    pub times: TimeObservations,
}
/// Immutable attempt evidence. Explicit access is sensitive, not a logging API.
pub struct Snapshot {
    execution: FinalizedExecution,
    catalog: CatalogSnapshot,
    diagnostics: DiagnosticsSnapshot,
    projection_error: Option<AdaptCode>,
}
impl Snapshot {
    pub fn execution(&self) -> &FinalizedExecution {
        &self.execution
    }
    pub fn diagnostics(&self) -> &DiagnosticsSnapshot {
        &self.diagnostics
    }
    /// Exact admission/projection refusal recorded during this attempt, if any.
    /// Remains inspectable when cancellation is the primary attempt outcome.
    pub fn projection_error(&self) -> Option<AdaptCode> {
        self.projection_error
    }
    /// Reuses the original catalog allocation, with no cross-attempt composition.
    pub fn project(&self) -> Result<PairedProjection<'_>, AdaptCode> {
        let members: Vec<_> = self.execution.members().iter().collect();
        let membership = ProjectionMembership::validate(
            Selection::WindowsOnly,
            SelectedRefs {
                windows: Some(
                    self.execution.members()[0]
                        .scope()
                        .environment_ref()
                        .clone(),
                ),
                selected_wsl: None,
            },
            &members,
            &self.catalog,
        )?;
        project_with_companion(&membership)
    }
}
pub struct AttemptReport {
    identity: Option<(AttemptRef, FinalizationId)>,
    code: Code,
    snapshot: Option<Arc<Snapshot>>,
}
impl AttemptReport {
    /// Issued labels survive failures that have no accepted finalization.
    /// None means pre-cancel or issuance exhaustion, never an invented identity.
    pub fn identity(&self) -> Option<(&AttemptRef, &FinalizationId)> {
        self.identity.as_ref().map(|(a, z)| (a, z))
    }
    pub fn code(&self) -> Code {
        self.code
    }
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_deref()
    }
}
/// One plan/scope for this object's lifetime. Keeps at most two immutable snapshots,
/// not a history store. Fixture references are local and are never reset here.
/// WindowsOnly has no WSL port or launch API.
/// ```compile_fail,E0599
/// use abf_supervisor_fixture::Supervisor;
/// fn launch(s: &mut Supervisor) { s.start_selected_wsl(); }
/// ```
/// A catalog claim alone cannot construct a supervisor.
/// ```compile_fail,E0308
/// use abf_supervisor_fixture::Supervisor;
/// use abf_source_run::CatalogSnapshot;
/// fn bypass(catalog: CatalogSnapshot) { Supervisor::new(catalog); }
/// ```
pub struct Supervisor {
    plan: FixturePlan,
    scope: CatalogScope,
    issuer: FixtureRefIssuer,
    latest: Option<AttemptReport>,
    last_good: Option<Arc<Snapshot>>,
}
macro_rules! redact {
    ($($t:ty),*) => {$(impl std::fmt::Debug for $t {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(concat!(stringify!($t), "([redacted])"))
        }
    })*};
}
redact!(SyntheticFacts, Snapshot, AttemptReport, Supervisor);
impl Supervisor {
    pub fn new(plan: FixturePlan) -> Result<Self, Code> {
        let [decision] = plan.decisions() else {
            return Err(Code::SingleConditionalWindowsScopeRequired);
        };
        if decision.state() != DecisionState::Conditional {
            return Err(Code::SingleConditionalWindowsScopeRequired);
        }
        let scope = decision
            .conditional_scope()
            .filter(|scope| scope.environment_kind() == EnvironmentKind::Windows)
            .ok_or(Code::SingleConditionalWindowsScopeRequired)?
            .clone();
        let issuer = FixtureRefIssuer::new(plan.catalog());
        Ok(Self {
            plan,
            scope,
            issuer,
            latest: None,
            last_good: None,
        })
    }
    pub fn latest(&self) -> Option<&AttemptReport> {
        self.latest.as_ref()
    }
    pub fn last_good(&self) -> Option<&Snapshot> {
        self.last_good.as_deref()
    }
    /// Freshness is logical attempt state, never a claim about wall-clock age.
    pub fn availability(&self) -> Availability {
        if self
            .latest
            .as_ref()
            .is_some_and(|r| r.code == Code::Completed)
        {
            Availability::Fresh
        } else if self.last_good.is_some() {
            Availability::Stale
        } else {
            Availability::Unavailable
        }
    }
    /// S1 owns all script validation, parsing, replay and cancellation polling.
    /// `barrier_after` partitions its bounded emitted deliveries (not script indices).
    /// The suffix goes only to bound diagnostics; None puts all deliveries before
    /// the barrier. No actual process, drain, deadline or cancellation API is used.
    /// Pre-cancel performs no issuance/emission. Otherwise one explicit call is
    /// one attempt, even when it fails. There are no automatic retries.
    /// Emitted Cancel overrides the fictional process input with KilledCancel.
    /// After accepted finalization it remains primary over projection refusal;
    /// the snapshot retains that refusal in `Snapshot::projection_error`.
    pub fn run(
        &mut self,
        script: &[Step<'_>],
        barrier_after: Option<usize>,
        facts: SyntheticFacts,
        cancel_before_start: bool,
    ) -> &AttemptReport {
        let mut report = AttemptReport {
            identity: None,
            code: Code::Cancelled,
            snapshot: None,
        };
        if !cancel_before_start {
            let issued = self.issuer.begin_execution().and_then(|attempt| {
                self.issuer
                    .issue_finalization()
                    .map(|finalization| (attempt, finalization))
            });
            match issued {
                Err(code) => report.code = Code::Identity(code),
                Ok((attempt, finalization)) => {
                    match self.execute(&attempt, &finalization, script, barrier_after, facts) {
                        Ok((code, snapshot)) => {
                            report.code = code;
                            report.snapshot = Some(snapshot);
                        }
                        Err(code) => report.code = code,
                    }
                    report.identity = Some((attempt, finalization));
                }
            }
        }
        if report.code == Code::Completed {
            self.last_good = report.snapshot.clone();
        }
        self.latest = Some(report);
        self.latest.as_ref().expect("just assigned")
    }
    fn execute(
        &self,
        attempt: &AttemptRef,
        finalization: &FinalizationId,
        script: &[Step<'_>],
        barrier_after: Option<usize>,
        facts: SyntheticFacts,
    ) -> Result<(Code, Arc<Snapshot>), Code> {
        let emission =
            emit(attempt, self.plan.catalog(), &self.scope, script).map_err(Code::Reader)?;
        let cancelled = emission.cancelled();
        let mut frames = emission.into_frames();
        let boundary = barrier_after.unwrap_or(frames.len());
        if boundary > frames.len() {
            return Err(Code::BarrierIndex);
        }
        let late = frames.split_off(boundary);
        let arrival = facts.times.barrier();
        let process = if cancelled {
            ProcessEnd::KilledCancel
        } else {
            facts.process
        };
        let execution = finalize(
            RawExecutionLog {
                manifest: ExecutionManifest {
                    attempt: attempt.clone(),
                    finalization: finalization.clone(),
                    members: vec![self.scope.clone()],
                    retry_of: None,
                    subset: RetrySubset::AllExpected,
                },
                pre_barrier: frames,
                process,
                barrier: facts.barrier,
                // Windows-only fictional input, no distribution identity claim.
                distribution_matches: true,
                times: facts.times,
            },
            self.plan.catalog(),
        )
        .map_err(Code::Finalize)?;
        let mut log = PostFinalizationLog::bind(&execution);
        for frame in late {
            log.append(frame, arrival).map_err(Code::Diagnostics)?;
        }
        let diagnostics = log.snapshot();
        let mut snapshot = Snapshot {
            execution,
            catalog: self.plan.catalog().clone(),
            diagnostics,
            projection_error: None,
        };
        // Refuse the whole projection, never skip an unprojectable record/source.
        snapshot.projection_error = snapshot.project().err();
        if let Some(error) = snapshot.projection_error {
            let code = if cancelled {
                Code::Cancelled
            } else {
                Code::Adapt(error)
            };
            return Ok((code, Arc::new(snapshot)));
        }
        let verdict = snapshot.execution.members()[0].verdict();
        let assessment = assess(&snapshot.execution.members()[0], &snapshot.diagnostics)
            .expect("diagnostics were bound to this execution");
        let reliable = matches!(
            verdict,
            CoverageVerdict::CoveredWithinEnumeration { .. }
                | CoverageVerdict::NoSourcesFound(NoSourcesReason::EmptyEnumeration)
        ) && process == ProcessEnd::ExitedNormally
            && snapshot.execution.members()[0].run_end_frame()
            && assessment.ceiling == PresentationCeiling::Original;
        let code = if cancelled {
            Code::Cancelled
        } else if reliable {
            Code::Completed
        } else {
            Code::Degraded(verdict)
        };
        Ok((code, Arc::new(snapshot)))
    }
}
