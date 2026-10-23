# ABF118: fictional Windows confinement contract fixture

This standalone, dependency-free Rust workspace models a conservative proposal
from ABF117 using only an in-memory graph. It is a synthetic security-contract
experiment, **not a Windows backend or native confinement proof**. There are no
features, host filesystem calls, paths, OS handles, FFI, network, process or WSL
operations. `unsafe` is forbidden. Existing S1, S2 and G4 crates are unchanged.

## Fictional API

`FakeWorld` contains at most eight objects, including a root and a reserved
outside sentinel. `directory` and `file` add bounded objects; file payloads are
at most eight bytes. Objects have opaque, world-bound tokens. Cloning a world
creates a replay with the same fictional identity and independent mutable state.
The memory-only atomic issuer uses checked, never-reused process-local tokens;
they have no persistent or native meaning. Token-space exhaustion panics rather
than reusing an identity. No allocator/OOM or process-abort guarantee is claimed.

`FictionalAuthority::explicit_approval` is an explicit **fictional** approval
operation. It authenticates no human. A private `Selection` binds world allocation,
Windows environment, root object, volume, root revision, descriptor/revision,
scope, attempt and authorization generation. `Driver::new` validates it and
privately constructs `FakeRoot`. Missing or changed bindings refuse work. Neither
an S2 `FixturePlan`, a locator, nor an arbitrary caller value is a capability.
There are no callbacks, native extension points or plan conversions. Each driver
is one attempt simulation; replaying the same selection is intentionally allowed
for deterministic tests, not a production authorization/revocation policy.

`Driver::step` accepts `Issue`, `Return(Completion)`, `Release`, bounded `Retry`,
explicit mutation, cancellation and logical-time events. It serially lists the
root, opens admitted candidates, and lists directories or reads files. Private
candidates bind parent, ordinal, listing revision, object and depth; callers cannot
supply candidates. Metadata graph validation is pure model inspection, not simulated
OS I/O. Only successful `Issue` records a simulated List/Open/Read target access.
The root's initial held handle is charged separately during validation.

Root replacement/revision change, moved objects/ancestors, changed candidate slots,
wrong volume, uncertain aliases, any reparse marker and multi/unknown link counts
refuse. Duplicate incoming graph edges also refuse. A movement marker means
observed loss of binding; it does not model native rename prevention. Redirects
never follow a target. Outside metadata can be inspected to refuse a token, but
no outside List/Open/Read operation or payload access is admitted. This is a
small object graph, not a path/name parser or Windows layout recognizer.

Public-created objects all use the fixed fictional volume `1`. Wrong-volume
behavior is currently exercised only through `Fault::WrongVolume`; the numeric
volume-comparison predicate has no independent public mismatch witness.

## Evidence and discovery

`evidence()` returns an immutable fixed-capacity snapshot. Debug output redacts
worlds, identities, bindings, capabilities, drivers and evidence; errors contain
fixed codes. `bytes()`, `binding()` and `bound_root()` provide explicit access to
bounded synthetic evidence. Evidence retains fictional binding/root/volume/revision
provenance even after the driver is gone. No timestamps or durable identities are
invented.

`NoWork`, `Unauthorized`, `Missing`, `Denied`, `Unsupported`, `Ambiguous`, binding
and security refusals, list/open/read failures, `Budget`, `Allocation`, cancellation
and protocol failure stay distinct from completion. `CompleteEmpty` requires every
queued directory actually to return and release a successful exhausted listing,
with no unresolved entry. `Partial(n)` models a list I/O failure after a bounded
prefix: admitted entries remain alongside `ListFailed`. A failure never becomes
successful empty enumeration. Already released bytes survive later failure or
cancellation; unpublished chunks are discarded. There is no G4 mapping, especially
no fabricated partial-I/O carrier, ReaderFrame, supervisor fact, End or RunEnd.

## Fixed resource contract

The ledger order is the following table order. Limits may be lowered to zero;
values above the fixed ceilings fail. Charges and peaks use checked arithmetic.

