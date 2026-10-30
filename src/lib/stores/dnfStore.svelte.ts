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

export interface DnfPackageDiff {
  name: string;
  arch: string;
  old_version: string | null;
  new_version: string;
  repo: string;
  size: string;
  action: string; // "Upgrade" | "Install" | "Remove" | "Downgrade" | "Obsolete"
}

export interface DnfAdvisory {
  id: string;
  kind: string;
  severity: string;
  url: string;
}

export interface DnfPackageAdvisory {
  nevra: string;
  advisories: DnfAdvisory[];
}

/** The result of a finished transaction. */
export interface DnfUpgradeSummary {
  /** Packages that were upgraded, installed or removed. */
  applied: number;
  /** Packages DNF skipped (for example a newer version is already installed). */
  skipped: number;
  /** Error lines DNF reported. */
  errors: string[];
  /** The transaction DNF summarised, when it printed one. */
  upgrading: number;
  installing: number;
  removing: number;
  /** The exit code of the transaction. */
  exitCode: number | null;
  success: boolean;
}

export interface DnfDryRunResult {
  packages: DnfPackageDiff[];
  total_download_size: string;
  total_inbound_size: string;
  disk_space_change: string;
  to_upgrade_count: number;
  to_install_count: number;
  to_remove_count: number;
  to_downgrade_count: number;
  raw_output: string;
}

const HANG_THRESHOLD_MS = 60_000;

/**
 * Make sure the root owned helper is installed.
 *
 * The first time, this asks for the password once to install the helper and
 * its PolicyKit rule. Afterwards the desktop's agent handles the
 * authorisation and keeps it for a few minutes, so the app never sees a
 * password again.
 */
export async function ensureAdminHelper(): Promise<boolean> {
  try {
    const installed = await invoke<boolean>('admin_install_helper');
    await refreshHelperInstalled();
    return installed;
  } catch {
    return false;
  }
}

/** Read whether the root owned helper is installed. */
export async function refreshHelperInstalled(): Promise<void> {
  try {
    statusStore.helperInstalled = await invoke<boolean>('helper_installed');
  } catch {
    // the last known state stands — reading it again is only opportunistic
  }
}

class DnfStore {
  isUpgrading = $state(false);
  upgradeFinished = $state(false);
  upgradeSuccess = $state(false);
  upgradeOutput = $state('');
  packagesBeingUpgraded = $state<string[]>([]);
  /** Advisories per NEVRA — marks security updates and explains why. */
  advisories = $state<Record<string, DnfAdvisory[]>>({});
  /** Are the advisories still being loaded? */
  advisoriesLoading = $state(false);
  /** Summary of the last finished transaction. */
  upgradeSummary = $state<DnfUpgradeSummary | null>(null);
  /** Sizes of the running transaction (from the dry-run preview). */
  upgradeSizes = $state<{ total: string; download: string }>({ total: '', download: '' });
  hangWarning = $state(false);
  lockInfo = $state<DnfLockInfo | null>(null);
  showFloatingDrawer = $state(false);

  // Dry-run Diff Preview State
  isDryRunning = $state(false);
  dryRunResult = $state<DnfDryRunResult | null>(null);
  dryRunError = $state<string | null>(null);
  showDryRunModal = $state(false);
  pendingUpgradePackages = $state<string[]>([]);

  private lastOutputTime = 0;
  private hangCheckInterval: any = null;
  private pendingCr = false;
  private unlistenOutput: UnlistenFn | null = null;
  private unlistenFinished: UnlistenFn | null = null;
  private listenersInitialized = false;

