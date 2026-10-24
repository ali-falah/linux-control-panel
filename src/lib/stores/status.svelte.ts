export interface StatusEntry {
  command: string;
  exitCode: number | null;
  timestamp: Date;
  success: boolean;
}

/** State of the administrator authorisation (PolicyKit). */
export interface AdminAuthState {
  /** The root owned helper is installed and can be authorised. */
  helperInstalled: boolean;
  /** Timestamp (ms) until which the authorisation is valid. */
  authorisedUntil: number | null;
  /** How long PolicyKit keeps an authorisation by default. */
  lifetimeSecs: number;
}

class StatusStore {
  lastEntry = $state<StatusEntry | null>(null);
  busy = $state(false);
  busyLabel = $state('');

  /** Administrator rights — shown in the status bar. */
  adminAuth = $state<AdminAuthState>({
    helperInstalled: false,
    authorisedUntil: null,
    lifetimeSecs: 300
  });

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

  /** True while the administrator authorisation is still valid. */
  get hasAdminRights(): boolean {
    return this.adminRightsRemaining !== null;
  }

  /** Seconds the administrator authorisation is still valid for. */
  get adminRightsRemaining(): number | null {
    const until = this.adminAuth.authorisedUntil;
    if (until === null) return null;
    const remaining = Math.round((until - Date.now()) / 1000);
    return remaining > 0 ? remaining : null;
  }

  setAdminAuth(state: AdminAuthState) {
    this.adminAuth = state;
  }
}

export const statusStore = new StatusStore();
