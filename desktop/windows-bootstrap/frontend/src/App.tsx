import * as React from 'react';
import { aggregateSynthetic } from './syntheticAggregation.ts';
import { syntheticBridge } from './syntheticBridge.ts';
import type { BridgeState } from './syntheticBridge.ts';
import { environmentOf, harnessOf, initialSelection, pageHeading, selectEnvironment, selectHarness } from './selection.ts';
import type { Selection } from './selection.ts';
import { environments, harnesses, shellText, unavailable } from './synthetic.ts';
import type { EnvironmentId, HarnessId, QuotaPanelDefinition } from './synthetic.ts';
import { fictionalResultsSnapshot } from './fictionalResultsSnapshot.ts';
import { splitFictionalResults } from './fictionalResultsView.ts';
import { fictionalClaudeBridge } from './fictionalClaudeBridge.ts';
import type { ComponentId, ComponentSummary, FictionalClaudeState, FictionalClaudeSummary } from './fictionalClaudeBridge.ts';
import { fictionalFileDiagnostic } from './fictionalFileDiagnostic.ts';
import type { FileDiagnosticState } from './fictionalFileDiagnostic.ts';

const fictionalResults = splitFictionalResults(fictionalResultsSnapshot);

const metricLabels = Object.freeze(['Tokens', 'Estimated API cost', 'Sessions', 'Models'] as const);

type ShellViewProps = Readonly<{
  selection: Selection;
  onSelectEnvironment: (id: EnvironmentId) => void;
  onSelectHarness: (id: HarnessId) => void;
  bridgeState?: BridgeState;
  /** Explicit fictional Claude summary; when omitted nothing extra is rendered. */
  fictionalClaude?: FictionalClaudeState;
  fileDiagnostic?: FileDiagnosticState;
  onRunFileDiagnostic?: () => void;
  onClearFileDiagnostic?: () => void;
}>;

function Status({ tone, children }: Readonly<{ tone: 'unavailable' | 'derived' | 'synthetic'; children: React.ReactNode }>): React.JSX.Element {
  return <span className={`status status--${tone}`}>{children}</span>;
}

function Region({ id, title, children }: Readonly<{ id: string; title: string; children: React.ReactNode }>): React.JSX.Element {
  return (
    <section className="panel" aria-labelledby={`${id}-title`}>
      <h3 className="panel__title" id={`${id}-title`}>{title}</h3>
      {children}
    </section>
  );
}

function EmptyState({ title, detail }: Readonly<{ title: string; detail: string }>): React.JSX.Element {
  return (
    <div className="empty">
      <p className="empty__title">{title}</p>
      <p className="empty__detail">{detail}</p>
    </div>
  );
}

const claudeComponents: readonly (readonly [ComponentId, string])[] = Object.freeze([
  ['input', 'Input tokens'], ['output', 'Output tokens'],
  ['cacheCreation', 'Cache creation tokens'], ['cacheRead', 'Cache read tokens'],
] as const);

/** Distinguishes a sum (an explicit zero stays "0"), no valid claims, no observations and overflow. */
function claudeValueText(component: ComponentSummary): string {
  switch (component.value.kind) {
    case 'sum': return `${component.value.amount} observed claims (fictional)`;
    case 'no-valid-claims': return 'No valid claims';
    case 'indeterminate': return 'Unavailable - no observations';
    case 'overflow': return 'Overflow - sum exceeds 64 bits';
  }
}

function claudeGaps(summary: FictionalClaudeSummary): string[] {
  const { caveats, lines, files, skipped } = summary;
  return [
    caveats.incompleteBatch ? 'Incomplete coverage: traversal or reading did not finish for every candidate.' : '',
    caveats.unparsedLines ? `Parser gaps: ${lines.rejected.total} rejected and ${lines.oversized} oversized lines were not counted.` : '',
    caveats.pendingTails ? `Pending tails: ${lines.pendingTailBytes} bytes in ${lines.filesWithPendingTail} file(s) were not parsed.` : '',
    caveats.rejectedFiles ? `Rejected files: ${files.rejected} file(s) were not read.` : '',
    caveats.skippedInScope ? `Skipped entries: ${skipped.mayHideData} may hide data.` : '',
  ].filter((entry) => entry !== '');
}

