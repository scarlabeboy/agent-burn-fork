export type FictionalScenario = Readonly<{ title: string; text: string }>;
export type FictionalResultsParts = Readonly<{ preamble: string; scenarios: readonly FictionalScenario[] }>;

const headingLine = /^== (.+) ==$/gmu;

/** Splits the snapshot at its "== title ==" lines; preamble + every scenario text rejoin to the exact input. */
export function splitFictionalResults(snapshot: string): FictionalResultsParts {
  const heads = [...snapshot.matchAll(headingLine)];
  const first = heads[0]?.index ?? snapshot.length;
  return {
    preamble: snapshot.slice(0, first),
    scenarios: heads.map((head, index) => ({
      title: head[1] ?? '',
      text: snapshot.slice(head.index, heads[index + 1]?.index ?? snapshot.length),
    })),
  };
}
