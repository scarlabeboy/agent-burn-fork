import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createClaudeCopyDiagnostic, isClaudeCopySummary, isClaudeCopyResult } from '../frontend/src/claudeCopyDiagnostic.ts';
import type { ClaudeCopySummary, CopyDiagnosticState } from '../frontend/src/claudeCopyDiagnostic.ts';
import type { ComponentSummary } from '../frontend/src/fictionalClaudeBridge.ts';
import { ShellView } from '../frontend/src/App.tsx';
import { snapshot } from './syntheticFixture.ts';
const fixture: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf136-expected-payload.json', import.meta.url), 'utf8'));
const rawGolden = readFileSync(new URL('../fixtures/abf137-expected-payload.json', import.meta.url), 'utf8').trim();
const golden = JSON.stringify({ schemaVersion: 1, raw: JSON.parse(rawGolden), replay: { status: 'allocation-failure' } });
const copy: unknown = JSON.parse(rawGolden);
assert.ok(isClaudeCopySummary(copy));

const failureGuidance = [
  ['COPY_PATH_REFUSED', /The selected path or file is unsupported/u],
  ['COPY_ROOT_REFUSED', /The selected location could not be accepted/u],
  ['COPY_LOCALITY_UNCOVERED', /A supported local fixed drive could not be confirmed/u],
  ['COPY_PLATFORM_UNSUPPORTED', /This reader requires a supported Windows environment/u],
  ['COPY_DIALOG_FAILED', /The Windows file dialog could not complete/u],
  ['COPY_NOT_CONFIGURED', /The optional legacy copy root is not configured/u],
  ['COPY_ATTEMPT_CONSUMED', /The one-read attempt has already been used/u],
  ['COPY_BUSY', /Another selection or read is already in progress/u],
  ['COPY_READ_FAILED', /The read could not return a usable result/u],
  ['COPY_RESPONSE_INVALID', /The result could not be validated/u],
] as const;

function renderCopyState(state: CopyDiagnosticState): string {
  return renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: state,
  }));
}

for (const [code, explanation] of failureGuidance) {
  test(`${code} explains the failure with a compact code and restart guidance`, async () => {
    const reader = createClaudeCopyDiagnostic(async () => golden, async () => { throw code; });
    await reader.choose();
    const markup = renderCopyState(reader.getSnapshot());
    const status = markup.match(/<p[^>]*role="status"[^>]*>(.*?)<\/p>/u)?.[1] ?? '';
    assert.match(status, explanation);
    assert.match(status, /Restart the app before another attempt/u);
    assert.doesNotMatch(status, /COPY_/u);
    assert.ok(markup.includes(`<details><summary>Technical detail</summary><p class="page-note"><code>${code}</code></p></details>`));
    assert.match(markup, /A refused selection consumes the one-read attempt/u);
    assert.match(markup, /Windows may retain dialog history independently of ABF/u);
    assert.match(markup, /disabled=""[^>]*>Choose JSONL file/u);
    reader.clear();
    const cleared = renderCopyState(reader.getSnapshot());
    assert.doesNotMatch(cleared, /Technical detail|<code>COPY_/u);
    assert.match(cleared, /Restart the app before another attempt/u);
  });
}

for (const error of [new Error('PRIVATE C:\\secret.jsonl OS error 123'),
  'COPY_PATH_REFUSED PRIVATE source content', { message: 'PRIVATE credential' }]) {
  test('unknown failures render safe generic guidance without rejection details', async () => {
    const reader = createClaudeCopyDiagnostic(async () => golden, async () => { throw error; });
    await reader.choose();
    const markup = renderCopyState(reader.getSnapshot());
    assert.match(markup, /The read could not return a usable result/u);
    assert.match(markup, /<code>COPY_READ_FAILED<\/code>/u);
    assert.doesNotMatch(markup, /PRIVATE|secret|OS error 123|credential|source content/u);
  });
}

