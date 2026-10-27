import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import { createFictionalFileDiagnostic } from '../frontend/src/fictionalFileDiagnostic.ts';
import { ShellView } from '../frontend/src/App.tsx';

const golden = readFileSync(new URL('../fixtures/abf136-expected-payload.json', import.meta.url), 'utf8').trim();

test('diagnostic is manual, one-shot and concurrent calls do not repeat IPC', async () => {
  let calls = 0;
  const diagnostic = createFictionalFileDiagnostic(async () => { calls++; return golden; });
  assert.equal(calls, 0);
  assert.equal(diagnostic.getSnapshot().status, 'idle');
  await Promise.all([diagnostic.run(), diagnostic.run()]);
  assert.equal(calls, 1);
  assert.equal(diagnostic.getSnapshot().status, 'fictional');
  diagnostic.clear();
  assert.equal(diagnostic.getSnapshot().status, 'cleared');
  await diagnostic.run();
  assert.equal(calls, 1);
});

test('clearing a pending request prevents late results and never rearms', async () => {
  let complete: (value: unknown) => void = () => assert.fail('request did not start');
  const diagnostic = createFictionalFileDiagnostic(() => new Promise((resolve) => { complete = resolve; }));
  const pending = diagnostic.run();
  assert.equal(diagnostic.getSnapshot().status, 'running');
  diagnostic.clear();
  complete(golden);
  await pending;
  assert.equal(diagnostic.getSnapshot().status, 'cleared');
  assert.equal(diagnostic.getSnapshot().attempted, true);
});

for (const raw of ['bad json', '{}', golden.replace('simulated', 'native'), golden.replace('fictional-fixture', 'real-history'), ' '.repeat(16385), 42]) {
  test('invalid file diagnostic response is unavailable', async () => {
    const diagnostic = createFictionalFileDiagnostic(async () => raw);
    await diagnostic.run();
    assert.deepEqual(diagnostic.getSnapshot(), { status: 'unavailable', attempted: true });
  });
}

test('read failure remains unavailable, is sanitized and cannot retry', async () => {
  let calls = 0;
  const diagnostic = createFictionalFileDiagnostic(async () => { calls++; throw new Error('PRIVATE-PATH'); });
  await diagnostic.run();
  await diagnostic.run();
  assert.equal(calls, 1);
  assert.deepEqual(diagnostic.getSnapshot(), { status: 'unavailable', attempted: true });
});

test('control appears only under Selected WSL / Claude Code with simulated provenance', async () => {
  const diagnostic = createFictionalFileDiagnostic(async () => golden);
  const props = { onSelectEnvironment: () => {}, onSelectHarness: () => {} };
  const render = (environment: 'windows' | 'selected-wsl' | 'computer', harness: 'claude-code' | 'general') =>
    renderToStaticMarkup(createElement(ShellView, { ...props, selection: { environment, harness },
      fileDiagnostic: diagnostic.getSnapshot(), onRunFileDiagnostic: () => {}, onClearFileDiagnostic: () => {} }));
  assert.match(render('selected-wsl', 'claude-code'), /Run fictional file-read once/u);
  assert.doesNotMatch(render('windows', 'claude-code'), /Run fictional file-read/u);
  assert.doesNotMatch(render('computer', 'claude-code'), /Run fictional file-read/u);
  assert.doesNotMatch(render('selected-wsl', 'general'), /Run fictional file-read/u);
  await diagnostic.run();
  assert.match(render('selected-wsl', 'claude-code'), /Simulated provenance; not native WSL collection/u);
  assert.match(render('selected-wsl', 'claude-code'), /data-component="input"/u);
  diagnostic.clear();
  const cleared = render('selected-wsl', 'claude-code');
  assert.doesNotMatch(cleared, /data-component=/u);
  assert.match(cleared, /Result cleared from memory/u);
  assert.match(cleared, /disabled=""[^>]*>Run fictional file-read once/u);
});

test('official IPC invocation has a fixed command and no caller arguments', async () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const calls: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => { calls.push(args); return Promise.resolve(golden); } },
  } });
  try {
    const diagnostic = createFictionalFileDiagnostic();
    await diagnostic.run();
    assert.deepEqual(calls, [['abf_read_fictional_claude_file_once', {}, undefined]]);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});
