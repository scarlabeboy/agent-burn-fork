# abf-collection-boundary (ABF M1)

The first, deliberately narrow layer of an ABF collector: an explicit
collection scope, conservative (path-based, not race-free) path handling, and
observations restricted to allowlisted metadata fields, exercised through a
**synthetic fixture format only**. It is intended for **small, stable,
trusted synthetic fixture trees**.

It is **not** a Claude Code, Codex or OpenCode parser, not a complete
Windows/WSL collector, not a production-ready collector, and not a
general deduplication or canonicalization engine. ABF140B adds only the optional
source-scoped exact-observation replay comparison documented below.

Status: implementation candidate from gate M1 (conversation ABF8, with the
ABF8 targeted security corrections). The contracts it follows are
ABF5/ABF5B/ABF5C design candidates. The approved D15 decision is noted
under Deferred.

**Production reuse: BLOCKED.** Filesystem containment is path-based and
**not race-free**: under concurrent filesystem changes, data from outside the
root can be read and emitted (see Known limitations). Production reuse
stays blocked until the production prerequisites below are met and
separately reviewed. Under Contract A V2 (AGENTS.md) handle-relative
containment is no longer an absolute prerequisite, but the link/reparse
no-follow policy, read-coherence behavior and documented residual limits must
be implemented and tested for the mechanisms actually used. Further production
prerequisites: a practical total memory budget, a reviewed read-coherence strategy and
an export/redaction policy. This note grants no production access.

## Build isolation

- std-only, `#![forbid(unsafe_code)]`, no dependencies, no build script.
- Standalone `[workspace]`, so it never re-resolves `rust/Cargo.lock` and
  never compiles upstream crates (`agent-burn`'s build script fetches pricing
  over the network).
- Does not link, call or spawn the upstream engine, the CLI, quota, OAuth or
  any process.

## Public API

| Item | Purpose |
|---|---|
| `OpaqueId` | Caller-supplied install-local id, `[A-Za-z0-9_-]{1,64}`. Never minted or derived here. |
| `EnvironmentIdentity::{windows, wsl}` | One environment: Windows host, or one WSL distro (`WslDistroName`). Equality includes kind and distro. |
| `Harness` | `ClaudeCode`, `Codex`, `OpenCode`. Separate from the model provider. |
| `SourceInstance` | One harness root in one environment (opaque id + harness). |
| `ScanRun` | Caller-chosen run id plus `CaptureTime` (the boundary reads no clock). |
| `AuthorizedRoot::new(path)` | Accepts only an absolute, canonical, existing directory that is not a symlink and has no symlinked ancestor. Records the directory's file-object identity. No `~` or variable expansion. |
| `check_root_text(text, HostFamily)` | Pure lexical root rules for Unix and Windows hosts. |
| `CollectionScope::new(env, source, root, HostPolicy)` | Fixes everything a collection may touch. `RequireNative` refuses an environment that is not the host's; `AllowSimulated` (synthetic use) records `HostMatch::Simulated` on every observation. |
| `CollectionLimits` + `CollectionScope::with_limits` | Per-file input size, total directory entries visited and path depth. Each value must be in `1..=` its ceiling (`MAX_FILE_BYTES_CEILING` = 256 MiB, `MAX_ENTRIES_CEILING` = 1,000,000, `MAX_DEPTH_CEILING` = 64). Otherwise `ScopeError::InvalidLimits`. These are **not** an output or batch memory budget (see Known limitations). |
| `collect_synthetic_file(scope, run, rel)` | One fixture by `/`-separated relative path. Path problems are returned as `Err` before any file is opened. Read and header failures return `Ok` with a rejected `FileStatus`. The root is revalidated before and after; on failure: `PathRejection::RootChanged`. |
| `collect_synthetic_root(scope, run)` | All fixtures below the root, in deterministic order. These conditions return an error instead of any batch: `RootUnreadable`, `RootChanged` (root revalidation failed before, during or after; everything read is discarded), `EntryLimitExceeded`. An `Ok` batch still has to be inspected: file statuses, record outcomes, pending tails, `Completeness`. |

### Output