test('invalid picker result gives validation guidance without exposing payload content', async () => {
  const reader = createClaudeCopyDiagnostic(async () => golden, async () => '{"secret":"PRIVATE"}');
  await reader.choose();
  const markup = renderCopyState(reader.getSnapshot());
  assert.match(markup, /The result could not be validated/u);
  assert.doesNotMatch(markup, /PRIVATE|secret/u);
});

test('picker cancellation reads nothing and allows either entry path afterwards', async () => {
  let legacyCalls = 0;
  const reader = createClaudeCopyDiagnostic(async () => { legacyCalls++; return golden; }, async () => null);
  await reader.choose();
  assert.deepEqual(reader.getSnapshot(), { status: 'cancelled', attempted: false });
  assert.equal(legacyCalls, 0);
  await reader.run('chosen.jsonl');
  assert.equal(reader.getSnapshot().status, 'copy');
  assert.equal(legacyCalls, 1);
});

test('picker shares pending and consumed guards with legacy and Clear suppresses late results', async () => {
  let finish: (value: unknown) => void = () => assert.fail('not requested');
  let selections = 0;
  const reader = createClaudeCopyDiagnostic(async () => assert.fail('legacy must stay blocked'),
    () => { selections++; return new Promise((resolve) => { finish = resolve; }); });
  const pending = reader.choose();
  assert.equal(reader.getSnapshot().status, 'selecting');
  await reader.choose();
  await reader.run('unselected.jsonl');
  reader.clear();
  finish(golden);
  await pending;
  assert.equal(reader.getSnapshot().status, 'cleared');
  await reader.choose();
  assert.equal(selections, 1);
});

test('Clear during a cancelled dialog does not consume a nonexistent read attempt', async () => {
  let finish: (value: unknown) => void = () => assert.fail('not requested');
  const reader = createClaudeCopyDiagnostic(async () => golden, () => new Promise((resolve) => { finish = resolve; }));
  const pending = reader.choose();
  reader.clear();
  const cleared = renderCopyState(reader.getSnapshot());
  assert.match(cleared, /unless the open dialog is cancelled without a selection/u);
  assert.match(cleared, /Clear does not close the dialog or cancel a read/u);
  finish(null);
  await pending;
  assert.deepEqual(reader.getSnapshot(), { status: 'cancelled', attempted: false });
  await reader.run('copy.jsonl');
  assert.equal(reader.getSnapshot().status, 'copy');
});

test('picker errors consume the attempt and disclose only a fixed code', async () => {
  const reader = createClaudeCopyDiagnostic(async () => assert.fail('legacy blocked'), async () => { throw new Error('PRIVATE C:\\secret.jsonl'); });
  await reader.choose();
  assert.deepEqual(reader.getSnapshot(), { status: 'unavailable', attempted: true, code: 'COPY_READ_FAILED' });
  await reader.run('copy.jsonl');
});

for (const code of ['COPY_DIALOG_FAILED', 'COPY_PATH_REFUSED', 'COPY_ROOT_REFUSED',
  'COPY_LOCALITY_UNCOVERED', 'COPY_ATTEMPT_CONSUMED', 'COPY_BUSY', 'COPY_RESPONSE_INVALID']) {
  test(`picker preserves fixed native failure ${code}`, async () => {
    const reader = createClaudeCopyDiagnostic(async () => assert.fail('legacy blocked'), async () => { throw code; });
    await reader.choose();
    assert.deepEqual(reader.getSnapshot(), { status: 'unavailable', attempted: true, code });
    await reader.run('copy.jsonl');
  });
}

test('picker action is available only in Windows / Claude Code', () => {
  for (const environment of ['windows', 'selected-wsl', 'computer'] as const) {
    for (const harness of ['general', 'claude-code', 'codex', 'opencode'] as const) {
      const markup = renderToStaticMarkup(createElement(ShellView, {
        selection: { environment, harness }, onSelectEnvironment: () => {}, onSelectHarness: () => {},
        copyDiagnostic: { status: 'idle', attempted: false },
      }));
      assert.equal(markup.includes('Choose JSONL file...'), environment === 'windows' && harness === 'claude-code');
    }
  }
});

