import { describe, expect, it } from 'vitest';
import {
  PROCESS_PRIORITY_MAX,
  PROCESS_PRIORITY_MIN,
  canReniceProcess,
  isValidProcessPriority,
  processPriorityLabel,
} from './processPriority';

describe('process priority validation', () => {
  it('accepts the full niceness range only as integers', () => {
    expect(isValidProcessPriority(PROCESS_PRIORITY_MIN)).toBe(true);
    expect(isValidProcessPriority(PROCESS_PRIORITY_MAX)).toBe(true);
    expect(isValidProcessPriority(0)).toBe(true);
    expect(isValidProcessPriority(-5)).toBe(true);
    expect(isValidProcessPriority(PROCESS_PRIORITY_MIN - 1)).toBe(false);
    expect(isValidProcessPriority(PROCESS_PRIORITY_MAX + 1)).toBe(false);
    expect(isValidProcessPriority(0.5)).toBe(false);
    expect(isValidProcessPriority(Number.NaN)).toBe(false);
  });

  it('allows renicing only the current user\'s processes', () => {
    expect(canReniceProcess('alice', 'alice')).toBe(true);
    expect(canReniceProcess('root', 'alice')).toBe(false);
    expect(canReniceProcess('', 'alice')).toBe(false);
    expect(canReniceProcess('alice', '')).toBe(false);
  });

  it('labels the scheduling bands', () => {
    expect(processPriorityLabel(-20)).toBe('High Priority');
    expect(processPriorityLabel(-1)).toBe('High Priority');
    expect(processPriorityLabel(0)).toBe('Normal Priority');
    expect(processPriorityLabel(1)).toBe('Low Priority');
    expect(processPriorityLabel(19)).toBe('Low Priority');
  });
});
