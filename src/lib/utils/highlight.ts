// Safe search-highlight segmentation for UNTRUSTED text (journal messages,
// process names, user names, commands, ...).
//
// This module NEVER builds HTML strings. Callers render the returned segments
// as ordinary Svelte text nodes (auto-escaped) with `<mark>` wrapping the
// matches — replacing the previous raw-HTML `highlight()` helper, which
// interpolated journal content into `{@html ...}` (script-injection defect).

/** Escape all RegExp metacharacters so a search query is a plain literal. */
export function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export interface HighlightSegment {
  text: string;
  hit: boolean;
}

/**
 * Split `text` into literal segments, marking those that match the search
 * query `q` (case-insensitive). `q` is treated as a literal string — no
 * regex characters are honored. With an empty query or text, the whole text
 * is returned as one plain segment.
 */
export function highlightSegments(text: string, q: string): HighlightSegment[] {
  if (!q || !text) return [{ text: text || '', hit: false }];
  const re = new RegExp(`(${escapeRegex(q)})`, 'gi');
  const segments: HighlightSegment[] = [];
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    // An escaped non-empty literal is at least one character long, so a match
    // can never be zero-length here; the assertion keeps exec() advancing in
    // the general (defensive) case.
    if (m[0].length === 0) {
      re.lastIndex += 1;
      continue;
    }
    if (m.index > last) segments.push({ text: text.slice(last, m.index), hit: false });
    segments.push({ text: m[0], hit: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) segments.push({ text: text.slice(last), hit: false });
  return segments;
}