test('native picker IPC accepts no frontend path and successful selection renders the snapshot', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(golden); } },
  } });
  try {
    const reader = createClaudeCopyDiagnostic();
    assert.equal(calls.length, 0);
    await reader.choose();
    assert.deepEqual(calls, [['abf_choose_claude_jsonl_once', {}, undefined]]);
    const markup = renderToStaticMarkup(createElement(ShellView, {
      selection: { environment: 'windows', harness: 'claude-code' },
      onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
    }));
    assert.match(markup, /Manual copy session snapshot/u);
    assert.match(markup, /disabled=""[^>]*>Choose JSONL file\.\.\./u);
    assert.doesNotMatch(markup, /C:\\|PRIVATE|\.jsonl<\/dd>/u);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});

test('cancelled picker renders ready controls and a no-read cancellation status', async () => {
  const reader = createClaudeCopyDiagnostic(async () => golden, async () => null);
  await reader.choose();
  const markup = renderToStaticMarkup(createElement(ShellView, {
    selection: { environment: 'windows', harness: 'claude-code' },
    onSelectEnvironment: () => {}, onSelectHarness: () => {}, copyDiagnostic: reader.getSnapshot(),
  }));
  assert.match(markup, /Cancelled\. No source read; the attempt is still available/u);
  assert.match(markup, /<button type="button">Choose JSONL file\.\.\.<\/button>/u);
  assert.doesNotMatch(markup, /Technical detail|<code>COPY_/u);
  assert.doesNotMatch(markup.match(/<p[^>]*role="status"[^>]*>(.*?)<\/p>/u)?.[1] ?? '', /restart|consumed/ui);
});
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
  assert.doesNotMatch(renderCopyState({ status: 'copy', attempted: true, summary: failed,
    replay: { status: 'allocation-failure' } }), /synthetic-demonstration/u);
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
function visibleCopy(summary: ClaudeCopySummary): string {
  return copyPanel(summary).split('<details class="scenario copy-details"')[0] ?? '';
}
test('successful copy keeps snapshot and unavailable quota visible before collapsed synthetic demonstration', async () => {
  const html = await renderResult(replayFixture);
  const disclosure = html.indexOf('<details class="scenario synthetic-demonstration">');
  assert.ok(disclosure > html.indexOf('Manual copy session snapshot'));
  assert.ok(disclosure > html.indexOf('id="quota-section-title"'));
  assert.match(html.slice(disclosure), /^<details class="scenario synthetic-demonstration"><summary>Synthetic demonstration - expand to view fictional dashboards and examples<\/summary>/u);
  assert.ok(disclosure < html.indexOf('aria-label="Overview metrics"'));
  assert.ok(disclosure < html.indexOf('id="fictional-results-title"'));
  assert.ok(disclosure < html.indexOf('id="activity-title"'));
  assert.ok(disclosure < html.indexOf('id="models-title"'));
  assert.match(html.slice(disclosure), /<\/div><\/details><\/div><\/main>/u);
  assert.doesNotMatch(html.slice(disclosure), /Manual copy session snapshot|quota-section-title/u);
});
for (const state of [
  { status: 'idle', attempted: false }, { status: 'cancelled', attempted: false },
  { status: 'selecting', attempted: true }, { status: 'running', attempted: true },
  { status: 'unavailable', attempted: true, code: 'COPY_READ_FAILED' },
  { status: 'cleared', attempted: true },
] as const satisfies readonly CopyDiagnosticState[]) {
  test(`${state.status} retains the no-result dashboard presentation`, () => {
    const html = renderCopyState(state);
    assert.doesNotMatch(html, /synthetic-demonstration/u);
    assert.match(html, /aria-label="Overview metrics"/u);
    assert.match(html, /id="fictional-results-title"/u);
    assert.match(html, /id="quota-section-title"/u);
  });
}
test('successful copy does not collapse demos in other environments or harnesses', () => {
  for (const environment of ['windows', 'selected-wsl', 'computer'] as const) {
    for (const harness of ['general', 'claude-code', 'codex', 'opencode'] as const) {
      const props = { selection: { environment, harness }, onSelectEnvironment: () => {}, onSelectHarness: () => {} };
      const baseline = renderToStaticMarkup(createElement(ShellView, {
        ...props, copyDiagnostic: { status: 'idle', attempted: false },
      }));
      const html = renderToStaticMarkup(createElement(ShellView, {
        ...props, copyDiagnostic: { status: 'copy', attempted: true, summary: copy, replay: evaluated },
      }));
      assert.equal(html.includes('synthetic-demonstration'), environment === 'windows' && harness === 'claude-code');
      // Other views retain their entire no-result presentation.
      assert.equal(environment === 'windows' && harness === 'claude-code' || html === baseline, true);
    }
  }
});
test('copy claims leave populated synthetic metrics and breakdowns unchanged', () => {
  for (const environment of ['windows', 'selected-wsl', 'computer'] as const) {
    const render = (copyDiagnostic: CopyDiagnosticState) => renderToStaticMarkup(createElement(ShellView, {
      selection: { environment, harness: 'claude-code' }, onSelectEnvironment: () => {}, onSelectHarness: () => {},
      bridgeState: { status: 'synthetic', snapshot }, copyDiagnostic,
    }));
    const baseline = render({ status: 'idle', attempted: false });
    const html = render({ status: 'copy', attempted: true, summary: copy, replay: evaluated });
    const metrics = (value: string) => value.match(/<section class="metrics".*?<\/section>/u)?.[0];
    const breakdowns = (value: string) => value.match(/<div class="grid">.*?<\/table><\/section><\/div>/u)?.[0];
    assert.ok(metrics(baseline));
    assert.ok(breakdowns(baseline));
    assert.equal(metrics(html), metrics(baseline));
    assert.equal(breakdowns(html), breakdowns(baseline));
  }
});
test('manual reader precedes synthetic metrics and fictional scenarios', async () => {
  const html = await renderResult(replayFixture);
  assert.ok(html.indexOf('id="claude-copy-title"') < html.indexOf('aria-label="Overview metrics"'));
  assert.ok(html.indexOf('id="claude-copy-title"') < html.indexOf('id="fictional-results-title"'));
  assert.match(html, /Synthetic dashboard - demonstration only/u);
});
test('snapshot shows raw components with diagnostics collapsed by default', () => {
  const visible = visibleCopy(copy);
  for (const [label, value] of [['Input tokens', '17'], ['Output tokens', '3'], ['Cache creation tokens', '2'], ['Cache read tokens', '4']]) {
    assert.ok(visible.includes(`<dt>${label}</dt><dd>${value}</dd>`));
  }
  assert.match(copyPanel(copy), /<details class="scenario copy-details"><summary>Token, parser and replay diagnostics<\/summary>/u);
  assert.doesNotMatch(copyPanel(copy), /<details[^>]*\bopen/u);
});
for (const [name, value, expected] of [
  ['no valid claims', { kind: 'no-valid-claims' }, 'No valid claims'],
  ['explicit zero', { kind: 'sum', amount: '0' }, '0'],
  ['large exact sum', { kind: 'sum', amount: '18446744073709551615' }, '18446744073709551615'],
  ['overflow', { kind: 'overflow' }, 'Overflow - sum exceeds 64 bits'],
  ['unavailable', { kind: 'indeterminate', reason: 'no-observations' }, 'Unavailable - no observations'],
] as const satisfies readonly (readonly [string, ComponentSummary['value'], string])[]) {
  test(`snapshot preserves ${name}`, () => {
    const summary = { ...copy, components: { ...copy.components, input: { ...copy.components.input, value } } };
    assert.ok(visibleCopy(summary).includes(`<dt>Input tokens</dt><dd>${expected}</dd>`));
  });
}
test('partial coverage is visible without opening diagnostics', () => {
  const visible = visibleCopy(copy);
  assert.match(visible, /Records with valid claims: 2; without valid claims: 0/u);
  assert.match(visible, /Files read: 1; rejected: 0/u);
  assert.match(visible, /Excluded lines: 1 rejected, 0 oversized, 0 blank/u);
  assert.match(visible, /Pending: 1 file\(s\), 10 bytes/u);
  assert.match(visible, /Coverage limited/u);
  assert.match(visibleCopy(noUsage), /Coverage limited/u);
  assert.match(visibleCopy(replayFixture.raw), /No known coverage gaps; complete usage is not established/u);
});
test('compact replay distinguishes candidate observations from exact copies', async () => {
  const result: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf143-candidates-payload.json', import.meta.url), 'utf8'));
  const visible = (await renderResult(result)).split('<details class="scenario copy-details"')[0] ?? '';
  assert.match(visible, /Confirmed exact copies: 0/u);
  assert.match(visible, /Unconfirmed matching-request candidate observations: 2/u);
  assert.match(visible, /Unresolved observations: 2/u);
  assert.match(visible, /Candidate observations are not duplicate copies or groups/u);
});
test('snapshot reports confirmed replay copies without replacing the raw token claims', async () => {
  const visible = (await renderResult(replayFixture)).split('<details class="scenario copy-details"')[0] ?? '';
  assert.match(visible, /Confirmed exact copies: 1/u);
  assert.match(visible, /Unconfirmed matching-request candidate observations: 0/u);
  assert.match(visible, /Unresolved observations: 0/u);
  assert.match(visible, /<dt>Input tokens<\/dt><dd>20<\/dd>/u);
});
test('copy results do not change synthetic environment or Computer metrics', () => {
  for (const environment of ['windows', 'selected-wsl', 'computer'] as const) {
    const render = (loaded: boolean): string => renderToStaticMarkup(createElement(ShellView, {
      selection: { environment, harness: 'claude-code' }, onSelectEnvironment: () => {}, onSelectHarness: () => {},
      copyDiagnostic: (loaded ? { status: 'copy', attempted: true, summary: copy, replay: evaluated }
        : { status: 'idle', attempted: false }) satisfies CopyDiagnosticState,
    }));
    const metrics = (html: string) => [...html.matchAll(/<p class="metric__value">(.*?)<\/p>/gu)].map((match) => match[1]);
    assert.deepEqual(metrics(render(true)), metrics(render(false)));
    assert.deepEqual(metrics(render(true)), ['Unavailable', 'Unavailable', 'Unavailable', 'Unavailable']);
  }
});
test('selection changes hide and restore the snapshot; Clear removes snapshot and diagnostics', async () => {
  let calls = 0;
  const reader = createClaudeCopyDiagnostic(async () => { calls++; return golden; });
  await reader.run('copy.jsonl');
  const render = (environment: 'windows' | 'computer' | 'selected-wsl', harness: 'claude-code' | 'codex') =>
    renderToStaticMarkup(createElement(ShellView, {
      selection: { environment, harness }, onSelectEnvironment: () => {}, onSelectHarness: () => {},
      copyDiagnostic: reader.getSnapshot(),
    }));
  const before = render('windows', 'claude-code');
  assert.match(before, /Manual copy session snapshot/u);
  assert.doesNotMatch(render('computer', 'claude-code'), /Manual copy session snapshot/u);
  assert.doesNotMatch(render('selected-wsl', 'claude-code'), /Manual copy session snapshot/u);
  assert.doesNotMatch(render('windows', 'codex'), /Manual copy session snapshot/u);
  assert.equal(render('windows', 'claude-code'), before);
  assert.equal(calls, 1);
  reader.clear();
  const cleared = render('windows', 'claude-code');
  assert.doesNotMatch(cleared, /Manual copy session snapshot|Token, parser and replay diagnostics|data-component=|synthetic-demonstration/u);
  assert.match(cleared, /aria-label="Overview metrics"/u);
  assert.deepEqual(reader.getSnapshot(), { status: 'cleared', attempted: true });
});
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

