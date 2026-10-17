import { environments, harnesses } from './synthetic.ts';
import type { EnvironmentDefinition, EnvironmentId, HarnessDefinition, HarnessId } from './synthetic.ts';

export type Selection = Readonly<{ environment: EnvironmentId; harness: HarnessId }>;

export const initialSelection: Selection = Object.freeze({ environment: 'windows', harness: 'general' });

/** Changing one axis keeps the other; selecting the current value returns the same object. */
export function selectEnvironment(current: Selection, environment: EnvironmentId): Selection {
  return current.environment === environment ? current : Object.freeze({ ...current, environment });
}

export function selectHarness(current: Selection, harness: HarnessId): Selection {
  return current.harness === harness ? current : Object.freeze({ ...current, harness });
}

export function environmentOf(selection: Selection): EnvironmentDefinition {
  const found = environments.find((entry) => entry.id === selection.environment);
  if (found === undefined) {
    throw new Error(`Unknown environment: ${selection.environment}`);
  }
  return found;
}

export function harnessOf(selection: Selection): HarnessDefinition {
  const found = harnesses.find((entry) => entry.id === selection.harness);
  if (found === undefined) {
    throw new Error(`Unknown harness: ${selection.harness}`);
  }
  return found;
}

export function pageHeading(selection: Selection): string {
  return `${environmentOf(selection).label} — ${harnessOf(selection).pageTitle}`;
}
