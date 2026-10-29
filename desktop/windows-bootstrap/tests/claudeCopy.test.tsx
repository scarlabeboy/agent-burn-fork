import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createClaudeCopyDiagnostic, isClaudeCopySummary, isClaudeCopyResult } from '../frontend/src/claudeCopyDiagnostic.ts';
import type { ClaudeCopySummary } from '../frontend/src/claudeCopyDiagnostic.ts';
import type { ComponentSummary } from '../frontend/src/fictionalClaudeBridge.ts';
import { ShellView } from '../frontend/src/App.tsx';
const fixture: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf136-expected-payload.json', import.meta.url), 'utf8'));
const rawGolden = readFileSync(new URL('../fixtures/abf137-expected-payload.json', import.meta.url), 'utf8').trim();
const golden = JSON.stringify({ schemaVersion: 1, raw: JSON.parse(rawGolden), replay: { status: 'allocation-failure' } });
const copy: unknown = JSON.parse(rawGolden);
assert.ok(isClaudeCopySummary(copy));
test('copy validator requires separate provenance and strict numerical payload', () => {
  assert.equal(isClaudeCopySummary(copy), true);
  for (const bad of [fixture, { ...copy, environmentKind: 'windows' }, { ...copy, hostMatch: 'native' }, { ...copy, secret: 'PRIVATE' },
    { ...copy, originalEnvironment: 'wsl' }, { ...copy, originalSource: 'claude' },
    { ...copy, records: { ...copy.records, observations: -1 } }, { ...copy, files: { ...copy.files, path: 100 } },
    { ...copy, components: { ...copy.components, input: { ...copy.components.input, secret: 'PRIVATE' } } }]) {
    assert.equal(isClaudeCopySummary(bad), false);
  }
});
test('copy request is manual, one-shot and clear never rearms', async () => {
  const paths: string[] = [];
  const reader = createClaudeCopyDiagnostic(async (path) => { paths.push(path); return golden; });
  assert.deepEqual(paths, []);
  await Promise.all([reader.run('chosen/copy.jsonl'), reader.run('other.jsonl')]);
  assert.deepEqual(paths, ['chosen/copy.jsonl']);
  assert.equal(reader.getSnapshot().status, 'copy');
  reader.clear();
  await reader.run('again.jsonl');
  assert.equal(reader.getSnapshot().status, 'cleared');
  assert.equal(paths.length, 1);
});
test('clear suppresses late copy completion', async () => {
  let finish: (value: unknown) => void = () => assert.fail('not requested');
  const reader = createClaudeCopyDiagnostic(() => new Promise((resolve) => { finish = resolve; }));
  const pending = reader.run('copy.jsonl');
  reader.clear();
  finish(golden);
  await pending;
  assert.equal(reader.getSnapshot().status, 'cleared');
});
for (const raw of ['{}', 'invalid', ' '.repeat(16385), fixture, golden.replace('not-established', 'native-wsl')]) {
  test('invalid copy transport is unavailable', async () => {
    const reader = createClaudeCopyDiagnostic(async () => raw);
    await reader.run('copy.jsonl');
    assert.deepEqual(reader.getSnapshot(), { status: 'unavailable', attempted: true, code: 'COPY_RESPONSE_INVALID' });
  });
}
test('errors expose only allowlisted codes and remain one-shot', async () => {
  const coded = createClaudeCopyDiagnostic(async () => { throw 'COPY_LOCALITY_UNCOVERED'; });
  await coded.run('copy.jsonl');
  assert.deepEqual(coded.getSnapshot(), { status: 'unavailable', attempted: true, code: 'COPY_LOCALITY_UNCOVERED' });
  let calls = 0;
  const reader = createClaudeCopyDiagnostic(async () => { calls++; throw new Error('PRIVATE-PATH'); });
  await reader.run('copy.jsonl');
  await reader.run('copy.jsonl');
  assert.equal(calls, 1);
  assert.deepEqual(reader.getSnapshot(), { status: 'unavailable', attempted: true, code: 'COPY_READ_FAILED' });
});
test('copy panel reuses claims without fictional labeling', async () => {
  const reader = createClaudeCopyDiagnostic(async () => golden);
  await reader.run('copy.jsonl');
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  const start = markup.indexOf('<section class="panel" aria-labelledby="claude-copy-title"');
  const panel = markup.slice(start, markup.indexOf('</section>', start));
  assert.match(panel, /original source and environment are not established/u);
  assert.match(panel, /17 observed claims/u);
  assert.match(panel, /Pending tails/u);
  assert.doesNotMatch(panel, /fictional|FICTIONAL|native WSL|copy.jsonl/u);
});

