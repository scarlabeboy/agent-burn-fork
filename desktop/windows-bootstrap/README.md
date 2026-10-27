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

Budgets: one file, 65536 bytes per file and batch, 8192 bytes per line, 128 retained
record outcomes, four relative components. The scope's one-entry budget is unused
by the single-file API. Parsing and summary reuse ABF135's
`collect_claude_jsonl_file()`; there is no second collector.

The separate schema v1 payload has fixed `provenance=configured-windows-copy`,
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