| Resource | Ceiling | Meaning and justification |
|---|---:|---|
| Graph | 8 | Includes root and reserved sentinel; enough for adversarial small trees |
| Edges | 16 | Preflight total edges; four child slots per directory, no arbitrary names |
| Depth | 3 | Peak candidate depth, root is depth zero; finite ancestor checks |
| Examined | 16 | All returned listing entries, before admission/refusal |
| Rejected | 16 | Uncertain entries; capacity reserved before security inspection |
| Operations | 32 | Every issued List/Open/Read including transient failures; fixed trace slots |
| Retries | 2 | Only explicit transient returns; never relax security checks |
| Sources | 8 | Candidate file admissions; directories counted by graph/operations |
| Requested | 64 | Eight-byte read windows, debited atomically with issue |
| Returned | 64 | Actual successful returned bytes, including discarded late results |
| Evidence | 64 | Retained released payload bytes |
| Handles | 4 | Peak live/reserved fake handles, including root and in-flight open reserve |
| Steps | 96 | Work transitions and mutations; bounded operation driver overhead |

Limits are fixture choices with headroom, not production budgets or a promise
that every maximum is reachable by a valid eight-object tree. Fixed graph,
16-candidate queue, 32-operation trace, 8-byte staging area and 64-byte evidence
area bound overhead independently of payload metering. No dynamic traversal/output
allocation occurs. The allocation-failure switch simulates preflight reservation
refusal before handles or operations; it cannot simulate real allocator failure.
A read reserves room for a full eight-byte returned window before issue even for
a smaller payload. It records the actual returned count on completion. A budget
failure preserves prior evidence and closes all quiescent handles. Handle/array
increments have bounds proved by admitted graph/operation/queue capacities; the
configurable resource meters themselves check overflow explicitly.

## Cancellation and ownership

The logical clock is injected, monotonic and never reads wall time. Ten logical
ticks request cancellation; this is not a native timeout. `Cancel` records
`Requested`. Observation while an operation is outstanding records `Pending`;
the evidence code preserves any deferred first failure, or reports `CancelPending`
when none exists. It does not claim stop acknowledgment or close a handle that may
still return. Successful late opens are adopted and closed once, and late read
bytes are counted then discarded. At return/quiescence, cancellation becomes
`Acknowledged`. No further work or byte release occurs after acknowledgment.
Cancellation between return and release discards the chunk. First pending protocol
or budget failure remains explicit even when cancellation also occurs.

Stop, time, cancel and operation-return/cleanup handling have fixed reserved
storage and remain possible after work budgets are exhausted. CancelPending admits
no new operation. After terminal completion, further events have no effect.
A caller that never returns a pending operation cannot obtain acknowledged stop.
Dropping/abandoning a driver abandons the simulation: there are no actual OS
handles or background operations, and no native destructor/unwind/termination
claim. Exact-once fake ownership is observable in opened/closed/live counters
for all executed terminal paths; it is not a kernel handle guarantee.

## Validation and limits of the result

Run only this manifest, offline and with `CARGO_TARGET_DIR` in the gate evidence
directory:

```text
cargo fmt --manifest-path rust/crates/abf-windows-confinement-fixture/Cargo.toml --check
cargo test --offline --locked --manifest-path rust/crates/abf-windows-confinement-fixture/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-windows-confinement-fixture/Cargo.toml --all-targets -- -D warnings
```

Tests specify expected codes, counters and bytes by hand; compile-fail doctests
cover private selection/root/object construction, generic conversion and an S2-shaped
plan passed as approval. Actual S2 is deliberately not linked: the generic conversion
rejection and typed selection boundary apply without importing another workspace.
No syntax/link compatibility with actual S2 is claimed. All meters have below/at/above
and overflow checks; runtime tests additionally exercise effective limits.

Native Windows ACLs, locality, file IDs, handles, races, reparse/link semantics,
rename behavior, continuous ancestry, cancellation APIs, cleanup under OS failure,
real allocation failures, native Windows/WSL execution and production discovery are
**NOT_RUN**. Production UD decisions about authority, persistent identity, rename
and cancellation remain open. No S4, S5, persistence or production reading is
implemented or authorized. A separate independent model audit remains necessary.
