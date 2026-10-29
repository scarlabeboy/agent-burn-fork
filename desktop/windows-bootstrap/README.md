# ABF Windows Bootstrap

An isolated, unpublished Windows x64 Tauri v2 host candidate with a bundled React,
TypeScript and Vite frontend. Its dashboard displays fictional and synthetic demonstrations;
a separate manual diagnostic can read one explicitly configured Windows JSONL copy.
Windows and Selected WSL are distinct display fixtures; Computer is a derived
view, never a collection source. Dashboard totals remain synthetic. The copy reader
reports observed record claims separately, with no account, quota or cost integration.

This workspace has its own pnpm workspace and locks and a standalone Rust
workspace. It does not import the production CLI or macOS application. The fictional Claude
panels use the isolated collection boundary and parser research crates. React is bundled into local static assets; the future
native executable will not need Node, pnpm or a production web server.

## Bounded frontend validation

Use Node 24.19.0 and a reviewed pnpm 11.1.1 copy. Run only from this directory,
with an explicitly authorized isolated store, empty npm user/global configuration,
and caches/compiler temp under the gate evidence root. Do not use root recipes,
Corepack/npx downloads or dependency lifecycle scripts.

```sh
pnpm install --offline --frozen-lockfile --ignore-scripts --store-dir <approved-store>
pnpm run typecheck
pnpm test
pnpm run build
```

The test script uses Node's --test-isolation=none so all node:test cases
are registered and counted under the supplied Node/tsx pair. Tests run in a
separate command process and use only synthetic render inputs.

The build writes only this workspace's dist/. There is no dev-server script.
The official Tauri JS API is pinned to 2.12.3; no Tauri CLI package is installed.
All frontend dependencies, including the bundled React runtime, are pinned in
devDependencies. The reviewed esbuild
optional platform package allows the tests to run without its postinstall script.
Do not enable scripts or allow missing-binary fallback downloads.

## Host and runtime boundary

Identity: dev.scarlabeboy.abf.windowsbootstrap; executable:
abf-win-host-bootstrap.exe. Installer bundling is disabled. The dedicated neutral
32x32 ICO is locally generated and explicitly configured for Windows resources.
M5B-R2 authorized it as the sole addition to the original 22-file source inventory.

Only ABF_BOOTSTRAP_RUNTIME_ROOT selects the profile in application code. A future,
separately authorized Windows run must provide a fresh, dedicated absolute local
drive path below the drive root (for example C:\ABF-Isolated-Run). Missing,
relative, UNC, device, bare drive-root and parent-traversal paths are rejected
before window creation. These are lexical checks only: they do not prove that
the selected root is fresh, safe, local or isolated. The fixed child webview2 is
passed to Tauri's data_directory API. Application code has no discovery or
HOME/LOCALAPPDATA fallback. Only the manual ABF136 diagnostic described below
explicitly creates a fixed test child and fictional file. Tauri may
recursively create missing profile directories, and WebView2 may create directories
and write profile/cache data. A missing or mistyped root is not refused merely
because it does not exist.

The separately authorized native gate must check actual profile placement,
aliases (including junctions/symlinks, SUBST, 8.3 names, trailing dots/spaces and
mapped drives), permissions, environment/registry overrides and coexistence.
Lexical acceptance is not filesystem-isolation proof. Never point this variable
at an existing Agent Burn installation or real user data.

The synthetic application command, `abf_get_synthetic_snapshot`, takes
no arguments and returns fixed synthetic data. Windows and Selected WSL are its
only source entries; Computer remains a derived view. The historical M6D schema v1
returned null-only placeholders. M6E explicitly evolves the response to schema v2
with 12 deterministic demo records across Claude Code, Codex and OpenCode, three
fictional demo models and fixed UTC day buckets (2026-01-01 through 2026-01-03).
This is a DEMO TRANSPORT FIXTURE, not an approved production event/database schema.

Tokens are input + output. Windows totals 235 tokens / 5 sessions; Selected WSL
totals 330 / 4. Computer totals 550 / 8 across 11 logical occurrences. Only the
explicit `demo-copy` equivalence pair (one record per source, identical harness,
model, day and token counts) is counted once. Source/harness/sessionId identifies
a local synthetic session; the equivalence pair joins its containing sessions
for Computer. Matching session labels, dates, models or tokens alone never join
records or sessions. Original record identities and both source provenances are
retained. These are fixture-only conventions, not production identity rules.

