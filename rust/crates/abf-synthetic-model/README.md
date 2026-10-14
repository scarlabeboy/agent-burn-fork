# ABF synthetic model — M3A candidate

This independent, unpublished crate is a deterministic, pure in-memory model of
the ABF36C V1 authority candidate under the ABF38B M2F interpretations. It uses
`no_std` plus `alloc`, forbids unsafe code and has no dependencies. It has its
own workspace boundary and lockfile; no production crate depends on it.

`simulate(limits, |model| ...)` creates a branded synthetic universe. Opaque
capabilities cannot be constructed externally or transferred across universes.
`ScopeSpec`, fake object facts and independent recovery witnesses are explicit
fixture-boundary injections. They are not authentication or native evidence.
Environment, principal, harness and direct-directory intent are part of each
synthetic scope. Numeric name hints carry no object or producer authority.

The transitions are `approve` → `discover` → individual `admit` → `begin` (A)
→ `return_read` → `release` (L) → `accept` (P). `retire` (R) permanently ends
a grant epoch. Scope loss retires all dependent child grants. Canonical owners,
worker pairs/copies and private stages have separate accounting; cleanup cannot
return cumulative credits. Read admission reserves both returned-read postchecks
and disposal work. Explicit scheduling steps fence overdue work; observed late read returns account
for cleanup. A deadline alone never releases outstanding resource credit;
there are no threads, clocks, processes, handles or operating-system calls.

Missing hints always apply re-encounter and unknown-history dispositions. Every
possibly applicable disposition must permit admission, and unknown-history must
be no more permissive than re-encounter. Fixed denial codes have no promised
precedence. Retained hints can classify or suppress, never restore authority.
Declines use stable scope lineage plus name hint; restart, policy change and
renewal retain them and cumulative credits. Ledger loss conservatively denies.
Clearing is a separately injected user authorization and consumes finite credit.
Manual presentation uses an opaque numeric hint, with a truthful unverified
origin label; this model contains no text renderer.

Observations are independent symbolic captures, even when payloads and capture
times are equal. Derivation and Computer retain contributing observations and
re-evaluate current qualifications. Sampled byte unions do not settle logical
event identity. Mutable/version-unproven, partial inventory/bytes and unresolved
event duplication remain explicit. Later disputes, stale status and revocation
propagate; lineage loss withholds possibly dependent projections. Computer has
no collection capability.

The `Limits.caps` indices are: 0 owners, 1 workers, 2 stages, 3 proposals,
4 retained hints, 5 declines, 6 observation/derived lineage, 7 cumulative work,
8 prompts, 9 batches, 10 suppression clears, 11 registry/replay records.
Classes 0–5 count releasable occupancy. Class 6 is retained lineage storage
and has no retention-authorized release in this model. Classes 7–11 are
cumulative credits and never refill. Counts and peaks are
symbolic, not production numerical budgets or allocator/RSS measurements.
Registry tombstones deliberately remain bounded and can exhaust the model.

Tests contain separately declared disposition, R/A/L/P and resource/counter
oracles, deterministic schedules, UI/receiver stubs and compile-fail controls.
Run only with explicit gate authorization, offline and locked, with isolated
Cargo home and target directories. See the M3A sealed reports for executed
commands, source hashes and actual mutation evidence.

This candidate implements no native collection, parser, UI, persistence, event
identity resolution, legacy multi-file adapter, version witness or exact usage
total. No native quiescence, syscall cancellation, containment, producer
noninterference, Windows/WSL property or production readiness is established.

M3A targeted corrections (ABF39B): outstanding read/copy/stage resources remain
charged after a deadline, fencing or revocation until `return_read` supplies an
actual synthetic returned-read fact and both checks. Hung operations therefore
block unsupported restart and further over-cap admission. No timeout closure,
termination witness or native bounded-completion mechanism is supplied.
Restrictive qualification facts update existing storage without exhaustible
work/lineage credit; revision overflow conservatively withholds results.

`derive` and `computer` union identical observation handles, charging only unique
contributors. Distinct observations with equal payload/range/time stay distinct.
New Computer creation checks current contributor liveness and use rights;
retained historical inspection continues to show current restrictive labels.
Invalid or unproved disposition configurations cannot present consent prompts.
Metadata-only scope approval remains possible, preserving DenyAllDefault.

The compatibility fixture methods `fixture_object` and `approve` explicitly
inject a sampled extent and scope ceiling of 64 synthetic units. The variants
`fixture_object_with_size` and `approve_with_range` inject other finite trusted
facts. `Range.domain_end` may request a narrower domain, but cannot enlarge the
held object's sampled extent or the scope's range permission. These fixture
facts are not native size sampling or real byte-offset enforcement. Renewal
preserves the scope range ceiling. Invalid renewal terms are checked before R;
a subsequent resource failure is intentionally fail-closed and can retire the
old scope without replacement. No rollback or automatic credit refill is promised.
