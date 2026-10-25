import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createFictionalClaudeBridge, isFictionalClaudeSummary } from '../frontend/src/fictionalClaudeBridge.ts';
import type { FictionalClaudeState, FictionalClaudeSummary } from '../frontend/src/fictionalClaudeBridge.ts';
import { ShellView } from '../frontend/src/App.tsx';
import type { Selection } from '../frontend/src/selection.ts';
import { environments, harnesses } from '../frontend/src/synthetic.ts';

// Rust output of the real parser, collector and summary over the fixed fictional fixture.
// The Rust suite asserts this file equals fictional_demo_payload_json() byte for byte.
const golden = readFileSync(
  new URL('../../../rust/crates/abf-collection-boundary/fixtures/fictional-claude-demo/expected-payload.json', import.meta.url),
  'utf8',
).trim();
const payload = JSON.parse(golden) as FictionalClaudeSummary;
const clone = (): any => JSON.parse(golden);

const noop = () => undefined;
const target: Selection = { environment: 'selected-wsl', harness: 'claude-code' };
const render = (selection: Selection, fictionalClaude?: FictionalClaudeState) =>
  renderToStaticMarkup(createElement(ShellView, { selection, onSelectEnvironment: noop, onSelectHarness: noop, fictionalClaude }));
const fictional = (summary: unknown): FictionalClaudeState => ({ status: 'fictional', summary: summary as FictionalClaudeSummary });
const textOf = (html: string) => html.replace(/<[^>]*>/gu, ' ').replace(/\s+/gu, ' ');
type Row = Readonly<{ outcome: string | undefined; cells: Array<string | undefined> }>;
const rows = (html: string): Record<string, Row> => Object.fromEntries(
  [...html.matchAll(/<tr data-component="(\w+)" data-outcome="([\w-]+)">(.*?)<\/tr>/gu)].map((row) => [
    row[1],
    { outcome: row[2], cells: [...(row[3] ?? '').matchAll(/<t[dh][^>]*>(.*?)<\/t[dh]>/gu)].map((cell) => cell[1]) },
  ]),
);
const allSelections: Selection[] = environments.flatMap((e) => harnesses.map((h) => ({ environment: e.id, harness: h.id })));

test('the Rust-produced payload validates and loads through the bridge as a string', async () => {
  assert.equal(isFictionalClaudeSummary(payload), true);
  const state = await createFictionalClaudeBridge(async () => golden).load();
  assert.deepEqual(state, { status: 'fictional', summary: payload });
  assert.equal(payload.provenance, 'fictional-fixture');
  assert.equal(payload.hostMatch, 'simulated');
  assert.equal(payload.duplicates, 'unresolved');
});

test('displayed token values come from the payload and keep the four components separate', () => {
  const html = render(target, fictional(payload));
  const table = rows(html);
  assert.deepEqual(Object.keys(table), ['input', 'output', 'cacheCreation', 'cacheRead']);
  for (const [id, label] of [['input', 'Input tokens'], ['output', 'Output tokens'],
    ['cacheCreation', 'Cache creation tokens'], ['cacheRead', 'Cache read tokens']] as const) {
    const component = payload.components[id];
    assert.equal(component.value.kind, 'sum');
    const amount = component.value.kind === 'sum' ? component.value.amount : '';
    assert.deepEqual(table[id]?.cells.slice(0, 2), [label, `${amount} observed claims (fictional)`]);
    assert.deepEqual(table[id]?.cells.slice(2), [component.states.valid, component.states.validZero, component.states.absent,
      component.states.null, component.states.wrongType + component.states.outOfRange, component.states.unsupported].map(String));
  }
  // Hand-checked against the fixture design (Rust asserts the same independently): 31 / 69 / 20 / 12.
  assert.deepEqual(['input', 'output', 'cacheCreation', 'cacheRead'].map((id) => table[id]?.cells[1]),
    ['31 observed claims (fictional)', '69 observed claims (fictional)', '20 observed claims (fictional)', '12 observed claims (fictional)']);
  // Not a hardcoded view: changing the payload changes the display.
  const changed = clone();
  changed.components.input.value.amount = '77';
  assert.equal(rows(render(target, fictional(changed))).input?.cells[1], '77 observed claims (fictional)');
  // No combined figure, cost or quota claim anywhere in the panel.
  const panel = html.match(/<section[^>]*aria-labelledby="fictional-claude-title"[\s\S]*?<\/section>/u)?.[0] ?? '';
  // Apart from the explicit disclaimer sentence that none of it is read.
  const shown = textOf(panel).replace(/No real Claude history[^.]*\./u, '');
  assert.doesNotMatch(shown, /combined|estimated|\bcost\b|quota|spend|\$/iu);
  assert.equal(panel.match(/<tr /gu)?.length, 4, 'exactly the four separate components');
});

