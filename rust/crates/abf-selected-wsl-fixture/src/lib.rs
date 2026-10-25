//! ABF125 - B1/S5 synthetic SelectedWSL transport fixture.
//! In-memory claims only. No discovery, launcher, native authority or I/O port.
#![forbid(unsafe_code)]

use abf_collection_boundary::identity::EnvironmentIdentity;
use abf_reader_fixture::{Step, emit};
use abf_source_run::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    SingleSelectedDistributionRequired,
    ScopeBinding,
    WindowsOnly,
    CancelledBeforeCall,
    DistributionMismatch,
    DeliveryIndex,
    Identity(RefCode),
    Reader(abf_reader_fixture::Error),
    Finalize(FinalizeCode),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// All S1-emitted deliveries, not a claim of complete collection.
    Complete,
    /// Explicit fictional transport failure after an emitted-frame prefix.
    Failed { after: usize },
    /// Delivery ended without confirmation; never an empty-success substitute.
    Incomplete { after: usize },
    /// Explicit fictional cancellation after an emitted-frame prefix.
    Cancelled { after: usize },
}
/// Explicit caller-owned fictional request and response facts, never a wire schema.
pub struct Request<'a> {
    pub target: &'a EnvironmentIdentity,
    pub responding_environment: &'a EnvironmentIdentity,
    pub script: &'a [Step<'a>],
    pub delivery: Delivery,
    pub process: ProcessEnd,
    pub barrier: Barrier,
    pub times: TimeObservations,
}
/// Issued identities survive emission/finalization refusal. No synthetic zero.
pub struct Response {
    environment: EnvironmentIdentity,
    attempt: AttemptRef,
    finalization: FinalizationId,
    delivery: Delivery,
    cancelled: bool,
    catalog: CatalogSnapshot,
    result: Result<FinalizedExecution, Error>,
}
impl Response {
    pub fn environment(&self) -> &EnvironmentIdentity {
        &self.environment
    }
    pub fn identity(&self) -> (&AttemptRef, &FinalizationId) {
        (&self.attempt, &self.finalization)
    }
    pub fn delivery(&self) -> Delivery {
        self.delivery
    }
    /// Accepted fictional cancellation: S1 Cancel, validated delivery cancellation,
    /// or effective KilledCancel. False on reader refusal; retained on finalizer
    /// refusal. This indicator never certifies native termination or completion.
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn execution(&self) -> Result<&FinalizedExecution, Error> {
        self.result.as_ref().map_err(|e| *e)
    }
    /// This is the SelectedWSL leg only, including when exchange selected Both.
    /// G4 owns admission and projection; no cross-environment composition here.
    pub fn project(&self) -> Result<PairedProjection<'_>, ProjectionError> {
        let execution = self.execution().map_err(ProjectionError::Execution)?;
        let members: Vec<_> = execution.members().iter().collect();
        let membership = ProjectionMembership::validate(
            Selection::SelectedWslOnly,
            SelectedRefs {
                windows: None,
                selected_wsl: Some(execution.members()[0].scope().environment_ref().clone()),
            },
            &members,
            &self.catalog,
        )
        .map_err(ProjectionError::Admission)?;
        project_with_companion(&membership).map_err(ProjectionError::Admission)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    Execution(Error),
    Admission(AdaptCode),
}
/// One explicitly selected fictional distribution and one scope for this lifetime.
/// Holds no response history; callers own returned immutable evidence.
pub struct Transport {
    selected: EnvironmentIdentity,
    catalog: CatalogSnapshot,
    scope: CatalogScope,
    issuer: FixtureRefIssuer,
    calls: u32,
}
macro_rules! redact {
    ($($t:ty),*) => {$(impl std::fmt::Debug for $t {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(concat!(stringify!($t), "([redacted])"))
        }
    })*};
}
redact!(Request<'_>, Response, Transport);
impl Transport {
    pub fn new(
        selected: &[EnvironmentIdentity],
        catalog: CatalogSnapshot,
        scope: CatalogScope,
    ) -> Result<Self, Error> {
        let [selected] = selected else {
            return Err(Error::SingleSelectedDistributionRequired);
        };
        if selected.wsl_distro().is_none() {
            return Err(Error::SingleSelectedDistributionRequired);
        }
        if scope.environment_kind() != EnvironmentKind::SelectedWsl
            || scope.environment_ref().as_str() != selected.id().as_str()
            || catalog.scope_tag(&scope).is_none()
        {
            return Err(Error::ScopeBinding);
        }
        let issuer = FixtureRefIssuer::new(&catalog);
        Ok(Self {
            selected: selected.clone(),
            catalog,
            scope,
            issuer,
            calls: 0,
        })
    }
    /// Counts explicit in-memory exchanges only, never OS operations.
    pub fn calls(&self) -> u32 {
        self.calls
    }
    /// WindowsOnly and pre-cancel gate before identity issuance or S1 emission.
    /// No callback, discovery, implicit retry or distribution fallback exists.
    pub fn exchange(
        &mut self,
        selection: Selection,
        request: Request<'_>,
        cancel_before_call: bool,
    ) -> Result<Response, Error> {
        if selection == Selection::WindowsOnly {
            return Err(Error::WindowsOnly);
        }
        if cancel_before_call {
            return Err(Error::CancelledBeforeCall);
        }
        if request.target != &self.selected || request.responding_environment != &self.selected {
            return Err(Error::DistributionMismatch);
        }
        let attempt = self.issuer.begin_execution().map_err(Error::Identity)?;
        let finalization = self.issuer.issue_finalization().map_err(Error::Identity)?;
        // Issuer bounds this counter to MAX_EXECUTIONS_PER_ISSUER.
        self.calls += 1;
        let mut response = Response {
            environment: self.selected.clone(),
            attempt,
            finalization,
            delivery: request.delivery,
            cancelled: false,
            catalog: self.catalog.clone(),
            result: Err(Error::Reader(abf_reader_fixture::Error::InvalidScript)),
        };
        response.result = (|| {
            let emission = emit(
                &response.attempt,
                &self.catalog,
                &self.scope,
                request.script,
            )
            .map_err(Error::Reader)?;
            response.cancelled = emission.cancelled();
            let mut frames = emission.into_frames();
            let process = match request.delivery {
                Delivery::Complete => request.process,
                Delivery::Failed { after }
                | Delivery::Incomplete { after }
                | Delivery::Cancelled { after } => {
                    if after > frames.len() {
                        return Err(Error::DeliveryIndex);
                    }
                    frames.truncate(after);
                    response.cancelled |= matches!(request.delivery, Delivery::Cancelled { .. });
                    ProcessEnd::TransportLost
                }
            };
            response.cancelled |= process == ProcessEnd::KilledCancel;
            finalize(
                RawExecutionLog {
                    manifest: ExecutionManifest {
                        attempt: response.attempt.clone(),
                        finalization: response.finalization.clone(),
                        members: vec![self.scope.clone()],
                        retry_of: None,
                        subset: RetrySubset::AllExpected,
                    },
                    pre_barrier: frames,
                    process: if response.cancelled {
                        ProcessEnd::KilledCancel
                    } else {
                        process
                    },
                    barrier: request.barrier,
                    // Equality of explicit fictional identities was checked above.
                    distribution_matches: true,
                    times: request.times,
                },
                &self.catalog,
            )
            .map_err(Error::Finalize)
        })();
        Ok(response)
    }
}
