import { isClaudeCopyReplay } from './claudeCopyReplay.ts';
import type { ClaudeCopyReplay } from './claudeCopyReplay.ts';
import { invoke } from '@tauri-apps/api/core';
import { isClaudeClaimSummary } from './fictionalClaudeBridge.ts';
import type { ClaudeClaimSummary } from './fictionalClaudeBridge.ts';
import { isClaudeCopyActivity } from './claudeCopyActivity.ts';
import type { ClaudeCopyActivity } from './claudeCopyActivity.ts';

export type ClaudeCopySummary = ClaudeClaimSummary & Readonly<{
  provenance: 'configured-windows-copy' | 'selected-windows-folder'; originalSource: 'not-established'; originalEnvironment: 'not-established';
}>;
export function isClaudeCopySummary(value: unknown, folder = false): value is ClaudeCopySummary {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  const keys = ['schemaVersion', 'provenance', 'originalSource', 'originalEnvironment', 'basis', 'duplicates',
    'completeness', 'components', 'records', 'lines', 'files', 'skipped', 'caveats'];
  return Object.keys(record).length === keys.length && keys.every((key) => Object.hasOwn(record, key))
    && record.provenance === (folder ? 'selected-windows-folder' : 'configured-windows-copy') && record.originalSource === 'not-established'
    && record.originalEnvironment === 'not-established' && isClaudeClaimSummary(value);
}

export type ClaudeCopyResult =
  | Readonly<{ schemaVersion: 1; raw: ClaudeCopySummary; replay: ClaudeCopyReplay }>
  | Readonly<{ schemaVersion: 2; raw: ClaudeCopySummary; replay: ClaudeCopyReplay; activity: ClaudeCopyActivity }>;
export function isClaudeCopyResult(value: unknown, folder = false): value is ClaudeCopyResult {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const result = value as Record<string, unknown>;
  const envelope = (Object.keys(result).length === 3 && result.schemaVersion === 1)
    || (folder && Object.keys(result).length === 4 && result.schemaVersion === 2 && Object.hasOwn(result, 'activity'));
  return envelope
    && isClaudeCopySummary(result.raw, folder) && isClaudeCopyReplay(result.replay, result.raw, folder ? 4096 : 1024)
    && (result.schemaVersion === 1 || isClaudeCopyActivity(result.activity, result.raw))
    && (!folder || (result.raw.files.read >= 1 && result.raw.files.read <= 16 && result.raw.files.rejected === 0 && result.raw.completeness === (result.raw.skipped.mayHideData > 0 ? 'incomplete' : 'complete')
      && result.raw.skipped.total === result.raw.skipped.mayHideData && result.raw.skipped.total <= 1024 - result.raw.files.read && result.raw.records.observations <= 4096));
}

const codes = ['COPY_ATTEMPT_CONSUMED', 'COPY_NOT_CONFIGURED', 'COPY_PATH_REFUSED', 'COPY_ROOT_REFUSED',
  'COPY_PLATFORM_UNSUPPORTED', 'COPY_LOCALITY_UNCOVERED', 'COPY_READ_FAILED', 'COPY_DIALOG_FAILED',
  'COPY_BUSY', 'COPY_RESPONSE_INVALID', 'COPY_FOLDER_REFUSED', 'COPY_FOLDER_LIMIT'] as const;
type FailureCode = (typeof codes)[number];
export type CopyDiagnosticState =
  | Readonly<{ status: 'idle' | 'cancelled'; attempted: false }>
  | Readonly<{ status: 'selecting' | 'running' | 'cleared'; attempted: true }>
  | Readonly<{ status: 'unavailable'; attempted: true; code: FailureCode }>
  | Readonly<{ status: 'pending'; attempted: true; count: number }>
  | Readonly<{ status: 'copy'; attempted: true; summary: ClaudeCopySummary; replay: ClaudeCopyReplay; activity?: ClaudeCopyActivity }>;
export function createClaudeCopyDiagnostic(
  request: (relativePath: string) => Promise<unknown> = (relativePath) => invoke<unknown>('abf_read_claude_copy_once', { relativePath }),
  choose: () => Promise<unknown> = () => invoke<unknown>('abf_choose_claude_jsonl_once'),
  chooseFolder: () => Promise<unknown> = () => invoke<unknown>('abf_choose_claude_folder_once'),
  analyzeFolder: () => Promise<unknown> = () => invoke<unknown>('abf_analyze_claude_folder_once'),
  clearFolder: () => Promise<unknown> = () => invoke<unknown>('abf_clear_claude_folder'),
) {
  let state: CopyDiagnosticState = { status: 'idle', attempted: false };
  let generation = 0;
  const listeners = new Set<() => void>();
  const publish = (next: CopyDiagnosticState) => { state = next; for (const listener of listeners) listener(); };
  async function execute(action: () => Promise<unknown>, picking: boolean, folder = false, analyzing = false) {
    if (analyzing ? state.status !== 'pending' : state.attempted) return;
    const pendingCount = state.status === 'pending' ? state.count : undefined;
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
      if (folder && picking && typeof raw === 'number' && Number.isInteger(raw) && raw >= 1 && raw <= 16) {
        if (current === generation) publish({ status: 'pending', attempted: true, count: raw });
        return;
      }
      let value: unknown = null;
      try { value = typeof raw === 'string' && raw.length <= 16384 ? JSON.parse(raw) : null; } catch { /* Invalid transport remains unavailable. */ }
      if ((!folder || analyzing) && isClaudeCopyResult(value, folder)
        && (!analyzing || value.raw.files.read === pendingCount)) next = { status: 'copy', attempted: true, summary: value.raw, replay: value.replay,
          ...(value.schemaVersion === 2 ? { activity: value.activity } : {}) };
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
    chooseFolder() { return execute(chooseFolder, true, true); },
    analyzeFolder() { return execute(analyzeFolder, false, true, true); },
    clear() {
      if (!state.attempted) return;
      generation++;
      void clearFolder().catch(() => {});
      publish({ status: 'cleared', attempted: true });
    },
  };
}
export const claudeCopyDiagnostic = createClaudeCopyDiagnostic();