- `CollectionBatch`: run, environment, source, host match, `files`
  (sorted), `skipped` (sorted, coded), explicit `Completeness` (traversal
  and read completeness only; see below). There is no API that merges
  batches.
- `FileReport`: relative path, file-object evidence (`dev`/`ino` on Unix),
  status, one `RecordOutcome` per newline-terminated line, pending tail
  length.
- `Observation`:
  - `occurrence` (physical locator): scan run, environment, source,
    relative path, file object, line ordinal, byte offset, byte length;
  - `claimed_ids` (message / request / session): what the record claims,
    kept apart from the occurrence and never used to merge;
  - `model_provider`, `model`, `event_time_raw`, `project_label`;
  - raw `usage` components;
  - `parse_status`;
  - `account_scope = NotCollected`;
  - `capture_time`;
  - `origin = SyntheticFixture { format_version, host_match }`.
- Every optional value is a `Field<T>`: `Value`, `Absent`,
  `Unparseable { byte_length }` or `NotCollected`. Never a silent zero.

## Checks performed

These are checks the implementation performs. Several are path-based and
hold only when nothing changes the tree concurrently; Known limitations lists
what they do not guarantee.

- **No implicit discovery.** The only way in is an explicit root. The source
  contains no environment-variable, home-directory, process or network access
  (guarded by a source-scan test).
- **No cross-OS reach.**
  - Unix rules reject `/mnt/<drive>` (default WSL drvfs mounts).
  - Windows rules reject `\\wsl$`, `\\wsl.localhost`, all UNC and device
    paths, drive-relative paths, reserved names, alternate data streams and
    trailing dots or spaces.
  - Nothing starts WSL.
- **Path checks against escaping the root** (path-based, not race-free):
  - relative paths with `..`, `.`, empty components, backslashes, `:` or
    absolute forms are refused;
  - every relative component is checked with `symlink_metadata` at check
    time, so a detected symbolic link is refused. On Windows, every detected
    reparse point is refused, including junctions; native execution is NOT_RUN.
    Root canonicalization checks also reject redirected ancestors;
  - hard-linked files (`nlink > 1`) and non-regular files seen at check time
    are refused.

  A component swapped after its check is not detected.
- **Root revalidation** (path-based, not race-free). Every collection
  rechecks the authorized root path: it must still be a non-symlink,
  canonical directory with the recorded file-object identity. If the root was
  replaced by a symlink or by another directory *before a recheck*, the
  collection fails closed (`RootChanged`) and emits nothing. It does not pin
  root ancestors, intermediate directories or the final open, and a swap that
  is restored between rechecks is not detected.
- **Bounded enumeration.** At most `max_entries + 1` directory entries are
  ever yielded to the walk, counted over the whole tree. A larger tree fails
  with `EntryLimitExceeded`, and no order-dependent subset is returned. This
  bounds **directory iteration only**, not the records later produced from
  the files that were found.
- **Read-change checks (limited; not a coherent snapshot):**
  - the length found at resolution must not exceed `max_file_bytes`;
  - after `open`, the handle must be a regular file and, on Unix, the same
    file object (`dev`/`ino`) as at resolution, with one hard link;
  - the read is capped at `max_file_bytes + 1` bytes;
  - length and modification time must match from resolution through open
    and the final handle check; failure to obtain modification time is a coded
    I/O rejection;
  - after the read, the handle's identity and length must equal those seen
    at `open`, and the bytes read must equal that length.

  On mismatch the whole file is rejected (`ChangedDuringRead`, or
  `TooLarge` / `Path(HardLinked)`). **Not detected:**
  - rewrites that preserve or restore checked metadata, including changes
    hidden by timestamp granularity;
  - ctime changes.

  Limits smaller than the header are rejected before opening the file.
- **Allowlisted metadata fields** (syntactic, not a privacy guarantee):
  - fixture keys are allowlisted;
  - prohibited keys (content, credential, account, pricing or quota) reject
    the record with a class code, and neither the key nor the value is kept;
  - identifier values must match `[A-Za-z0-9._:/@-]{1,128}` and event times
    `[0-9TZ:.+-]{1,40}`, otherwise only the byte length is kept;
  - diagnostics are enumerated codes.

  Values that pass these checks, and file names and relative paths (which
  are not charset-restricted beyond the path rules), can still contain
  sensitive information.
