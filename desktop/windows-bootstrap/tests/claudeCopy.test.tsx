import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createClaudeCopyDiagnostic, isClaudeCopySummary } from '../frontend/src/claudeCopyDiagnostic.ts';
import { ShellView } from '../frontend/src/App.tsx';
const fixture: unknown = JSON.parse(readFileSync(new URL('../fixtures/abf136-expected-payload.json', import.meta.url), 'utf8'));
const golden = readFileSync(new URL('../fixtures/abf137-expected-payload.json', import.meta.url), 'utf8').trim();
const copy: unknown = JSON.parse(golden);
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