test('folder selection exposes only a pending count until separate analysis', async () => {
  let reads = 0;
  const folderPayload = readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8');
  const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2, async () => { reads++; return folderPayload; }, async () => {});
  await reader.chooseFolder();
  assert.deepEqual(reader.getSnapshot(), { status: 'pending', attempted: true, count: 2 });
  assert.equal(reads, 0);
  assert.match(renderCopyState(reader.getSnapshot()), /Analyze selected folder once/u);
  await reader.analyzeFolder();
  assert.equal(reads, 1);
  assert.equal(reader.getSnapshot().status, 'copy');
  reader.clear();
  assert.equal(reader.getSnapshot().status, 'cleared');
  await reader.analyzeFolder();
  assert.equal(reads, 1);
});

for (const response of [0, 17, 1.5, 'PRIVATE/path.jsonl', { count: 2, path: 'PRIVATE' }]) {
  test('folder selection rejects malformed counts without exposing payload', async () => {
    const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => response, undefined, async () => {});
    await reader.chooseFolder();
    assert.equal(reader.getSnapshot().status, 'unavailable');
    assert.doesNotMatch(renderCopyState(reader.getSnapshot()), /PRIVATE/u);
  });
}

test('folder cancel preserves attempt; clear suppresses late pending and analyze results', async () => {
  const cancelled = createClaudeCopyDiagnostic(undefined, undefined, async () => null, undefined, async () => {});
  await cancelled.chooseFolder();
  assert.deepEqual(cancelled.getSnapshot(), { status: 'cancelled', attempted: false });
  let resolve: (value: unknown) => void = () => {};
  const reader = createClaudeCopyDiagnostic(undefined, undefined, () => new Promise((done) => { resolve = done; }), undefined, async () => {});
  const selection = reader.chooseFolder();
  await reader.choose();
  reader.clear();
  resolve(2);
  await selection;
  assert.equal(reader.getSnapshot().status, 'cleared');
  const folderPayload = readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8');
  const analyzing = createClaudeCopyDiagnostic(undefined, undefined, async () => 2, () => new Promise((done) => { resolve = done; }), async () => {});
  await analyzing.chooseFolder();
  const analysis = analyzing.analyzeFolder();
  analyzing.clear();
  resolve(folderPayload);
  await analysis;
  assert.equal(analyzing.getSnapshot().status, 'cleared');
});

