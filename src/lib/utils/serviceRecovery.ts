/** Stable marker the helper emits when a unit write failed, rollback ran,
 *  and systemd could not be reloaded afterwards. */
export const RECOVERY_REQUIRED_PREFIX = 'RECOVERY_REQUIRED:';

/** Browser-persistent storage key: the warning must survive navigation. */
export const RECOVERY_STORAGE_KEY = 'lcp.serviceManager.recoveryRequired';

export type RecoveryEvent =
  | { kind: 'write-failed'; error: unknown }
  | { kind: 'recovery-succeeded' }
  | { kind: 'recovery-failed'; error: unknown };

/** The recovery detail when (and only when) the helper marked the failure. */
export function recoveryWarningFromError(error: unknown): string | null {
  const message = error instanceof Error ? error.message : String(error);
  return message.startsWith(RECOVERY_REQUIRED_PREFIX) ? message : null;
}

/** State transition for the persistent warning.
 *  - a marked write failure installs/refreshes the detail;
 *  - an ordinary failure leaves the state untouched (never creates a warning);
 *  - only a confirmed successful recovery clears it. */
export function reduceRecoveryWarning(
  current: string | null,
  event: RecoveryEvent,
): string | null {
  switch (event.kind) {
    case 'write-failed':
      return recoveryWarningFromError(event.error) ?? current;
    case 'recovery-succeeded':
      return null;
    case 'recovery-failed':
      return current;
  }
}

type ReadStorage = Pick<Storage, 'getItem'>;
type WriteStorage = Pick<Storage, 'setItem' | 'removeItem'>;

export function loadRecoveryWarning(storage: ReadStorage | null): string | null {
  if (!storage) return null;
  try {
    const value = storage.getItem(RECOVERY_STORAGE_KEY);
    return value && value.startsWith(RECOVERY_REQUIRED_PREFIX) ? value : null;
  } catch {
    return null;
  }
}

export function persistRecoveryWarning(
  storage: WriteStorage | null,
  warning: string | null,
): void {
  if (!storage) return;
  try {
    if (warning) storage.setItem(RECOVERY_STORAGE_KEY, warning);
    else storage.removeItem(RECOVERY_STORAGE_KEY);
  } catch {
    /* storage is best-effort; the in-memory warning still shows */
  }
}