- **Nothing silently dropped.** Every newline-terminated line yields an
  accepted or a rejected outcome. Bytes after the last newline are reported
  as `pending_tail_bytes`. Skipped entries are listed, and skips that may hide
  data mark the batch `Incomplete`.
- **Completeness semantics (M1).** `Completeness` describes traversal and
  file reading, not usage.
  - `Complete` means: enumeration finished without a skip that may hide
    data, and every candidate fixture was read (`FileStatus::Read`) and split
    into newline-terminated records.
  - It is **not** proof that all usage was collected, parsed or counted. A
    `Complete` batch can contain rejected records, `Partial` observations and
    non-zero `pending_tail_bytes`, and can even have zero observations.
  - Rejected records, unparseable fields and pending tails stay visible and
    must never be interpreted as zero usage. Neither may `Incomplete` or a
    `CollectError`.
- **No dedup, no invention.** No cross-file, cross-source,
  cross-environment or cross-run merging. No deltas, normalization,
  pricing, quotas or accounts.

## Synthetic fixture format `abf-synthetic-fixture/1`

See the module docs in `src/synthetic.rs`. In short:

- the extension is `.abf-fixture`;
- the first line is exactly `abf-synthetic-fixture/1`;
- then one record per `\n`-terminated line, as tab-separated `key=value`
  pairs.

Real harness logs cannot parse as fixtures, and files without the extension
are never opened.

## Assumptions

- The future coordinator assigns unique opaque ids and keeps
  Windows/WSL environment ids distinct.
- `HostPolicy::AllowSimulated` is for synthetic fixtures only. Future
  production entry points must use `RequireNative`.
- A physical locator is meaningful only within its scan run (see Deferred).

## Deferred (not implemented, not to be assumed)

- Generation lineage and incremental scanning (ABF5C P0–P12, checkpoints,
  window fingerprints, verification scheduling). Until then, locators carry no
  cross-run identity.
- The detailed `record_ref` / record-reference hashing policy. D15 is
  decided as **K7**: ACL-protected plaintext SQLite, with no DPAPI-based K2
  architecture in V1. This crate implements no hashing and no persistence.
- Real harness adapters (Claude JSONL, Codex rollouts and counters `cx.v2`,
  OpenCode DB/JSON) and their privacy-preserving extraction.
- Event-time parsing and precision, usage normalization and overlap models.
- Identity relations, `id.v2` components, revision authority, counting,
  projections, the "Computer" union view.
- Out-of-process wire format and transport (D3/D4), serialization,
  hello/handshake.
- WSL distro GUID, Windows file ids and hard-link counts (unstable std APIs).
- SQLite persistence, quotas, pricing, accounts, Tauri UI.

## Known limitations

- **No race-free containment (production reuse BLOCKED).** All checks are
  path-based (`lstat`, `canonicalize`, `read_dir`, `open` by path), and none
  of them is race-free.
  - A concurrent swap of the root, an intermediate directory or the final
    file between a check and the following use is not prevented. This
    includes a swap that is restored before the next root recheck.
  - The independent ABF9 review showed this with an instrumented copy. An
    intermediate directory replaced by a symlink after its `lstat`, but
    before the final file's `lstat`/`open`, makes the boundary **read an
    outside file and emit its fixture metadata as accepted observations**,
    under an authorized-root relative locator, while root revalidation still
    passes.
  - A directory swapped during enumeration can also expose outside file
    names in `skipped`.
  - A file replaced by a FIFO between resolution and `open` may block the
    `open` before its type is checked. This is reasoned, not tested, and no
    limit bounds that wait.
  - Robust containment needs handle-relative, no-follow, type-safe traversal:
    `openat` with `O_NOFOLLOW` / `O_DIRECTORY` (and non-blocking opens) on
    Linux, handle-relative opens with reparse-point checks on Windows. That
    requires FFI/`unsafe` or a new dependency, outside the approved M1
    scope. Under Contract A V2 it is an optional hardening, not an absolute
    prerequisite; any production reading mechanism must still be implemented,
    tested and reviewed for what it actually does.