test('official copy IPC sends only selected relative path', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(golden); } },
  } });
  try {
    const reader = createClaudeCopyDiagnostic();
    assert.equal(calls.length, 0);
    await reader.run('chosen/copy.jsonl');
    assert.deepEqual(calls, [['abf_read_claude_copy_once', { relativePath: 'chosen/copy.jsonl' }, undefined]]);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});
test('cleared copy renders no prior claims and controls stay consumed', async () => {
  const reader = createClaudeCopyDiagnostic(async () => golden);
  await reader.run('copy.jsonl');
  reader.clear();
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  assert.doesNotMatch(markup, /data-component=/u);
  assert.match(markup, /Attempt remains consumed/u);
  assert.match(markup, /disabled=""[^>]*>Read selected copy once/u);
});
test('shell scopes demo-only claims when a separate copy reader is present', () => {
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: { status: 'idle', attempted: false },
  }));
  assert.match(markup, /Dashboard is synthetic; manual copy reader is separate/u);
  assert.doesNotMatch(markup, /nothing has been observed|Collection is disabled in this shell/u);
});

const noUsage: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf139-no-usage-payload.json', import.meta.url), 'utf8'));
const mixed: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf139-mixed-payload.json', import.meta.url), 'utf8'));
assert.ok(isClaudeCopySummary(noUsage));
assert.ok(isClaudeCopySummary(mixed));
function copyPanel(summary: ClaudeCopySummary): string {
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {},
    copyDiagnostic: { status: 'copy', attempted: true, summary, replay: { status: 'allocation-failure' } },
  }));
  const start = markup.indexOf('<section class="panel" aria-labelledby="claude-copy-title"');
  return markup.slice(start, markup.indexOf('</section>', start));
}
test('successful no-usage session explains missing claims without claiming zero usage', () => {
  const panel = copyPanel(noUsage);
  assert.match(panel, /File read succeeded; no valid token claims were recognized/u);
  assert.match(panel, /does not establish zero usage or a parser defect/u);
  assert.match(panel, /Session events and records with absent or null usage can have no token claims/u);
  assert.doesNotMatch(panel, /0 observed claims|File was not read/u);
});
test('mixed session displays recognized claims and warns about incomplete token coverage', () => {
  const panel = copyPanel(mixed);
  assert.match(panel, /File read succeeded; valid token claims were recognized/u);
  assert.match(panel, /130 observed claims/u);
  assert.match(panel, /Records without valid claims and missing, null or invalid components leave token coverage incomplete/u);
});
test('read result with discarded lines or tails explains uncovered input', () => {
  assert.match(copyPanel(copy), /Malformed, oversized or parser-refused lines and unterminated tails are not counted/u);
});
test('file failures and record exhaustion are not presented as successful no-usage reads', () => {
  const unobserved = {
    value: { kind: 'indeterminate', reason: 'no-observations' },
    states: { ...noUsage.components.input.states, absent: 0, null: 0, unsupported: 0 },
  } as const satisfies ComponentSummary;
  const failed = {
    ...noUsage, completeness: 'incomplete',
    components: { input: unobserved, output: unobserved, cacheCreation: unobserved, cacheRead: unobserved },
    records: { observations: 0, accepted: 0, qualified: 0, incomplete: 0, withValidClaim: 0, withoutValidClaim: 0,
      withInvalidClaim: 0, withNullClaim: 0, withMissingClaim: 0, cacheCreationFlatIgnored: 0 },
    lines: { ...noUsage.lines, terminatedLines: 0 },
    files: { ...noUsage.files, read: 0, rejected: 1, outputLimit: 1 },
    caveats: { ...noUsage.caveats, incompleteBatch: true, rejectedFiles: true },
  } as const satisfies ClaudeCopySummary;
  assert.ok(isClaudeCopySummary(failed));
  const panel = copyPanel(failed);
  assert.match(panel, /File was not read successfully/u);
  assert.match(panel, /Record budget exhaustion discards all claims from the file/u);
  assert.doesNotMatch(panel, /File read succeeded/u);
});

