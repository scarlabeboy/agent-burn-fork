import * as React from 'react';
import { synthetic } from './synthetic.ts';

export function App(): React.JSX.Element {
  return (
    <main className="bootstrap-shell">
      <header className="bootstrap-shell__header">
        <h1>{synthetic.heading}</h1>
        <p className="bootstrap-shell__banner" role="note">{synthetic.banner}</p>
        <p className="bootstrap-shell__explanation">{synthetic.explanation}</p>
      </header>
      <section className="bootstrap-shell__subjects" aria-label="Synthetic environments and derived view">
        {synthetic.subjects.map((subject) => (
          <article className="bootstrap-shell__card" key={subject.id} aria-labelledby={`${subject.id}-label`}>
            <h2 id={`${subject.id}-label`}>{subject.label}</h2>
            <p>{subject.status}</p>
          </article>
        ))}
      </section>
    </main>
  );
}
