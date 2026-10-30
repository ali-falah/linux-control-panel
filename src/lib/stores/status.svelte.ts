import { invoke } from '@tauri-apps/api/core';

export interface StatusEntry {
  command: string;
  exitCode: number | null;
  timestamp: Date;
  success: boolean;
}

/** Real administrator state, exactly as PolicyKit reports it. */
export interface AdminModeState {
  /** A temporary PolicyKit authorisation exists for this process. */
  on: boolean;
  /** Epoch ms at which PolicyKit drops it (converted from its `expires_at`). */
  expiresAt: number | null;
  /** Why the state could not be read or changed, when it could not be. */
  error: string | null;
}

/** The wire shape of the admin-mode commands (expiry in Unix seconds). */
interface BackendAdminModeState {
  on: boolean;
  expires_at: number | null;
  error: string | null;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

class StatusStore {
  lastEntry = $state<StatusEntry | null>(null);
  busy = $state(false);
  busyLabel = $state('');

  /** The root owned helper (`lcp-admin`) is installed and can be authorised. */
  helperInstalled = $state(false);

  /** Administrator mode — the status bar toggle, backed by PolicyKit. */
  adminMode = $state<AdminModeState>({ on: false, expiresAt: null, error: null });

  /** A short-lived, calm note shown next to the toggle. */
  adminNotice = $state<string | null>(null);

  /** The error of the last explicit toggle attempt; cleared on the next one. */
  adminActionError = $state<string | null>(null);

  setLastCommand(command: string, exitCode: number, success: boolean) {
    this.lastEntry = {
      command,
      exitCode,
      timestamp: new Date(),
      success,
    };
  }

  setBusy(label: string) {
    this.busy = true;
    this.busyLabel = label;
  }

  clearBusy() {
    this.busy = false;
    this.busyLabel = '';
  }

  /** Seconds left on the real PolicyKit authorisation, `null` when there is none. */
  get adminModeRemaining(): number | null {
    const until = this.adminMode.expiresAt;
    if (!this.adminMode.on || until === null) return null;
    const remaining = Math.round((until - Date.now()) / 1000);
    return remaining > 0 ? remaining : null;
  }

  /**
   * Adopt a state returned by the backend. A poll that finds the authorisation
   * gone while the toggle still showed On turns into a one-off "expired" note
   * instead of the switch silently flipping.
   */
  applyAdminMode(next: BackendAdminModeState, notice?: string) {
    const wasOn = this.adminMode.on;
    this.adminMode = {
      on: next.on,
      expiresAt: next.expires_at === null ? null : next.expires_at * 1000,
      error: next.error,
    };
    if (notice !== undefined) {
      this.adminNotice = notice;
    } else if (wasOn && !next.on && next.error === null) {
      this.adminNotice = 'Admin mode expired';
    }
    if (next.on) this.adminActionError = null;
  }

  clearAdminNotice() {
    this.adminNotice = null;
  }
}

export const statusStore = new StatusStore();

/** Read the real admin state. Read-only: it can never prompt. */
export async function refreshAdminModeState(): Promise<void> {
  try {
    statusStore.applyAdminMode(await invoke<BackendAdminModeState>('admin_mode_state'));
  } catch (error) {
    statusStore.applyAdminMode({ on: false, expires_at: null, error: errorText(error) });
  }
}

/**
 * Turn admin mode on (exactly one PolicyKit prompt) or off (revoke the real
 * cached authorisation now). The returned state is applied immediately — the
 * UI never waits for the next poll.
 */
export async function setAdminMode(on: boolean): Promise<void> {
  statusStore.clearAdminNotice();
  statusStore.adminActionError = null;
  try {
    const next = await invoke<BackendAdminModeState>(on ? 'admin_mode_on' : 'admin_mode_off');
    const notice = !on && !next.on && next.error === null ? 'Admin mode off' : undefined;
    statusStore.applyAdminMode(next, notice);
    statusStore.adminActionError = next.error;
  } catch (error) {
    statusStore.applyAdminMode({ on: false, expires_at: null, error: errorText(error) });
    statusStore.adminActionError = errorText(error);
  }
}
