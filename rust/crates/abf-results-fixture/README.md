# ABF126 - B1 synthetic results integration

ABF126C corrects error-selection consistency, Windows unavailability, view
construction privacy and the discriminating integration regressions.

Standalone, unpublished, read-only presentation fixture for independently obtained
B1/S4 Windows supervisor and B1/S5 SelectedWSL responses. No native access,
collection, finalization, counting rules, callbacks, transport calls, discovery,
clocks, persistence or production contracts are introduced. Existing crates and
workspace manifests are unchanged.

## Consumer API

`present(Inputs)` returns `Results` with fixed Windows and SelectedWSL accessors
and the requested selection. This is a presentation envelope, not a shared G4
membership, Computer projection, total or collection-success verdict.

- `Inputs::WindowsOnly(&supervisor)` has no WSL input or transport capability.
- `Inputs::SelectedWsl { selected, response }` requires exactly one explicit S0
  WSL identity, including its fictional distribution name. A response must match
  the entire identity. No identity or distribution is inferred from records.
- `Inputs::Both { windows, selected, response }` retains the same independent
  environment views. Invalid selection or mismatched response refuses the
  envelope. `Err(S5::Error::WindowsOnly)` contradicts either WSL-enabled mode
  and refuses with `Error::SelectionMismatch`; all other S5 errors remain exact
  visible SelectedWSL results alongside Windows. No fallback, retry or exchange
  occurs. Invalid Both inputs never return a partial validated envelope.

Callers supply actual immutable S4 state and the result of an independently
performed S5 exchange (`Result<&Response, S5::Error>`). Errors are caller-supplied
observations, not authenticated transport evidence. Inputs and outputs borrow
source evidence; Rust prevents mutable supervisor access while it is borrowed.

The fields of `WindowsSnapshot`, `WindowsView` and `WslView` are private;
external callers cannot construct mismatched views. Read-only accessors expose
all evidence and failures without allowing replacements:

- `WindowsSnapshot::evidence()`, `projection()` and `assessment()`.
- `WindowsView::source()`, `latest()` and `last_good()`.
- `WslView::selected()`, `response()` and `projection()`.

`WindowsView::source()` preserves the latest report, exact code and issued identity,
including attempts without accepted snapshots. `latest()` and `last_good()` each
pair the original snapshot with its independent G4 projection result and
`assess(member, original_diagnostics)` result. Projection refusal hides neither
assessment nor evidence. Last-known-good selection remains exclusively S4's
existing policy; no history, supersession or recovery policy is added.

`WslView` retains the explicit selection, original response or exact pre-call
error, and the independent `Response::project()` result when a response exists.
The response retains its distribution, attempt/finalization, exact execution or
refusal, delivery and accepted cancellation facts. A pre-call error has no
fabricated response, attempt, projection, timestamp or numeric zero. S5 supplies
no post-finalization diagnostics snapshot; this fixture creates none and makes
no late-evidence assessment claim for S5.

## Reading evidence states

These states are separate facts and can coexist. Consumers must inspect them
rather than infer collection success from a successful projection.

| State | Authoritative evidence |
| --- | --- |
| Fresh Windows evidence | `source().availability() == Fresh`; `freshness() == Fresh` |
| Stale Windows evidence | S4 `Stale`; inspect `last_good()` separately from `latest()` |
| Incomplete evidence | Original G4 coverage, source outcomes, process, RunEnd and qualifiers; S5 delivery also remains visible |
| Unavailable Windows evidence | S4 `Unavailable`; `freshness() == Unavailable`, distinct from WSL `Unestablished` |
| Unavailable collection evidence | Exact S4 failure, G4 unavailable/no-attempt verdict, or exact S5 error; these are not interchangeable |
| Cancelled evidence | S4 `Code::Cancelled` or S5 accepted `cancelled()`; pre-call S5 cancellation remains its exact error |
| WSL freshness | Always `Unestablished`, even with covered evidence and complete delivery |

