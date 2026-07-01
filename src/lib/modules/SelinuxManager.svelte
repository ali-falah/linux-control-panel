<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ShieldAlert, Shield, RefreshCw, Power } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

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

  let status = $state<SelinuxStatus | null>(null);
  let denials = $state<Denial[]>([]);
  let loading = $state(true);
  
  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading SELinux status…');
    try {
      status = await invoke<SelinuxStatus>('get_selinux_status');
      if (status.status !== 'disabled') {
        denials = await invoke<Denial[]>('get_selinux_denials');
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
      uiStore.addToast(`SELinux set to ${newMode}`, 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to change SELinux mode: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><ShieldAlert size={20} /></div>
    <div>
      <h1 class="module-title">SELinux Manager</h1>
      <p class="module-subtitle">Manage Security-Enhanced Linux state and view access denials</p>
    </div>
    <div style="margin-left:auto; display:flex; gap:8px">
      <button class="btn btn-ghost" onclick={loadData} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
      </button>
    </div>
  </div>

  {#if loading && !status}
    <div style="padding:32px; display:flex; align-items:center; justify-content:center; gap:10px; color:var(--color-text-muted)">
      <RefreshCw size={16} class="animate-spin-slow" /> Loading SELinux State…
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
            <button class="btn {status.current_mode === 'enforcing' ? 'btn-primary' : 'btn-outline'}" disabled={status.current_mode === 'enforcing'} onclick={() => confirmChangeMode('enforcing')}>
              Enforcing
            </button>
            <button class="btn {status.current_mode === 'permissive' ? 'btn-warning' : 'btn-outline'}" disabled={status.current_mode === 'permissive'} onclick={() => confirmChangeMode('permissive')}>
              Permissive
            </button>
          </div>
        </div>
      {/if}
    </div>

    {#if status.status !== 'disabled'}
      <div class="card module-content-scroll" style="flex:1; padding:0; display:flex; flex-direction:column">
        <div style="padding:16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center">
          <h3 style="margin:0; color:var(--color-text-primary)">Recent Access Denials (AVC)</h3>
          <span class="badge badge-muted">{denials.length} events</span>
        </div>
        
        <div style="flex:1; overflow-y:auto; padding:16px; display:flex; flex-direction:column; gap:12px">
          {#if denials.length === 0}
            <div class="empty-state" style="padding:40px">
              <Shield size={48} class="empty-state-icon" style="color:var(--color-success)" />
              <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No recent denials found</span>
              <span style="font-size:14px; margin-top:8px">SELinux has not blocked any actions recently.</span>
            </div>
          {:else}
            {#each denials as denial}
              <div style="background:var(--color-bg-raised); border:1px solid var(--color-border); border-radius:8px; padding:12px">
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
    {/if}
  {/if}
</div>
