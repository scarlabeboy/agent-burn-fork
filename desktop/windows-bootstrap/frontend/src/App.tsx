import type { ClaudeCopyReplay } from './claudeCopyReplay.ts';
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
import type { ComponentId, ComponentSummary, FictionalClaudeState } from './fictionalClaudeBridge.ts';
import { fictionalFileDiagnostic } from './fictionalFileDiagnostic.ts';
import { claudeCopyDiagnostic } from './claudeCopyDiagnostic.ts';
import type { CopyDiagnosticState } from './claudeCopyDiagnostic.ts';
import type { ClaudeClaimSummary } from './fictionalClaudeBridge.ts';
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
  copyDiagnostic?: CopyDiagnosticState;
  onRunCopyDiagnostic?: (relativePath: string) => void;
  onClearCopyDiagnostic?: () => void;
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
function claudeValueText(component: ComponentSummary, fictional: boolean): string {
  switch (component.value.kind) {
    case 'sum': return `${component.value.amount} observed claims${fictional ? ' (fictional)' : ''}`;
    case 'no-valid-claims': return 'No valid claims';
    case 'indeterminate': return 'Unavailable - no observations';
    case 'overflow': return 'Overflow - sum exceeds 64 bits';
  }
}

function claudeGaps(summary: ClaudeClaimSummary): string[] {
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
  return (
    <Region id={id} title={title}>
      <p><Status tone="synthetic">FICTIONAL FIXTURE · not your data</Status></p>
      <p className="page-note">Produced by the Rust parser, collector and summary from a fixed fictional fixture. No
        real Claude history, account, quota or spending is read. This is not a Selected WSL, Windows or Computer total.</p>
      <p className="page-note">Counting basis: observed record claims, every record counted. Duplicate-record identity
        is unresolved, so repeated records count each time; these are neither unique events nor billed consumption.</p>
      <ClaudeSummaryDetails summary={summary} fictional />
    </Region>
  );
}

function ClaudeSummaryDetails({ summary, fictional = false }: Readonly<{ summary: ClaudeClaimSummary; fictional?: boolean }>): React.JSX.Element {
  const gaps = claudeGaps(summary);
  return <>
      <table className="table">
        <caption className="visually-hidden">Observed claims by token component{fictional ? ', fictional' : ''}</caption>
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
                <td>{claudeValueText(component, fictional)}</td>
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
      <ul className="breakdown" aria-label={fictional ? 'Fictional Claude records and coverage' : 'Copy records and coverage'}>
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
        <ul className="breakdown" aria-label={fictional ? 'Fictional Claude coverage gaps' : 'Copy coverage gaps'}>
          {gaps.map((gap) => <li className="breakdown__row" key={gap}>{gap}</li>)}
        </ul>
      ) : <p className="page-note">No known gaps observed. This is not a completeness proof.</p>}
  </>;
}