React validates exact v2 fields, demo identities, calendar dates, unique record
IDs, equivalence consistency, at most 100 records and at most 1,000,000 tokens
per input/output field before deterministic aggregation. Empty, failed or invalid
data stays unavailable. One cached invocation serves all 12 filter views without
polling or retries. Visible DEMO / SYNTHETIC metrics, UTC activity meters, model
and General harness breakdowns describe demo data only. Estimated API cost,
actual billed cost, subscription-equivalent value, quotas and resets remain
unavailable; quota remains provider/account scoped.

There are no real collectors, provider-history or credential reads, account
discovery, observation persistence, background workers, plugins, updater, tray
or autostart. Installer bundling remains disabled.

The capability list is empty and no capability directory or files are present;
the directory-absence regression guard does not prove the complete permission
model. Production CSP permits packaged scripts/styles and only the required
local Tauri IPC transport (`ipc:` and `http://ipc.localhost`) for connections.
External frontend network connections remain denied; no external network access
is implemented. Native navigation remains restricted to the packaged origin,
and new-window requests are denied. Synthetic IPC does not activate collection.

## Evidence and limits

Historical M5B-R2 evidence is under /tmp/abf47c-m5b-react-bootstrap/ in the validation
environment. It records reused registry provenance, exact commands and source
hashes. The source snapshot includes both genuine locks and the binary ICO.
Frontend typechecks, server-render tests, production build and static checks are
the bounded acceptance scope of that historical gate. It does not validate the
current M6E demo bridge. For M6D and M6E, native Windows compilation, WebView2 IPC,
rendering, CSP enforcement, profile isolation, OS compatibility and coexistence
are NOT_RUN. No M6D/M6E native IPC operation or production integration is claimed. Native
compilation or startup requires a separate gate; do not run cargo check, tauri
dev/build, native installers or a development server as part of these checks.

## Fictional Claude usage summary (ABF134F)

The existing argument-free demo command, `abf_get_fictional_claude_summary`, returns one JSON string
(schema version 1) that the frontend validates strictly and renders in a clearly labelled
panel for **Selected WSL / Claude Code only**; every other view is unchanged and an absent or
invalid payload shows nothing in its place. It is **fictional**: the Rust command calls
`abf_collection_boundary::claude_demo::fictional_demo_payload_json()`, which runs the real
streaming collector, parser and summary over two checked-in fictional JSONL files
(`rust/crates/abf-collection-boundary/fixtures/fictional-claude-demo/`, embedded at compile
time). Nothing is read from disk, environment or any provider location at run time, no path or
root is accepted, and the payload carries no identifier, label, name, timestamp, model or
content. Its provenance is the fixed `fictional-fixture` and its host match must be
`simulated`; a real-data payload would need its own provenance and approval.

The panel shows four separate observed-claim sums (input, output, cache creation, cache read;
sums are decimal strings so no `u64` loses precision), per-component counts of valid, explicit
zero, absent, null, invalid and parent-unavailable claims, record counts, the observed-claim
counting basis with unresolved duplicate identity, parser rejections, oversized lines, pending
tails, rejected files, skipped entries and incomplete coverage. A sum of 0, no valid claims,
no observations and overflow are four distinct displays. It is never merged into the DEMO
metrics, never a Windows or Computer total, and has no cost, quota or billing figure.

The Rust crate now has one local path dependency (`abf-collection-boundary`, plus its sibling
`abf-parser`, both std-only); `src-tauri/Cargo.lock` gained exactly those two path packages and
one dependency edge, edited by hand because the Tauri registry packages are not available
offline. `tests/fictionalClaude.test.tsx` reads the Rust-generated golden payload
(`expected-payload.json`, asserted byte-for-byte by the Rust suite). Native Windows compilation
of the new command, IPC and rendering are NOT_RUN.


## Manual fictional file-read diagnostic (ABF136)

Selected WSL / Claude Code also has **Run fictional file-read once**. This invokes
the argument-free `abf_read_fictional_claude_file_once` command only on a manual click.
It copies the fixed checked-in `fixtures/abf136-fictional.jsonl` bytes into
`ABF_BOOTSTRAP_RUNTIME_ROOT/abf136-fictional-claude-once/fictional.jsonl`, then
calls ABF135's `collect_claude_jsonl_file()` on that single file and the existing
Rust summary pipeline. There is no directory enumeration or provider discovery.

