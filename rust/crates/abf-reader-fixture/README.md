# ABF112 — synthetic scripted reader fixture

Unpublished S1 experiment. No I/O, paths, clock, callbacks or source discovery.
Caller supplies an AttemptRef, existing CatalogSnapshot/CatalogScope and borrowed
script. Scope must belong to that snapshot. This proves no real root authorization,
platform confinement, physical source identity or producer layout.

Contract: one initial enumeration for member 0, then interleaved expected-source
operations. Closed/Partial counts must equal the distinct supplied SourceRefs;
all other statuses require an empty listing. Scope/root failure remains explicit.
One scope per invocation; second enumeration is deliberately omitted (optional G4).
Original deliveries have explicit nonzero u32-representable frame identities;
record operations have explicit consecutive source ordinals beginning at 1.
Control source ordinals are explicit nonzero u32 values; they do not consume record
ordinals. End controls should follow record ordinals; G4 checks their meaning.
OpenAttempt -> OpenSuccess -> Record* -> End/ReadError; OpenAttempt may instead
OpenFail/Vanished. Skip is valid before opening. End counts are computed from
original Record operations, never supplied by caller. All sources must terminate
before explicit RunEnd. Ending the script earlier preserves incomplete evidence.
No ProcessEnd, barrier, lifecycle or distribution evidence is generated.

Replay names an earlier original delivery by script index. It preserves frame and
source identities; optional replacement bytes are allowed only for a record and
are parsed anew. Replay does not advance source state or record count. Equal bytes
with distinct identities remain distinct. G4 alone applies existing synthetic P-Q;
this is no production replay policy. Contradictory ordinary transitions fail with
InvalidScript; replay conflicts remain expressible as adversarial input.

Cancel is a polling point before open, during pending open or between reads.
Cancel after an accepted RunEnd is InvalidScript; exact Replay after RunEnd remains
legal. Cancellation as the first step is legal and emits no frames.
It emits no frame and suppresses the entire suffix. Prefix validation is atomic;
all suffix bytes/steps/records still count toward preflight resource limits.
Cancellation creates no End, RunEnd or supervisor barrier. Empty script is valid
absent evidence. No implicit success, truncation or projection on any error.

Fixture-only ceilings: 512 steps (below G4 MAX_FRAMES), 128 record deliveries
(including replacement/exact record replays), 131072 total fictional record bytes
(including replay copies), and G4 MAX_RECORD_BYTES per record. Enumeration uses
G4 MAX_INSTANCES_PER_SCOPE. Parser uses Limits::MAX and correlation None; parser
rejections remain Record results. Checked counters, fallible output reservation;
allocator abort inside accepted parser/G4 allocations cannot be recovered here.
No caller-forged ParseResult input. These are experimental bounds, not defaults
for production. Caller owns input allocation; bounds precede transformed output.

MAX_BYTES limits fictional record payload bytes, not copied source names. Enumeration
replays copy their entire listing. SourceRef requires an ASCII scope tag of at most
16 bytes, one hyphen and an ASCII instance tag of at most 47 bytes, within OpaqueRef's
64-byte ceiling. Thus each copied name has at most 16 + 1 + 47 = 64 character bytes.
At most MAX_STEPS (512) enumeration deliveries, each with MAX_INSTANCES_PER_SCOPE
(256) names, produce 131072 copied Strings and at most 8388608 character bytes
(8 MiB). For a shorter fixed scope tag, the character bound is
512 * 256 * (tag length + 1 + 47). These maxima are attainable with distinct
47-byte nonreserved instance tags; exact replays retain the original identity/list.
Memory also includes String headers, collection/frame storage, allocator overhead
and caller-owned inputs, beyond character bytes; capacity and layout are not a
portable byte-for-byte memory guarantee. This finite experimental ceiling does not
prove native production memory confinement. Parser/dependency allocations and
reference clones are not universally recoverable from allocator aborts.

Payload-bearing public types have redacted Debug; fixed errors contain no inputs.
Explicit frame access is intentional evidence access, never logging. G4 retains
internal replay-comparison bytes; production retention/privacy remains unresolved.
No wire schema, storage lifetime, filesystem authorization or production UD is set.

Executable acceptance matrix (tests/reader.rs and tests/downstream.rs):

| Contract | Tests |
| --- | --- |
| A: enumeration/root/scope fidelity, zero sources | enumeration_fidelity, inventory_never_zero |
| A: open/end/failure/read/parser fidelity | parser_conservation, transitions_and_failures |
| B: cancellation at three points; no later reads | cancellation_points |
| B: contradictory grammar and atomic refusal | invalid_transitions |
| B: each fixture ceiling -1/at/+1; overflow | bounds, identity_overflow |
| C: distinct equal bytes; exact/conflicting replay; ordering | delivery_identity |
| D: unchanged finalize/admit/project; literals and conservation | downstream tests |
| D: P-Q, absence of zero, whole refusal, diagnostics | downstream tests |
| E: source/dependency isolation | gate static inspection + offline metadata |

Tests inject fictional supervisor process/barrier/distribution facts. They do not
prove actual termination or drain. Native Windows/WSL validation is prohibited and
NOT_RUN. Historical production readers and existing tests are not invoked here.

Replay and Cancel ignore their Step.frame_seq placeholders; replay uses exactly
its original delivery's identity. Replay targets must be earlier non-Replay,
non-Cancel steps. This structural rule and resource preflight also apply to a
cancelled suffix; operational transitions in that suffix are never executed.
Frame slice order is script delivery order, not sorted identity order. G4 retains
its own ordering/reconciliation rules; End's supplied control ordinal can still
produce a G4 sequence anomaly. The emitter never certifies coverage from its state.

Additional executable checks: total_bytes_are_bounded_independently_of_per_record_limit,
record_replay_counts_toward_budgets, enumeration_bound_and_binding,
active_step_boundary_and_no_fabricated_end, partial_listing_and_invalid_terminal_transitions,
contents_never_assign_delivery_or_source_identity_and_debug_is_redacted,
impossible_replay_indices_and_counts_fail_without_panicking, and the private
checked_resource_failure_and_overflow_are_fixed_errors unit test.
G4 source rows exercised here are S3, S4, S10, S11, S13, S14 and S15;
this narrow valid grammar does not generate every contradictory S1–S15 input.
All outcome/verdict computation stays in the unchanged accepted G4 dependency.

ABF112B focused regressions in tests/reader.rs:

| Finding | Tests |
| --- | --- |
| F01: refuse Cancel after RunEnd, preserve replay/cancel rules | cancel_after_run_end_is_invalid, exact_replay_after_run_end_remains_legal, cancel_first_emits_no_completion_and_preserves_suffix_preflight |
| F02: replacement bytes bind parser results; original IDs remain unique | replacement_record_bytes_bind_the_actual_parser_result, duplicate_original_frame_identity_is_invalid_but_replay_reuses_it |
| F03: direct suppression of an otherwise valid second-source tail; fictional G4 conservation | cancellation_preserves_exact_prefix_with_an_otherwise_valid_second_source_tail |
| F04: maximal listing replay, 512 accepted / 513 refused | maximal_enumeration_replays_are_bounded_by_steps_and_source_ref_rules |
