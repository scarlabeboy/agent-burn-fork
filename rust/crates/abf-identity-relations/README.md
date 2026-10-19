# Synthetic occurrence and identity-relation research (G2B)

Standalone experimental Rust library; fictional in-memory observations only.
Requires independent audit. This is an evidence representation boundary, not an
approved production identity policy. No root workspace or application integration.

## Occurrences

`SyntheticSet::new(Limits)` creates an empty bounded set. `Occurrence::new` takes
an existing `OpaqueRef` and `ObservationEnvelope` by value; `set.insert` moves the
whole occurrence into insertion-ordered storage. No parser result or declared
context is rebuilt, translated, cloned, normalized or revalidated. The envelope
is already bounded by the existing observation adapter. Parser rejection codes,
all original fields/diagnostics/statuses, declared Windows or Selected WSL context,
source-run status and inconsistency information remain accessible unchanged.

The handle is **only a local reference within this set**. It is not a verified
physical event ID, filesystem identity, persistent database key or certificate of
cross-environment equivalence. `OpaqueRef` uses the existing ASCII
`[A-Za-z0-9_-]{1,64}` validation and fixed `ContextCode` errors. Handles in different
sets have no established relationship. Syntax validation cannot recognize secrets.

Duplicate local handles return `Code::DuplicateHandle`; they never replace the
first occurrence. Different handles are accepted even with identical parser
results, message/session IDs, source/environment labels, timestamps, models or
token values. Accepted/Complete never establishes event uniqueness. Windows and
Selected WSL remain separate declared environments; Computer is not a source.
The slice length is a storage occurrence count, never a unique-event count.

Failed occurrence insertion returns only a fixed `Code`, without payload or
source text. The candidate argument is consumed on success or failure; a rejected
candidate is dropped, not accepted or retained. All occurrences and assertions
already stored in the set remain unchanged. Callers needing a retry must retain
appropriate caller-owned evidence before making a consuming call.

## Explicit assertions

`set.assert_relation(from, to, UnverifiedHint)` accepts only two different handles
already present in this set. No assertion is generated automatically. Categories:

| UnverifiedHint | Caller assertion, never checked |
| --- | --- |
| MatchingClaimedIdentifiers | Claimed identifier text matches |
| DifferentClaimedIdentifiers | Claimed identifier text differs |
| EqualReportedValues | Reported values are equal |
| PossibleReplayOrCopy | An observation might be replayed or copied |
| Unclassified | Caller has not classified the relation |

The type name **UnverifiedHint** applies to every variant. The first two do not
specify which identifiers the caller meant. No field selector, equality check or
proof is included. Even a demonstrably false or conflicting hint is retained as
an unverified assertion; this library does not certify equal identifier text.
Equal claimed identifier text, if checked in a future gate, would still not
establish that two physical events are the same. `verification()` can return only
`Verification::Unverified`. No `ProvenSamePhysicalEvent` state exists.

Endpoints are stored in caller-supplied order. Reversing them produces a separate
entry, without implying causality or a directional physical identity relation.
Every repeated, reversed or conflicting assertion is appended and consumes one
slot. Nothing is silently collapsed. Conflicting Matching/Different assertions
may coexist. Relations can involve rejected records and can cross environments;
neither case gains identity authority. No graph traversal, closure or inference
occurs. A chain A-B and B-C does not create A-C. Assertions never mutate or remove
occurrences, and are never used to calculate unique-event or usage counts.

The API exposes immutable slices and shared references, with no occurrence
removal/replacement or endpoint mutation, so stored endpoints cannot become
dangling. Endpoint values are consumed even on assertion failure; only these
bounded arguments are dropped. Existing occurrences and assertions stay intact.

## Limits, failure atomicity and allocation reasoning

Callers may lower ceilings, including to zero, but cannot raise them:

| Budget | Hard ceiling |
| --- | --- |
| Occurrences in one set | 128 |
| Assertions in one set, including repeats | 256 |
| Each opaque handle or context reference | 64 bytes |
| Each parser metadata/correlation string | 256 bytes |
| Retained parser result accounting | 4,096 bytes |
| Upstream input frame, before parsing | 65,536 bytes |

The new set accepts envelopes, not frames; parser frame/depth/entry limits apply
upstream. Observation-adapter payload checks remain unchanged. Limits above the
hard ceilings, including `usize::MAX`, return `InvalidLimits` before allocation.

Validation order is deterministic. Occurrence insertion checks duplicate handle,
then checked `len + 1` against capacity, then `try_reserve_exact(1)`. Assertion
insertion checks self-link (`SelfLink`), missing endpoint (`MissingEndpoint`),
checked capacity (`RelationCapacity`), then `try_reserve_exact(1)`. Occurrence
capacity failure is `OccurrenceCapacity`; reservation failure is
`AllocationFailure`. Only after all fallible work succeeds is a value pushed.
There is no partial append, overwrite or loss of existing data. Duplicate/self/
missing checks precede capacity errors even when storage is full. Reservation
failure handling is implemented, but allocator failure injection is not tested.