function ClaudeCopyReplayDetails({ replay }: Readonly<{ replay: ClaudeCopyReplay }>): React.JSX.Element {
  let unavailable: string;
  switch (replay.status) {
    case 'identity-budget-exhausted':
      unavailable = `Replay evaluation exhausted: ${replay.observations} observations exceed the identity limit of ${replay.limit}.`; break;
    case 'invalid-identity-limit': unavailable = 'Replay evaluation invalid: identity limit is invalid.'; break;
    case 'wrong-harness': unavailable = 'Replay evaluation unavailable: wrong harness.'; break;
    case 'allocation-failure': unavailable = 'Replay evaluation unavailable: allocation failure.'; break;
    case 'evaluated':
      return <div aria-label="Conservative exact-observation replay diagnostics">
        <h3>Conservative exact-observation replay diagnostics</h3>
        <p className="page-note">Basis: source-scoped exact observation claims v1, this batch only.
          Equal retained evidence does not establish unique physical events, cross-file history identity or billable consumption.
          Rejected, oversized and pending lines remain coverage limitations and may hide conflicts or further copies.</p>
        <ul className="breakdown">
          <li>Eligible observations: {replay.eligibleObservations}</li>
          <li>Representatives: {replay.representatives}</li>
          <li>Exact replay copies: {replay.exactReplayCopies}</li>
          <li>Unresolved observations: {replay.unresolvedObservations}</li>
        </ul>
        <p className="page-note">Unresolved reasons overlap; do not add them as a count of observations.</p>
        <ul className="breakdown">
          <li>Invalid identity: {replay.unresolved.invalidIdentity}</li>
          <li>Unsupported record type: {replay.unresolved.unsupportedRecordType}</li>
          <li>Sidechain: {replay.unresolved.sidechain}</li>
          <li>Invalid claims: {replay.unresolved.invalidClaims}</li>
          <li>Conflicting evidence: {replay.unresolved.conflictingEvidence}</li>
        </ul>
        <p className="page-note">Conflict attribution: Each counter counts an observation once if it has at least one
          conflicting peer with that reason or relation. Reasons and relations overlap, including across peers;
          do not add them as unique observations, pairs or physical events. Ineligible peer counts are directional.</p>
        <ul className="breakdown">
          <li>Related ineligible peer: {replay.conflicts.ineligiblePeer}</li>
          <li>Differing identity or metadata: {replay.conflicts.identityMetadata}</li>
          <li>Differing token fields: {replay.conflicts.tokenFields}</li>
          <li>Differing parser evidence: {replay.conflicts.parserEvidence}</li>
          <li>UUID-related conflict: {replay.conflicts.uuidRelated}</li>
          <li>Session/message-related conflict: {replay.conflicts.sessionMessageRelated}</li>
        </ul>
        <p className="page-note">Eligible-subset token components, one representative per eligible group.
          NOT the complete or billed usage total. Unresolved observations are excluded, never zero consumption.
          Evaluated does not establish complete coverage.</p>
        <ul className="breakdown">{claudeComponents.map(([id, label]) =>
          <li key={id}>{label}: {claudeValueText(replay.components[id], false)}</li>)}</ul>
      </div>;
    default: replay satisfies never; throw new Error('Invalid replay state');
  }
  return <p className="page-note" role="status">{unavailable} No eligible-subset counts or tokens are available. Raw claims and coverage remain below.</p>;
}

