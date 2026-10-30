import { isComponent } from './fictionalClaudeBridge.ts';
import type { ClaudeClaimSummary, ComponentId, ComponentSummary } from './fictionalClaudeBridge.ts';

type ActivityRow = Readonly<{
  day: string | null; observations: number; components: Readonly<Record<ComponentId, ComponentSummary>>;
}>;
export type ClaudeCopyActivity =
  | Readonly<{ status: 'available'; rows: readonly ActivityRow[]; }>
  | Readonly<{ status: 'budget-exhausted'; }>;

const ids = ['input', 'output', 'cacheCreation', 'cacheRead'] as const;
const states = ['valid', 'validZero', 'absent', 'null', 'wrongType', 'outOfRange', 'unsupported'] as const;
const u64Max = 18_446_744_073_709_551_615n;
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function keys(value: Record<string, unknown>, names: readonly string[]): boolean {
  return Object.keys(value).length === names.length && names.every((name) => Object.hasOwn(value, name));
}
function day(value: unknown): value is string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/u.test(value) || value.startsWith('0000')) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

/** Exact allowlist plus reconciliation with the unchanged raw diagnostic. */
export function isClaudeCopyActivity(value: unknown, raw: ClaudeClaimSummary): value is ClaudeCopyActivity {
  if (!record(value)) return false;
  if (value.status === 'budget-exhausted') return keys(value, ['status']);
  if (value.status !== 'available' || !keys(value, ['status', 'rows']) || !Array.isArray(value.rows) || value.rows.length > 32) return false;
  const rows: ActivityRow[] = [];
  let previous = '';
  for (const [index, row] of value.rows.entries()) {
    if (!record(row) || !keys(row, ['day', 'observations', 'components'])
      || typeof row.observations !== 'number' || !Number.isSafeInteger(row.observations) || row.observations < 1 || row.observations > 4096
      || !record(row.components) || !keys(row.components, ids)) return false;
    if (row.day === null) {
      if (index !== value.rows.length - 1) return false;
    } else {
      if (!day(row.day) || row.day <= previous) return false;
      previous = row.day;
    }
    const c = row.components;
    if (!isComponent(c.input, row.observations) || !isComponent(c.output, row.observations)
      || !isComponent(c.cacheCreation, row.observations) || !isComponent(c.cacheRead, row.observations)) return false;
    rows.push({
      day: row.day, observations: row.observations,
      components: { input: c.input, output: c.output, cacheCreation: c.cacheCreation, cacheRead: c.cacheRead }
    });
  }
  if (rows.reduce((n, row) => n + row.observations, 0) !== raw.records.observations) return false;
  return ids.every((id) => {
    const component = raw.components[id];
    if (!states.every((state) => rows.reduce((n, row) => n + row.components[id].states[state], 0) === component.states[state])) return false;
    let amount = 0n;
    let overflow = false;
    for (const row of rows) {
      const value = row.components[id].value;
      if (value.kind === 'overflow') overflow = true;
      if (value.kind === 'sum') amount += BigInt(value.amount);
    }
    if (overflow || amount > u64Max) return component.value.kind === 'overflow';
    if (component.states.valid > 0) return component.value.kind === 'sum' && BigInt(component.value.amount) === amount;
    return component.value.kind === (raw.records.observations > 0 ? 'no-valid-claims' : 'indeterminate');
  });
}