Resource accounting separates the following:

- **Payloads:** an occurrence owns at most five 256-byte parser strings, four
  64-byte context strings and one 64-byte local handle: at most 1,600 owned string
  payload bytes, in addition to its inline Rust layout. An assertion owns two
  endpoint reference strings, at most 128 payload bytes plus its inline layout.
  At both hard ceilings this is at most 237,568 string payload bytes. The parser's
  4,096-byte accounting includes its inline layout; do not add it again as another
  string payload. Fixed diagnostic codes cannot grow.
- **Collection-owned allocations:** two vectors, initially empty. Per accepted
  insertion, request only the additional capacity needed for one element using
  `try_reserve_exact(1)`, without geometric over-reservation by this component.
  Requested inline vector storage grows with accepted occurrences/assertions;
  string buffers are moved from caller ownership, not copied. There is no global
  registry, cache or auxiliary graph index.
- **Temporary allocations:** no new strings or temporary collections are created
  during insertion/validation. A vector reallocation can temporarily retain old
  and new inline buffers, conservatively twice requested vector storage.
  Allocators may round reservations upward; allocator bookkeeping/rounding and
  stack/runtime overhead are separate. Validation scans at most 128 handles
  (64 bytes each); assertion insertion makes at most two such endpoint searches.
- **Caller-owned values:** parser input frames, parser/adaptation temporaries,
  pending occurrences/endpoints and any caller
  snapshots are outside set storage. They need their own lifetime/budget policy.
  The upstream parser README describes its separate temporary decoding budget.
- **Clone amplification:** new data-bearing collection/occurrence/assertion
  types do not implement Clone. This library never clones an envelope or handle.
  Explicit accessors expose upstream Clone-capable data, so callers can still
  amplify memory by copying it. Tests use bounded snapshots as test evidence;
  these are not runtime set allocations.

These are structural/requested-allocation bounds, **not an exact RSS bound**.
No allocator instrumentation or RSS measurement has been performed. Repeated
exact vector growth can copy O(n²) inline bytes across the lifetime of a set;
research ceilings keep this finite. No performance benchmark is claimed.

## Privacy and capabilities

Runtime code uses in-memory standard-library operations plus the committed local
`abf-observation` and its `abf-parser` dependency. `abf-parser` is also a direct
local dev dependency for fictional fixtures. No third-party package, build script,
binary, engine or Tauri dependency. Unsafe code is forbidden. No filesystem,
network, process, credentials, account, environment, clock or database access;
no runtime logging or arbitrary user-controlled diagnostic text.

Debug for new data-bearing types uses the existing redacted envelope/OpaqueRef
Debug implementations. Errors print fixed codes only. Explicit accessors expose
preserved claims; they are not sanitized for export or logging. In particular,
the upstream parser-result Debug **does expose metadata**. This is synthetic-only
research, not a comprehensive export sandbox or a production privacy boundary.

## Exclusions and deferred decisions

No canonical event IDs, proven physical identity, trusted attestation, transitive
identity closure, union-find, connected-component merging, deduplication, Computer/
environment/session totals, numeric aggregation, token normalization, pricing,
quotas, discovery, incremental reading, persistence/SQLite, capture-time
attestation, Windows/WSL filesystem transport or automatic WSL launch. No
synthetic equivalenceId convention is promoted to production. Earlier research
contracts are references, not approved production dependencies or policies.
Production identity proofs, counting policy, cross-environment identity,
schema/storage/transport and export policy require separately approved gates.
G1B/G2A accepted findings are untouched.

## Validation

Tests exercise the actual collection API with fictional in-memory fixtures:
independent equal occurrences, matching claims, Windows/Selected WSL, rejected
records, every source status, original parser fields/diagnostics and owned-buffer
addresses, handle errors, atomic failures, unverified false/cross-environment/
repeated/reversed/conflicting hints, deterministic sequences, lowered/zero/hard
ceilings, maximum payloads with both ceilings, generated invalid references and
redacted Debug/error output.

Use the standalone manifest, existing toolchain, verified isolated network
namespace, isolated Cargo home/target/temp and offline dependency resolution.
After offline lock generation, dependency tree/check/test/Clippy use
`--offline --locked`; cargo-fmt does not expose those switches, so it uses the
same network isolation and `CARGO_NET_OFFLINE=true`. Supply an explicit isolated
rustfmt config to avoid home-config discovery. ABF100's report at
`/tmp/abf100-g2b-identity-relations/report.md` records exact commands and counts.
Native Windows, WSL transport, real histories and Tauri tests are NOT_RUN and
unauthorized. No production readiness, independent audit or durable backup is
implied by successful local validation.
