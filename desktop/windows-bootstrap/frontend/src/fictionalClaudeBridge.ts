/// <reference lib="esnext.disposable" />
// The pinned official core API declares Symbol.asyncDispose; this adds types only.
import { invoke } from '@tauri-apps/api/core';

// Gate ABF134F: explicit FICTIONAL demonstration payload (schema version 1), produced by the
// Rust parser, collector and summary over a fixed fixture. Not real consumption and not a
// production contract. Token quantities are decimal strings so no u64 loses precision.
export type ClaimStates = Readonly<{
  valid: number; validZero: number; absent: number; null: number;
  wrongType: number; outOfRange: number; unsupported: number;
}>;
export type ComponentValue =
  | Readonly<{ kind: 'sum'; amount: string }>
  | Readonly<{ kind: 'no-valid-claims' }>
  | Readonly<{ kind: 'indeterminate'; reason: 'no-observations' }>
  | Readonly<{ kind: 'overflow' }>;
export type ComponentSummary = Readonly<{ value: ComponentValue; states: ClaimStates }>;
export type ComponentId = 'input' | 'output' | 'cacheCreation' | 'cacheRead';
export type RejectionCounts = Readonly<{
  total: number; frameBudget: number; stringBudget: number; depthBudget: number; entryBudget: number;
  resultBudget: number; invalidLimits: number; invalidUtf8: number; incompleteJson: number;
  malformedJson: number; trailingData: number; rootShape: number; duplicateKey: number;
}>;
export type FictionalClaudeSummary = Readonly<{
  schemaVersion: 1;
  provenance: 'fictional-fixture';
  basis: 'observed-record-claims';
  duplicates: 'unresolved';
  environmentKind: 'windows' | 'wsl';
  hostMatch: 'simulated';
  completeness: 'complete' | 'incomplete';
  components: Readonly<Record<ComponentId, ComponentSummary>>;
  records: Readonly<{
    observations: number; accepted: number; qualified: number; incomplete: number;
    withValidClaim: number; withoutValidClaim: number; withInvalidClaim: number;
    withNullClaim: number; withMissingClaim: number; cacheCreationFlatIgnored: number;
  }>;
  lines: Readonly<{
    terminatedLines: number; rejected: RejectionCounts; oversized: number; blank: number;
    filesWithPendingTail: number; pendingTailBytes: number;
  }>;
  files: Readonly<{
    read: number; rejected: number; path: number; tooLarge: number; byteBudget: number;
    outputLimit: number; changedDuringRead: number; io: number;
  }>;
  skipped: Readonly<{ total: number; mayHideData: number; outOfScope: number }>;
  caveats: Readonly<{
    incompleteBatch: boolean; unparsedLines: boolean; pendingTails: boolean;
    rejectedFiles: boolean; skippedInScope: boolean;
  }>;
}>;

type SettledState =
  | Readonly<{ status: 'fictional'; summary: FictionalClaudeSummary }>
  | Readonly<{ status: 'unavailable' }>;
export type FictionalClaudeState = Readonly<{ status: 'loading' }> | SettledState;

const U64_MAX = 18_446_744_073_709_551_615n;
const COMPONENT_IDS = ['input', 'output', 'cacheCreation', 'cacheRead'] as const;
const STATE_KEYS = ['valid', 'validZero', 'absent', 'null', 'wrongType', 'outOfRange', 'unsupported'] as const;
const RECORD_KEYS = ['observations', 'accepted', 'qualified', 'incomplete', 'withValidClaim', 'withoutValidClaim',
  'withInvalidClaim', 'withNullClaim', 'withMissingClaim', 'cacheCreationFlatIgnored'] as const;
const LINE_COUNT_KEYS = ['terminatedLines', 'oversized', 'blank', 'filesWithPendingTail', 'pendingTailBytes'] as const;
const FILE_KEYS = ['read', 'rejected', 'path', 'tooLarge', 'byteBudget', 'outputLimit', 'changedDuringRead', 'io'] as const;
const SKIP_KEYS = ['total', 'mayHideData', 'outOfScope'] as const;
const FLAG_KEYS = ['incompleteBatch', 'unparsedLines', 'pendingTails', 'rejectedFiles', 'skippedInScope'] as const;
const REJECTION_KEYS = ['frameBudget', 'stringBudget', 'depthBudget', 'entryBudget', 'resultBudget',
  'invalidLimits', 'invalidUtf8', 'incompleteJson', 'malformedJson', 'trailingData', 'rootShape',
  'duplicateKey'] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function hasKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}
// Counters are small by construction (record, file and byte budgets); reject anything else.
function isCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 2 ** 40;
}
function counts<K extends string>(value: unknown, keys: readonly K[]): value is Record<K, number> {
  return isRecord(value) && hasKeys(value, keys) && keys.every((key) => isCount(value[key]));
}
function isAmount(value: unknown): value is string {
  return typeof value === 'string' && /^(?:0|[1-9][0-9]{0,19})$/u.test(value) && BigInt(value) <= U64_MAX;
}
function sum(values: readonly number[]): number {
  return values.reduce((total, value) => total + value, 0);
}

