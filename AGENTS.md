# Agent Burn Fork (ABF) — Instructions for coding agents

**Status:** project bootstrap rules. Read this file before any operation. It grants **no permission by itself** to inspect sensitive data, execute tests, modify code, or mutate Git. The current gate prompt defines the exact allowed scope.

## Project intent

- Build a reliable **Windows 10/11 x64** Tauri v2 desktop application, using Agent Burn's Rust engine and selectively adapting the existing Windows port.
- Support **Windows alone**, **Ubuntu WSL2 alone (controlled from Windows)**, and **both**; one computer, one selected WSL distro.
- `Computer` is a deduplicated aggregate **view**, never a third collection source. Keep environment and harness filters consistent across reports, models, charts and projects.
- Preserve the current Agent Burn experience where practicable. Prefer correctness, reproducibility, data safety and maintainability to adding features.
- Main engine lineage: `ccusage/ccusage` -> `Melvynx/agent-burn` -> `DavidSZD/agent-burn`. Prefer recent, verified Melvynx engine behavior; audit any DavidSZD code before reuse. Do not automatically synchronize upstream.
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

The user stages selected files **manually in VS Code**, then executes reviewed commit/push commands. An audit PASS does not authorize publication.

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

**Undecided until approved:** SQLite, event schema, cross-environment identity policy, WSL startup policy and CLI bridge design. Do not present suggestions as accepted decisions.

## Engineering practices

- Verify repository, branch, exact HEAD, relevant hashes and current worktree state **before** a gate affecting code; do not reset or repair differences.
- Prefer minimal, targeted changes; respect adapter boundaries and minimize edits to upstream parsers.
- Keep collection, normalization, aggregation, quotas, pricing, persistence, Tauri commands and UI responsibilities explicit.
- Validate all Windows/WSL path and process arguments. Document provider network calls, credential reads and side effects.
- Preserve backward compatibility only where required by approved contracts; do not invent a migration from the production installation.
- Run **only** gate-authorized tests, builds and commands; consider their output directories and cache side effects. Never claim PASS for unexecuted tests.
- Prefer independent expected outcomes, adversarial cases (copied sessions, token collisions, offline sources, concurrent writes, interrupted persistence) and negative-path tests.
- Independent auditors work read-only on source by default; write reports and authorized test artifacts outside the project, never fix the audited code.

## Conversation, reports and completion

- Agent **new conversation** IDs: `ABF1`, `ABF2`, `ABF11`, `ABF111`, etc. (sequential integers, no mandatory padding).
- **Resume** retains its integer and adds `B`, `C`, `D`...: `ABF1B` = first resume of `ABF1`. New independent audit = new integer.
- Gates (`A0`, `A1`, `A2`, `B1`, ...) are **separate** from conversation IDs.
- Standard non-persistent reports belong in an authorized, isolated temporary directory such as `/tmp/abf1-a0-audit/` **in the running environment**. Do not assume Windows and WSL share `/tmp`.
- Report: gate/conversation, baseline, inspect/edit list, exact commands and exit codes, tests PASS/FAIL/SKIP, findings, protected-state checks, artifacts and SHA256, risks, and **one** recommended next action (not started).
- Finding severities: `BLOCKER`, `MAJOR`, `MINOR`. Audit verdict: `PASS`, `FINDINGS`, `HOLD`. A PASS applies only to the checked scope.
- Corrections normally return to the original author thread; an independent audit remains immutable and is repeated as warranted.

**Initial phase:** architecture audit and contract design in **read-only mode**. No fork creation, installation, code edits or implementation until specifically approved by the user.
