//! Fixed fictional Claude usage demonstration (ABF134F). **Not real data.**
//!
//! Runs the real streaming line pipeline, parser and summary over two checked-in fictional
//! JSONL files (`fixtures/fictional-claude-demo/`, embedded at compile time) and serializes
//! the result as a sanitized payload for the application's explicit fictional view. Nothing
//! here reads the filesystem, environment or a clock at run time, takes a path, or
//! discovers a provider root. Real-data collection stays in
//! [`crate::claude_jsonl::collect_claude_jsonl_root`] and has no payload here: a payload for
//! real data needs its own provenance and approval.
//!
//! # Payload (schema version 1)
//!
//! One JSON object of integers, booleans and fixed enum strings only. Token sums and other
//! `u64` quantities are decimal strings (`amount`) so that no consumer loses precision; small
//! counters are numbers. It carries no identifier, path, name, timestamp, model, content or
//! caller label: environment is reduced to its kind and every value derives from
//! [`ClaudeSummary`]. `provenance` is always `"fictional-fixture"`, which is why the
//! serializer is private and only the fixed fixture can reach it.

use crate::claude_jsonl::{ClaudeReadLimits, collect_claude_jsonl_memory};
use crate::claude_summary::{
    ClaimStates, ClaudeSummary, ComponentSummary, ComponentValue, IndeterminateReason,
};
use crate::identity::{
    EnvironmentIdentity, EnvironmentKind, Harness, OpaqueId, SourceInstance, WslDistroName,
};
use crate::observation::{CaptureTime, Completeness, ScanRun};
use crate::scope::HostMatch;

pub const PAYLOAD_SCHEMA_VERSION: u8 = 1;
pub const PAYLOAD_PROVENANCE: &str = "fictional-fixture";

const FILE_A: &[u8] = include_bytes!("../fixtures/fictional-claude-demo/a.jsonl");
const FILE_B: &[u8] = include_bytes!("../fixtures/fictional-claude-demo/b.jsonl");

/// Summary of the fixed fictional fixture, built by the real collector pipeline.
pub fn fictional_demo_summary() -> ClaudeSummary {
    let label = |value: &str| OpaqueId::new(value).expect("fixed valid demo label");
    let environment = EnvironmentIdentity::wsl(
        label("demo-env"),
        WslDistroName::new("Fictional-Demo").expect("fixed valid demo distro"),
    );
    let source = SourceInstance::new(label("demo-source"), Harness::ClaudeCode);
    let run = ScanRun::new(label("demo-run"), CaptureTime::NotRecorded);
    let limits = ClaudeReadLimits {
        // Small enough that the fixture's long line is reported as oversized.
        max_line_bytes: 4096,
        ..ClaudeReadLimits::default()
    };
    collect_claude_jsonl_memory(environment, source, &run, &[FILE_A, FILE_B], limits)
        .expect("fixed fictional demo input is within its limits")
        .summary()
}

/// The sanitized payload of [`fictional_demo_summary`].
pub fn fictional_demo_payload_json() -> String {
    payload_json(&fictional_demo_summary())
}

fn object(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("\"{key}\":{value}"))
        .collect();
    format!("{{{}}}", body.join(","))
}
fn text(value: &str) -> String {
    format!("\"{value}\"") // only fixed enum strings are ever passed
}
fn number(value: u64) -> String {
    value.to_string()
}
fn flag(value: bool) -> String {
    value.to_string()
}

fn states(states: &ClaimStates) -> String {
    object(&[
        ("valid", number(states.valid)),
        ("validZero", number(states.valid_zero)),
        ("absent", number(states.absent)),
        ("null", number(states.null)),
        ("wrongType", number(states.wrong_type)),
        ("outOfRange", number(states.out_of_range)),
        ("unsupported", number(states.unsupported)),
    ])
}

fn component(component: &ComponentSummary) -> String {
    let value = match component.value {
        ComponentValue::ReportedClaimSum(sum) => {
            object(&[("kind", text("sum")), ("amount", text(&sum.to_string()))])
        }
        ComponentValue::NoValidClaims => object(&[("kind", text("no-valid-claims"))]),
        ComponentValue::Indeterminate(IndeterminateReason::NoObservations) => object(&[
            ("kind", text("indeterminate")),
            ("reason", text("no-observations")),
        ]),
        ComponentValue::Overflow => object(&[("kind", text("overflow"))]),
    };
    object(&[("value", value), ("states", states(&component.states))])
}

