//! ABF126 - read-only presentation of independently obtained synthetic evidence.
//! No collection, I/O, clocks, persistence, shared catalog or combined total.
#![forbid(unsafe_code)]

use abf_collection_boundary::identity::EnvironmentIdentity;
use abf_selected_wsl_fixture::{Error as WslError, ProjectionError, Response};
use abf_source_run::*;
use abf_supervisor_fixture::{Availability, Snapshot, Supervisor};

/// Logical S4 state only. S5 does not establish freshness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Stale,
    Unavailable,
    Unestablished,
}

pub enum Inputs<'a> {
    WindowsOnly(&'a Supervisor),
    SelectedWsl {
        selected: &'a [EnvironmentIdentity],
        response: Result<&'a Response, WslError>,
    },
    Both {
        windows: &'a Supervisor,
        selected: &'a [EnvironmentIdentity],
        response: Result<&'a Response, WslError>,
    },
}

/// Original S4 snapshot plus independent projection and late-evidence assessment.
/// Projection refusal never hides the execution, diagnostics or assessment.
/// External callers cannot pair evidence with an unrelated projection/assessment.
/// ```compile_fail,E0451
/// use abf_results_fixture::WindowsSnapshot;
/// use abf_source_run::{PairedProjection, AdaptCode, CurrentAssessment, AssessCode};
/// use abf_supervisor_fixture::Snapshot;
/// fn forge<'a>(evidence: &'a Snapshot, projection: Result<PairedProjection<'a>, AdaptCode>,
///              assessment: Result<CurrentAssessment, AssessCode>) {
///     let _ = WindowsSnapshot { evidence, projection, assessment };
/// }
/// ```
pub struct WindowsSnapshot<'a> {
    evidence: &'a Snapshot,
    projection: Result<PairedProjection<'a>, AdaptCode>,
    assessment: Result<CurrentAssessment, AssessCode>,
}
impl<'a> WindowsSnapshot<'a> {
    pub fn evidence(&self) -> &'a Snapshot {
        self.evidence
    }
    pub fn projection(&self) -> &Result<PairedProjection<'a>, AdaptCode> {
        &self.projection
    }
    pub fn assessment(&self) -> &Result<CurrentAssessment, AssessCode> {
        &self.assessment
    }
}

/// Only presentation can bind a supervisor to its snapshot views.
/// ```compile_fail,E0451
/// use abf_results_fixture::{WindowsView, WindowsSnapshot};
/// use abf_supervisor_fixture::Supervisor;
/// fn forge<'a>(source: &'a Supervisor, latest: Option<WindowsSnapshot<'a>>,
///              last_good: Option<WindowsSnapshot<'a>>) {
///     let _ = WindowsView { source, latest, last_good };
/// }
/// ```
pub struct WindowsView<'a> {
    /// Latest report (including failures without snapshots) remains authoritative.
    source: &'a Supervisor,
    latest: Option<WindowsSnapshot<'a>>,
    last_good: Option<WindowsSnapshot<'a>>,
}
impl<'a> WindowsView<'a> {
    pub fn source(&self) -> &'a Supervisor {
        self.source
    }
    pub fn latest(&self) -> Option<&WindowsSnapshot<'a>> {
        self.latest.as_ref()
    }
    pub fn last_good(&self) -> Option<&WindowsSnapshot<'a>> {
        self.last_good.as_ref()
    }
    pub fn freshness(&self) -> Freshness {
        match self.source.availability() {
            Availability::Fresh => Freshness::Fresh,
            Availability::Stale => Freshness::Stale,
            Availability::Unavailable => Freshness::Unavailable,
        }
    }
}

