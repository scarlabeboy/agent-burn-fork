import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { isClaudeCopyActivity } from '../frontend/src/claudeCopyActivity.ts';
import { createClaudeCopyDiagnostic, isClaudeCopyResult } from '../frontend/src/claudeCopyDiagnostic.ts';
import type { CopyDiagnosticState } from '../frontend/src/claudeCopyDiagnostic.ts';
import { ShellView } from '../frontend/src/App.tsx';
import type { Selection } from '../frontend/src/selection.ts';
import { snapshot } from './syntheticFixture.ts';

const golden = readFileSync(new URL('../fixtures/abf150c-daily-payload.json', import.meta.url), 'utf8').trim();
function result() {
  const value: unknown = JSON.parse(golden);
  assert.ok(isClaudeCopyResult(value, true));
  assert.ok(value.schemaVersion === 2);
  const activity = value.activity;
  assert.ok(activity.status === 'available');
  return { ...value, activity };
}
const render = (state: CopyDiagnosticState, selection: Selection = { environment: 'windows', harness: 'claude-code' }) =>
  renderToStaticMarkup(createElement(ShellView, {
    selection, onSelectEnvironment: () => { }, onSelectHarness: () => { },
    bridgeState: { status: 'synthetic', snapshot }, copyDiagnostic: state
  }));
const readerFor = (payload: string) => createClaudeCopyDiagnostic(undefined, undefined, async () => 2, async () => payload, async () => { });

test('daily golden renders chronological raw claims with acquisition provenance', async () => {
  const reader = readerFor(golden);
  await reader.chooseFolder();
  assert.doesNotMatch(render(reader.getSnapshot()), /Claude daily claims/u);
  await reader.analyzeFolder();
  const markup = render(reader.getSnapshot());
  assert.match(markup, /Claude daily claims/u);
  assert.match(markup, /Selected Windows folder; original environment not established/u);
  assert.match(markup, /2025-12-31/u);
  assert.match(markup, /<td>20<\/td>.*<td>6<\/td>.*<td>22<\/td>.*<td>8<\/td>/u);
  assert.match(markup, /Repeated observations remain counted/u);
  assert.doesNotMatch(markup, /PRIVATE|00:15:00|\.jsonl<|fictional-model/u);
});

test('daily budget exhaustion preserves diagnostics', async () => {
  const value = result();
  const reader = readerFor(JSON.stringify({ ...value, activity: { status: 'budget-exhausted' } }));
  await reader.chooseFolder();
  await reader.analyzeFolder();
  assert.equal(reader.getSnapshot().status, 'copy');
  const markup = render(reader.getSnapshot());
  assert.match(markup, /Daily activity unavailable - projection budget exceeded/u);
  assert.match(markup, /Manual copy session snapshot/u);
  assert.match(markup, /Token, parser and replay diagnostics/u);
});

test('daily Clear suppresses late completion and removes all activity', async () => {
  let finish: (value: string) => void = () => { };
  const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2,
    () => new Promise<string>((resolve) => { finish = resolve; }), async () => { });
  await reader.chooseFolder();
  const reading = reader.analyzeFolder();
  reader.clear();
  finish(golden);
  await reading;
  assert.deepEqual(reader.getSnapshot(), { status: 'cleared', attempted: true });
  assert.doesNotMatch(render(reader.getSnapshot()), /Claude daily claims|2025-12-31/u);
  await reader.analyzeFolder();
  assert.equal(reader.getSnapshot().status, 'cleared');
});

test('daily navigation retains the result without leaking into WSL, General or Computer', async () => {
  let reads = 0;
  const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2,
    async () => { reads++; return golden; }, async () => { });
  await reader.chooseFolder();
  await reader.analyzeFolder();
  const state = reader.getSnapshot();
  for (const selection of [{ environment: 'selected-wsl', harness: 'claude-code' },
  { environment: 'computer', harness: 'claude-code' }, { environment: 'windows', harness: 'general' }] as const satisfies readonly Selection[]) {
    assert.doesNotMatch(render(state, selection), /Claude daily claims|2025-12-31/u);
    assert.equal(render(state, selection), render({ status: 'idle', attempted: false }, selection));
  }
  assert.match(render(state), /Claude daily claims/u);
  assert.equal(reads, 1);
  reader.clear();
  assert.doesNotMatch(render(reader.getSnapshot()), /Claude daily claims|2025-12-31/u);
});

for (const activity of [
  { status: 'budget-exhausted', path: 'PRIVATE' },
  { status: 'available', rows: [] },
  { status: 'available', rows: [{ day: '2025-12-31T00:00:00Z', observations: 2 }] },
]) {
  test('daily validation refuses invalid or private projections', () => {
    assert.equal(isClaudeCopyResult({ ...result(), activity }, true), false);
  });
}