test('copy envelope shows replay unavailable while preserving raw totals', async () => {
  const reader = createClaudeCopyDiagnostic(async () => golden);
  await reader.run('copy.jsonl');
  assert.equal(reader.getSnapshot().status, 'copy');
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  assert.match(markup, /Replay evaluation unavailable: allocation failure/u);
  assert.match(markup, /17 observed claims/u);
  assert.doesNotMatch(markup, /Eligible observations:/u);
});


const replayFixture: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf141-replay-payload.json', import.meta.url), 'utf8'));
assert.ok(isClaudeCopyResult(replayFixture));
assert.equal(replayFixture.replay.status, 'evaluated');
// Narrow once at the untyped IPC boundary.
if (replayFixture.replay.status !== 'evaluated') throw new Error('Expected evaluated fixture');
const evaluated = replayFixture.replay;
const noObservations = {
  value: { kind: 'indeterminate', reason: 'no-observations' },
  states: { valid: 0, validZero: 0, absent: 0, null: 0, wrongType: 0, outOfRange: 0, unsupported: 0 },
} as const satisfies ComponentSummary;
const emptyComponents = { input: noObservations, output: noObservations, cacheCreation: noObservations, cacheRead: noObservations };
async function renderResult(result: unknown): Promise<string> {
  const reader = createClaudeCopyDiagnostic(async () => JSON.stringify(result));
  await reader.run('copy.jsonl');
  assert.equal(reader.getSnapshot().status, 'copy');
  return renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
}
test('Rust golden renders separate replay subset and unchanged raw sums', async () => {
  const markup = await renderResult(replayFixture);
  assert.match(markup, /Eligible observations: 2/u);
  assert.match(markup, /Representatives: 1/u);
  assert.match(markup, /Exact replay copies: 1/u);
  assert.match(markup, /Unresolved observations: 0/u);
  assert.match(markup, /Differing token fields: 0/u);
  assert.match(markup, /10 observed claims/u);
  assert.match(markup, /20 observed claims/u);
  assert.match(markup, /NOT the complete or billed usage total/u);
  assert.match(markup, /Unresolved reasons overlap/u);
  assert.match(markup, /Rejected, oversized and pending lines remain coverage limitations/u);
  assert.doesNotMatch(markup, /PRIVATE|fictional-time|fictional-model/u);
});
for (const [status, text] of [
  ['identity-budget-exhausted', 'Replay evaluation exhausted: 2 observations exceed the identity limit of 1'],
  ['invalid-identity-limit', 'Replay evaluation invalid: identity limit is invalid'],
  ['wrong-harness', 'Replay evaluation unavailable: wrong harness'],
  ['allocation-failure', 'Replay evaluation unavailable: allocation failure'],
] as const) {
  test(`replay ${status} shows no subset values and keeps raw`, async () => {
    const replay = status === 'identity-budget-exhausted' ? { status, observations: 2, limit: 1 } : { status };
    const markup = await renderResult({ ...replayFixture, replay });
    assert.ok(markup.includes(text));
    assert.match(markup, /20 observed claims/u);
    assert.doesNotMatch(markup, /Eligible observations:|Eligible-subset token components/u);
  });
}
for (const [label, raw, unresolved, reasons] of [
  ['conflicting', replayFixture.raw, 2, { ...evaluated.unresolved, conflictingEvidence: 2 }],
  ['ineligible', replayFixture.raw, 2, { ...evaluated.unresolved, invalidIdentity: 2, invalidClaims: 2 }],
  ['empty', { ...replayFixture.raw, records: { ...replayFixture.raw.records, observations: 0, accepted: 0, withValidClaim: 0 },
    components: emptyComponents }, 0, evaluated.unresolved],
] as const) {
  test(`${label} evaluation has unavailable subset tokens rather than zero usage`, async () => {
    const replay = { ...evaluated, eligibleObservations: 0, representatives: 0, exactReplayCopies: 0,
      unresolvedObservations: unresolved, unresolved: reasons, components: emptyComponents,
      conflicts: { ineligiblePeer: 0, identityMetadata: 0, tokenFields: reasons.conflictingEvidence,
        parserEvidence: 0, uuidRelated: reasons.conflictingEvidence, sessionMessageRelated: reasons.conflictingEvidence } };
    const markup = await renderResult({ ...replayFixture, raw, replay });
    assert.match(markup, /Eligible observations: 0/u);
    assert.match(markup, /Input tokens: Unavailable - no observations/u);
    assert.match(markup, /Unresolved reasons overlap/u);
    assert.doesNotMatch(markup, /Input tokens: 0 observed claims/u);
  });
}
test('strict replay validation rejects private fields, invalid partitions and partial failure claims', () => {
  for (const replay of [
    { ...evaluated, uuid: 'PRIVATE' },
    { ...evaluated, eligibleObservations: 3 },
    { ...evaluated, representatives: 0, exactReplayCopies: 2 },
    { ...evaluated, unresolved: { ...evaluated.unresolved, conflictingEvidence: 1 } },
    { ...evaluated, components: { ...evaluated.components, input: { ...evaluated.components.input, path: 'PRIVATE' } } },
    { status: 'allocation-failure', components: evaluated.components },
    { status: 'identity-budget-exhausted', observations: 2, limit: 2 },
    { status: 'unknown' },
  ]) assert.equal(isClaudeCopyResult({ ...replayFixture, replay }), false);
  assert.equal(isClaudeCopyResult({ ...replayFixture, path: 'PRIVATE' }), false);
  assert.equal(isClaudeCopyResult(copy), false);
});
test('Clear discards evaluated replay and raw together and cannot reread', async () => {
  let calls = 0;
  const reader = createClaudeCopyDiagnostic(async () => { calls++; return JSON.stringify(replayFixture); });
  await reader.run('copy.jsonl');
  reader.clear();
  await reader.run('copy.jsonl');
  assert.deepEqual(reader.getSnapshot(), { status: 'cleared', attempted: true });
  assert.equal(calls, 1);
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  assert.doesNotMatch(markup, /20 observed claims|Eligible observations:|Exact replay copies:|Differing token fields:|Conflict attribution/u);
});