The root must already exist at diagnostic time. Before filesystem contact the
diagnostic applies the collection boundary's root syntax checks. It checks all
ancestors for symlinks and, on Windows, reparse attributes, and requires the
boundary's canonical authorized root. It creates the fixed child exclusively
with `create_dir`, checks it again, and creates the file with `create_new`.
Any existing child, including a directory, file or dangling link, refuses the
attempt. There is no overwrite, reuse, fallback directory, cleanup or deletion.
Writes and sync errors refuse the attempt. Both success and failure consume the
native atomic one-shot guard; simultaneous requests cannot create multiple reads.
The root is captured from the required startup configuration, never from IPC.

Budgets are one file, 8192 bytes per file and batch, 2048 bytes per line, 16 retained
outcomes, one directory entry and depth one. The file has separate fictional claim
sums of 17 input, 3 output, 2 cache creation and 4 cache read tokens, an explicit
input zero, a null output, absent cache claims, a malformed line and a pending tail.
These gaps are visible; complete reading is not complete usage. Rejected files
retain coverage codes and indeterminate claims, never invented zero consumption.

Only the bootstrap-local, private schema v1 projection crosses IPC: numerical
claims, coverage diagnostics and fixed fictional/simulated enum strings. Paths,
file contents, identities, models and labels do not reach React. The existing
fictional demo command and its private serializer remain unchanged. The diagnostic
uses the existing strict frontend validator and summary panel with distinct IDs.
It never contributes to synthetic dashboard or environment totals.

The diagnostic's state is separate and has no mount-time read, timer, retry or
polling. **Clear diagnostic result** clears only UI memory, suppresses late
in-flight results and leaves the attempt consumed. It never deletes files or
rearms collection. Another attempt requires an app restart with a fresh isolated
runtime root. No arbitrary path, account, credential, home, history, network or
WSL access is implemented.

Checks remain path-based under Contract A V2, not atomic confinement or a coherent
snapshot against concurrent path swaps or metadata-preserving rewrites. Windows
file identity/hard-link limitations and blocking filesystem-call limits of the
existing reader still apply. Use only a trusted, isolated fictional test tree.
This slice grants no production reader authority or native WSL provenance.

Local offline checks, with existing dependencies (no install or downloads):

```sh
# Repository root; standalone harness avoids compiling or running Tauri.
cargo test --offline --locked --manifest-path desktop/windows-bootstrap/diagnostic-tests/Cargo.toml
cargo test --offline --locked --manifest-path rust/crates/abf-collection-boundary/Cargo.toml --test claude_jsonl
cargo test --offline --locked --manifest-path rust/crates/abf-collection-boundary/Cargo.toml --test claude_demo
# desktop/windows-bootstrap; use the already installed local Node tools.
node --import tsx --test --test-isolation=none tests/App.test.tsx tests/syntheticAggregation.test.ts tests/fictionalClaude.test.tsx tests/fictionalFileRead.test.tsx tests/claudeCopy.test.tsx
node_modules/.bin/tsc --noEmit -p tsconfig.app.json
node_modules/.bin/tsc --noEmit -p tsconfig.node.json
node_modules/.bin/tsc --noEmit -p tsconfig.test.json
node_modules/.bin/vite build --config vite.config.ts
```

The standalone harness compiles the actual diagnostic helper with only the two
existing local Rust dependencies. Disposable trees are under its ignored target/.
Linux tests simulate Windows, while the Windows application simulates WSL; the
golden comparison explicitly adjusts only that environment-kind enum. The
checked-in sanitized golden payload is also validated and rendered by frontend
tests. This is synthetic Linux evidence, not native Windows evidence.

Native Windows compilation/run, Windows reparse execution, WebView2 IPC, visual
interaction, profile isolation, OS compatibility, coexistence and WSL execution
are NOT_RUN. CI and a separately authorized Windows visual check remain separate.


## Explicit Windows Claude copy diagnostic (ABF137)

Windows / Claude Code has a separate **Read selected copy once** action and
**Clear copy result** action. Its result never enters dashboard totals, historical
fictional panels, environment aggregation or Computer. This is a diagnostic-local
contract, not a production event or persistence schema.

