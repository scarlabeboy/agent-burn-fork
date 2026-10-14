# ABF Windows Bootstrap

An isolated, unpublished Windows x64 Tauri v2 host candidate with a bundled React,
TypeScript and Vite frontend. It displays fixed synthetic placeholders only.
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
No Tauri JS API or CLI package is installed. All frontend dependencies, including
the bundled React runtime, are pinned in devDependencies. The reviewed esbuild
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

There are no application commands, capability files, plugins, collectors,
persistence, background workers, updater, tray or autostart. Empty capability
selection includes any capability files present, so the absence of those files
is part of this candidate's boundary. Production CSP permits packaged scripts
and styles and denies frontend connections. Native navigation is restricted to
the packaged origin and new-window requests are denied. Framework internals are
not an application IPC contract or permission to collect data.

## Evidence and limits

M5B-R2 evidence is under /tmp/abf47c-m5b-react-bootstrap/ in the validation
environment. It records reused registry provenance, exact commands and source
hashes. The source snapshot includes both genuine locks and the binary ICO.
Frontend typechecks, server-render tests, production build and static checks are
the bounded acceptance scope. Native Windows compilation, WebView2 rendering,
CSP enforcement, profile isolation, OS compatibility and coexistence are NOT_RUN.
No working native executable or production integration is claimed. Native
compilation or startup requires a separate gate; do not run cargo check, tauri
dev/build, native installers or a development server as part of these checks.