function isComponent(value: unknown, observations: number): value is ComponentSummary {
  if (!isRecord(value) || !hasKeys(value, ['value', 'states']) || !counts(value.states, STATE_KEYS)) return false;
  const states = value.states as unknown as ClaimStates;
  // The states partition the observations; an explicit zero is a valid claim.
  if (sum(STATE_KEYS.map((key) => states[key]).filter((_, index) => index !== 1)) !== observations
    || states.validZero > states.valid) return false;
  const outcome = value.value;
  if (!isRecord(outcome)) return false;
  switch (outcome.kind) {
    case 'sum': return hasKeys(outcome, ['kind', 'amount']) && isAmount(outcome.amount) && states.valid > 0;
    case 'no-valid-claims': return hasKeys(outcome, ['kind']) && states.valid === 0 && observations > 0;
    case 'indeterminate':
      return hasKeys(outcome, ['kind', 'reason']) && outcome.reason === 'no-observations' && observations === 0;
    case 'overflow': return hasKeys(outcome, ['kind']) && states.valid >= 2;
    default: return false;
  }
}

/** Strict validation of the whole payload, including the arithmetic the Rust summary guarantees. */
export function isFictionalClaudeSummary(value: unknown): value is FictionalClaudeSummary {
  if (!isRecord(value) || !hasKeys(value, ['schemaVersion', 'provenance', 'basis', 'duplicates',
    'environmentKind', 'hostMatch', 'completeness', 'components', 'records', 'lines', 'files', 'skipped', 'caveats'])
    || value.schemaVersion !== 1 || value.provenance !== 'fictional-fixture'
    || value.basis !== 'observed-record-claims' || value.duplicates !== 'unresolved'
    || (value.environmentKind !== 'windows' && value.environmentKind !== 'wsl')
    // A fictional payload is always simulated: a native host match would be real-mode data.
    || value.hostMatch !== 'simulated'
    || (value.completeness !== 'complete' && value.completeness !== 'incomplete')) return false;
  const { components, records, lines, files, skipped, caveats } = value;
  if (!counts(records, RECORD_KEYS)) return false;
  if (records.accepted + records.qualified + records.incomplete !== records.observations
    || records.withValidClaim + records.withoutValidClaim !== records.observations) return false;
  if (!isRecord(components) || !hasKeys(components, COMPONENT_IDS)
    || !COMPONENT_IDS.every((id) => isComponent(components[id], records.observations))) return false;
  if (!isRecord(lines) || !hasKeys(lines, ['rejected', ...LINE_COUNT_KEYS])
    || !LINE_COUNT_KEYS.every((key) => isCount(lines[key]))) return false;
  // Every plain counter was just validated; `rejected` is checked separately below.
  const lineCounts = lines as unknown as Record<(typeof LINE_COUNT_KEYS)[number], number>;
  const rejected = lines.rejected;
  if (!counts(rejected, ['total', ...REJECTION_KEYS])
    || sum(REJECTION_KEYS.map((key) => rejected[key])) !== rejected.total) return false;
  if (!counts(files, FILE_KEYS) || files.path + files.tooLarge + files.byteBudget + files.outputLimit
    + files.changedDuringRead + files.io !== files.rejected) return false;
  if (!counts(skipped, SKIP_KEYS) || skipped.mayHideData + skipped.outOfScope !== skipped.total) return false;
  if (!isRecord(caveats) || !hasKeys(caveats, FLAG_KEYS) || !FLAG_KEYS.every((key) => typeof caveats[key] === 'boolean')) return false;
  // Caveats are derived from the counts above and cannot disagree with them.
  return caveats.incompleteBatch === (value.completeness === 'incomplete')
    && caveats.unparsedLines === (rejected.total > 0 || lineCounts.oversized > 0)
    && caveats.pendingTails === (lineCounts.filesWithPendingTail > 0)
    && caveats.rejectedFiles === (files.rejected > 0)
    && caveats.skippedInScope === (skipped.mayHideData > 0);
}

function requestNativeSummary(): Promise<unknown> {
  // The official API rejects when the Tauri runtime is absent; load() contains that failure.
  return invoke<unknown>('abf_get_fictional_claude_summary');
}

function decode(raw: unknown): SettledState {
  try {
    const value: unknown = typeof raw === 'string' ? JSON.parse(raw) : raw;
    return isFictionalClaudeSummary(value) ? { status: 'fictional', summary: value } : { status: 'unavailable' };
  } catch {
    return { status: 'unavailable' };
  }
}

export function createFictionalClaudeBridge(request: () => Promise<unknown> = requestNativeSummary) {
  let result: Promise<SettledState> | undefined;
  return {
    load(): Promise<SettledState> {
      // Share in-flight and settled results, including failures. No polling or retries.
      result ??= Promise.resolve().then(request).then(decode, (): SettledState => ({ status: 'unavailable' }));
      return result;
    },
  };
}

// One request per frontend module lifetime, including React StrictMode effect replay.
export const fictionalClaudeBridge = createFictionalClaudeBridge();