`ABF_BOOTSTRAP_CLAUDE_COPY_ROOT` is optional and independent of
`ABF_BOOTSTRAP_RUNTIME_ROOT`. It is captured once at process startup without
filesystem access. No root is supplied over IPC, discovered or defaulted from home,
Claude settings, WSL, accounts or providers. If absent, a read returns
`COPY_NOT_CONFIGURED`. The configured copy tree must already exist; the reader
performs no creation, modification, deletion or persistence. ABF137 development
and tests use only repository fixtures and disposable synthetic roots. No actual
history read was enabled or exercised. Setting a real-history root or running the
Windows application still requires separate user authorization.

Public IPC contract:

```text
abf_read_claude_copy_once({ relativePath: string }) -> Promise<string>
```

The success string is JSON, bounded to 16384 characters by the frontend. The path
is caller-selected, ASCII, at most 512 bytes, `/`-separated, at most four components,
and ends in `.jsonl`. Absolute paths, traversal, empty components, backslashes,
alternate streams, reserved Windows names, wildcards and trailing dots/spaces are
refused. The configured root is an ASCII absolute drive path below the drive root,
with the existing boundary's canonical-path requirements. UNC, device and WSL
paths are rejected before filesystem contact. A narrow `GetDriveTypeW` probe accepts
only `DRIVE_FIXED`; remote, removable, unknown and unsupported drive types yield
`COPY_LOCALITY_UNCOVERED`. This is a conservative locality safeguard, not storage
attestation. Root ancestors are checked from the drive downward for symlinks and
reparse attributes before root authorization. The existing single-file reader
checks selected path components for detected redirections and regular-file status.
No directories are enumerated. Missing files and file admission failures return
incomplete coverage with file failure counts and unavailable claims.

ABF139 copy-only budgets: one file, 1048576 bytes (1 MiB) per file and batch,
65536 bytes (64 KiB) per line, 1024 retained record outcomes, four relative
components. The previous caps were 65536 bytes per file, 8192 bytes per line and
128 outcomes. A session can contain many non-usage events and substantial message
content before assistant usage appears. These caps admit modest sessions while
remaining at or below the shared collector defaults (64 MiB batch, 1 MiB line,
1024 outcomes); they do not promise coverage of every session. Files above the
file/byte budget are rejected, not partially totaled. Exhausting the record budget
discards every outcome from that file. An oversized line is discarded unparsed;
a malformed/parser-refused line or unterminated tail contributes no claims.

Memory remains bounded by the existing streaming implementation: a 64 KiB I/O
chunk, a line buffer limited to 64 KiB (Vec allocation growth can reserve more),
and at most 1024 retained outcomes. The shared long parser limits each result to
4096 retained bytes excluding allocator overhead, each retained string to 256
bytes, nesting to 32 and entries to 1048576 per line. It skips message content
without building a JSON tree. The file cap is not a whole-file allocation.
The fictional reader's limits and fixtures are unchanged. The scope's one-entry
budget is unused by the single-file API. Parsing and summary reuse ABF135's
`collect_claude_jsonl_file()`; there is no second collector.

The raw schema v1 payload (nested under `raw` since ABF141) has fixed `provenance=configured-windows-copy`,
`originalSource=not-established`, `originalEnvironment=not-established`,
`basis=observed-record-claims` and `duplicates=unresolved`. Its remaining fields
are `completeness`, `components`, `records`, `lines`, `files`, and `skipped` counts,
and derived `caveats`. Numerical claim sums are decimal strings; absence and
indeterminate claims stay distinct from valid zero. No source paths, source IDs,
models, messages, content or native environment assertions are serialized.
React uses the existing exact nested numerical validator and shared summary table,
with a separate exact provenance validator. It never treats the copy as fictional
or claims its original source was Windows or native WSL. The synthetic test golden
is explicitly a fictional test of this copy transport, not proof of provenance.

Native errors are fixed codes only: `COPY_NOT_CONFIGURED`, `COPY_ATTEMPT_CONSUMED`,
`COPY_PATH_REFUSED`, `COPY_ROOT_REFUSED`, `COPY_LOCALITY_UNCOVERED`,
`COPY_PLATFORM_UNSUPPORTED`, or `COPY_READ_FAILED`. Frontend transport validation
adds `COPY_RESPONSE_INVALID`; unknown rejection details become `COPY_READ_FAILED`.
The summary's file failure codes remain numerical counts (`path`, `tooLarge`,
`byteBudget`, `outputLimit`, `changedDuringRead`, `io`).

