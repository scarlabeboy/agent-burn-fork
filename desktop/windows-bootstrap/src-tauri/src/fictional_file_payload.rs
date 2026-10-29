// Bootstrap-local numerical projection shared by the two separate diagnostic contracts.
// Kept private so the collection boundary's fictional-only serializer stays closed.
use abf_collection_boundary::*;
const PAYLOAD_SCHEMA_VERSION: u8 = 1;
const PAYLOAD_PROVENANCE: &str = "fictional-fixture";

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

// Each entry point fixes its own provenance contract; no source text is serialized.
pub(super) fn fictional_payload_json(summary: &ClaudeSummary) -> String {
    payload_json(summary, false)
}

pub(super) fn copy_payload_json(summary: &ClaudeSummary) -> String {
    payload_json(summary, true)
}

fn payload_json(summary: &ClaudeSummary, copy: bool) -> String {
    let rejected = &summary.lines.rejected;
    let components = &summary.components;
    let mut fields = vec![
        ("schemaVersion", number(u64::from(PAYLOAD_SCHEMA_VERSION))),
        (
            "provenance",
            text(if copy {
                "configured-windows-copy"
            } else {
                PAYLOAD_PROVENANCE
            }),
        ),
        ("basis", text("observed-record-claims")),
        ("duplicates", text("unresolved")),
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
    ];
    if copy {
        fields.push(("originalSource", text("not-established")));
        fields.push(("originalEnvironment", text("not-established")));
    } else {
        // Preserve the fictional schema's original wire ordering.
        fields.insert(
            4,
            (
                "environmentKind",
                text(match summary.environment.kind() {
                    EnvironmentKind::WindowsNative => "windows",
                    EnvironmentKind::Wsl2 => "wsl",
                }),
            ),
        );
        fields.insert(
            5,
            (
                "hostMatch",
                text(match summary.host_match {
                    HostMatch::Native => "native",
                    HostMatch::Simulated => "simulated",
                }),
            ),
        );
    }
    object(&fields)
}

// ABF141 diagnostic-local envelope. Only fixed enums and numerical claims cross IPC.
pub(super) fn copy_replay_payload_json(summary: &claude_replay::ClaudeReplaySummary) -> String {
    object(&[
        ("schemaVersion", number(1)),
        ("raw", copy_payload_json(&summary.raw)),
        ("replay", replay_json(&summary.evaluation)),
    ])
}

pub(super) fn replay_json(evaluation: &claude_replay::ReplayEvaluation) -> String {
    use claude_replay::{ReplayBasis, ReplayEvaluation};
    match evaluation {
        ReplayEvaluation::Evaluated(claims) => {
            let basis = match claims.basis {
                ReplayBasis::SourceScopedExactObservationClaimsV1 => {
                    "source-scoped-exact-observation-claims-v1"
                }
            };
            object(&[
                ("status", text("evaluated")),
                ("basis", text(basis)),
                ("eligibleObservations", number(claims.eligible_observations)),
                ("representatives", number(claims.representatives)),
                ("exactReplayCopies", number(claims.replay_observations)),
                (
                    "unresolvedObservations",
                    number(claims.unresolved_observations),
                ),
                (
                    "unresolved",
                    object(&[
                        (
                            "invalidIdentity",
                            number(claims.unresolved.invalid_identity),
                        ),
                        (
                            "unsupportedRecordType",
                            number(claims.unresolved.unsupported_record_type),
                        ),
                        ("sidechain", number(claims.unresolved.sidechain)),
                        ("invalidClaims", number(claims.unresolved.invalid_claims)),
                        (
                            "conflictingEvidence",
                            number(claims.unresolved.conflicting_evidence),
                        ),
                    ]),
                ),
                (
                    "conflicts",
                    object(&[
                        ("ineligiblePeer", number(claims.conflicts.ineligible_peer)),
                        (
                            "identityMetadata",
                            number(claims.conflicts.identity_metadata),
                        ),
                        ("tokenFields", number(claims.conflicts.token_fields)),
                        ("parserEvidence", number(claims.conflicts.parser_evidence)),
                        ("uuidRelated", number(claims.conflicts.uuid_related)),
                        (
                            "sessionMessageRelated",
                            number(claims.conflicts.session_message_related),
                        ),
                    ]),
                ),
                (
                    "components",
                    object(&[
                        ("input", component(&claims.components.input)),
                        ("output", component(&claims.components.output)),
                        (
                            "cacheCreation",
                            component(&claims.components.cache_creation),
                        ),
                        ("cacheRead", component(&claims.components.cache_read)),
                    ]),
                ),
            ])
        }
        ReplayEvaluation::IdentityBudgetExhausted {
            observations,
            limit,
        } => object(&[
            ("status", text("identity-budget-exhausted")),
            ("observations", number(*observations)),
            ("limit", number(*limit as u64)),
        ]),
        ReplayEvaluation::InvalidIdentityLimit => {
            object(&[("status", text("invalid-identity-limit"))])
        }
        ReplayEvaluation::WrongHarness => object(&[("status", text("wrong-harness"))]),
        ReplayEvaluation::AllocationFailure => object(&[("status", text("allocation-failure"))]),
    }
}
