import type { SyntheticSnapshot } from '../frontend/src/syntheticBridge.ts';

// Independently enumerated demo fixture; Rust parity is checked in the aggregation suite without executing native code.
export const snapshot = {
  schemaVersion: 2, provenance: 'fixed-synthetic',
  sources: [{ id: 'windows' }, { id: 'selected-wsl' }],
  records: [
    { id: 'demo-w1', source: 'windows', harness: 'claude-code', sessionId: 'demo-ws1', equivalenceId: 'demo-copy', model: 'demo-amber', date: '2026-01-01', inputTokens: 10, outputTokens: 5 },
    { id: 'demo-w2', source: 'windows', harness: 'claude-code', sessionId: 'demo-ws2', equivalenceId: null, model: 'demo-amber', date: '2026-01-01', inputTokens: 20, outputTokens: 10 },
    { id: 'demo-w3', source: 'windows', harness: 'codex', sessionId: 'demo-ws3', equivalenceId: null, model: 'demo-birch', date: '2026-01-02', inputTokens: 30, outputTokens: 10 },
    { id: 'demo-w4', source: 'windows', harness: 'codex', sessionId: 'demo-ws4', equivalenceId: null, model: 'demo-birch', date: '2026-01-02', inputTokens: 30, outputTokens: 10 },
    { id: 'demo-w5', source: 'windows', harness: 'opencode', sessionId: 'demo-ws5', equivalenceId: null, model: 'demo-coral', date: '2026-01-03', inputTokens: 40, outputTokens: 10 },
    { id: 'demo-w6', source: 'windows', harness: 'opencode', sessionId: 'demo-ws5', equivalenceId: null, model: 'demo-coral', date: '2026-01-03', inputTokens: 50, outputTokens: 10 },
    { id: 'demo-l1', source: 'selected-wsl', harness: 'claude-code', sessionId: 'demo-ls1', equivalenceId: 'demo-copy', model: 'demo-amber', date: '2026-01-01', inputTokens: 10, outputTokens: 5 },
    { id: 'demo-l2', source: 'selected-wsl', harness: 'claude-code', sessionId: 'demo-ls2', equivalenceId: null, model: 'demo-amber', date: '2026-01-01', inputTokens: 25, outputTokens: 10 },
    { id: 'demo-l3', source: 'selected-wsl', harness: 'codex', sessionId: 'demo-ls3', equivalenceId: null, model: 'demo-birch', date: '2026-01-02', inputTokens: 45, outputTokens: 10 },
    { id: 'demo-l4', source: 'selected-wsl', harness: 'codex', sessionId: 'demo-ls3', equivalenceId: null, model: 'demo-birch', date: '2026-01-02', inputTokens: 55, outputTokens: 10 },
    { id: 'demo-l5', source: 'selected-wsl', harness: 'opencode', sessionId: 'demo-ls4', equivalenceId: null, model: 'demo-coral', date: '2026-01-03', inputTokens: 65, outputTokens: 10 },
    { id: 'demo-l6', source: 'selected-wsl', harness: 'opencode', sessionId: 'demo-ls4', equivalenceId: null, model: 'demo-coral', date: '2026-01-03', inputTokens: 75, outputTokens: 10 },
  ],
} as const satisfies SyntheticSnapshot;

// [tokens, sessions], calculated by hand, not by the implementation.
export const expectedViews = {
  windows: { general: [235, 5], 'claude-code': [45, 2], codex: [80, 2], opencode: [110, 1] },
  'selected-wsl': { general: [330, 4], 'claude-code': [50, 2], codex: [120, 1], opencode: [160, 1] },
  computer: { general: [550, 8], 'claude-code': [80, 3], codex: [200, 3], opencode: [270, 2] },
} as const;
