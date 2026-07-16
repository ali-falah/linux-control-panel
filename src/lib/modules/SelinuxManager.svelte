<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { ShieldAlert, Shield, RefreshCw, Power } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  interface SelinuxStatus {
    status: string;
    current_mode: string;
    config_mode: string;
  }

  interface Denial {
    raw: string;
    timestamp: string;
    scontext: string;
    tcontext: string;
    tclass: string;
  }

  import SideDrawer from '../components/SideDrawer.svelte';
  import Select from '../components/ui/Select.svelte';

  let status = $state<SelinuxStatus | null>(null);
  let denials = $state<Denial[]>([]);
  let loading = $state(true);

  // Tab control
  let activeTab = $state<'status' | 'booleans'>('status');

  // Booleans list
  let booleans = $state<any[]>([]);
  let searchQuery = $state('');
  let loadingBools = $state(false);
  let permanentChange = $state(true);

  // Smart AVC Troubleshooter
  let selectedDenial = $state<any | null>(null);
  let showTroubleshooter = $state(false);
  let audit2allowExplanation = $state('');
  let loadingExplanation = $state(false);
  let customModuleName = $state('my_allow_policy');
  let applyingOverride = $state(false);

  // Pagination for booleans
  let currentPage = $state(1);
  const itemsPerPage = 15;

  let filteredBooleans = $derived.by(() => {
    if (!searchQuery) return booleans;
    const lower = searchQuery.toLowerCase();
    return booleans.filter(b => b.name.toLowerCase().includes(lower));
  });

  let paginatedBooleans = $derived.by(() => {
    const start = (currentPage - 1) * itemsPerPage;
    return filteredBooleans.slice(start, start + itemsPerPage);
  });

  let totalPages = $derived(Math.ceil(filteredBooleans.length / itemsPerPage));
  
  async function loadBooleans() {
    loadingBools = true;
    try {
      booleans = await invoke<any[]>('selinux_get_booleans');
    } catch(e) {
      console.error(e);
    } finally {
      loadingBools = false;
    }
  }
  
  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading SELinux status…');
    try {
      status = await invoke<SelinuxStatus>('get_selinux_status');
      if (status.status !== 'disabled') {
        denials = await invoke<Denial[]>('get_selinux_denials');
        await loadBooleans();
      }
      statusStore.setLastCommand('sestatus / ausearch', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load SELinux data: ${e}`, 'error');
      statusStore.setLastCommand('sestatus', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function toggleBoolean(name: string, currentValue: boolean) {
    const newValue = !currentValue;
    statusStore.setBusy(`Updating boolean ${name}…`);
    try {
      const msg = await invoke<string>('selinux_set_boolean', {
        name,
        value: newValue,
        permanent: permanentChange
      });
      uiStore.addToast(msg, 'success');
      await loadBooleans();
    } catch (e) {
      uiStore.addToast(`Failed to set boolean: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function explainDenial(denial: any) {
    selectedDenial = denial;
    showTroubleshooter = true;
    loadingExplanation = true;
    audit2allowExplanation = '';
    
    const parts = denial.tcontext.split(':');
    const targetType = parts.length > 2 ? parts[2] : 'custom';
    customModuleName = `allow_${targetType}_${denial.tclass}`;
    
    try {
      audit2allowExplanation = await invoke<string>('selinux_explain_denial', {
        rawLog: denial.raw
      });
    } catch (e) {
      console.error(e);
      audit2allowExplanation = `Failed to analyze denial: ${e}`;
    } finally {
      loadingExplanation = false;
    }
  }

  async function applyPolicyOverride() {
    if (!selectedDenial) return;
    applyingOverride = true;
    statusStore.setBusy('Compiling and loading SELinux policy module…');
    try {
      const res = await invoke<string>('selinux_apply_policy_override', {
        name: customModuleName,
        rawLog: selectedDenial.raw
      });
      uiStore.addToast(res, 'success');
      showTroubleshooter = false;
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to install policy override: ${e}`, 'error');
    } finally {
      applyingOverride = false;
      statusStore.clearBusy();
    }
  }

  function confirmChangeMode(newMode: string) {
    uiStore.confirm(
      'Change SELinux Mode',
      `Are you sure you want to change SELinux to ${newMode}?\n\nSetting it to permissive disables access denial enforcement but continues logging.`,
      () => doChangeMode(newMode),
      true
    );
  }

  async function doChangeMode(newMode: string) {
    statusStore.setBusy(`Setting SELinux to ${newMode}…`);
    try {
      await invoke('set_selinux_state', { mode: newMode });
      statusStore.setLastCommand(`setenforce ${newMode === 'enforcing' ? '1' : '0'}`, 0, true);
      uiStore.addToast(`SELinux mode changed to ${newMode}`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to change SELinux mode: ${e}`, 'error');
      statusStore.setLastCommand(`setenforce ${newMode === 'enforcing' ? '1' : '0'}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page">
  <PageHeader title="SELinux Manager" subtitle="Manage Security-Enhanced Linux state and view access denials" icon={ShieldAlert}>
    <Button variant="ghost" class="" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
  </PageHeader>

  {#if loading && !status}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading SELinux State…</span>
    </div>
  {:else if status}
    <div class="card" style="margin-bottom:16px">
      <h3 style="margin-top:0; color:var(--color-text-primary); margin-bottom:16px">Current Status</h3>
      
      {#if status.status === 'disabled'}
        <div style="padding:16px; border-radius:8px; background:rgba(255, 71, 87, 0.1); color:var(--color-danger); display:flex; align-items:center; gap:12px">
          <Power size={24} />
          <div>
            <div style="font-weight:600">SELinux is Disabled</div>
            <div style="font-size:13px">Your system does not have SELinux active. Reboot may be required to enable it.</div>
          </div>
        </div>
      {:else}
        <div style="display:flex; gap:16px; align-items:center">
          <div style="flex:1">
            <div style="font-size:12px; color:var(--color-text-secondary); margin-bottom:4px">Current Runtime Mode</div>
            <div style="font-size:20px; font-weight:600; color:var(--color-text-primary); text-transform:capitalize">
              {status.current_mode}
              {#if status.current_mode === 'enforcing'}
                <Shield size={18} style="color:var(--color-success); display:inline-block; vertical-align:middle; margin-left:4px" />
              {:else if status.current_mode === 'permissive'}
                <ShieldAlert size={18} style="color:var(--color-warning); display:inline-block; vertical-align:middle; margin-left:4px" />
              {/if}
            </div>
          </div>
          <div style="flex:1">
            <div style="font-size:12px; color:var(--color-text-secondary); margin-bottom:4px">Config File Mode (Next Boot)</div>
            <div style="font-size:16px; color:var(--color-text-primary); text-transform:capitalize">
              {status.config_mode}
            </div>
          </div>
          <div style="display:flex; gap:8px">
            <Button class="btn {status.current_mode === 'enforcing' ? 'btn-primary' : '-outline'}" disabled={status.current_mode === 'enforcing'} onclick={() => confirmChangeMode('enforcing')}>
              Enforcing
            </Button>
            <Button class="btn {status.current_mode === 'permissive' ? 'btn-warning' : '-outline'}" disabled={status.current_mode === 'permissive'} onclick={() => confirmChangeMode('permissive')}>
              Permissive
            </Button>
          </div>
        </div>
      {/if}
    </div>

    {#if status.status !== 'disabled'}
      <div class="controls-row" style="margin-bottom:16px; border-bottom:1px solid var(--color-border); padding-bottom:12px;">
        <div class="tab-bar">
          <button class="tab-btn { activeTab === 'status' ? 'active' : '' }" onclick={() => activeTab = 'status'}>
            Status & Denials
          </button>
          <button class="tab-btn { activeTab === 'booleans' ? 'active' : '' }" onclick={() => activeTab = 'booleans'}>
            SELinux Booleans
          </button>
        </div>
      </div>

      {#if activeTab === 'status'}
        <div class="card module-content-scroll" style="flex:1; padding:0; display:flex; flex-direction:column">
          <div style="padding:16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center">
            <h3 style="margin:0; color:var(--color-text-primary)">Recent Access Denials (AVC)</h3>
            <span class="badge badge-muted">{denials.length} events</span>
          </div>
          
          <div style="flex:1; overflow-y:auto; padding:16px; display:flex; flex-direction:column; gap:12px">
            {#if denials.length === 0}
              <div class="empty-state" style="padding: 64px 32px;">
                <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
                  <Shield size={32} class="empty-state-icon" style="margin:0; color:var(--color-success);" />
                </div>
                <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
                  No Recent Denials
                </span>
                <span style="color:var(--color-text-muted); margin-top:8px;">
                  SELinux has not blocked any actions recently.
                </span>
              </div>
            {:else}
              <div style="font-size:12px; color:var(--color-text-secondary); margin-bottom:8px;">
                Click on any denial event below to analyze and troubleshoot it using audit2allow.
              </div>
              {#each denials as denial}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div 
                  class="card" 
                  style="cursor:pointer; transition:all 0.2s; border:1px solid transparent; background:var(--color-bg-raised); padding:12px;"
                  onclick={() => explainDenial(denial)}
                  onmouseenter={(e) => e.currentTarget.style.borderColor='var(--color-danger)'}
                  onmouseleave={(e) => e.currentTarget.style.borderColor='transparent'}
                >
                  <div style="display:flex; justify-content:space-between; margin-bottom:8px">
                    <span style="font-size:12px; font-weight:600; color:var(--color-danger)">DENIED</span>
                    <span style="font-size:12px; color:var(--color-text-secondary)">{denial.timestamp}</span>
                  </div>
                  <div style="font-size:13px; font-family:var(--font-mono); color:var(--color-text-primary); margin-bottom:4px">
                    <strong>Source:</strong> {denial.scontext}
                  </div>
                  <div style="font-size:13px; font-family:var(--font-mono); color:var(--color-text-primary); margin-bottom:4px">
                    <strong>Target:</strong> {denial.tcontext}
                  </div>
                  <div style="font-size:13px; font-family:var(--font-mono); color:var(--color-text-primary)">
                    <strong>Class:</strong> {denial.tclass}
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>
      {:else}
        <!-- SELinux Booleans Panel -->
        <div class="card" style="flex:1; min-height:0; display:flex; flex-direction:column; gap:16px;">
          <div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:12px;">
            <div style="display:flex; align-items:center; gap:8px;">
              <input class="input" bind:value={searchQuery} placeholder="Search booleans..." style="width:240px; margin:0;" oninput={() => currentPage = 1} />
            </div>
            <div style="display:flex; align-items:center; gap:8px;">
              <span style="font-size:12.5px; color:var(--color-text-secondary);">Save persistently (-P)</span>
              <Toggle checked={permanentChange} onToggle={(checked) => permanentChange = checked} />
            </div>
          </div>

          {#if loadingBools}
            <div style="padding:40px; text-align:center; color:var(--color-text-muted);">
              <RefreshCw size={20} class="animate-spin-slow" /> Loading booleans...
            </div>
          {:else}
            <div class="table-wrap" style="flex:1; min-height:0; overflow-y:auto;">
              <table>
                <thead>
                  <tr>
                    <th>Boolean Name</th>
                    <th style="width:100px; text-align:center;">Value</th>
                    <th style="width:120px; text-align:center;">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {#each paginatedBooleans as bool}
                    <tr>
                      <td style="font-family:var(--font-mono); color:var(--color-text-secondary);">{bool.name}</td>
                      <td style="text-align:center;">
                        <span class="badge {bool.value ? 'badge-success' : 'badge-muted'}">
                          {bool.value ? 'on' : 'off'}
                        </span>
                      </td>
                      <td style="text-align:center;">
                        <Button variant="outline" style="padding: 4px 10px; font-size:12px;" onclick={() => toggleBoolean(bool.name, bool.value)}>
                          Toggle to {bool.value ? 'off' : 'on'}
                        </Button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <!-- Pagination controls -->
            {#if totalPages > 1}
              <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding-top:12px; border-top:1px solid var(--color-border); flex-shrink:0;">
                <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === 1} onclick={() => currentPage--}>
                  Previous
                </Button>
                <span style="font-size:13px; color:var(--color-text-secondary);">
                  Page {currentPage} of {totalPages}
                </span>
                <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === totalPages} onclick={() => currentPage++}>
                  Next
                </Button>
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    {/if}
  {/if}

  <SideDrawer bind:isOpen={showTroubleshooter} title="AVC Denial Troubleshooter" width="540px">
    {#if selectedDenial}
      <div style="display:flex; flex-direction:column; gap:16px; padding:8px 0;">
        <div>
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">Raw Audit Event Log</h4>
          <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:6px; padding:10px; max-height:100px; overflow:auto;">
            <pre style="margin:0; font-family:var(--font-mono); font-size:11px; color:var(--color-text-muted); white-space:pre-wrap;">{selectedDenial.raw}</pre>
          </div>
        </div>

        <div>
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">smart audit2allow Explanation</h4>
          {#if loadingExplanation}
            <div style="color:var(--color-text-muted); font-size:13px; display:flex; align-items:center; gap:6px;">
              <RefreshCw size={14} class="animate-spin-slow" /> Analyzing AVC logs...
            </div>
          {:else}
            <div style="background:rgba(0,218,243,0.02); border:1px solid var(--color-border); border-radius:6px; padding:12px; display:flex; flex-direction:column; gap:8px;">
              <pre style="margin:0; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); white-space:pre-wrap;">{audit2allowExplanation || 'No suggestions could be generated.'}</pre>
              
              <!-- Check for suggested boolean -->
              {#if audit2allowExplanation.match(/boolean '([^']+)'/)}
                {@const suggestedBool = audit2allowExplanation.match(/boolean '([^']+)'/)[1]}
                <div style="margin-top:10px; padding:10px; border-radius:6px; background:rgba(0,218,243,0.05); border:1px solid rgba(0,218,243,0.2); display:flex; flex-direction:column; gap:6px;">
                  <span style="font-size:12px; color:var(--color-text-primary); font-weight:600;">Suggested Solution</span>
                  <span style="font-size:12px; color:var(--color-text-secondary); line-height:1.4;">
                    Enable the <code>{suggestedBool}</code> boolean to allow this activity.
                  </span>
                  <div style="display:flex; justify-content:flex-end;">
                    <Button variant="primary" style="padding: 4px 10px; font-size:12px;" onclick={() => { toggleBoolean(suggestedBool, false); showTroubleshooter = false; }}>
                      Enable Boolean
                    </Button>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <div style="border-top:1px solid var(--color-border); padding-top:16px;">
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">Compile Policy Override</h4>
          <p style="font-size:12px; color:var(--color-text-secondary); margin-bottom:12px; line-height:1.4;">
            If no boolean matches, you can compile a custom policy module override containing the rules shown above. This compiles a secure type enforcement module and loads it using <code>semodule</code>.
          </p>

          <div style="display:flex; flex-direction:column; gap:12px;">
            <div style="display:flex; flex-direction:column; gap:4px;">
              <label for="policy-mod-name" style="font-size:11.5px; color:var(--color-text-muted);">Policy Module Name</label>
              <input id="policy-mod-name" type="text" class="input" bind:value={customModuleName} placeholder="e.g. allow_http_read" />
            </div>
            <div style="display:flex; justify-content:flex-end;">
              <Button variant="primary" disabled={applyingOverride || loadingExplanation} onclick={applyPolicyOverride}>
                {applyingOverride ? 'Installing Module...' : 'Install Policy Override'}
              </Button>
            </div>
          </div>
        </div>
      </div>
    {/if}
  </SideDrawer>
</div>
