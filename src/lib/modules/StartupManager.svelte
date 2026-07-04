<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { Rocket, RefreshCw, Search, Filter } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  interface SystemdUnit {
    name: string;
    unit_type: string;
    state: string;
    scope: string;
    vendor_preset: string;
  }

  interface AutostartEntry {
    name: string;
    exec: string;
    comment: string;
    enabled: boolean;
    file_path: string;
    icon: string | null;
  }

  type ViewFilter = 'all' | 'service' | 'autostart';

  let systemdUnits = $state<SystemdUnit[]>([]);
  let autostartEntries = $state<AutostartEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let viewFilter = $state<ViewFilter>('all');
  let togglingId = $state<string | null>(null);

  const filteredSystemd = $derived(
    systemdUnits.filter(u => {
      if (viewFilter === 'autostart') return false;
      const q = filter.toLowerCase();
      return !q || u.name.toLowerCase().includes(q);
    })
  );

  const filteredAutostart = $derived(
    autostartEntries.filter(e => {
      if (viewFilter === 'service') return false;
      const q = filter.toLowerCase();
      return !q || e.name.toLowerCase().includes(q) || e.exec.toLowerCase().includes(q);
    })
  );

  function stateBadge(state: string): string {
    switch (state.toLowerCase()) {
      case 'enabled': return 'badge-success';
      case 'disabled': return 'badge-muted';
      case 'masked': return 'badge-error';
      case 'static': return 'badge-info';
      default: return 'badge-muted';
    }
  }

  async function load() {
    loading = true;
    statusStore.setBusy('Loading startup items…');
    try {
      [systemdUnits, autostartEntries] = await Promise.all([
        invoke<SystemdUnit[]>('list_systemd_units'),
        invoke<AutostartEntry[]>('list_autostart_entries'),
      ]);
      statusStore.setLastCommand('list_systemd_units + list_autostart_entries', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load startup items: ${e}`, 'error');
      statusStore.setLastCommand('list_systemd_units', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function toggleUnit(unit: SystemdUnit) {
    togglingId = unit.name;
    const enable = unit.state !== 'enabled';
    try {
      await invoke('toggle_service_unit', {
        name: unit.name,
        enabled: enable,
        scope: unit.scope,
      });
      unit.state = enable ? 'enabled' : 'disabled';
      systemdUnits = [...systemdUnits];
      uiStore.addToast(
        `${unit.name} ${enable ? 'enabled' : 'disabled'}`,
        'success'
      );
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} ${unit.name}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle service: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl toggle ${unit.name}`, 1, false);
    } finally {
      togglingId = null;
    }
  }

  async function toggleAutostart(entry: AutostartEntry) {
    togglingId = entry.file_path;
    const newEnabled = !entry.enabled;
    try {
      await invoke('toggle_autostart', {
        filePath: entry.file_path,
        enabled: newEnabled,
      });
      entry.enabled = newEnabled;
      autostartEntries = [...autostartEntries];
      uiStore.addToast(
        `Autostart "${entry.name}" ${newEnabled ? 'enabled' : 'disabled'}`,
        'success'
      );
    } catch (e) {
      uiStore.addToast(`Failed to toggle autostart: ${e}`, 'error');
    } finally {
      togglingId = null;
    }
  }

  $effect(() => { load(); });
</script>

