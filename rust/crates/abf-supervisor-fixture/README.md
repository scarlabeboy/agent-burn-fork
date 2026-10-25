# ABF124B - B1/S4 synthetic supervisor

Standalone unpublished Rust workspace, synchronous and synthetic only. It has no
filesystem, process, clock, network, WSL, callback, native reader or persistence
port. WindowsOnly never requires an installed WSL distribution. Windows and
SelectedWSL authorities are not combined. S5 is excluded.
ABF124D adds focused cancellation precedence and audit regressions.

## Contract decisions for this fixture

- `Supervisor::new` takes an existing S2 `FixturePlan` with exactly one conditional
  Windows decision. No work, unresolved/unrecognized and multi-scope plans refuse
  explicitly, rather than being filtered or becoming empty success. The original
  catalog allocation survives through finalization and admission.
- S1 `emit` owns grammar, parsing, identities of deliveries, replay, cancellation
  polling and script budgets. S4 does not implement another reader or parser.
  Source references remain explicit caller-supplied synthetic claims. The private
  fixture issuer issues fresh local attempt/finalization labels without reset.
  Issued labels remain in the attempt report even on reader/drain refusal with
  no accepted finalization; they are never fabricated for pre-canceled calls.
- S3 is deliberately not converted into authority: its existing contract has no
  S2-to-S3 conversion or record framing interface. This supervisor needs no file
  I/O. It does not infer authorization from a locator, plan, fake root or bytes.
- Each explicit `run` is an independent attempt. There is no scheduling, automatic
  retry, cross-attempt membership, lineage composition, deduplication or summing
  batches to bypass admission ceilings.
- `SyntheticFacts` supplies fictional process, drain and opaque time observations.
  No real termination, drain, elapsed time or security property is established.
  Pre-cancellation performs no issuance/emission. Otherwise S1 Cancel suppresses
  its suffix and models `ProcessEnd::KilledCancel`, superseding the caller's
  fictional process input, including crash or timeout. This is intentional
  synthetic model precedence, not a separate native termination observation.
  It never upgrades the supplied barrier, invents End/RunEnd or bypasses suffix
  resource preflight. Cancellation can be refused by an invalid script/barrier,
  preserving the exact underlying error.
- Optional `barrier_after` counts emitted deliveries, including replays, after
  cancellation suppression. The bounded suffix goes only through an execution-
  bound `PostFinalizationLog`. No late delivery mutates finalized records/verdicts.
  Times remain opaque; the injected barrier instant labels all late arrivals.
- Unchanged G4 finalization, membership, projection and assessment own evidence
  interpretation. Dependency error variants propagate verbatim in fixed codes;
  `Code::action` explains the next inspection. Projection refusal retains the
  complete finalized snapshot without skipping or truncating records/sources.
  After accepted finalization, emitted cancellation remains the primary
  `Code::Cancelled` outcome even when projection refuses. The immutable snapshot's
  `projection_error()` retains the exact `AdaptCode` independently. Without
  cancellation, projection refusal remains `Code::Adapt(error)`. Neither case
  replaces last-known-good or produces Fresh availability.
- A fixture-local reliable result requires successful admission/projection,
  CoveredWithinEnumeration or explicit Closed(0), normal process exit, RunEnd,
  accepted drain and no weakened late-evidence ceiling. This means reliable
  synthetic evidence only, never complete real usage. Root absence, failure,
  unavailable inventory, missing completion and cancellation do not qualify.
- Latest attempt and last-known-good are separate immutable snapshots. Only a
  reliable result replaces last-known-good. Availability is Fresh after such a
  result, Stale after any subsequent unsuccessful call with prior reliable data,
  or Unavailable without reliable data. It is logical attempt state, not measured
  age. Latest incomplete records remain explicitly inspectable independently.
  There is no durable history, restart recovery or production supersession rule.

## Resource and privacy limits

S1 ceilings remain 512 script steps, 128 record deliveries, 131072 total record
bytes, 65536 bytes per record and 256 sources per listing. Canceled suffixes still
count. All input allocations are caller-owned. Admission independently enforces
128 occurrences/declarations; excess evidence refuses whole projection, not a
truncated result. Diagnostic storage retains at most 64 individual facts with
exact bounded aggregate counts beyond that limit. At most 512 deliveries can be
classified in this fixture, so diagnostic integer overflow is unreachable.

At most 1024 issued attempts/finalizations per supervisor, including unsuccessful
issued attempts; exhaustion is terminal for issuance and does not clear reliable
data. Pre-canceled calls do not consume identity. No unbounded history grows:
two snapshots are retained after a call (latest and last-known-good, sharing one
Arc on success), with at most one additional candidate during execution. Old
latest/last-good references cannot be mutated through the public API.

S1 enumeration replays can copy at most 8 MiB of source-name character data per
emission, in addition to headers, frames and dependency overhead. These are finite
fixture bounds, not a portable process-memory ceiling. Vec split, Arc and accepted
dependency allocations can abort on OOM; universal allocator recovery is not
claimed. No native blocking operation exists, so cancellation latency under native
I/O is not tested or promised.

Debug output redacts supervisor, facts, reports and snapshots. Explicit snapshot,
parser-result and projection access remains sensitive evidence access, not a
logging/serialization interface. Existing G4 private raw replay bytes are retained
unchanged; no production payload retention/export policy is selected.

## Validation

Run from repository root, offline:

```sh
cargo fmt --manifest-path rust/crates/abf-supervisor-fixture/Cargo.toml --check
cargo test --offline --locked --manifest-path rust/crates/abf-supervisor-fixture/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-supervisor-fixture/Cargo.toml --all-targets -- -D warnings
```

Integration tests cover literal claims/provenance, equal-byte distinct records,
independent deterministic executions, stale preservation, exact failures, partial
reads, cancellation at three polls and before start, canceled-suffix budgets,
512-step boundaries, diagnostic capacity, 1024-attempt exhaustion, projection
refusal and late frames. Compile-fail tests witness the absence of a WSL launch
API and refusal of a bare catalog in place of an S2 plan. Existing S1/S2/S3/G4
regressions remain unchanged. ABF124D permanent tests additionally cover Cancel
with projection refusal (with and without prior good data), late delivery alone
preventing Completed despite normal exit and RunEnd, empty/nonempty
AbandonedWithoutReader, and Cancel with valid/out-of-range barrier_after.

Native Windows/WSL, production data, processes, filesystem security, real clocks,
real allocator failures, transport, persistence/restart and S5 are NOT_RUN and
outside scope. ABF120 findings are dispositioned by Contract A V2 in AGENTS.md
(documentation only; no access granted). Synthetic
success does not establish native confinement under either Contract A or B.
