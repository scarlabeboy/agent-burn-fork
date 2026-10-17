# Agent Burn Fork (ABF) — Instructions for coding agents

**Status:** standing repository instructions for coding agents. Read this file before any operation. It grants **no permission by itself** to inspect sensitive data, execute tests, modify code, or mutate Git. The current gate prompt defines the exact allowed scope.

## Project intent

- Build a reliable **Windows 10/11 x64** Tauri v2 desktop application, using Agent Burn's Rust engine and selectively adapting the existing Windows port.
- Support **Windows alone**, **Ubuntu WSL2 alone (controlled from Windows)**, and **both**; one computer, one selected WSL distro.
- `Computer` is a deduplicated aggregate **view**, never a third collection source. Keep environment and harness filters consistent across reports, models, charts and projects.
- Preserve the current Agent Burn experience where practicable. Prefer correctness, reproducibility, data safety and maintainability to adding features.
- Engine heritage includes `ccusage/ccusage`. The Git upstream baseline is `Melvynx/agent-burn`; `DavidSZD/agent-burn` is a separate Windows-port reference, not an automatic Git ancestor or synchronization source. Audit reused code. Never synchronize upstream automatically.
- Detailed product decisions live in the ABF project handoff/specifications. Do not invent or silently freeze open architectural choices.

## Authority and work scope

- The **user** approves architecture, sensitive operations, manual Git staging, commit/push/merge/release/deployment.
- **ChatGPT** orchestrates gates, prompts, evidence review and independent audits in normal chat.
- You execute **only the explicitly authorized gate**. No implicit next gate, opportunistic refactor, unauthorized subagent or scope expansion.
- Respect per-gate read/write/execute/network/WSL allowlists. If missing, treat the capability as **not authorized**.
- On unexpected baseline, unclear identity, sensitive access, protected-file modification or out-of-scope dependency: **STOP**, report what was found, and request renewed scope. Do not fix by improvisation.

## Git — strict no-mutation policy

**Agents must not execute Git mutations or publication actions, even when technically possible.** Forbidden without exception for agents:

- `git add` (including `-N`), `commit`, `push`, `reset`, `restore`, `checkout`, `switch`, `merge`, `rebase`, `cherry-pick`, `stash`;
- branch/tag/reference creation, update or deletion, and `git fetch`/`git pull` that change local repository state;
- equivalent IDE, script, GitHub API or subagent actions.

Read-only checks such as `git status`, `git diff`, `git diff --cached`, `git log`, `git show`, `git rev-parse`, `git ls-files` and `git diff --check` require gate authorization. Check **tracked, untracked, staged and deleted** files; `git diff` alone misses new untracked files.

The user stages selected files **manually, through VS Code or reviewed shell commands**, then executes reviewed commit/push commands. An audit PASS does not authorize publication.

## Protect the existing working installation

**Never modify, clean, overwrite, uninstall or migrate** the currently used Agent Burn Windows app, its `%LOCALAPPDATA%\Agent Burn` data, autostart, updater, settings or caches; the WSL CLI; the historical `~/Tools/agent-burn` checkout and `fix/pricing-cache-race` branch; or real provider/tool histories. These are protected unless the user grants a separate, narrow authorization.

- Create all future ABF application identifiers, executables, profiles, caches, update channels and data directories **independently**. Test coexistence explicitly.
- Never alter real Claude Code/Codex/OpenCode logs, tokens, credentials, OAuth files, provider billing information or pricing caches during tests.
- Use synthetic fixtures or explicitly approved isolated copies. Be mindful that even a "read" can spawn `wsl.exe`, start Ubuntu, query an API, or refresh credentials.
- No unauthorized installers, downloads, package updates, network requests, registry edits, tasks or process changes.

## Data-correctness invariants

1. **Preserve provenance:** machine/environment, harness, session, event/record identity, source file and capture time where available.
2. **Deduplicate cautiously:** equal timestamps/models/token totals do **not** prove event identity. Do not collapse distinct events or assert exact deduplication from pre-aggregated reports.
3. **Separate identities:** accounts and quotas are provider/account scoped, **not environment scoped**. Never sum the same account's quota twice; never merge unverified accounts.
4. **Distinguish money concepts:** observed tokens, estimated API cost, verified billed cost and subscription-equivalent value are different. Show tariff provenance/version and classification assumptions.
5. **Preserve history:** failed or disabled collection, unavailable WSL/API, timeout, corrupt cache, restart or update must not silently replace reliable historical data with zero. Track freshness/status explicitly.
6. **Determinism:** identical normalized inputs and rules should produce reproducible environment and Computer views. Version schemas and migration logic.
7. **Scale:** pursue incremental scans and bounded memory without losing data after log rotation, rewrite or archival; benchmark rather than assume.
8. **Privacy:** local-first. Never emit prompts, tokens, private logs or secrets into repository, reports, telemetry or external services.

**Approved relevant decisions:** D15 K7 uses ACL-protected plaintext SQLite for V1, not the DPAPI-based K2 architecture. Windows and WSL are distinct collection environments, `Computer` is a computed union view, and WSL must never be started automatically without explicit authorization.