test('conflict attribution rejects missing, private, fractional and inconsistent counters', () => {
  for (const conflicts of [undefined, {}, { ...evaluated.conflicts, uuid: 'PRIVATE' },
    { ...evaluated.conflicts, tokenFields: -1 }, { ...evaluated.conflicts, tokenFields: 0.5 },
    { ...evaluated.conflicts, tokenFields: 1 }, { ...evaluated.conflicts, uuidRelated: 1025 }]) {
    assert.equal(isClaudeCopyResult({ ...replayFixture, replay: { ...evaluated, conflicts } }), false);
  }
});

test('overlapping conflict attribution renders counts and Clear removes them', async () => {
  const result = { ...replayFixture, replay: { ...evaluated, eligibleObservations: 0,
    representatives: 0, exactReplayCopies: 0, unresolvedObservations: 2,
    unresolved: { ...evaluated.unresolved, conflictingEvidence: 2 }, components: emptyComponents,
    conflicts: { ineligiblePeer: 1, identityMetadata: 2, tokenFields: 2, parserEvidence: 2,
      uuidRelated: 2, sessionMessageRelated: 2 } } };
  const markup = await renderResult(result);
  for (const label of ['Related ineligible peer: 1', 'Differing identity or metadata: 2',
    'Differing token fields: 2', 'Differing parser evidence: 2', 'UUID-related conflict: 2',
    'Session/message-related conflict: 2']) assert.ok(markup.includes(label));
  assert.match(markup, /Each counter counts an observation once/u);
  assert.match(markup, /20 observed claims/u);
  for (const conflicts of [
    { ...result.replay.conflicts, uuidRelated: 0, sessionMessageRelated: 0 },
    { ...result.replay.conflicts, ineligiblePeer: 0, identityMetadata: 0, tokenFields: 0, parserEvidence: 0 },
    { ...result.replay.conflicts, tokenFields: 3 },
  ]) assert.equal(isClaudeCopyResult({ ...result, replay: { ...result.replay, conflicts } }), false);
  let calls = 0;
  const reader = createClaudeCopyDiagnostic(async () => { calls++; return JSON.stringify(result); });
  await reader.run('copy.jsonl');
  reader.clear();
  await reader.run('copy.jsonl');
  assert.deepEqual(reader.getSnapshot(), { status: 'cleared', attempted: true });
  assert.equal(calls, 1);
});

