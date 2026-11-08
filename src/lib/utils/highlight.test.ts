// STEP 5A (W-10) regression tests: journal content is untrusted, so the
// highlight util must return plain segments that a Svelte template renders
// as escaped text — every kind of hostile payload stays inert.
import { describe, expect, it } from 'vitest';
import { escapeRegex, highlightSegments } from './highlight';

/** A payload must survive as inert text: it may be split into segments, but
 *  the concatenated text equals the original and every segment is a plain
 *  string (the Svelte template escapes it into a text node; no HTML is
 *  ever built). */
function assertInert(text: string, q: string) {
  const segs = highlightSegments(text, q);
  expect(segs.map((s) => s.text).join('')).toBe(text);
  for (const seg of segs) {
    expect(typeof seg.text).toBe('string');
    expect(seg.text).not.toContain('<mark');
  }
}

describe('highlightSegments — untrusted journal payload stays inert', () => {
  it('returns the whole text as one plain segment for scripts and handlers', () => {
    for (const payload of [
      '<script>alert(1)</script>',
      '<img src=x onerror=alert(1)>',
      '<a href="javascript:alert(1)">x</a>',
      '</textarea><script>alert(1)</script>',
      '<b><i unclosed',
      '" onclick="alert(document.cookie)"',
      "'onmouseover=alert(1)",
      '<s style="background:url(javascript:alert(1))">',
    ]) {
      assertInert(payload, 'alert');
    }
  });

  it('never reorders or rewrites the original bytes, with or without a match', () => {
    assertInert("<img src=x onerror=alert('xss')>", '<img');
    assertInert('<script>alert(1)</script>', '<script>');
    assertInert('rm -rf --no-preserve-root /', 'rm');
  });

  it('treats the query as a literal, case-insensitive', () => {
    const segs = highlightSegments('Error FAIL success', 'fail');
    expect(segs.map((s) => s.text).join('')).toBe('Error FAIL success');
    expect(segs).toEqual([
      { text: 'Error ', hit: false },
      { text: 'FAIL', hit: true },
      { text: ' success', hit: false },
    ]);
  });

  it('regex-special characters in the query are literals, never magic', () => {
    for (const q of ['.*', '|', '((', '[a-z', '{2,}', '\\d', '$', '^', '*', '+', '?', 'a)b', 'x}y{z']) {
      const segs = highlightSegments('verified clean entry', q);
      expect(segs.map((s) => s.text).join('')).toBe('verified clean entry');
      expect(segs.every((s) => s.hit === false)).toBe(true);
    }
    // but a literal query that matches still highlights
    expect(highlightSegments('value .* here', '.*')).toEqual([
      { text: 'value ', hit: false },
      { text: '.*', hit: true },
      { text: ' here', hit: false },
    ]);
  });

  it('also escapes query characters when only the query is hostile', () => {
    const segs = highlightSegments('systemd[1]: started', '[1]');
    expect(segs.map((s) => s.text).join('')).toBe('systemd[1]: started');
    expect(segs).toEqual([
      { text: 'systemd', hit: false },
      { text: '[1]', hit: true },
      { text: ': started', hit: false },
    ]);
  });

  it('highlights every occurrence of a query', () => {
    expect(highlightSegments('a user user user', 'user').filter((s) => s.hit).length).toBe(3);
  });

  it('handles empty query, empty text, and exact-slice boundaries', () => {
    expect(highlightSegments('', 'q')).toEqual([{ text: '', hit: false }]);
    expect(highlightSegments('plain', '')).toEqual([{ text: 'plain', hit: false }]);
    expect(highlightSegments('start', 'start')).toEqual([{ text: 'start', hit: true }]);
    expect(highlightSegments('end', 'end')).toEqual([{ text: 'end', hit: true }]);
  });
});

describe('escapeRegex', () => {
  it('escapes every RegExp metacharacter', () => {
    expect(escapeRegex('^$.*+?()[]{}|\\')).toBe('\\^\\$\\.\\*\\+\\?\\(\\)\\[\\]\\{\\}\\|\\\\');
    expect(escapeRegex('safe')).toBe('safe');
  });

  it('makes a query safe for the RegExp constructor (no throw, no magic)', () => {
    expect(() => new RegExp(escapeRegex('(a|b['))).not.toThrow();
    expect(() => new RegExp('(a|b[')).toThrow();
  });
});
