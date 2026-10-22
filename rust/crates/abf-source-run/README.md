# abf-source-run

Bounded, synthetic-only G4B-1 evidence and G4B-2A admission companion to the
accepted Agent Burn crates. This isolated unpublished Cargo workspace implements
the V1.7 synthetic evidence and membership contract, with the two ABF107L
acceptance errata and the gate-local ABF108D admission decisions below. It performs no I/O, collection, process execution, clock reads,
network access, persistence or usage projection.

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

G4B-2A admission is separate from projection. Use the re-exported counting
`Selection`, `SelectedRefs { windows, selected_wsl }` (optional opaque refs), and
`ProjectionMembership::validate(selection, selected_refs, members, &catalog)`.
The member slice contains references to immutable `FinalizedMember`s. Validation
checks A1–A5, the ABF108D catalog-authority precondition, A6–A8, A8b, then
independent checked A9 ceilings (128 retained records and 128 expected-source
declarations), returning the first fixed `AdaptCode` for the
whole membership. No accepted membership or downstream artifact exists on refusal;
original finalizations remain inspectable. A5 is enabled: layout versions of one
family cannot be co-admitted. Distinct allowed families and environments can be.
`CatalogBuilder::declare_overlap` records symmetric possible-overlap claims between
registered scopes for A6; no physical overlap is inferred. Each finalized member
privately retains its complete ordered execution manifest for A7.

ABF108D D1 adds `CatalogAuthorityMismatch` after A5 and before A6. Every member
of a nonempty membership must have been finalized against the same immutable
catalog authority supplied to admission. A snapshot clone preserves that authority;
a separately built snapshot does not, even with identical entries and overlaps.
Empty, re-tagged, incomplete or overlap-stripped replacements cannot substitute
for it. Multiple finalizations and Windows/Selected WSL members can share one
catalog authority. The snapshot and the authority bound to finalized members have
no public mutation or replacement API. With the correct authority, declared
overlap still returns `DeclaredCatalogOverlap`. Source binding remains the
finalizer's guarantee. This is a bounded synthetic gate decision, not production
catalog policy or a change to the frozen V1.7 documents.

ABF108D D2 clarifies A8b: both initial expected references and additional sources
preserved from a performed second enumeration participate in cross-scope namespace
validation, independently of the attempt/run reference. Repetition within one
member does not create a cross-scope conflict. Second discoveries remain outside
A8 run keys and both A9 populations; they create no declarations or records.

Successful memberships expose read-only `selection()`, `selected_refs()` and
`members()`. Members are canonical by kind (Windows, Selected WSL), environment
ref, family and layout (the harness is always OpenCode). No caller input order
breaks ties: A4 already excludes equal scopes. Private fields, no default or
unchecked conversion, and external compile-fail examples enforce validated-only
construction. Empty memberships are valid when selected refs match; unknown,
empty or unavailable inventories keep their original evidence and verdicts.

Deliberately deferred to G4B-2B: adapter M1–M13, project_with_companion,
PairedProjection, calls to abf_counting::project, G3B figures and adaptation
conservation. The counting dependency supplies only its accepted Selection type;
no projection is called. The existing crates remain independently usable.
Production readers, identity, clocks, filesystem and transport semantics remain
unapproved. No runtime P-R/P-F modes or physical-event deduplication exist.

Acceptance errata: R99g quarantines the two **f3** variants; it has no f5.
The defined boundary set is **R101a–R101f**; no R101g is invented.

Run offline from this standalone crate, reusing its ignored local target directory:

```sh
cargo fmt --check --manifest-path rust/crates/abf-source-run/Cargo.toml
cargo test --offline --locked --manifest-path rust/crates/abf-source-run/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-source-run/Cargo.toml --all-targets -- -D warnings
```

The gate's external coverage report records actual executable results and deferred
parts; a Rust test name alone is not evidence that every projection in a matrix row
was exercised.