test('Rust overlap golden crosses strict validation and renders attribution with raw totals', async () => {
  const result: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf142-conflicts-payload.json', import.meta.url), 'utf8'));
  assert.ok(isClaudeCopyResult(result));
  const markup = await renderResult(result);
  assert.match(markup, /Related ineligible peer: 2/u);
  assert.match(markup, /Differing identity or metadata: 3/u);
  assert.match(markup, /Differing token fields: 3/u);
  assert.match(markup, /Differing parser evidence: 3/u);
  assert.match(markup, /UUID-related conflict: 3/u);
  assert.match(markup, /Session\/message-related conflict: 3/u);
  assert.match(markup, /30 observed claims/u);
  assert.match(markup, /13 observed claims/u);
  assert.doesNotMatch(markup, /PRIVATE|fictional-time|fictional-model|unknown_tokens/u);
});

test('unconfirmed candidate fixture validates, renders only counts and clears', async () => {
  const payload = readFileSync(new URL('../fixtures/abf143-candidates-payload.json', import.meta.url), 'utf8');
  const result: unknown = JSON.parse(payload);
  assert.ok(isClaudeCopyResult(result));
  assert.equal(result.replay.status, 'evaluated');
  if (result.replay.status !== 'evaluated') assert.fail('expected evaluated candidates');
  const replay = result.replay;
  const reader = createClaudeCopyDiagnostic(async () => payload);
  await reader.run('PRIVATE-copy.jsonl');
  const render = () => renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  const markup = render();
  for (const label of ['Matching-request candidates (unconfirmed): 2', 'Request-ID mismatches: 0',
    'Missing or invalid identity evidence: 0', 'Other unresolved metadata: 0',
    'do not establish replays', 'never discard observations']) assert.ok(markup.includes(label));
  assert.doesNotMatch(markup, /PRIVATE|fictional-uuid|2026-01/u);
  for (const candidates of [undefined, {}, { ...replay.candidates, identifier: 'PRIVATE' },
    { ...replay.candidates, matchingRequest: -1 }, { ...replay.candidates, matchingRequest: 0.5 },
    { ...replay.candidates, matchingRequest: 3 }, { ...replay.candidates, matchingRequest: 1025 }]) {
    assert.equal(isClaudeCopyResult({ ...result, replay: { ...replay, candidates } }), false);
  }
  reader.clear();
  assert.doesNotMatch(render(), /Matching-request candidates/u);
});