test('daily schema validates days, ordering, counts, values and exact privacy keys', () => {
  const value = result();
  const row = value.activity.rows[0];
  assert.ok(row);
  for (const invalid of [
    { ...row, day: '2025-02-29' }, { ...row, day: '0000-01-01' }, { ...row, day: '2025-12-31T00:00:00Z' },
    { ...row, path: 'PRIVATE' }, { ...row, observations: 4097 },
    { ...row, components: { ...row.components, input: { ...row.components.input, value: { kind: 'sum', amount: '21' } } } },
  ]) assert.equal(isClaudeCopyActivity({ status: 'available', rows: [invalid] }, value.raw), false);
  assert.equal(isClaudeCopyActivity({ status: 'available', rows: [row, row] }, value.raw), false);
  assert.equal(isClaudeCopyActivity({ status: 'available', rows: Array.from({ length: 33 }, () => row) }, value.raw), false);
  assert.equal(isClaudeCopyResult(value), false);
});

test('daily multiple rows sort before Undated and reconcile across days', async () => {
  const value = result();
  const components = value.replay.status === 'evaluated' ? value.replay.components : assert.fail('expected fixture replay');
  const rows = [
    { day: '2025-12-30', observations: 1, components },
    { day: '2025-12-31', observations: 1, components },
  ];
  const activity = { status: 'available', rows };
  assert.equal(isClaudeCopyActivity(activity, value.raw), true);
  assert.equal(isClaudeCopyActivity({ ...activity, rows: [...rows].reverse() }, value.raw), false);
  const first = rows[0];
  const last = rows[1];
  assert.ok(first && last);
  assert.equal(isClaudeCopyActivity({ ...activity, rows: [first, { ...last, day: null }] }, value.raw), true);
  assert.equal(isClaudeCopyActivity({ ...activity, rows: [{ ...first, day: null }, last] }, value.raw), false);
  const reader = readerFor(JSON.stringify({ ...value, activity }));
  await reader.chooseFolder();
  await reader.analyzeFolder();
  const markup = render(reader.getSnapshot());
  assert.ok(markup.indexOf('2025-12-30') < markup.indexOf('2025-12-31'));
});

test('daily incomplete folder coverage stays visible with valid activity', async () => {
  const value = result();
  const payload = {
    ...value, raw: {
      ...value.raw, completeness: 'incomplete',
      skipped: { total: 1, mayHideData: 1, outOfScope: 0 },
      caveats: { ...value.raw.caveats, incompleteBatch: true, skippedInScope: true },
    }
  };
  const reader = readerFor(JSON.stringify(payload));
  await reader.chooseFolder();
  await reader.analyzeFolder();
  const markup = render(reader.getSnapshot());
  assert.match(markup, /Claude daily claims/u);
  assert.match(markup, /folder coverage is incomplete/u);
  assert.match(markup, /Coverage limited/u);
});

test('daily partial component sums disclose observations without valid claims', async () => {
  const value = result();
  const row = value.activity.rows[0];
  assert.ok(row);
  const input = { value: { kind: 'sum', amount: '10' }, states: { valid: 1, validZero: 0,
    absent: 1, null: 0, wrongType: 0, outOfRange: 0, unsupported: 0 } };
  const components = { ...row.components, input };
  const payload = { ...value, raw: { ...value.raw, components,
    records: { ...value.raw.records, withMissingClaim: 1 } }, replay: { status: 'allocation-failure' },
    activity: { status: 'available', rows: [{ ...row, components }] } };
  const reader = readerFor(JSON.stringify(payload));
  await reader.chooseFolder();
  await reader.analyzeFolder();
  assert.ok(render(reader.getSnapshot()).includes('1 without valid claims'));
});

test('daily transport retains the exact response cap', async () => {
  const exact = readerFor(golden.padEnd(16384, ' '));
  await exact.chooseFolder();
  await exact.analyzeFolder();
  assert.equal(exact.getSnapshot().status, 'copy');
  const over = readerFor(golden.padEnd(16385, ' '));
  await over.chooseFolder();
  await over.analyzeFolder();
  assert.equal(over.getSnapshot().status, 'unavailable');
});

test('daily undated row preserves explicit zero, missing values and overflow displays', async () => {
  const value = result();
  const state = { valid: 2, validZero: 0, absent: 0, null: 0, wrongType: 0, outOfRange: 0, unsupported: 0 };
  const components = {
    input: { value: { kind: 'overflow' }, states: state },
    output: { value: { kind: 'sum', amount: '0' }, states: { ...state, validZero: 2 } },
    cacheCreation: { value: { kind: 'no-valid-claims' }, states: { ...state, valid: 0, absent: 2 } },
    cacheRead: { value: { kind: 'no-valid-claims' }, states: { ...state, valid: 0, null: 2 } },
  };
  const payload = {
    ...value, raw: { ...value.raw, components }, replay: { status: 'allocation-failure' },
    activity: { status: 'available', rows: [{ day: null, observations: 2, components }] }
  };
  const reader = readerFor(JSON.stringify(payload));
  await reader.chooseFolder();
  await reader.analyzeFolder();
  const markup = render(reader.getSnapshot());
  assert.match(markup, /Undated/u);
  assert.match(markup, /Overflow - sum exceeds 64 bits/u);
  assert.match(markup, /<td>0<\/td>/u);
  assert.match(markup, /No valid claims/u);
});
