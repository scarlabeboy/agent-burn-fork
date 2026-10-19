import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { aggregateSynthetic } from '../frontend/src/syntheticAggregation.ts';
import { createSyntheticBridge, isSyntheticSnapshot } from '../frontend/src/syntheticBridge.ts';
import { snapshot, expectedViews } from './syntheticFixture.ts';

for (const environment of ['windows', 'selected-wsl', 'computer'] as const) {
  for (const harness of ['general', 'claude-code', 'codex', 'opencode'] as const) {
    test(`independent aggregation totals and reconciliation: ${environment}/${harness}`, () => {
      const result = aggregateSynthetic(snapshot, environment, harness);
      assert.ok(result);
      assert.equal(result.tokens, expectedViews[environment][harness][0]);
      assert.equal(result.sessions, expectedViews[environment][harness][1]);
      assert.equal(result.models.length, harness === 'general' ? 3 : 1);
      assert.equal(result.activity.reduce((sum, row) => sum + row.tokens, 0), result.tokens);
      assert.equal(result.models.reduce((sum, row) => sum + row.tokens, 0), result.tokens);
      assert.equal(result.harnesses.reduce((sum, row) => sum + row.tokens, 0), result.tokens);
      assert.deepEqual(aggregateSynthetic({ ...snapshot, records: [...snapshot.records].reverse() }, environment, harness), result);
    });
  }
}

test('Computer deduplicates only the explicit copy and keeps colliding distinct occurrences', () => {
  const result = aggregateSynthetic(snapshot, 'computer', 'general');
  assert.ok(result);
  assert.equal(result.occurrences.length, 11);
  assert.deepEqual(result.activity, [
    { id: '2026-01-01', tokens: 80 }, { id: '2026-01-02', tokens: 200 }, { id: '2026-01-03', tokens: 270 },
  ]);
  assert.deepEqual(result.models, [
    { id: 'demo-amber', tokens: 80 }, { id: 'demo-birch', tokens: 200 }, { id: 'demo-coral', tokens: 270 },
  ]);
  assert.deepEqual(result.occurrences.find((row) => row.record.equivalenceId === 'demo-copy')?.sources, ['selected-wsl', 'windows']);
  assert.equal(aggregateSynthetic(snapshot, 'windows', 'codex')?.occurrences.length, 2);
});

test('session labels in separate sources are not equivalence evidence', () => {
  const records = snapshot.records.map((row) => ({ ...row, equivalenceId: null, sessionId: 'demo-same-label' }));
  assert.equal(aggregateSynthetic({ ...snapshot, records }, 'computer', 'general')?.sessions, 6);
  assert.equal(aggregateSynthetic({ ...snapshot, records }, 'computer', 'general')?.tokens, 565);
});

test('empty and unavailable data have no measured aggregate', () => {
  assert.equal(aggregateSynthetic(null, 'computer', 'general'), null);
  assert.equal(aggregateSynthetic({ ...snapshot, records: [] }, 'windows', 'general'), null);
  assert.equal(aggregateSynthetic({ ...snapshot, records: [snapshot.records[0]] }, 'selected-wsl', 'general'), null);
});

const badRecord = (patch: Record<string, unknown>) => ({ ...snapshot, records: [{ ...snapshot.records[0], equivalenceId: null, ...patch }] });
const invalidResponses: readonly unknown[] = [
  undefined, null, {}, [],
  { ...snapshot, schemaVersion: 1 }, { ...snapshot, schemaVersion: 3 }, { ...snapshot, provenance: 'observed' },
  { ...snapshot, extra: 1 }, { ...snapshot, records: undefined },
  { ...snapshot, sources: [] }, { ...snapshot, sources: [{ id: 'computer' }, { id: 'selected-wsl' }] },
  { ...snapshot, sources: [{ id: 'windows', extra: 1 }, { id: 'selected-wsl' }] },
  { ...snapshot, sources: [snapshot.sources[0], snapshot.sources[0]] },
  { ...snapshot, sources: [...snapshot.sources].reverse() },
  { ...snapshot, records: Array.from({ length: 101 }, (_, index) => ({ ...snapshot.records[0], id: `demo-over-${index}`, equivalenceId: null })) },
  { ...snapshot, records: [{ ...snapshot.records[0], equivalenceId: null }, { ...snapshot.records[0], equivalenceId: null }] },
  { ...snapshot, records: [snapshot.records[0]] }, // dangling equivalence marker
  ...['source', 'harness', 'id', 'sessionId', 'model', 'date', 'inputTokens', 'outputTokens', 'equivalenceId'].map((key) => badRecord({ [key]: undefined })),
  ...[-1, 0.5, Number.MAX_SAFE_INTEGER + 1, Number.MAX_SAFE_INTEGER, NaN, Infinity, 1_000_001, true, '1', null, {}, []].flatMap((value) => [badRecord({ inputTokens: value }), badRecord({ outputTokens: value })]),
  ...['2026-02-30', '2025-02-29', '2026-13-01', '2026-00-01', '2026-01-00', '2026-1-01', '2026-01-01T00:00:00Z', 'x'.repeat(10000)].map((date) => badRecord({ date })),
  badRecord({ source: 'computer' }), badRecord({ harness: 'general' }), badRecord({ id: '' }),
  badRecord({ model: 'commercial-model' }), badRecord({ extra: 0 }), badRecord({ id: 'demo-' + 'x'.repeat(100) }),
  { ...snapshot, records: snapshot.records.map((row) => row.id === 'demo-l1' ? { ...row, inputTokens: 11 } : row) },
  { ...snapshot, records: snapshot.records.map((row) => row.id === 'demo-l1' ? { ...row, source: 'windows' } : row) },
];
for (const [index, response] of invalidResponses.entries()) {
  test(`rejects malformed v2 response ${index + 1} as unavailable`, async () => {
    assert.equal(isSyntheticSnapshot(response), false);
    assert.deepEqual(await createSyntheticBridge(async () => response).load(), { status: 'unavailable' });
  });
}