**Still subject to separate approval:** detailed event and persistence schemas, unresolved cross-environment identity rules, CLI bridge design and other proposed architecture contracts. Do not present design candidates as accepted decisions.

## Engineering practices

- Verify repository, branch, exact HEAD, relevant hashes and current worktree state **before** a gate affecting code; do not reset or repair differences.
- Prefer minimal, targeted changes; respect adapter boundaries and minimize edits to upstream parsers.
- Read the nearest package-specific `AGENTS.md` before editing package code. Root ABF safety rules and the current gate always take precedence over conflicting inherited instructions.
- Consult relevant repository-local skills when useful and authorized, but never treat a skill as permission to activate direnv/Nix, install tools, run scripts, create PRs, mutate Git or access real data.
- Keep Rust modules focused and internal APIs narrow; prefer synthetic fixture-backed parser and loader tests.
- For user-facing changes, review the impact on README files, documentation, navigation and examples. Modify documentation only within the authorized gate.
- Use US English for repository-facing GitHub issues, PRs and review comments unless the user specifies otherwise.
- Keep collection, normalization, aggregation, quotas, pricing, persistence, Tauri commands and UI responsibilities explicit.
- Validate all Windows/WSL path and process arguments. Document provider network calls, credential reads and side effects.
- Preserve backward compatibility only where required by approved contracts; do not invent a migration from the production installation.
- Run **only** gate-authorized tests, builds and commands; consider their output directories and cache side effects. Never claim PASS for unexecuted tests.
- Prefer independent expected outcomes, adversarial cases (copied sessions, token collisions, offline sources, concurrent writes, interrupted persistence) and negative-path tests.
- Independent auditors work read-only on source by default; write reports and authorized test artifacts outside the project, never fix the audited code.
- Environment-dependent tests are representative only if their execution environment has been verified for the property under test. A sandbox result does not automatically prove native Windows/WSL filesystem, permissions, mount, or kernel behavior. Report unavailable or nonrepresentative tests; never bypass isolation.
- Required validations follow the gate's risks and affected invariants. Investigate failures, contradictions, and new risks, but avoid repeating equivalent checks without new evidentiary value. Never sacrifice required validation for time or tokens.
- Use deterministic checks for repository identity, file inventories, hashes, command results, and test counts. Reuse verified unchanged evidence when applicable, while independently examining affected code and invariants.

## Conversation, reports and completion

- **Intervention IDs, independent of conversations:** Each logical gate starts with a unique integer ID (`ABF55`). Subsequent interventions within that gate receive unique, increasing suffixes (`ABF55B`, `ABF55C`, `ABF55D`...), whether performed in a NEW or RESUMED conversation. The first intervention has no `A` suffix. Never reuse an intervention ID or retroactively rename historical interventions. The current gate prompt supplies the exact authorized intervention ID.
- **NEW versus RESUME:** Conversation choice does not determine intervention numbering. An independent audit normally uses a new conversation, but may retain the same gate integer with a new suffix when reviewing that gate. A genuinely distinct logical gate receives the next available integer. Opening a new conversation alone never creates a new gate.
- **Unique and immutable transfer ZIPs:** When explicitly authorized by the gate prompt, create one transfer ZIP with the intervention's unique ID and exact designated path, normally at the root of `/tmp`. Finalize and verify its contents before safely publishing the complete archive without overwriting any existing file. Never modify, replace, regenerate at the same path, rename, or delete a published ZIP. Corrections require a new intervention ID and a separately authorized ZIP path. Agents must not access the Windows D: backup or operate SyncBack without separate explicit authorization.
- Gates (`A0`, `A1`, `A2`, `B1`, ...) are **separate** from intervention IDs and conversation sessions.
- Standard non-persistent reports belong in an authorized, isolated temporary directory such as `/tmp/abf1-a0-audit/` **in the running environment**. Do not assume Windows and WSL share `/tmp`.
- Reporting is proportional to the gate. Preserve: verified baseline, inspected/edited paths, material commands and exit codes, actual tests versus `NOT_RUN`, findings, relevant protected-state checks, evidence references, limitations, and exactly **one** proposed next action (not started). Avoid duplicating unchanged logs and creating unnecessary reports.
- Finding severities: `BLOCKER`, `MAJOR`, `MINOR`. Audit verdict: `PASS`, `PASS_WITH_FINDINGS`, `HOLD`. Use `FINDINGS` only when a particular audit contract explicitly defines that legacy label. A PASS applies only to the checked scope.
- The orchestrator chooses NEW or RESUME for corrections from technical continuity, remaining context, known compactions, previous resumptions, risk, and evidence. Independent audits remain separate from author corrections, immutable, and are repeated as warranted.

**Implementation status:** never infer current HEAD, completed gates, accepted findings, or production readiness from this file. Verify current repository state and the applicable approved contracts. Synthetic research does not authorize production collection, real-history access, or production reuse; these remain **BLOCKED** without separate authorization and required security review. No future gate is implicitly authorized.
