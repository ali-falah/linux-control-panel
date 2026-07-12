$PATH Entries<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
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

  type ViewFilter = 'service' | 'autostart';

  let systemdUnits = $state<SystemdUnit[]>([]);
  let autostartEntries = $state<AutostartEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let viewFilter = $state<ViewFilter>('service');
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

  let visibleLimit = $state(50);
  const visibleSystemd = $derived(filteredSystemd.slice(0, visibleLimit));
  const visibleAutostart = $derived(filteredAutostart.slice(0, visibleLimit));

  $effect(() => {
    filter;
    viewFilter;
    visibleLimit = 50;
  });

  function handleScroll(e: Event) {
    const target = e.target as HTMLElement;
    if (target.scrollTop + target.clientHeight >= target.scrollHeight - 200) {
      if (viewFilter === 'service' && visibleLimit < filteredSystemd.length) {
        visibleLimit += 50;
      } else if (viewFilter === 'autostart' && visibleLimit < filteredAutostart.length) {
        visibleLimit += 50;
      }
    }
  }

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
      statusStore.setLastCommand('systemctl list-unit-files; ls ~/.config/autostart', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load startup items: ${e}`, 'error');
      statusStore.setLastCommand('systemctl list-unit-files; ls ~/.config/autostart', 1, false);
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
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} ${unit.name}`, 1, false);
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
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle autostart: ${e}`, 'error');
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 1, false);
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

  <!-- Controls: Tabs & Actions -->
  <div class="controls-row">
    <div class="tab-bar">
      <button class="tab-btn { viewFilter === 'service' ? 'active' : '' }"
        onclick={() => viewFilter = 'service'}
      >
        <span style="color:var(--color-accent)">⬡</span> systemd Services
        <span class="badge badge-muted">{systemdUnits.length}</span>
      </button>
      <button class="tab-btn { viewFilter === 'autostart' ? 'active' : '' }"
        onclick={() => viewFilter = 'autostart'}
      >
        <span style="color:var(--color-success)">⬡</span> XDG Autostart
        <span class="badge badge-muted">{autostartEntries.length}</span>
      </button>
    </div>

    <div class="tab-actions">
      <SearchBar bind:value={filter} placeholder="Filter entries…" style="margin: 0; width: 250px;" />
    </div>
  </div>

  <div style="display:flex; flex-direction:column; flex:1; min-height:0;">
    <!-- Systemd Units -->
    {#if viewFilter === 'service'}
      <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0;">
        {#if loading}
          <div style="padding:24px;display:flex;align-items:center;justify-content:center;gap:8px;color:var(--color-text-muted); flex:1;">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading…
          </div>
        {:else if filteredSystemd.length === 0}
          <div class="empty-state" style="padding:24px; flex:1;">
            <span>No units match your filter</span>
          </div>
        {:else}
          <div class="table-wrap module-content-scroll" style="border:none; border-radius:0; flex:1; min-height:0;" onscroll={handleScroll}>
            <table use:tableFeatures>
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
                {#each visibleSystemd as unit (unit.name + unit.scope)}
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
    {/if}

  <!-- Autostart Entries -->
  {#if viewFilter === 'autostart'}
      <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0;">
        {#if loading}
          <div style="padding:24px;display:flex;align-items:center;justify-content:center;gap:8px;color:var(--color-text-muted); flex:1;">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading…
          </div>
        {:else if filteredAutostart.length === 0}
          <div class="empty-state" style="padding:24px; flex:1;">
            <span>No XDG autostart entries in ~/.config/autostart/</span>
          </div>
        {:else}
          <div class="table-wrap module-content-scroll" style="border:none; border-radius:0; flex:1; min-height:0;" onscroll={handleScroll}>
            <table use:tableFeatures>
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
