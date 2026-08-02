<script lang="ts">
  import { dnfStore } from '../stores/dnfStore.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { Terminal, RefreshCw, X, AlertTriangle, ExternalLink, OctagonX, CheckCircle2 } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';

  let terminalContainer: HTMLPreElement | null = $state(null);

  $effect(() => {
    // Auto-scroll terminal log to bottom as output streams
    if (dnfStore.upgradeOutput && terminalContainer) {
      terminalContainer.scrollTop = terminalContainer.scrollHeight;
    }
  });

  function goToDnfModule() {
    dnfStore.closeDrawer();
    uiStore.setActiveTab('apps');
  }
</script>

<!-- Floating Top-Bar Live Status Pill (always visible when upgrading or finished) -->
{#if dnfStore.isUpgrading || dnfStore.upgradeFinished || dnfStore.showFloatingDrawer}
  <div class="dnf-widget-container">
    <button
      type="button"
      class="dnf-status-pill"
      class:is-upgrading={dnfStore.isUpgrading}
      class:is-success={dnfStore.upgradeFinished && dnfStore.upgradeSuccess}
      class:is-error={dnfStore.upgradeFinished && !dnfStore.upgradeSuccess}
      onclick={() => dnfStore.toggleDrawer()}
      title="Click to view live DNF upgrade terminal"
    >
      {#if dnfStore.isUpgrading}
        <span class="spin-icon">
          <RefreshCw size={13} />
        </span>
        <span class="pill-label">DNF Upgrading ({dnfStore.packagesBeingUpgraded.length} pkgs)...</span>
      {:else if dnfStore.upgradeFinished && dnfStore.upgradeSuccess}
        <CheckCircle2 size={13} color="var(--color-success)" />
        <span class="pill-label">DNF Upgrade Complete</span>
      {:else if dnfStore.upgradeFinished && !dnfStore.upgradeSuccess}
        <AlertTriangle size={13} color="var(--color-error)" />
        <span class="pill-label">DNF Upgrade Failed</span>
      {/if}
      <Terminal size={13} style="margin-left:4px; opacity:0.8;" />
    </button>
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
          <button class="btn btn-secondary btn-sm" onclick={goToDnfModule} title="Go to DNF Package Manager page">
            <ExternalLink size={13} /> Open DNF Page
          </button>
          <button class="btn btn-icon btn-ghost btn-sm" onclick={() => dnfStore.closeDrawer()}>
            <X size={16} />
          </button>
        </div>
      </div>

      <!-- Hang Warning Alert -->
      {#if dnfStore.hangWarning}
        <div class="hang-alert">
          <AlertTriangle size={15} />
          <span>No terminal output received for >30s. Process may be waiting for a prompt or locked.</span>
        </div>
      {/if}

      <!-- Terminal Output Window -->
      <pre bind:this={terminalContainer} class="drawer-terminal">{dnfStore.upgradeOutput || 'Awaiting output…'}</pre>

      <!-- Footer -->
      <div class="drawer-footer">
        <span class="status-indicator">
          {#if dnfStore.isUpgrading}
            <span class="pulse-dot"></span> Active DNF Transaction Running
          {:else if dnfStore.upgradeFinished}
            Ready • {dnfStore.upgradeSuccess ? 'Success' : 'Failed'}
          {/if}
        </span>
        <button class="btn btn-secondary btn-sm" onclick={() => dnfStore.closeDrawer()}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dnf-widget-container {
    position: fixed;
    top: 14px;
    right: 220px;
    z-index: 999;
  }

  .dnf-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: rgba(15, 23, 42, 0.85);
    border: 1px solid var(--color-accent);
    color: var(--color-text-primary);
    font-size: 11.5px;
    font-weight: 600;
    font-family: var(--font-sans);
    padding: 4px 10px;
    border-radius: 20px;
    cursor: pointer;
    backdrop-filter: blur(10px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
    transition: all 0.2s ease;
  }

  .dnf-status-pill:hover {
    transform: translateY(-1px);
    box-shadow: 0 6px 16px rgba(0, 218, 243, 0.25);
  }

  .dnf-status-pill.is-upgrading {
    border-color: var(--color-accent);
  }

  .dnf-status-pill.is-success {
    border-color: var(--color-success);
    background: rgba(34, 197, 94, 0.12);
  }

  .dnf-status-pill.is-error {
    border-color: var(--color-error);
    background: rgba(239, 68, 68, 0.12);
  }

  /* Light Mode Status Pill Overrides */
  :global(html.light-mode) .dnf-status-pill {
    background: #FFFFFF;
    border: 1px solid #CBD5E1;
    color: #0F172A;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
  }

  :global(html.light-mode) .dnf-status-pill:hover {
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.12);
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

  .drawer-terminal {
    flex: 1;
    min-height: 320px;
    max-height: 500px;
    margin: 0;
    padding: 14px 16px;
    background: #020617;
    color: #38bdf8;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    line-height: 1.5;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
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
