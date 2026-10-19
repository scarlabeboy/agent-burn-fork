import type { EnvironmentId, HarnessId } from './synthetic.ts';
import type { SourceId, SyntheticRecord, SyntheticSnapshot } from './syntheticBridge.ts';

type TotalRow = Readonly<{ id: string; tokens: number }>;
type Occurrence = Readonly<{ record: SyntheticRecord; sources: readonly SourceId[]; sourceRecords: readonly SyntheticRecord[] }>;
type Aggregate = Readonly<{
  tokens: number;
  sessions: number;
  activity: readonly TotalRow[];
  models: readonly TotalRow[];
  harnesses: readonly TotalRow[];
  occurrences: readonly Occurrence[];
  sources: readonly SourceId[];
}>;

const compare = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
const sessionKey = (record: SyntheticRecord) => `${record.source}/${record.harness}/${record.sessionId}`;

// Accept validated transport only. Null means absent/unavailable, never a measured zero.
// DEMO ONLY: source/harness/sessionId is a local session identity. An explicit
// equivalent occurrence connects the two containing sessions; labels never do.
export function aggregateSynthetic(
  snapshot: SyntheticSnapshot | null, environment: EnvironmentId, harness: HarnessId,
): Aggregate | null {
  if (!snapshot) return null;
  const records = snapshot.records.filter((record) =>
    (environment === 'computer' || record.source === environment)
    && (harness === 'general' || record.harness === harness),
  ).sort((a, b) => compare(a.id, b.id));
  if (records.length === 0) return null;

  const parents = new Map<string, string>();
  function root(key: string): string {
    const parent = parents.get(key);
    return parent === undefined || parent === key ? key : root(parent);
  }
  const groups = new Map<string, SyntheticRecord[]>();
  for (const record of records) {
    const key = record.equivalenceId === null ? `record/${record.id}` : `equivalent/${record.equivalenceId}`;
    const group = groups.get(key) ?? [];
    group.push(record);
    groups.set(key, group);
    parents.set(sessionKey(record), sessionKey(record));
  }
  const occurrences: Occurrence[] = [];
  for (const group of groups.values()) {
    const first = group[0];
    if (!first) continue;
    for (const record of group) parents.set(root(sessionKey(record)), root(sessionKey(first)));
    occurrences.push({ record: first, sourceRecords: group, sources: [...new Set(group.map((record) => record.source))].sort(compare) });
  }
  function totals(key: (record: SyntheticRecord) => string): TotalRow[] {
    const sums = new Map<string, number>();
    for (const { record } of occurrences) {
      const id = key(record);
      sums.set(id, (sums.get(id) ?? 0) + record.inputTokens + record.outputTokens);
    }
    return [...sums].sort(([a], [b]) => compare(a, b)).map(([id, tokens]) => ({ id, tokens }));
  }
  return {
    tokens: occurrences.reduce((sum, { record }) => sum + record.inputTokens + record.outputTokens, 0),
    sessions: new Set(records.map((record) => root(sessionKey(record)))).size,
    activity: totals((record) => record.date),
    models: totals((record) => record.model),
    harnesses: totals((record) => record.harness),
    occurrences,
    sources: [...new Set(records.map((record) => record.source))].sort(compare),
  };
}
