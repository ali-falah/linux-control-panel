<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Settings, RefreshCw, Search, Play, Square, RotateCcw,
    FileText, ShieldBan, ShieldCheck, Rocket
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';

  // ─── Tab ──────────────────────────────────────────────────────────────────
  type MainTab = 'services' | 'autostart';
  let mainTab = $state<MainTab>('services');

  // ─── Service Manager state ─────────────────────────────────────────────────
  interface ServiceUnit {
    name: string;
    load_state: string;
    active_state: string;
    sub_state: string;
    description: string;
    unit_file_state: string;
  }

  type ServiceAction = 'start' | 'stop' | 'restart' | 'enable' | 'disable' | 'mask' | 'unmask' | 'reload';

  let units = $state<ServiceUnit[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let selectedUnit = $state<ServiceUnit | null>(null);
  let activePanel = $state<'logs' | 'editor' | null>(null);
  let panelOpen = $state(false);
  let logs = $state('');
  let logsLoading = $state(false);
  let unitFileContent = $state('');
  let unitFileLoading = $state(false);
  let actionInProgress = $state<string | null>(null);
  let editedContent = $state('');
  let saving = $state(false);

  const filteredUnits = $derived(
    units.filter(u => {
      const q = filter.toLowerCase();
      return !q || u.name.toLowerCase().includes(q) || u.description.toLowerCase().includes(q);
    })
  );

  let visibleLimit = $state(50);
  const visibleUnits = $derived(filteredUnits.slice(0, visibleLimit));

  $effect(() => { filter; visibleLimit = 50; });

  function handleScroll(e: Event) {
    const target = e.target as HTMLElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight - 200) {
      if (visibleLimit < filteredUnits.length) visibleLimit += 50;
    }
  }

  function activeStateBadge(state: string): string {
    switch (state) {
      case 'active': return 'badge-success';
      case 'failed': return 'badge-error';
      case 'activating': case 'deactivating': return 'badge-warning';
      default: return 'badge-muted';
    }
  }

  async function load() {
    loading = true;
    statusStore.setBusy('Loading service units…');
    try {
      units = await invoke<ServiceUnit[]>('list_all_units', { filter: null });
      statusStore.setLastCommand('systemctl list-units', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load units: ${e}`, 'error');
      statusStore.setLastCommand('systemctl list-units', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function confirmDoAction(unit: ServiceUnit, action: ServiceAction) {
    if (action === 'stop' || action === 'restart') {
      uiStore.confirm(
        `Confirm ${action === 'stop' ? 'Stop' : 'Restart'}`,
        `Are you sure you want to ${action} ${unit.name}?\n\nWARNING: Modifying critical system services can cause system instability or loss of network connectivity.`,
        () => doAction(unit, action),
        true
      );
    } else {
      doAction(unit, action);
    }
  }

  async function doAction(unit: ServiceUnit, action: ServiceAction) {
    actionInProgress = `${unit.name}-${action}`;
    statusStore.setBusy(`${action} ${unit.name}…`);
    try {
      await invoke<string>('unit_action', { name: unit.name, action });
      uiStore.addToast(`${unit.name}: ${action} succeeded`, 'success');
      statusStore.setLastCommand(`systemctl ${action} ${unit.name}`, 0, true);
      await load();
    } catch (e) {
      uiStore.addToast(`${action} failed: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${action} ${unit.name}`, 1, false);
    } finally {
      actionInProgress = null;
      statusStore.clearBusy();
    }
  }

  async function openLogs(unit: ServiceUnit) {
    selectedUnit = unit;
    activePanel = 'logs';
    panelOpen = true;
    logsLoading = true;
    logs = '';
    try {
      logs = await invoke<string>('get_service_logs', { name: unit.name, lines: 100 });
      statusStore.setLastCommand(`journalctl -u ${unit.name} -n 100`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load logs: ${e}`, 'error');
      statusStore.setLastCommand(`journalctl -u ${unit.name} -n 100`, 1, false);
    } finally {
      logsLoading = false;
    }
  }

  async function openEditor(unit: ServiceUnit) {
    selectedUnit = unit;
    activePanel = 'editor';
    panelOpen = true;
    unitFileLoading = true;
    unitFileContent = '';
    try {
      unitFileContent = await invoke<string>('read_unit_file', { name: unit.name });
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl cat ${unit.name}`, 0, true);
    } catch (e) {
      unitFileContent = `# Error reading unit file: ${e}`;
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl cat ${unit.name}`, 1, false);
    } finally {
      unitFileLoading = false;
    }
  }

  function confirmSaveUnitFile() {
    uiStore.confirm(
      'Confirm Save Unit File',
      `Are you sure you want to save changes to ${selectedUnit?.name}?\n\nWARNING: An invalid systemd unit file can prevent your system from booting properly. Please ensure the syntax is correct.`,
      () => saveUnitFile(),
      true
    );
  }

  async function saveUnitFile() {
    if (!selectedUnit) return;
    saving = true;
    try {
      await invoke('write_unit_file', { name: selectedUnit.name, content: editedContent });
      statusStore.setLastCommand(`echo "..." > /etc/systemd/system/${selectedUnit.name} && systemctl daemon-reload`, 0, true);
      uiStore.addToast(`Unit file saved for ${selectedUnit.name}`, 'success');
      unitFileContent = editedContent;
    } catch (e) {
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      statusStore.setLastCommand(`echo "..." > /etc/systemd/system/${selectedUnit.name}`, 1, false);
    } finally {
      saving = false;
    }
  }

  function closePanel() {
    panelOpen = false;
    activePanel = null;
    selectedUnit = null;
  }

  $effect(() => {
    if (!panelOpen) { activePanel = null; selectedUnit = null; }
  });

  // ─── XDG Autostart state ───────────────────────────────────────────────────
  interface AutostartEntry {
    name: string;
    exec: string;
    comment: string;
    enabled: boolean;
    file_path: string;
    icon: string | null;
  }

  let autostartEntries = $state<AutostartEntry[]>([]);
  let autostartLoading = $state(false);
  let autostartFilter = $state('');
  let togglingId = $state<string | null>(null);
  let autostartVisibleLimit = $state(50);

  const filteredAutostart = $derived(
    autostartEntries.filter(e => {
      const q = autostartFilter.toLowerCase();
      return !q || e.name.toLowerCase().includes(q) || e.exec.toLowerCase().includes(q);
    })
  );
  const visibleAutostart = $derived(filteredAutostart.slice(0, autostartVisibleLimit));

  $effect(() => { autostartFilter; autostartVisibleLimit = 50; });

  function handleAutostartScroll(e: Event) {
    const target = e.target as HTMLElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight - 200) {
      if (autostartVisibleLimit < filteredAutostart.length) autostartVisibleLimit += 50;
    }
  }

  async function loadAutostart() {
    autostartLoading = true;
    statusStore.setBusy('Loading XDG autostart entries…');
    try {
      autostartEntries = await invoke<AutostartEntry[]>('list_autostart_entries');
      statusStore.setLastCommand('ls ~/.config/autostart', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load autostart entries: ${e}`, 'error');
      statusStore.setLastCommand('ls ~/.config/autostart', 1, false);
    } finally {
      autostartLoading = false;
      statusStore.clearBusy();
    }
  }

  async function toggleAutostart(entry: AutostartEntry) {
    togglingId = entry.file_path;
    const newEnabled = !entry.enabled;
    try {
      await invoke('toggle_autostart', { filePath: entry.file_path, enabled: newEnabled });
      entry.enabled = newEnabled;
      autostartEntries = [...autostartEntries];
      uiStore.addToast(`Autostart "${entry.name}" ${newEnabled ? 'enabled' : 'disabled'}`, 'success');
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle autostart: ${e}`, 'error');
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 1, false);
    } finally {
      togglingId = null;
    }
  }

  // ─── Init ──────────────────────────────────────────────────────────────────
  $effect(() => { load(); });
  $effect(() => { loadAutostart(); });
</script>

<div class="module-page">
  <PageHeader title="Service Manager" subtitle="Browse, control, and inspect systemd service units" icon={Settings}>
    <Button variant="ghost" onclick={() => mainTab === 'services' ? load() : loadAutostart()}
      disabled={mainTab === 'services' ? loading : autostartLoading}>
      <RefreshCw size={14} class={(mainTab === 'services' ? loading : autostartLoading) ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  <!-- Single header row: tabs | stats | search -->
  <div class="header-row">
    <div class="tab-bar">
      <button class="tab-btn" class:active={mainTab === 'services'} onclick={() => mainTab = 'services'}>
        <Settings size={14} /> Services
        <span class="tab-count">{units.length}</span>
      </button>
      <button class="tab-btn" class:active={mainTab === 'autostart'} onclick={() => mainTab = 'autostart'}>
        <Rocket size={14} /> XDG Autostart
        <span class="tab-count">{autostartEntries.length}</span>
      </button>
    </div>

    {#if mainTab === 'services'}
      <!-- Inline stat chips -->
      <div class="stat-chip">
        <span class="stat-num" style="color:var(--color-success)">{units.filter(u => u.active_state === 'active').length}</span>
        <span class="stat-label">Active</span>
      </div>
      <div class="stat-chip">
        <span class="stat-num" style="color:var(--color-error)">{units.filter(u => u.active_state === 'failed').length}</span>
        <span class="stat-label">Failed</span>
      </div>
      <div class="stat-chip">
        <span class="stat-num">{units.length}</span>
        <span class="stat-label">Total Units</span>
      </div>

      <div class="header-spacer"></div>
      <SearchBar bind:value={filter} placeholder="Filter services by name or description…" style="min-width:220px; max-width:320px; margin:0;" />
    {:else}
      <div class="header-spacer"></div>
      <SearchBar bind:value={autostartFilter} placeholder="Filter autostart entries…" style="min-width:220px; max-width:320px; margin:0;" />
    {/if}
  </div>

  {#if mainTab === 'services'}
    <!-- Side panel: Logs or Editor -->
    <SideDrawer
      bind:isOpen={panelOpen}
      title={activePanel === 'logs' ? `Logs — ${selectedUnit?.name}` : `Unit File — ${selectedUnit?.name}`}
      width="600px"
    >
      {#snippet headerActions()}
        {#if activePanel === 'editor'}
          <Button variant="primary" class="btn-sm" onclick={confirmSaveUnitFile}
            disabled={saving || editedContent === unitFileContent}>
            {saving ? 'Saving…' : 'Save Override'}
          </Button>
        {/if}
      {/snippet}

      {#if activePanel === 'logs'}
        {#if logsLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading logs…
          </div>
        {:else}
          <pre class="log-output">{logs || 'No log output found.'}</pre>
        {/if}
      {:else if activePanel === 'editor'}
        {#if unitFileLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading unit file…
          </div>
        {:else}
          <div style="display:flex; flex-direction:column; height: 100%;">
            <CodeEditor value={unitFileContent} height="100%" onchange={(v) => editedContent = v} />
          </div>
        {/if}
      {/if}
    </SideDrawer>

    <!-- Service List -->
    <div class="card module-content-scroll" style="padding:0" onscroll={handleScroll}>
      {#if loading}
        <div style="padding: 16px; display: flex; flex-direction: column; gap: 8px;">
          <Skeleton height="42px" borderRadius="8px" />
          <Skeleton height="42px" borderRadius="8px" />
          <Skeleton height="42px" borderRadius="8px" />
          <Skeleton height="42px" borderRadius="8px" />
          <Skeleton height="42px" borderRadius="8px" />
        </div>
      {:else if filteredUnits.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <Settings size={32} class="empty-state-icon" style="margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No results found</span>
          <span style="color:var(--color-text-muted); margin-top:8px;">Try adjusting your search criteria.</span>
        </div>
      {:else}
        <Table tableAction={tableFeatures}>
          <thead>
            <tr>
              <th>Service</th>
              <th>State</th>
              <th>Unit File</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleUnits as unit (unit.name)}
              <tr class:selected-unit={selectedUnit?.name === unit.name}>
                <td style="min-width:220px">
                  <div style="font-weight:500;color:var(--color-text-primary);font-family:var(--font-mono);font-size:12px">{unit.name}</div>
                  {#if unit.description}
                    <div style="font-size:11px;color:var(--color-text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:220px">{unit.description}</div>
                  {/if}
                </td>
                <td>
                  <span class="badge {activeStateBadge(unit.active_state)}">{unit.active_state}</span>
                  <div style="font-size:10px;color:var(--color-text-muted);margin-top:2px">{unit.sub_state}</div>
                </td>
                <td>
                  {#if unit.unit_file_state}
                    <span class="badge {unit.unit_file_state === 'enabled' ? 'badge-success' : unit.unit_file_state === 'masked' ? 'badge-error' : 'badge-muted'}">
                      {unit.unit_file_state}
                    </span>
                  {:else}
                    <span class="badge badge-muted">—</span>
                  {/if}
                </td>
                <td style="width: 140px;">
                  <div style="display:flex;gap:8px; align-items:center">
                    {#if unit.active_state !== 'active'}
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'start')} title="Start" disabled={actionInProgress === `${unit.name}-start`}><Play size={14}/></button>
                    {:else}
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'stop')} title="Stop" disabled={actionInProgress === `${unit.name}-stop`}><Square size={14}/></button>
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'restart')} title="Restart" disabled={actionInProgress === `${unit.name}-restart`}><RotateCcw size={14}/></button>
                    {/if}
                    <KebabMenu align="right">
                      <button class="menu-item" onclick={() => confirmDoAction(unit, 'restart')} disabled={!!actionInProgress}>
                        <RotateCcw size={14} /> Restart
                      </button>
                      <button class="menu-item" onclick={() => openLogs(unit)}>
                        <FileText size={14} /> View Logs
                      </button>
                      <button class="menu-item" onclick={() => openEditor(unit)}>
                        <Settings size={14} /> Edit Unit File
                      </button>
                      <div style="height:1px; background:var(--color-border); margin:4px 0;"></div>
                      {#if unit.unit_file_state !== 'enabled'}
                        <button class="menu-item" onclick={() => confirmDoAction(unit, 'enable')}>
                          <ShieldCheck size={14} /> Enable (Autostart)
                        </button>
                      {:else}
                        <button class="menu-item text-error" onclick={() => confirmDoAction(unit, 'disable')}>
                          <ShieldBan size={14} /> Disable (Autostart)
                        </button>
                      {/if}
                    </KebabMenu>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </Table>
      {/if}
    </div>

  <!-- ── XDG Autostart Tab ─────────────────────────────────────────────────── -->
  {:else}
    <div class="table-area">
      {#if autostartLoading}
        <div class="loading-state">
          <RefreshCw size={14} class="animate-spin-slow" /> Loading…
        </div>
      {:else if filteredAutostart.length === 0}
        <div class="empty-state">No XDG autostart entries in ~/.config/autostart/</div>
      {:else}
        <Table tableAction={tableFeatures} onscroll={handleAutostartScroll}>
          <thead>
            <tr>
              <th>Application</th>
              <th>Command</th>
              <th>Comment</th>
              <th style="text-align:center">Enabled</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleAutostart as entry (entry.file_path)}
              <tr>
                <td>
                  <div class="col-name">{entry.name}</div>
                  <div class="col-subtext">{entry.file_path.split('/').pop()}</div>
                </td>
                <td><code class="col-code">{entry.exec}</code></td>
                <td class="col-muted">{entry.comment || '—'}</td>
                <td style="text-align:center">
                  <button
                    class="ui-toggle"
                    class:on={entry.enabled}
                    onclick={() => toggleAutostart(entry)}
                    disabled={togglingId === entry.file_path}
                    title="{entry.enabled ? 'Disable' : 'Enable'} autostart"
                    aria-checked={entry.enabled}
                    role="switch"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </Table>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── Header row (tabs + stats + search) ─────────────────────────────── */
  .header-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }

  .header-spacer {
    flex: 1;
  }



  /* ── Stats ───────────────────────────────────────────────────────────── */
  .stats-row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }

  .stat-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 16px;
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 10px;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
  }

  .stat-num {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1;
  }

  .stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }

  /* ── Autostart tab layout ────────────────────────────────────────────── */
  .table-area {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .loading-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  /* Cell helpers */
  .col-name {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    font-weight: 500;
  }
  .col-muted {
    color: var(--color-text-muted);
    font-size: 12px;
  }
  .col-subtext {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }
  .col-code {
    font-size: 11px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  /* Toggle switch */
  .ui-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    border: none;
    background: rgba(255, 255, 255, 0.12);
    cursor: pointer;
    transition: background 0.2s ease;
    padding: 0;
    flex-shrink: 0;
  }
  .ui-toggle.on { background: var(--color-accent); }
  .ui-toggle:disabled { opacity: 0.35; cursor: not-allowed; }

  .ui-toggle-thumb {
    position: absolute;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.2s ease;
    pointer-events: none;
  }
  .ui-toggle.on .ui-toggle-thumb { transform: translateX(16px); }

  /* Service tab */
  .log-output {
    font-size: 11px;
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    margin: 0;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: rgba(0,0,0,0.2);
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .selected-unit {
    background: var(--color-active-bg) !important;
  }
</style>