function ClaudeCopyPanel({ state, onRun, onClear }: Readonly<{
  state: CopyDiagnosticState; onRun?: (relativePath: string) => void; onClear?: () => void;
}>): React.JSX.Element {
  const [relativePath, setRelativePath] = React.useState('');
  return <Region id="claude-copy" title="Explicit Windows Claude JSONL copy - manual read">
    <p className="page-note">Explicitly configured Windows copy; original source and environment are not established.
      This result stays separate from all environment, Computer and demo totals.</p>
    <p className="page-note">One manual attempt per app run, including failures. No refresh or persistence.
      Counting basis: observed record claims. Duplicate identity is unresolved; these are not unique events or billed consumption.</p>
    <p className="page-note">Copy limits: one file, 1 MiB per file, 64 KiB per line and 1,024 retained record outcomes.</p>
    <label>Relative JSONL path (use / between folders)
      <input value={relativePath} maxLength={512} disabled={state.attempted}
        onChange={(event) => setRelativePath(event.target.value)} autoComplete="off" spellCheck={false} />
    </label>
    <button type="button" disabled={state.attempted || relativePath.length === 0}
      onClick={() => { onRun?.(relativePath); setRelativePath(''); }}>Read selected copy once</button>
    <button type="button" disabled={!state.attempted || state.status === 'cleared'}
      onClick={() => { setRelativePath(''); onClear?.(); }}>Clear copy result</button>
    <p className="page-note" role="status" aria-live="polite">{
      state.status === 'idle' ? 'Ready. A separately configured copy root is required.'
        : state.status === 'running' ? 'Reading selected copy. No result yet.'
          : state.status === 'cleared' ? 'Result cleared from memory. Attempt remains consumed; late results are discarded.'
            : state.status === 'unavailable' ? `Unavailable: ${state.code}. No result; restart required for another attempt.`
              : state.status === 'copy' ? (state.summary.files.read === 0 ? 'File was not read successfully. See file failure counts; no usage conclusion is available.'
                : state.summary.records.withValidClaim === 0 ? 'File read succeeded; no valid token claims were recognized. This does not establish zero usage or a parser defect.'
                  : 'File read succeeded; valid token claims were recognized. Sums describe observed record claims only.') : ''
    }</p>
    {state.status === 'copy' ? <>
      <p className="page-note">Captured once; current freshness is unknown. Complete file reading does not establish
        complete usage. Session events and records with absent or null usage can have no token claims.</p>
      {state.summary.records.withoutValidClaim > 0 || state.summary.records.withMissingClaim > 0
        || state.summary.records.withNullClaim > 0 || state.summary.records.withInvalidClaim > 0 ?
        <p className="page-note">Records without valid claims and missing, null or invalid components leave token coverage incomplete.</p> : null}
      {state.summary.caveats.unparsedLines || state.summary.caveats.pendingTails ?
        <p className="page-note">Malformed, oversized or parser-refused lines and unterminated tails are not counted.
          Coverage excludes these bytes; any shown sums cover only retained valid claims.</p> : null}
      {state.summary.files.tooLarge > 0 || state.summary.files.byteBudget > 0 || state.summary.files.outputLimit > 0 ?
        <p className="page-note">A file or byte budget can prevent reading. Record budget exhaustion discards all claims from the file;
          no truncated prefix total is published. Restart is required for another attempt.</p> : null}
      <ClaudeCopyReplayDetails replay={state.replay} />
      <ClaudeSummaryDetails summary={state.summary} />
    </> : null}
  </Region>;
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

export function ShellView({ selection, onSelectEnvironment, onSelectHarness, bridgeState = { status: 'loading' }, fictionalClaude, fileDiagnostic, onRunFileDiagnostic, onClearFileDiagnostic, copyDiagnostic, onRunCopyDiagnostic, onClearCopyDiagnostic }: ShellViewProps): React.JSX.Element {
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
        <p className="banner" role="note">{copyDiagnostic === undefined ? shellText.banner : 'DEMO / SYNTHETIC - dashboard; separate manual copy reader'}</p>
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
          <p className="page-scope">{copyDiagnostic !== undefined && environment.id === 'windows'
            ? 'Windows dashboard fixture. Automatic collection disabled; manual copy reader below is separate.' : environment.scope}</p>
          <p className="page-note">{copyDiagnostic === undefined ? shellText.explanation : 'Dashboard is synthetic; manual copy reader is separate. No account, quota or spending is observed.'}</p>
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

          {copyDiagnostic !== undefined && environment.id === 'windows' && harness.id === 'claude-code'
            ? <ClaudeCopyPanel state={copyDiagnostic} onRun={onRunCopyDiagnostic} onClear={onClearCopyDiagnostic} /> : null}

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
        <p>{copyDiagnostic === undefined ? shellText.statusBar : 'Synthetic dashboard - separate manual copy result; no account connected'}</p>
      </footer>
    </div>
  );
}

export function App(): React.JSX.Element {
  const copyDiagnostic = React.useSyncExternalStore(
    claudeCopyDiagnostic.subscribe, claudeCopyDiagnostic.getSnapshot, claudeCopyDiagnostic.getSnapshot,
  );
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
    copyDiagnostic={copyDiagnostic} onRunCopyDiagnostic={(path) => { void claudeCopyDiagnostic.run(path); }}
    onClearCopyDiagnostic={claudeCopyDiagnostic.clear} fictionalClaude={fictionalClaude} fileDiagnostic={fileDiagnostic}
    onRunFileDiagnostic={() => { void fictionalFileDiagnostic.run(); }} onClearFileDiagnostic={fictionalFileDiagnostic.clear} />;
}
