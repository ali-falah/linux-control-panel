<script lang="ts">
  import { CheckCircle2, XCircle, Loader, Terminal, Wifi, Network, Copy, ExternalLink, Search } from '@lucide/svelte';
  import { statusStore, refreshAdminModeState, setAdminMode } from '../stores/status.svelte.ts';
  import { ShieldCheck } from '@lucide/svelte';

  // The countdown ticks once a second from PolicyKit's real expiry; the
  // five-second poll below corrects any drift.
  let tick = $state(0);
  $effect(() => {
    if (!statusStore.adminMode.on) return;
    const timer = setInterval(() => { tick += 1; }, 1000);
    return () => clearInterval(timer);
  });

  const adminRemaining = $derived.by(() => {
    tick; // re-evaluate once per second
    return statusStore.adminModeRemaining;
  });

  const adminState = $derived.by(() => {
    if (!statusStore.adminMode.on) return 'off';
    return adminRemaining === null ? 'expired' : 'on';
  });

  const adminRemainingLabel = $derived.by(() => {
    if (adminRemaining === null) return '';
    const minutes = Math.floor(adminRemaining / 60);
    const seconds = adminRemaining % 60;
    return `${minutes}:${String(seconds).padStart(2, '0')}`;
  });

  const adminTooltip = $derived.by(() => {
    if (adminState === 'on') {
      return 'Admin mode is on. PolicyKit keeps this authorisation until the timer runs out; click to revoke it now.';
    }
    if (adminState === 'expired') {
      return 'The PolicyKit authorisation has run out. Click to switch admin mode off.';
    }
    return 'Admin mode is off. Click to request administrator rights (one PolicyKit prompt); privileged actions are blocked until then.';
  });

  const adminError = $derived(statusStore.adminActionError ?? statusStore.adminMode.error);

  // Read the real PolicyKit state every five seconds while the window is
  // visible. This also notices an expiry and turns it into a calm note.
  $effect(() => {
    let cancelled = false;
    const poll = async () => {
      if (cancelled || document.hidden) return;
      await refreshAdminModeState();
    };
    void poll();
    const timer = setInterval(poll, 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  });

  // "Admin mode off" / "Admin mode expired" stays for a few seconds only.
  $effect(() => {
    if (!statusStore.adminNotice) return;
    const timer = setTimeout(() => statusStore.clearAdminNotice(), 4000);
    return () => clearTimeout(timer);
  });

  let toggling = $state(false);
  async function toggleAdminMode() {
    if (toggling) return;
    toggling = true;
    try {
      await setAdminMode(!statusStore.adminMode.on);
    } finally {
      toggling = false;
    }
  }
  import { uiStore } from '../stores/ui.svelte.ts';
  import { invoke } from '@tauri-apps/api/core';

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

  let distroPrettyName = $state('Linux');

  async function loadDistro() {
    try {
      const d = await invoke<{ pretty_name: string }>('get_distro_info');
      if (d?.pretty_name) distroPrettyName = d.pretty_name;
    } catch {
      distroPrettyName = 'Linux';
    }
  }

  $effect(() => {
    loadInterfaces();
    loadDistro();

    // Refresh every 30 seconds
    const interval = setInterval(loadInterfaces, 30_000);
    return () => {
      clearInterval(interval);
    };
  });
</script>

<!-- Click-outside to close popover -->
<svelte:window
  onkeydown={(e) => {
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

  <!-- CENTER: Global Search Trigger -->
  <div class="status-center">
    <button
      type="button"
      class="global-search-trigger"
      onclick={() => uiStore.openSearchModal()}
      title="Global Search (Ctrl + K)"
    >
      <Search size={11} class="search-trigger-icon" />
      <span class="search-trigger-text">Search...</span>
      <kbd class="search-trigger-kbd">Ctrl K</kbd>
    </button>
  </div>

  <!-- RIGHT: Application status -->
  <div class="status-right">
    {#if adminError}
      <span class="admin-error" title={adminError}>{adminError}</span>
    {/if}
    {#if statusStore.adminNotice}
      <span class="admin-note" role="status">{statusStore.adminNotice}</span>
    {/if}
    <button
      type="button"
      class="admin-toggle"
      class:on={adminState === 'on'}
      class:expired={adminState === 'expired'}
      role="switch"
      aria-checked={adminState === 'on'}
      aria-label="Admin mode"
      title={adminTooltip}
      disabled={toggling}
      onclick={toggleAdminMode}
    >
      <span class="admin-track" aria-hidden="true"><span class="admin-knob"></span></span>
      <ShieldCheck size={11} />
      <span>Admin mode: {adminState === 'on' ? 'On' : adminState === 'expired' ? 'Expired' : 'Off'}</span>
      {#if adminState === 'on' && adminRemainingLabel}
        <span class="admin-time">{adminRemainingLabel}</span>
      {/if}
    </button>
    {#if statusStore.lastEntry}
      <span class="pill {statusStore.lastEntry.success ? 'pill-ok' : 'pill-fail'}" title="Process Exit Code">
        {#if statusStore.lastEntry.success}
          ✓ exit {statusStore.lastEntry.exitCode ?? 0}
        {:else}
          ✕ exit {statusStore.lastEntry.exitCode ?? 1}
        {/if}
      </span>
    {:else}
      <span class="ts-pill muted">{distroPrettyName}</span>
    {/if}
  </div>
</footer>

<style>
  /* ── Admin mode toggle ───────────────────────────────────────────────── */
  .admin-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 9px 2px 5px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    color: var(--color-text-secondary);
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.18s ease, background 0.18s ease, border-color 0.18s ease;
  }
  .admin-toggle:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }
  .admin-toggle:disabled {
    cursor: progress;
    opacity: 0.65;
  }
  .admin-toggle.on {
    color: var(--color-success);
    background: color-mix(in srgb, var(--color-success) 12%, transparent);
    border-color: color-mix(in srgb, var(--color-success) 38%, transparent);
  }
  .admin-toggle.expired {
    color: var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-warning) 38%, transparent);
  }
  .admin-track {
    position: relative;
    width: 20px;
    height: 11px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-text-muted) 35%, transparent);
    flex-shrink: 0;
    transition: background 0.18s ease;
  }
  .admin-knob {
    position: absolute;
    top: 1px;
    left: 1px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
    transition: transform 0.18s ease;
  }
  .admin-toggle.on .admin-track { background: var(--color-success); }
  .admin-toggle.on .admin-knob { transform: translateX(9px); }
  .admin-toggle.expired .admin-track { background: var(--color-warning); }
  .admin-time {
    font-variant-numeric: tabular-nums;
    opacity: 0.9;
  }
  .admin-note {
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .admin-error {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10.5px;
    color: var(--color-error);
  }
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
  }/* ── Sections ──────────────────────────────────────────────────────────── */ .status-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }.status-center {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 175px;
    flex-shrink: 0;
  }.global-search-trigger {
    width: 100%;
    height: 20px;
    padding: 0 7px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 11px;
    font-family: var(--font-sans);
    transition: all 0.15s ease;
  }.global-search-trigger:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: var(--color-accent);
    color: var(--color-text-primary);
  }:global(.search-trigger-icon) {
    color: var(--color-accent);
    flex-shrink: 0;
  }.search-trigger-text {
    flex: 1;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 450;
  }.search-trigger-kbd {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 0 4px;
    font-size: 9.5px;
    font-family: var(--font-mono);
    color: var(--color-text-secondary);
    line-height: 14px;
  }:global(html.light-mode) .global-search-trigger {
    background: #F1F5F9 !important;
    border-color: #CBD5E1 !important;
    color: #64748B !important;
  }:global(html.light-mode) .global-search-trigger:hover {
    background: #E2E8F0 !important;
    border-color: var(--color-accent) !important;
    color: #0F172A !important;
  }:global(html.light-mode) .search-trigger-kbd {
    background: #E2E8F0 !important;
    border-color: #CBD5E1 !important;
    color: #475569 !important;
  }.status-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    justify-content: flex-end;
    min-width: 0;
  }.status-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }.cmd-text {
    color: var(--color-text-primary);
    font-weight: 500;
    max-width: 500px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }.cmd-text.ok { color: var(--color-text-primary); }.cmd-text.fail { color: var(--color-error); }.cmd-text.idle { color: var(--color-text-muted); }.cmd-text.busy { color: var(--color-accent); font-weight: 600; }:global(.icon-ok) { color: var(--color-success); flex-shrink: 0; }:global(.icon-fail) { color: var(--color-error); flex-shrink: 0; }:global(.icon-idle) { color: var(--color-text-muted); flex-shrink: 0; }/* ── Pill badges ──────────────────────────────────────────────────────── */ .pill {
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
  }.pill-ok { background: rgba(34, 197, 94, 0.12); color: #166534; border-color: rgba(34, 197, 94, 0.25); cursor: default; }.pill-fail { background: rgba(239, 68, 68, 0.12); color: #991B1B; border-color: rgba(239, 68, 68, 0.25); cursor: default; }.ts-pill {
    padding: 3px 9px;
    border-radius: 6px;
    background: var(--color-bg-base);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    border: 1px solid var(--color-border);
  }.ts-pill.muted { color: var(--color-text-muted); }/* ── Light Mode High Contrast Status Bar Overrides ── */ :global(html.light-mode .status-bar) {
    background: #FFFFFF !important;
    border-top: 1px solid #E5E7EB !important;
    color: #111827 !important;
  }:global(html.light-mode .cmd-text) {
    color: #0F172A !important;
    font-weight: 500 !important;
  }:global(html.light-mode .pill-ok) {
    background: rgba(34, 197, 94, 0.1) !important;
    color: #166534 !important;
    border: 1px solid rgba(34, 197, 94, 0.25) !important;
  }:global(html.light-mode .pill-fail) {
    background: rgba(239, 68, 68, 0.1) !important;
    color: #991B1B !important;
    border: 1px solid rgba(239, 68, 68, 0.25) !important;
  }/* ── IP pill ──────────────────────────────────────────────────────────── *//* ── Popover ──────────────────────────────────────────────────────────── *//* ── Spin/icon helpers ─────────────────────────────────────────────── */ :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--color-info);
    flex-shrink: 0;
  }:global(.icon-ok) { color: var(--color-success); flex-shrink: 0; }:global(.icon-fail) { color: var(--color-error);   flex-shrink: 0; }:global(.icon-idle) { color: var(--color-text-muted); flex-shrink: 0; }</style>