pub struct Results<'a> {
    selection: Selection,
    windows: Option<WindowsView<'a>>,
    selected_wsl: Option<WslView<'a>>,
}
impl<'a> Results<'a> {
    pub fn windows(&self) -> Option<&WindowsView<'a>> {
        self.windows.as_ref()
    }
    pub fn selected_wsl(&self) -> Option<&WslView<'a>> {
        self.selected_wsl.as_ref()
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ExplicitSelectedWslRequired,
    EnvironmentMismatch,
    /// A WindowsOnly refusal contradicts either WSL-enabled input selection.
    SelectionMismatch,
}

/// No S5 history, supersession, late-diagnostics or freshness claim is added.
/// A pre-call refusal has no response, projection, attempt or timestamp.
/// ```compile_fail,E0451
/// use abf_results_fixture::WslView;
/// use abf_collection_boundary::identity::EnvironmentIdentity;
/// use abf_selected_wsl_fixture::{Response, Error, ProjectionError};
/// use abf_source_run::PairedProjection;
/// fn forge<'a>(selected: &'a EnvironmentIdentity, response: Result<&'a Response, Error>,
///              projection: Option<Result<PairedProjection<'a>, ProjectionError>>) {
///     let _ = WslView { selected, response, projection };
/// }
/// ```
pub struct WslView<'a> {
    selected: &'a EnvironmentIdentity,
    response: Result<&'a Response, WslError>,
    projection: Option<Result<PairedProjection<'a>, ProjectionError>>,
}
impl<'a> WslView<'a> {
    pub fn selected(&self) -> &'a EnvironmentIdentity {
        self.selected
    }
    pub fn response(&self) -> Result<&'a Response, WslError> {
        self.response
    }
    pub fn projection(&self) -> Option<&Result<PairedProjection<'a>, ProjectionError>> {
        self.projection.as_ref()
    }
    pub fn freshness(&self) -> Freshness {
        Freshness::Unestablished
    }
}

fn selected_wsl<'a>(
    selected: &'a [EnvironmentIdentity],
    response: Result<&'a Response, WslError>,
) -> Result<WslView<'a>, Error> {
    let [selected] = selected else {
        return Err(Error::ExplicitSelectedWslRequired);
    };
    if selected.wsl_distro().is_none() {
        return Err(Error::ExplicitSelectedWslRequired);
    }
    if response.is_ok_and(|r| r.environment() != selected) {
        return Err(Error::EnvironmentMismatch);
    }
    if matches!(response, Err(WslError::WindowsOnly)) {
        return Err(Error::SelectionMismatch);
    }
    Ok(WslView {
        selected,
        response,
        projection: response.ok().map(Response::project),
    })
}

fn windows(source: &Supervisor) -> WindowsView<'_> {
    fn snapshot(evidence: &Snapshot) -> WindowsSnapshot<'_> {
        WindowsSnapshot {
            evidence,
            projection: evidence.project(),
            assessment: assess(&evidence.execution().members()[0], evidence.diagnostics()),
        }
    }
    WindowsView {
        source,
        latest: source.latest().and_then(|r| r.snapshot()).map(snapshot),
        last_good: source.last_good().map(snapshot),
    }
}

pub fn present(input: Inputs<'_>) -> Result<Results<'_>, Error> {
    match input {
        Inputs::WindowsOnly(source) => Ok(Results {
            selection: Selection::WindowsOnly,
            windows: Some(windows(source)),
            selected_wsl: None,
        }),
        Inputs::SelectedWsl { selected, response } => Ok(Results {
            selection: Selection::SelectedWslOnly,
            windows: None,
            selected_wsl: Some(selected_wsl(selected, response)?),
        }),
        Inputs::Both {
            windows: source,
            selected,
            response,
        } => {
            let wsl = selected_wsl(selected, response)?;
            Ok(Results {
                selection: Selection::Both,
                windows: Some(windows(source)),
                selected_wsl: Some(wsl),
            })
        }
    }
}

macro_rules! redact {
    ($($name:ident),+ $(,)?) => {$(
        impl std::fmt::Debug for $name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($name), "([redacted])"))
            }
        }
    )+};
}
redact!(Inputs, Results, WindowsView, WindowsSnapshot, WslView);