  async initGlobalListeners() {
    if (this.listenersInitialized) return;

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
        this.upgradeSummary = this.buildSummary(this.upgradeOutput, 0, event.payload);
        this.hangWarning = false;
        if (this.hangCheckInterval) {
          clearInterval(this.hangCheckInterval);
          this.hangCheckInterval = null;
        }
        statusStore.clearBusy();

        if (event.payload) {
          uiStore.addToast('DNF upgrade completed successfully', 'success');
          statusStore.setLastCommand('dnf upgrade -y', 0, true);
          void refreshHelperInstalled();
        } else {
          uiStore.addToast('DNF upgrade failed — check terminal logs', 'error');
          statusStore.setLastCommand('dnf upgrade -y', 1, false);
        }
        this.checkLockStatus();
      });

      // only mark as ready when both listeners are registered — otherwise a
      // failed first call would never register them again
      this.listenersInitialized = true;
    } catch (e) {
      this.unlistenOutput?.();
      this.unlistenOutput = null;
      this.unlistenFinished?.();
      this.unlistenFinished = null;
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

  async startUpgrade(
    packages: string[],
    expectedPackages: DnfPackageDiff[],
    sudoPassword?: string
  ) {
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
      await invoke('dnf_run_upgrade', { packages, expectedPackages, sudoPassword: sudoPassword ?? null });
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);

      // The transaction changed between the preview and the apply (the
      // repository metadata was refreshed in between). Instead of aborting,
      // calculate the new transaction and let the user review it.
      if (/transaction changed/i.test(msg)) {
        this.isUpgrading = false;
        this.upgradeFinished = false;
        this.upgradeSuccess = false;
        this.hangWarning = false;
        if (this.hangCheckInterval) {
          clearInterval(this.hangCheckInterval);
          this.hangCheckInterval = null;
        }
        statusStore.clearBusy();
        await this.requestUpgrade(packages);
        uiStore.addToast(
          'The transaction changed while you were reviewing it — please review the updated preview and confirm again.',
          'warning'
        );
        return;
      }

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
    this.upgradeSummary = null;
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

  // ── Transaction summary ───────────────────────────────────────────────────

  /**
   * Summarise a finished transaction from the DNF output.
   *
   * DNF prints the transaction it is about to run ("Upgrading: 12 packages")
   * and reports skipped packages and errors per package, so the numbers can be
   * read straight from the output.
   */
  buildSummary(output: string, exitCode: number | null, success: boolean): DnfUpgradeSummary {
    const lines = output.split('\n');
    const count = (label: string) => {
      for (const line of lines) {
        const match = line.match(new RegExp(`^\\s*${label}:\\s*(\\d+)\\s+packages?`, 'i'));
        if (match) return parseInt(match[1], 10);
      }
      return 0;
    };

    const errors = lines
      .map((line) => line.trim())
      .filter(
        (line) =>
          /^error:/i.test(line) ||
          /^failed to/i.test(line) ||
          /^.*: command failed/i.test(line) ||
          /^problem:/i.test(line)
      )
      .slice(0, 20);

    const skipped = lines.filter((line) => /^\s*skipped:/i.test(line)).length;

    return {
      applied: count('Upgrading') + count('Installing') + count('Removing'),
      skipped,
      errors,
      upgrading: count('Upgrading'),
      installing: count('Installing'),
      removing: count('Removing'),
      exitCode,
      success
    };
  }

  // ── Advisories (updateinfo) ───────────────────────────────────────────────

  /**
   * Load the available advisories.
   *
   * This is a slow call (it reads the updateinfo metadata), so it runs in the
   * background and the list stays usable while it is loading.
   */
  async loadAdvisories() {
    if (this.advisoriesLoading) return;
    this.advisoriesLoading = true;
    try {
      const entries = await invoke<DnfPackageAdvisory[]>('dnf_update_advisories');
      const map: Record<string, DnfAdvisory[]> = {};
      for (const entry of entries ?? []) {
        if (entry.advisories?.length) map[entry.nevra] = entry.advisories;
      }
      this.advisories = map;
    } catch {
      // no advisories available — the list simply shows no security marks
      this.advisories = {};
    } finally {
      this.advisoriesLoading = false;
    }
  }

  /** The advisories that cover an update (matched by its full NEVRA). */
  advisoriesFor(packageName: string, arch: string, version: string): DnfAdvisory[] {
    // the version may carry an epoch ("0:1.2-3.fc44") — the NEVRA in the
    // updateinfo data never does
    const cleanVersion = version.includes(':') ? version.split(':').slice(1).join(':') : version;
    return this.advisories[`${packageName}-${cleanVersion}.${arch}`] ?? [];
  }

  /** Is this update covered by a security advisory? */
  isSecurityUpdate(packageName: string, arch: string, version: string): boolean {
    return this.advisoriesFor(packageName, arch, version).some((a) => a.kind === 'security');
  }

  // ── Dry Run Flow ──────────────────────────────────────────────────────────

  async requestUpgrade(packages: string[] = []) {
    this.pendingUpgradePackages = [...packages];
    this.showDryRunModal = true;
    this.isDryRunning = true;
    this.dryRunError = null;
    this.dryRunResult = null;
    statusStore.setBusy('Calculating DNF upgrade dry-run diff…');

    try {
      const result = await invoke<DnfDryRunResult>('dnf_dry_run_upgrade', { packages });
      this.dryRunResult = result;
      statusStore.clearBusy();
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      this.dryRunError = msg;
      statusStore.clearBusy();
      uiStore.addToast(`Dry-run calculation failed: ${msg}`, 'error');
    } finally {
      this.isDryRunning = false;
    }
  }

  async confirmAndExecuteUpgrade() {
    this.showDryRunModal = false;
    const pkgs = [...this.pendingUpgradePackages];
    const expectedPackages = [...(this.dryRunResult?.packages ?? [])];

    // The administrator password is requested *before* the progress drawer is
    // shown, so the user is asked first and only sees the live progress once
    // the transaction actually starts.
    //
    // When the root owned helper is installed the desktop's PolicyKit agent
    // asks for the password — the app never sees it and PolicyKit keeps the
    // authorisation for a few minutes. Without the helper the app asks for
    // the password itself.
    // Set the helper up once — afterwards the desktop's PolicyKit agent asks
    // for the password and keeps the authorisation for a few minutes.
    if (!statusStore.helperInstalled) {
      await ensureAdminHelper();
    }

    let password: string | undefined;
    if (!statusStore.helperInstalled) {
      const entered = await uiStore.requestAdminPassword(
        'Administrator authentication required',
        'The package transaction needs administrator rights. Enter your password to continue.'
      );
      if (entered === null) {
        this.upgradeOutput += '\n\n✗ Cancelled — no administrator password was entered.\n';
        return;
      }
      password = entered;
    }

    this.upgradeSizes = {
      total: this.dryRunResult?.total_inbound_size || this.dryRunResult?.total_download_size || '',
      download: this.dryRunResult?.total_download_size || ''
    };
    this.openDrawer();
    this.startUpgrade(pkgs, expectedPackages, password);
  }

  closeDryRunModal() {
    this.showDryRunModal = false;
    this.dryRunResult = null;
    this.dryRunError = null;
    this.isDryRunning = false;
    statusStore.clearBusy();
  }
}

export const dnfStore = new DnfStore();
