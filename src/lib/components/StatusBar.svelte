<script lang="ts">
  import { CheckCircle2, XCircle, Loader, Minus, Shield, ShieldCheck, Terminal, Wifi, Network, Copy, ExternalLink } from '@lucide/svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { invoke } from '@tauri-apps/api/core';

  // ─── Root Auth State ────────────────────────────────────────────────────────

  let hasRoot = $state(false);
  let showRootModal = $state(false);
  let sudoPassword = $state('');
  let sudoError = $state('');
  let isTestingSudo = $state(false);

  // Svelte action: focus the element as soon as it's mounted in the DOM
  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  async function checkSudoStatus() {
    try { hasRoot = await invoke('check_sudo_status'); } catch(e) {}
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

  // ─── Network Interfaces State ────────────────────────────────────────────────

  interface NetworkInterface {
    name: string;
    ip4: string | null;
    ip6: string | null;
    is_up: boolean;
    iface_type: string; // "ethernet" | "wifi" | "loopback" | "virtual" | "other"
    mac: string | null;
  }

  let interfaces = $state<NetworkInterface[]>([]);
  let showIfacePopover = $state(false);
  let copySuccess = $state(false);

  /** The primary interface: first non-loopback, non-virtual, UP interface with an IP4 */
  let primaryIface = $derived(
    interfaces.find(i => i.is_up && i.ip4 && i.iface_type !== 'loopback' && i.iface_type !== 'virtual')
    ?? interfaces.find(i => i.ip4 && i.iface_type !== 'loopback')
    ?? null
  );

  async function loadInterfaces() {
    try {
      interfaces = await invoke<NetworkInterface[]>('get_network_interfaces');
    } catch (e) {
      interfaces = [];
    }
  }

  async function copyPrimaryIp() {
    if (!primaryIface?.ip4) return;
    try {
      await navigator.clipboard.writeText(primaryIface.ip4);
      copySuccess = true;
      setTimeout(() => (copySuccess = false), 1800);
    } catch {}
  }

  function openInterface(ifaceName: string) {
    showIfacePopover = false;
    uiStore.setActiveTabWithInterface('network-manager', ifaceName);
  }

  function closePopover() {
    showIfacePopover = false;
  }

  // ─── Lifecycle ───────────────────────────────────────────────────────────────

  $effect(() => {
    checkSudoStatus();
    loadInterfaces();
    // Refresh every 30 seconds
    const interval = setInterval(loadInterfaces, 30_000);
    return () => clearInterval(interval);
  });
</script>

<!-- Click-outside to close popover -->
<svelte:window
  onkeydown={(e) => {
    if (showRootModal && e.key === 'Escape') showRootModal = false;
    if (showIfacePopover && e.key === 'Escape') showIfacePopover = false;
  }}
  onclick={(e) => {
    // If popover open and click is outside the popover anchor, close it
    const target = e.target as Element;
    if (showIfacePopover && !target.closest('.ip-anchor')) {
      showIfacePopover = false;
    }
  }}
/>

<footer class="status-bar" role="status" aria-live="polite">

  <!-- LEFT: command / status text -->
  <div class="status-left">
    {#if statusStore.busy}
      <div class="status-indicator">
        <Loader size={12} class="spin-icon" />
        <span class="cmd-text busy" title={statusStore.busyLabel}>{statusStore.busyLabel}</span>
      </div>
    {:else if statusStore.lastEntry}
      <div class="status-indicator">
        {#if statusStore.lastEntry.success}
          <CheckCircle2 size={12} class="icon-ok" />
        {:else}
          <XCircle size={12} class="icon-fail" />
        {/if}
        <span class="cmd-text" class:ok={statusStore.lastEntry.success} class:fail={!statusStore.lastEntry.success} title={statusStore.lastEntry.command}>
          {statusStore.lastEntry.command}
        </span>
      </div>
    {:else}
      <div class="status-indicator">
        <Terminal size={12} class="icon-idle" />
        <span class="cmd-text idle">Ready</span>
      </div>
    {/if}
  </div>

  <!-- RIGHT: Root toggle + exit code -->
  <div class="status-right">
    <button
      class="pill {hasRoot ? 'pill-root-on' : 'pill-root-off'}"
      onclick={toggleRoot}
      title={hasRoot ? 'Click to disable root privileges' : 'Click to enable root privileges'}
    >
      {#if hasRoot}
        <ShieldCheck size={12} /> Root: ON
      {:else}
        <Shield size={12} /> Root: OFF
      {/if}
    </button>

    <div class="right-divider"></div>

    {#if statusStore.lastEntry}
      <span class="pill {statusStore.lastEntry.success ? 'pill-ok' : 'pill-fail'}" title="Process Exit Code">
        {#if statusStore.lastEntry.success}
          ✓ exit {statusStore.lastEntry.exitCode ?? 0}
        {:else}
          ✕ exit {statusStore.lastEntry.exitCode ?? 1}
        {/if}
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
    background: var(--color-sidebar-bg);
    border-top: 1px solid var(--color-border);
    font-size: 11.5px;
    font-family: var(--font-mono);
    flex-shrink: 0;
    gap: 12px;
    position: relative;
    box-sizing: border-box;
  }

  /* ── Sections ──────────────────────────────────────────────────────────── */
  .status-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    justify-content: flex-end;
    min-width: 0;
  }

  .right-divider {
    width: 1px;
    height: 14px;
    background: var(--color-border);
    flex-shrink: 0;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .cmd-text {
    color: var(--color-text-primary);
    font-weight: 500;
    max-width: 500px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cmd-text.ok   { color: var(--color-text-primary); }
  .cmd-text.fail { color: var(--color-error); }
  .cmd-text.idle { color: var(--color-text-muted); }
  .cmd-text.busy { color: var(--color-accent); font-weight: 600; }

  .icon-ok   { color: var(--color-success); flex-shrink: 0; }
  .icon-fail { color: var(--color-error); flex-shrink: 0; }
  .icon-idle { color: var(--color-text-muted); flex-shrink: 0; }

  /* ── Pill badges ──────────────────────────────────────────────────────── */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 14px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    letter-spacing: 0.02em;
    cursor: pointer;
    border: 1px solid transparent;
    font-family: var(--font-mono);
    transition: all 0.18s ease;
  }
  .pill-ok   { background: rgba(34, 197, 94, 0.12); color: #166534; border-color: rgba(34, 197, 94, 0.25); cursor: default; }
  .pill-fail { background: rgba(239, 68, 68, 0.12); color: #991B1B; border-color: rgba(239, 68, 68, 0.25); cursor: default; }
  .pill-root-on  { background: rgba(34, 197, 94, 0.14); color: #15803D; border-color: rgba(34, 197, 94, 0.3); }
  .pill-root-off { background: var(--color-bg-base); color: var(--color-text-secondary); border-color: var(--color-border); }
  .pill-root-on:hover  { background: rgba(34, 197, 94, 0.24); }
  .pill-root-off:hover { background: var(--color-bg-hover); color: var(--color-text-primary); border-color: var(--color-border-hover); }

  .ts-pill {
    padding: 3px 9px;
    border-radius: 6px;
    background: var(--color-bg-base);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    border: 1px solid var(--color-border);
  }
  .ts-pill.muted { color: var(--color-text-muted); }

  /* ── Light Mode High Contrast Status Bar Overrides ── */
  :global(html.light-mode .status-bar) {
    background: #FFFFFF !important;
    border-top: 1px solid #E5E7EB !important;
    color: #111827 !important;
  }
  :global(html.light-mode .cmd-text) {
    color: #0F172A !important;
    font-weight: 500 !important;
  }
  :global(html.light-mode .right-divider) {
    background: #E5E7EB !important;
  }
  :global(html.light-mode .pill-root-on) {
    background: rgba(34, 197, 94, 0.12) !important;
    color: #15803D !important;
    border: 1px solid rgba(34, 197, 94, 0.3) !important;
  }
  :global(html.light-mode .pill-root-off) {
    background: #F3F4F6 !important;
    color: #374151 !important;
    border: 1px solid #E5E7EB !important;
  }
  :global(html.light-mode .pill-ok) {
    background: rgba(34, 197, 94, 0.1) !important;
    color: #166534 !important;
    border: 1px solid rgba(34, 197, 94, 0.25) !important;
  }
  :global(html.light-mode .pill-fail) {
    background: rgba(239, 68, 68, 0.1) !important;
    color: #991B1B !important;
    border: 1px solid rgba(239, 68, 68, 0.25) !important;
  }

  /* ── IP pill ──────────────────────────────────────────────────────────── */
  .ip-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 9px;
    border-radius: 20px;
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-mono);
    letter-spacing: 0.01em;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
    white-space: nowrap;
  }
  .ip-connected {
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.2);
  }
  .ip-down {
    background: rgba(245, 158, 11, 0.08);
    color: var(--color-warning);
    border: 1px solid rgba(245, 158, 11, 0.18);
  }
  .ip-none {
    background: rgba(59, 73, 76, 0.2);
    color: var(--color-text-muted);
    border: 1px solid rgba(59, 73, 76, 0.35);
  }
  .ip-pill:hover { filter: brightness(1.15); box-shadow: 0 0 6px var(--color-accent-glow); }
  .ip-ifname { color: inherit; opacity: 0.7; }
  .ip-sep    { opacity: 0.35; margin: 0 1px; }
  .ip-addr   { font-weight: 700; letter-spacing: 0.02em; }
  .ip-caret  { font-size: 8px; opacity: 0.5; margin-left: 2px; }

  /* ── Popover ──────────────────────────────────────────────────────────── */
  .ip-anchor {
    position: relative;
  }

  .iface-popover {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    background: var(--color-bg-popover, var(--color-bg-card));
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    min-width: 280px;
    box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.25);
    z-index: 2000;
    overflow: hidden;
  }

  .popover-header {
    padding: 10px 14px 8px;
    border-bottom: 1px solid rgba(59, 73, 76, 0.45);
  }
  .popover-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--color-text-muted);
    font-family: var(--font-sans);
  }

  .popover-list {
    padding: 6px 0;
  }

  .iface-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 14px;
    transition: background 0.12s ease;
  }
  .iface-row:hover {
    background: rgba(0, 218, 243, 0.03);
  }

  .iface-icon {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .iface-icon.ethernet { background: rgba(0, 218, 243, 0.10); color: var(--color-accent); }
  .iface-icon.wifi     { background: rgba(16, 185, 129, 0.12); color: #10B981; }
  .iface-icon.virtual  { background: rgba(59, 73, 76, 0.25); color: var(--color-text-muted); }
  .iface-icon.other    { background: rgba(59, 73, 76, 0.25); color: var(--color-text-muted); }

  .iface-info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 1px;
  }
  .iface-name {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }
  .iface-ip {
    font-size: 10px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  .iface-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .dot-up   { background: #10B981; box-shadow: 0 0 5px rgba(16,185,129,0.55); }
  .dot-down { background: var(--color-text-muted); }

  .iface-open-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    border: 1px solid rgba(59, 73, 76, 0.5);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.12s ease;
    flex-shrink: 0;
  }
  .iface-open-btn:hover {
    background: rgba(0, 218, 243, 0.08);
    border-color: rgba(0, 218, 243, 0.3);
    color: var(--color-accent);
  }

  .popover-empty {
    padding: 12px 14px;
    font-size: 11px;
    color: var(--color-text-muted);
    text-align: center;
    font-family: var(--font-sans);
  }

  .popover-footer {
    padding: 8px 14px 10px;
    border-top: 1px solid rgba(59, 73, 76, 0.4);
  }

  .copy-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 4px;
    border: 1px solid rgba(59, 73, 76, 0.5);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 10px;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.15s ease;
    width: 100%;
    justify-content: center;
  }
  .copy-btn:hover { background: rgba(0, 218, 243, 0.06); color: var(--color-accent); border-color: rgba(0,218,243,0.25); }
  .copy-btn.copy-ok { color: #10B981; border-color: rgba(16,185,129,0.3); }

  /* ── Spin/icon helpers ─────────────────────────────────────────────── */
  :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--color-info);
    flex-shrink: 0;
  }
  :global(.icon-ok)   { color: var(--color-success); flex-shrink: 0; }
  :global(.icon-fail) { color: var(--color-error);   flex-shrink: 0; }
  :global(.icon-idle) { color: var(--color-text-muted); flex-shrink: 0; }
</style>

<!-- Root auth modal -->
{#if showRootModal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) showRootModal = false; }}>
    <div class="modal" style="max-width: 320px;">
      <h3 style="margin-top:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
        <Shield size={18} style="color:var(--color-accent)"/>
        Root Privileges
      </h3>
      <p style="font-size:13px; color:var(--color-text-secondary); margin-bottom:16px;">
        Enter your sudo password. It will be securely held in memory to bypass OS prompts for this session.
      </p>
      {#if sudoError}
        <div style="background:rgba(239,68,68,0.1); color:var(--color-error); padding:8px 12px; border-radius:6px; font-size:12px; margin-bottom:12px;">
          {sudoError}
        </div>
      {/if}
      <form onsubmit={(e) => { e.preventDefault(); submitSudo(); }}>
        <input
          use:focusOnMount
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
