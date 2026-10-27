import { invoke } from '@tauri-apps/api/core';
import { isFictionalClaudeSummary } from './fictionalClaudeBridge.ts';
import type { FictionalClaudeSummary } from './fictionalClaudeBridge.ts';

export type FileDiagnosticState =
  | Readonly<{ status: 'idle'; attempted: false }>
  | Readonly<{ status: 'running' | 'unavailable' | 'cleared'; attempted: true }>
  | Readonly<{ status: 'fictional'; attempted: true; summary: FictionalClaudeSummary }>;

export function createFictionalFileDiagnostic(
  request: () => Promise<unknown> = () => invoke<unknown>('abf_read_fictional_claude_file_once'),
) {
  let state: FileDiagnosticState = { status: 'idle', attempted: false };
  let generation = 0;
  const listeners = new Set<() => void>();
  const publish = (next: FileDiagnosticState) => {
    state = next;
    for (const listener of listeners) listener();
  };
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    async run() {
      if (state.attempted) return;
      const current = generation;
      publish({ status: 'running', attempted: true });
      let next: FileDiagnosticState = { status: 'unavailable', attempted: true };
      try {
        const raw = await request();
        // Bound decoding and require the native JSON-string transport.
        const value: unknown = typeof raw === 'string' && raw.length <= 16384 ? JSON.parse(raw) : null;
        if (isFictionalClaudeSummary(value)) next = { status: 'fictional', attempted: true, summary: value };
      } catch {
        // Native errors can contain private details; never forward them to React.
      }
      if (generation === current) publish(next);
    },
    clear() {
      if (!state.attempted) return;
      generation++;
      publish({ status: 'cleared', attempted: true });
    },
  };
}

// No request at module initialization, mount, selection changes or clear.
export const fictionalFileDiagnostic = createFictionalFileDiagnostic();
