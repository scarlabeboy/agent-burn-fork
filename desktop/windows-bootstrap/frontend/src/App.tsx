import * as React from 'react';
import { aggregateSynthetic } from './syntheticAggregation.ts';
import { syntheticBridge } from './syntheticBridge.ts';
import type { BridgeState } from './syntheticBridge.ts';
import { environmentOf, harnessOf, initialSelection, pageHeading, selectEnvironment, selectHarness } from './selection.ts';
import type { Selection } from './selection.ts';
import { environments, harnesses, shellText, unavailable } from './synthetic.ts';
import type { EnvironmentId, HarnessId, QuotaPanelDefinition } from './synthetic.ts';
import { fictionalResultsSnapshot } from './fictionalResultsSnapshot.ts';

const metricLabels = Object.freeze(['Tokens', 'Estimated API cost', 'Sessions', 'Models'] as const);

type ShellViewProps = Readonly<{
  selection: Selection;
  onSelectEnvironment: (id: EnvironmentId) => void;
  onSelectHarness: (id: HarnessId) => void;
  bridgeState?: BridgeState;
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

export function ShellView({ selection, onSelectEnvironment, onSelectHarness, bridgeState = { status: 'loading' } }: ShellViewProps): React.JSX.Element {
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
          <details>
            <summary>Show captured results and coverage</summary>
            <pre className="fictional-results" tabIndex={0} role="region" aria-label="Fixed fictional collection output">{fictionalResultsSnapshot}</pre>
          </details>
        </Region>

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
      </main>

      <footer className="statusbar">
        <p>{shellText.statusBar}</p>
      </footer>
    </div>
  );
}

export function App(): React.JSX.Element {
  const [selection, setSelection] = React.useState<Selection>(initialSelection);
  const [bridgeState, setBridgeState] = React.useState<BridgeState>({ status: 'loading' });
  React.useEffect(() => {
    let active = true;
    void syntheticBridge.load().then((state) => { if (active) setBridgeState(state); });
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
  return <ShellView selection={selection} onSelectEnvironment={onSelectEnvironment} onSelectHarness={onSelectHarness} bridgeState={bridgeState} />;
}