test('counting basis, duplicates, record counts and coverage gaps are visible', () => {
  const text = textOf(render(target, fictional(payload)));
  assert.match(text, /FICTIONAL FIXTURE · not your data/u);
  assert.match(text, /observed record claims, every record counted/u);
  assert.match(text, /Duplicate-record identity is unresolved/u);
  assert.match(text, /neither unique events nor billed consumption/u);
  assert.match(text, /not a Selected WSL, Windows or Computer total/u);
  assert.match(text, /Records parsed 8 \(accepted 5, qualified 1, incomplete 2\)/u);
  assert.match(text, /Files read \/ rejected 2 \/ 0/u);
  assert.match(text, /Parser rejections \/ oversized \/ blank 1 \/ 1 \/ 1/u);
  assert.match(text, /Pending tails 1 file\(s\), \d+ bytes, not parsed/u);
  assert.match(text, /Collection complete/u);
  assert.match(text, /Parser gaps: 1 rejected and 1 oversized lines were not counted\./u);
  assert.match(text, /Pending tails: \d+ bytes in 1 file\(s\) were not parsed\./u);
  assert.match(text, /\(1 record\(s\) had both\)/u);
  assert.doesNotMatch(text, /Incomplete coverage/u, 'a complete traversal is not labelled incomplete');
});

test('incomplete coverage and rejected files are reported, and a clean summary is not a proof', () => {
  const partial = clone();
  partial.completeness = 'incomplete';
  partial.caveats.incompleteBatch = true;
  partial.files.rejected = 2; partial.files.changedDuringRead = 1; partial.files.io = 1;
  partial.caveats.rejectedFiles = true;
  partial.skipped = { total: 1, mayHideData: 1, outOfScope: 0 };
  partial.caveats.skippedInScope = true;
  assert.equal(isFictionalClaudeSummary(partial), true);
  const text = textOf(render(target, fictional(partial)));
  assert.match(text, /Incomplete coverage: traversal or reading did not finish/u);
  assert.match(text, /Rejected files: 2 file\(s\) were not read\./u);
  assert.match(text, /Skipped entries: 1 may hide data\./u);
  assert.match(text, /Collection incomplete/u);

  const clean = clone();
  clean.lines.rejected = Object.fromEntries(Object.keys(clean.lines.rejected).map((key) => [key, 0]));
  Object.assign(clean.lines, { oversized: 0, filesWithPendingTail: 0, pendingTailBytes: 0 });
  Object.assign(clean.caveats, { unparsedLines: false, pendingTails: false });
  assert.equal(isFictionalClaudeSummary(clean), true);
  const cleanText = textOf(render(target, fictional(clean)));
  assert.match(cleanText, /No known gaps observed\. This is not a completeness proof\./u);
});

test('valid zero, no valid claims, no observations and overflow are all distinguishable', () => {
  const states = (valid: number, validZero: number, other: number) =>
    ({ valid, validZero, absent: other, null: 0, wrongType: 0, outOfRange: 0, unsupported: 0 });
  const variant = clone();
  variant.records = { ...variant.records, observations: 3, accepted: 3, qualified: 0, incomplete: 0, withValidClaim: 3, withoutValidClaim: 0 };
  variant.components.input = { value: { kind: 'sum', amount: '0' }, states: states(3, 3, 0) };
  variant.components.output = { value: { kind: 'no-valid-claims' }, states: states(0, 0, 3) };
  variant.components.cacheCreation = { value: { kind: 'overflow' }, states: states(2, 0, 1) };
  variant.components.cacheRead = { value: { kind: 'sum', amount: '18446744073709551615' }, states: states(1, 0, 2) };
  assert.equal(isFictionalClaudeSummary(variant), true);
  const table = rows(render(target, fictional(variant)));
  assert.equal(table.input?.cells[1], '0 observed claims (fictional)');
  assert.equal(table.input?.cells[3], '3', 'explicit zero count shown');
  assert.equal(table.output?.cells[1], 'No valid claims');
  assert.equal(table.cacheCreation?.cells[1], 'Overflow - sum exceeds 64 bits');
  assert.equal(table.cacheRead?.cells[1], '18446744073709551615 observed claims (fictional)');
  assert.deepEqual(Object.values(table).map((row) => row.outcome), ['sum', 'no-valid-claims', 'overflow', 'sum']);

  const empty = clone();
  empty.records = Object.fromEntries(Object.keys(empty.records).map((key) => [key, 0]));
  for (const id of ['input', 'output', 'cacheCreation', 'cacheRead']) {
    empty.components[id] = { value: { kind: 'indeterminate', reason: 'no-observations' }, states: states(0, 0, 0) };
  }
  assert.equal(isFictionalClaudeSummary(empty), true);
  const emptyTable = rows(render(target, fictional(empty)));
  for (const row of Object.values(emptyTable)) assert.equal(row.cells[1], 'Unavailable - no observations');
});

