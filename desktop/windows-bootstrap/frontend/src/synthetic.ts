export type EnvironmentId = 'windows' | 'selected-wsl' | 'computer';
export type HarnessId = 'general' | 'claude-code' | 'codex' | 'opencode';

export type EnvironmentDefinition = Readonly<{
  id: EnvironmentId;
  label: string;
  /** `collection` environments are sources; `derived` is a computed union view and never a source. */
  kind: 'collection' | 'derived';
  status: string;
  scope: string;
}>;

export type QuotaPanelDefinition = Readonly<{
  id: string;
  title: string;
  note: string;
}>;

export type HarnessDefinition = Readonly<{
  id: HarnessId;
  label: string;
  /** Heading segment shown after the environment label. */
  pageTitle: string;
  quotaPanels: readonly QuotaPanelDefinition[];
}>;

export const unavailable = 'Unavailable';

// Display fixtures only: these do not describe discovered environments or observations.
export const environments = Object.freeze([
  Object.freeze({
    id: 'windows',
    label: 'Windows',
    kind: 'collection',
    status: 'Not connected',
    scope: 'Collection source: Windows. Collection is disabled in this shell.',
  }),
  Object.freeze({
    id: 'selected-wsl',
    label: 'Selected WSL',
    kind: 'collection',
    status: 'Not connected; no distro discovered',
    scope: 'Collection source: one selected WSL distro. None has been discovered, and WSL is never started automatically.',
  }),
  Object.freeze({
    id: 'computer',
    label: 'Computer',
    kind: 'derived',
    status: 'Derived view only; no collection source',
    scope: 'Computed from Windows and Selected WSL after deduplication. It never collects data itself.',
  }),
] as const satisfies readonly EnvironmentDefinition[]);

export const harnesses = Object.freeze([
  Object.freeze({ id: 'general', label: 'General', pageTitle: 'Overview', quotaPanels: Object.freeze([]) }),
  Object.freeze({
    id: 'claude-code',
    label: 'Claude Code',
    pageTitle: 'Claude Code',
    quotaPanels: Object.freeze([
      Object.freeze({ id: 'five-hour', title: '5-hour window', note: 'Reset time unknown.' }),
      Object.freeze({ id: 'weekly', title: 'Weekly window', note: 'Reset time unknown.' }),
    ]),
  }),
  Object.freeze({
    id: 'codex',
    label: 'Codex',
    pageTitle: 'Codex',
    quotaPanels: Object.freeze([
      Object.freeze({ id: 'rate-limits', title: 'Rate-limit windows', note: 'Windows and reset times unknown.' }),
    ]),
  }),
  Object.freeze({
    id: 'opencode',
    label: 'OpenCode',
    pageTitle: 'OpenCode',
    quotaPanels: Object.freeze([
      Object.freeze({ id: 'provider-quota', title: 'Provider quota', note: 'Provider and account not identified.' }),
    ]),
  }),
] as const satisfies readonly HarnessDefinition[]);

export const shellText = Object.freeze({
  product: 'Agent Burn Fork',
  banner: 'Synthetic placeholder — collection disabled',
  explanation: 'No provider usage, quota or spending has been observed.',
  quotaScope: 'Quota is provider and account scoped, not environment scoped; it is never summed across environments.',
  statusBar: 'Synthetic shell · no data source connected · nothing has been observed',
});