<div class="module-page">
  <PageHeader title="Startup Manager" subtitle="Manage systemd services and XDG autostart entries" icon={Rocket}>
    <Button variant="ghost" class="" onclick={load} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  <!-- Controls: Stats, Search & Tabs -->
  <div style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    <div style="display:flex; gap:12px; flex-wrap:wrap; margin: 0; align-items:stretch;">
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-text-primary);line-height:1;">{systemdUnits.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">System Services</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-success);line-height:1;">{systemdUnits.filter(u => u.state === 'enabled').length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Enabled</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-accent);line-height:1;">{autostartEntries.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Autostart Apps</span>
      </div>
    </div>

    <!-- Search -->
    <div class="search-bar" style="flex:1; min-width:200px; margin: 0;">
      <Search size={14} style="color:var(--color-text-muted)" />
      <input bind:value={filter} placeholder="Filter entries…" />
    </div>

    <!-- Tabs -->
    <div class="tab-bar">
      {#each [['all','All'],['service','Services'],['autostart','Autostart']] as [id, label]}
        <button class="tab-btn { viewFilter === id ? 'active' : '' }"
          onclick={() => viewFilter = id as ViewFilter}
        >
          {label}
        </button>
      {/each}
    </div>
  </div>

  <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:24px;">
    <!-- Systemd Units -->
    {#if viewFilter !== 'autostart'}
      <div>
        <h3 class="section-title" style="margin-bottom:10px">
          <span style="color:var(--color-accent)">⬡</span> systemd Service Units
          <span class="badge badge-muted">{filteredSystemd.length}</span>
        </h3>
        <div class="card" style="padding:0">
        {#if loading}
          <div style="padding:24px;display:flex;align-items:center;justify-content:center;gap:8px;color:var(--color-text-muted)">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading…
          </div>
        {:else if filteredSystemd.length === 0}
          <div class="empty-state" style="padding:24px">
            <span>No units match your filter</span>
          </div>
        {:else}
          <div class="table-wrap" style="border:none;border-radius:0">
            <table>
              <thead>
                <tr>
                  <th>Unit Name</th>
                  <th>Scope</th>
                  <th>Preset</th>
                  <th style="text-align:center">State</th>
                  <th style="text-align:center">Toggle</th>
                </tr>
              </thead>
              <tbody>
                {#each filteredSystemd as unit (unit.name + unit.scope)}
                  <tr>
                    <td style="font-family:var(--font-mono);font-size:12px;color:var(--color-text-primary)">{unit.name}</td>
                    <td><span class="badge badge-muted">{unit.scope}</span></td>
                    <td style="color:var(--color-text-muted);font-size:12px">{unit.vendor_preset || '—'}</td>
                    <td style="text-align:center">
                      <span class="badge {stateBadge(unit.state)}">{unit.state}</span>
                    </td>
                    <td style="text-align:center">
                      <button
                        class="ui-toggle"
                        class:on={unit.state === 'enabled'}
                        onclick={() => toggleUnit(unit)}
                        disabled={togglingId === unit.name || unit.state === 'masked' || unit.state === 'static'}
                        title="{unit.state === 'enabled' ? 'Disable' : 'Enable'} unit"
                        aria-checked={unit.state === 'enabled'}
                        role="switch"
                      >
                        <span class="ui-toggle-thumb"></span>
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Autostart Entries -->
  {#if viewFilter !== 'service'}
    <div>
      <h3 class="section-title" style="margin-bottom:10px">
        <span style="color:var(--color-success)">⬡</span> XDG Autostart Entries
        <span class="badge badge-muted">{filteredAutostart.length}</span>
      </h3>
      <div class="card" style="padding:0">
        {#if loading}
          <div style="padding:24px;display:flex;align-items:center;justify-content:center;gap:8px;color:var(--color-text-muted)">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading…
          </div>
        {:else if filteredAutostart.length === 0}
          <div class="empty-state" style="padding:24px">
            <span>No XDG autostart entries in ~/.config/autostart/</span>
          </div>
        {:else}
          <div class="table-wrap" style="border:none;border-radius:0">
            <table>
              <thead>
                <tr>
                  <th>Application</th>
                  <th>Command</th>
                  <th>Comment</th>
                  <th style="text-align:center">Enabled</th>
                </tr>
              </thead>
              <tbody>
                {#each filteredAutostart as entry (entry.file_path)}
                  <tr>
                    <td>
                      <div style="font-weight:500;color:var(--color-text-primary)">{entry.name}</div>
                      <div style="font-size:11px;color:var(--color-text-muted)">{entry.file_path.split('/').pop()}</div>
                    </td>
                    <td><code style="font-size:11px;color:var(--color-text-secondary)">{entry.exec}</code></td>
                    <td style="font-size:12px;color:var(--color-text-muted)">{entry.comment || '—'}</td>
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
            </table>
          </div>
        {/if}
      </div>
    </div>
  {/if}
  </div>
</div>

<style>
  .filter-btn {
    padding: 5px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    font-family: var(--font-sans);
    transition: all 0.15s;
  }
  .filter-btn.active {
    background: var(--color-bg-card);
    color: var(--color-text-primary);
  }
</style>
