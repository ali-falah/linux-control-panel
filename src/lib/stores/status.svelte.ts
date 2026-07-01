export interface StatusEntry {
  command: string;
  exitCode: number | null;
  timestamp: Date;
  success: boolean;
}

class StatusStore {
  lastEntry = $state<StatusEntry | null>(null);
  busy = $state(false);
  busyLabel = $state('');

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
}

export const statusStore = new StatusStore();