for (const code of ['COPY_FOLDER_REFUSED', 'COPY_FOLDER_LIMIT', 'COPY_READ_FAILED', 'PRIVATE OS detail']) {
  test(`folder refusal uses sanitized fixed guidance: ${code === 'PRIVATE OS detail' ? 'unknown' : code}`, async () => {
    const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => { throw code; }, undefined, async () => {});
    await reader.chooseFolder();
    assert.equal(reader.getSnapshot().status, 'unavailable');
    assert.doesNotMatch(renderCopyState(reader.getSnapshot()), /PRIVATE/u);
    await reader.choose();
    assert.equal(reader.getSnapshot().status, 'unavailable');
  });
}

test('folder aggregate results remain separate from synthetic environment and Computer totals', async () => {
  const payload = readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8');
  const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2, async () => payload, async () => {});
  await reader.chooseFolder();
  await reader.analyzeFolder();
  const markup = renderCopyState(reader.getSnapshot());
  assert.match(markup, /Selected folder batch: 2 files/u);
  assert.match(markup, /Synthetic dashboard - demonstration only/u);
  assert.match(markup, /Replay checks span the whole batch/u);
  assert.doesNotMatch(markup, /PRIVATE|\.jsonl<|selected-windows-folder/u);
});

test('folder result schema admits 4096 aggregate observations while legacy stays at 1024', () => {
  const result: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8'));
  assert.ok(isClaudeCopyResult(result, true));
  assert.equal(isClaudeCopyResult(result), false);
  if (result.replay.status !== 'evaluated') assert.fail('expected replay evaluation');
  const raw = result.raw;
  const components = Object.fromEntries(Object.entries(raw.components).map(([key, value]) => [key, {
    ...value, states: { ...value.states, valid: 4096 },
    value: value.value.kind === 'sum' ? { kind: 'sum', amount: (BigInt(value.value.amount) * 2048n).toString() } : value.value,
  }]));
  const large = { ...result, raw: { ...raw, components,
    records: { ...raw.records, observations: 4096, accepted: 4096, withValidClaim: 4096 },
    lines: { ...raw.lines, terminatedLines: 4096 }, files: { ...raw.files, read: 16 },
  }, replay: { ...result.replay, eligibleObservations: 4096, exactReplayCopies: 4095 } };
  assert.equal(isClaudeCopyResult(large, true), true);
  assert.equal(isClaudeCopyResult({ ...large, raw: { ...large.raw, provenance: 'configured-windows-copy' } }), false);
  assert.equal(isClaudeCopyResult({ ...large, secret: 'PRIVATE' }, true), false);
});

