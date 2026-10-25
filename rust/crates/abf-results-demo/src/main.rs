//! ABF127 - deterministic presentation of fictional S4/S5 results through ABF126.
#![forbid(unsafe_code)]

use abf_catalog_plan_fixture::*;
use abf_collection_boundary::identity::{EnvironmentIdentity, OpaqueId, WslDistroName};
use abf_reader_fixture::{Action, Event, Step};
use abf_results_fixture::{Inputs, Results, WindowsSnapshot, present};
use abf_selected_wsl_fixture::{Delivery, Request, Transport};
use abf_source_run::*;
use abf_supervisor_fixture::{Supervisor, SyntheticFacts};

fn windows() -> Supervisor {
    let plan = plan_windows_fixture(
        &WindowsRequest {
            namespace: "fictionalWindows",
            families: &["demo"],
        },
        &[FixtureDescriptor {
            id: "descriptor",
            schema: 1,
            revision: "r1",
            scope: CatalogScope::new(
                EnvironmentKind::Windows,
                "fictionalWindows",
                "demo",
                "fixture-v1",
            )
            .unwrap(),
            rule: "rule",
            scope_tag: "windows",
            approved_classes: &[RootClass::Configured],
            overlaps: None,
        }],
        &[FictionalCandidate {
            id: "candidate",
            family: "demo",
            namespace: "fictionalWindows",
            revision: "r1",
            class: RootClass::Configured,
            locator: r"C:\Fictional-Demo\Unused",
            recognition: Some(&[RecognitionClaim {
                descriptor: "descriptor",
                descriptor_revision: "r1",
                rule: "rule",
            }]),
        }],
        &[FictionalGrant {
            candidate: "candidate",
            family: "demo",
            namespace: "fictionalWindows",
            candidate_revision: "r1",
            class: RootClass::Configured,
            descriptor: "descriptor",
            descriptor_revision: "r1",
        }],
    )
    .expect("fixed fictional Windows plan");
    Supervisor::new(plan).expect("fixed single Windows scope")
}

fn facts() -> SyntheticFacts {
    SyntheticFacts {
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        times: TimeObservations::new(11, 19),
    }
}

fn script<'a>(source: &'a SourceRef, sources: &'a [SourceRef]) -> Vec<Step<'a>> {
    let events = [
        Event::Enumeration {
            outcome: RawEnumeration::Closed(1),
            sources,
        },
        Event::Source {
            source,
            source_seq: 1,
            action: Action::OpenAttempt,
        },
        Event::Source {
            source,
            source_seq: 1,
            action: Action::OpenSuccess,
        },
        Event::Record {
            source,
            source_seq: 1,
            bytes: br#"{"tokens":{"input":37},"private":"PRIVATE-DEMO-SENTINEL"}"#,
        },
        Event::Source {
            source,
            source_seq: 2,
            action: Action::End(ChangeObservation::NoChangeObserved),
        },
        Event::RunEnd,
    ];
    events
        .into_iter()
        .enumerate()
        .map(|(i, event)| Step {
            frame_seq: i as u64 + 1,
            event,
        })
        .collect()
}

fn request<'a>(
    env: &'a EnvironmentIdentity,
    script: &'a [Step<'a>],
    delivery: Delivery,
) -> Request<'a> {
    Request {
        target: env,
        responding_environment: env,
        script,
        delivery,
        process: ProcessEnd::ExitedNormally,
        barrier: Barrier::Drained,
        times: TimeObservations::new(101, 109),
    }
}

// Only explicit safe fields are formatted. Never Debug an execution or parser payload.
fn evidence(execution: &FinalizedExecution) {
    for member in execution.members() {
        println!(
            "    environment={} coverage={:?} qualifiers={:?}",
            member.scope().environment_ref().as_str(),
            member.verdict(),
            member.qualifiers()
        );
        println!(
            "    process={:?} run_end={} provenance={:?}",
            member.process(),
            member.run_end_frame(),
            member.attestation()
        );
    }
}

fn projection<E: std::fmt::Debug>(value: &Result<PairedProjection<'_>, E>) {
    match value {
        Ok(paired) => println!(
            "    projection=admitted selection={:?}; admission is not collection success",
            paired.selection()
        ),
        Err(code) => println!("    projection_error={code:?}"),
    }
}

