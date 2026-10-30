import { isClaudeCopyReplay } from './claudeCopyReplay.ts';
import type { ClaudeCopyReplay } from './claudeCopyReplay.ts';
import { invoke } from '@tauri-apps/api/core';
import { isClaudeClaimSummary } from './fictionalClaudeBridge.ts';
import type { ClaudeClaimSummary } from './fictionalClaudeBridge.ts';

export type ClaudeCopySummary = ClaudeClaimSummary & Readonly<{
  provenance: 'configured-windows-copy'; originalSource: 'not-established'; originalEnvironment: 'not-established';
}>;
export function isClaudeCopySummary(value: unknown): value is ClaudeCopySummary {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  const keys = ['schemaVersion', 'provenance', 'originalSource', 'originalEnvironment', 'basis', 'duplicates',
    'completeness', 'components', 'records', 'lines', 'files', 'skipped', 'caveats'];
  return Object.keys(record).length === keys.length && keys.every((key) => Object.hasOwn(record, key))
    && record.provenance === 'configured-windows-copy' && record.originalSource === 'not-established'
    && record.originalEnvironment === 'not-established' && isClaudeClaimSummary(value);
}

export type ClaudeCopyResult = Readonly<{ schemaVersion: 1; raw: ClaudeCopySummary; replay: ClaudeCopyReplay }>;
export function isClaudeCopyResult(value: unknown): value is ClaudeCopyResult {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const result = value as Record<string, unknown>;
  return Object.keys(result).length === 3 && result.schemaVersion === 1
    && isClaudeCopySummary(result.raw) && isClaudeCopyReplay(result.replay, result.raw);
}

const codes = ['COPY_ATTEMPT_CONSUMED', 'COPY_NOT_CONFIGURED', 'COPY_PATH_REFUSED', 'COPY_ROOT_REFUSED',
  'COPY_PLATFORM_UNSUPPORTED', 'COPY_LOCALITY_UNCOVERED', 'COPY_READ_FAILED', 'COPY_DIALOG_FAILED',
  'COPY_BUSY', 'COPY_RESPONSE_INVALID'] as const;
type FailureCode = (typeof codes)[number];
export type CopyDiagnosticState =
  | Readonly<{ status: 'idle' | 'cancelled'; attempted: false }>
  | Readonly<{ status: 'selecting' | 'running' | 'cleared'; attempted: true }>
  | Readonly<{ status: 'unavailable'; attempted: true; code: FailureCode }>
  | Readonly<{ status: 'copy'; attempted: true; summary: ClaudeCopySummary; replay: ClaudeCopyReplay }>;
export function createClaudeCopyDiagnostic(
  request: (relativePath: string) => Promise<unknown> = (relativePath) => invoke<unknown>('abf_read_claude_copy_once', { relativePath }),
  choose: () => Promise<unknown> = () => invoke<unknown>('abf_choose_claude_jsonl_once'),
) {
  let state: CopyDiagnosticState = { status: 'idle', attempted: false };
  let generation = 0;
  const listeners = new Set<() => void>();
  const publish = (next: CopyDiagnosticState) => { state = next; for (const listener of listeners) listener(); };
  async function execute(action: () => Promise<unknown>, picking: boolean) {
    if (state.attempted) return;
    const current = generation;
    // While the dialog is open this reserves the attempt; cancellation releases it.
    publish({ status: picking ? 'selecting' : 'running', attempted: true });
    let next: CopyDiagnosticState = { status: 'unavailable', attempted: true, code: 'COPY_RESPONSE_INVALID' };
    try {
      const raw = await action();
      if (picking && raw === null) {
        // Even Clear cannot consume an attempt when no file was selected or read.
        publish({ status: 'cancelled', attempted: false });
        return;
      }
      let value: unknown = null;
      try { value = typeof raw === 'string' && raw.length <= 16384 ? JSON.parse(raw) : null; } catch { /* Invalid transport remains unavailable. */ }
      if (isClaudeCopyResult(value)) next = { status: 'copy', attempted: true, summary: value.raw, replay: value.replay };
    } catch (error) {
      const code = codes.find((code) => error === code) ?? 'COPY_READ_FAILED';
      next = { status: 'unavailable', attempted: true, code };
    }
    if (current === generation) publish(next);
  }
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    run(relativePath: string) { return execute(() => request(relativePath), false); },
    choose() { return execute(choose, true); },
    clear() {
      if (!state.attempted) return;
      generation++;
      publish({ status: 'cleared', attempted: true });
    },
  };
}
export const claudeCopyDiagnostic = createClaudeCopyDiagnostic();
