import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createSyntheticBridge, isSyntheticSnapshot } from '../frontend/src/syntheticBridge.ts';
import type { BridgeState, SyntheticSnapshot } from '../frontend/src/syntheticBridge.ts';
import { App, ShellView } from '../frontend/src/App.tsx';
import {
  environmentOf,
  harnessOf,
  initialSelection,
  pageHeading,
  selectEnvironment,
  selectHarness,
} from '../frontend/src/selection.ts';
import type { Selection } from '../frontend/src/selection.ts';
import { environments, harnesses } from '../frontend/src/synthetic.ts';

const noop = () => undefined;
const renderSelection = (selection: Selection) =>
  renderToStaticMarkup(createElement(ShellView, { selection, onSelectEnvironment: noop, onSelectHarness: noop }));
const render = () => renderToStaticMarkup(createElement(App));
const textOf = (html: string) => html.replace(/<[^>]*>/gu, ' ').replace(/\s+/gu, ' ');

const attribute = (attributes: string, name: string) =>
  attributes.match(new RegExp(`\\s${name}="([^"]*)"`, 'u'))?.[1];

const buttons = (html: string) =>
  [...html.matchAll(/<button\b([^>]*)>(.*?)<\/button>/gu)].map((match) => ({
    attributes: match[1] ?? '',
    label: textOf(match[2] ?? '').trim(),
  }));

const allSelections: Selection[] = environments.flatMap((environment) =>
  harnesses.map((harness) => ({ environment: environment.id, harness: harness.id })),
);

test('defines three distinct environments and Computer as the only derived view', () => {
  assert.deepEqual(environments.map((entry) => entry.id), ['windows', 'selected-wsl', 'computer']);
  assert.deepEqual(environments.map((entry) => entry.label), ['Windows', 'Selected WSL', 'Computer']);
  assert.equal(new Set(environments.map((entry) => entry.status)).size, 3);
  assert.deepEqual(environments.filter((entry) => entry.kind === 'derived').map((entry) => entry.id), ['computer']);
  assert.deepEqual(harnesses.map((entry) => entry.label), ['General', 'Claude Code', 'Codex', 'OpenCode']);
});

test('selection logic is deterministic, independent per axis and idempotent', () => {
  assert.deepEqual(initialSelection, { environment: 'windows', harness: 'general' });
  const wsl = selectEnvironment(initialSelection, 'selected-wsl');
  assert.deepEqual(wsl, { environment: 'selected-wsl', harness: 'general' });
  const codex = selectHarness(wsl, 'codex');
  assert.deepEqual(codex, { environment: 'selected-wsl', harness: 'codex' });
  assert.deepEqual(selectEnvironment(codex, 'computer'), { environment: 'computer', harness: 'codex' });
  assert.equal(selectEnvironment(codex, 'selected-wsl'), codex, 'reselecting the current environment is a no-op');
  assert.equal(selectHarness(codex, 'codex'), codex, 'reselecting the current harness is a no-op');
  assert.deepEqual(selectEnvironment(codex, 'computer'), selectEnvironment(codex, 'computer'));
  assert.equal(initialSelection.environment, 'windows', 'selection never mutates its input');
});

test('headings follow the selected environment and harness', () => {
  assert.equal(pageHeading({ environment: 'windows', harness: 'general' }), 'Windows — Overview');
  assert.equal(pageHeading({ environment: 'selected-wsl', harness: 'claude-code' }), 'Selected WSL — Claude Code');
  assert.equal(pageHeading({ environment: 'computer', harness: 'opencode' }), 'Computer — OpenCode');
  for (const selection of allSelections) {
    const html = renderSelection(selection);
    assert.match(html, new RegExp(`<h1[^>]*>${pageHeading(selection)}</h1>`, 'u'));
    assert.match(html, new RegExp(`data-environment="${selection.environment}"`, 'u'));
    assert.match(html, new RegExp(`data-harness="${selection.harness}"`, 'u'));
  }
});

