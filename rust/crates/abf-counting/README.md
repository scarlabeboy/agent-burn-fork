# Synthetic counting and projection research (G3B)

Standalone experimental Rust library implementing the G3A V2.2 counting contract
(ABF102D, SHA-256 `00479528…e8d`, accepted for experimental implementation after
ABF103C). Fictional in-memory input only. Requires independent audit. Not a
production policy, not a collector, not a deduplicator. No root workspace,
Tauri, persistence, pricing or network integration.

## Entry point

```rust
pub fn project(set: &SyntheticSet, runs: RunInventory<'_>, selection: Selection)
    -> Result<Projection, Code>;
```

- `set`: one G2B `abf_identity_relations::SyntheticSet` (≤ 128 occurrences, ≤ 256
  relation entries). Read through its public accessors only; never mutated.
- `runs`: `RunInventory::Undeclared` or `RunInventory::Declared(&[RunDeclaration])`.
  A declaration is a caller-reported, unverified run claim keyed by
  (`environment_kind`, `environment_ref`, `source_ref`, `run_ref`). `Declared`
  means only that a list was supplied; it is never an exhaustive inventory.
  Zero-record declarations are valid.
- `selection`: `WindowsOnly`, `SelectedWslOnly` or `Both`.

`Code` covers invalid input/resources only: `RunInventoryCapacity` (more than
`MAX_RUN_DECLARATIONS = 128`), `DuplicateRunDeclaration` (equal key, any statuses,
including unselected kinds; no last-wins), `CountOverflow`, `AllocationFailure`.
Capacity is checked before duplicates so the duplicate scan stays bounded.
Inconsistent evidence is always reported inside `Ok(Projection)`.

## Result domains (never conflated)

| Domain | Field | Meaning |
| --- | --- | --- |
| A | `stored_occurrences: Exact<u32>` | Stored entries in scope. Not events. |
| B | `outcomes: Exact<OutcomeCounts>` | Original parser outcomes: accepted, qualified, incomplete, rejected. Sum = A. |
| B' | `records: Exact<RecordDiagnostics>` | Overlapping diagnostics: eligible, contradictory (C1–C7), unsupported-component observations, records without reported traversal. |
| C / C' | `components: Components<ComponentResult>` | Per component (`input`, `output`, `total`, `cache_read`, `cache_write`): `value` plus always-present `evidence`. |
| D / D' | `physical_events`, `usage` | Single variants `Unresolved` / `Unsupported`. Not computable. |
| F | `runs: RunSummary` | Caller-reported raw claims, per-key resolution, coverage. |
| G | `Projection.relations` | Unverified assertion storage counts. The only carrier of hint information. |

`ComponentValue` is `ReportedClaimSum(u128)`, `NoValidClaims`,
`Indeterminate(reason)` or `Overflow`. `ComponentEvidence` (eligible records,
contributing claims, explicit zeros, field-state histogram, qualifications,
`IdentityBasis::Unresolved`, `TraversalBasis::CallerReported`) is exact for every
value variant. There is no single-number "total tokens", "unique events" or
"usage" accessor.

## Consumer labels (R8; final UI wording still requires approval)

| Machine form | Label |
| --- | --- |
| `ReportedClaimSum` in one scope | "Reported `<component>` claims (eligible records, not deduplicated)" |
| Computer `ReportedClaimSum` with `CrossEnvironmentSum` | "Combined reported claims, both environments — may double-count" |
| `UsageTotal::Unsupported` | "Not available: event identity is unverified" |

Those three rows are the contract §5 labels. Non-normative guidance taken from the
§9 consumer examples: `NoValidClaims` is never shown as "0 tokens";
`Indeterminate` reads as "No data" / "Not collected", never "0";
`ReportedComplete` covers represented run keys only, never an exhaustive scan.
The parser field `total` is always "reported `total` claim".

## Rules implemented

- **Eligibility (§4.1–4.2).** Rejected → Contradictory (first failing C1–C7) →
  Incomplete → Eligible. Only Eligible records feed C/C'. Contradictory records keep
  their original outcome in B and stay in the set. The screen is partial: passing
  it does not authenticate parser execution or synthetic provenance. C6 inspects
  field-state discriminants of metadata fields only, never their values.
- **Components (§3).** Each component is independent. `Valid(0)` is a contributing
  explicit zero. Non-Valid states only fill their bin and set `MissingClaims`. No
  cross-component arithmetic, consistency check or inference. Checked `u128`
  accumulation; overflow yields `Overflow` with exact evidence (unreachable at the
  128-occurrence ceiling; covered by crate-internal tests).
- **Runs (§4.3).** Raw status flags (`PartialRun`, `FailedRun`, `NotCollectedRun`,
  `UnavailableRun`) are set by any raw claim regardless of resolution. A key with two
  or more distinct statuses is `Conflicting` and enters no resolved bin; no status
  is chosen. Coverage table, first match: `Inconsistent`, `NoEvidence` (no
  represented key), `ReportedComplete`, `NoTraversalReported`,
  `ReportedIncomplete`. An absent or empty inventory never erases represented
  record evidence (ABF103B-N1).
- **Value selection (§8.3).** `Overflow`; else `ReportedClaimSum` if ≥ 1
  contributor; else `NoValidClaims` if the scope stores occurrences or coverage is
  reported/inconsistent; else `Indeterminate(NoRunEvidence | NoTraversalReported)`.
  Missing evidence never becomes a numeric zero.
- **Qualifications (§8.5).** Fixed `u32` bitset, one canonical trigger per flag,
  union only. Relation assertions set no qualification.
- **Computer (§6.3).** Derived view over the selected scopes of one set. Single
  selection: the selected scope unchanged. `Both`: evidence summed with checked
  arithmetic for every row; value table symmetric; Computer-only flags
  `CrossEnvironmentSum`, `EnvironmentWithoutClaims`, `EnvironmentIndeterminate`.
  No run summary or coverage of its own (`per_environment` exposes the scopes').
- **Relations (§6.4).** Each stored entry examined once; selected entries
  partitioned into Windows-within, Selected-WSL-within and cross-environment.
  Incident counts are non-additive. Repeated, reversed and conflicting entries
  remain separate. Only `relations` and `excluded.relation_entries` depend on
  assertions. G2B guarantees both endpoints exist; an endpoint that could not be
  found would be counted as excluded, never dropped (unreachable).
- **Selection (§6.1).** Unselected occurrences, declarations and relation entries
  are counted in `Projection.excluded`, never silently dropped.
- **Privacy.** Results and errors contain only enums and integers; `Debug` exposes
  no input-derived text.

## Not provided (deferred)

Real history readers, source-run attestation, token-convention normalization (U1),
physical-event identity and trusted deduplication (U2, E1–E7), reader authority
and exhaustive source lists (U3), cross-set namespaces (U4), ownership of
contradiction screening (U5), multi-harness conventions (U6), R6 hint-referenced
claim subtotals, endpoint-incidence exposure, date bucketing, pricing, quotas,
persistence, WSL/Windows discovery, Tauri or UI integration.

## Tests

`tests/scenarios.rs` (S01–S20), `tests/invariants.rs` (T2–T19),
`tests/regressions.rs` (gate regressions A–Q), shared fixtures in
`tests/support.rs` (explicit `[[test]]` targets keep it out of the target list).
Crate-internal tests cover C1–C7 check identity, injected accumulator overflow
(T6b) and every Computer value-table row including `Overflow`.

```text
cargo test --offline --manifest-path rust/crates/abf-counting/Cargo.toml
```
