/// <reference lib="esnext.disposable" />
// The pinned official core API declares Symbol.asyncDispose; this adds types only.
import { invoke } from '@tauri-apps/api/core';

// Gate M6E DEMO TRANSPORT FIXTURE ONLY. No production event/database contract.
export type SourceId = 'windows' | 'selected-wsl';
export type RecordHarnessId = 'claude-code' | 'codex' | 'opencode';
export type SyntheticRecord = Readonly<{
  id: string;
  source: SourceId;
  harness: RecordHarnessId;
  sessionId: string;
  // Only an explicit pair in distinct sources is equivalent; matching values prove nothing.
  equivalenceId: string | null;
  model: string;
  // Fixed UTC calendar date bucket, YYYY-MM-DD (not a local date or capture timestamp).
  date: string;
  inputTokens: number;
  outputTokens: number;
}>;
export type SyntheticSnapshot = Readonly<{
  schemaVersion: 2;
  provenance: 'fixed-synthetic';
  sources: readonly [Readonly<{ id: 'windows' }>, Readonly<{ id: 'selected-wsl' }>];
  records: readonly SyntheticRecord[];
}>;

type SettledBridgeState =
  | Readonly<{ status: 'synthetic'; snapshot: SyntheticSnapshot }>
  | Readonly<{ status: 'unavailable' }>;

export type BridgeState = Readonly<{ status: 'loading' }> | SettledBridgeState;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function isSource(value: unknown, id: SourceId): boolean {
  return isRecord(value) && hasKeys(value, ['id']) && value.id === id;
}

function isDemoId(value: unknown): value is string {
  return typeof value === 'string' && value.length <= 64 && /^demo-[a-z0-9-]+$/u.test(value);
}

function isUtcDay(value: unknown): value is string {
  if (typeof value !== 'string' || value.length !== 10 || !/^\d{4}-\d{2}-\d{2}$/u.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return year >= 1 && month >= 1 && month <= 12 && day >= 1 && day <= (days[month - 1] ?? 0);
}

function isTokenCount(value: unknown): value is number {
  // At most 100 records, each with two counts <= 1,000,000: every sum is safe.
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 1_000_000;
}

function isDemoRecord(value: unknown): value is SyntheticRecord {
  return isRecord(value) && hasKeys(value, [
    'id', 'source', 'harness', 'sessionId', 'equivalenceId', 'model', 'date', 'inputTokens', 'outputTokens',
  ]) && isDemoId(value.id) && (value.source === 'windows' || value.source === 'selected-wsl')
    && (value.harness === 'claude-code' || value.harness === 'codex' || value.harness === 'opencode')
    && isDemoId(value.sessionId) && (value.equivalenceId === null || isDemoId(value.equivalenceId))
    && isDemoId(value.model) && isUtcDay(value.date)
    && isTokenCount(value.inputTokens) && isTokenCount(value.outputTokens);
}

export function isSyntheticSnapshot(value: unknown): value is SyntheticSnapshot {
  if (!isRecord(value) || !hasKeys(value, ['schemaVersion', 'provenance', 'sources', 'records'])
    || value.schemaVersion !== 2 || value.provenance !== 'fixed-synthetic'
    || !Array.isArray(value.sources) || value.sources.length !== 2
    || !isSource(value.sources[0], 'windows') || !isSource(value.sources[1], 'selected-wsl')
    || !Array.isArray(value.records) || value.records.length > 100) return false;
  const ids = new Set<string>();
  const equivalents = new Map<string, SyntheticRecord[]>();
  for (const record of value.records) {
    if (!isDemoRecord(record) || ids.has(record.id)) return false;
    ids.add(record.id);
    if (record.equivalenceId !== null) {
      const group = equivalents.get(record.equivalenceId) ?? [];
      group.push(record);
      equivalents.set(record.equivalenceId, group);
    }
  }
  for (const group of equivalents.values()) {
    const [a, b] = group;
    // Fixture-only, exactly one occurrence per source, with identical logical content.
    if (group.length !== 2 || !a || !b || a.source === b.source
      || a.harness !== b.harness || a.model !== b.model || a.date !== b.date
      || a.inputTokens !== b.inputTokens || a.outputTokens !== b.outputTokens) return false;
  }
  return true;
}

function requestNativeSnapshot(): Promise<unknown> {
  // The official API rejects when the Tauri runtime is absent; load() contains that failure.
  return invoke<unknown>('abf_get_synthetic_snapshot');
}

export function createSyntheticBridge(request: () => Promise<unknown> = requestNativeSnapshot) {
  let result: Promise<SettledBridgeState> | undefined;
  return {
    load(): Promise<SettledBridgeState> {
      // Share both in-flight and settled results, including failures. No polling or retries.
      result ??= Promise.resolve().then(request).then(
        (snapshot): SettledBridgeState => isSyntheticSnapshot(snapshot)
          ? { status: 'synthetic', snapshot }
          : { status: 'unavailable' },
        (): SettledBridgeState => ({ status: 'unavailable' }),
      );
      return result;
    },
  };
}

// One request per frontend module lifetime, including React StrictMode effect replay.
export const syntheticBridge = createSyntheticBridge();
