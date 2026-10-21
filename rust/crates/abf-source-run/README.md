# abf-source-run

Bounded, synthetic-only G4B-1 evidence companion to the accepted Agent Burn
crates. This isolated unpublished Cargo workspace implements V1.6 from ABF107O,
with the two ABF107L acceptance errata. It performs no I/O, collection, process
execution, clock reads, network access, persistence or usage projection.

`CatalogBuilder` assigns unique catalog tags. `FixtureRefIssuer` creates fictional
instances separately from listing their existing references. Labelled creation
and rejected calls do not advance counter-form counters. Reset clears registries
and counters; references can repeat after reset or across issuers. These labels
supply no production identity, restart stability or physical-event equivalence.
The harness is the accepted parser's single OpenCode harness.

Supply an untrusted `RawExecutionLog`, a manifest for one environment/ref, explicit
fictional process termination, a completed drain barrier and opaque times.
`finalize(raw, &catalog)` always uses P-Q. It validates representation and admitted
listings, resolves control identities before freezing any expected population,
assigns every pre-barrier delivery a disposition, reconciles expected sources with
S1–S15, then derives second-enumeration qualifications and ordered verdicts.
Multiple admitted INITIAL identities fail closed with `AmbiguousInitial` and no
expected population. D2 conflicts retain their established precedence.

Reader payloads are structured synthetic frames. Record payload equality includes
original bounded bytes and the supplied parser representation; finalization never
reparses it or authenticates the reader's claim. Admitted parser results are retained
verbatim, including rejections and records with contradictory/no-traversal claims.
Controls and quarantined records produce no retained parser records. Fixed
`DeliveryEvidence` keeps original conflicting terminal and enumeration claims
for member-owned sources. Unbound B/C deliveries retain only non-sensitive frame
metadata and fixed-code dispositions; no source reference, tag or payload survives
in the snapshot, including D2 and mismatch-quarantined variants.
Private admitted transport equality keys keep bytes only for later replay
classification; they contain no additional parser records and have no export API.
All opaque types redact Debug. Explicit parser-result and opaque-reference
accessors remain sensitive and must not be logged by consumers.

The initial expected set remains frozen. Optional SECOND listings report new
references relative to that set, with Exact/LowerBound qualification. They never
add sources, change a successful traversal, or assert exhaustive coverage.
Unknown, absent, unavailable and empty inventories have no fabricated source or
numeric value. Each member has its own verdict and lineage; Computer is absent
from the evidence environment model. Consumers must retain the subset-retry marker
and must not turn relative coverage into a claim of full-scope coverage.

Pre-barrier input and post-barrier diagnostics use separate APIs. Bind a
`PostFinalizationLog` to the validated execution, append fictional late frames,
then take immutable `DiagnosticsSnapshot`s. Payloads are dropped after
classification. `assess(member, &diagnostics)` leaves the original verdict intact
and returns a presentation ceiling; member-attributed late facts affect that member,
while unattributed late controls affect every member's assessment. Snapshot
identity mismatches are rejected. Capacity overflow preserves individual entries
and exact aggregates; actual aggregate arithmetic overflow sets a terminal flag,
freezes counts as marked lower bounds, and never changes layer 1.

Implementation-only bounds:

| Bound | Value |
| --- | ---: |
| Instances per scope / each listing / new-source list | 256 |
| Counter-form executions and finalizations per issuer | 1024 each |
| Labelled attempt registry per issuer | 1024 |
| Catalog/manifest members | 16 |
| Pre-barrier deliveries / quarantines / retained-record ceiling | 4096 each |
| Individually kept diagnostic entries | 64 |
| Original bytes in one record | 65536 |
| Parser result retained bytes | 4096 (accepted parser ceiling) |

Labelled and counter-form sources share the bounded per-scope registry. Labelled
attempts use their separate bounded registry. Configured issuer exhaustion is
`CounterExhausted`; collection overflow is `CapacityExceeded`; unrepresentable raw
u64 fields or an actual checked u32 counter overflow is `CountOverflow`. Zero and
u32::MAX are representable sequence values; reconciliation judges sequence facts
separately. Gap accounting uses checked differences between observed identities,
subtracting attributable source deficits; it invents no leading or trailing frame.
A source frame must use a syntactically valid reference. An unlisted reference
whose tag a manifest member owns is quarantined as `UnexpectedSource` and counted
only in that member's `unexpected_source`. A valid reference whose tag no manifest
member owns (catalog-known or unknown) is also `UnexpectedSource`; every non-D2
pre-barrier delivery instance increments the checked execution-level
`unbound_unexpected_source_deliveries()` count. No member or source is created.
Rule 1a makes every selected member of that execution `Inconsistent`, preserving
its original evidence and legitimate retained records. Exact unbound repeats count
once per delivery, with no admit-once record or replay count. D2 retains precedence;
distribution-mismatch and late deliveries never increment this layer-1 count.
Wrong-scope enumeration listings still return `SourceScopeMismatch`; invalid
source syntax still returns `InvalidSourceRef`.

For each successful unseeded finalization, `UnexpectedSource` dispositions equal
the sum of member-local unexpected counts plus the execution-level unbound count.

Source and claim iteration preserves initial listing order and frame identity order;
retained records are canonical by reference, source sequence and frame identity.
DeliveryIndex follows the local input list and is excluded from comparisons between
permutations. Enumeration member indices refer to the supplied manifest; reorder
those indices with the manifest when testing equivalent inputs.

Deliberately deferred to G4B-2: ProjectionMembership, A1–A10/A8b (including A5),
adapter M1–M13, project_with_companion, PairedProjection, calls to
abf_counting::project, G3B figures, projection/refusal conservation, and external
construction compile-fail tests. The existing crates remain independently usable.
Production readers, identity, clocks, filesystem and transport semantics remain
unapproved. No runtime P-R/P-F modes or physical-event deduplication exist.

Acceptance errata: R99g quarantines the two **f3** variants; it has no f5.
The defined boundary set is **R101a–R101f**; no R101g is invented.

Run locally, using an isolated target directory:

```sh
CARGO_TARGET_DIR=/tmp/abf107q-g4b1-ua7-rust-correction/target cargo test --offline --locked --manifest-path rust/crates/abf-source-run/Cargo.toml
```

The gate's external coverage report records actual executable results and deferred
parts; a Rust test name alone is not evidence that every projection in a matrix row
was exercised.