Both frontend and native guards allow one attempt per process/module lifetime,
including errors and concurrent requests. No mount, selection, timer, polling,
refresh or automatic retry invokes this command. Clearing removes UI results,
discards late completions and leaves the attempt consumed. It does not cancel a
blocking filesystem read or delete files. A displayed capture has unknown current
freshness; complete file reading does not establish complete usage. Restart is
required for another attempt.

Contract A V2 residual limits apply: checks and opens are path-based, not atomic
confinement against concurrent swaps. Metadata-preserving rewrites may escape
change detection. The existing Windows reader does not establish stable file
identity or detect hard links, and blocking filesystem calls have no wall-time
cancellation deadline. Detected reparse points, including mounted folders, are
refused; ASCII-only paths and fixed drives deliberately restrict this diagnostic.
The locality probe uses one documented synchronous Kernel32 API through a narrow
unsafe FFI module; the bootstrap denies unsafe code elsewhere. No dependencies
were added. Native Windows compilation/execution, reparse/mapped-drive behavior,
WebView2 interaction and Windows 10/11 compatibility are NOT_RUN and require a
separate user-controlled synthetic validation step. Linux harness evidence does
not validate those mechanisms or authorize real data access.

ABF139 realistic session coverage uses only invented checked-in fixtures:
`fixtures/abf139-session-events.jsonl` has 13 user, system, progress, summary,
file-history and assistant records, including absent, empty and null usage. All
13 produce incomplete observations with no valid token claims and no parser
rejections; no component is reported as zero. Nested progress data does not count
as the record's `message.usage`. Appending `abf139-assistant-usage.jsonl` adds four
assistant records, including partial claims, an explicit zero and a cache-creation
breakdown. The resulting separate claim sums are input 130, output 32, cache
creation 64 and cache read 500; flat creation 999 is ignored when the breakdown
11 + 13 is present. The two ABF139 payload goldens are compared with actual Rust
copy-reader output and validated/rendered by frontend tests.

Both the retained upstream adapter (`UsageEntry.message.usage`) and ABF's parser
read the nested usage shape. The upstream adapter requires input and output and
defaults absent flat cache components; the diagnostic intentionally retains ABF's
more explicit per-field states and unresolved duplicates. No parser, dependency,
IPC schema or collection architecture was added. A successful read without claims
does not establish zero usage or a parser defect. File coverage and token coverage
remain distinct, including when collection is labeled complete.

For a user-controlled Windows native retest, build this candidate with the existing
Windows toolchain and use a trusted isolated local fixed-drive copy root, first
with these synthetic fixtures. Configure `ABF_BOOTSTRAP_CLAUDE_COPY_ROOT` before
startup, select a permitted relative JSONL path, read once and inspect coverage
and separate component claims. Restart for each further attempt and verify Clear
removes the displayed result without rearming. Retest boundary failures and the
unchanged ABF136/137 commands. Native compilation, WebView2 interaction and Windows
10/11 filesystem behavior remain unverified here. Any personal-copy retest remains
user-controlled and requires separate explicit authorization; ABF139 reads none.


## Conservative copy replay diagnostic (ABF141)

The existing command remains
`abf_read_claude_copy_once({ relativePath: string }) -> Promise<string>`,
with the same allowlisted errors and 16384-character transport cap. Its success
string now contains an explicit diagnostic-local envelope:

```text
ClaudeCopyResult {
  schemaVersion: 1,
  raw: ClaudeCopySummary, // unchanged raw schema v1, fields and counting basis
  replay: ClaudeCopyReplay
}
```

`ClaudeCopyReplay` is a separate discriminated union:

- `{ status: "evaluated", basis: "source-scoped-exact-observation-claims-v1",
  eligibleObservations, representatives, exactReplayCopies,
  unresolvedObservations, unresolved, conflicts, components }`.
- `{ status: "identity-budget-exhausted", observations, limit }`.
- `{ status: "invalid-identity-limit" }`.
- `{ status: "wrong-harness" }`.
- `{ status: "allocation-failure" }`.