test('environment buttons expose exactly one pressed state matching the selection', () => {
  for (const selection of allSelections) {
    const group = renderSelection(selection).match(/<nav[^>]*aria-label="Environment"[^>]*>(.*?)<\/nav>/u)?.[1] ?? '';
    const found = buttons(group);
    assert.deepEqual(found.map((entry) => entry.label.replace(/\s*derived$/u, '')), ['Windows', 'Selected WSL', 'Computer']);
    for (const [index, entry] of found.entries()) {
      assert.match(entry.attributes, /type="button"/u);
      const expected = environments[index]?.id === selection.environment ? 'true' : 'false';
      assert.equal(attribute(entry.attributes, 'aria-pressed'), expected);
    }
  }
});

test('harness buttons mark only the selected harness as the current page', () => {
  for (const selection of allSelections) {
    const group = renderSelection(selection).match(/<nav[^>]*aria-label="Harness"[^>]*>(.*?)<\/nav>/u)?.[1] ?? '';
    const found = buttons(group);
    assert.deepEqual(found.map((entry) => entry.label), ['General', 'Claude Code', 'Codex', 'OpenCode']);
    const current = found.filter((entry) => attribute(entry.attributes, 'aria-current') === 'page');
    assert.equal(current.length, 1);
    assert.equal(current[0]?.label, harnessOf(selection).label);
    for (const entry of found) {
      assert.match(entry.attributes, /type="button"/u);
    }
  }
});

test('the only controls are the seven native selection buttons', () => {
  const html = render();
  assert.equal(buttons(html).length, environments.length + harnesses.length);
  assert.doesNotMatch(html, /<(?:input|select|textarea|form|a|iframe|img|script|link|object|embed)\b|(?:href|src)=|\sstyle=/u);
  assert.doesNotMatch(textOf(html), /\b(?:refresh|settings|log ?in|sign ?in|filter|connect now|start)\b/iu);
});

test('Computer is described as derived and lists sources without collecting', () => {
  const html = renderSelection({ environment: 'computer', harness: 'general' });
  const text = textOf(html);
  assert.match(text, /Derived view only; no collection source/u);
  assert.match(text, /never collects data itself/u);
  assert.match(text, /Derived from/u);
  assert.equal(environmentOf({ environment: 'computer', harness: 'general' }).kind, 'derived');
  const sources = html.match(/<section[^>]*aria-labelledby="sources-title"[^>]*>(.*?)<\/section>/u)?.[1] ?? '';
  assert.match(sources, /Windows/u);
  assert.match(sources, /Selected WSL/u);
  assert.doesNotMatch(sources, /Computer/u, 'Computer must not list itself as a source');
  for (const environment of ['windows', 'selected-wsl'] as const) {
    assert.doesNotMatch(renderSelection({ environment, harness: 'general' }), /Derived from/u);
  }
});

test('General shows overview regions; harness views add a separate quota/status section', () => {
  const general = renderSelection({ environment: 'windows', harness: 'general' });
  for (const title of ['Overview metrics', 'Spending and activity', 'Breakdown by harness', 'Models']) {
    assert.ok(general.includes(title), `General view must include ${title}`);
  }
  assert.doesNotMatch(general, /quota-section-title/u);

  const claude = renderSelection({ environment: 'windows', harness: 'claude-code' });
  assert.match(claude, /id="quota-section-title"[^>]*>Claude Code quota and status</u);
  assert.match(claude, /5-hour window/u);
  assert.match(claude, /Weekly window/u);
  assert.doesNotMatch(claude, /Breakdown by harness/u);
  assert.match(renderSelection({ environment: 'windows', harness: 'codex' }), /Rate-limit windows/u);
  assert.match(renderSelection({ environment: 'windows', harness: 'opencode' }), /Provider quota/u);
});

test('quota panel titles are h3 headings directly below the h2 quota section heading', () => {
  for (const harness of harnesses.filter((entry) => entry.quotaPanels.length > 0)) {
    const html = renderSelection({ environment: 'windows', harness: harness.id });
    const section = html.match(/<section[^>]*aria-labelledby="quota-section-title"[^>]*>(.*?)<\/section>/u)?.[1] ?? '';
    assert.match(section, /^<h2\b[^>]*id="quota-section-title"/u, 'the quota section must start with its h2');
    assert.doesNotMatch(section, /<h4\b/u, 'quota panels must not skip to h4');
    const titles = [...section.matchAll(/<h3\b[^>]*id="(quota-[^"]*-title)"[^>]*>(.*?)<\/h3>/gu)];
    assert.deepEqual(titles.map((match) => match[2]), harness.quotaPanels.map((panel) => panel.title));
    const levels = [...section.matchAll(/<h([1-6])\b/gu)].map((match) => Number(match[1]));
    assert.deepEqual(levels, [2, ...harness.quotaPanels.map(() => 3)], 'no skipped heading level');
  }
});

