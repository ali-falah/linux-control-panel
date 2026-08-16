<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Settings, RefreshCw, Search, Play, Square, RotateCcw,
    FileText, ShieldBan, ShieldCheck, ShieldAlert, Rocket, ChevronRight, User, Server, Activity, Network, GitFork, Link2,
    Copy, Edit3, Lock, Unlock
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Card from '../components/ui/Card.svelte';

  // ─── Tab ──────────────────────────────────────────────────────────────────
  type MainTab = 'services' | 'autostart' | 'boot_analyzer';
  let mainTab = $state<MainTab>('services');

  // ─── Service Manager state ─────────────────────────────────────────────────
  interface ServiceUnit {
    name: string;
    load_state: string;
    active_state: string;
    sub_state: string;
    description: string;
    unit_file_state: string;
    is_protected?: boolean;
    protection_level?: string;
    protection_reason?: string;
  }

  type ServiceAction = 'start' | 'stop' | 'restart' | 'enable' | 'disable' | 'mask' | 'unmask' | 'reload';

  let units = $state<ServiceUnit[]>([]);
  let loading = $state(false);
  let filter = $state('');
  
  // Read and consume deep-linked filter from Dashboard synchronously
  let statusFilter = $state<'active' | 'failed' | 'all'>(
    uiStore.serviceFilter === 'failed' ? 'failed' : 'all'
  );
  if (uiStore.serviceFilter === 'failed') {
    uiStore.serviceFilter = null;
  }

  let selectedUnit = $state<ServiceUnit | null>(null);
  let activePanel = $state<'logs' | 'editor' | 'dependencies' | null>(null);
  let panelOpen = $state(false);
  let logs = $state('');
  let logsLoading = $state(false);
  let unitFileContent = $state('');
  let unitFileLoading = $state(false);
  let actionInProgress = $state<string | null>(null);
  let editedContent = $state('');
  let saving = $state(false);

  // Context Menu State for Services
  let contextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    unit: ServiceUnit | null;
  }>({ x: 0, y: 0, show: false, unit: null });

  function handleServiceContextMenu(e: MouseEvent, unit: ServiceUnit) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = {
      x: Math.min(e.clientX, window.innerWidth - 240),
      y: Math.min(e.clientY, window.innerHeight - 340),
      show: true,
      unit
    };
  }

  function closeContextMenu() {
    contextMenu.show = false;
  }

  // Service Dependencies State
  interface UnitDeps {
    requires: string[];
    wants: string[];
    after: string[];
    before: string[];
  }
  let unitDeps = $state<UnitDeps | null>(null);
  let depsLoading = $state(false);

  // System vs User scope
  let userScope = $state(false);

  // Boot Analyzer state
  interface BlameEntry {
    time_ms: number;
    time_str: string;
    name: string;
  }
  let blameEntries = $state<BlameEntry[]>([]);
  let loadingBlame = $state(false);

  const filteredUnits = $derived(
    units.filter(u => {
      if (statusFilter === 'active' && u.active_state !== 'active') return false;
      if (statusFilter === 'failed' && u.active_state !== 'failed') return false;
      const q = filter.toLowerCase();
      return !q || u.name.toLowerCase().includes(q) || u.description.toLowerCase().includes(q);
    })
  );

  let currentPage = $state(1);
  const itemsPerPage = 30;
  const totalPages = $derived(Math.ceil(filteredUnits.length / itemsPerPage) || 1);
  const paginatedUnits = $derived(filteredUnits.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage));

  $effect(() => { filter; statusFilter; currentPage = 1; });

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
    statusStore.setBusy(`Loading ${userScope ? 'user' : 'system'} service units…`);
    try {
      units = await invoke<ServiceUnit[]>('list_all_units', { filter: null, userMode: userScope });
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} list-units`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load units: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} list-units`, 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function confirmDoAction(unit: ServiceUnit, action: ServiceAction) {
    if (!userScope && (action === 'stop' || action === 'restart')) {
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
    
    // Store backup for rollback on error
    const prevActive = unit.active_state;
    const prevSub = unit.sub_state;
    const prevFileState = unit.unit_file_state;

    // Optimistic UI state update
    if (action === 'start') {
      unit.active_state = 'activating';
      unit.sub_state = 'starting';
    } else if (action === 'stop') {
      unit.active_state = 'deactivating';
      unit.sub_state = 'stopping';
    } else if (action === 'restart') {
      unit.active_state = 'activating';
      unit.sub_state = 'restarting';
    } else if (action === 'enable') {
      unit.unit_file_state = 'enabled';
    } else if (action === 'disable') {
      unit.unit_file_state = 'disabled';
    } else if (action === 'mask') {
      unit.unit_file_state = 'masked';
    } else if (action === 'unmask') {
      unit.unit_file_state = 'disabled';
    }

    try {
      await invoke<string>('unit_action', { name: unit.name, action, userMode: userScope });
      uiStore.addToast(`${unit.name}: ${action} succeeded`, 'success');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} ${action} ${unit.name}`, 0, true);
      
      // Fast single-unit sync via get_services_status (~15ms)
      try {
        const statuses = await invoke<Array<{ name: string; active_state: string; sub_state: string; unit_file_state: string }>>(
          'get_services_status', 
          { names: [unit.name], userMode: userScope }
        );
        if (statuses && statuses.length > 0) {
          unit.active_state = statuses[0].active_state || (action === 'start' ? 'active' : 'inactive');
          unit.sub_state = statuses[0].sub_state || (action === 'start' ? 'running' : 'dead');
          if (statuses[0].unit_file_state && statuses[0].unit_file_state !== 'unknown') {
            unit.unit_file_state = statuses[0].unit_file_state;
          }
        } else {
          if (action === 'start' || action === 'restart') {
            unit.active_state = 'active';
            unit.sub_state = 'running';
          } else if (action === 'stop') {
            unit.active_state = 'inactive';
            unit.sub_state = 'dead';
          }
        }
      } catch {
        if (action === 'start' || action === 'restart') {
          unit.active_state = 'active';
          unit.sub_state = 'running';
        } else if (action === 'stop') {
          unit.active_state = 'inactive';
          unit.sub_state = 'dead';
        }
      }
    } catch (e) {
      unit.active_state = prevActive;
      unit.sub_state = prevSub;
      unit.unit_file_state = prevFileState;
      uiStore.addToast(`${action} failed: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} ${action} ${unit.name}`, 1, false);
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
      logs = await invoke<string>('get_service_logs', { name: unit.name, lines: 100, userMode: userScope });
      statusStore.setLastCommand(`journalctl ${userScope ? '--user' : ''} -u ${unit.name} -n 100`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load logs: ${e}`, 'error');
      statusStore.setLastCommand(`journalctl ${userScope ? '--user' : ''} -u ${unit.name} -n 100`, 1, false);
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
      unitFileContent = await invoke<string>('read_unit_file', { name: unit.name, userMode: userScope });
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} cat ${unit.name}`, 0, true);
    } catch (e) {
      unitFileContent = `# Error reading unit file: ${e}`;
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} cat ${unit.name}`, 1, false);
    } finally {
      unitFileLoading = false;
    }
  }

  async function openDependencies(unit: ServiceUnit) {
    selectedUnit = unit;
    activePanel = 'dependencies';
    panelOpen = true;
    depsLoading = true;
    unitDeps = null;
    try {
      unitDeps = await invoke<UnitDeps>('get_unit_dependencies', { name: unit.name, userMode: userScope });
      statusStore.setLastCommand(`systemctl show ${unit.name} --property=Requires,Wants,After,Before`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load dependencies: ${e}`, 'error');
    } finally {
      depsLoading = false;
    }
  }

  function confirmSaveUnitFile() {
    uiStore.confirm(
      'Confirm Save Unit File',
      `Are you sure you want to save changes to ${selectedUnit?.name}?\n\nWARNING: An invalid systemd unit file can prevent your services from starting properly. Please ensure the syntax is correct.`,
      () => saveUnitFile(),
      true
    );
  }

  async function saveUnitFile() {
    if (!selectedUnit) return;
    saving = true;
    try {
      await invoke('write_unit_file', { name: selectedUnit.name, content: editedContent, userMode: userScope });
      const targetPath = userScope ? `~/.config/systemd/user/${selectedUnit.name}` : `/etc/systemd/system/${selectedUnit.name}`;
      statusStore.setLastCommand(`echo "..." > ${targetPath} && systemctl ${userScope ? '--user' : ''} daemon-reload`, 0, true);
      uiStore.addToast(`Unit file saved for ${selectedUnit.name}`, 'success');
      unitFileContent = editedContent;
    } catch (e) {
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      const targetPath = userScope ? `~/.config/systemd/user/${selectedUnit.name}` : `/etc/systemd/system/${selectedUnit.name}`;
      statusStore.setLastCommand(`echo "..." > ${targetPath}`, 1, false);
    } finally {
      saving = false;
    }
  }

  async function loadBlame() {
    loadingBlame = true;
    try {
      blameEntries = await invoke<BlameEntry[]>('get_boot_blame');
      statusStore.setLastCommand('systemd-analyze blame', 0, true);
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Failed to load boot latency: ${e}`, 'error');
      statusStore.setLastCommand('systemd-analyze blame', 1, false);
    } finally {
      loadingBlame = false;
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
  let autostartCurrentPage = $state(1);
  const autostartItemsPerPage = 30;
  const autostartTotalPages = $derived(Math.ceil(filteredAutostart.length / autostartItemsPerPage) || 1);
  const paginatedAutostart = $derived(filteredAutostart.slice((autostartCurrentPage - 1) * autostartItemsPerPage, autostartCurrentPage * autostartItemsPerPage));

  $effect(() => { autostartFilter; autostartCurrentPage = 1; });

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
  $effect(() => {
    if (mainTab === 'services') {
      load();
    } else if (mainTab === 'autostart') {
      loadAutostart();
    } else if (mainTab === 'boot_analyzer') {
      loadBlame();
    }
  });
</script>

<div class="module-page">
  <PageHeader title="Service Manager" subtitle="Browse, control, and inspect systemd service units" icon={Settings}>
    <div style="display:flex; align-items:center; gap:8px;">
      {#if mainTab === 'services'}
        <!-- Single Toggleable Scope Button -->
        <button 
          class="scope-toggle-btn"
          class:active={userScope}
          onclick={() => { userScope = !userScope; load(); }}
          title={userScope ? "Viewing User Services (--user). Click to switch to System Services." : "Viewing System Services. Click to switch to User Services."}
        >
          {#if userScope}
            <User size={12} />
            <span>User Scope</span>
          {:else}
            <Server size={12} />
            <span>System Scope</span>
          {/if}
        </button>
      {/if}

      <div class="header-tab-bar">
        <button class="header-tab-btn" class:active={mainTab === 'services'} onclick={() => mainTab = 'services'}>
          <Settings size={12} /> Services
          <span class="header-tab-count" class:active-count={mainTab === 'services'}>{units.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'autostart'} onclick={() => mainTab = 'autostart'}>
          <Rocket size={12} /> Autostart
          <span class="header-tab-count" class:active-count={mainTab === 'autostart'}>{autostartEntries.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'boot_analyzer'} onclick={() => mainTab = 'boot_analyzer'}>
          <Activity size={12} /> Boot Analyzer
          {#if blameEntries.length > 0}
            <span class="header-tab-count" class:active-count={mainTab === 'boot_analyzer'}>{blameEntries.length}</span>
          {/if}
        </button>
      </div>

      <Button variant="ghost" size="sm" onclick={() => {
        if (mainTab === 'services') load();
        else if (mainTab === 'autostart') loadAutostart();
        else loadBlame();
      }}
        disabled={mainTab === 'services' ? loading : mainTab === 'autostart' ? autostartLoading : loadingBlame}>
        <RefreshCw size={13} class={(mainTab === 'services' ? loading : mainTab === 'autostart' ? autostartLoading : loadingBlame) ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    </div>
  </PageHeader>

  {#if mainTab === 'services'}
    <!-- Filter & search row -->
    <div class="header-row">
      <div class="filter-pills">
        <button 
          class="pill-btn {statusFilter === 'all' ? 'active' : ''}" 
          onclick={() => statusFilter = 'all'}
        >
          All ({units.length})
        </button>
        <button 
          class="pill-btn {statusFilter === 'active' ? 'active' : ''}" 
          onclick={() => statusFilter = 'active'}
        >
          Active ({units.filter(u => u.active_state === 'active').length})
        </button>
        <button 
          class="pill-btn {statusFilter === 'failed' ? 'active' : ''}" 
          onclick={() => statusFilter = 'failed'}
        >
          Failed ({units.filter(u => u.active_state === 'failed').length})
        </button>
      </div>

      <div class="header-spacer"></div>
      <SearchBar 
        bind:value={filter} 
        count={filteredUnits.length} 
        total={units.length} 
        placeholder="Filter services by name or description…" 
        style="min-width:240px; max-width:340px; margin:0;" 
      />
    </div>
  {:else if mainTab === 'autostart'}
    <div class="header-row">
      <div class="header-spacer"></div>
      <SearchBar 
        bind:value={autostartFilter} 
        count={filteredAutostart.length} 
        total={autostartEntries.length} 
        placeholder="Filter autostart entries…" 
        style="min-width:240px; max-width:340px; margin:0;" 
      />
    </div>
  {/if}

  {#if mainTab === 'services'}
    <!-- Side panel: Logs, Editor, or Dependencies -->
    <SideDrawer
      bind:isOpen={panelOpen}
      title={activePanel === 'logs' ? `Logs — ${selectedUnit?.name}` : (activePanel === 'dependencies' ? `Dependencies — ${selectedUnit?.name}` : `Unit File — ${selectedUnit?.name}`)}
      width="640px"
    >
      {#snippet headerActions()}
        {#if activePanel === 'logs' && selectedUnit}
          <Button variant="outline" class="btn-sm" onclick={() => uiStore.jumpToJournalService(selectedUnit!.name)}>
            <FileText size={13} /> Full Journal ↗
          </Button>
        {:else if activePanel === 'editor'}
          <Button variant="primary" class="btn-sm" onclick={confirmSaveUnitFile}
            disabled={saving || editedContent === unitFileContent}>
            {saving ? 'Saving…' : 'Save Override'}
          </Button>
        {/if}
      {/snippet}

      {#if selectedUnit?.is_protected}
        <div class="drawer-protection-alert {selectedUnit.protection_level}">
          {#if selectedUnit.protection_level === 'critical'}
            <ShieldAlert size={18} class="protection-alert-icon critical" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-error); font-size: 12.5px;">Critical Operating System Core</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{selectedUnit.protection_reason || 'This unit is critical for operating system integrity. Masking, disabling, or stopping is strictly locked.'}</span>
            </div>
          {:else}
            <ShieldCheck size={18} class="protection-alert-icon essential" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-accent); font-size: 12.5px;">Protected Infrastructure Service</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{selectedUnit.protection_reason || 'This unit provides essential security, network, or display services. Guarded against accidental masking or disablement.'}</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if activePanel === 'logs'}
        {#if logsLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading logs…
          </div>
        {:else}
          <pre class="log-output">{logs || 'No log output found.'}</pre>
        {/if}
      {:else if activePanel === 'dependencies'}
        {#if depsLoading}
          <div style="padding:24px;color:var(--color-text-muted);display:flex;align-items:center;justify-content:center;gap:8px">
            <RefreshCw size={18} class="animate-spin-slow" /> Querying systemd dependency tree…
          </div>
        {:else if unitDeps}
          <div style="display:flex; flex-direction:column; gap:16px; padding:4px 0;">
            <!-- Requires -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ShieldCheck size={14} style="color:var(--color-error);" /> Requires (Hard Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.requires.length}</span>
              </div>
              {#if unitDeps.requires.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.requires as req}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{req}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Wants -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <Activity size={14} style="color:var(--color-warning);" /> Wants (Weak Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.wants.length}</span>
              </div>
              {#if unitDeps.wants.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.wants as want}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{want}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- After -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-accent);" /> Starts After (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.after.length}</span>
              </div>
              {#if unitDeps.after.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.after as aft}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{aft}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Before -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-success);" /> Starts Before (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.before.length}</span>
              </div>
              {#if unitDeps.before.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.before as bef}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{bef}</code>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
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
    <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden;">
      <div style="flex:1; display:flex; flex-direction:column; min-height:0; overflow:hidden;">
        {#if loading && units.length === 0}
          <div class="cyber-loading-matrix">
            <div class="cyber-scanner-hero">
              <div class="cyber-radar-orb">
                <div class="radar-sweep"></div>
                <Server size={24} class="radar-core-icon" />
              </div>
              <div class="cyber-scan-text">
                <div class="cyber-scan-title">Querying Systemd Unit Registry</div>
                <div class="cyber-scan-sub">Synchronizing service states, unit configurations, and security guardrails…</div>
              </div>
            </div>
            <div class="cyber-skeleton-rows">
              {#each [1, 2, 3, 4, 5, 6, 7] as _idx}
                <div class="cyber-shimmer-row" style="animation-delay: {_idx * 0.12}s">
                  <div class="shimmer-col-name">
                    <div class="shimmer-pill-title"></div>
                    <div class="shimmer-pill-sub"></div>
                  </div>
                  <div class="shimmer-col-badge"></div>
                  <div class="shimmer-col-state"></div>
                  <div class="shimmer-col-actions"></div>
                </div>
              {/each}
            </div>
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
            {#each paginatedUnits as unit (unit.name)}
              <tr class:selected-unit={selectedUnit?.name === unit.name} oncontextmenu={(e) => handleServiceContextMenu(e, unit)}>
                <td style="min-width:240px">
                  <div style="display:flex; align-items:center; gap:6px;">
                    <span style="font-weight:600;color:var(--color-text-primary);font-family:var(--font-mono);font-size:12px">{unit.name}</span>
                    {#if unit.is_protected}
                      {#if unit.protection_level === 'critical'}
                        <span class="protection-badge critical" title={unit.protection_reason || 'Critical operating system core component. Destructive actions are strictly locked.'}>
                          <ShieldAlert size={10} /> Core
                        </span>
                      {:else}
                        <span class="protection-badge essential" title={unit.protection_reason || 'Essential infrastructure service. Guarded against masking or disabling.'}>
                          <ShieldCheck size={10} /> Protected
                        </span>
                      {/if}
                    {/if}
                  </div>
                  {#if unit.description}
                    <div style="font-size:11px;color:var(--color-text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:240px">{unit.description}</div>
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
                    {#if actionInProgress === `${unit.name}-start` || actionInProgress === `${unit.name}-stop` || actionInProgress === `${unit.name}-restart`}
                      <button class="action-btn" disabled title="Processing action…">
                        <RefreshCw size={13} class="animate-spin-slow" style="color:var(--color-accent)" />
                      </button>
                    {:else if unit.active_state !== 'active'}
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'start')} title="Start" disabled={!!actionInProgress}><Play size={14}/></button>
                    {:else}
                      <button 
                        class="action-btn" 
                        onclick={() => confirmDoAction(unit, 'stop')} 
                        title={unit.protection_level === 'critical' ? 'Cannot stop critical system unit' : 'Stop'} 
                        disabled={unit.protection_level === 'critical' || !!actionInProgress}
                        style={unit.protection_level === 'critical' ? 'opacity: 0.4; cursor: not-allowed;' : ''}
                      >
                        <Square size={14}/>
                      </button>
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'restart')} title="Restart" disabled={!!actionInProgress}><RotateCcw size={14}/></button>
                    {/if}
                    <KebabMenu align="right">
                      <button class="menu-item" onclick={() => confirmDoAction(unit, 'restart')} disabled={!!actionInProgress}>
                        <RotateCcw size={14} /> Restart
                      </button>
                      <button class="menu-item" onclick={() => openLogs(unit)}>
                        <FileText size={14} /> View Inline Logs
                      </button>
                      <button class="menu-item" onclick={() => uiStore.jumpToJournalService(unit.name)}>
                        <FileText size={14} /> Open in Journal Logs ↗
                      </button>
                      <button class="menu-item" onclick={() => openDependencies(unit)}>
                        <GitFork size={14} /> Inspect Dependencies
                      </button>
                      <button class="menu-item" onclick={() => openEditor(unit)}>
                        <Settings size={14} /> Edit Unit File
                      </button>
                      <div style="height:1px; background:var(--color-border); margin:4px 0;"></div>
                      {#if unit.unit_file_state !== 'enabled'}
                        <button 
                          class="menu-item" 
                          onclick={() => confirmDoAction(unit, 'enable')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be modified' : ''}
                        >
                          <ShieldCheck size={14} /> Enable (Autostart)
                        </button>
                      {:else}
                        <button 
                          class="menu-item text-error" 
                          onclick={() => confirmDoAction(unit, 'disable')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be disabled' : ''}
                        >
                          <ShieldBan size={14} /> Disable (Autostart)
                        </button>
                      {/if}
                      {#if unit.unit_file_state === 'masked'}
                        <button 
                          class="menu-item" 
                          onclick={() => confirmDoAction(unit, 'unmask')}
                          disabled={!!actionInProgress}
                        >
                          <Unlock size={14} /> Unmask Service
                        </button>
                      {:else}
                        <button 
                          class="menu-item text-error" 
                          onclick={() => confirmDoAction(unit, 'mask')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be masked' : ''}
                        >
                          <Lock size={14} /> Mask Service
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
      {#if !loading && filteredUnits.length > 0 && totalPages > 1}
        <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:12px; border-top:1px solid var(--color-border); flex-shrink:0;">
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === 1} onclick={() => currentPage--}>Previous</Button>
          <span style="font-size:12px; color:var(--color-text-secondary);">Page {currentPage} of {totalPages}</span>
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === totalPages} onclick={() => currentPage++}>Next</Button>
        </div>
      {/if}
    </div>

  <!-- ── XDG Autostart Tab ─────────────────────────────────────────────────── -->
  {:else if mainTab === 'autostart'}
    <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0;">
      <div style="flex:1; overflow-y:auto; min-height:0;">
      {#if autostartLoading}
        <div class="loading-state">
          <RefreshCw size={14} class="animate-spin-slow" /> Loading…
        </div>
      {:else if filteredAutostart.length === 0}
        <div class="empty-state">No XDG autostart entries in ~/.config/autostart/</div>
      {:else}
        <Table tableAction={tableFeatures}>
          <thead>
            <tr>
              <th>Application</th>
              <th>Command</th>
              <th>Comment</th>
              <th style="text-align:center">Enabled</th>
            </tr>
          </thead>
          <tbody>
            {#each paginatedAutostart as entry (entry.file_path)}
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
      {#if !autostartLoading && filteredAutostart.length > 0 && autostartTotalPages > 1}
        <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:12px; border-top:1px solid var(--color-border); flex-shrink:0;">
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={autostartCurrentPage === 1} onclick={() => autostartCurrentPage--}>Previous</Button>
          <span style="font-size:12px; color:var(--color-text-secondary);">Page {autostartCurrentPage} of {autostartTotalPages}</span>
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={autostartCurrentPage === autostartTotalPages} onclick={() => autostartCurrentPage++}>Next</Button>
        </div>
      {/if}
    </div>
  {:else if mainTab === 'boot_analyzer'}
    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">
      {#if loadingBlame}
        <div class="card" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted)">
          <RefreshCw size={24} class="animate-spin-slow" />
        </div>
      {:else if blameEntries.length === 0}
        <div class="card empty-state" style="padding: 64px 32px;">
          <Rocket size={32} class="empty-state-icon" style="margin:0 0 16px;" />
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No boot latency data</span>
          <span style="color:var(--color-text-muted); margin-top:8px;">Ensure systemd is running and supports analysis blame.</span>
        </div>
      {:else}
        <Card title="System Boot Startup Latencies (systemd-analyze blame)" icon={Rocket}>
          <div style="font-size:12px; color:var(--color-text-muted); margin-bottom:16px; line-height:1.5;">
            Below is a ranked list of services causing boot latency, ordered from slowest to fastest. Services starting in more than 2 seconds are flagged for inspection.
          </div>
          
          <Table tableAction={tableFeatures} style="max-height: calc(100vh - 280px); overflow-y:auto; border:none; border-radius:0;">
            <thead>
              <tr>
                <th style="width:140px;">Startup Time</th>
                <th>Service Unit</th>
                <th style="width:120px; text-align:center;">Severity</th>
              </tr>
            </thead>
            <tbody>
              {#each blameEntries as entry}
                <tr>
                  <td>
                    <code style="font-family:var(--font-mono); font-weight:700; color:var(--color-text-primary);">
                      {entry.time_str}
                    </code>
                  </td>
                  <td style="font-family:var(--font-mono); color:var(--color-text-secondary);">{entry.name}</td>
                  <td style="text-align:center;">
                    {#if entry.time_ms > 5000}
                      <span class="badge badge-danger">CRITICAL</span>
                    {:else if entry.time_ms > 2000}
                      <span class="badge badge-warning">SLOW</span>
                    {:else}
                      <span class="badge badge-success">FAST</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </Table>
        </Card>
      {/if}
    </div>
  {/if}
</div>

<svelte:window onclick={closeContextMenu} oncontextmenu={closeContextMenu} />

{#if contextMenu.show && contextMenu.unit}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div 
    class="custom-context-menu" 
    style="position: fixed; left: {contextMenu.x}px; top: {contextMenu.y}px; z-index: 10000; min-width: 240px;"
    onclick={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
        <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 130px;" title={contextMenu.unit.name}>
          {contextMenu.unit.name}
        </span>
        {#if contextMenu.unit.is_protected}
          <span class="protection-badge {contextMenu.unit.protection_level}" style="font-size: 9px; padding: 1px 4px;">
            {contextMenu.unit.protection_level === 'critical' ? 'Core' : 'Protected'}
          </span>
        {/if}
      </div>
      <span class="badge {activeStateBadge(contextMenu.unit.active_state)}" style="font-size: 9.5px; padding: 1px 5px;">
        {contextMenu.unit.active_state}
      </span>
    </div>
    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => { const u = contextMenu.unit!; closeContextMenu(); confirmDoAction(u, 'restart'); }}
      disabled={!!actionInProgress}
    >
      <RotateCcw size={14} style="color: var(--color-warning);" />
      <span>Restart Service</span>
    </button>

    {#if contextMenu.unit.active_state !== 'active'}
      <button 
        type="button"
        class="context-menu-item"
        onclick={() => { const u = contextMenu.unit!; closeContextMenu(); confirmDoAction(u, 'start'); }}
        disabled={!!actionInProgress}
      >
        <Play size={14} style="color: var(--color-success);" />
        <span>Start Service</span>
      </button>
    {:else}
      <button 
        type="button"
        class="context-menu-item text-danger"
        onclick={() => { const u = contextMenu.unit!; closeContextMenu(); confirmDoAction(u, 'stop'); }}
        disabled={contextMenu.unit.protection_level === 'critical' || !!actionInProgress}
        title={contextMenu.unit.protection_level === 'critical' ? 'Cannot stop critical system unit' : ''}
      >
        <Square size={14} style="color: var(--color-error);" />
        <span>{contextMenu.unit.protection_level === 'critical' ? 'Stop Service (Locked)' : 'Stop Service'}</span>
      </button>
    {/if}

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => { 
        const u = contextMenu.unit!; 
        closeContextMenu(); 
        const nextAction = u.unit_file_state === 'enabled' ? 'disable' : 'enable';
        confirmDoAction(u, nextAction); 
      }}
      disabled={contextMenu.unit.is_protected || !!actionInProgress}
      title={contextMenu.unit.is_protected ? 'Protected system service cannot be modified' : ''}
    >
      <ShieldCheck size={14} style="color: var(--color-accent);" />
      <span>{contextMenu.unit.is_protected ? 'Boot Autostart (Locked)' : (contextMenu.unit.unit_file_state === 'enabled' ? 'Disable at Boot' : 'Enable at Boot')}</span>
    </button>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => {
        const u = contextMenu.unit!;
        closeContextMenu();
        uiStore.jumpToJournalService(u.name);
        uiStore.setActiveTab('journal-logs');
      }}
    >
      <Activity size={14} style="color: var(--color-info);" />
      <span>View Service Logs (Journalctl)</span>
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => { const u = contextMenu.unit!; closeContextMenu(); openEditor(u); }}
    >
      <Edit3 size={14} />
      <span>Edit Service File</span>
    </button>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => { const u = contextMenu.unit!; closeContextMenu(); openDependencies(u); }}
    >
      <GitFork size={14} />
      <span>Inspect Dependencies</span>
    </button>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => {
        const u = contextMenu.unit!;
        closeContextMenu();
        const maskAction = u.unit_file_state === 'masked' ? 'unmask' : 'mask';
        confirmDoAction(u, maskAction);
      }}
      disabled={contextMenu.unit.is_protected || !!actionInProgress}
      title={contextMenu.unit.is_protected ? 'Protected system service cannot be masked' : ''}
    >
      {#if contextMenu.unit.unit_file_state === 'masked'}
        <Unlock size={14} style="color: var(--color-success);" />
        <span>Unmask Service</span>
      {:else}
        <Lock size={14} style="color: var(--color-warning);" />
        <span>{contextMenu.unit.is_protected ? 'Mask Service (Locked)' : 'Mask Service Unit'}</span>
      {/if}
    </button>

    <button 
      type="button"
      class="context-menu-item"
      onclick={() => {
        const name = contextMenu.unit!.name;
        navigator.clipboard.writeText(name);
        uiStore.addToast(`Copied unit name: ${name}`, 'info');
        closeContextMenu();
      }}
    >
      <Copy size={14} />
      <span>Copy Unit Name</span>
    </button>
  </div>
{/if}

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

  /* ── Scope Toggle Button ───────────────────────────────────────────── */
  .scope-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    background: var(--color-bg-card, #FFFFFF);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
    transition: all 0.2s ease;
  }
  .scope-toggle-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.08);
  }
  .scope-toggle-btn.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 2px 8px rgba(37, 99, 235, 0.25) !important;
  }

  .stat-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 16px;
    background: var(--color-bg-surface, #FFFFFF);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: 0 2px 8px -1px rgba(0, 0, 0, 0.06), 0 1px 3px rgba(0, 0, 0, 0.04);
    color: var(--color-text-primary);
    font-family: inherit;
    font-size: inherit;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .stat-chip:hover {
    border-color: var(--color-border-hover);
    box-shadow: 0 4px 14px -2px rgba(0, 0, 0, 0.10);
  }

  .stat-chip.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.25) !important;
  }

  .stat-chip.active .stat-num,
  .stat-chip.active .stat-label {
    color: #FFFFFF !important;
    font-weight: 700;
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

  /* Header controls optimization */
  .scope-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .scope-toggle-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
  }
  .scope-toggle-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .header-tab-bar {
    display: inline-flex;
    align-items: center;
    background: var(--color-tab-bar-bg, rgba(0, 0, 0, 0.2));
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }
  .header-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .header-tab-btn:hover {
    color: var(--color-text-primary);
  }
  .header-tab-btn.active {
    background: var(--color-accent);
    color: #FFFFFF;
    font-weight: 600;
  }
  .header-tab-count {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.15);
    color: inherit;
  }
  .header-tab-btn.active .header-tab-count {
    background: rgba(255, 255, 255, 0.25);
    color: #FFFFFF;
  }

  .filter-pills {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    padding: 3px;
    border-radius: 8px;
  }

  :global(html.light-mode) .filter-pills {
    background: #F1F5F9;
    border-color: #E2E8F0;
  }

  .pill-btn {
    border: none;
    background: transparent;
    padding: 4px 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pill-btn:hover {
    color: var(--color-text-primary);
  }

  .pill-btn.active {
    background: var(--color-accent);
    color: #000000;
    font-weight: 700;
  }

  :global(html.light-mode) .pill-btn.active {
    background: #2563EB;
    color: #FFFFFF;
  }

  /* ── Custom Context Menu ────────────────────────────────────────── */
  .custom-context-menu {
    background: var(--color-bg-card, #131b26);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .context-menu-item:hover:not(:disabled) {
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--color-text-primary);
  }

  .context-menu-item:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .context-menu-item.text-danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: var(--color-error);
  }

  /* ── Security Protection Badges & Alerts ───────────────────────────── */
  .protection-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex-shrink: 0;
  }
  .protection-badge.critical {
    background: rgba(239, 68, 68, 0.14);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
  }
  .protection-badge.essential {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.3);
  }

  .drawer-protection-alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 8px;
    margin-bottom: 16px;
  }
  .drawer-protection-alert.critical {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.25);
  }
  .drawer-protection-alert.essential {
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
  }

  .protection-alert-icon.critical {
    color: var(--color-error);
    margin-top: 2px;
    flex-shrink: 0;
  }
  .protection-alert-icon.essential {
    color: var(--color-accent);
    margin-top: 2px;
    flex-shrink: 0;
  }

  .protection-alert-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    line-height: 1.4;
  }

  /* ── Cybernetic Pulse Loading Animation ───────────────────────────── */
  .cyber-loading-matrix {
    display: flex;
    flex-direction: column;
    padding: 16px;
    gap: 14px;
    height: 100%;
    min-height: 380px;
    background: var(--color-bg-base);
  }

  .cyber-scanner-hero {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 8px;
    background: linear-gradient(135deg, rgba(0, 218, 243, 0.07) 0%, rgba(37, 99, 235, 0.04) 100%);
    border: 1px solid rgba(0, 218, 243, 0.22);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.12), inset 0 0 16px rgba(0, 218, 243, 0.03);
  }

  .cyber-radar-orb {
    position: relative;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: rgba(0, 218, 243, 0.08);
    border: 1.5px solid rgba(0, 218, 243, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow: 0 0 14px rgba(0, 218, 243, 0.25);
    overflow: hidden;
  }

  .radar-sweep {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: conic-gradient(from 0deg, transparent 0deg, rgba(0, 218, 243, 0.3) 300deg, rgba(0, 218, 243, 0.75) 360deg);
    animation: radar-rotate 1.8s linear infinite;
  }

  @keyframes radar-rotate {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .radar-core-icon {
    position: relative;
    z-index: 2;
    color: var(--color-accent);
    filter: drop-shadow(0 0 4px var(--color-accent));
  }

  .cyber-scan-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .cyber-scan-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }

  .cyber-scan-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .cyber-skeleton-rows {
    display: flex;
    flex-direction: column;
    gap: 7px;
    flex: 1;
  }

  .cyber-shimmer-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-radius: 6px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    position: relative;
    overflow: hidden;
    gap: 14px;
  }

  .cyber-shimmer-row::after {
    content: '';
    position: absolute;
    top: 0;
    left: -150%;
    width: 150%;
    height: 100%;
    background: linear-gradient(90deg, transparent 0%, rgba(0, 218, 243, 0.09) 50%, transparent 100%);
    animation: cyber-shimmer 1.6s infinite ease-in-out;
  }

  @keyframes cyber-shimmer {
    0% { transform: translateX(0); }
    100% { transform: translateX(200%); }
  }

  .shimmer-col-name {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
  }

  .shimmer-pill-title {
    width: 140px;
    height: 13px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }

  .shimmer-pill-sub {
    width: 220px;
    height: 9px;
    border-radius: 3px;
    background: var(--color-bg-hover);
  }

  .shimmer-col-badge {
    width: 54px;
    height: 18px;
    border-radius: 999px;
    background: var(--color-bg-raised);
  }

  .shimmer-col-state {
    width: 50px;
    height: 16px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }

  .shimmer-col-actions {
    width: 65px;
    height: 24px;
    border-radius: 6px;
    background: var(--color-bg-raised);
  }
</style>