fn snapshot(label: &str, value: Option<&WindowsSnapshot<'_>>) {
    match value {
        Some(view) => {
            println!("  {label}: retained separately");
            evidence(view.evidence().execution());
            projection(view.projection());
            match view.assessment() {
                Ok(a) => println!(
                    "    assessment={:?} ceiling={:?} post={:?}",
                    a.verdict, a.ceiling, a.post
                ),
                Err(code) => println!("    assessment_error={code:?}"),
            }
        }
        None => println!("  {label}: absent; no snapshot"),
    }
}

fn display(title: &str, results: &Results<'_>) {
    println!("\n== {title} ==\nselection={:?}", results.selection());
    match results.windows() {
        Some(view) => {
            println!(
                "Windows availability={:?} freshness={:?}",
                view.source().availability(),
                view.freshness()
            );
            match view.source().latest() {
                Some(report) => println!("  latest_code={:?}", report.code()),
                None => println!("  latest_report: absent; no attempt issued"),
            }
            snapshot("latest", view.latest());
            snapshot("last_good", view.last_good());
        }
        None => println!("Windows: not selected"),
    }
    match results.selected_wsl() {
        Some(view) => {
            println!(
                "SelectedWSL environment={} distro={} freshness={:?}",
                view.selected().id().as_str(),
                view.selected()
                    .wsl_distro()
                    .expect("validated WSL selection")
                    .as_str(),
                view.freshness()
            );
            match view.response() {
                Ok(response) => {
                    println!(
                        "  delivery={:?} cancelled={}",
                        response.delivery(),
                        response.cancelled()
                    );
                    match response.execution() {
                        Ok(execution) => evidence(execution),
                        Err(code) => println!("  execution_error={code:?}"),
                    }
                }
                Err(code) => println!("  response_error={code:?}; no response or projection"),
            }
            if let Some(value) = view.projection() {
                projection(value);
            }
        }
        None => println!("SelectedWSL: not selected; no WSL input required"),
    }
}

fn main() {
    println!("ABF127 - fictional in-memory demonstration");
    println!("Synthetic only; provenance=Unattested.");
    println!(
        "Coverage is limited to fictional enumeration; freshness is logical, not measured age."
    );
    println!("No combined total, shared catalog authority or overall collection verdict.");

    // WindowsOnly is presented before any WSL identity or transport is constructed.
    let mut supervisor = windows();
    let source = SourceRef::new("windows-i1").unwrap();
    let sources = [source.clone()];
    let windows_script = script(&source, &sources);
    supervisor.run(&windows_script, None, facts(), false);
    display(
        "WindowsOnly",
        &present(Inputs::WindowsOnly(&supervisor)).unwrap(),
    );

    let env = EnvironmentIdentity::wsl(
        OpaqueId::new("fictionalWsl").unwrap(),
        WslDistroName::new("Fictional-Ubuntu-Demo").unwrap(),
    );
    let selected = std::slice::from_ref(&env);
    let scope = CatalogScope::new(
        EnvironmentKind::SelectedWsl,
        env.id().as_str(),
        "demo",
        "fixture-v1",
    )
    .unwrap();
    let mut catalog = CatalogBuilder::new();
    catalog.add(scope.clone(), "wsl").unwrap();
    let mut transport = Transport::new(selected, catalog.build(), scope).unwrap();
    let wsl_source = SourceRef::new("wsl-i1").unwrap();
    let wsl_sources = [wsl_source.clone()];
    let wsl_script = script(&wsl_source, &wsl_sources);
    let interrupted = transport
        .exchange(
            Selection::SelectedWslOnly,
            request(&env, &wsl_script, Delivery::Incomplete { after: 4 }),
            false,
        )
        .unwrap();
    display(
        "SelectedWSL interrupted",
        &present(Inputs::SelectedWsl {
            selected,
            response: Ok(&interrupted),
        })
        .unwrap(),
    );

    let mut cancelled = windows_script[..4].to_vec();
    cancelled.push(Step {
        frame_seq: 5,
        event: Event::Cancel,
    });
    supervisor.run(&cancelled, None, facts(), false);
    let complete = transport
        .exchange(
            Selection::Both,
            request(&env, &wsl_script, Delivery::Complete),
            false,
        )
        .unwrap();
    display(
        "Both independent",
        &present(Inputs::Both {
            windows: &supervisor,
            selected,
            response: Ok(&complete),
        })
        .unwrap(),
    );

    let unavailable = windows();
    let refused = transport.exchange(
        Selection::Both,
        request(&env, &wsl_script, Delivery::Complete),
        true,
    );
    display(
        "Both unavailable",
        &present(Inputs::Both {
            windows: &unavailable,
            selected,
            response: refused.as_ref().map_err(|e| *e),
        })
        .unwrap(),
    );
}
