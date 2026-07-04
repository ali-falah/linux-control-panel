<script lang="ts">
  import { CheckCircle2, XCircle, Loader, Minus, Shield, ShieldCheck } from '@lucide/svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import { invoke } from '@tauri-apps/api/core';

  let hasRoot = $state(false);
  let showRootModal = $state(false);
  let sudoPassword = $state('');
  let sudoError = $state('');
  let isTestingSudo = $state(false);

  async function checkSudoStatus() {
    try {
      hasRoot = await invoke('check_sudo_status');
    } catch(e) {}
  }

  async function toggleRoot() {
    if (hasRoot) {
      await invoke('clear_sudo_password');
      hasRoot = false;
    } else {
      sudoPassword = '';
      sudoError = '';
      showRootModal = true;
    }
  }

  async function submitSudo() {
    if (!sudoPassword) return;
    isTestingSudo = true;
    sudoError = '';
    try {
      await invoke('set_sudo_password', { password: sudoPassword });
      hasRoot = true;
      showRootModal = false;
    } catch(e: any) {
      sudoError = e.toString();
    } finally {
      isTestingSudo = false;
    }
  }

  $effect(() => {
    checkSudoStatus();
  });


</script>

<footer class="status-bar" role="status" aria-live="polite">
  <!-- Left: last command -->
  <div class="status-left">
    {#if statusStore.busy}
      <div class="status-indicator busy">
        <Loader size={11} class="spin-icon" />
        <span class="cmd-text">{statusStore.busyLabel}</span>
      </div>
    {:else if statusStore.lastEntry}
      <div class="status-indicator">
        {#if statusStore.lastEntry.success}
          <CheckCircle2 size={11} class="icon-ok" />
        {:else}
          <XCircle size={11} class="icon-fail" />
        {/if}
        <span class="cmd-text" class:ok={statusStore.lastEntry.success} class:fail={!statusStore.lastEntry.success}>
          {statusStore.lastEntry.command}
        </span>
      </div>
    {:else}
      <div class="status-indicator">
        <Minus size={11} class="icon-idle" />
        <span class="cmd-text idle">Ready</span>
      </div>
    {/if}
  </div>

  <!-- Right: exit code + timestamp pill -->
  <div class="status-right">
    <button class="pill {hasRoot ? 'pill-root-on' : 'pill-root-off'}" style="cursor: pointer; display: flex; align-items: center; gap: 4px; border: none; font-family: inherit; font-size: inherit;" onclick={toggleRoot}>
      {#if hasRoot}
        <ShieldCheck size={11} /> Root: ON
      {:else}
        <Shield size={11} /> Root: OFF
      {/if}
    </button>
    {#if statusStore.lastEntry}
      <span class="pill {statusStore.lastEntry.success ? 'pill-ok' : 'pill-fail'}">
        exit {statusStore.lastEntry.exitCode ?? '—'}
      </span>
    {:else}
      <span class="ts-pill muted">Fedora / RHEL</span>
    {/if}
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    padding: 0 16px;
    background: rgba(37, 40, 54, 0.4);
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 11px;
    font-family: var(--font-mono);
    flex-shrink: 0;
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    gap: 12px;
  }

  .status-left, .status-right {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .cmd-text {
    color: var(--color-text-muted);
    max-width: 480px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cmd-text.ok   { color: var(--color-success); }
  .cmd-text.fail { color: var(--color-error); }
  .cmd-text.idle { color: var(--color-text-muted); }

  /* pill badges on the right */
  .pill {
    padding: 1px 7px;
    border-radius: 20px;
    font-size: 10px;
    font-weight: 600;
    white-space: nowrap;
    letter-spacing: 0.02em;
  }
  .pill-ok   { background: var(--color-success-muted); color: var(--color-success); }
  .pill-fail { background: rgba(239, 68, 68, 0.1); color: var(--color-error); }
  .pill-root-on { background: rgba(34, 197, 94, 0.15); color: #4ade80; border: 1px solid rgba(34, 197, 94, 0.3); }
  .pill-root-off { background: rgba(255, 255, 255, 0.05); color: var(--color-text-muted); }
  .pill-root-on:hover { background: rgba(34, 197, 94, 0.25); }
  .pill-root-off:hover { background: rgba(255, 255, 255, 0.1); color: var(--color-text-primary); }

  .ts-pill {
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.2);
    color: var(--color-text-muted);
    font-size: 10px;
    white-space: nowrap;
  }
  .ts-pill.muted { color: var(--color-text-muted); }

  :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--color-info);
    flex-shrink: 0;
  }
  :global(.icon-ok)   { color: var(--color-success); flex-shrink: 0; }
  :global(.icon-fail) { color: var(--color-error); flex-shrink: 0; }
  :global(.icon-idle) { color: var(--color-text-muted); flex-shrink: 0;  }
</style>

<svelte:window onkeydown={(e) => { if (showRootModal && e.key === 'Escape') showRootModal = false; }} />

{#if showRootModal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) showRootModal = false; }}>
    <div class="modal-content" style="max-width: 320px;">
      <h3 style="margin-top:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
        <Shield size={18} style="color:var(--color-accent)"/>
        Root Privileges
      </h3>
      <p style="font-size:13px; color:var(--color-text-secondary); margin-bottom:16px;">
        Enter your sudo password. This will be securely held in memory to bypass OS prompts for this session.
      </p>
      
      {#if sudoError}
        <div style="background:rgba(239,68,68,0.1); color:var(--color-error); padding:8px 12px; border-radius:6px; font-size:12px; margin-bottom:12px;">
          {sudoError}
        </div>
      {/if}

      <form onsubmit={(e) => { e.preventDefault(); submitSudo(); }}>
        <input 
          type="password" 
          bind:value={sudoPassword}
          class="input"
          placeholder="Password..."
          style="width: 100%; margin-bottom:16px;"
        />
        <div style="display:flex; gap:8px; justify-content:flex-end;">
          <button type="button" class="btn btn-outline" onclick={() => showRootModal = false}>Cancel</button>
          <button type="submit" class="btn btn-primary" disabled={isTestingSudo || !sudoPassword}>
            {#if isTestingSudo}
              <Loader size={14} class="animate-spin-slow" />
            {:else}
              Authenticate
            {/if}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
