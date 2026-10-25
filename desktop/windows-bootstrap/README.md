# ABF Windows Bootstrap

An isolated, unpublished Windows x64 Tauri v2 host candidate with a bundled React,
TypeScript and Vite frontend. It displays a fixed synthetic demo only.
Windows and Selected WSL are distinct display fixtures; Computer is a derived
view, never a collection source. No provider data, quota, cost or usage is observed.

This workspace has its own pnpm workspace and locks and a standalone Rust
workspace. It does not import the production CLI, model/adapter research crates
or macOS application. React is bundled into local static assets; the future
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
passed to Tauri's data_directory API. Application code does not discover or
explicitly create directories and has no HOME/LOCALAPPDATA fallback. Tauri may
recursively create missing profile directories, and WebView2 may create directories
and write profile/cache data. A missing or mistyped root is not refused merely
because it does not exist.

The separately authorized native gate must check actual profile placement,
aliases (including junctions/symlinks, SUBST, 8.3 names, trailing dots/spaces and
mapped drives), permissions, environment/registry overrides and coexistence.
Lexical acceptance is not filesystem-isolation proof. Never point this variable
at an existing Agent Burn installation or real user data.

Exactly one synthetic application command, `abf_get_synthetic_snapshot`, takes
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

A second argument-free command, `abf_get_fictional_claude_summary`, returns one JSON string
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
