<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { History, RefreshCw, Undo2, Calendar, Package, Search, Trash2, Info, ListTree, CheckCircle, Database } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';

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

  type Tab = 'updates' | 'history' | 'packages' | 'maintenance' | 'logs';
  let activeTab = $state<Tab>('updates');

  // --- LOGS STATE ---
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
    if (activeTab === 'logs' && !dnfLogContent) {
      loadDnfLog();
    }
  });

  // --- UPDATES STATE ---
  let updates = $state<DnfUpdateEntry[]>([]);
  let selectedUpdates = $state<Set<string>>(new Set());
  let loadingUpdates = $state(false);
  let upgradeOutput = $state('');
  let isUpgrading = $state(false);
  let upgradeTerminalRef: HTMLElement | null = $state(null);
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenFinished: UnlistenFn | null = null;
  let pendingCr = $state(false);

  let selectAllUpdates = $derived(
    updates.length > 0 && selectedUpdates.size === updates.length
  );

  function parseSize(sizeStr: string): number {
    const units: Record<string, number> = {
      'B': 1, 'K': 1024, 'M': 1024 ** 2, 'G': 1024 ** 3,
      'KiB': 1024, 'MiB': 1024 ** 2, 'GiB': 1024 ** 3, 'TiB': 1024 ** 4,
      'k': 1024, 'm': 1024 ** 2, 'g': 1024 ** 3,
    };
    const match = sizeStr.trim().match(/^([\d.]+)\s*([a-zA-Z]+)?$/);
    if (!match) return 0;
    const val = parseFloat(match[1]);
    const unit = match[2] || 'B';
    const multiplier = units[unit] || 1;
    return val * multiplier;
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

  function toggleSelectAll(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    if (checked) {
      selectedUpdates = new Set(updates.map(u => u.package));
    } else {
      selectedUpdates = new Set();
    }
  }

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

  async function startUpgrade() {
    if (selectedUpdates.size === 0) return;
    const pkgs = Array.from(selectedUpdates);
    
    isUpgrading = true;
    upgradeOutput = 'Starting upgrade process...\n';
    pendingCr = false;
    statusStore.setBusy('Upgrading packages…');

    try {
      unlistenOutput = await listen<string>('dnf-upgrade-output', async (event) => {
        let chunk = event.payload;
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
                if (lastNewline !== -1) {
                  upgradeOutput = upgradeOutput.substring(0, lastNewline + 1);
                } else {
                  upgradeOutput = '';
                }
             }
          }

          if (c === '\r') {
             pendingCr = true;
          } else if (c === '\b') {
             if (upgradeOutput.length > 0 && upgradeOutput[upgradeOutput.length - 1] !== '\n') {
                upgradeOutput = upgradeOutput.slice(0, -1);
             }
          } else {
             upgradeOutput += c;
          }
        }
        await tick();
        if (upgradeTerminalRef) {
          upgradeTerminalRef.scrollTop = upgradeTerminalRef.scrollHeight;
        }
      });

      unlistenFinished = await listen<boolean>('dnf-upgrade-finished', (event) => {
        isUpgrading = false;
        statusStore.clearBusy();
        if (event.payload) {
          uiStore.addToast('Upgrade completed successfully', 'success');
          statusStore.setLastCommand('dnf upgrade -y', 0, true);
          checkUpdates();
        } else {
          uiStore.addToast('Upgrade failed', 'error');
          statusStore.setLastCommand('dnf upgrade -y', 1, false);
        }
        if (unlistenOutput) unlistenOutput();
        if (unlistenFinished) unlistenFinished();
      });

      await invoke('dnf_run_upgrade', { packages: pkgs });
    } catch (e) {
      uiStore.addToast(`Upgrade error: ${e}`, 'error');
      isUpgrading = false;
      statusStore.clearBusy();
      statusStore.setLastCommand('dnf upgrade -y', 1, false);
      if (unlistenOutput) unlistenOutput();
      if (unlistenFinished) unlistenFinished();
    }
  }

  function toggleUpdateSelection(pkg: string) {
    if (selectedUpdates.has(pkg)) {
      selectedUpdates.delete(pkg);
    } else {
      selectedUpdates.add(pkg);
    }
    // trigger reactivity
    selectedUpdates = new Set(selectedUpdates);
  }

  // --- HISTORY STATE ---
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

  // --- PACKAGES STATE ---
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

  // --- MAINTENANCE STATE ---
  let maintRunning = $state(false);
  let maintOutput = $state('');

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

  // Initialize
  $effect(() => {
    loadHistory();
    checkUpdates();
  });