test('zero counts and calendar leap day are valid; empty data is valid but unmeasured', () => {
  assert.equal(isSyntheticSnapshot(badRecord({ inputTokens: 0, outputTokens: 0, date: '2024-02-29' })), true);
  assert.equal(isSyntheticSnapshot({ ...snapshot, records: [] }), true);
});

test('Rust fixture rows exactly match the independently specified transport fixture', () => {
  const rust = readFileSync(new URL('../src-tauri/src/synthetic_snapshot.rs', import.meta.url), 'utf8');
  const rows = [...rust.replace(/\s+/gu, ' ').replace(/record\( /gu, 'record(').replace(/, \)/gu, ')').matchAll(/record\("([^"]+)", SourceId::(Windows|SelectedWsl), HarnessId::(ClaudeCode|Codex|OpenCode), "([^"]+)", (Some\("[^"]+"\)|None), "([^"]+)", "([^"]+)", (\d+), (\d+)\)/gu)];
  assert.equal(rows.length, 12);
  assert.deepEqual(rows.map((row) => ({ id: row[1], source: row[2] === 'Windows' ? 'windows' : 'selected-wsl',
    harness: { ClaudeCode: 'claude-code', Codex: 'codex', OpenCode: 'opencode' }[row[3] ?? ''],
    sessionId: row[4], equivalenceId: row[5] === 'None' ? null : row[5]?.slice(6, -2), model: row[6], date: row[7], inputTokens: Number(row[8]), outputTokens: Number(row[9]),
  })), snapshot.records);
  // Static source guards only: these do not execute Rust/Serde serialization.
  assert.match(rust, /#\[serde\(rename_all = "camelCase"\)\]\s+pub\(crate\) struct SyntheticSnapshot\b/u);
  assert.match(rust, /#\[serde\(rename_all = "camelCase"\)\]\s+struct SyntheticRecord\b/u);
  assert.match(rust, /#\[serde\(rename_all = "kebab-case"\)\]\s+enum SourceId\b/u);
  assert.match(rust, /#\[serde\(rename_all = "kebab-case"\)\]\s+enum HarnessId\b/u);
  assert.match(rust, /schema_version: 2/u);
  assert.match(rust, /provenance: "fixed-synthetic"/u);
});

test('deduplicated occurrences retain every original source record identity', () => {
  const result = aggregateSynthetic(snapshot, 'computer', 'general');
  assert.deepEqual(result?.occurrences.find((row) => row.record.equivalenceId === 'demo-copy')?.sourceRecords.map((row) => row.id), ['demo-l1', 'demo-w1']);
});

test('multiple explicit equivalences join sessions transitively without losing occurrence counts', () => {
  const records = [
    snapshot.records[0], snapshot.records[6],
    { ...snapshot.records[1], sessionId: 'demo-ws1', equivalenceId: 'demo-second-copy' },
    { ...snapshot.records[7], sessionId: 'demo-ls2', equivalenceId: 'demo-second-copy', inputTokens: 20 },
  ];
  assert.equal(isSyntheticSnapshot({ ...snapshot, records }), true);
  assert.equal(aggregateSynthetic({ ...snapshot, records }, 'computer', 'general')?.sessions, 1);
  assert.equal(aggregateSynthetic({ ...snapshot, records }, 'computer', 'general')?.tokens, 45);
});

test('historical null-only v1 response cannot become demo success', async () => {
  const legacy = { schemaVersion: 1, provenance: 'fixed-synthetic', sources: [
    { id: 'windows', observations: { usage: null, sessions: null, models: null, cost: null, quota: null } },
    { id: 'selected-wsl', observations: { usage: null, sessions: null, models: null, cost: null, quota: null } },
  ] };
  assert.deepEqual(await createSyntheticBridge(async () => legacy).load(), { status: 'unavailable' });
});

test('Rust source declares the approved opencode wire override (static only)', () => {
  const rust = readFileSync(new URL('../src-tauri/src/synthetic_snapshot.rs', import.meta.url), 'utf8');
  // Serde kebab-case would otherwise serialize OpenCode as open-code.
  assert.match(rust, /#\[serde\(rename = "opencode"\)\]\s+OpenCode/u);
});


test('maximum accepted fixture remains integer-safe and aggregation does not mutate input', () => {
  const records = Object.freeze(Array.from({ length: 100 }, (_, index) => Object.freeze({
    ...snapshot.records[0], id: `demo-bound-${index}`, equivalenceId: null,
    inputTokens: 1_000_000, outputTokens: 1_000_000,
  })));
  const bounded = Object.freeze({ ...snapshot, records });
  assert.equal(isSyntheticSnapshot(bounded), true);
  assert.equal(aggregateSynthetic(bounded, 'windows', 'general')?.tokens, 200_000_000);
});
