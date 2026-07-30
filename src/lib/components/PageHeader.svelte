<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronLeft, ChevronDown, History } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';

  let {
    title,
    subtitle,
    icon: Icon = undefined,
    children = undefined,
  }: {
    title: string;
    subtitle: string;
    icon?: any;
    children?: Snippet;
  } = $props();

  let showHistoryDropdown = $state(false);
  let dropdownContainerRef = $state<HTMLDivElement | null>(null);

  // Close history dropdown when clicking outside
  $effect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (!showHistoryDropdown) return;
      if (dropdownContainerRef && dropdownContainerRef.contains(e.target as Node)) return;
      showHistoryDropdown = false;
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  });

  const TAB_LABELS: Record<string, string> = {
    'system-dashboard': 'Dashboard',
    'system-monitor': 'Monitoring',
    'app-manager': 'App Manager',
    'repo-manager': 'Repo Manager',
    'dnf-history': 'DNF Manager',
    'copr-browser': 'Copr Browser',
    'journal-logs': 'Journal Logs',
    'service-manager': 'Service Manager',
    'device-manager': 'Device Manager',
    'grub-manager': 'GRUB Bootloader',
    'selinux-manager': 'SELinux Manager',
    'network-manager': 'Advanced Network',
    'hosts-manager': 'Hosts Manager',
    'firewall-manager': 'Firewall Manager',
    'security-auditor': 'Security Auditor',
    'nginx-manager': 'Nginx Manager',
    'user-manager': 'Users & Groups',
    'env-manager': 'Environment',
    'shell-env': 'Shell Environment',
    'cron-manager': 'Scheduled Tasks',
  };
</script>

<div class="header-wrapper">
  <div class="page-header">
    <div class="breadcrumb">
      <!-- Unified Back + Recent History Split Dropdown -->
      {#if uiStore.canGoBack || uiStore.recentTabs.length > 0}
        <div bind:this={dropdownContainerRef} class="back-split-container">
          {#if uiStore.canGoBack}
            <button class="back-main-btn" onclick={() => uiStore.goBack()} title="Go back to previous page">
              <ChevronLeft size={14} />
              <span>Back</span>
            </button>
          {/if}

          {#if uiStore.recentTabs.length > 0}
            <button
              class="back-dropdown-btn"
              class:open={showHistoryDropdown}
              onclick={() => showHistoryDropdown = !showHistoryDropdown}
              title="Recently Visited Pages"
            >
              <History size={11} style="color:var(--color-accent);" />
              <ChevronDown size={11} />
            </button>

            {#if showHistoryDropdown}
              <div class="history-dropdown-menu" role="menu">
                <div class="history-menu-title">
                  <History size={11} />
                  <span>Recent History</span>
                </div>
                {#each uiStore.recentTabs as recentId}
                  {@const label = TAB_LABELS[recentId]}
                  {@const isCurrent = uiStore.activeTab === recentId}
                  {#if label}
                    <button
                      class="history-menu-item"
                      class:current={isCurrent}
                      onclick={() => {
                        uiStore.setActiveTab(recentId as TabId);
                        showHistoryDropdown = false;
                      }}
                    >
                      <span>{label}</span>
                      {#if isCurrent}
                        <span class="active-dot"></span>
                      {/if}
                    </button>
                  {/if}
                {/each}
              </div>
            {/if}
          {/if}
        </div>
        <span class="crumb-separator" style="opacity: 0.25;">›</span>
      {/if}

      <span class="crumb-text active">{title}</span>
      {#if subtitle}
        <span class="crumb-separator" style="margin: 0 4px; opacity: 0.3;">&mdash;</span>
        <span class="crumb-subtitle">{subtitle}</span>
      {/if}
    </div>
    <div class="header-actions">
      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
</div>

<style>
  .header-wrapper {
    display: flex;
    flex-direction: column;
    margin: -24px -24px 6px -24px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 24px;
    background: var(--color-bg-base);
    border-bottom: none;
    border-radius: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  /* Back Split Button Container */
  .back-split-container {
    position: relative;
    display: inline-flex;
    align-items: center;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 1px;
    transition: border-color 0.15s ease;
  }

  .back-split-container:hover {
    border-color: rgba(0, 218, 243, 0.25);
  }

  .back-main-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px 8px 3px 6px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 11px;
    font-family: var(--font-sans);
    font-weight: 500;
    cursor: pointer;
    transition: color 0.15s ease, background 0.15s ease;
    border-radius: 5px 0 0 5px;
  }

  .back-main-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.06);
  }

  .back-dropdown-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px 6px;
    background: transparent;
    border: none;
    border-left: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
    border-radius: 0 5px 5px 0;
  }

  .back-dropdown-btn:hover, .back-dropdown-btn.open {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
  }

  /* History Floating Dropdown Menu */
  .history-dropdown-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 100;
    min-width: 170px;
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    padding: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menu-fade 0.15s ease;
  }

  @keyframes menu-fade {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .history-menu-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 700;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 8px 6px;
    border-bottom: 1px dashed rgba(255, 255, 255, 0.08);
    margin-bottom: 2px;
  }

  .history-menu-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
  }

  .history-menu-item:hover {
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-text-primary);
  }

  .history-menu-item.current {
    color: var(--color-accent);
    font-weight: 600;
  }

  .active-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--color-accent);
  }

  .crumb-text {
    color: var(--color-text-muted);
    font-weight: 500;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.03em;
  }

  .crumb-text.active {
    color: var(--color-text-primary);
    font-weight: 600;
    font-family: var(--font-sans);
    font-size: 12px;
    letter-spacing: 0;
  }

  .crumb-separator {
    color: var(--color-text-muted);
    font-size: 14px;
    opacity: 0.4;
    font-family: var(--font-mono);
  }

  .crumb-subtitle {
    color: var(--color-text-muted);
    font-weight: 400;
    font-size: 11px;
    font-family: var(--font-sans);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
