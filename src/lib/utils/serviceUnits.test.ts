import { describe, expect, it } from 'vitest';
import { bootEnableState } from './serviceUnits';

describe('bootEnableState', () => {
  it('treats persistent and runtime enablement as enabled', () => {
    expect(bootEnableState('enabled')).toBe('enabled');
    expect(bootEnableState('enabled-runtime')).toBe('enabled-runtime');
  });

  it('marks states systemd cannot enable as unavailable', () => {
    for (const state of [
      'static',
      'indirect',
      'generated',
      'alias',
      'transient',
      'masked',
      'masked-runtime',
      'bad',
    ]) {
      expect(bootEnableState(state)).toBe('unavailable');
    }
  });

  it('falls back to disabled for disabled and unknown states', () => {
    expect(bootEnableState('disabled')).toBe('disabled');
    expect(bootEnableState('')).toBe('disabled');
    expect(bootEnableState(null)).toBe('disabled');
    expect(bootEnableState(undefined)).toBe('disabled');
    expect(bootEnableState('some-future-state')).toBe('disabled');
  });
});