for (const transform of [
  (payload: string) => payload.replace('"read":2', '"read":1'),
  (payload: string) => payload.replace('selected-windows-folder', 'configured-windows-copy'),
  (_payload: string) => '{"path":"PRIVATE"}',
  (_payload: string) => 'x'.repeat(16385),
]) {
  test('folder analysis refuses mismatched counts, provenance and private or oversized payloads', async () => {
    const payload = readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8');
    const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2, async () => transform(payload), async () => {});
    await reader.chooseFolder();
    await reader.analyzeFolder();
    assert.equal(reader.getSnapshot().status, 'unavailable');
    assert.doesNotMatch(renderCopyState(reader.getSnapshot()), /PRIVATE/u);
  });
}

test('folder ignored entries preserve selected-file success and disclose incomplete coverage', async () => {
  const value: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf148b-folder-payload.json', import.meta.url), 'utf8'));
  assert.ok(isClaudeCopyResult(value, true));
  const incomplete = { ...value, raw: { ...value.raw, completeness: 'incomplete',
    skipped: { total: 2, mayHideData: 2, outOfScope: 0 },
    caveats: { ...value.raw.caveats, incompleteBatch: true, skippedInScope: true },
  } };
  assert.ok(isClaudeCopyResult(incomplete, true));
  const reader = createClaudeCopyDiagnostic(undefined, undefined, async () => 2, async () => JSON.stringify(incomplete), async () => {});
  await reader.chooseFolder();
  await reader.analyzeFolder();
  assert.equal(reader.getSnapshot().status, 'copy');
  const markup = renderCopyState(reader.getSnapshot());
  assert.match(markup, /Ignored entries: 2/u);
  assert.match(markup, /folder coverage is incomplete/u);
  assert.doesNotMatch(markup, /PRIVATE/u);
});

test('native folder dialog uses the tested creation restriction flags', () => {
  const source = readFileSync(new URL('../src-tauri/src/claude_folder_dialog_windows.rs', import.meta.url), 'utf8');
  assert.match(source, /flags: super::FOLDER_DIALOG_FLAGS,/u);
});

test('folder guidance shows revised initial safeguards and unchanged single-file limits', () => {
  const markup = renderCopyState({ status: 'idle', attempted: false });
  assert.match(markup, /Initial folder safeguards: 16 files, 3 MiB\/file, 24 MiB aggregate/u);
  assert.match(markup, /Copy limits: one file, 1 MiB per file/u);
  assert.match(markup, /Locality checks enforce admission/u);
});