</script>

<div class="module-page">
  <PageHeader title="DNF Manager" subtitle="Manage packages, view history, and perform maintenance" icon={Package}>
    {#if activeTab === 'history'}
      <Button variant="outline" class="btn-sm" onclick={loadHistory} disabled={loadingHistory}>
        <RefreshCw size={13} class={loadingHistory ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    {:else if activeTab === 'updates'}
      <Button variant="outline" class="btn-sm" onclick={checkUpdates} disabled={loadingUpdates}>
        <RefreshCw size={13} class={loadingUpdates ? 'animate-spin-slow' : ''} /> Check
      </Button>
    {:else if activeTab === 'packages'}
      <Button variant="primary" class="btn-sm" onclick={() => runPkgCmd('search')} disabled={!pkgQuery || pkgLoading}>
        <Search size={13} /> Search
      </Button>
      <Button variant="outline" class="btn-sm" onclick={() => runPkgCmd('info')} disabled={!pkgQuery || pkgLoading}>
        <Info size={13} /> Info
      </Button>
      <Button variant="outline" class="btn-sm" onclick={() => runPkgCmd('versions')} disabled={!pkgQuery || pkgLoading}>
        <ListTree size={13} /> Versions
      </Button>
    {:else if activeTab === 'logs'}
      <Button variant="outline" class="btn-sm" onclick={loadDnfLog} disabled={loadingLog}>
        <RefreshCw size={13} class={loadingLog ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    {/if}
  </PageHeader>

  <!-- Controls: Tabs & Actions -->
  <div class="controls-row">
    <div class="tab-bar">
      {#each [['updates', 'Updates'], ['history','Transaction History'],['packages','Find Packages'],['maintenance','Maintenance'],['logs', 'DNF System Logs']] as [id, label]}
        <button class="tab-btn { activeTab === id ? 'active' : '' }"
          onclick={() => activeTab = id as Tab}
        >
          {label}
        </button>
      {/each}
    </div>

    <div class="tab-actions">
      {#if activeTab === 'history'}
        <div class="search-bar" style="margin: 0;">
          <Search size={14} style="color:var(--color-text-muted)" />
          <input bind:value={historySearch} placeholder="Search history by command or action…" />
        </div>
      {:else if activeTab === 'updates'}
        <span style="font-size:13px; color:var(--color-text-secondary); margin-right:8px;">{updates.length} updates available</span>
        {#if updates.length > 0}
          <Button variant="primary" class="btn-sm" onclick={startUpgrade} disabled={selectedUpdates.size === 0}>
            <RefreshCw size={13} /> Update {selectedUpdates.size} Package{selectedUpdates.size !== 1 ? 's' : ''} ({totalSelectedSize})
          </Button>
        {/if}
      {:else if activeTab === 'packages'}
        <div class="search-bar" style="margin:0; width: 250px;">
          <Search size={14} style="color:var(--color-text-muted)" />
          <input bind:value={pkgQuery} placeholder="Enter package name (e.g. htop)..." onkeydown={(e) => e.key === 'Enter' && runPkgCmd('search')} />
        </div>
      {:else if activeTab === 'logs'}
        <span style="font-size:13px; color:var(--color-text-secondary); margin-right:8px;">reads /var/log/dnf.log</span>
      {/if}
    </div>
  </div>

  {#if activeTab === 'updates'}
    <div class="card" style="display:flex; flex-direction:column; padding: 0; flex: 1; min-height: 0;">
      {#if isUpgrading}
        <div style="padding: 16px; display:flex; flex-direction:column; gap:16px; flex:1; min-height: 0;">
          <h3 style="margin:0; font-size:16px; font-weight:600;">Upgrading Packages...</h3>
          <div 
            bind:this={upgradeTerminalRef}
            style="flex:1; background:#000; color:#0f0; font-family:var(--font-mono); font-size:13px; padding:12px; border-radius:8px; overflow-y:auto; white-space:pre-wrap; min-height: 0;"
          >{upgradeOutput}</div>
        </div>
      {:else}

        <div style="flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column;">
          {#if loadingUpdates && updates.length === 0}
          <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;gap:16px;color:var(--color-text-muted)">
            <RefreshCw size={24} class="animate-spin-slow" />
            <span>Checking for updates...</span>
          </div>
        {:else if updates.length === 0}
          <div class="empty-state" style="padding: 64px 32px;">
            <CheckCircle size={32} class="empty-state-icon" style="color:var(--color-success)" />
            <span style="font-size:16px; font-weight:600;">System is up to date</span>
          </div>
          {:else}
            <div class="table-wrap" style="border:none; border-radius:0; flex:1; overflow: visible;">
            <table use:tableFeatures>
              <thead>
                <tr>
                  <th style="width: 40px; text-align:center;">
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
                      <input type="checkbox" checked={selectedUpdates.has(pkg.package)} onclick={(e) => { e.stopPropagation(); toggleUpdateSelection(pkg.package); }} />
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

  {:else if activeTab === 'history'}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingHistory && history.length === 0}
        <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
          <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
            <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          </div>
          <span style="font-weight:500">Loading DNF history…</span>
        </div>
      {:else if filteredHistory.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <History size={32} class="empty-state-icon" style="margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
            No History Found
          </span>
          <span style="color:var(--color-text-muted); margin-top:8px;">
            {historySearch ? 'No history matches your search.' : 'No DNF history found. DNF may not be available.'}
          </span>
        </div>
      {:else}
        <div class="table-wrap" style="border:none; border-radius:0">
          <table use:tableFeatures>
            <thead>
              <tr>
                <th style="width:60px">ID</th>
                <th>Command</th>
                <th>Date & Time</th>
                <th>Action</th>
                <th style="text-align:right">Altered</th>
                <th style="text-align:right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredHistory as entry (entry.id)}
                <tr>
                  <td>
                    <code style="font-size:11px; color:var(--color-text-accent)">#{entry.id}</code>
                  </td>
                  <td>
                    <div style="font-weight:500; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary)">
                      {entry.command || '—'}
                    </div>
                  </td>
                  <td>
                    <div style="display:flex; align-items:center; gap:6px; font-size:12px; color:var(--color-text-secondary)">
                      <Calendar size={12} /> {entry.date}
                    </div>
                  </td>
                  <td>
                    <span class="badge {actionBadge(entry.action)}">{entry.action || 'Unknown'}</span>
                  </td>
                  <td style="text-align:right; font-weight:500; color:var(--color-text-primary)">
                    {entry.altered}
                  </td>
                  <td style="text-align:right">
                    <KebabMenu>
                      <button
                        class="menu-item danger"
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

  {:else if activeTab === 'packages'}
    <div class="card" style="display:flex; flex-direction:column; gap:16px; flex:1; min-height:0;">
      <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden">
        <CodeEditor value={pkgOutput || 'Enter a package name and select an action to view output...'} readonly={true} height="100%" />
      </div>
    </div>

  {:else if activeTab === 'maintenance'}
    <div class="card module-content-scroll" style="display:flex; flex-direction:column; gap:16px">

      <div style="display:flex; flex-direction:column; gap:12px">
        <!-- Clear Cache -->
        <div style="display:flex; gap:16px; align-items:flex-start; padding:16px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px">
          <div style="padding:10px; background:rgba(255, 71, 87, 0.1); border-radius:8px">
            <Trash2 size={24} style="color:var(--color-danger)" />
          </div>
          <div style="flex:1">
            <div style="font-weight:600; margin-bottom:4px; color:var(--color-text-primary)">Clear DNF Cache</div>
            <div style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Removes all cached repository data and downloaded packages. Resolves "metadata doesn't match" errors.</div>
            <Button variant="outline" class=" btn-danger" onclick={() => confirmMaintenance('dnf clean all', 'dnf_clean_all', 'Are you sure you want to clear the DNF cache?\n\nThis will remove all cached repository data and downloaded packages. A polkit password prompt will appear.')} disabled={maintRunning}>
              <Trash2 size={14} /> Run dnf clean all
            </Button>
          </div>
        </div>

        <!-- Autoremove -->
        <div style="display:flex; gap:16px; align-items:flex-start; padding:16px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px">
          <div style="padding:10px; background:rgba(255, 165, 2, 0.1); border-radius:8px">
            <Package size={24} style="color:var(--color-warning)" />
          </div>
          <div style="flex:1">
            <div style="font-weight:600; margin-bottom:4px; color:var(--color-text-primary)">Autoremove Unused Packages</div>
            <div style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Removes packages that were installed as dependencies but are no longer needed by any installed program.</div>
            <Button variant="outline" class=" btn-warning" onclick={() => confirmMaintenance('dnf autoremove', 'dnf_autoremove', 'Are you sure you want to autoremove unused packages?\n\nWARNING: This can sometimes remove critical system packages if dependencies were mismanaged. Please review the output carefully. A polkit password prompt will appear.')} disabled={maintRunning}>
              <Package size={14} /> Run dnf autoremove
            </Button>
          </div>
        </div>

        <!-- Check Health -->
        <div style="display:flex; gap:16px; align-items:flex-start; padding:16px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px">
          <div style="padding:10px; background:rgba(46, 213, 115, 0.1); border-radius:8px">
            <CheckCircle size={24} style="color:var(--color-success)" />
          </div>
          <div style="flex:1">
            <div style="font-weight:600; margin-bottom:4px; color:var(--color-text-primary)">Check System Health</div>
            <div style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Checks the local RPM database and produces information on any problems it discovers (duplicates, broken dependencies).</div>
            <Button variant="outline" class="" onclick={() => runMaintenance('dnf check', 'dnf_check')} disabled={maintRunning}>
              <CheckCircle size={14} /> Run dnf check
            </Button>
          </div>
        </div>

        <!-- Makecache -->
        <div style="display:flex; gap:16px; align-items:flex-start; padding:16px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px">
          <div style="padding:10px; background:rgba(30, 144, 255, 0.1); border-radius:8px">
            <Database size={24} style="color:var(--color-info)" />
          </div>
          <div style="flex:1">
            <div style="font-weight:600; margin-bottom:4px; color:var(--color-text-primary)">Refresh Metadata (Makecache)</div>
            <div style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Forces DNF to connect to repositories and download the latest package lists and metadata.</div>
            <Button variant="outline" class="" onclick={() => runMaintenance('dnf makecache', 'dnf_makecache_cmd')} disabled={maintRunning}>
              <Database size={14} /> Run dnf makecache
            </Button>
          </div>
        </div>
      </div>

      {#if maintOutput}
        <div style="margin-top:10px; display:flex; flex-direction:column; height: 300px; border:1px solid var(--color-border); border-radius:8px; overflow:hidden">
          <CodeEditor value={maintOutput} readonly={true} height="100%" />
        </div>
      {/if}
    </div>

  {:else if activeTab === 'logs'}
    <div class="card" style="display:flex; flex-direction:column; flex:1; min-height:0; padding:0; border:none; background:transparent;">
      <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:10px; overflow:hidden">
        <CodeEditor value={dnfLogContent || 'Loading...'} readonly={true} height="100%" />
      </div>
    </div>
  {/if}
</div>

<style>
  
  
  
</style>
