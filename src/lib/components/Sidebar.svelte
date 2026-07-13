<script lang="ts">
  import { Package, History, LayoutGrid, Layers, Settings2, Globe } from '@lucide/svelte';
  import { Users, Shield, Cpu, ShieldCheck, Clock, FileText, Server } from '@lucide/svelte';
  import { ChevronLeft, ChevronRight, Database, Terminal, ChevronDown } from '@lucide/svelte';
  import { HardDrive, Wifi, Activity, Search, LayoutDashboard } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';
  import { getVersion } from '@tauri-apps/api/app';

  let appVersion = $state('...');
  
  $effect(() => {
    getVersion().then(v => appVersion = `v${v}`).catch(() => appVersion = 'v0.0.0');
  });

  let searchQuery = $state('');

  let expandedGroups = $state<Record<string, boolean>>({
    'Overview': true,
    'Packages': true,
    'System': false,
    'Network & Security': false,
    'Users & Config': false
  });

  $effect(() => {
    if (uiStore.activeTab) {
      for (const group of groups) {
        if (group.items.some(i => i.id === uiStore.activeTab)) {
          expandedGroups[group.label] = true;
        }
      }
    }
  });

  function toggleGroup(label: string) {
    expandedGroups[label] = !expandedGroups[label];
  }

  const groups: {
    label: string;
    items: { id: TabId; label: string; icon: any }[];
  }[] = [
    {
      label: 'Overview',
      items: [
        { id: 'system-dashboard', label: 'Dashboard',  icon: LayoutDashboard },
        { id: 'system-monitor',   label: 'Monitoring', icon: Activity },
      ],
    },
    {
      label: 'Packages',
      items: [
        { id: 'app-manager',   label: 'App Manager',    icon: LayoutGrid },
        { id: 'repo-manager',  label: 'Repo Manager',   icon: Database },
        { id: 'dnf-history',   label: 'DNF Manager',    icon: Package },
        { id: 'copr-browser',  label: 'Copr Browser',   icon: LayoutGrid },
        { id: 'flatpak-rpm',   label: 'Flatpak vs RPM', icon: Layers },
      ],
    },
    {
      label: 'System',
      items: [
        { id: 'journal-logs',    label: 'Journal Logs',    icon: FileText },

        { id: 'service-manager', label: 'Service Manager', icon: Settings2 },
        { id: 'device-manager',  label: 'Device Manager',  icon: HardDrive },
        { id: 'grub-manager',    label: 'GRUB Bootloader', icon: Cpu },
        { id: 'selinux-manager', label: 'SELinux Manager', icon: ShieldCheck },
      ],
    },
    {
      label: 'Network & Security',
      items: [
        { id: 'network-manager',  label: 'Advanced Network', icon: Wifi },
        { id: 'hosts-manager',    label: 'Hosts Manager',    icon: Globe },
        { id: 'firewall-manager', label: 'Firewall Manager', icon: Shield },
        { id: 'nginx-manager',    label: 'Nginx Manager',    icon: Server },
      ],
    },
    {
      label: 'Users & Config',
      items: [
        { id: 'user-manager', label: 'Users & Groups',    icon: Users },
        { id: 'env-manager',  label: 'Environment',       icon: FileText },
        { id: 'shell-env',    label: 'Shell Environment', icon: Terminal },
        { id: 'cron-manager', label: 'Scheduled Tasks',   icon: Clock },
      ],
    },
  ];
</script>

<aside
  class="sidebar"
  class:collapsed={uiStore.sidebarCollapsed}
  role="navigation"
  aria-label="Module navigation"
