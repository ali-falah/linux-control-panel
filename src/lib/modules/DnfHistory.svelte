<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';

  import { tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import {
    History, RefreshCw, Undo2, Calendar, Package, Search,
    Trash2, Info, ListTree, CheckCircle, Database, XCircle,
    AlertTriangle, Lock, Ban
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  // ─── Types ──────────────────────────────────────────────────────────────────

  interface DnfHistoryEntry {
    id: number;
    command: string;
    date: string;
    action: string;
    altered: number;
  }

  interface DnfUpdateEntry {
    package: string;
    arch: string;
    version: string;
    repo: string;
    size: string;
  }

  interface DnfLockInfo {
    locked: boolean;
    pid: number | null;
    process_name: string | null;
    lock_path: string | null;
  }

  // ─── Tab State ───────────────────────────────────────────────────────────────

  type Tab = 'updates' | 'history' | 'packages' | 'maintenance' | 'logs';
  let activeTab = $state<Tab>('updates');

  // ─── DNF System Logs ─────────────────────────────────────────────────────────

  let dnfLogContent = $state('');
  let loadingLog = $state(false);

  async function loadDnfLog() {
    loadingLog = true;
    dnfLogContent = '';
    statusStore.setBusy('Loading DNF log…');
    try {
      dnfLogContent = await invoke('dnf_read_log');
      statusStore.setLastCommand('cat /var/log/dnf.log', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load DNF log: ${e}`, 'error');
      dnfLogContent = `Error: ${e}`;
      statusStore.setLastCommand('cat /var/log/dnf.log', 1, false);
    } finally {
      loadingLog = false;
      statusStore.clearBusy();
    }
  }

  $effect(() => {
    if (activeTab === 'logs' && !dnfLogContent) loadDnfLog();
  });

  // ─── Updates State ───────────────────────────────────────────────────────────

  let updates = $state<DnfUpdateEntry[]>([]);
  let selectedUpdates = $state<Set<string>>(new Set());
  let loadingUpdates = $state(false);

  // Terminal output — the full accumulated string
  let upgradeOutput = $state('');
  let isUpgrading = $state(false);
  let upgradeFinished = $state(false);
  let upgradeSuccess = $state(false);

  let pendingCr = $state(false);
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenFinished: UnlistenFn | null = null;

  // Hang detector
  let lastOutputTime = $state(0);
  let hangWarning = $state(false);
  let hangCheckInterval: ReturnType<typeof setInterval> | null = null;
  const HANG_THRESHOLD_MS = 60_000; // 60 seconds

  /** Last 40 lines of upgradeOutput — what is shown in the non-scrollable terminal */
  let terminalLines = $derived(() => {
    const lines = upgradeOutput.split('\n');
    return lines.slice(Math.max(0, lines.length - 40)).join('\n');
  });

  let selectAllUpdates = $derived(
    updates.length > 0 && selectedUpdates.size === updates.length
  );

  // ─── Size helpers ─────────────────────────────────────────────────────────────

  function parseSize(sizeStr: string): number {
    const units: Record<string, number> = {
      'B': 1, 'K': 1024, 'M': 1024 ** 2, 'G': 1024 ** 3,
      'KiB': 1024, 'MiB': 1024 ** 2, 'GiB': 1024 ** 3, 'TiB': 1024 ** 4,
      'k': 1024, 'm': 1024 ** 2, 'g': 1024 ** 3,
    };
    const match = sizeStr.trim().match(/^([\d.]+)\s*([a-zA-Z]+)?$/);
    if (!match) return 0;
    return parseFloat(match[1]) * (units[match[2] || 'B'] || 1);
  }

  function formatSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  let totalSelectedSize = $derived(
    formatSize(
      updates
        .filter(u => selectedUpdates.has(u.package))
        .reduce((acc, u) => acc + parseSize(u.size), 0)
    )
  );

  // ─── Selection ───────────────────────────────────────────────────────────────

  function toggleSelectAll(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    selectedUpdates = checked ? new Set(updates.map(u => u.package)) : new Set();
  }

  function toggleUpdateSelection(pkg: string) {
    if (selectedUpdates.has(pkg)) selectedUpdates.delete(pkg);
    else selectedUpdates.add(pkg);
    selectedUpdates = new Set(selectedUpdates);
  }

  // ─── Check Updates ─────────────────────────────────────────────────────────

  async function checkUpdates() {
    loadingUpdates = true;
    statusStore.setBusy('Checking for updates…');
    try {
      updates = await invoke<DnfUpdateEntry[]>('dnf_check_updates');
      statusStore.setLastCommand('dnf check-update', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to check updates: ${e}`, 'error');
      statusStore.setLastCommand('dnf check-update', 1, false);
      updates = [];
    } finally {
      loadingUpdates = false;
      statusStore.clearBusy();
    }
  }

  // ─── Cancel Upgrade ─────────────────────────────────────────────────────────

  async function cancelUpgrade() {
    try {
      await invoke('dnf_cancel_upgrade');
      upgradeOutput += '\n⚠ Upgrade cancelled by user.\n';
      uiStore.addToast('Upgrade cancellation signal sent.', 'warning');
    } catch (e) {
      uiStore.addToast(`Could not cancel: ${e}`, 'error');
    }
  }

  // ─── Start Upgrade ───────────────────────────────────────────────────────────

  async function startUpgrade() {
    if (selectedUpdates.size === 0) return;
    const pkgs = Array.from(selectedUpdates);

    isUpgrading = true;
    upgradeFinished = false;
    upgradeSuccess = false;
    upgradeOutput = 'Starting upgrade…\n';
    pendingCr = false;
    hangWarning = false;
    lastOutputTime = Date.now();
    statusStore.setBusy('Upgrading packages…');

    // Start hang detector
    hangCheckInterval = setInterval(() => {
      if (isUpgrading && Date.now() - lastOutputTime > HANG_THRESHOLD_MS) {
        hangWarning = true;
      }
    }, 5_000);

    try {
      unlistenOutput = await listen<string>('dnf-upgrade-output', async (event) => {
        let chunk = event.payload;
        lastOutputTime = Date.now();
        hangWarning = false;

        // Strip ANSI escape codes
        chunk = chunk.replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '');

        for (let i = 0; i < chunk.length; i++) {
          let c = chunk[i];
          if (pendingCr) {
            pendingCr = false;
            if (c === '\n') {
              upgradeOutput += '\n';
              continue;
            } else {
              const lastNewline = upgradeOutput.lastIndexOf('\n');
              upgradeOutput = lastNewline !== -1
                ? upgradeOutput.substring(0, lastNewline + 1)
                : '';
            }
          }
          if (c === '\r') { pendingCr = true; }
          else if (c === '\b') {
            if (upgradeOutput.length > 0 && upgradeOutput[upgradeOutput.length - 1] !== '\n') {
              upgradeOutput = upgradeOutput.slice(0, -1);
            }
          } else {
            upgradeOutput += c;
          }
        }
        await tick();
      });

      unlistenFinished = await listen<boolean>('dnf-upgrade-finished', (event) => {
        isUpgrading = false;
        upgradeFinished = true;
        upgradeSuccess = event.payload;
        hangWarning = false;
        if (hangCheckInterval) { clearInterval(hangCheckInterval); hangCheckInterval = null; }
        statusStore.clearBusy();
        if (event.payload) {
          uiStore.addToast('Upgrade completed successfully', 'success');
          statusStore.setLastCommand('dnf upgrade -y', 0, true);
          selectedUpdates = new Set();
          checkUpdates();
        } else {
          uiStore.addToast('Upgrade failed — check terminal output', 'error');
          statusStore.setLastCommand('dnf upgrade -y', 1, false);
        }
        if (unlistenOutput) unlistenOutput();
        if (unlistenFinished) unlistenFinished();
      });

      await invoke('dnf_run_upgrade', { packages: pkgs });
    } catch (e) {
      const msg = String(e);
      uiStore.addToast(msg, 'error');
      upgradeOutput += `\n\n✗ Error: ${msg}\n`;
      isUpgrading = false;
      upgradeFinished = true;
      upgradeSuccess = false;
      hangWarning = false;
      if (hangCheckInterval) { clearInterval(hangCheckInterval); hangCheckInterval = null; }
      statusStore.clearBusy();
      statusStore.setLastCommand('dnf upgrade -y', 1, false);
      if (unlistenOutput) unlistenOutput();
      if (unlistenFinished) unlistenFinished();
    }
  }

  function resetUpgradeView() {
    upgradeOutput = '';
    upgradeFinished = false;
    upgradeSuccess = false;
  }

  // ─── History ─────────────────────────────────────────────────────────────────

  let history = $state<DnfHistoryEntry[]>([]);
  let loadingHistory = $state(false);
  let undoingId = $state<number | null>(null);
  let historySearch = $state('');

  const filteredHistory = $derived(
    history.filter(h =>
      h.command.toLowerCase().includes(historySearch.toLowerCase()) ||
      h.action.toLowerCase().includes(historySearch.toLowerCase())
    ).sort((a, b) => b.id - a.id)
  );

  function actionBadge(action: string): string {
    const a = action.toLowerCase();
    if (a.includes('install')) return 'badge-success';
    if (a.includes('remove') || a.includes('erase')) return 'badge-error';
    if (a.includes('update') || a.includes('upgrade')) return 'badge-info';
    if (a.includes('downgrade')) return 'badge-warning';
    return 'badge-muted';
  }

  async function loadHistory() {
    loadingHistory = true;
    statusStore.setBusy('Loading DNF history…');
    try {
      history = await invoke<DnfHistoryEntry[]>('list_dnf_history');
      statusStore.setLastCommand('dnf history list', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load history: ${e}`, 'error');
      statusStore.setLastCommand('dnf history list', 1, false);
    } finally {
      loadingHistory = false;
      statusStore.clearBusy();
    }
  }

  function confirmUndo(entry: DnfHistoryEntry) {
    uiStore.confirm(
      'Rollback Transaction',
      `Are you sure you want to undo transaction #${entry.id}?\n\n"${entry.command || entry.action}" — ${entry.date}\n\nThis will ${entry.action.toLowerCase()} ${entry.altered} package(s). A polkit password prompt will appear.`,
      () => doUndo(entry.id),
      true
    );
  }

  async function doUndo(id: number) {
    undoingId = id;
    statusStore.setBusy(`Undoing transaction #${id}…`);
    try {
      await invoke('undo_transaction', { id });
      uiStore.addToast(`Successfully rolled back transaction #${id}`, 'success');
      statusStore.setLastCommand(`pkexec dnf history undo ${id} -y`, 0, true);
      loadHistory();
    } catch (e) {
      uiStore.addToast(`Rollback failed: ${e}`, 'error');
      statusStore.setLastCommand(`pkexec dnf history undo ${id} -y`, 1, false);
    } finally {
      undoingId = null;
      statusStore.clearBusy();
    }
  }

  // ─── Find Packages ────────────────────────────────────────────────────────────

  let pkgQuery = $state('');
  let pkgOutput = $state('');
  let pkgLoading = $state(false);

  async function runPkgCmd(cmd: string) {
    if (!pkgQuery.trim()) return;
    pkgLoading = true;
    pkgOutput = '';
    statusStore.setBusy(`Running dnf ${cmd}…`);
    try {
      if (cmd === 'search') {
        pkgOutput = await invoke('dnf_search_packages', { query: pkgQuery });
        statusStore.setLastCommand(`dnf search ${pkgQuery}`, 0, true);
      } else if (cmd === 'info') {
        pkgOutput = await invoke('dnf_package_info', { pkg: pkgQuery });
        statusStore.setLastCommand(`dnf info ${pkgQuery}`, 0, true);
      } else if (cmd === 'versions') {
        pkgOutput = await invoke('dnf_list_versions', { pkg: pkgQuery });
        statusStore.setLastCommand(`dnf list --showduplicates ${pkgQuery}`, 0, true);
      }
    } catch (e) {
      pkgOutput = `Error: ${e}`;
      statusStore.setLastCommand(`dnf ${cmd}`, 1, false);
    } finally {
      pkgLoading = false;
      statusStore.clearBusy();
    }
  }

  // ─── Maintenance ─────────────────────────────────────────────────────────────

  let maintRunning = $state(false);
  let maintOutput = $state('');

  // Lock status
  let lockInfo = $state<DnfLockInfo | null>(null);
  let lockLoading = $state(false);
  let lockKillRunning = $state(false);

  async function checkLockStatus() {
    lockLoading = true;
    try {
      lockInfo = await invoke<DnfLockInfo>('dnf_check_lock_status');
    } catch (e) {
      lockInfo = null;
    } finally {
      lockLoading = false;
    }
  }

  async function killLock() {
    uiStore.confirm(
      'Remove Stale DNF Lock',
      'This will remove the stale DNF lock file.\n\nOnly proceed if you are CERTAIN no DNF process is running. Incorrect use may affect the package database.\n\ndnf check will run automatically after removal to verify integrity.',
      async () => {
        lockKillRunning = true;
        try {
          const result = await invoke<string>('dnf_kill_lock');
          uiStore.addToast('Lock removed. Running dnf check…', 'info');
          maintOutput = result;
          // Run integrity check automatically
          const checkOut = await invoke<string>('dnf_check');
          maintOutput += '\n\n--- dnf check output ---\n' + (checkOut || 'No issues found.');
          uiStore.addToast('Database integrity check complete.', 'success');
          await checkLockStatus();
        } catch (e) {
          uiStore.addToast(`Failed to remove lock: ${e}`, 'error');
          maintOutput = `Error: ${e}`;
        } finally {
          lockKillRunning = false;
        }
      },
      true
    );
  }

  function confirmMaintenance(cmdName: string, tauriCmd: string, warning: string) {
    uiStore.confirm(
      `Confirm ${cmdName}`,
      warning,
      () => runMaintenance(cmdName, tauriCmd),
      true
    );
  }

  async function runMaintenance(cmdName: string, tauriCmd: string) {
    maintRunning = true;
    maintOutput = `Running ${cmdName}…\nThis may take a moment.`;
    statusStore.setBusy(`Running ${cmdName}…`);
    try {
      const out = await invoke<string>(tauriCmd);
      uiStore.addToast(`${cmdName} completed successfully`, 'success');
      statusStore.setLastCommand(cmdName, 0, true);
      maintOutput = out || 'Command completed with no output.';
    } catch (e) {
      uiStore.addToast(`${cmdName} failed: ${e}`, 'error');
      statusStore.setLastCommand(cmdName, 1, false);
      maintOutput = `Error: ${e}`;
    } finally {
      maintRunning = false;
      statusStore.clearBusy();
    }
  }

  // ─── Init ────────────────────────────────────────────────────────────────────

  $effect(() => {
    loadHistory();
    checkUpdates();
  });

  $effect(() => {
    if (activeTab === 'maintenance') checkLockStatus();
  });
</script>

<div class="module-page">
  <PageHeader title="DNF Manager" subtitle="Manage packages, view history, and perform maintenance" icon={Package}>
    {#if activeTab === 'history'}
      <Button variant="outline" size="sm" onclick={loadHistory} disabled={loadingHistory}>
        <RefreshCw size={13} class={loadingHistory ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    {:else if activeTab === 'updates'}
      <Button variant="outline" size="sm" onclick={checkUpdates} disabled={loadingUpdates}>
        <RefreshCw size={13} class={loadingUpdates ? 'animate-spin-slow' : ''} /> Check
      </Button>
    {:else if activeTab === 'packages'}
      <Button variant="primary" size="sm" onclick={() => runPkgCmd('search')} disabled={!pkgQuery || pkgLoading}>
        <Search size={13} /> Search
      </Button>
      <Button variant="outline" size="sm" onclick={() => runPkgCmd('info')} disabled={!pkgQuery || pkgLoading}>
        <Info size={13} /> Info
      </Button>
      <Button variant="outline" size="sm" onclick={() => runPkgCmd('versions')} disabled={!pkgQuery || pkgLoading}>
        <ListTree size={13} /> Versions
      </Button>
    {:else if activeTab === 'logs'}
      <Button variant="outline" size="sm" onclick={loadDnfLog} disabled={loadingLog}>
        <RefreshCw size={13} class={loadingLog ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    {:else if activeTab === 'maintenance'}
      <Button variant="outline" size="sm" onclick={checkLockStatus} disabled={lockLoading}>
        <RefreshCw size={13} class={lockLoading ? 'animate-spin-slow' : ''} /> Check Lock
      </Button>
    {/if}
  </PageHeader>

  <!-- Tab bar + actions row -->
  <div class="controls-row">
    <div class="tab-bar">
      {#each [['updates','Updates'],['history','Transaction History'],['packages','Find Packages'],['maintenance','Maintenance'],['logs','DNF System Logs']] as [id, label]}
        <button class="tab-btn {activeTab === id ? 'active' : ''}" onclick={() => activeTab = id as Tab}>
          {label}
        </button>
      {/each}
    </div>

    <div class="tab-actions">
      {#if activeTab === 'history'}
        <SearchBar bind:value={historySearch} placeholder="Search history by command or action…" style="margin: 0;" />
      {:else if activeTab === 'updates'}
        <span style="font-size:13px; color:var(--color-text-secondary);">{updates.length} updates available</span>
        {#if updates.length > 0}
          <Button variant="primary" size="sm" onclick={startUpgrade} disabled={selectedUpdates.size === 0 || isUpgrading}>
            <RefreshCw size={13} class={isUpgrading ? 'animate-spin-slow' : ''} />
            Update {selectedUpdates.size} Package{selectedUpdates.size !== 1 ? 's' : ''} ({totalSelectedSize})
          </Button>
        {/if}
      {:else if activeTab === 'packages'}
        <SearchBar bind:value={pkgQuery} placeholder="Enter package name (e.g. htop)..." style="margin:0; width: 250px;" />
      {:else if activeTab === 'logs'}
        <span style="font-size:13px; color:var(--color-text-secondary);">reads /var/log/dnf.log</span>
      {/if}
    </div>
  </div>

  <!-- ── Updates Tab ─────────────────────────────────────────────────────────── -->
  {#if activeTab === 'updates'}
    <div class="card" style="display:flex; flex-direction:column; padding: 0; flex: 1; min-height: 0;">

      {#if isUpgrading || upgradeFinished}
        <!-- Terminal view (non-scrollable during upgrade) -->
        <div style="padding: 16px; display:flex; flex-direction:column; gap:12px; flex:1; min-height:0;">
          <div style="display:flex; align-items:center; justify-content:space-between; gap:12px;">
            <h3 style="margin:0; font-size:15px; font-weight:600; display:flex; align-items:center; gap:8px;">
              {#if isUpgrading}
                <RefreshCw size={15} class="animate-spin-slow" style="color:var(--color-accent)" />
                Upgrading Packages…
              {:else if upgradeSuccess}
                <CheckCircle size={15} style="color:var(--color-success)" />
                Upgrade Complete
              {:else}
                <XCircle size={15} style="color:var(--color-error)" />
                Upgrade Failed
              {/if}
            </h3>
            <div style="display:flex; align-items:center; gap:8px;">
              {#if isUpgrading}
                <Button variant="outline" size="sm" onclick={cancelUpgrade}>
                  <Ban size={13} /> Cancel
                </Button>
              {:else}
                <Button variant="outline" size="sm" onclick={resetUpgradeView}>
                  Back to Package List
                </Button>
              {/if}
            </div>
          </div>

          <!-- Hang warning banner -->
          {#if hangWarning}
            <div class="hang-warning">
              <AlertTriangle size={14} />
              <span>No output for 60+ seconds — the upgrade may be stuck on a slow repository. You can Cancel and retry, or wait.</span>
            </div>
          {/if}

          <!-- Non-scrollable terminal: always shows last 40 lines -->
          <div class="upgrade-terminal">
            {terminalLines()}
          </div>

          <!-- Post-upgrade: full scrollable log -->
          {#if upgradeFinished}
            <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
              <div style="padding:8px 14px; background:rgba(0,0,0,0.2); border-bottom:1px solid var(--color-border); font-size:11px; font-weight:600; color:var(--color-text-muted); text-transform:uppercase; letter-spacing:0.05em;">
                Full Upgrade Log
              </div>
              <CodeEditor value={upgradeOutput} readonly={true} height="100%" />
            </div>
          {/if}
        </div>

      {:else}
        <!-- Package list view -->
        <div style="flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column;">
          {#if loadingUpdates && updates.length === 0}
            <div style="padding:48px 32px; display:flex; flex-direction:column; align-items:center; gap:16px; color:var(--color-text-muted);">
              <RefreshCw size={24} class="animate-spin-slow" />
              <span>Checking for updates...</span>
            </div>
          {:else if updates.length === 0}
            <div class="empty-state" style="padding: 64px 32px;">
              <CheckCircle size={32} class="empty-state-icon" style="color:var(--color-success)" />
              <span style="font-size:16px; font-weight:600;">System is up to date</span>
            </div>
          {:else}
            <div class="table-wrap" style="border:none; border-radius:0; flex:1; overflow:visible;">
              <table use:tableFeatures>
                <thead>
                  <tr>
                    <th style="width:40px; text-align:center;">
                      <input type="checkbox" checked={selectAllUpdates} onchange={toggleSelectAll} />
                    </th>
                    <th>Package</th>
                    <th>Version</th>
                    <th>Size</th>
                    <th>Arch</th>
                    <th>Repository</th>
                  </tr>
                </thead>
                <tbody>
                  {#each updates as pkg}
                    <tr onclick={() => toggleUpdateSelection(pkg.package)} style="cursor:pointer;">
                      <td style="text-align:center;">
                        <input type="checkbox" checked={selectedUpdates.has(pkg.package)}
                          onclick={(e) => { e.stopPropagation(); toggleUpdateSelection(pkg.package); }} />
                      </td>
                      <td style="font-weight:500;">{pkg.package}</td>
                      <td style="font-family:var(--font-mono); font-size:12px;">{pkg.version}</td>
                      <td style="font-size:12px; font-weight:500;">{pkg.size}</td>
                      <td style="color:var(--color-text-secondary);">{pkg.arch}</td>
                      <td><span class="badge badge-muted">{pkg.repo}</span></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>
      {/if}
    </div>

  <!-- ── History Tab ────────────────────────────────────────────────────────── -->
  {:else if activeTab === 'history'}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingHistory && history.length === 0}
        <div style="padding:48px 32px; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:16px; color:var(--color-text-muted);">
          <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          <span style="font-weight:500">Loading DNF history…</span>
        </div>
      {:else if filteredHistory.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <History size={32} class="empty-state-icon" style="margin:0 auto 16px;" />
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No History Found</span>
          <span style="color:var(--color-text-muted); margin-top:8px;">
            {historySearch ? 'No history matches your search.' : 'No DNF history found.'}
          </span>
        </div>
      {:else}
        <div class="table-wrap" style="border:none; border-radius:0">
          <table use:tableFeatures>
            <thead>
              <tr>
                <th style="width:60px">ID</th>
                <th>Command</th>
                <th>Date &amp; Time</th>
                <th>Action</th>
                <th style="text-align:right">Altered</th>
                <th style="text-align:right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredHistory as entry (entry.id)}
                <tr>
                  <td><code style="font-size:11px; color:var(--color-text-accent)">#{entry.id}</code></td>
                  <td><div style="font-weight:500; font-family:var(--font-mono); font-size:12px;">{entry.command || '—'}</div></td>
                  <td>
                    <div style="display:flex; align-items:center; gap:6px; font-size:12px; color:var(--color-text-secondary);">
                      <Calendar size={12} /> {entry.date}
                    </div>
                  </td>
                  <td><span class="badge {actionBadge(entry.action)}">{entry.action || 'Unknown'}</span></td>
                  <td style="text-align:right; font-weight:500;">{entry.altered}</td>
                  <td style="text-align:right">
                    <KebabMenu>
                      <button class="menu-item danger"
                        onclick={(e) => { e.stopPropagation(); confirmUndo(entry); }}
                        disabled={undoingId === entry.id}
                      >
                        {#if undoingId === entry.id}
                          <RefreshCw size={14} class="animate-spin" /> Rolling back…
                        {:else}
                          <Undo2 size={14} /> Undo Transaction
                        {/if}
                      </button>
                    </KebabMenu>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

  <!-- ── Find Packages Tab ──────────────────────────────────────────────────── -->
  {:else if activeTab === 'packages'}
    <div class="card" style="display:flex; flex-direction:column; gap:16px; flex:1; min-height:0;">
      <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
        <CodeEditor value={pkgOutput || 'Enter a package name and select an action to view output...'} readonly={true} height="100%" />
      </div>
    </div>

  <!-- ── Maintenance Tab ────────────────────────────────────────────────────── -->
  {:else if activeTab === 'maintenance'}
    <div class="card module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">

      <!-- DNF Lock Status card -->
      <div class="maint-card {lockInfo?.locked ? 'maint-card-danger' : lockInfo?.lock_path ? 'maint-card-warning' : 'maint-card-neutral'}">
        <div class="maint-card-icon {lockInfo?.locked ? 'icon-danger' : lockInfo?.lock_path ? 'icon-warning' : 'icon-success'}">
          <Lock size={22} />
        </div>
        <div class="maint-card-body">
          <div class="maint-card-title">DNF Lock Status</div>
          {#if lockLoading}
            <div class="maint-card-desc">Checking lock status…</div>
          {:else if lockInfo?.locked}
            <div class="maint-card-desc" style="color:var(--color-error);">
              ⚠ DNF is locked by <strong>{lockInfo.process_name}</strong> (PID {lockInfo.pid})
              at <code style="font-size:11px">{lockInfo.lock_path}</code>
            </div>
            <div class="maint-card-desc">Another package operation is running. Wait for it to complete.</div>
          {:else if lockInfo?.lock_path}
            <div class="maint-card-desc" style="color:var(--color-warning);">
              Stale lock file found at <code style="font-size:11px">{lockInfo.lock_path}</code> — owning process is no longer running.
            </div>
            <Button variant="outline" size="sm" class="btn-warning" onclick={killLock} disabled={lockKillRunning || !lockInfo}>
              {#if lockKillRunning}
                <RefreshCw size={13} class="animate-spin-slow" /> Removing…
              {:else}
                <Trash2 size={13} /> Remove Stale Lock
              {/if}
            </Button>
          {:else}
            <div class="maint-card-desc">No lock — DNF is free to run.</div>
          {/if}
        </div>
      </div>

      <!-- Maintenance actions -->
      <div style="display:flex; flex-direction:column; gap:12px;">

        <!-- Clear Cache -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-danger">
            <Trash2 size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Clear DNF Cache</div>
            <div class="maint-card-desc">Removes all cached repository data and downloaded packages. Resolves "metadata doesn't match" errors.</div>
            <Button variant="outline" size="sm" class="btn-danger"
              onclick={() => confirmMaintenance('dnf clean all', 'dnf_clean_all', 'Are you sure you want to clear the DNF cache?\n\nThis will remove all cached repository data and downloaded packages. A polkit password prompt will appear.')}
              disabled={maintRunning}>
              <Trash2 size={13} /> Run dnf clean all
            </Button>
          </div>
        </div>

        <!-- Autoremove -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-warning">
            <Package size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Autoremove Unused Packages</div>
            <div class="maint-card-desc">Removes packages installed as dependencies but no longer needed. Review the output carefully before confirming.</div>
            <Button variant="outline" size="sm" class="btn-warning"
              onclick={() => confirmMaintenance('dnf autoremove', 'dnf_autoremove', 'Are you sure you want to autoremove unused packages?\n\nWARNING: This can sometimes remove critical packages if dependencies were mismanaged. Review the output carefully. A polkit password prompt will appear.')}
              disabled={maintRunning}>
              <Package size={13} /> Run dnf autoremove
            </Button>
          </div>
        </div>

        <!-- Check Health -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-success">
            <CheckCircle size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Check System Health</div>
            <div class="maint-card-desc">Checks the local RPM database for problems — duplicates, broken dependencies, and missing files.</div>
            <Button variant="outline" size="sm" onclick={() => runMaintenance('dnf check', 'dnf_check')} disabled={maintRunning}>
              <CheckCircle size={13} /> Run dnf check
            </Button>
          </div>
        </div>

        <!-- Makecache -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-info">
            <Database size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Refresh Metadata (Makecache)</div>
            <div class="maint-card-desc">Forces DNF to connect to repositories and download the latest package lists and metadata.</div>
            <Button variant="outline" size="sm" onclick={() => runMaintenance('dnf makecache', 'dnf_makecache_cmd')} disabled={maintRunning}>
              <Database size={13} /> Run dnf makecache
            </Button>
          </div>
        </div>

      </div>

      {#if maintOutput}
        <div style="display:flex; flex-direction:column; height:280px; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
          <CodeEditor value={maintOutput} readonly={true} height="100%" />
        </div>
      {/if}
    </div>

  <!-- ── DNF Logs Tab ───────────────────────────────────────────────────────── -->
  {:else if activeTab === 'logs'}
    <div class="card" style="display:flex; flex-direction:column; flex:1; min-height:0; padding:0; border:none; background:transparent;">
      <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:10px; overflow:hidden;">
        <CodeEditor value={dnfLogContent || 'Loading...'} readonly={true} height="100%" />
      </div>
    </div>
  {/if}
</div>

<style>
  /* ── Non-scrollable upgrade terminal ─────────────────────────────────── */
  .upgrade-terminal {
    background: #0d0f14;
    color: #00ff88;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    padding: 14px 16px;
    border-radius: 8px;
    overflow: hidden;         /* NON-SCROLLABLE: intentional */
    white-space: pre-wrap;
    word-break: break-all;
    min-height: 220px;
    max-height: 340px;
    border: 1px solid rgba(0, 255, 136, 0.1);
    box-shadow: inset 0 2px 12px rgba(0, 0, 0, 0.4);
    flex-shrink: 0;
  }

  /* ── Hang warning banner ─────────────────────────────────────────────── */
  .hang-warning {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 14px;
    background: rgba(253, 203, 110, 0.1);
    border: 1px solid rgba(253, 203, 110, 0.25);
    border-radius: 8px;
    color: var(--color-warning);
    font-size: 12px;
    line-height: 1.5;
    flex-shrink: 0;
  }

  /* ── Maintenance cards ───────────────────────────────────────────────── */
  .maint-card {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    padding: 16px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-base);
    transition: border-color 0.15s ease;
  }
  .maint-card-neutral { border-color: var(--color-border); }
  .maint-card-danger  { border-color: rgba(239, 68, 68, 0.3); background: rgba(239, 68, 68, 0.04); }
  .maint-card-warning { border-color: rgba(253, 203, 110, 0.3); background: rgba(253, 203, 110, 0.04); }

  .maint-card-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .icon-danger  { background: rgba(239, 68, 68, 0.12);   color: var(--color-error); }
  .icon-warning { background: rgba(253, 203, 110, 0.12); color: var(--color-warning); }
  .icon-success { background: rgba(0, 210, 211, 0.12);   color: var(--color-success); }
  .icon-info    { background: rgba(9, 132, 227, 0.12);   color: var(--color-info); }

  .maint-card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .maint-card-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text-primary);
  }
  .maint-card-desc {
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin-bottom: 4px;
  }

  /* ── Empty state ──────────────────────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    color: var(--color-text-muted);
  }
</style>