All counts are nonnegative integers. The evaluated `unresolved` object contains
`invalidIdentity`, `unsupportedRecordType`, `sidechain`, `invalidClaims` and
`conflictingEvidence`; reasons overlap and must not be added as a partition.
`eligibleObservations = representatives + exactReplayCopies`, and eligible plus
unresolved observations equals the raw observation count. `components` uses
the existing input/output/cacheCreation/cacheRead component schema, decimal
string sums and explicit indeterminate/overflow states, over representatives
only. No eligible observations produces unavailable components, not zero usage.

The reader invokes `exact_replay_summary(COPY_MAX_RECORDS)` on the one existing
batch, with the same 1024 constant used for retained record outcomes. This covers
all observations, including ineligible peers; there is no second read, parser,
collection source or partial-prefix evaluation. Comparison is bounded O(n^2),
with at most 1024 borrowed observation pointers (8 KiB requested pointer payload
on x64), plus the existing batch and summary allocations. Replay semantics,
parser limits, file/line budgets and permissions are unchanged.

React validates the exact envelope and nested replay union separately from the
raw summary. It displays evaluated counts, overlapping reasons and representative
token components, labeled **NOT the complete or billed usage total**. These are
equal retained source claims within this batch, not proven unique physical
events, cross-file history identities or billable consumption. Unresolved claims
remain raw. Rejected, oversized and pending lines remain visible coverage gaps
and may conceal conflicts or more copies. Exhausted, invalid and unavailable
evaluations display their state without subset counts or token values; raw
claims and coverage remain available.

Both results share the existing one-shot request and Clear lifecycle. Clearing
removes both, suppresses late results and never rearms the attempt. No identifiers,
raw JSONL, message content or source paths enter the response, logs or persistence.
The fictional panels, dashboard and other harnesses remain unchanged.

Synthetic evidence uses `fixtures/abf141-replay.jsonl` and its sanitized
`abf141-replay-payload.json`, checked byte-for-byte against the actual Rust
copy-reader output and consumed by React tests. The standalone Rust harness
also covers conflicts, empty/ineligible batches, lowered/invalid replay budgets,
and injected wrong-harness/allocation-failure projection variants. Allocation
failure is not forced in the allocator. Linux synthetic tests compile the actual
reader and serializer; they do not compile Tauri or exercise native IPC.
Native Windows compilation/run, WebView2 transport/interaction, Windows 10/11
filesystem checks, native WSL collection and GitHub Actions are NOT_RUN.

### Aggregate conflict explanation (ABF142)

The evaluated replay variant now requires an exact `conflicts` object containing
six numeric counters: `ineligiblePeer`, `identityMetadata`, `tokenFields`,
`parserEvidence`, `uuidRelated` and `sessionMessageRelated`. Failure variants and
the enclosing/raw schema v1 remain unchanged. Backend and frontend must be built
together; the strict frontend rejects older evaluated payloads missing this object.

Each counter counts an observation at most once, over all its conflicting peers.
The first four explain the unchanged comparison: an ineligible peer; differing
retained identity/metadata; differing token fields/container states, including
flat/breakdown evidence; and differing parser status/diagnostics. The last two
count observations with a conflicting peer related by UUID or session/message.
Only conflicts contribute. Ineligible-peer attribution is directional. Reasons
and relations overlap, potentially through different peers, so they cannot be
summed as unique observations, pairs or physical events. See the collection
boundary README's ABF142 section for the exact retained-field categories.

The frontend requires all six fixed keys, integer counts in 0..=1024, each at
most `conflictingEvidence`, and coverage of that count by both the cause counters
and relation counters. Only numeric counts and fixed labels cross the IPC boundary.
The existing 16384-character cap, raw counting, one-shot request, Clear suppression,
collection permissions and limits remain unchanged. All peers are scanned for
attribution with fixed additional stack storage and unchanged O(n^2) bounds.

`fixtures/abf142-conflicts.jsonl` contains three fictional observations with
overlapping metadata, token and parser evidence differences. Its sanitized
`abf142-conflicts-payload.json` is checked against the actual Rust copy reader and
rendered by frontend tests. Raw input/output/cache-creation/cache-read claims
remain 30/13/33/12; all three observations remain conflicting and unresolved,
with no representative or replay copy. Additional focused cases isolate each
cause, relation type, identical ineligible peers and conflict-free observations.
A 1024-record synthetic case verifies the unchanged transport cap. These tests
establish no cause for the native ABF141 conflicts. Native Windows/IPC/WebView2
validation still requires a separately authorized build and user-controlled run.