>
  <!-- Logo -->
  <div class="sidebar-logo">
    <div class="logo-icon">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="2" y="3" width="20" height="14" rx="2"/>
        <path d="M8 21h8M12 17v4"/>
        <circle cx="12" cy="10" r="2" fill="currentColor" stroke="none" opacity="0.6"/>
        <path d="M7 10h1M16 10h1" stroke-linecap="round"/>
      </svg>
    </div>
    {#if !uiStore.sidebarCollapsed}
      <div class="logo-text">
        <span class="logo-title">Control Panel</span>
        <span class="logo-version">{appVersion}</span>
      </div>
    {/if}
  </div>

  <div class="sidebar-divider"></div>

  <!-- Search -->
  {#if !uiStore.sidebarCollapsed}
    <div class="sidebar-search">
      <Search size={14} class="search-icon" />
      <input type="text" placeholder="Search..." bind:value={searchQuery} />
    </div>
  {/if}

  <!-- Grouped Navigation -->
  <nav class="sidebar-nav">
    {#each groups as group}
      {@const filteredItems = group.items.filter(i => i.label.toLowerCase().includes(searchQuery.toLowerCase()))}
      {#if filteredItems.length > 0}
        {#if !uiStore.sidebarCollapsed && !searchQuery}
          <button class="group-label-btn" onclick={() => toggleGroup(group.label)}>
            <span class="group-label">{group.label}</span>
            <span class="group-chevron" class:expanded={expandedGroups[group.label]}>
              <ChevronDown size={14} />
            </span>
          </button>
        {:else if !searchQuery}
          <div class="group-sep"></div>
        {/if}

        {#if uiStore.sidebarCollapsed || expandedGroups[group.label] || searchQuery}
          <div class="group-items">
            {#each filteredItems as item}
              {@const isActive = uiStore.activeTab === item.id}
              <button
                class="nav-item"
                class:active={isActive}
                onclick={() => { uiStore.setActiveTab(item.id); searchQuery = ''; }}
                title={uiStore.sidebarCollapsed ? item.label : ''}
                aria-current={isActive ? 'page' : undefined}
              >
                <span class="nav-icon" class:active={isActive}>
                  <item.icon size={16} />
                </span>
                {#if !uiStore.sidebarCollapsed}
                  <span class="nav-label">{item.label}</span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    {/each}
  </nav>

  <div class="sidebar-spacer"></div>

  <!-- Collapse toggle -->
  <button
    class="collapse-btn"
    onclick={() => uiStore.toggleSidebar()}
    title={uiStore.sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
  >
    {#if uiStore.sidebarCollapsed}
      <ChevronRight size={15} />
    {:else}
      <ChevronLeft size={15} />
      <span>Collapse</span>
    {/if}
  </button>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 220px;
    min-width: 220px;
    height: 100%;
    background: var(--color-sidebar-bg);
    border-right: 1px solid var(--color-sidebar-border);
    transition: width 0.22s cubic-bezier(0.4, 0, 0.2, 1),
                min-width 0.22s cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
    padding: 12px 8px;
    gap: 0;
  }

  .sidebar.collapsed {
    width: 52px;
    min-width: 52px;
    padding: 12px 6px;
  }

  /* ── Logo ─────────────────────────────────────────────────────────── */
  .sidebar-logo {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 12px;
    overflow: hidden;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .logo-icon {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: linear-gradient(135deg, rgba(99,102,241,0.25), rgba(79,70,229,0.1));
    border: 1px solid rgba(99,102,241,0.25);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .logo-icon svg { width: 16px; height: 16px; }

  .logo-text {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .logo-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
    line-height: 1.2;
  }

  .logo-version {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .sidebar-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.05);
    margin: 0 2px 8px;
    flex-shrink: 0;
  }

  .sidebar-search {
    margin: 4px 10px 12px;
    position: relative;
    display: flex;
    align-items: center;
    flex-shrink: 0;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.02);
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .sidebar-search:focus-within {
    background: rgba(0, 0, 0, 0.35);
    border-color: rgba(255, 255, 255, 0.08);
    box-shadow: inset 0 1px 3px rgba(0,0,0,0.3);
  }
  .sidebar-search :global(.search-icon) {
    position: absolute;
    left: 10px;
    color: var(--color-text-muted);
    transition: color 0.2s ease;
    pointer-events: none;
    opacity: 0.7;
  }
  .sidebar-search:focus-within :global(.search-icon) {
    color: var(--color-text-primary);
    opacity: 1;
  }
  .sidebar-search input {
    width: 100%;
    background: transparent;
    border: none;
    padding: 8px 10px 8px 32px;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    outline: none;
  }
  .sidebar-search input::placeholder {
    color: var(--color-text-muted);
    font-weight: 400;
  }

  /* ── Grouped nav ──────────────────────────────────────────────────── */
  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .group-label-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 12px 8px 6px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--color-text-muted);
    transition: color 0.15s ease;
    flex-shrink: 0;
  }

  .group-label-btn:hover {
    color: var(--color-text-primary);
  }

  .group-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    flex-shrink: 0;
  }

  .group-chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    color: var(--color-text-muted);
  }

  .group-label-btn:hover .group-chevron {
    color: var(--color-text-primary);
  }

  .group-chevron.expanded {
    transform: rotate(180deg);
  }

  .group-items {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .group-sep {
    height: 6px;
  }

  /* ── Nav item ─────────────────────────────────────────────────────── */
  .nav-item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border-radius: 7px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    font-family: var(--font-sans);
    white-space: nowrap;
    overflow: hidden;
    transition: background 0.15s ease, color 0.15s ease;
    text-align: left;
    position: relative;
    min-height: 34px;
    width: 100%;
  }

  .nav-item:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-primary);
  }

  .nav-item.active {
    background: var(--color-active-bg);
    color: var(--color-text-primary);
  }

  .nav-icon {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: color 0.15s ease;
  }

  .nav-icon.active {
    color: var(--color-text-primary);
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* svelte-ignore a11y-no-static-element-interactions */
  /* ── Bottom ───────────────────────────────────────────────────────── */
  .sidebar-spacer { flex: 0; min-height: 8px; }

  .collapse-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 7px 8px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    background: transparent;
    color: var(--color-text-muted);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    overflow: hidden;
    flex-shrink: 0;
    width: 100%;
  }

  .collapse-btn:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-primary);
    border-color: rgba(255, 255, 255, 0.1);
  }
</style>
