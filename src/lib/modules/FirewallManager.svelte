<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { ShieldAlert, Shield, ShieldCheck, Power, RefreshCw, Trash2, Plus } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  interface FirewallState {
    is_running: boolean;
    is_panic: boolean;
    active_zones: string[];
    default_zone: string;
  }

  interface ZoneRules {
    zone: string;
    services: string[];
    ports: string[];
  }

  let state = $state<FirewallState | null>(null);
  let loading = $state(true);
  
  let activeZone = $state<string>('');
  let rules = $state<ZoneRules | null>(null);
  let loadingRules = $state(false);

  let newPort = $state('');
  let newService = $state('');

  async function loadState() {
    loading = true;
    statusStore.setBusy('Loading firewalld state…');
    try {
      state = await invoke<FirewallState>('get_firewall_state');
      statusStore.setLastCommand('firewall-cmd --state', 0, true);
      
      if (state.is_running && state.active_zones.length > 0) {
        if (!activeZone || !state.active_zones.includes(activeZone)) {
          activeZone = state.default_zone || state.active_zones[0];
        }
        await loadRules(activeZone);
      }
    } catch (e) {
      uiStore.addToast(`Failed to load firewall state: ${e}`, 'error');
      statusStore.setLastCommand('firewall-cmd', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function loadRules(zone: string) {
    loadingRules = true;
    try {
      rules = await invoke<ZoneRules>('get_zone_rules', { zone });
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load rules for zone ${zone}: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 1, false);
    } finally {
      loadingRules = false;
    }
  }

  function confirmTogglePanic() {
    const enable = !state?.is_panic;
    uiStore.confirm(
      enable ? 'Enable Panic Mode' : 'Disable Panic Mode',
      enable 
        ? 'WARNING: Enabling Panic Mode will immediately drop all incoming and outgoing network connections! You will lose all connectivity.'
        : 'Are you sure you want to disable Panic Mode and restore normal firewall rules?',
      () => doTogglePanic(enable),
      true
    );
  }

  async function doTogglePanic(enable: boolean) {
    statusStore.setBusy(`${enable ? 'Enabling' : 'Disabling'} panic mode…`);
    try {
      await invoke('toggle_panic_mode', { enable });
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 0, true);
      uiStore.addToast(`Panic mode ${enable ? 'enabled' : 'disabled'}`, 'success');
      await loadState();
    } catch (e) {
      uiStore.addToast(`Failed to toggle panic mode: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  async function addRule(type: 'port' | 'service') {
    let val = type === 'port' ? newPort.trim() : newService.trim();
    if (!val) return;
    
    // Auto-append /tcp if the user just typed a number like "80"
    if (type === 'port' && val.match(/^\d+$/)) {
      val = val + '/tcp';
    }

    statusStore.setBusy(`Adding ${type} ${val}…`);
    try {
      await invoke('modify_firewall_rule', { zone: activeZone, ruleType: type, value: val, add: true });
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --add-${type}=${val}`, 0, true);
      uiStore.addToast(`Added ${type} ${val} successfully`, 'success');
      if (type === 'port') newPort = '';
      if (type === 'service') newService = '';
      await loadRules(activeZone);
    } catch (e) {
      uiStore.addToast(`Failed to add ${type}: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --add-${type}=${val}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmRemoveRule(type: 'port' | 'service', val: string) {
    uiStore.confirm(
      `Remove ${type === 'port' ? 'Port' : 'Service'}`,
      `Are you sure you want to block ${type} ${val} by removing it from the firewall?`,
      () => doRemoveRule(type, val),
      true
    );
  }

  async function doRemoveRule(type: 'port' | 'service', val: string) {
    statusStore.setBusy(`Removing ${type} ${val}…`);
    try {
      await invoke('modify_firewall_rule', { zone: activeZone, ruleType: type, value: val, add: false });
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --remove-${type}=${val}`, 0, true);
      uiStore.addToast(`Removed ${type} ${val} successfully`, 'success');
      await loadRules(activeZone);
    } catch (e) {
      uiStore.addToast(`Failed to remove ${type}: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --remove-${type}=${val}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadState(); });
</script>

<div class="module-page">
  <PageHeader title="Firewall Manager" subtitle="Manage firewalld zones, open ports, and allowed services" icon={Shield}>
    <Button variant="ghost" class="" onclick={loadState} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    {#if state?.is_running}
      <Button 
        class="btn {state.is_panic ? 'btn-success' : '-danger'}" 
        onclick={confirmTogglePanic}
      >
        <Power size={14} /> {state.is_panic ? 'Disable Panic Mode' : 'Panic Mode'}
      </Button>
    {/if}
  </PageHeader>

  {#if loading && !state}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading Firewall State…</span>
    </div>
  {:else if state && !state.is_running}
    <div class="empty-state" style="padding: 64px 32px;">
      <div style="width:64px; height:64px; border-radius:50%; background:rgba(255,71,87,0.1); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
        <ShieldAlert size={32} class="empty-state-icon" style="color:var(--color-danger); margin:0;" />
      </div>
      <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
        Firewall is Offline
      </span>
      <span style="color:var(--color-text-muted); margin-top:8px;">
        firewalld is not currently running. You can start it from the Service Manager.
      </span>
    </div>
  {:else if state}
    {#if state.is_panic}
      <div style="margin-bottom:16px; padding:16px; border-radius:12px; background:rgba(255, 71, 87, 0.1); border:1px solid rgba(255, 71, 87, 0.3); display:flex; align-items:center; gap:12px">
        <ShieldAlert size={24} style="color:var(--color-danger)" />
        <div>
          <div style="font-weight:600; color:var(--color-danger)">Panic Mode Active</div>
          <div style="font-size:13px; color:var(--color-text-primary)">All incoming and outgoing network traffic is currently being blocked.</div>
        </div>
      </div>
    {/if}

    <div style="display:flex; gap:16px; flex:1; min-height:0">
      <!-- Zones Sidebar -->
      <div class="card" style="width: 250px; display:flex; flex-direction:column; gap:8px">
        <h3 style="margin-top:0; color:var(--color-text-primary); font-size:14px">Active Zones</h3>
        {#each state.active_zones as zone}
          <Button 
            class="zone- {activeZone === zone ? 'active' : ''}"
            onclick={() => { activeZone = zone; loadRules(zone); }}
          >
            <ShieldCheck size={16} />
            {zone}
            {#if zone === state.default_zone}
              <span class="badge badge-info" style="margin-left:auto; font-size:10px; padding:2px 6px">Default</span>
            {/if}
          </Button>
        {/each}
      </div>

      <!-- Rules Content -->
      <div class="card module-content-scroll" style="flex:1; display:flex; flex-direction:column; gap:24px">
        {#if loadingRules}
          <div style="display:flex; align-items:center; gap:8px; color:var(--color-text-muted)">
            <RefreshCw size={14} class="animate-spin" /> Loading rules…
          </div>
        {:else if rules}
          <div>
            <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Allowed Services</h3>
            <div style="display:flex; gap:8px; margin-bottom:12px">
              <input class="input" bind:value={newService} placeholder="e.g. http, https, ssh" onkeydown={(e) => e.key === 'Enter' && addRule('service')} />
              <Button variant="outline" class="" onclick={() => addRule('service')} disabled={!newService.trim()}>
                <Plus size={14} /> Add
              </Button>
            </div>
            
            {#if rules.services.length === 0}
              <div style="font-size:13px; color:var(--color-text-muted); font-style:italic">No services allowed.</div>
            {:else}
              <div style="display:flex; flex-wrap:wrap; gap:8px">
                {#each rules.services as svc}
                  <div class="rule-chip">
                    {svc}
                    <button class="rule-chip-del" onclick={() => confirmRemoveRule('service', svc)}>
                      <Trash2 size={12} />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <hr style="border:0; border-top:1px solid var(--color-border); margin:0" />

          <div>
            <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Open Ports</h3>
            <div style="display:flex; gap:8px; margin-bottom:12px">
              <input class="input" bind:value={newPort} placeholder="e.g. 8080/tcp, 53/udp" onkeydown={(e) => e.key === 'Enter' && addRule('port')} />
              <Button variant="outline" class="" onclick={() => addRule('port')} disabled={!newPort.trim()}>
                <Plus size={14} /> Add
              </Button>
            </div>

            {#if rules.ports.length === 0}
              <div style="font-size:13px; color:var(--color-text-muted); font-style:italic">No specific ports opened.</div>
            {:else}
              <div style="display:flex; flex-wrap:wrap; gap:8px">
                {#each rules.ports as port}
                  <div class="rule-chip port-chip">
                    {port}
                    <button class="rule-chip-del" onclick={() => confirmRemoveRule('port', port)}>
                      <Trash2 size={12} />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  :global(.zone-btn) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    color: var(--color-text-secondary);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: all 0.2s ease;
  }
  :global(.zone-btn:hover) {
    background: var(--color-bg-raised);
    color: var(--color-text-primary);
  }
  :global(.zone-btn.active) {
    background: var(--color-bg-raised);
    border-color: var(--color-border-focus);
    color: var(--color-accent);
  }

  .rule-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    padding: 4px 8px 4px 12px;
    border-radius: 16px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .port-chip {
    font-family: var(--font-mono);
  }
  .rule-chip-del {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }
  .rule-chip-del:hover {
    color: var(--color-danger);
    background: rgba(255, 71, 87, 0.1);
  }
</style>
