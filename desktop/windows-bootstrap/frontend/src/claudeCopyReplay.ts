import { isComponent } from './fictionalClaudeBridge.ts';
import type { ClaudeClaimSummary, ComponentId, ComponentSummary } from './fictionalClaudeBridge.ts';

export type ClaudeCopyReplay =
  | Readonly<{ status: 'evaluated'; basis: 'source-scoped-exact-observation-claims-v1';
    eligibleObservations: number; representatives: number; exactReplayCopies: number; unresolvedObservations: number;
    unresolved: Readonly<{ invalidIdentity: number; unsupportedRecordType: number; sidechain: number;
      invalidClaims: number; conflictingEvidence: number }>;
    conflicts: Readonly<{ ineligiblePeer: number; identityMetadata: number; tokenFields: number;
      parserEvidence: number; uuidRelated: number; sessionMessageRelated: number }>;
    candidates: Readonly<{ matchingRequest: number; requestMismatch: number;
      missingIdentity: number; otherMetadata: number }>;
    components: Readonly<Record<ComponentId, ComponentSummary>> }>
  | Readonly<{ status: 'identity-budget-exhausted'; observations: number; limit: number }>
  | Readonly<{ status: 'invalid-identity-limit' | 'wrong-harness' | 'allocation-failure' }>;

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function keys(value: Record<string, unknown>, names: readonly string[]): boolean {
  return Object.keys(value).length === names.length && names.every((name) => Object.hasOwn(value, name));
}
function count(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 1024;
}
export function isClaudeCopyReplay(value: unknown, raw: ClaudeClaimSummary): value is ClaudeCopyReplay {
  if (!record(value)) return false;
  switch (value.status) {
    case 'invalid-identity-limit':
    case 'wrong-harness':
    case 'allocation-failure': return keys(value, ['status']);
    case 'identity-budget-exhausted':
      return keys(value, ['status', 'observations', 'limit']) && count(value.observations) && count(value.limit)
        && value.observations === raw.records.observations && value.observations > value.limit;
    case 'evaluated': {
      if (!keys(value, ['status', 'basis', 'eligibleObservations', 'representatives', 'exactReplayCopies',
        'unresolvedObservations', 'unresolved', 'conflicts', 'candidates', 'components'])
        || value.basis !== 'source-scoped-exact-observation-claims-v1'
        || !count(value.eligibleObservations) || !count(value.representatives)
        || !count(value.exactReplayCopies) || !count(value.unresolvedObservations)
        || value.eligibleObservations + value.unresolvedObservations !== raw.records.observations
        || value.representatives + value.exactReplayCopies !== value.eligibleObservations
        || (value.eligibleObservations > 0 && value.representatives === 0)) return false;
      const unresolvedObservations = value.unresolvedObservations;
      const representatives = value.representatives;
      const reasons = value.unresolved;
      const names = ['invalidIdentity', 'unsupportedRecordType', 'sidechain', 'invalidClaims', 'conflictingEvidence'] as const;
      if (!record(reasons) || !keys(reasons, names)
        || !names.every((name) => count(reasons[name]) && reasons[name] <= unresolvedObservations)
        || names.reduce((sum, name) => sum + Number(reasons[name]), 0) < value.unresolvedObservations) return false;
      const conflicts = value.conflicts;
      const causes = ['ineligiblePeer', 'identityMetadata', 'tokenFields', 'parserEvidence'] as const;
      const relations = ['uuidRelated', 'sessionMessageRelated'] as const;
      const conflictCount = Number(reasons.conflictingEvidence);
      if (!record(conflicts) || !keys(conflicts, [...causes, ...relations])
        || ![...causes, ...relations].every((name) => count(conflicts[name]) && conflicts[name] <= conflictCount)
        || causes.reduce((sum, name) => sum + Number(conflicts[name]), 0) < conflictCount
        || relations.reduce((sum, name) => sum + Number(conflicts[name]), 0) < conflictCount) return false;
      const candidates = value.candidates;
      const categories = ['matchingRequest', 'requestMismatch', 'missingIdentity', 'otherMetadata'] as const;
      if (!record(candidates) || !keys(candidates, categories)
        || !categories.every((name) => count(candidates[name]) && candidates[name] <= Number(conflicts.identityMetadata))
        || Number(candidates.matchingRequest) > Number(conflicts.sessionMessageRelated)
        || Number(candidates.requestMismatch) > Number(conflicts.sessionMessageRelated)) return false;
      const components = value.components;
      const ids = ['input', 'output', 'cacheCreation', 'cacheRead'] as const;
      return record(components) && keys(components, ids) && ids.every((id) => {
        const component = components[id];
        return isComponent(component, representatives)
          && component.states.valid === representatives;
      });
    }
    default: return false;
  }
}
