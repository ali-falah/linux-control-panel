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
  let firstResultEl = $state<HTMLButtonElement | null>(null);

  function handleSearchKeydown(e: KeyboardEvent) {
    if (e.key === 'Tab' && searchQuery.trim()) {
      if (firstResultEl) {
        e.preventDefault();
        firstResultEl.focus();
      }
    }
  }

  // Svelte action to conditionally capture a ref for the first search result
  function captureFirstResult(node: HTMLButtonElement, isFirst: boolean) {
    if (isFirst) firstResultEl = node;
    return {
      update(newIsFirst: boolean) {
        if (newIsFirst) firstResultEl = node;
        else if (firstResultEl === node) firstResultEl = null;
      },
      destroy() {
        if (firstResultEl === node) firstResultEl = null;
      }
    };
  }

  let expandedGroup = $state<string | null>('Overview');

  $effect(() => {
    if (uiStore.activeTab) {
      for (const group of groups) {
        if (group.items.some(i => i.id === uiStore.activeTab)) {
          expandedGroup = group.label;
        }
      }
    }
  });

  function toggleGroup(label: string) {
    expandedGroup = expandedGroup === label ? null : label;
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
        { id: 'security-auditor', label: 'Security Auditor', icon: ShieldCheck },
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
  <button class="sidebar-logo" onclick={() => uiStore.toggleSidebar()} title={uiStore.sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}>
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
      <div class="logo-chevron">
        <ChevronLeft size={16} />
      </div>
    {/if}
  </button>

  <div class="sidebar-divider"></div>

  <!-- Search -->
  {#if !uiStore.sidebarCollapsed}
    <div class="sidebar-search">
      <Search size={14} style="color: var(--color-text-muted); opacity: 0.7; flex-shrink: 0;" />
      <input
        type="text"
        placeholder="Search..."
        bind:value={searchQuery}
        onkeydown={handleSearchKeydown}
      />
    </div>
  {/if}

  <!-- Grouped Navigation -->
  <nav class="sidebar-nav">
    {#each groups as group}
      {@const filteredItems = group.items.filter(i => i.label.toLowerCase().includes(searchQuery.toLowerCase()))}
      {@const isGroupActive = group.items.some(i => i.id === uiStore.activeTab)}
      {#if filteredItems.length > 0}
        {#if !uiStore.sidebarCollapsed && !searchQuery}
          <button 
            class="group-label-btn" 
            class:active-group={isGroupActive} 
            onclick={() => toggleGroup(group.label)}
          >
            <span class="group-label">{group.label}</span>
            <span class="group-chevron" class:expanded={expandedGroup === group.label} class:active-chevron={isGroupActive}>
              <ChevronDown size={14} />
            </span>
          </button>
        {:else if !searchQuery}
          <div class="group-sep"></div>
        {/if}

        <div class="group-items-wrapper" class:collapsed-anim={!uiStore.sidebarCollapsed && expandedGroup !== group.label && !searchQuery}>
          <div class="group-items">
            {#each filteredItems as item, itemIdx}
              {@const isActive = uiStore.activeTab === item.id}
              {@const isFirstResult = searchQuery.trim() && itemIdx === 0}
              <button
                class="nav-item"
                class:active={isActive}
                onclick={() => { uiStore.setActiveTab(item.id); searchQuery = ''; }}
                title={uiStore.sidebarCollapsed ? item.label : ''}
                aria-current={isActive ? 'page' : undefined}
                use:captureFirstResult={isFirstResult}
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
        </div>
      {/if}
    {/each}
  </nav>

  <div class="sidebar-spacer"></div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 220px;
    min-width: 220px;
    height: 100%;
    background: var(--color-sidebar-bg);
    border-right: 1px solid rgba(59, 73, 76, 0.5);
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
    width: 100%;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    outline: none;
    border-radius: 6px;
    transition: background 0.15s ease;
  }
  .sidebar-logo:hover {
    background: rgba(0, 218, 243, 0.04);
  }
  .logo-chevron {
    margin-left: auto;
    color: var(--color-text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease, transform 0.2s ease;
  }
  .sidebar-logo:hover .logo-chevron {
    color: var(--color-accent);
  }

  .logo-icon {
    width: 30px;
    height: 30px;
    border-radius: 6px;
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.22);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
    box-shadow: 0 0 10px rgba(0, 218, 243, 0.10);
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
    letter-spacing: 0.04em;
  }

  .sidebar-divider {
    height: 1px;
    background: rgba(59, 73, 76, 0.45);
    margin: 0 2px 8px;
    flex-shrink: 0;
  }

  /* ── Search ───────────────────────────────────────────────────────── */
  .sidebar-search {
    margin: 4px 6px 12px;
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    flex-shrink: 0;
    background: rgba(1, 15, 31, 0.6);
    border-radius: 4px;
    border: 1px solid rgba(59, 73, 76, 0.5);
    transition: all 0.18s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .sidebar-search:hover {
    border-color: rgba(59, 73, 76, 0.8);
  }
  .sidebar-search:focus-within {
    background: rgba(1, 15, 31, 0.8);
    border-color: var(--color-accent);
    box-shadow:
      inset 0 1px 3px rgba(0, 0, 0, 0.3),
      0 0 0 2px rgba(0, 218, 243, 0.08),
      0 0 8px rgba(0, 218, 243, 0.1);
  }
  .sidebar-search input,
  .sidebar-search input:focus,
  .sidebar-search input:hover {
    width: 100%;
    background: transparent !important;
    border: none !important;
    box-shadow: none !important;
    backdrop-filter: none !important;
    -webkit-backdrop-filter: none !important;
    padding: 8px 0 !important;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    outline: none !important;
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
    padding: 12px 8px 5px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--color-text-muted);
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .group-label-btn.active-group {
    color: var(--color-text-primary);
  }

  .group-label-btn.active-group .group-label {
    font-weight: 800;
    color: var(--color-text-primary);
    text-shadow: 0 0 12px rgba(212, 228, 250, 0.15);
  }

  .group-label-btn.active-group .group-chevron {
    color: var(--color-accent) !important;
    opacity: 0.95 !important;
  }

  .group-label-btn:hover {
    color: var(--color-text-secondary);
  }

  .group-label {
    /* label-caps typography */
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    flex-shrink: 0;
    font-family: var(--font-sans);
  }

  .group-chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    color: var(--color-text-muted);
    opacity: 0.6;
  }

  .group-label-btn:hover .group-chevron {
    opacity: 1;
  }

  .group-chevron.expanded {
    transform: rotate(180deg);
  }

  .group-items-wrapper {
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 0.22s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .group-items-wrapper.collapsed-anim {
    grid-template-rows: 0fr;
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
    padding: 7px 8px 7px 18px; /* Slightly indented for layout hierarchy */
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 12.5px;
    font-weight: 500;
    font-family: var(--font-sans);
    white-space: nowrap;
    overflow: hidden;
    transition: background 0.15s ease, color 0.15s ease, padding 0.2s ease;
    text-align: left;
    position: relative;
    min-height: 34px;
    width: 100%;
  }

  .sidebar.collapsed .nav-item {
    padding: 7px 8px 7px 11px; /* Centered padding for collapsed view */
  }

  /* Left pill indicator for active state */
  .nav-item::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%) scaleY(0);
    width: 3px;
    height: 60%;
    border-radius: 0 9999px 9999px 0;
    background: var(--color-accent);
    box-shadow: 0 0 6px var(--color-accent-glow);
    transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1),
                opacity 0.2s ease;
    opacity: 0;
  }

  .nav-item:hover {
    background: rgba(0, 218, 243, 0.06);
    color: var(--color-text-secondary);
  }

  /* Keyboard focus — shown after Tab from search input */
  .nav-item:focus-visible {
    outline: none;
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-accent);
    box-shadow: inset 0 0 0 1.5px rgba(0, 218, 243, 0.45);
  }

  .nav-item:focus-visible .nav-icon {
    color: var(--color-accent);
  }

  .nav-item.active {
    background: var(--color-active-bg);
    color: var(--color-accent-soft);
  }

  .nav-item.active::before {
    transform: translateY(-50%) scaleY(1);
    opacity: 1;
  }

  .nav-icon {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: color 0.15s ease;
  }

  .nav-icon.active {
    color: var(--color-accent);
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Bottom ───────────────────────────────────────────────────────── */
  .sidebar-spacer { flex: 0; min-height: 8px; }

</style>