- **Bounded record output, not a total byte budget.** ABF129 caps retained
  outcomes at `synthetic::MAX_RECORD_OUTCOMES` (4096), shared across a root
  call or applied to one single-file call. Both accepted and rejected records
  count. A file exceeding the remaining budget is rejected as `OutputLimit`
  before any of its records are built; earlier files remain visible and the
  batch is `Incomplete`. Later files are still checked, including header-only
  files that require no record slots. No successful truncation is reported.
  - Input bytes remain bounded per file; directory entries and depth retain
    their existing limits. One file is read at a time.
  - Paths, file reports, skips, parser scratch allocations and allocator
    overhead are not covered by a practical cumulative byte budget. Large
    caller-selected entry/input limits can still exhaust memory.
  - Use small, trusted fictional fixtures. A practical total memory budget
    and allocation-failure handling remain production prerequisites.
- **No coherent read snapshot.** See the read-change checks above:
  - changes preserving the checked length and modification time are not detected;
  - a read can combine bytes from different moments;
  - the origin header identifies the format; it does not authenticate the
    file.

  A production read-coherence strategy and its tests are deferred.
- **Privacy is a property of collector-produced values, not of the
  types.**
  - The public output structs have mutable public fields; `non_exhaustive`
    blocks external construction but not mutation. Nothing re-validates
    values after collection.
  - Allowlisted scalars (ids, model, provider, project label), file names
    and relative paths can carry sensitive strings.
  - `Debug` output contains them and is **not** a safe export or support
    format.
  - Production redaction and export policy are deferred.
- **Windows paths:** behavior is compiled but not executed, and only the
  pure lexical rules are tested on Linux. Windows file identity and
  hard-link behavior are **unverified**. On Windows:
  - `FileObjectRef` is `Unavailable`, so the open-handle identity check and
    the root identity check are vacuous: replacing the root with another real
    directory is not detected;
  - hard links are not detected.
- **No cross-run generation identity:** not implemented. Locators are
  scoped to their scan run.
- **Custom WSL automount roots** (`wsl.conf`) are not detected. Only
  `/mnt/<drive>` is rejected.

## ABF129 controlled local reading

ABF129 reuses the existing executable fixture reader and existing public scope,
file reports and coverage codes. It adds the record-output cap above, stronger
metadata-change checks and standard-library Windows reparse-attribute refusal.
There is no new reader protocol, dependency, discovery or production integration.
Missing paths return `PathRejection::Io(NotFound)`; open/read failures return
`FileStatus::Rejected(Io(kind))`. Rejected files emit no records. Pending tails
and rejected/partial records retain the existing explicit reporting semantics;
`Read` never means complete usage. Diagnostics retain codes, not raw file bytes.

Focused offline validation (run from the repository root):

```sh
TMPDIR="$PWD/rust/crates/abf-collection-boundary/target/abf129-tmp" \
CARGO_TARGET_DIR="$PWD/rust/crates/abf-collection-boundary/target" \
cargo test --offline --locked \
  --manifest-path rust/crates/abf-collection-boundary/Cargo.toml \
  -- --skip hard_links_and_non_regular_files_are_refused
```

Create that disposable TMPDIR before running. All fictional files stay inside
this crate's target area and are removed by test fixture guards. The existing
Unix socket fixture test exceeds the Unix socket pathname limit with this
required in-crate TMPDIR; its setup fails before reader assertions. It remains
unchanged and is excluded from the final focused run. Native Windows compilation,
reparse behavior, Windows filesystem compatibility and WSL execution are NOT_RUN.
No real histories, credentials, network or external directories were accessed.
These tests do not prove a coherent snapshot, bounded wall time or production
readiness. Path swaps can still redirect a standard path-based open; replacing a
file with a FIFO can still block it. Locality checks reject explicit UNC/device
inputs, not every remotely backed filesystem before contact.

## Claude-shaped JSONL collection (ABF134D, fictional trees only)