test('every view shows explicit unavailable states and invents no measurements', () => {
  for (const selection of allSelections) {
    const html = renderSelection(selection);
    const text = textOf(html).replace(/5-hour/gu, '');
    assert.doesNotMatch(text, /[0-9$€£%]/u, `${pageHeading(selection)} must not contain numbers or currency`);
    assert.doesNotMatch(text, /last updated|observed zero|tokens used|quota remaining|resets? (?:in|at)\b/iu);
    assert.match(text, /Synthetic placeholder — collection disabled/u);
    assert.match(text, /No provider usage, quota or spending has been observed\./u);
    assert.match(text, /No spending or activity observed|No .* activity observed/u);
    assert.match(text, /No models observed/u);
    assert.match(text, /Unavailable/u);
    assert.ok(!html.includes('<svg') && !html.includes('<canvas'), 'no chart graphics without data');
    const metrics = [...html.matchAll(/<p class="metric__value">(.*?)<\/p>/gu)].map((match) => match[1]);
    assert.deepEqual(metrics, ['Unavailable', 'Unavailable', 'Unavailable', 'Unavailable']);
  }
});

test('quota text keeps account scope and never sums across environments', () => {
  const html = renderSelection({ environment: 'computer', harness: 'claude-code' });
  assert.match(textOf(html), /provider and account scoped, not environment scoped; it is never summed across environments/u);
});

test('the banner is the unique note and ARIA references resolve to unique targets', () => {
  for (const selection of allSelections) {
    const html = renderSelection(selection);
    const notes = [...html.matchAll(/<p\b([^>]*)>(.*?)<\/p>/gu)].filter((match) => attribute(match[1] ?? '', 'role') === 'note');
    assert.equal(notes.length, 1);
    assert.equal(notes[0]?.[2], 'Synthetic placeholder — collection disabled');

    const elements = [...html.matchAll(/<([a-z][a-z0-9]*)\b([^>]*)>/gu)];
    const ids = elements.map((element) => attribute(element[2] ?? '', 'id')).filter((id): id is string => id !== undefined);
    assert.equal(new Set(ids).size, ids.length, 'rendered IDs must be unique');
    for (const element of elements) {
      const reference = attribute(element[2] ?? '', 'aria-labelledby');
      if (reference !== undefined) {
        for (const id of reference.trim().split(/\s+/u)) {
          assert.ok(ids.includes(id), `aria-labelledby target ${id} must exist`);
        }
      }
    }
    assert.equal([...html.matchAll(/<h1\b/gu)].length, 1);
    assert.equal([...html.matchAll(/<main\b/gu)].length, 1);
  }
});

test('renders deterministically without runtime inputs', () => {
  assert.equal(render(), render());
  assert.equal(render(), renderSelection(initialSelection));
  for (const selection of allSelections) {
    assert.equal(renderSelection(selection), renderSelection({ ...selection }));
  }
});

