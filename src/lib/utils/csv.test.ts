import { describe, expect, it } from 'vitest';
import { csvCell } from './csv.ts';

describe('csvCell', () => {
  it('quotes and escapes ordinary values', () => {
    expect(csvCell('systemd[1]: Started')).toEqual('"systemd[1]: Started"');
    expect(csvCell('say "hi"')).toEqual('"say ""hi"""');
    expect(csvCell(null)).toEqual('""');
    expect(csvCell(42)).toEqual('"42"');
  });

  it('neutralises anything a spreadsheet would read as a formula', () => {
    for (const value of [
      '=1+1',
      '+1',
      '-1',
      '@SUM(A1)',
      '\tstart',
      '\rstart',
      '=cmd|\'/C calc\'!A0',
    ]) {
      expect(csvCell(value), JSON.stringify(value)).toMatch(/^"'/);
    }
  });

  it('only rewrites a leading trigger, not one appearing mid-message', () => {
    expect(csvCell('Failed password for invalid user root')).toEqual(
      '"Failed password for invalid user root"'
    );
    expect(csvCell('value =1 after')).toEqual('"value =1 after"');
  });

  it('leaves an already-apostrophe-prefixed value untouched', () => {
    expect(csvCell("'already safe'")).toEqual('"\'already safe\'"');
  });
});