`claude_jsonl::collect_claude_jsonl_root(scope, run, ClaudeReadLimits)` reads `*.jsonl`
files below an explicit `AuthorizedRoot` (source harness must be `ClaudeCode`) and hands each
LF-terminated line to `abf_parser::parse_claude_long`. It adds one sibling dependency,
`abf-parser`, and a small extension-parameterized enumeration/resolution in `scope.rs`; the
`.abf-fixture` path and its tests are unchanged. Tested only on disposable fictional trees under
`target/`; never run on provider histories, with no discovery, deduplication or persistence.

- **Result:** `ClaudeBatch` with per-file reports identified by sorted ordinal (no paths, no
  names, no content), file-object evidence, retained `ClaudeLine`s (line ordinal, byte offset
  and length, and an allowlisted `ClaudeObservation`, a parser rejection code, or `Oversized`),
  separate counts for observations, rejected, oversized and blank lines, and
  `pending_tail_bytes`. `observations()` yields claims with their full synthetic provenance;
  `totals()` sums the counts. Skipped entries keep only their reason.
- **Framing:** only complete LF-terminated lines are parsed; a tail without LF is counted, never
  parsed. Blank lines are counted, not retained. Lines are streamed in 64 KiB chunks; a line over
  `max_line_bytes` (ceiling 8 MiB) is discarded as it streams and reported `Oversized`, so
  buffering is bounded by the line limit, not the file size (allocation-tracked test).
- **Budgets (explicit, coded rejections, nothing truncated or zeroed):** scope depth and
  directory entries (`CollectionLimits`), per-file bytes (`max_file_bytes`), and
  `ClaudeReadLimits`: files (more candidates fail the batch, no subset), bytes per batch, line
  length, retained outcomes (exhaustion rejects the affected file as `OutputLimit`).
- **Refusals:** symlinks and detected reparse points (skipped), hard links and entries that
  stop being safe regular files (file `Rejected`), traversal and non-UTF-8 names. All keep the
  batch `Incomplete`. Wrong-extension files are out of scope, not incomplete.
- **Change detection:** identity, length and modification time are compared before, during and
  after the read; any change rejects the whole file as `ChangedDuringRead` and emits nothing from
  it. A live, growing file is therefore reported, not partially accepted; retry policy belongs to
  a later supervisor. Not a coherent snapshot: rewrites preserving the checked metadata, and swaps
  between a path check and the open, are not detected (Contract A V2 limits; path-based).
- **Completeness** is traversal and read coverage only. Rejected, oversized and blank lines and
  pending tails leave it `Complete` and must never be read as zero usage.

## Claude usage summary (ABF134E, fictional trees only)

`ClaudeBatch::summary()` (`claude_summary::summarize_claude_batch`) reduces one batch to
integers, enums and the caller's own opaque labels: no record, path, name, identifier,
timestamp or content, and a `Debug` that shows nothing input-derived. It is pure,
deterministic and never fails; every gap is a field.

- **Basis:** sums of *observed record claims*, fixed as `CountingBasis::ObservedRecordClaims`
  with `DuplicateIdentity::Unresolved`. Repeated records each count; nothing is deduplicated,
  and sums are neither unique physical events nor billed consumption.
- **Components:** input, output, cache creation and cache read are independent sums with no
  combined total. Cache creation is the parser's resolved claim: a valid breakdown alone, else
  the flat claim, never both (`records.cache_creation_flat_ignored` counts overridden flats).
- **Value outcomes** (names follow `abf-counting`, which has no Claude input and so is not a
  dependency): `ReportedClaimSum(u64)` (an explicit zero is a sum of 0), `NoValidClaims`,
  `Indeterminate(NoObservations)` for a batch with no observation, and `Overflow` for a
  checked-`u64` overflow (sticky for that component; never wrapped, saturated or zero).
  `states` partitions observations per component into valid / valid-zero / absent / null /
  wrong-type / out-of-range / unsupported-parent.
- **Coverage kept visible:** batch completeness, files read and rejected by reason, skipped
  entries (may-hide-data vs out-of-scope), parser rejections per code, oversized and blank
  lines, pending tails, and `Caveats` flags. Rejected and oversized lines, pending tails and a
  `Complete` batch coexist: a clean summary is never a completeness proof.
