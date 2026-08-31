<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    ChevronLeft,
    ChevronDown,
    ChevronRight,
    History,
    LayoutDashboard,
    Activity,
    LayoutGrid,
    Package,
    FileText,
    Settings2,
    HardDrive,
    Terminal,
    Cpu,
    Wifi,
    Globe,
    Shield,
    ShieldCheck,
    Server,
    Users,
    Clock,
    Layers,
    KeyRound,
    Search,
    Database,
    type Icon as LucideIcon
  } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';

  export interface BreadcrumbItem {
    label: string;
    tab?: TabId;
    subTab?: string;
    icon?: any;
    action?: () => void;
    isCurrent?: boolean;
  }

  let {
    title,
    subtitle = '',
    description = '',
    icon: Icon = undefined,
    breadcrumbs = undefined,
    children = undefined,
  }: {
    title: string;
    subtitle?: string;
    description?: string;
    icon?: any;
    breadcrumbs?: BreadcrumbItem[];
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

  const TAB_INFO: Record<string, { label: string; icon: any }> = {
    'system-dashboard': { label: 'Dashboard', icon: LayoutDashboard },
    'system-monitor': { label: 'Monitoring', icon: Activity },
    'app-manager': { label: 'App Manager', icon: LayoutGrid },
    'repo-manager': { label: 'Repo Manager', icon: Database },
    'dnf-history': { label: 'DNF Manager', icon: Package },
    'copr-browser': { label: 'Copr Browser', icon: Package },
    'journal-logs': { label: 'Journal Logs', icon: FileText },
    'service-manager': { label: 'Service Manager', icon: Settings2 },
    'device-manager': { label: 'Device Manager', icon: HardDrive },
    'grub-manager': { label: 'GRUB Bootloader', icon: Cpu },
    'selinux-manager': { label: 'SELinux Manager', icon: ShieldCheck },
    'network-manager': { label: 'Advanced Network', icon: Wifi },
    'hosts-manager': { label: 'Hosts Manager', icon: Globe },
    'firewall-manager': { label: 'Firewall Manager', icon: Shield },
    'security-auditor': { label: 'Security Auditor', icon: ShieldCheck },
    'nginx-manager': { label: 'Nginx Manager', icon: Server },
    'user-manager': { label: 'Users & Groups', icon: Users },
    'env-manager': { label: 'Environment', icon: FileText },
    'shell-env': { label: 'Shell Environment', icon: Terminal },
    'cron-manager': { label: 'Scheduled Tasks', icon: Clock },
    'pm2-manager': { label: 'PM2 Manager', icon: Layers },
    'ssh-cert-manager': { label: 'SSH & SSL Vault', icon: KeyRound },
  };

  const computedBreadcrumbs = $derived.by(() => {
    // 1. If explicit custom breadcrumbs provided
    if (breadcrumbs && breadcrumbs.length > 0) {
      return breadcrumbs.map((c, i) => ({
        ...c,
        isCurrent: i === breadcrumbs.length - 1
      }));
    }

    const currentTabId = uiStore.activeTab;
    const currentMeta = TAB_INFO[currentTabId] || { label: title, icon: Icon };

    // If on Dashboard root
    if (currentTabId === 'system-dashboard') {
      return [
        {
          label: 'Dashboard',
          tab: 'system-dashboard' as TabId,
          icon: LayoutDashboard,
          isCurrent: true
        }
      ];
    }

    const trail: BreadcrumbItem[] = [];

    // Root Home link
    trail.push({
      label: 'Dashboard',
      tab: 'system-dashboard' as TabId,
      icon: LayoutDashboard,
      isCurrent: false
    });

    // Check immediate predecessor from navHistory (if not Dashboard and not current)
    const history = uiStore.navHistory;
    if (history.length > 0) {
      const prevTab = history[history.length - 1];
      if (prevTab !== 'system-dashboard' && prevTab !== currentTabId && TAB_INFO[prevTab]) {
        trail.push({
          label: TAB_INFO[prevTab].label,
          tab: prevTab,
          icon: TAB_INFO[prevTab].icon,
          isCurrent: false
        });
      }
    }

    // Special deep-link spatial contexts:
    // A) Jump to Journal Logs for a specific systemd unit
    if (currentTabId === 'journal-logs' && uiStore.preAppliedJournalUnit) {
      if (!trail.some(t => t.tab === 'service-manager')) {
        trail.push({
          label: 'Services',
          tab: 'service-manager' as TabId,
          icon: Settings2,
          isCurrent: false
        });
      }
      trail.push({
        label: uiStore.preAppliedJournalUnit,
        icon: Terminal,
        isCurrent: false,
        action: () => {
          uiStore.jumpToJournalService(uiStore.preAppliedJournalUnit);
        }
      });
    }

    // B) Jump to Journal Logs for a path / disk (e.g. /var/log from storage)
    if (currentTabId === 'journal-logs' && uiStore.preAppliedJournalSearch && !uiStore.preAppliedJournalUnit) {
      if (!trail.some(t => t.tab === 'device-manager' || t.tab === 'system-monitor')) {
        trail.push({
          label: 'Storage & Disks',
          tab: 'system-monitor' as TabId,
          subTab: 'disks',
          icon: HardDrive,
          isCurrent: false
        });
      }
      trail.push({
        label: uiStore.preAppliedJournalSearch,
        icon: Search,
        isCurrent: false
      });
    }

    // C) Current Page
    trail.push({
      label: currentMeta.label || title,
      tab: currentTabId,
      icon: Icon || currentMeta.icon,
      isCurrent: true
    });

    // Mark current item
    return trail.map((c, i) => ({
      ...c,
      isCurrent: i === trail.length - 1
    }));
  });

  function handleCrumbClick(crumb: BreadcrumbItem) {
    if (crumb.isCurrent) return;

    if (crumb.action) {
      crumb.action();
      return;
    }

    if (crumb.tab) {
      if (crumb.tab === 'system-dashboard') {
        uiStore.navHistory = [];
        uiStore.setActiveTab('system-dashboard');
      } else {
        const idx = uiStore.navHistory.lastIndexOf(crumb.tab);
        if (idx !== -1) {
          uiStore.navHistory = uiStore.navHistory.slice(0, idx);
        }
        uiStore.navigateTo(crumb.tab, crumb.subTab);
      }
    }
  }
</script>

<div class="header-wrapper">
  <div class="page-header">
    <div class="breadcrumb" aria-label="Breadcrumb navigation">
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
                  {@const meta = TAB_INFO[recentId]}
                  {@const isCurrent = uiStore.activeTab === recentId}
                  {#if meta}
                    <button
                      class="history-menu-item"
                      class:current={isCurrent}
                      onclick={() => {
                        uiStore.setActiveTab(recentId as TabId);
                        showHistoryDropdown = false;
                      }}
                    >
                      <span class="history-item-label">
                        <meta.icon size={12} />
                        <span>{meta.label}</span>
                      </span>
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
        <span class="crumb-divider-bar" aria-hidden="true">|</span>
      {/if}

      <!-- Interactive Breadcrumb Trail with Deep Linking -->
      <nav class="breadcrumb-trail" aria-label="Breadcrumbs">
        {#each computedBreadcrumbs as crumb, i}
          {#if i > 0}
            <span class="crumb-chevron" aria-hidden="true">
              <ChevronRight size={12} />
            </span>
          {/if}

          {#if crumb.isCurrent}
            <span class="crumb-node current" aria-current="page">
              {#if crumb.icon}
                <crumb.icon size={13} class="crumb-icon active" />
              {/if}
              <span class="crumb-text">{crumb.label}</span>
            </span>
          {:else}
            <button
              type="button"
              class="crumb-node link"
              onclick={() => handleCrumbClick(crumb)}
              title={crumb.tab ? `Navigate to ${crumb.label}` : crumb.label}
            >
              {#if crumb.icon}
                <crumb.icon size={13} class="crumb-icon" />
              {/if}
              <span class="crumb-text">{crumb.label}</span>
            </button>
          {/if}
        {/each}
      </nav>
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
    margin: -24px -24px 8px -24px;
    padding-top: 6px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 24px;
    background: var(--color-bg-base);
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    min-height: 44px;
  }

  :global(html.light-mode) .page-header {
    border-bottom-color: #EDEEF1;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    min-width: 0;
    flex-shrink: 1;
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
    flex-shrink: 0;
  }

  :global(html.light-mode) .back-split-container {
    background: #FFFFFF;
    border-color: #CBD5E1;
  }

  .back-split-container:hover {
    border-color: var(--color-accent, rgba(0, 218, 243, 0.35));
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

  :global(html.light-mode) .back-main-btn:hover {
    background: #F1F5F9;
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

  :global(html.light-mode) .back-dropdown-btn {
    border-left-color: #CBD5E1;
  }

  .back-dropdown-btn:hover, .back-dropdown-btn.open {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    color: var(--color-accent);
  }

  .crumb-divider-bar {
    color: var(--color-text-muted);
    opacity: 0.2;
    font-size: 13px;
    user-select: none;
    flex-shrink: 0;
  }

  /* History Floating Dropdown Menu */
  .history-dropdown-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 100;
    min-width: 180px;
    background: var(--color-bg-card, #0b1726);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    padding: 6px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menu-fade 0.15s ease;
  }

  :global(html.light-mode) .history-dropdown-menu {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.1);
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

  :global(html.light-mode) .history-menu-title {
    border-bottom-color: #E2E8F0;
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

  .history-item-label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .history-menu-item:hover {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.1));
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

  /* Interactive Breadcrumb Trail */
  .breadcrumb-trail {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: nowrap;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 2px 0;
  }

  .breadcrumb-trail::-webkit-scrollbar {
    display: none;
  }

  .crumb-node {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-family: var(--font-sans);
    padding: 3px 7px;
    border-radius: 5px;
    white-space: nowrap;
    transition: all 0.15s ease;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
  }

  .crumb-node.link {
    cursor: pointer;
    font-weight: 500;
  }

  .crumb-node.link:hover {
    color: var(--color-accent);
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.08));
    transform: translateY(-0.5px);
  }

  .crumb-node.current {
    color: var(--color-text-primary);
    font-weight: 600;
    cursor: default;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  :global(html.light-mode) .crumb-node.current {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .crumb-icon {
    flex-shrink: 0;
    opacity: 0.7;
    transition: opacity 0.15s ease;
  }

  .crumb-node.link:hover .crumb-icon {
    opacity: 1;
    color: var(--color-accent);
  }

  .crumb-icon.active {
    opacity: 1;
    color: var(--color-accent);
  }

  .crumb-text {
    line-height: 1.3;
  }

  .crumb-chevron {
    display: inline-flex;
    align-items: center;
    color: var(--color-text-muted);
    opacity: 0.35;
    flex-shrink: 0;
    margin: 0 1px;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
</style>
