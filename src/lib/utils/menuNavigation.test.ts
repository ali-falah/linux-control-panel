import { describe, expect, it } from 'vitest';
import { nextMenuIndex } from './menuNavigation';

describe('nextMenuIndex', () => {
  it('starts at the first item on ArrowDown when no item has focus', () => {
    expect(nextMenuIndex(-1, 5, 'down')).toBe(0);
  });

  it('starts at the last item on ArrowUp when no item has focus', () => {
    expect(nextMenuIndex(-1, 5, 'up')).toBe(4);
  });

  it('wraps past the end and the start', () => {
    expect(nextMenuIndex(4, 5, 'down')).toBe(0);
    expect(nextMenuIndex(0, 5, 'up')).toBe(4);
  });

  it('moves one item in the requested direction', () => {
    expect(nextMenuIndex(2, 5, 'down')).toBe(3);
    expect(nextMenuIndex(2, 5, 'up')).toBe(1);
  });

  it('handles an empty menu', () => {
    expect(nextMenuIndex(-1, 0, 'down')).toBe(-1);
    expect(nextMenuIndex(0, 0, 'up')).toBe(-1);
  });
});