function FictionalClaudePanel({ state, id = 'fictional-claude' }: Readonly<{ state: FictionalClaudeState; id?: string }>): React.JSX.Element {
  const title = 'Fictional Claude usage summary - demonstration only';
  if (state.status !== 'fictional') {
    return (
      <Region id={id} title={title}>
        <p className="page-note" role="status">{state.status === 'loading'
          ? 'Fictional Claude summary: loading'
          : 'Fictional Claude summary unavailable. Nothing is shown in its place.'}</p>
      </Region>
    );
  }
  const { summary } = state;
  const gaps = claudeGaps(summary);
  return (
    <Region id={id} title={title}>
      <p><Status tone="synthetic">FICTIONAL FIXTURE · not your data</Status></p>
      <p className="page-note">Produced by the Rust parser, collector and summary from a fixed fictional fixture. No
        real Claude history, account, quota or spending is read. This is not a Selected WSL, Windows or Computer total.</p>
      <p className="page-note">Counting basis: observed record claims, every record counted. Duplicate-record identity
        is unresolved, so repeated records count each time; these are neither unique events nor billed consumption.</p>
      <table className="table">
        <caption className="visually-hidden">Observed claims by token component, fictional</caption>
        <thead>
          <tr>
            <th scope="col">Component</th>
            <th scope="col">Observed claim sum</th>
            <th scope="col">Valid claims</th>
            <th scope="col">Explicit zero</th>
            <th scope="col">Absent</th>
            <th scope="col">Null</th>
            <th scope="col">Invalid</th>
            <th scope="col">Parent unavailable</th>
          </tr>
        </thead>
        <tbody>
          {claudeComponents.map(([id, label]) => {
            const component = summary.components[id];
            const { states } = component;
            return (
              <tr key={id} data-component={id} data-outcome={component.value.kind}>
                <th scope="row">{label}</th>
                <td>{claudeValueText(component)}</td>
                <td>{states.valid}</td>
                <td>{states.validZero}</td>
                <td>{states.absent}</td>
                <td>{states.null}</td>
                <td>{states.wrongType + states.outOfRange}</td>
                <td>{states.unsupported}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="page-note">Cache tokens are shown separately and never added to input or output. Cache creation
        uses the detailed breakdown when present and is never added to the flat claim
        ({summary.records.cacheCreationFlatIgnored} record(s) had both).</p>
      <ul className="breakdown" aria-label="Fictional Claude records and coverage">
        <li className="breakdown__row"><span>Records parsed</span>
          <span>{summary.records.observations} (accepted {summary.records.accepted}, qualified {summary.records.qualified}, incomplete {summary.records.incomplete})</span></li>
        <li className="breakdown__row"><span>Records with a valid / no valid token claim</span>
          <span>{summary.records.withValidClaim} / {summary.records.withoutValidClaim}</span></li>
        <li className="breakdown__row"><span>Files read / rejected</span>
          <span>{summary.files.read} / {summary.files.rejected}</span></li>
        <li className="breakdown__row"><span>Lines terminated</span><span>{summary.lines.terminatedLines}</span></li>
        <li className="breakdown__row"><span>Parser rejections / oversized / blank</span>
          <span>{summary.lines.rejected.total} / {summary.lines.oversized} / {summary.lines.blank}</span></li>
        <li className="breakdown__row"><span>Pending tails</span>
          <span>{summary.lines.filesWithPendingTail} file(s), {summary.lines.pendingTailBytes} bytes, not parsed</span></li>
        <li className="breakdown__row"><span>Skipped entries</span>
          <span>{summary.skipped.total} ({summary.skipped.mayHideData} may hide data)</span></li>
        <li className="breakdown__row"><span>Collection</span><span>{summary.completeness}</span></li>
      </ul>
      {gaps.length > 0 ? (
        <ul className="breakdown" aria-label="Fictional Claude coverage gaps">
          {gaps.map((gap) => <li className="breakdown__row" key={gap}>{gap}</li>)}
        </ul>
      ) : <p className="page-note">No known gaps observed. This is not a completeness proof.</p>}
    </Region>
  );
}

function QuotaPanel({ panel }: Readonly<{ panel: QuotaPanelDefinition }>): React.JSX.Element {
  return (
    <article className="quota" aria-labelledby={`quota-${panel.id}-title`}>
      <h3 className="quota__title" id={`quota-${panel.id}-title`}>{panel.title}</h3>
      <Status tone="unavailable">{unavailable}</Status>
      <div className="quota__meter" aria-hidden="true"><span className="quota__meter-empty" /></div>
      <p className="quota__note">{panel.note}</p>
    </article>
  );
}

export function ShellView({ selection, onSelectEnvironment, onSelectHarness, bridgeState = { status: 'loading' }, fictionalClaude, fileDiagnostic, onRunFileDiagnostic, onClearFileDiagnostic }: ShellViewProps): React.JSX.Element {
  const environment = environmentOf(selection);
  const harness = harnessOf(selection);
  const isGeneral = harness.id === 'general';
  const derived = environment.kind === 'derived';
  const summary = aggregateSynthetic(bridgeState.status === 'synthetic' ? bridgeState.snapshot : null,
    environment.id, harness.id);
  const values = [summary?.tokens, undefined, summary?.sessions, summary?.models.length];
  const sourceNames = summary?.sources.map((source) => environments.find((entry) => entry.id === source)?.label).join(' + ');

  return (
    <div className="app" data-environment={environment.id} data-harness={harness.id}>
      <header className="toolbar">
        <p className="toolbar__brand">{shellText.product}</p>
        <nav className="toolbar__environments" aria-label="Environment">
          <ul className="segmented">
            {environments.map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  className="segmented__button"
                  aria-pressed={entry.id === environment.id}
                  onClick={() => onSelectEnvironment(entry.id)}
                >
                  {entry.label}
                  {entry.kind === 'derived' ? <span className="segmented__tag">derived</span> : null}
                </button>
              </li>
            ))}
          </ul>
        </nav>
        <p className="banner" role="note">{shellText.banner}</p>
      </header>

      <nav className="sidebar" aria-label="Harness">
        <p className="sidebar__heading">Harness</p>
        <ul className="sidebar__list">
          {harnesses.map((entry) => (
            <li key={entry.id}>
              <button
                type="button"
                className="sidebar__button"
                aria-current={entry.id === harness.id ? 'page' : undefined}
                onClick={() => onSelectHarness(entry.id)}
              >
                {entry.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <main className="content" aria-labelledby="page-title">
        <div className="content__inner">
          <div className="page-head">
            <h1 id="page-title">{pageHeading(selection)}</h1>
            <Status tone={derived ? 'derived' : 'unavailable'}>{environment.status}</Status>
          </div>
          <p className="page-scope">{environment.scope}</p>
          <p className="page-note">{shellText.explanation}</p>
          <p className="page-note" role="status">
            {bridgeState.status === 'synthetic'
              ? 'Local synthetic bridge responded · DEMO / SYNTHETIC · collection disabled'
              : bridgeState.status === 'loading'
                ? 'Local synthetic bridge: loading · collection disabled'
                : 'Local synthetic bridge unavailable · collection disabled'}
          </p>

          {summary ? <p className="page-note">Synthetic sources: {sourceNames}. Tokens = input tokens + output tokens.
            Sessions use source/harness identities, joined only by explicit fixture equivalence in Computer.
            Demo equivalents count once; unrelated matching values remain distinct.</p> : null}

          <section className="metrics" aria-label="Overview metrics">
            {metricLabels.map((label, index) => (
              <article className="metric" key={label}>
                <h2 className="metric__label">{label}</h2>
                <p className="metric__value">{values[index] ?? unavailable}</p>
                <p className="metric__detail">{values[index] === undefined ? 'Not observed' : 'DEMO / SYNTHETIC'}</p>
              </article>
            ))}
          </section>

          <Region id="fictional-results" title="Fixed fictional results - demonstration only">
            <p className="page-note">Captured from fixed fictional fixtures. Reported claim sums, not deduplicated usage.
              All scenarios are fixed and independent of the selected environment and harness.
              Freshness is logical, not measured age. No real account data is shown.</p>
            <pre className="fictional-results" tabIndex={0} role="region" aria-label="Fixed fictional results provenance and limits">{fictionalResults.preamble}</pre>
            <ul className="scenarios" aria-label="Fictional scenarios, demonstration only">
              {fictionalResults.scenarios.map((scenario) => (
                <li key={scenario.title}>
                  <details className="scenario">
                    <summary>{scenario.title}</summary>
                    <pre className="fictional-results" tabIndex={0} role="region" aria-label={`Fixed fictional output: ${scenario.title}`}>{scenario.text}</pre>
                  </details>
                </li>
              ))}
            </ul>
          </Region>

          {fileDiagnostic !== undefined && environment.id === 'selected-wsl' && harness.id === 'claude-code' ? (
            <Region id="fictional-file-read" title="Fictional one-shot Claude file-read diagnostic">
              <p className="page-note">Fixed fictional test data only. Simulated provenance; not native WSL collection.
                One manual attempt per app run. Results stay separate from all demo totals.</p>
              <button type="button" disabled={fileDiagnostic.attempted} onClick={onRunFileDiagnostic}>Run fictional file-read once</button>
              <button type="button" disabled={!fileDiagnostic.attempted || fileDiagnostic.status === 'cleared'}
                onClick={onClearFileDiagnostic}>Clear diagnostic result</button>
              <p className="page-note" role="status" aria-live="polite">{
                fileDiagnostic.status === 'idle' ? 'Ready for a manual fictional file-read attempt.'
                  : fileDiagnostic.status === 'running' ? 'Reading the fixed fictional file...'
                    : fileDiagnostic.status === 'cleared' ? 'Result cleared from memory. Files are never deleted; the attempt stays consumed.'
                      : fileDiagnostic.status === 'unavailable' ? 'Diagnostic unavailable. No result. Another attempt requires a fresh isolated runtime root and app restart.'
                        : 'Fictional file read finished. Inspect coverage below.'
              }</p>
              {fileDiagnostic.status === 'fictional'
                ? <FictionalClaudePanel id="fictional-file-summary" state={fileDiagnostic} /> : null}
            </Region>
          ) : null}

          {fictionalClaude !== undefined && environment.id === 'selected-wsl' && harness.id === 'claude-code'
            ? <FictionalClaudePanel state={fictionalClaude} /> : null}

          {isGeneral ? null : (
            <section className="quotas" aria-labelledby="quota-section-title">
              <h2 className="section-title" id="quota-section-title">{harness.label} quota and status</h2>
              <p className="section-note">{shellText.quotaScope}</p>
              <div className="quotas__grid">
                {harness.quotaPanels.map((panel) => <QuotaPanel key={panel.id} panel={panel} />)}
              </div>
            </section>
          )}

          <div className="grid">
            <Region id="activity" title={isGeneral ? 'Synthetic activity' : `${harness.label} synthetic activity`}>
              {summary ? <ul className="activity">
                {summary.activity.map((day) => <li className="activity__day" key={day.id}>
                  <span>{day.id} UTC · {day.tokens} synthetic tokens</span>
                  <progress aria-label={`${day.id} UTC synthetic tokens`} value={day.tokens}
                    max={Math.max(1, ...summary.activity.map((row) => row.tokens))}>{day.tokens} tokens</progress>
                </li>)}
              </ul> : <EmptyState title="No spending or activity observed" detail="No synthetic activity available. Collection is disabled." />}
            </Region>

            {isGeneral ? (
              <Region id="harness-breakdown" title="Breakdown by harness">
                <ul className="breakdown">
                  {harnesses.filter((entry) => entry.id !== 'general').map((entry) => {
                    const tokens = summary?.harnesses.find((row) => row.id === entry.id)?.tokens;
                    return (
                      <li className="breakdown__row" key={entry.id}>
                        <span>{entry.label}</span>
                        <Status tone={tokens === undefined ? 'unavailable' : 'synthetic'}>
                          {tokens === undefined ? unavailable : `DEMO · ${tokens} synthetic tokens`}
                        </Status>
                      </li>
                    );
                  })}
                </ul>
              </Region>
            ) : null}

            {derived ? (
              <Region id="sources" title="Derived from">
                <ul className="breakdown">
                  {environments.filter((entry) => entry.kind === 'collection').map((entry) => (
                    <li className="breakdown__row" key={entry.id}>
                      <span>{entry.label}</span>
                      <Status tone="unavailable">{entry.status}</Status>
                    </li>
                  ))}
                </ul>
              </Region>
            ) : null}

            <Region id="models" title={isGeneral ? 'Models' : `${harness.label} models`}>
              <table className="table">
                <caption className="visually-hidden">Models used</caption>
                <thead>
                  <tr>
                    <th scope="col">Model</th>
                    <th scope="col">Tokens</th>
                    <th scope="col">Estimated API cost</th>
                  </tr>
                </thead>
                <tbody>
                  {summary ? summary.models.map((model) => <tr key={model.id}>
                    <th scope="row">{model.id} (DEMO)</th>
                    <td>{model.tokens}</td>
                    <td>{unavailable}</td>
                  </tr>) : <tr>
                    <td colSpan={3}>No models observed. Collection is disabled.</td>
                  </tr>}
                </tbody>
              </table>
            </Region>
          </div>
        </div>
      </main>

      <footer className="statusbar">
        <p>{shellText.statusBar}</p>
      </footer>
    </div>
  );
}

export function App(): React.JSX.Element {
  const fileDiagnostic = React.useSyncExternalStore(
    fictionalFileDiagnostic.subscribe, fictionalFileDiagnostic.getSnapshot, fictionalFileDiagnostic.getSnapshot,
  );
  const [selection, setSelection] = React.useState<Selection>(initialSelection);
  const [bridgeState, setBridgeState] = React.useState<BridgeState>({ status: 'loading' });
  React.useEffect(() => {
    let active = true;
    void syntheticBridge.load().then((state) => { if (active) setBridgeState(state); });
    return () => { active = false; };
  }, []);
  const [fictionalClaude, setFictionalClaude] = React.useState<FictionalClaudeState>({ status: 'loading' });
  React.useEffect(() => {
    let active = true;
    void fictionalClaudeBridge.load().then((state) => { if (active) setFictionalClaude(state); });
    return () => { active = false; };
  }, []);
  const onSelectEnvironment = React.useCallback(
    (id: EnvironmentId) => setSelection((current) => selectEnvironment(current, id)),
    [],
  );
  const onSelectHarness = React.useCallback(
    (id: HarnessId) => setSelection((current) => selectHarness(current, id)),
    [],
  );
  return <ShellView selection={selection} onSelectEnvironment={onSelectEnvironment} onSelectHarness={onSelectHarness} bridgeState={bridgeState}
    fictionalClaude={fictionalClaude} fileDiagnostic={fileDiagnostic}
    onRunFileDiagnostic={() => { void fictionalFileDiagnostic.run(); }} onClearFileDiagnostic={fictionalFileDiagnostic.clear} />;
}
