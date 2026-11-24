/**
 * Roving-focus index for menu items. A `current` of -1 means focus is on the
 * menu container rather than an item: ArrowDown then starts at the first
 * item and ArrowUp at the last. Indices wrap in both directions.
 */
export function nextMenuIndex(
  current: number,
  length: number,
  direction: 'up' | 'down',
): number {
  if (length <= 0) return -1;
  if (direction === 'down') {
    return current < 0 ? 0 : (current + 1) % length;
  }
  return current < 0 ? length - 1 : (current - 1 + length) % length;
}
