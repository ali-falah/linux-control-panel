import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { uiStore } from './ui.svelte.ts';
import { statusStore } from './status.svelte.ts';
import { invokeSafe } from '../utils/ipc.ts';

export interface DnfLockInfo {
  locked: boolean;
  pid: number | null;
  process_name: string | null;
  lock_path: string | null;
}

const HANG_THRESHOLD_MS = 30_000;

class DnfStore {
  isUpgrading = $state(false);
  upgradeFinished = $state(false);
  upgradeSuccess = $state(false);
  upgradeOutput = $state('');
  packagesBeingUpgraded = $state<string[]>([]);
  hangWarning = $state(false);
  lockInfo = $state<DnfLockInfo | null>(null);
  showFloatingDrawer = $state(false);

  private lastOutputTime = 0;
  private hangCheckInterval: any = null;
  private pendingCr = false;
  private unlistenOutput: UnlistenFn | null = null;
  private unlistenFinished: UnlistenFn | null = null;
  private listenersInitialized = false;

  async initGlobalListeners() {
    if (this.listenersInitialized) return;
    this.listenersInitialized = true;

    try {
      this.unlistenOutput = await listen<string>('dnf-upgrade-output', (event) => {
        let chunk = event.payload;
        this.lastOutputTime = Date.now();
        this.hangWarning = false;

        // Strip ANSI escape codes
        chunk = chunk.replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '');

        for (let i = 0; i < chunk.length; i++) {
          const c = chunk[i];
          if (this.pendingCr) {
            this.pendingCr = false;
            if (c === '\n') {
              this.upgradeOutput += '\n';
              continue;
            } else {
              const lastNewline = this.upgradeOutput.lastIndexOf('\n');
              this.upgradeOutput = lastNewline !== -1
                ? this.upgradeOutput.substring(0, lastNewline + 1)
                : '';
            }
          }
          if (c === '\r') {
            this.pendingCr = true;
          } else if (c === '\b') {
            if (this.upgradeOutput.length > 0 && this.upgradeOutput[this.upgradeOutput.length - 1] !== '\n') {
              this.upgradeOutput = this.upgradeOutput.slice(0, -1);
            }
          } else {
            this.upgradeOutput += c;
          }
        }
      });

      this.unlistenFinished = await listen<boolean>('dnf-upgrade-finished', (event) => {
        this.isUpgrading = false;
        this.upgradeFinished = true;
        this.upgradeSuccess = event.payload;
        this.hangWarning = false;
        if (this.hangCheckInterval) {
          clearInterval(this.hangCheckInterval);
          this.hangCheckInterval = null;
        }
        statusStore.clearBusy();

        if (event.payload) {
          uiStore.addToast('DNF upgrade completed successfully', 'success');
          statusStore.setLastCommand('dnf upgrade -y', 0, true);
        } else {
          uiStore.addToast('DNF upgrade failed — check terminal logs', 'error');
          statusStore.setLastCommand('dnf upgrade -y', 1, false);
        }
        this.checkLockStatus();
      });
    } catch (e) {
      console.error('[dnfStore] Failed to setup global IPC listeners:', e);
    }
  }

  async checkLockStatus(): Promise<DnfLockInfo | null> {
    const res = await invokeSafe<DnfLockInfo>('dnf_check_lock_status', {}, { quiet: true });
    if (res) {
      this.lockInfo = res;
    }
    return res;
  }

  async startUpgrade(packages: string[]) {
    if (this.isUpgrading) return;
    this.packagesBeingUpgraded = [...packages];
    this.isUpgrading = true;
    this.upgradeFinished = false;
    this.upgradeSuccess = false;
    this.upgradeOutput = `Starting DNF upgrade for ${packages.length} package(s)…\n`;
    this.pendingCr = false;
    this.hangWarning = false;
    this.lastOutputTime = Date.now();
    statusStore.setBusy('Upgrading packages via DNF…');

    if (this.hangCheckInterval) clearInterval(this.hangCheckInterval);
    this.hangCheckInterval = setInterval(() => {
      if (this.isUpgrading && Date.now() - this.lastOutputTime > HANG_THRESHOLD_MS) {
        this.hangWarning = true;
      }
    }, 5_000);

    try {
      await invoke('dnf_run_upgrade', { packages });
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      uiStore.addToast(msg, 'error');
      this.upgradeOutput += `\n\n✗ Error: ${msg}\n`;
      this.isUpgrading = false;
      this.upgradeFinished = true;
      this.upgradeSuccess = false;
      this.hangWarning = false;
      if (this.hangCheckInterval) {
        clearInterval(this.hangCheckInterval);
        this.hangCheckInterval = null;
      }
      statusStore.clearBusy();
      statusStore.setLastCommand('dnf upgrade -y', 1, false);
    }
  }

  async cancelUpgrade() {
    try {
      await invoke('dnf_cancel_upgrade');
      uiStore.addToast('Sent SIGTERM to cancel DNF upgrade process', 'info');
    } catch (e) {
      uiStore.addToast(`Failed to cancel upgrade: ${e}`, 'error');
    }
  }

  resetUpgradeView() {
    this.upgradeOutput = '';
    this.upgradeFinished = false;
    this.upgradeSuccess = false;
    this.packagesBeingUpgraded = [];
  }

  toggleDrawer() {
    this.showFloatingDrawer = !this.showFloatingDrawer;
  }

  openDrawer() {
    this.showFloatingDrawer = true;
  }

  closeDrawer() {
    this.showFloatingDrawer = false;
  }
}

export const dnfStore = new DnfStore();