/// Serializes a summary as the version 1 payload and stamps it `fictional-fixture`.
///
/// Deliberately **private**: the stamp is only true for the fixed fixture, so the sole public
/// route to a payload is [`fictional_demo_payload_json`], which feeds it
/// [`fictional_demo_summary`]. Do not make this `pub`; a summary of real data must get its own
/// serializer with its own provenance. A source guard in `tests/claude_demo.rs` checks this.
fn payload_json(summary: &ClaudeSummary) -> String {
    let rejected = &summary.lines.rejected;
    let components = &summary.components;
    object(&[
        ("schemaVersion", number(u64::from(PAYLOAD_SCHEMA_VERSION))),
        ("provenance", text(PAYLOAD_PROVENANCE)),
        ("basis", text("observed-record-claims")),
        ("duplicates", text("unresolved")),
        (
            "environmentKind",
            text(match summary.environment.kind() {
                EnvironmentKind::WindowsNative => "windows",
                EnvironmentKind::Wsl2 => "wsl",
            }),
        ),
        (
            "hostMatch",
            text(match summary.host_match {
                HostMatch::Native => "native",
                HostMatch::Simulated => "simulated",
            }),
        ),
        (
            "completeness",
            text(match summary.completeness {
                Completeness::Complete => "complete",
                Completeness::Incomplete => "incomplete",
            }),
        ),
        (
            "components",
            object(&[
                ("input", component(&components.input)),
                ("output", component(&components.output)),
                ("cacheCreation", component(&components.cache_creation)),
                ("cacheRead", component(&components.cache_read)),
            ]),
        ),
        (
            "records",
            object(&[
                ("observations", number(summary.records.observations)),
                ("accepted", number(summary.records.accepted)),
                ("qualified", number(summary.records.qualified)),
                ("incomplete", number(summary.records.incomplete)),
                ("withValidClaim", number(summary.records.with_valid_claim)),
                (
                    "withoutValidClaim",
                    number(summary.records.without_valid_claim),
                ),
                (
                    "withInvalidClaim",
                    number(summary.records.with_invalid_claim),
                ),
                ("withNullClaim", number(summary.records.with_null_claim)),
                (
                    "withMissingClaim",
                    number(summary.records.with_missing_claim),
                ),
                (
                    "cacheCreationFlatIgnored",
                    number(summary.records.cache_creation_flat_ignored),
                ),
            ]),
        ),
        (
            "lines",
            object(&[
                ("terminatedLines", number(summary.lines.terminated_lines)),
                (
                    "rejected",
                    object(&[
                        ("total", number(rejected.total())),
                        ("frameBudget", number(rejected.frame_budget)),
                        ("stringBudget", number(rejected.string_budget)),
                        ("depthBudget", number(rejected.depth_budget)),
                        ("entryBudget", number(rejected.entry_budget)),
                        ("resultBudget", number(rejected.result_budget)),
                        ("invalidLimits", number(rejected.invalid_limits)),
                        ("invalidUtf8", number(rejected.invalid_utf8)),
                        ("incompleteJson", number(rejected.incomplete_json)),
                        ("malformedJson", number(rejected.malformed_json)),
                        ("trailingData", number(rejected.trailing_data)),
                        ("rootShape", number(rejected.root_shape)),
                        ("duplicateKey", number(rejected.duplicate_key)),
                    ]),
                ),
                ("oversized", number(summary.lines.oversized)),
                ("blank", number(summary.lines.blank)),
                (
                    "filesWithPendingTail",
                    number(summary.lines.files_with_pending_tail),
                ),
                ("pendingTailBytes", number(summary.lines.pending_tail_bytes)),
            ]),
        ),
        (
            "files",
            object(&[
                ("read", number(summary.files.read)),
                ("rejected", number(summary.files.rejected)),
                ("path", number(summary.files.path)),
                ("tooLarge", number(summary.files.too_large)),
                ("byteBudget", number(summary.files.byte_budget)),
                ("outputLimit", number(summary.files.output_limit)),
                (
                    "changedDuringRead",
                    number(summary.files.changed_during_read),
                ),
                ("io", number(summary.files.io)),
            ]),
        ),
        (
            "skipped",
            object(&[
                ("total", number(summary.skipped.total)),
                ("mayHideData", number(summary.skipped.may_hide_data)),
                ("outOfScope", number(summary.skipped.out_of_scope)),
            ]),
        ),
        (
            "caveats",
            object(&[
                ("incompleteBatch", flag(summary.caveats.incomplete_batch)),
                ("unparsedLines", flag(summary.caveats.unparsed_lines)),
                ("pendingTails", flag(summary.caveats.pending_tails)),
                ("rejectedFiles", flag(summary.caveats.rejected_files)),
                ("skippedInScope", flag(summary.caveats.skipped_in_scope)),
            ]),
        ),
    ])
}

#[cfg(test)]
mod tests;
