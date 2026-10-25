# Agent Burn Fork (ABF) - Instructions for coding agents - V2.0

**Standing repository instructions.** This file does not itself authorize reading sensitive data, executing tests, modifying code, creating artifacts, or mutating Git. The intervention prompt establishes the exact permitted scope. Follow applicable nested `AGENTS.md` instructions too; stop on a material conflict rather than silently overriding a security boundary.

## Project intent

- Build a reliable native Windows 10/11 x64 application with Tauri v2 and Agent Burn's Rust engine. Selectively audit and adapt the separate DavidSZD Windows port; it is not an automatic Git ancestor.
- Support Windows-only collection, one selected Ubuntu WSL2 distribution supervised by Windows, and Windows + WSL. `Computer` is an aggregate view, never an independent collection source. Windows-only mode must not start WSL or contact its namespace.
- Preserve Agent Burn's useful experience where practicable. Prioritize correctness, provenance, privacy, reproducibility, maintainability, and practical incremental delivery.
- Upstream baseline: `Melvynx/agent-burn`. Engine heritage includes `ccusage/ccusage`. Never synchronize upstream automatically or edit the historical working installation.
- Product decisions and open contracts are in ABF project specifications and approved handoffs. Do not silently invent or adopt unresolved architecture.

## Authority and intervention boundaries

- The **user** approves sensitive permissions, structural contracts, publication, release, deployment, staging, commits, pushes, merges, and branch/ref changes.
- **ChatGPT** orchestrates missions, tool choice, prompts, evidence review and independent audits. Its proposal of the next prompt is not authorization for this agent to run another intervention.
- Execute **only the intervention explicitly delivered**. Read/write/execute/network/WSL/process permissions, fixture roots and temporary outputs must be granted for this intervention. Missing permissions mean not authorized.
- No unapproved subagents, opportunistic edits, access expansions, repairs of unexpected baseline, or implicit future gates.
- On unexpected Git identity or status, missing evidence, protected path, risky side effect, unclear permission or conflicting authority: **STOP**, report the issue, do not improvise.

## Git - strict no-mutation policy

Agents **never** stage, commit, push, merge, rebase, reset, restore, checkout, switch, cherry-pick, stash, change branches/tags/refs, or perform `git fetch`/`git pull` that change local state. This includes indirect IDE, script, API or subagent actions. Even an otherwise authorized implementation gate does not grant these rights.

Read-only Git checks require the intervention scope: `git status`, `git diff`, `git diff --cached`, `git log`, `git show`, `git rev-parse`, `git ls-files`, `git diff --check`, and similar checks. Verify branch, full HEAD, staged, unstaged, deleted/renamed, and untracked paths; plain `git diff` does not include new untracked files.

A **deliberately pre-staged baseline** may be present when the user prepared a Patchr correction. If the prompt identifies this condition, treat it as expected and compare the new unstaged changes plus untracked files against the staged baseline. Never stage them yourself.

Only the user performs Git mutations with reviewed commands. A successful test or audit never grants commit or publication authority.

## Protect the existing installation and data

**Do not modify, clean, overwrite, uninstall, migrate or inspect without specific scope:**

- The productive Agent Burn Windows app, its `%LOCALAPPDATA%\Agent Burn` profile, updater, settings, autostart, registry integration or caches.
- The old WSL CLI and `~/Tools/agent-burn` repository, including `fix/pricing-cache-race`.
- Real Claude Code, Codex and OpenCode histories/logs, credentials, OAuth tokens, subscriptions, quota data, provider billing or pricing caches.
- Host or guest systems, unrelated repositories, user backups, installation folders and external services.

The fork must have independent app/Tauri identity, executable, data folders, shortcuts, autostart, updater and release channel. Use synthetic fixtures or explicitly admitted isolated copies. A seemingly read-only action can spawn `wsl.exe`, start a distribution, contact a network/API, or refresh credentials. None is implicitly authorized. No unapproved installs, downloads, updates, registry edits or process changes.

## Data correctness invariants

1. **Provenance:** retain machine/environment, harness, source, event/record/session, attempt and acquisition identity when available. Do not invent proof of physical provenance from a synthetic label.
2. **Deduplication:** matching timestamps, model names, text or token totals do not prove identity. Deduplicate only with justified event-level evidence, before aggregation.
3. **Accounts:** quotas and limits belong to provider accounts, not environments. Do not double-count the same account or merge unverified account identities.
4. **Costs:** separate observed tokens, estimated API cost, confirmed billed cost and subscription-equivalent value; record tariff assumptions and version when applicable.
5. **Resilience:** unavailable or failed collection, restart, corrupt cache or timeout must not silently turn known historical values into zero. Mark freshness and availability.
6. **Determinism:** equal normalized inputs and rules produce reproducible environment and `Computer` projections; version relevant schemas and migrations.
7. **Scale:** pursue incremental scans and bounded memory without losing information through log rewrites, rotation or archival; measure rather than assume.
8. **Privacy:** local-first; no private prompts, secrets, histories or credentials in code, fixture output, reports, logs, telemetry or external systems.

**Approved decision:** D15 K7 uses ACL-protected plaintext SQLite in V1, not DPAPI K2. Windows and WSL are separate environments and `Computer` is a computed union view. WSL is not launched without explicit permission.

**Not automatically approved:** final event/persistence schemas, cross-environment identities, CLI bridge, native reader authority, migrations or other open production contracts. Do not present proposals as accepted decisions.