test('frontend allows only the reviewed core invoke and synthetic command, with no network, timers or storage', () => {
  const root = fileURLToPath(new URL('../frontend/src/', import.meta.url));
  const files = (directory: string): string[] =>
    readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory() ? files(join(directory, entry.name)) : [join(directory, entry.name)],
    );
  const sources = files(root).filter((file) => /\.(?:tsx?|css)$/u.test(file));
  assert.ok(sources.length >= 6);
  const forbidden =
    /@tauri-apps|__TAURI|\binvoke\s*(?:<[^>]*>)?\s*\(|\bfetch\s*\(|XMLHttpRequest|WebSocket|EventSource|sendBeacon|setInterval|setTimeout|requestAnimationFrame|localStorage|sessionStorage|indexedDB|document\.cookie|node:fs|child_process|https?:\/\/|@import|url\(|@font-face/u;
  for (const file of sources) {
    let source = readFileSync(file, 'utf8');
    if (file === join(root, 'syntheticBridge.ts')) {
      const allowedImport = "import { invoke } from '@tauri-apps/api/core';";
      const allowedCall = "invoke<unknown>('abf_get_synthetic_snapshot')";
      assert.equal(source.split(allowedImport).length, 2, 'one official core invoke import');
      assert.equal(source.split(allowedCall).length, 2, 'one argument-free approved application command');
      source = source.replace(allowedImport, '').replace(allowedCall, '');
    }
    assert.doesNotMatch(source, forbidden, `${file} must not use forbidden capabilities`);
  }
});


// Transport fixtures only: no provider observations or discovered environments.
const snapshot = {
  schemaVersion: 1,
  provenance: 'fixed-synthetic',
  sources: [
    { id: 'windows', observations: { usage: null, sessions: null, models: null, cost: null, quota: null } },
    { id: 'selected-wsl', observations: { usage: null, sessions: null, models: null, cost: null, quota: null } },
  ],
} as const satisfies SyntheticSnapshot;

test('accepts the versioned synthetic snapshot with two separate unavailable sources', async () => {
  assert.equal(isSyntheticSnapshot(snapshot), true);
  const bridge = createSyntheticBridge(async () => snapshot);
  assert.deepEqual(await bridge.load(), { status: 'synthetic', snapshot });
});

const malformedSnapshots: readonly unknown[] = [
  undefined, null, {}, [],
  { ...snapshot, schemaVersion: 2 },
  { ...snapshot, provenance: 'observed' },
  { ...snapshot, sources: [] },
  { ...snapshot, sources: [snapshot.sources[0]] },
  { ...snapshot, sources: [...snapshot.sources, { ...snapshot.sources[0], id: 'computer' }] },
  { ...snapshot, sources: [snapshot.sources[0], snapshot.sources[0]] },
  { ...snapshot, sources: [snapshot.sources[1], snapshot.sources[0]] },
  { ...snapshot, sources: [{ ...snapshot.sources[0], id: 'computer' }, snapshot.sources[1]] },
  { ...snapshot, sources: [{ ...snapshot.sources[0], distro: 'Ubuntu' }, snapshot.sources[1]] },
  { ...snapshot, account: 'invented' },
  ...['usage', 'sessions', 'models', 'cost', 'quota'].flatMap((field) => [0, 1, '', [], {}].map((value) => ({
    ...snapshot,
    sources: [{ ...snapshot.sources[0], observations: { ...snapshot.sources[0].observations, [field]: value } }, snapshot.sources[1]],
  }))),
  { ...snapshot, sources: [{ id: 'windows' }, snapshot.sources[1]] },
  { ...snapshot, sources: [{ ...snapshot.sources[0], observations: null }, snapshot.sources[1]] },
  { ...snapshot, sources: [{ ...snapshot.sources[0], observations: { ...snapshot.sources[0].observations, quotaReset: 0 } }, snapshot.sources[1]] },
];

for (const [index, response] of malformedSnapshots.entries()) {
  test(`rejects missing or malformed synthetic response ${index + 1}`, async () => {
    assert.equal(isSyntheticSnapshot(response), false);
    assert.deepEqual(await createSyntheticBridge(async () => response).load(), { status: 'unavailable' });
  });
}

test('missing native runtime becomes unavailable without throwing', async () => {
  assert.deepEqual(await createSyntheticBridge().load(), { status: 'unavailable' });
});

test('native rejection becomes unavailable without exposing native error details', async () => {
  const bridge = createSyntheticBridge(() => Promise.reject(new Error('private details')));
  assert.deepEqual(await bridge.load(), { status: 'unavailable' });
});

test('synchronous invocation failure becomes unavailable', async () => {
  const bridge = createSyntheticBridge(() => { throw new Error('missing runtime'); });
  assert.deepEqual(await bridge.load(), { status: 'unavailable' });
});

test('one bridge request is shared across repeated consumers and all filter changes', async () => {
  let requests = 0;
  let respond: (value: unknown) => void = noop;
  const bridge = createSyntheticBridge(() => {
    requests += 1;
    return new Promise<unknown>((resolve) => { respond = resolve; });
  });
  const first = bridge.load();
  const second = bridge.load();
  assert.equal(first, second);
  await Promise.resolve();
  assert.equal(requests, 1);
  respond(snapshot);
  assert.deepEqual(await first, { status: 'synthetic', snapshot });
  for (const selection of allSelections) {
    renderSelection(selection);
    assert.equal(bridge.load(), first);
  }
  assert.equal(requests, 1);
});

test('unavailable invocation is cached without automatic retries', async () => {
  let requests = 0;
  const bridge = createSyntheticBridge(async () => { requests += 1; throw new Error('offline'); });
  await bridge.load();
  await bridge.load();
  assert.equal(requests, 1);
});

for (const state of [
  { status: 'loading' },
  { status: 'synthetic', snapshot },
  { status: 'unavailable' },
] as const satisfies readonly BridgeState[]) {
  test(`renders ${state.status} bridge state while keeping every observation unavailable in all views`, () => {
    for (const selection of allSelections) {
      const html = renderToStaticMarkup(createElement(ShellView, {
        selection, onSelectEnvironment: noop, onSelectHarness: noop, bridgeState: state,
      }));
      const expected = {
        loading: 'Local synthetic bridge: loading',
        synthetic: 'Local synthetic bridge responded',
        unavailable: 'Local synthetic bridge unavailable',
      }[state.status];
      assert.match(html, /role="status"/u);
      assert.ok(textOf(html).includes(expected));
      assert.match(textOf(html), /collection disabled/u);
      assert.deepEqual([...html.matchAll(/<p class="metric__value">(.*?)<\/p>/gu)].map((entry) => entry[1]),
        ['Unavailable', 'Unavailable', 'Unavailable', 'Unavailable']);
      assert.equal(buttons(html).length, 7);
      assert.match(html, new RegExp(`<h1[^>]*>${pageHeading(selection)}</h1>`, 'u'));
    }
  });
}


test('official core invoke sends only the approved command with no frontend arguments', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(snapshot); } },
  } });
  try {
    assert.deepEqual(await createSyntheticBridge().load(), { status: 'synthetic', snapshot });
    assert.deepEqual(calls, [['abf_get_synthetic_snapshot', {}, undefined]]);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});

