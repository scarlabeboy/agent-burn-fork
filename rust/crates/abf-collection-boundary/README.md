# abf-collection-boundary (ABF M1)

The first, deliberately narrow layer of an ABF collector: an explicit
collection scope, conservative (path-based, not race-free) path handling, and
observations restricted to allowlisted metadata fields, exercised through a
**synthetic fixture format only**. It is intended for **small, stable,
trusted synthetic fixture trees**.

It is **not** a Claude Code, Codex or OpenCode parser, not a complete
Windows/WSL collector, not a production-ready collector, and not a
deduplication or canonicalization engine.

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
prerequisites: practical output memory budgets, a read-coherence strategy and
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
  - every component is checked with `lstat` at check time, so a symbolic
    link present at that moment (file, directory or root ancestor) is
    refused;
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
  - after the read, the handle's identity and length must equal those seen
    at `open`, and the bytes read must equal that length.

  On mismatch the whole file is rejected (`ChangedDuringRead`, or
  `TooLarge` / `Path(HardLinked)`). **Not detected:**
  - growth or shrinkage between resolution and `open` (the length at
    resolution is not compared with the length at `open`);
  - same-size rewrites at any time;
  - mtime/ctime changes.

  The fixed-size header read may exceed a very small `max_file_bytes` before
  the file is rejected.
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
- **No practical memory budget for output.**
  - `max_entries` bounds directory iteration only.
  - `max_file_bytes` is a per-file input limit, not a batch memory budget.
    A single file within the limit can produce one record per line. Each
    record clones run, environment, source and path provenance, the pairs of
    every line are collected before validation, and all file reports of a
    batch are retained.
  - Retained memory can therefore be many times the input size. ABF9
    extrapolates about 3 GiB of record slots for one default-limit file of
    blank lines.
  - Resource exhaustion can abort the process rather than return a coded
    result.
  - M1 is for small, stable, trusted synthetic fixtures only. Record, pair,
    result and cumulative-byte budgets (or streaming), with coded
    deterministic exhaustion, are a production prerequisite.
- **No coherent read snapshot.** See the read-change checks above:
  - growth before `open` and same-size rewrites are not detected;
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
