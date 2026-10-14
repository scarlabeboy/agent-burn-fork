# Synthetic readout adapter (M4B candidate)

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
coded inspection failure. `receiver_uses` distinguishes immediate successful
`use_result` readouts from refusals with exact M3A reason codes. A success records
one past synchronous transition, not current permission after subsequent
retirement or lineage loss. No public View converter or historical-to-current
conversion exists. All public fields remain descriptive and forgeable; consumers
must never treat even a constructed `Authorized` value as a credential.

Every successful projection carries all eight qualification flags and each
contributing capture's fixture object, environment, principal, harness, directory,
range and symbolic time. Sidecars are matched in fixed observation creation order.
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
snapshots describe M3A accounting only. A deadline fences without releasing an
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
directory under `/tmp/abf43-m4b-synthetic-adapter/`, with offline networking and no
ambient Rust wrappers. Run from that temporary root, with absolute manifest paths:

```sh
cargo generate-lockfile --offline --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo fmt --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo fmt --check --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo check --offline --locked --lib --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo test --offline --locked --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo clippy --offline --locked --all-targets --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml -- -D warnings
cargo tree --offline --locked --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-adapter/Cargo.toml
cargo test --offline --locked --manifest-path /home/beboy/Projects/agent-burn-fork/rust/crates/abf-synthetic-model/Cargo.toml
```

This candidate is not approved for application use. Independent audit is pending.
