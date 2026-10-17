import * as React from 'react';
import { environmentOf, harnessOf, initialSelection, pageHeading, selectEnvironment, selectHarness } from './selection.ts';
import type { Selection } from './selection.ts';
import { environments, harnesses, shellText, unavailable } from './synthetic.ts';
import type { EnvironmentId, HarnessId, QuotaPanelDefinition } from './synthetic.ts';

const metricLabels = Object.freeze(['Tokens', 'Estimated API cost', 'Sessions', 'Models'] as const);

type ShellViewProps = Readonly<{
  selection: Selection;
  onSelectEnvironment: (id: EnvironmentId) => void;
  onSelectHarness: (id: HarnessId) => void;
}>;

function Status({ tone, children }: Readonly<{ tone: 'unavailable' | 'derived'; children: React.ReactNode }>): React.JSX.Element {
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

export function ShellView({ selection, onSelectEnvironment, onSelectHarness }: ShellViewProps): React.JSX.Element {
  const environment = environmentOf(selection);
  const harness = harnessOf(selection);
  const isGeneral = harness.id === 'general';
  const derived = environment.kind === 'derived';

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

        <section className="metrics" aria-label="Overview metrics">
          {metricLabels.map((label) => (
            <article className="metric" key={label}>
              <h2 className="metric__label">{label}</h2>
              <p className="metric__value">{unavailable}</p>
              <p className="metric__detail">Not observed</p>
            </article>
          ))}
        </section>

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
          <Region id="activity" title={isGeneral ? 'Spending and activity' : `${harness.label} activity`}>
            <EmptyState title="No spending or activity observed" detail="Collection is disabled, so this chart has no data." />
          </Region>

          {isGeneral ? (
            <Region id="harness-breakdown" title="Breakdown by harness">
              <ul className="breakdown">
                {harnesses.filter((entry) => entry.id !== 'general').map((entry) => (
                  <li className="breakdown__row" key={entry.id}>
                    <span>{entry.label}</span>
                    <Status tone="unavailable">{unavailable}</Status>
                  </li>
                ))}
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
                <tr>
                  <td colSpan={3}>No models observed. Collection is disabled.</td>
                </tr>
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
  const onSelectEnvironment = React.useCallback(
    (id: EnvironmentId) => setSelection((current) => selectEnvironment(current, id)),
    [],
  );
  const onSelectHarness = React.useCallback(
    (id: HarnessId) => setSelection((current) => selectHarness(current, id)),
    [],
  );
  return <ShellView selection={selection} onSelectEnvironment={onSelectEnvironment} onSelectHarness={onSelectHarness} />;
}
