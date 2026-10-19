/// <reference lib="esnext.disposable" />
// The pinned official core API declares Symbol.asyncDispose; this adds types only.
import { invoke } from '@tauri-apps/api/core';

// Gate M6D transport fixture only; not a production observation schema.
type UnavailableObservations = Readonly<{
  usage: null;
  sessions: null;
  models: null;
  cost: null;
  quota: null;
}>;

type SyntheticSource<Id extends 'windows' | 'selected-wsl'> = Readonly<{
  id: Id;
  observations: UnavailableObservations;
}>;

export type SyntheticSnapshot = Readonly<{
  schemaVersion: 1;
  provenance: 'fixed-synthetic';
  sources: readonly [SyntheticSource<'windows'>, SyntheticSource<'selected-wsl'>];
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

function isSource(value: unknown, id: 'windows' | 'selected-wsl'): boolean {
  if (!isRecord(value) || !hasKeys(value, ['id', 'observations']) || value.id !== id) return false;
  const observations = value.observations;
  const fields = ['usage', 'sessions', 'models', 'cost', 'quota'];
  return isRecord(observations) && hasKeys(observations, fields)
    && fields.every((field) => observations[field] === null);
}

export function isSyntheticSnapshot(value: unknown): value is SyntheticSnapshot {
  return isRecord(value) && hasKeys(value, ['schemaVersion', 'provenance', 'sources'])
    && value.schemaVersion === 1 && value.provenance === 'fixed-synthetic'
    && Array.isArray(value.sources) && value.sources.length === 2
    && isSource(value.sources[0], 'windows') && isSource(value.sources[1], 'selected-wsl');
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