- **Scope:** one batch, one environment, one source. No Windows or Computer totals; summaries
  are not merged.

## Conservative Claude exact-observation replays (ABF140B)

`ClaudeBatch::exact_replay_summary(max_identity_observations)` returns
`claude_replay::ClaudeReplaySummary { raw, evaluation }`. `raw` is exactly the
existing `summary()` output; the collection, token rules, public raw summary,
demo golden payload and Windows IPC/dashboard payload remain unchanged.

`ReplayEvaluation::Evaluated(ReplayClaims)` carries the explicit basis
`SourceScopedExactObservationClaimsV1`. It compares only observations in this
one batch/environment/source/run. A replay means equal retained source-declared
identity and usage evidence, not equal raw JSON, authenticated UUIDs, proven
physical events or billed consumption. Content is never retained or compared.

Eligibility requires all of the following:

- Explicit `type: "assistant"` and `isSidechain: false`.
- Nonempty, at-most-256-byte UUID, session, request, message, model and timestamp
  strings. UUID and timestamp remain opaque claims; no authenticity or clock
  semantics are inferred. Missing/invalid fields are ineligible.
- Accepted parser status, no diagnostics, and valid input, output, cache-read
  and resolved cache-creation claims. Incomplete/qualified usage remains raw.
- Every observation related by an equal nonempty UUID or session/message pair
  must have identical full retained evidence and also be eligible. These
  relations can flag uncertainty only; neither establishes equivalence alone.
  Different request IDs, UUIDs, types, metadata, field states, flat/breakdown
  cache evidence or token values leave the affected group unresolved. A later
  conflict invalidates preceding exact copies too. No larger/later preference.

`eligible_observations` includes replay copies; `representatives` counts one per
eligible group; `replay_observations` counts its extra copies.
`eligible_observations + unresolved_observations == raw.records.observations`.
`components` uses the existing checked component accumulators on representatives
only. It is an **eligible subset**, never an all-source usage total. Its field
states describe that subset; original states remain in `raw`. No eligible
observations yields Indeterminate(NoObservations), not zero usage. Cache creation
uses the existing breakdown rule and never adds the flat claim to the breakdown.

`unresolved` reports overlapping reason counts: invalid identity, unsupported
record type, sidechain, invalid claims and conflicting evidence. Conflicting
evidence includes a related ineligible peer, not just differing token numbers.
Sidechains always remain unresolved; an otherwise eligible related parent is
also unresolved. Every raw record and claim stays in the batch and raw summary.
`raw` carries all file/line/traversal coverage and caveats. Evaluated only means
the bounded comparison ran; it never certifies complete coverage. Unknown records
or skipped lines may contain further duplicates or conflicts outside this view.

The caller can lower the identity observation budget to any value in 0..=4096.
The bound includes all observations, including ineligible ones, to prevent a
later unseen conflict from validating a prefix. Exceeding it returns
`IdentityBudgetExhausted { observations, limit }`, with no ReplayClaims payload;
a limit above 4096 returns InvalidIdentityLimit. Reservation failure returns
AllocationFailure; a non-Claude source returns WrongHarness. Raw coverage and
claims remain available on every outcome. No partial replay result is emitted.

Storage is one fallibly reserved vector of at most 4096 borrowed observation
pointers (32 KiB of requested pointer payload on x64); no identity strings or
observations are cloned, no persistent index is created. Allocator overhead and
the caller's existing batch are separate. The unchanged raw summary also copies
its bounded caller labels. Comparison is O(n^2) over bounded fields and at most
4096 observations; this is a bounded first slice, not an
incremental scanner or a throughput claim. The original file/record/parser and
collection limits are unchanged. The larger parser result layout/payload must
be accounted for by callers using lowered parser result budgets (parser README).

