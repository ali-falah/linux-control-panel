<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Settings, RefreshCw, Search, Play, Square, RotateCcw, Eye,
    FileText, ShieldBan, ShieldCheck, ToggleLeft, ChevronDown, X, PlayCircle
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';

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

  $effect(() => {
    filter;
    visibleLimit = 50;
  });

  function handleScroll(e: Event) {
    const target = e.target as HTMLElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight - 200) {
      if (visibleLimit < filteredUnits.length) {
        visibleLimit += 50;
      }
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
      const result = await invoke<string>('unit_action', { name: unit.name, action });
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
      await invoke('write_unit_file', {
        name: selectedUnit.name,
        content: editedContent,
      });
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
    if (!panelOpen) {
      activePanel = null;
      selectedUnit = null;
    }
  });

  $effect(() => { load(); });
</script>

<div class="module-page">
  <PageHeader title="Service Manager" subtitle="Browse, control, and inspect systemd service units" icon={Settings}>
    <Button variant="ghost" class="" onclick={load} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  <!-- Controls: Stats & Search -->
  <div class="service-btns" style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    <div style="display:flex; gap:12px; flex-wrap:wrap; margin: 0; align-items:stretch;">
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-success);line-height:1;">{units.filter(u => u.active_state === 'active').length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Active</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-error);line-height:1;">{units.filter(u => u.active_state === 'failed').length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Failed</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-text-primary);line-height:1;">{units.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Total Units</span>
      </div>
    </div>

    <!-- Search -->
    <SearchBar bind:value={filter} placeholder="Filter services by name or description…" style="flex:1; min-width:200px; margin: 0;" />
  </div>

  <!-- Side panel: Logs or Editor -->
  <SideDrawer 
    bind:isOpen={panelOpen} 
    title={activePanel === 'logs' ? `Logs — ${selectedUnit?.name}` : `Unit File — ${selectedUnit?.name}`}
    width="600px"
  >
    {#snippet headerActions()}
      {#if activePanel === 'editor'}
        <Button
          variant="primary" class=" btn-sm"
          onclick={confirmSaveUnitFile}
          disabled={saving || editedContent === unitFileContent}
        >
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
          <CodeEditor
            value={unitFileContent}
            height="100%"
            onchange={(v) => editedContent = v}
          />
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
        <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
          No results found
        </span>
        <span style="color:var(--color-text-muted); margin-top:8px;">
          Try adjusting your search criteria.
        </span>
      </div>
    {:else}
      <div class="table-wrap" style="border:none;border-radius:0">
        <table use:tableFeatures>
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
              {@const key = `${unit.name}-`}
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
                      <Button
                        class="btn btn-sm -success"
                        onclick={() => doAction(unit, 'start')}
                        disabled={!!actionInProgress}
                        title="Start"
                        style="min-width: 70px; display:flex; justify-content:center;"
                      >
                        <Play size={12} style="margin-right:4px"/> Start
                      </Button>
                    {:else}
                      <Button
                        class="btn btn-sm btn-outline -danger"
                        onclick={() => confirmDoAction(unit, 'stop')}
                        disabled={!!actionInProgress}
                        title="Stop"
                        style="min-width: 70px; display:flex; justify-content:center;"
                      >
                        <Square size={12} style="margin-right:4px"/> Stop
                      </Button>
                    {/if}

                    <KebabMenu>
                      <button
                        class="menu-item"
                        onclick={() => confirmDoAction(unit, 'restart')}
                        disabled={!!actionInProgress}
                      >
                        <RotateCcw size={14} /> Restart
                      </button>
                      <button
                        class="menu-item"
                        onclick={() => openLogs(unit)}
                      >
                        <Eye size={14} /> View Logs
                      </button>
                      <button
                        class="menu-item"
                        onclick={() => openEditor(unit)}
                      >
                        <FileText size={14} /> Edit Unit File
                      </button>
                    </KebabMenu>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<style>
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
