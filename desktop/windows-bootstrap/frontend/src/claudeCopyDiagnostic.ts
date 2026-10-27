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
const codes = ['COPY_ATTEMPT_CONSUMED', 'COPY_NOT_CONFIGURED', 'COPY_PATH_REFUSED', 'COPY_ROOT_REFUSED',
  'COPY_PLATFORM_UNSUPPORTED', 'COPY_LOCALITY_UNCOVERED', 'COPY_READ_FAILED'] as const;
type FailureCode = (typeof codes)[number] | 'COPY_RESPONSE_INVALID';
export type CopyDiagnosticState =
  | Readonly<{ status: 'idle'; attempted: false }>
  | Readonly<{ status: 'running' | 'cleared'; attempted: true }>
  | Readonly<{ status: 'unavailable'; attempted: true; code: FailureCode }>
  | Readonly<{ status: 'copy'; attempted: true; summary: ClaudeCopySummary }>;
export function createClaudeCopyDiagnostic(
  request: (relativePath: string) => Promise<unknown> = (relativePath) => invoke<unknown>('abf_read_claude_copy_once', { relativePath }),
) {
  let state: CopyDiagnosticState = { status: 'idle', attempted: false };
  let generation = 0;
  const listeners = new Set<() => void>();
  const publish = (next: CopyDiagnosticState) => { state = next; for (const listener of listeners) listener(); };
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    async run(relativePath: string) {
      if (state.attempted) return;
      const current = generation;
      publish({ status: 'running', attempted: true });
      let next: CopyDiagnosticState = { status: 'unavailable', attempted: true, code: 'COPY_RESPONSE_INVALID' };
      try {
        const raw = await request(relativePath);
        let value: unknown = null;
        try { value = typeof raw === 'string' && raw.length <= 16384 ? JSON.parse(raw) : null; } catch { /* Invalid transport remains unavailable. */ }
        if (isClaudeCopySummary(value)) next = { status: 'copy', attempted: true, summary: value };
      } catch (error) {
        const code = codes.find((code) => error === code) ?? 'COPY_READ_FAILED';
        next = { status: 'unavailable', attempted: true, code };
      }
      if (current === generation) publish(next);
    },
    clear() {
      if (!state.attempted) return;
      generation++;
      publish({ status: 'cleared', attempted: true });
    },
  };
}
export const claudeCopyDiagnostic = createClaudeCopyDiagnostic();