Upstream reference: `agent-burn/src/adapter/claude/mod.rs` uses
`push_deduped_entry`, `loaded_entry_matches_dedupe_key`,
`loaded_entry_matches_sidechain_dedupe_key` and `should_replace_deduped_entry`;
`claude/daily.rs` has `push_deduped_daily_entry`. Their message/request and
sidechain observations informed candidate-conflict detection. Their parent,
larger-token-total, speed and cost replacement preferences are not adopted.
ABF reuses its existing parsers and summary accumulators. No upstream adapter,
pricing, quota, persistence, discovery, Windows or WSL integration is added.

### Bounded conflict attribution (ABF142)

Evaluated claims also carry `conflicts: ConflictCounts`. Each counter counts an
observation once if at least one **conflicting** peer satisfies its condition.
The ABF140B `related && (!eligible(peer) || observation != peer)` decision remains
authoritative, excluding self-comparisons. Attribution does not classify a
second set of conflicts or change eligibility, representatives, replay copies,
unresolved counts, raw summaries or token accounting.

- `ineligible_peer`: at least one conflicting peer fails the existing eligibility
  checks. This is directional: an ineligible observation with only eligible peers
  does not itself contribute here. Identical related ineligible peers can conflict
  without any retained-field difference.
- `identity_metadata`: retained identity object (type, UUID, sidechain), caller
  correlation, session, request, timestamp, message container state, message ID or
  model differs, including field states.
- `token_fields`: usage container state, input, output, cache-read, flat cache
  creation, cache-creation container state or either cache-breakdown child differs,
  including field states. Comparison retains flat/breakdown evidence even when
  resolved token totals match or a flat claim is ignored for summation.
- `parser_evidence`: parser status or diagnostic bits differ. For example, an
  unknown token key can change parser evidence without changing retained token
  fields. This is not proof of a provider lifecycle or a specific invalid field.
- `uuid_related`: at least one conflicting peer has the same valid bounded UUID.
- `session_message_related`: at least one conflicting peer has the same valid
  bounded session/message pair.

Causes and relations overlap within and across peers. Each counter is at most
`unresolved.conflicting_evidence`; the first four together cover that count,
and the last two together cover it. Their sums are not observation, unique-pair,
physical-event or group counts. Zero counters do not certify complete coverage.
No per-record attribution or retained values are exported. All peers are visited
instead of stopping at the first conflict; time remains bounded O(n^2), with six
boolean flags per observation being evaluated and six aggregate integer counters,
no additional heap allocation and unchanged collection/identity budgets.

### Unconfirmed metadata-only usage candidates (ABF143)

Evaluated claims additionally carry `candidates: CandidateCounts`. This diagnostic
reuses the ABF142 peer scan and retained-field differences. It examines only
conflicting pairs with differing metadata, identical retained token fields
(including container/field states, flat cache claims and breakdown children),
and identical parser status/diagnostics. Exact copies and token/parser-differing
pairs do not contribute. A pair falls into one of these categories:

- `matching_request`: both observations pass existing individual ABF140B
  eligibility, with matching session, message, request and model evidence,
  caller correlation and message-container state. Only valid UUID and timestamp
  claims may differ. These are **unconfirmed candidate repeated usage claims**.
- `request_mismatch`: the same conditions except valid request IDs differ.
  This is unresolved reuse, not a replay or a sidechain inference.
- `missing_identity`: either observation fails the existing identity validity
  checks, including missing, invalid, empty or over-256-byte evidence. Invalid
  identity that also changes parser evidence is outside this metadata-only scope
  and remains explained by the existing parser-evidence counter.
- `other_metadata`: remaining metadata-only conflicting pairs, including model
  differences, sidechains, unsupported record types or incomplete claims.

Each counter counts an observation once if it has at least one qualifying peer.
Categories are exclusive per pair, but overlap across peers. They are not a
partition of conflicts, pair counts, group counts, or a count of extra copies.
A candidate can also have token-differing or ineligible peers; candidate matches
never override those conflicts. No transitive identity, lifecycle, supersession,
later/larger preference, uniqueness, billing or original provenance is inferred.
UUID and timestamp differences are metadata differences, never replay proof.

No accounting changes: raw sums, exact-replay eligibility and subset components,
conflict counts and coverage remain authoritative and unchanged. No observations
are discarded. The diagnostic allocates no additional collection, parses no
records again, and retains existing batch, comparison and parser budgets.