## Windows confinement boundary

B1/S1-S3 are synthetic research; the B1/S3 Rust fixture has no validated native Windows confinement. This section was revised by the human-approved Contract A V2 (ABF128D, documentation only). The revision grants **no** native, production, WSL or private-history access; every such access still needs its own explicit intervention permission. Synthetic fixtures and tests remain synthetic-only evidence.

- **Contract A (V2):** Agent Burn Fork reads only expected user-authorized history sources in the selected environments. Collection uses standard filesystem mechanisms appropriate for Windows 10/11, read-only, with bounded resources, explicit error handling and privacy protections. Exploration is limited to authorized roots. Symlinks, junctions and other detected redirections are not intentionally followed unless a separate explicitly approved contract allows them. Explicit network paths and unselected sources are excluded. Uncertain cases are refused or reported as uncovered when access cannot be justified. Ordinary concurrent file changes must not create invented reliable results: partial reads, errors, cancellation and retained last-known values are distinguished, with stale values labeled accordingly. This contract does not claim absolute confinement against a hostile or privileged local actor, or an actor concurrently modifying filesystem paths and objects; its residual limits are documented. Validation uses functional tests, targeted security tests and Windows checks proportionate to mechanisms actually implemented.
- **ABF120 findings under Contract A V2:** F01 is a simplified product safeguard requirement (refuse explicit network paths and unselected sources; report uncertain locality as uncovered). F02 is a historical finding; its absolute continuous-membership guarantee is no longer mandatory. F03 is limited to compatibility checks for the Windows APIs actually used, with explicit refusal on unsupported or missing support.
- **Contract B:** authority established at one checked instant and reuse of the same object remains a separate research hypothesis, not production authorization and not a substitute for A.
- Windows Sandbox is a possible **guest-only, non-representative research environment**, not an attested storage/host security boundary. No W1-W7 native witness is authorized by the sandbox's mere availability.

## Engineering, tests and review

- Verify repo, branch, full HEAD, worktree/index/untracked state, input hashes and candidate identity as relevant before code-affecting work. Never reset or repair a divergence.
- Prefer minimal cohesive changes that deliver useful functionality. Maintain explicit boundaries among collection, normalization, aggregation, persistence, quotas, pricing, Tauri commands and UI. Minimize unneeded edits to upstream parsers.
- Read applicable package-level `AGENTS.md` before editing its code. Repository-local skills are guidance, not permission to install tools, run unapproved processes or change Git.
- Use focused Rust modules, narrow interfaces and independent expected outcomes. Prefer synthetic fixture-backed tests for loaders and parsers; cover meaningful negative paths.
- Run **only authorized** tests/builds/commands, mindful of caches and output paths. Separate actually run tests from `NOT_RUN`, including non-representative Windows/WSL native checks. Do not bypass isolation to claim PASS.
- Validate Windows/WSL paths and process arguments; document side effects and network/credential accesses. Preserve backward compatibility only where contracts require it.
- For UI or documents, check relevant README, navigation and examples, but edit only authorized paths. Use US English for repository-facing issues, PR comments and reviews unless explicitly directed otherwise.
- Reuse existing still-valid tests and audit evidence. Use mechanical SHA, file inventory and counts for deterministic checks. Analyze new/affected logic; do not repeat large campaigns without a new safety or correctness question.
- Match verification depth to risk: ordinary changes get relevant tests; important changes receive focused review when appropriate; critical security/integrity changes require deeper independent scrutiny. Do not weaken essential checks for time or token savings.
- Independent auditors do not fix the code being audited. They work read-only on the candidate and write only authorized reports/artifacts outside the repository.
- **Typography:** use only the ASCII hyphen-minus `-` in authored UI strings, prose, reports, comments and prompts. Do not rewrite technical syntax, literal third-party data or exact quotations just to meet this style preference.

## Intervention IDs, reports and transfer archives

- A logical mission starts at `ABF55`; later interventions use `ABF55B`, `ABF55C`, ... `ABF55Z`, across agents and NEW/RESUME sessions. No initial A suffix. Never reuse or retroactively rename an ID. After Z, the orchestrator assigns a free new integer with explicit continuation reference; do not do so independently.
- Conversation names and intervention IDs are different. Follow the **intervention ID in the delivered prompt** when labeling reports and artifacts.
- Reports are proportional: candidate and HEAD, inspected/edited paths, relevant commands/results, `RUN`/`NOT_RUN`, findings, scope limits, evidence and one recommended next action **not executed**. Verdicts: `PASS`, `PASS_WITH_FINDINGS`, `HOLD`; severity: `BLOCKER`, `MAJOR`, `MINOR`.
- Write reports only in specifically permitted isolated temporary paths. `/tmp` WSL is volatile and separate from Windows temp paths. Do not assume persisted history or backups.
- If a ZIP transfer is explicitly required, use the **single exact unique final `/tmp` path** authorized for that intervention. Finalize and verify selected files/manifest, hashes, ZIP members and CRC before publishing safely without replacing an existing file. A published ZIP is immutable; no overwriting, deletion, renaming or regeneration at that path. A correction gets its own ID/path.
- **SyncBack and D:** belong to the user. Do not read/write D:, control SyncBack, or search backups. If a required ZIP is absent from permitted `/tmp`, report its absence instead of restoring it.

**Completion rule:** report only what was actually proved in the permitted scope. A passed synthetic test does not authorize production access. Never start a subsequent gate merely because it is logically next.