test('the panel appears only for Selected WSL / Claude Code and leaves every other view unchanged', () => {
  for (const selection of allSelections) {
    const plain = render(selection);
    for (const state of [{ status: 'loading' }, { status: 'unavailable' }, fictional(payload)] as FictionalClaudeState[]) {
      const withPanel = render(selection, state);
      if (selection.environment === target.environment && selection.harness === target.harness) {
        assert.match(withPanel, /Fictional Claude usage summary - demonstration only/u);
        assert.notEqual(withPanel, plain);
      } else {
        assert.equal(withPanel, plain, `${selection.environment}/${selection.harness} must be unchanged`);
      }
    }
  }
  // Without the prop the target view is also exactly what it was before this gate.
  assert.doesNotMatch(render(target), /fictional-claude/u);
  // Windows, WSL and Computer states keep their own text beside the fictional panel.
  const html = render(target, fictional(payload));
  assert.match(html, /Not connected; no distro discovered/u);
  assert.match(html, /DEMO \/ SYNTHETIC — collection disabled/u);
  const computer = render({ environment: 'computer', harness: 'claude-code' }, fictional(payload));
  assert.match(computer, /Derived view only; no collection source/u);
  assert.doesNotMatch(computer, /FICTIONAL FIXTURE/u);
});

test('loading and unavailable states are explicit and show no numbers', () => {
  const loading = textOf(render(target, { status: 'loading' }));
  assert.match(loading, /Fictional Claude summary: loading/u);
  const unavailable = render(target, { status: 'unavailable' });
  assert.match(textOf(unavailable), /Fictional Claude summary unavailable\. Nothing is shown in its place\./u);
  assert.doesNotMatch(unavailable, /data-component|observed claims/u);
});

test('malformed or inconsistent payloads are rejected rather than displayed', async () => {
  const mutations: Array<[string, (value: any) => void]> = [
    ['other provenance', (v) => { v.provenance = 'local-history'; }],
    ['native host match', (v) => { v.hostMatch = 'native'; }],
    ['schema 2', (v) => { v.schemaVersion = 2; }],
    ['basis', (v) => { v.basis = 'unique-events'; }],
    ['duplicates resolved', (v) => { v.duplicates = 'resolved'; }],
    ['extra key', (v) => { v.sessionId = 'x'; }],
    ['missing key', (v) => { delete v.caveats; }],
    ['amount over u64', (v) => { v.components.input.value.amount = '18446744073709551616'; }],
    ['amount leading zero', (v) => { v.components.input.value.amount = '031'; }],
    ['amount number', (v) => { v.components.input.value.amount = 31; }],
    ['negative count', (v) => { v.records.accepted = -1; }],
    ['fractional count', (v) => { v.records.observations = 8.5; }],
    ['state partition', (v) => { v.components.output.states.absent += 1; }],
    ['zero above valid', (v) => { v.components.input.states.validZero = 99; }],
    ['sum without valid claims', (v) => { v.components.input.states = { ...v.components.input.states, valid: 0, absent: 6 }; }],
    ['status partition', (v) => { v.records.incomplete += 1; }],
    ['rejection total', (v) => { v.lines.rejected.total += 1; }],
    ['file reason total', (v) => { v.files.rejected = 1; }],
    ['caveat disagrees', (v) => { v.caveats.pendingTails = false; }],
    ['incomplete flag disagrees', (v) => { v.completeness = 'incomplete'; }],
    ['overflow needs two claims', (v) => {
      v.components.input = { value: { kind: 'overflow' },
        states: { valid: 1, validZero: 0, absent: 7, null: 0, wrongType: 0, outOfRange: 0, unsupported: 0 } };
    }],
    ['unknown outcome', (v) => { v.components.input.value = { kind: 'estimated' }; }],
  ];
  for (const [name, mutate] of mutations) {
    const value = clone();
    mutate(value);
    assert.equal(isFictionalClaudeSummary(value), false, name);
  }
  for (const raw of ['not json', '', 'null', '[]', '{}', 42, null, undefined]) {
    assert.deepEqual(await createFictionalClaudeBridge(async () => raw).load(), { status: 'unavailable' });
  }
  assert.deepEqual(await createFictionalClaudeBridge(async () => { throw new Error('native detail'); }).load(), { status: 'unavailable' });
  assert.deepEqual(await createFictionalClaudeBridge().load(), { status: 'unavailable' }, 'missing runtime');
});

test('official core invoke sends only the fictional command with no argument and is shared', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(golden); } },
  } });
  try {
    const bridge = createFictionalClaudeBridge();
    const first = bridge.load();
    assert.equal(bridge.load(), first);
    assert.deepEqual(await first, { status: 'fictional', summary: payload });
    assert.deepEqual(calls, [['abf_get_fictional_claude_summary', {}, undefined]]);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});

test('payload and rendered panel carry no identifiers, content, paths or private metadata', () => {
  const html = render(target, fictional(payload));
  for (const secret of ['FICTIONAL-DEMO-CONTENT', 'demo-session', 'demo-req', 'demo-msg', 'fictional-model', 'demo-project',
    'Fictional-Demo', 'demo-env', 'demo-source', 'demo-run', '2026-01-02', 'jsonl']) {
    assert.ok(!golden.includes(secret), `payload leaked ${secret}`);
    assert.ok(!html.includes(secret), `view leaked ${secret}`);
  }
  assert.ok(!golden.includes('fixtures') && !golden.includes('/') && !golden.includes('\\'));
  assert.doesNotMatch(html, /<svg|<canvas|\sstyle=|(?:src|href)=/u);
  assert.equal(render(target, fictional(payload)), render(target, fictional(payload)), 'deterministic');
});
