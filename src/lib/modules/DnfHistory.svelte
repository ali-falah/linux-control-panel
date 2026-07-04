<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
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

  type Tab = 'history' | 'packages' | 'maintenance';
  let activeTab = $state<Tab>('history');

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
  });
</script>

<div class="module-page">
  <PageHeader title="DNF Manager" subtitle="Manage packages, view history, and perform maintenance" icon={Package}>
    {#if activeTab === 'history'}
      <button class="btn btn-outline" onclick={loadHistory} disabled={loadingHistory}>
        <RefreshCw size={14} class={loadingHistory ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    {/if}
  </PageHeader>

  <!-- Controls: Tabs & Search -->
  <div style="display:flex; gap:16px; align-items:center; flex-wrap:wrap; margin-bottom: 16px;">
    <div style="display:flex; gap:2px; background:var(--color-bg-raised); padding:4px; border-radius:10px; width:fit-content; margin: 0;">
      {#each [['history','Transaction History'],['packages','Find Packages'],['maintenance','Maintenance']] as [id, label]}
        <button
          class="tab-btn"
          class:active={activeTab === id}
          onclick={() => activeTab = id as Tab}
        >
          {label}
        </button>
      {/each}
    </div>

    {#if activeTab === 'history'}
      <div class="search-bar" style="flex:1; min-width:200px; margin: 0;">
        <Search size={14} style="color:var(--color-text-muted)" />
        <input bind:value={historySearch} placeholder="Search history by command or action…" />
      </div>
    {:else}
      <div style="flex:1"></div>
    {/if}
  </div>

  {#if activeTab === 'history'}
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
          <table>
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
    <div class="card" style="display:flex; flex-direction:column; gap:16px; flex:1; min-height:0">
      <div style="display:flex; gap:10px">
        <div class="search-bar" style="flex:1; margin:0">
          <Search size={14} style="color:var(--color-text-muted)" />
          <input bind:value={pkgQuery} placeholder="Enter package name (e.g. htop)..." onkeydown={(e) => e.key === 'Enter' && runPkgCmd('search')} />
        </div>
        <button class="btn btn-primary" onclick={() => runPkgCmd('search')} disabled={!pkgQuery || pkgLoading}>
          <Search size={14} /> Search
        </button>
        <button class="btn btn-outline" onclick={() => runPkgCmd('info')} disabled={!pkgQuery || pkgLoading}>
          <Info size={14} /> Info
        </button>
        <button class="btn btn-outline" onclick={() => runPkgCmd('versions')} disabled={!pkgQuery || pkgLoading}>
          <ListTree size={14} /> Versions
        </button>
      </div>
      
      <div style="flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden">
        <CodeEditor value={pkgOutput || 'Enter a package name and select an action to view output...'} readonly={true} />
      </div>
    </div>

  {:else if activeTab === 'maintenance'}
    <div class="card module-content-scroll" style="display:flex; flex-direction:column; gap:16px">
      <h3 style="font-size:16px; font-weight:600; margin:0; color:var(--color-text-primary)">System Maintenance</h3>
      <p style="font-size:13px; color:var(--color-text-secondary); margin:0">Run cleanup tasks to free up disk space or fix repository issues.</p>
      
      <div style="display:flex; flex-direction:column; gap:12px">
        <!-- Clear Cache -->
        <div style="display:flex; gap:16px; align-items:flex-start; padding:16px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px">
          <div style="padding:10px; background:rgba(255, 71, 87, 0.1); border-radius:8px">
            <Trash2 size={24} style="color:var(--color-danger)" />
          </div>
          <div style="flex:1">
            <div style="font-weight:600; margin-bottom:4px; color:var(--color-text-primary)">Clear DNF Cache</div>
            <div style="font-size:13px; color:var(--color-text-secondary); margin-bottom:12px">Removes all cached repository data and downloaded packages. Resolves "metadata doesn't match" errors.</div>
            <button class="btn btn-outline btn-danger" onclick={() => confirmMaintenance('dnf clean all', 'dnf_clean_all', 'Are you sure you want to clear the DNF cache?\n\nThis will remove all cached repository data and downloaded packages. A polkit password prompt will appear.')} disabled={maintRunning}>
              <Trash2 size={14} /> Run dnf clean all
            </button>
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
            <button class="btn btn-outline btn-warning" onclick={() => confirmMaintenance('dnf autoremove', 'dnf_autoremove', 'Are you sure you want to autoremove unused packages?\n\nWARNING: This can sometimes remove critical system packages if dependencies were mismanaged. Please review the output carefully. A polkit password prompt will appear.')} disabled={maintRunning}>
              <Package size={14} /> Run dnf autoremove
            </button>
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
            <button class="btn btn-outline" onclick={() => runMaintenance('dnf check', 'dnf_check')} disabled={maintRunning}>
              <CheckCircle size={14} /> Run dnf check
            </button>
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
            <button class="btn btn-outline" onclick={() => runMaintenance('dnf makecache', 'dnf_makecache_cmd')} disabled={maintRunning}>
              <Database size={14} /> Run dnf makecache
            </button>
          </div>
        </div>
      </div>

      {#if maintOutput}
        <div style="margin-top:10px; height: 300px; border:1px solid var(--color-border); border-radius:8px; overflow:hidden">
          <CodeEditor value={maintOutput} readonly={true} />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tab-btn {
    background: transparent;
    border: none;
    color: var(--color-text-secondary);
    padding: 6px 16px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .tab-btn:hover {
    color: var(--color-text-primary);
  }
  .tab-btn.active {
    background: var(--color-bg-base);
    color: var(--color-text-primary);
    box-shadow: 0 2px 4px rgba(0,0,0,0.2);
  }
</style>
