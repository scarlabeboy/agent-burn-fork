# ABF observation research (G2A)

Synthetic-only, standalone library experiment. Requires independent audit before
acceptance. This is not a production observation schema or a production privacy
boundary. No root workspace, existing crate, Tauri or engine integration.

## Contract

`from_opencode(ParseResult, DeclaredContext, SourceRunStatus)` moves one bounded
parser result into an immutable `ObservationEnvelope`. It never accepts JSON,
invokes parsing, reads files, consults environment variables or runs a reader.
The only dependency is the local, exact-version `abf-parser = 0.1.0` crate.
`ParserId::AbfOpenCodeV0_1_0` identifies that parser implementation, not an
OpenCode upstream schema version. `Harness::OpenCode` is independent of the
record's model provider. No other adapter is implemented.

The original `ParseResult` remains directly inspectable by reference. Every
`Field<T>` state, container state, raw signed `time.created`, independent input,
output, cache read/write and total claim, bounded metadata, parser correlation,
status and diagnostic/rejection code remains unchanged. Nothing substitutes
zero, computes a sum, checks total consistency, infers output or changes status.
A rejection has no numeric observation. Publicly constructed parser structs are
budget-checked but their semantic claims are not reinterpreted or certified.
Adapter budget errors are separate code-only failures, never replacement parser
outcomes. The adapter consumes its arguments, including on budget failure.

`DeclaredContext` is caller-declared synthetic context, never filesystem-attested
provenance. It holds Windows or Selected WSL, an opaque environment reference,
source reference, run reference, optional correlation and optional zero-based
`u64` ordinal. The latter is not a physical locator; zero and `u64::MAX` are valid
claims. Absent correlation and ordinal stay absent. Context correlation and
parser correlation are separate claims, with no inferred relationship.
There is no Computer source, distro name, account, path, project or clock field.
References and session/message IDs establish no physical identity, authenticity,
uniqueness or cross-environment equivalence. Equality compares values only;
separate calls remain separate occurrences, including identical claims.

## Independent qualifications

`SourceRunStatus` is supplied independently by the caller/future reader:

| Status | Declared meaning |
| --- | --- |
| NotCollected | No reader attempt reported |
| Complete | Traversal reported complete, including records that fail parsing |
| Partial | Partial traversal reported |
| Unavailable | Access unavailable, no traversal reported |
| Failed | Reader failure reported, possibly after earlier records |

Accepted never implies Complete. Complete never means every record parsed
successfully or every usage component was reported. Partial and Failed evidence
remains qualified by its source status even when its parser result is Accepted.
A record result paired with NotCollected or Unavailable is retained with
`Inconsistency::RecordWithoutReportedTraversal`; rejection counts as record
evidence too. Source status and parser outcome are never silently repaired.
The inconsistency accessor computes only this fixed local contradiction, with
no source-wide validation. Its absence is not a completeness certificate.
There is no convenience API for complete usage, numeric usage, batch success,
source-wide validity or identity. Consumers must inspect both dimensions and
inconsistency. No sequence or run consistency engine is implemented.

## Bounds and privacy

All four possible context strings are validated before allocation as
`[A-Za-z0-9_-]{1,64}` (the earlier research boundary's practical ceiling).
Empty, oversized and malformed references return distinct fixed enum codes;
length is checked before scanning. No user text appears in errors. Ordinals,
environment, harness, parser identification and statuses have fixed sizes.
Parser strings retain the parser ceiling of 256 bytes each (five strings,
maximum 1,280 payload bytes). Checked retained-size accounting rejects parser
results exceeding 4,096 bytes; this is defensive for public constructed types.
On the tested target, envelope inline size + 1,280 parser payload + 256 context
payload is at most 4,096 bytes. Allocator overhead and caller-owned inputs are
excluded. No growing batch, diagnostic list or runtime global state exists.
Validation does bounded scans without cloning the parser result. Test-only
clones do not describe runtime adaptation.

Envelope, context and opaque-reference Debug implementations redact all values.
Explicit accessors expose bounded claims for later processing. The upstream
parser result and its Observation still have metadata-revealing Debug: do not
log those references. Character validation cannot recognize secrets disguised
as identifiers; bounded parser metadata can likewise contain sensitive content.
Use fictional inputs only. This is not a comprehensive sandbox or export policy.
No raw frame field, arbitrary diagnostic text, prompt, transcript or credential
field is introduced; no runtime logging, I/O, networking, processes or persistence
is implemented. The existing parser validates/discards unknown JSON content
before this adapter receives its result.

## Exclusions and deferred decisions

No normalized totals, cost, buckets, counts, deduplication, identity relations,
equivalenceId, merging, Computer aggregation, SQLite, cursors, discovery, reader
execution, scheduling, freshness or recovery. M1/M3 research crates are reference
material only, not dependencies or accepted production contracts. No collection,
WSL launch, real histories or installed harness information is authorized.
Production schemas, identity rules, normalization, run-wide validation, reader
contracts, storage and export policy require later approval. The two accepted
G1B MINOR findings remain open and untouched.

## Validation

Use the standalone manifest and existing toolchain in a verified network
namespace, isolated Cargo home, target and temporary directories. Generate the
lockfile offline; then use `--offline --locked` for dependency tree, checks,
tests and Clippy, plus `cargo fmt --all -- --check`. No external packages or
build scripts are needed. Tests use only fictional in-memory inputs. ABF98's
report under `/tmp/abf98-g2a-synthetic-observation/report.md` records exact
commands and counts. Native Windows, WSL transport, real data and Tauri are
NOT_RUN. No audit, acceptance, publication or durable backup is implied.
