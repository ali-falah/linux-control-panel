import { describe, expect, it } from 'vitest';
import {
  RECOVERY_REQUIRED_PREFIX,
  loadRecoveryWarning,
  persistRecoveryWarning,
  recoveryWarningFromError,
  reduceRecoveryWarning,
} from './serviceRecovery';

const marked = `${RECOVERY_REQUIRED_PREFIX} daemon-reload failed; rollback restored the file; systemd still stale`;

describe('service recovery warning state', () => {
  it('installs the warning for a helper-marked recovery failure', () => {
    expect(recoveryWarningFromError(marked)).toBe(marked);
    expect(
      reduceRecoveryWarning(null, { kind: 'write-failed', error: new Error(marked) }),
    ).toBe(marked);
  });

  it('does not install a warning for ordinary operation failures', () => {
    expect(recoveryWarningFromError('lcp-admin: permission denied')).toBeNull();
    expect(
      reduceRecoveryWarning(null, {
        kind: 'write-failed',
        error: new Error('lcp-admin: invalid unit name'),
      }),
    ).toBeNull();
    // an existing warning is preserved by an unrelated failure
    expect(
      reduceRecoveryWarning(marked, { kind: 'write-failed', error: 'unrelated' }),
    ).toBe(marked);
  });

  it('clears only after a confirmed successful recovery', () => {
    expect(reduceRecoveryWarning(marked, { kind: 'recovery-succeeded' })).toBeNull();
    expect(
      reduceRecoveryWarning(marked, { kind: 'recovery-failed', error: 'still broken' }),
    ).toBe(marked);
  });

  it('persists and reloads the warning across navigation', () => {
    const data = new Map<string, string>();
    const storage = {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => void data.set(key, value),
      removeItem: (key: string) => void data.delete(key),
    };
    persistRecoveryWarning(storage, marked);
    expect(loadRecoveryWarning(storage)).toBe(marked);
    persistRecoveryWarning(storage, null);
    expect(loadRecoveryWarning(storage)).toBeNull();
  });
});
