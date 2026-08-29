<script lang="ts">
  import { dnfStore } from '../stores/dnfStore.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { Terminal, RefreshCw, X, AlertTriangle, ExternalLink, OctagonX, CheckCircle2 } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';
  import DnfDryRunModal from './DnfDryRunModal.svelte';

  let terminalContainer: HTMLPreElement | null = $state(null);
  let userHasScrolledUp = $state(false);

  function handleScroll() {
    if (!terminalContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = terminalContainer;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 40;
    userHasScrolledUp = !isAtBottom;
  }

  $effect(() => {
    // Auto-scroll terminal log to bottom as output streams unless user scrolled up
    if (dnfStore.upgradeOutput && terminalContainer && !userHasScrolledUp) {
      terminalContainer.scrollTop = terminalContainer.scrollHeight;
    }
  });

  function goToDnfModule() {
    dnfStore.closeDrawer();
    uiStore.setActiveTab('dnf-history');
  }
</script>

<!-- Floating Live Status Pill (bottom-right floating card, never collides with header buttons) -->
{#if dnfStore.isUpgrading || dnfStore.upgradeFinished || dnfStore.showFloatingDrawer}
  <div class="dnf-widget-container" transition:fly={{ y: 15, duration: 250 }}>
    <div
      class="dnf-status-pill"
      class:is-upgrading={dnfStore.isUpgrading}
      class:is-success={dnfStore.upgradeFinished && dnfStore.upgradeSuccess}
      class:is-error={dnfStore.upgradeFinished && !dnfStore.upgradeSuccess}
      role="button"
      tabindex="0"
      onclick={() => dnfStore.toggleDrawer()}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') dnfStore.toggleDrawer(); }}
      title="Click to view live DNF upgrade terminal"
    >
      {#if dnfStore.isUpgrading}
        <span class="pulse-dot"></span>
        <span class="spin-icon">
          <RefreshCw size={13} />
        </span>
        <span class="pill-label">DNF Upgrading ({dnfStore.packagesBeingUpgraded.length} pkgs)...</span>
        <span class="pill-terminal-badge">
          <Terminal size={12} /> Live Logs
        </span>
      {:else if dnfStore.upgradeFinished && dnfStore.upgradeSuccess}
        <CheckCircle2 size={14} color="var(--color-success)" />
        <span class="pill-label">DNF Upgrade Complete</span>
        <button
          type="button"
          class="pill-dismiss-btn"
          onclick={(e) => { e.stopPropagation(); dnfStore.resetUpgradeView(); }}
          title="Dismiss notification"
        >
          <X size={13} />
        </button>
      {:else if dnfStore.upgradeFinished && !dnfStore.upgradeSuccess}
        <AlertTriangle size={14} color="var(--color-error)" />
        <span class="pill-label">DNF Upgrade Failed</span>
        <button
          type="button"
          class="pill-dismiss-btn"
          onclick={(e) => { e.stopPropagation(); dnfStore.resetUpgradeView(); }}
          title="Dismiss notification"
        >
          <X size={13} />
        </button>
      {/if}
    </div>
  </div>
{/if}

<!-- Slide-over / Modal Live Terminal Drawer -->
{#if dnfStore.showFloatingDrawer}
  <div class="drawer-backdrop" transition:fade={{ duration: 150 }}>
    <div class="backdrop" onclick={() => dnfStore.closeDrawer()}></div>

    <div class="drawer-modal" transition:fly={{ y: 20, duration: 250 }}>
      <!-- Header -->
      <div class="drawer-header">
        <div class="drawer-title-group">
          <div class="drawer-icon" class:spin={dnfStore.isUpgrading}>
            <Terminal size={18} color="var(--color-accent)" />
          </div>
          <div>
            <h3 class="drawer-title">
              {#if dnfStore.isUpgrading}
                Live DNF Upgrade Progress
              {:else if dnfStore.upgradeSuccess}
                DNF Upgrade Output (Completed)
              {:else}
                DNF Terminal Logs
              {/if}
            </h3>
            <p class="drawer-subtitle">
              {#if dnfStore.isUpgrading}
                Upgrading {dnfStore.packagesBeingUpgraded.length} selected packages in background
              {:else}
                Background transaction output stream
              {/if}
            </p>
          </div>
        </div>

        <div class="drawer-actions">
          {#if dnfStore.isUpgrading}
            <button class="btn btn-secondary btn-sm text-error" onclick={() => dnfStore.cancelUpgrade()} title="Cancel running upgrade">
              <OctagonX size={13} /> Cancel
            </button>
          {/if}
          <button class="btn btn-secondary btn-sm" onclick={goToDnfModule} title="Go to DNF Manager page">
            <ExternalLink size={13} /> Open DNF Manager
          </button>
          <button class="btn btn-icon btn-ghost btn-sm" onclick={() => dnfStore.closeDrawer()}>
            <X size={16} />
          </button>
        </div>
      </div>

      <!-- Hang Warning Alert -->
      {#if dnfStore.hangWarning}
        <div class="hang-alert">
          <AlertTriangle size={16} color="var(--color-warning)" />
          <span>No terminal activity detected for > 60 seconds. DNF may be downloading large packages or awaiting disk sync.</span>
        </div>
      {/if}

      <!-- Terminal Body -->
      <div class="terminal-body">
        <pre
          bind:this={terminalContainer}
          onscroll={handleScroll}
          class="drawer-terminal"
        >{dnfStore.upgradeOutput || 'Initializing DNF package manager transaction...\n'}</pre>
      </div>

      <!-- Footer / Status Bar -->
      <div class="drawer-footer">
        <div class="footer-status">
          {#if dnfStore.isUpgrading}
            <span class="status-indicator active"></span>
            <span>Transaction active & streaming logs</span>
          {:else if dnfStore.upgradeFinished && dnfStore.upgradeSuccess}
            <span class="status-indicator success"></span>
            <span>Transaction completed with Exit Code 0</span>
          {:else if dnfStore.upgradeFinished && !dnfStore.upgradeSuccess}
            <span class="status-indicator error"></span>
            <span>Transaction exited with errors</span>
          {/if}
        </div>

        <div class="footer-actions">
          <button class="btn btn-primary btn-sm" onclick={() => dnfStore.closeDrawer()}>
            Dismiss
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<DnfDryRunModal />

<style>
  .dnf-widget-container {
    position: fixed;
    bottom: 24px;
    right: 28px;
    z-index: 1500;
    pointer-events: auto;
  }

  .dnf-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: rgba(11, 23, 38, 0.92);
    border: 1px solid rgba(0, 218, 243, 0.35);
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-sans);
    padding: 7px 14px;
    border-radius: 30px;
    cursor: pointer;
    backdrop-filter: blur(12px);
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 0 15px rgba(0, 218, 243, 0.12);
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    user-select: none;
  }

  .dnf-status-pill:hover {
    transform: translateY(-2px);
    box-shadow: 0 12px 28px -5px rgba(0, 0, 0, 0.6), 0 0 20px rgba(0, 218, 243, 0.25);
    border-color: var(--color-accent);
  }

  .dnf-status-pill.is-upgrading {
    border-color: var(--color-accent);
  }

  .dnf-status-pill.is-success {
    border-color: var(--color-success);
    background: rgba(16, 185, 129, 0.15);
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 0 15px rgba(16, 185, 129, 0.15);
  }

  .dnf-status-pill.is-error {
    border-color: var(--color-error);
    background: rgba(239, 68, 68, 0.15);
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 0 15px rgba(239, 68, 68, 0.15);
  }

  /* Light Mode */
  :global(html.light-mode) .dnf-status-pill {
    background: #FFFFFF;
    border: 1px solid #CBD5E1;
    color: #0F172A;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.12), 0 4px 6px -2px rgba(0, 0, 0, 0.05);
  }

  :global(html.light-mode) .dnf-status-pill:hover {
    box-shadow: 0 12px 28px -5px rgba(0, 0, 0, 0.18), 0 6px 10px -2px rgba(0, 0, 0, 0.08);
  }

  :global(html.light-mode) .dnf-status-pill.is-upgrading {
    background: #EFF6FF;
    border-color: #3B82F6;
    color: #1D4ED8;
  }

  :global(html.light-mode) .dnf-status-pill.is-success {
    background: #F0FDF4;
    border-color: #22C55E;
    color: #15803D;
  }

  :global(html.light-mode) .dnf-status-pill.is-error {
    background: #FEF2F2;
    border-color: #EF4444;
    color: #B91C1C;
  }

  .pulse-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
    animation: pulse-dot-anim 1.5s infinite ease-in-out;
  }

  @keyframes pulse-dot-anim {
    0%, 100% { transform: scale(1); opacity: 1; }
    50% { transform: scale(1.4); opacity: 0.5; }
  }

  .pill-terminal-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-family: var(--font-mono);
    padding: 2px 6px;
    border-radius: 12px;
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
    margin-left: 2px;
  }
  :global(html.light-mode) .pill-terminal-badge {
    background: rgba(37, 99, 235, 0.12);
    color: #1D4ED8;
  }

  .pill-dismiss-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--color-text-muted);
    padding: 2px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
    margin-left: 4px;
  }
  .pill-dismiss-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
  }
  :global(html.light-mode) .pill-dismiss-btn:hover {
    background: rgba(0, 0, 0, 0.08);
    color: #0F172A;
  }

  .spin-icon {
    display: inline-flex;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* ── Drawer Modal ────────────────────────────────────────────────────────── */
  .drawer-backdrop {
    position: fixed;
    inset: 0;
    z-index: 2000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
  }

  :global(html.light-mode) .backdrop {
    background: rgba(15, 23, 42, 0.35);
  }

  .drawer-modal {
    position: relative;
    z-index: 1;
    width: 780px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 40px);
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 14px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :global(html.light-mode) .drawer-modal {
    background: #FFFFFF;
    border: 1px solid #E2E8F0;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.15);
  }

  .drawer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.2);
  }

  :global(html.light-mode) .drawer-header {
    background: #F8FAFC;
    border-bottom-color: #E2E8F0;
  }

  .drawer-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .drawer-icon {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: rgba(0, 218, 243, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  :global(html.light-mode) .drawer-icon {
    background: #E0F2FE;
  }

  .drawer-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .drawer-title {
    color: #0F172A;
  }

  .drawer-subtitle {
    margin: 2px 0 0 0;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  :global(html.light-mode) .drawer-subtitle {
    color: #64748B;
  }

  .drawer-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hang-alert {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(245, 158, 11, 0.15);
    border-bottom: 1px solid rgba(245, 158, 11, 0.3);
    color: var(--color-warning);
    font-size: 12px;
    padding: 8px 16px;
  }

  .terminal-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .drawer-terminal {
    flex: 1;
    min-height: 280px;
    max-height: calc(85vh - 170px);
    margin: 0;
    padding: 14px 16px;
    background: #020617;
    color: #38bdf8;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    line-height: 1.5;
    overflow-y: auto;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .drawer-terminal::-webkit-scrollbar {
    width: 7px;
    height: 7px;
  }

  .drawer-terminal::-webkit-scrollbar-track {
    background: rgba(0, 0, 0, 0.25);
  }

  .drawer-terminal::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.2);
    border-radius: 4px;
  }

  .drawer-terminal::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.35);
  }

  :global(html.light-mode) .drawer-terminal {
    background: #090D16;
    color: #38BDF8;
    border-top: 1px solid #1E293B;
    border-bottom: 1px solid #1E293B;
  }

  .drawer-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 16px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .drawer-footer {
    background: #F8FAFC;
    border-top-color: #E2E8F0;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  :global(html.light-mode) .status-indicator {
    color: #475569;
  }

  .pulse-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.4; transform: scale(0.85); }
  }
</style>