test('native boundary registers exactly the synthetic command and enables only official IPC CSP endpoints', () => {
  const nativeRoot = new URL('../src-tauri/', import.meta.url);
  const main = readFileSync(new URL('src/main.rs', nativeRoot), 'utf8');
  const command = readFileSync(new URL('src/synthetic_snapshot.rs', nativeRoot), 'utf8');
  const build = readFileSync(new URL('build.rs', nativeRoot), 'utf8');
  const config = JSON.parse(readFileSync(new URL('tauri.conf.json', nativeRoot), 'utf8'));
  const handlers = [...main.matchAll(/\.invoke_handler\(tauri::generate_handler!\[([^\]]*)\]\)/gu)];
  assert.deepEqual(handlers.map((match) => match[1]?.trim()), ['synthetic_snapshot::abf_get_synthetic_snapshot']);
  assert.equal([...command.matchAll(/#\[tauri::command\]/gu)].length, 1);
  assert.match(command, /fn abf_get_synthetic_snapshot\(\)/u);
  assert.doesNotMatch(command, /std::|tauri::(?:State|AppHandle|Manager)|\b(?:use|mod)\b|\b(?:async|unsafe)\b|SystemTime|rand|thread|spawn|include!|include_str!|include_bytes!/u);
  assert.equal(config.identifier, 'dev.scarlabeboy.abf.windowsbootstrap');
  assert.equal(config.app.withGlobalTauri, false);
  assert.deepEqual(config.app.windows, []);
  assert.deepEqual(config.app.security.capabilities, []);
  assert.equal(
    readdirSync(nativeRoot).includes('capabilities'), false,
    'src-tauri/capabilities must remain absent; unexpected capability directories or files require review',
  );
  assert.equal(config.bundle.active, false);
  assert.doesNotMatch(build, /app_manifest|commands/u, 'no app ACL manifest: local custom command default applies');
  const directives = config.app.security.csp.split(';').map((entry: string) => entry.trim());
  assert.ok(directives.includes('connect-src ipc: http://ipc.localhost'));
  assert.ok(directives.includes("default-src 'none'"));
  assert.doesNotMatch(config.app.security.csp.replace('http://ipc.localhost', ''), /https?:|wss?:|\*/u);
  assert.match(main, /\.disable_drag_drop_handler\(\)/u);
  assert.match(main, /NewWindowResponse::Deny/u);
  assert.match(main, /url\.host_str\(\) == Some\("tauri.localhost"\)/u);
});
