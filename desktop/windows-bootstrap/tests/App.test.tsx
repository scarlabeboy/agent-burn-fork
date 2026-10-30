import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { Children, createElement, isValidElement } from 'react';
import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createSyntheticBridge, isSyntheticSnapshot, syntheticBridge } from '../frontend/src/syntheticBridge.ts';
import type { BridgeState } from '../frontend/src/syntheticBridge.ts';
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
import { fictionalResultsSnapshot } from '../frontend/src/fictionalResultsSnapshot.ts';
import { splitFictionalResults } from '../frontend/src/fictionalResultsView.ts';

import { snapshot, expectedViews } from './syntheticFixture.ts';

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

const preBlocks = (html: string) =>
  [...html.matchAll(/<pre\b[^>]*class="fictional-results"[^>]*>([\s\S]*?)<\/pre>/gu)].map((match) => match[1] ?? '');
const escapeHtml = (text: string) => text.replace(/&/gu, '&amp;').replace(/</gu, '&lt;')
  .replace(/>/gu, '&gt;').replace(/"/gu, '&quot;').replace(/'/gu, '&#x27;');
const expectedTitles = ['WindowsOnly', 'SelectedWSL interrupted', 'Both independent', 'Both unavailable',
  'Fictional file', 'Fictional multiple files', 'Fictional rejected file', 'Fictional incomplete coverage',
  'Fictional partial records'];

test('fixed fictional results show nine collapsed scenarios with visible demo label and provenance', () => {
  const html = render();
  assert.match(html, /Fixed fictional results - demonstration only/u);
  assert.match(html, /Reported claim sums, not deduplicated usage/u);
  assert.match(html, /All scenarios are fixed and independent of the selected environment and harness/u);
  assert.match(html, /Freshness is logical, not measured age\. No real account data is shown/u);
  const summaries = [...html.matchAll(/<summary>(.*?)<\/summary>/gu)].map((match) => match[1]);
  assert.deepEqual(summaries, expectedTitles);
  assert.equal([...html.matchAll(/<details\b/gu)].length, 9);
  assert.doesNotMatch(html, /<details[^>]*\bopen(?:\s|=|>)/u);
  assert.match(html, /<ul class="scenarios" aria-label="Fictional scenarios, demonstration only">/u);
  // Provenance and limits stay outside any disclosure.
  const outside = html.replace(/<details\b[\s\S]*?<\/details>/gu, '');
  assert.match(outside, /Synthetic only; provenance=Unattested\./u);
  assert.match(outside, /freshness is logical, not measured age/u);
  assert.match(outside, /No combined total, shared catalog authority or overall collection verdict/u);
  for (const block of html.matchAll(/<pre\b([^>]*)>/gu)) {
    assert.match(block[1] ?? '', /tabindex="0"/u);
    assert.match(block[1] ?? '', /role="region"/u);
    assert.match(block[1] ?? '', /aria-label="Fixed fictional /u);
  }
});

test('fixed snapshot renders verbatim as text in every selection with its recorded SHA-256', () => {
  assert.equal(createHash('sha256').update(fictionalResultsSnapshot, 'utf8').digest('hex'),
    '11d70e3068f694e66a1aa35a49ffb50fb1c2b87aa8a0a986da39e5b8b39901e0');
  assert.equal(Buffer.byteLength(fictionalResultsSnapshot, 'utf8'), 11946);
  const parts = splitFictionalResults(fictionalResultsSnapshot);
  assert.equal(parts.scenarios.length, 9);
  assert.deepEqual(parts.scenarios.map((entry) => entry.title), expectedTitles);
  assert.equal(parts.preamble + parts.scenarios.map((entry) => entry.text).join(''), fictionalResultsSnapshot);
  for (const selection of allSelections) {
    const blocks = preBlocks(renderSelection(selection));
    assert.deepEqual(blocks, [parts.preamble, ...parts.scenarios.map((entry) => entry.text)].map(escapeHtml));
    assert.equal(blocks.join(''), escapeHtml(fictionalResultsSnapshot));
  }
});

test('each scenario block holds its own evidence', () => {
  const byTitle = new Map(splitFictionalResults(fictionalResultsSnapshot).scenarios.map((entry) => [entry.title, entry.text]));
  const evidence: Array<[string, RegExp]> = [
    ['Fictional multiple files', /stored_occurrences=2 input_tokens=ReportedClaimSum\(74\) output_tokens=ReportedClaimSum\(10\) total_tokens=NoValidClaims/u],
    ['Fictional rejected file', /Rejected\(NotSyntheticFixture\)/u],
    ['Fictional incomplete coverage', /coverage=InventoryIncomplete/u],
    ['Fictional incomplete coverage', /skip=DepthLimit/u],
    ['Fictional partial records', /coverage=PartiallyCovered/u],
    ['Fictional partial records', /record_error=ProhibitedField\(Content\)/u],
    ['Fictional partial records', /pending_tail_bytes=17/u],
    ['Fictional partial records', /input_tokens=ReportedClaimSum\(9\) output_tokens=NoValidClaims total_tokens=ReportedClaimSum\(12\)/u],
    ['Both independent', /freshness=Stale/u],
    ['Both unavailable', /freshness=Unavailable/u],
    ['SelectedWSL interrupted', /freshness=Unestablished/u],
  ];
  for (const [title, pattern] of evidence) assert.match(byTitle.get(title) ?? '', pattern, title);
});

test('fixed snapshot retains coverage, rejected inputs and freshness limits without raw payloads', () => {
  assert.equal([...fictionalResultsSnapshot.matchAll(/^== /gmu)].length, 9);
  assert.match(fictionalResultsSnapshot, /Synthetic only; provenance=Unattested\./u);
  assert.match(fictionalResultsSnapshot, /freshness is logical, not measured age/u);
  assert.match(fictionalResultsSnapshot, /No combined total, shared catalog authority or overall collection verdict/u);
  assert.match(fictionalResultsSnapshot, /stored_occurrences=2 input_tokens=ReportedClaimSum\(74\) output_tokens=ReportedClaimSum\(10\) total_tokens=NoValidClaims/u);
  assert.match(fictionalResultsSnapshot, /Rejected\(NotSyntheticFixture\)/u);
  assert.match(fictionalResultsSnapshot, /coverage=InventoryIncomplete/u);
  assert.match(fictionalResultsSnapshot, /skip=DepthLimit/u);
  assert.match(fictionalResultsSnapshot, /coverage=PartiallyCovered/u);
  assert.match(fictionalResultsSnapshot, /record_error=ProhibitedField\(Content\)/u);
  assert.match(fictionalResultsSnapshot, /pending_tail_bytes=17/u);
  assert.match(fictionalResultsSnapshot, /input_tokens=ReportedClaimSum\(9\) output_tokens=NoValidClaims total_tokens=ReportedClaimSum\(12\)/u);
  assert.doesNotMatch(fictionalResultsSnapshot, /SENTINEL|9999|"tokens"|C:\\|\/home\/|\/tmp\//u);
});

test('fictional disclosures have no event handlers or deferred content requests', () => {
  const disclosures: ReactNode[] = [];
  const find = (node: ReactNode): void => {
    if (!isValidElement<{ children?: ReactNode }>(node)) return;
    if (node.type === 'details') disclosures.push(node);
    Children.forEach(node.props.children, find);
  };
  find(ShellView({ selection: initialSelection, onSelectEnvironment: noop, onSelectHarness: noop }));
  assert.equal(disclosures.length, 9);
  const inspect = (node: ReactNode): void => {
    if (!isValidElement<{ children?: ReactNode }>(node)) return;
    assert.equal(typeof node.type, 'string', 'disclosure contains only native elements and static text');
    assert.doesNotMatch(Object.keys(node.props).join(' '), /\bon[A-Z]|\bref\b|dangerouslySetInnerHTML/u);
    Children.forEach(node.props.children, inspect);
  };
  disclosures.forEach(inspect);
});

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

test('the shell has seven selection buttons and nine native scenario disclosures', () => {
  const html = render();
  assert.equal(buttons(html).length, environments.length + harnesses.length);
  assert.equal([...html.matchAll(/<summary>/gu)].length, 9);
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
  for (const title of ['Overview metrics', 'Synthetic activity', 'Breakdown by harness', 'Models']) {
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
    // The separate fixed capture contains fictional numbers; the shell still has no observed usage.
    const text = textOf(html.replace(/<pre\b[^>]*class="fictional-results"[^>]*>[\s\S]*?<\/pre>/gu, '')).replace(/5-hour/gu, '');
    assert.doesNotMatch(text, /[0-9$€£%]/u, `${pageHeading(selection)} must not contain numbers or currency`);
    assert.doesNotMatch(text, /last updated|observed zero|tokens used|quota remaining|resets? (?:in|at)\b/iu);
    assert.match(text, /DEMO \/ SYNTHETIC — collection disabled/u);
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
    assert.equal(notes[0]?.[2], 'DEMO / SYNTHETIC — collection disabled');

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
  assert.equal(render(), renderToStaticMarkup(createElement(ShellView, {
    selection: initialSelection, onSelectEnvironment: noop, onSelectHarness: noop,
    copyDiagnostic: { status: 'idle', attempted: false },
  })));
  for (const selection of allSelections) {
    assert.equal(renderSelection(selection), renderSelection({ ...selection }));
  }
});

test('frontend allows only the reviewed core invoke and five reviewed commands, with no network, timers or storage', () => {
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
    // Each bridge has one core import and exactly its listed calls/arguments.
    const reviewed: Record<string, readonly string[]> = {
      'syntheticBridge.ts': ["invoke<unknown>('abf_get_synthetic_snapshot')"],
      'fictionalClaudeBridge.ts': ["invoke<unknown>('abf_get_fictional_claude_summary')"],
      'fictionalFileDiagnostic.ts': ["invoke<unknown>('abf_read_fictional_claude_file_once')"],
      'claudeCopyDiagnostic.ts': ["invoke<unknown>('abf_read_claude_copy_once', { relativePath })",
        "invoke<unknown>('abf_choose_claude_jsonl_once')"],
    };
    const calls = Object.entries(reviewed).find(([name]) => file === join(root, name))?.[1];
    if (calls !== undefined) {
      const allowedImport = "import { invoke } from '@tauri-apps/api/core';";
      assert.equal(source.split(allowedImport).length, 2, 'one official core invoke import');
      source = source.replace(allowedImport, '');
      for (const allowedCall of calls) {
        assert.equal(source.split(allowedCall).length, 2, 'one approved application command with exact arguments');
        source = source.replace(allowedCall, '');
      }
    }
    assert.doesNotMatch(source, forbidden, `${file} must not use forbidden capabilities`);
  }
});


test('accepts the version 2 demo transport', async () => {
  assert.equal(isSyntheticSnapshot(snapshot), true);
  assert.deepEqual(await createSyntheticBridge(async () => snapshot).load(), { status: 'synthetic', snapshot });
});

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
  test(`renders ${state.status} bridge state with truthful metrics in all views`, () => {
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
        state.status === 'synthetic'
          ? [String(expectedViews[selection.environment][selection.harness][0]), 'Unavailable', String(expectedViews[selection.environment][selection.harness][1]), '3'].map((value, index) => index === 3 && selection.harness !== 'general' ? '1' : value)
          : ['Unavailable', 'Unavailable', 'Unavailable', 'Unavailable']);
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

test('native boundary registers exactly five reviewed commands and enables only official IPC CSP endpoints', () => {
  const nativeRoot = new URL('../src-tauri/', import.meta.url);
  const main = readFileSync(new URL('src/main.rs', nativeRoot), 'utf8');
  const command = readFileSync(new URL('src/synthetic_snapshot.rs', nativeRoot), 'utf8');
  const build = readFileSync(new URL('build.rs', nativeRoot), 'utf8');
  const config = JSON.parse(readFileSync(new URL('tauri.conf.json', nativeRoot), 'utf8'));
  const handlers = [...main.matchAll(/\.invoke_handler\(tauri::generate_handler!\[([^\]]*)\]\)/gu)];
  assert.deepEqual(handlers.map((match) => match[1]?.replace(/\s+/gu, ' ').trim()), [
    'synthetic_snapshot::abf_get_synthetic_snapshot, fictional_claude_summary::abf_get_fictional_claude_summary, abf_read_fictional_claude_file_once, abf_read_claude_copy_once, abf_choose_claude_jsonl_once',
  ]);
  assert.match(main, /async fn abf_choose_claude_jsonl_once\(\s*window: tauri::WebviewWindow,\s*state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>,\s*\) -> Result<Option<String>, &'static str>/u,
    'picker IPC has only injected native window/state, with no frontend path parameter (static check)');
  const claude = readFileSync(new URL('src/fictional_claude_summary.rs', nativeRoot), 'utf8');
  assert.equal([...claude.matchAll(/#\[tauri::command\]/gu)].length, 1);
  assert.match(claude, /fn abf_get_fictional_claude_summary\(\) -> String/u);
  assert.match(claude, /abf_collection_boundary::claude_demo::fictional_demo_payload_json\(\)/u);
  // No argument, state, path, environment, file or process access in the command itself.
  assert.doesNotMatch(claude, /std::|tauri::(?:State|AppHandle|Manager)|\b(?:use|mod)\b|\b(?:async|unsafe)\b|SystemTime|rand|thread|spawn|include!|include_str!|include_bytes!|\benv\b|\bfs\b|\bpath\b|Path|File|Command/u);
  const manifest = readFileSync(new URL('Cargo.toml', nativeRoot), 'utf8');
  const dependencies = manifest.slice(manifest.indexOf('[dependencies]'), manifest.indexOf('[build-dependencies]'));
  assert.deepEqual(dependencies.split('\n').map((line) => line.trim()).filter((line) => line !== '' && line !== '[dependencies]'), [
    'abf-collection-boundary = { path = "../../../rust/crates/abf-collection-boundary" }',
    'serde = { version = "=1.0.229", features = ["derive"] }',
    'tauri = "=2.12.3"',
  ], 'only the reviewed local path dependency is added');
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

for (const selection of allSelections) {
  test(`accessible deterministic demo chart and breakdowns: ${selection.environment}/${selection.harness}`, () => {
    const props = { selection, onSelectEnvironment: noop, onSelectHarness: noop,
      bridgeState: { status: 'synthetic', snapshot } } as const;
    const html = renderToStaticMarkup(createElement(ShellView, props));
    assert.equal(html, renderToStaticMarkup(createElement(ShellView, props)));
    assert.match(html, /DEMO \/ SYNTHETIC/u);
    assert.match(html, /input tokens \+ output tokens/u);
    assert.match(html, /UTC/u);
    assert.match(html, /<progress[^>]*aria-label="[^"]+"[^>]*value="[0-9]+"/u);
    assert.match(html, /demo-(?:amber|birch|coral)/u);
    assert.match(html, /Estimated API cost/u);
    assert.equal(buttons(html).length, 7);
    assert.doesNotMatch(html, /<svg|<canvas|\sstyle=|(?:src|href)=/u);
    assert.match(html, /Synthetic sources:/u);
  });
}

test('empty validated snapshot renders unavailable rather than measured zero', () => {
  const html = renderToStaticMarkup(createElement(ShellView, {
    selection: initialSelection, onSelectEnvironment: noop, onSelectHarness: noop,
    bridgeState: { status: 'synthetic', snapshot: { ...snapshot, records: [] } },
  }));
  assert.deepEqual([...html.matchAll(/<p class="metric__value">(.*?)<\/p>/gu)].map((entry) => entry[1]),
    ['Unavailable', 'Unavailable', 'Unavailable', 'Unavailable']);
  assert.doesNotMatch(html, /<progress/u);
});

test('StrictMode setup/cleanup/setup uses the same module singleton and cached native invocation', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(snapshot); } },
  } });
  try {
    // Replay the effect's load boundary. SSR cannot mount/replay React effects.
    const firstSetup = syntheticBridge.load();
    const secondSetup = syntheticBridge.load();
    assert.equal(firstSetup, secondSetup);
    assert.deepEqual(await secondSetup, { status: 'synthetic', snapshot });
    for (const selection of allSelections) {
      renderSelection(selection);
      assert.equal(syntheticBridge.load(), firstSetup);
    }
    assert.deepEqual(calls, [['abf_get_synthetic_snapshot', {}, undefined]]);
    const appSource = readFileSync(new URL('../frontend/src/App.tsx', import.meta.url), 'utf8');
    assert.match(appSource, /React\.useEffect\(\(\) => \{[\s\S]*?syntheticBridge\.load\(\)[\s\S]*?\}, \[\]\);/u);
    assert.doesNotMatch(appSource, /createSyntheticBridge|\binvoke\b/u);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});

test('harness totals distinguish available DEMO values from unavailable data and derived scope', () => {
  const selection = { environment: 'computer', harness: 'general' } as const;
  const success = renderToStaticMarkup(createElement(ShellView, {
    selection, onSelectEnvironment: noop, onSelectHarness: noop,
    bridgeState: { status: 'synthetic', snapshot },
  }));
  const breakdown = success.match(/<section[^>]*aria-labelledby="harness-breakdown-title"[^>]*>(.*?)<\/section>/u)?.[1] ?? '';
  assert.deepEqual([...breakdown.matchAll(/<span class="status status--synthetic">(.*?)<\/span>/gu)].map((entry) => entry[1]),
    ['DEMO · 80 synthetic tokens', 'DEMO · 200 synthetic tokens', 'DEMO · 270 synthetic tokens']);
  assert.doesNotMatch(breakdown, /status--unavailable/u);
  assert.match(success, /class="status status--derived">Derived view only; no collection source/u);
  for (const state of [{ status: 'loading' }, { status: 'unavailable' }] as const) {
    const html = renderToStaticMarkup(createElement(ShellView, {
      selection, onSelectEnvironment: noop, onSelectHarness: noop, bridgeState: state,
    }));
    const absent = html.match(/<section[^>]*aria-labelledby="harness-breakdown-title"[^>]*>(.*?)<\/section>/u)?.[1] ?? '';
    assert.equal([...absent.matchAll(/class="status status--unavailable">Unavailable</gu)].length, 3);
    assert.doesNotMatch(absent, /status--synthetic|synthetic tokens/u);
  }
});

test('main scroller spans its grid area and only the inner column is width-limited', () => {
  // Static checks only: JSDOM and static rendering cannot prove WebView2 scrollbar geometry.
  const html = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'computer', harness: 'general' }, onSelectEnvironment: noop, onSelectHarness: noop,
    bridgeState: { status: 'loading' },
  }));
  const main = html.match(/<main class="content"[^>]*>([\s\S]*)<\/main>/u)?.[1] ?? '';
  assert.match(main, /^<div class="content__inner">/u);
  assert.match(main, /<caption class="visually-hidden">Models used<\/caption>/u);
  const css = readFileSync(new URL('../frontend/src/styles/shell.css', import.meta.url), 'utf8');
  const rule = (selector: string) => css.match(new RegExp(`(?:^|\\n)${selector.replace('.', '\\.')} \\{([^}]*)\\}`, 'u'))?.[1] ?? '';
  const scroller = rule('.content');
  assert.match(scroller, /overflow-y:\s*auto/u);
  assert.doesNotMatch(scroller, /(?<![-\w])(?:width|max-width|margin(?:-inline)?)\s*:/u);
  const inner = rule('.content__inner');
  assert.match(inner, /width:\s*min\(100%,\s*90rem\)/u);
  assert.match(inner, /margin-inline:\s*auto/u);
  assert.doesNotMatch(inner, /overflow/u);
  assert.match(css, /@media \(min-width: 120rem\)[\s\S]*\.content__inner \{ width: min\(100%, 110rem\); \}/u);
});
