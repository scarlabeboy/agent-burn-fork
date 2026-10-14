# Synthetic readout adapter (M4D correction candidate)

`run_synthetic_fixture(SyntheticCase) -> SyntheticReadout` runs one fresh,
closed in-memory M3A universe. Six fixed cases are available; the default is
`DenyAllDefault`. This unpublished edition-2024 library is `no_std` with `alloc`,
forbids unsafe code, and has its own workspace. Its sole dependency is the
accepted sibling `abf-synthetic-model`. No production caller is connected.

The contract version is 1. Origin is always `SyntheticOnly`; provider account
identity is `NotModeled`. Environment 1 is simulated Windows and environment 2
is simulated selected WSL. Principal 7, harness 3, directory 9, receiver 11 and
wrong receiver 99 are fixture parameters, not authenticated identities.
Computer is only a derived union, never a collection environment.

`historical` contains separately typed inspection from `Model::view`, including
coded inspection failure. Each inspection identifies its typed `Subject`
(`Environment(FixtureEnvironment)` or `Computer`) and lifecycle `Phase`.
Both authorized and refused `receiver_uses` identify subject and attempt phase.
`receiver_uses` distinguishes immediate successful
`use_result` readouts from refusals with exact M3A reason codes. A success records
one past synchronous transition, not current permission after subsequent
retirement or lineage loss. No public View converter or historical-to-current
conversion exists. All public fields remain descriptive and forgeable; consumers
must never treat even a constructed `Authorized` value as a credential.

Every successful projection carries all eight qualification flags and each
contributing capture's fixture object, environment, principal, harness, directory,
range and symbolic time. Private projections retain the exact handle-bound
capture records submitted to `derive` and unioned by `computer`. The sole successful
accept path records an ordinal by counting successful accepts (failed accepts do
not append observations). In the sealed M3A implementation `accept` appends at
`observations.len()`, `derive`/`computer` sort unique observation indices, and `view`
emits ranges in that order. Thus this ordinal equals the model observation index
in each fresh closed universe. No handle internals or debug strings are parsed.
Presentation sidecars are deduplicated by handle equality, checked against the
retained contributing records, and ordered by that ordinal, never by capture label.
Conflicting duplicates, wrong sets, counts, environments, ranges or times panic
before returning data. The model environment set is checked too. Even when every
visible attribute is equal, distinct opaque handles remain bound to their original
fixture identities through the retained records. Identical visible values alone
are never used to establish identity.

The fixture object binding retains the lineage supplied to M3A at object creation;
its values are synthetic fixture facts, not model-authenticated native provenance.
The ordinal invariant depends on this closed sole-accept wiring and the sealed M3A
ordering; adding accept paths or changing that ordering requires renewed review.
Identical handles collapse in the model; distinct equal payload/time captures
remain distinct. Sampled extent is a symbolic range union per object, not event
deduplication, token usage, money or verified source provenance.

Positive fixtures use explicit synthetic permit terms, allow unverified producers
while preserving their label, and do not demand newness. The finite cap array is
`[20,8,8,8,8,8,80,300,4,4,2,200]`; the outstanding-read case limits worker occupancy
to 1. Range is `[0,4)` in a 64-unit domain, payload is symbolic 42, capture time
is 0, scope expiration is 100, and worker deadline is 10. These values are test
parameters, never production policy. Resource classes in array order are owners,
workers, stages, proposals, hints, declines, retained lineage, work, prompts,
batches, clear actions and registry/replay records. Counters and held/spent/peak
snapshots describe M3A accounting only. The first outstanding-read snapshot is
`Outstanding`, and the snapshot after refused reservations is `Exhausted`. A deadline fences without releasing an
outstanding read; synthetic CompleteDrain cannot override charged resources;
observed synthetic return accounts for two postchecks. No native quiescence is
claimed. Fixed scenarios bound vector cardinalities, not allocator/RSS behavior.

The adapter has no native inputs, filesystem readers, processes, network,
credentials, clock input, callbacks, threads, IPC, persistence, serialization or
application integration. A fresh call is a separate universe, not recovery or
reuse of a snapshot. Native eligibility, source trust, event/account reconciliation,
production budgets, Windows/WSL behavior and application consumption remain
outside this candidate and require separate approval and evidence.

Gate-authorized validation uses an isolated Cargo home, target and compiler temp
directory under `/tmp/abf45-m4d-adapter-corrections/`, with offline networking and no
ambient Rust wrappers. Run from that temporary root, with absolute manifest paths:

```sh
cargo fmt --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo fmt --check --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo check --offline --locked --lib --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo test --offline --locked --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo clippy --offline --locked --all-targets --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml -- -D warnings
cargo tree --offline --locked --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
```

This candidate is not approved for application use. Independent audit is pending.

The seven compile-fail examples illustrate specific public-consumer type/arity,
import and conversion restrictions. They do not prove absence of all native
operations or prevent forging an `Authorized` DTO. A positive doctest demonstrates
legitimate fixture invocation and mutable descriptive data. Dependency/API scans
supply separate isolation evidence. No public constructors or input authority
were added; `Subject`, two snapshot phase variants, and mandatory entry labels are
descriptive additions to the ephemeral version-1 fixture contract.