A stale Windows view can retain a cancelled latest attempt. An unavailable S4
state can retain incomplete records. Covered G4 evidence can have a weakened
late-evidence ceiling. Complete S5 delivery can contain an incomplete collection.
None of these combinations is collapsed into success or zero. `Unestablished`
is absence of a freshness guarantee, not a claim that no response exists.

## Authority, provenance and bounds

Only each original snapshot/response API invokes G4 membership validation and
paired projection. Independent catalogs are never rebuilt, merged or substituted.
G4 requires common original catalog authority for joint membership; S4 and S5
independent results do not supply it. Both therefore exposes separate projections.
Even matching local `a1`/`z1` labels are never treated as cross-environment identity.
The test explicitly witnesses G4's `CatalogAuthorityMismatch` on attempted joint
membership with the Windows catalog.

Original borrowed evidence preserves environment/ref, catalog authority,
manifest/lineage, harness/family/layout, source, record/frame/delivery sequence,
parser claims, attestation, coverage and diagnostics where the upstream API
provides them. No missing identity is invented before a snapshot exists.
Supplied opaque times remain in their original executions; there is no clock
comparison, timestamp synthesis, age calculation or cross-environment ordering.
Synthetic provenance remains `Unattested`, never physical provenance.

The envelope has at most two environment slots, two Windows snapshot views and
one WSL response view. It adds no history and copies no raw evidence. At most
three whole G4 projections are requested, each under the existing 128
occurrence/declaration ceilings. Even when latest and last-good reference the
same S4 snapshot, each view has its own bounded projection. Excess evidence
refuses whole projection and remains accessible unchanged; it is never split,
filtered or truncated. S1/S4/S5 resource limits and caller-owned allocations
remain unchanged. No portable allocator recovery guarantee is added.

Presentation is deterministic for equal borrowed inputs and existing rules, with
fixed environment slots and G4's existing ordering. Debug output redacts all
presentation types. Explicit evidence/projection access remains sensitive and
is not a logging, serialization or production UI contract.

## Validation

From the repository root, using already installed Rust tools and local path
packages only:

```sh
cargo fmt --manifest-path rust/crates/abf-results-fixture/Cargo.toml --check
cargo test --offline --locked --manifest-path rust/crates/abf-results-fixture/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-results-fixture/Cargo.toml --all-targets -- -D warnings
cargo test --offline --locked --manifest-path rust/crates/abf-source-run/Cargo.toml --target-dir rust/crates/abf-results-fixture/target
cargo test --offline --locked --manifest-path rust/crates/abf-supervisor-fixture/Cargo.toml --target-dir rust/crates/abf-results-fixture/target
cargo test --offline --locked --manifest-path rust/crates/abf-selected-wsl-fixture/Cargo.toml --target-dir rust/crates/abf-results-fixture/target
```

The integration tests cover WindowsOnly isolation, explicit WSL selection,
distribution mismatch, distinct authority and colliding local labels, exact
provenance/times, stale data, no-attempt/unavailable state, pre-cancellation,
late diagnostics, interrupted prefixes, successful projection with incomplete
collection, finalizer refusal with cancellation, 129-source projection refusal,
pre-call errors alongside Windows, deterministic repeated presentation, redacted
Debug and absence of invented WSL last-good/freshness after a prior covered
response. ABF126C extends these tests to distinguish Windows Unavailable from
WSL Unestablished, reject only the contradictory WindowsOnly error in both WSL
modes, refuse invalid Both selections, verify original Fresh latest/last-good
snapshot sharing, preserve Fresh/Stale Windows evidence beside WSL errors, and
compare populated Windows projections across repeated presentation. Three
compile-fail tests prevent external construction of the private views.

For ABF126C, the locked fixture tests, formatting and clippy are the focused
checks. Existing ABF126 G4/S4/S5 regression evidence and unaffected S1/S2/S3 and
lower-level counting proof are reused; dependency contracts are unchanged.

NOT_RUN: native Windows/WSL, distribution discovery/startup, real transport,
filesystem confinement, provider histories, network, production data, persistence,
restart recovery, full product builds and product presentation. ABF120-F01/F02
blockers and F03 prerequisite remain unchanged. Synthetic success grants no
production authority or validated native confinement under Contract A or B.
