import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App } from '../frontend/src/App.tsx';

const render = () => renderToStaticMarkup(createElement(App));

const attribute = (attributes: string, name: 'id' | 'aria-labelledby' | 'role') =>
  attributes.match(new RegExp(`\\s${name}="([^"]*)"`, 'u'))?.[1];

test('clearly identifies the synthetic shell and absence of observed data', () => {
  const html = render();
  assert.match(html, /<h1[^>]*>Agent Burn Fork — Windows shell<\/h1>/u);
  assert.match(html, /Synthetic placeholder — collection disabled/u);
  assert.match(html, /No provider usage, quota or spending has been observed\./u);
});

test('renders exactly three distinct subjects with their independently expected statuses', () => {
  const html = render();
  const cards = [...html.matchAll(/<article\b[^>]*>(.*?)<\/article>/gu)].map((match) => match[1]);
  assert.equal(cards.length, 3);
  assert.match(cards[0] ?? '', /<h2[^>]*>Windows<\/h2>.*<p>Not connected<\/p>/u);
  assert.match(cards[1] ?? '', /<h2[^>]*>Selected WSL<\/h2>.*<p>Not connected; no distro discovered<\/p>/u);
  assert.match(cards[2] ?? '', /<h2[^>]*>Computer<\/h2>.*<p>Derived view only; no collection source<\/p>/u);
});

test('does not imply measurements or expose collection controls or external resources', () => {
  const html = render();
  const text = html.replace(/<[^>]*>/gu, '');
  assert.doesNotMatch(text, /[0-9$€£%]/u);
  assert.doesNotMatch(html, /<(?:button|input|form|a|iframe|img|script)\b|(?:href|src)=/u);
  assert.doesNotMatch(text, /last updated|observed zero|tokens used|quota remaining/iu);
});

test('renders deterministically without runtime inputs', () => {
  assert.equal(render(), render());
});

test('identifies the synthetic banner uniquely with the note role', () => {
  const banners = [...render().matchAll(/<p\b([^>]*)>(.*?)<\/p>/gu)]
    .filter((match) => match[2] === 'Synthetic placeholder — collection disabled');
  assert.equal(banners.length, 1, 'one synthetic banner must be identifiable');
  assert.equal(attribute(banners[0]?.[1] ?? '', 'role'), 'note', 'the synthetic banner must keep its note role');
});

test('resolves labelledby references to unique targets and each card to its own subject heading', () => {
  const html = render();
  const elements = [...html.matchAll(/<([a-z][a-z0-9]*)\b([^>]*)>/gu)];
  const labelledby = elements.map((element) => attribute(element[2] ?? '', 'aria-labelledby'))
    .filter((value): value is string => value !== undefined);
  for (const references of labelledby) {
    assert.notEqual(references.trim(), '', 'labelledby references must not be empty');
    for (const id of references.trim().split(/\s+/u)) {
      const targets = elements.filter((target) => attribute(target[2] ?? '', 'id') === id);
      assert.equal(targets.length, 1, `labelledby target ${id} must exist exactly once`);
    }
  }

  const cards = [...html.matchAll(/<article\b([^>]*)>(.*?)<\/article>/gu)];
  assert.equal(cards.length, 3);
  const subjects = ['Windows', 'Selected WSL', 'Computer'];
  for (const [index, card] of cards.entries()) {
    const headings = [...(card[2] ?? '').matchAll(/<h2\b([^>]*)>(.*?)<\/h2>/gu)];
    assert.equal(headings.length, 1, 'each card must contain one subject heading');
    const id = attribute(headings[0]?.[1] ?? '', 'id');
    assert.ok(id, 'the subject heading must have an ID');
    assert.equal(attribute(card[1] ?? '', 'aria-labelledby'), id, 'each card must be labelled by its own heading');
    assert.equal(headings[0]?.[2], subjects[index], 'the three subject labels must remain distinct');
  }
});

test('keeps all rendered IDs unique', () => {
  const ids = [...render().matchAll(/\sid="([^"]*)"/gu)].map((match) => match[1]);
  assert.equal(ids.length, 3, 'all three subject headings must have IDs');
  assert.equal(new Set(ids).size, ids.length, 'rendered IDs must be unique');
});
