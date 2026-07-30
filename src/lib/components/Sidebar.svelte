<script lang="ts">
  import { Package, History, LayoutGrid, Layers, Settings2, Globe } from '@lucide/svelte';
  import { Users, Shield, Cpu, ShieldCheck, Clock, FileText, Server } from '@lucide/svelte';
  import { ChevronLeft, ChevronRight, Database, Terminal, ChevronDown } from '@lucide/svelte';
  import { HardDrive, Wifi, Activity, Search, LayoutDashboard } from '@lucide/svelte';
  import { Sun, Moon, Settings } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';

  let appVersion = $state('...');
  let currentUsername = $state('user');
  let userInitial = $derived(currentUsername.charAt(0).toUpperCase());
  
  $effect(() => {
    getVersion().then(v => appVersion = `v${v}`).catch(() => appVersion = 'v0.0.0');
    invoke<string>('get_current_user')
      .then(u => { if (u) currentUsername = u; })
      .catch(() => {});
  });

  let searchQuery = $state('');
  let firstResultEl = $state<HTMLButtonElement | null>(null);
  let showSettingsPopover = $state(false);
  let settingsPopoverRef = $state<HTMLDivElement | null>(null);

  // Watch for click outside settings popover menu
  $effect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (showSettingsPopover && settingsPopoverRef && !settingsPopoverRef.contains(e.target as Node)) {
        showSettingsPopover = false;
      }
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  });

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

  let hoveredGroup = $state<string | null>(null);
  let hoverTimeout: any = null;

  function handleGroupMouseEnter(groupLabel: string) {
    if (!uiStore.sidebarCollapsed) return;
    if (hoverTimeout) clearTimeout(hoverTimeout);
    hoveredGroup = groupLabel;
  }

  function handleGroupMouseLeave() {
    if (!uiStore.sidebarCollapsed) return;
    hoverTimeout = setTimeout(() => {
      hoveredGroup = null;
    }, 180);
  }

  function handleFlyoutMouseEnter() {
    if (hoverTimeout) clearTimeout(hoverTimeout);
  }

  function handleFlyoutMouseLeave() {
    handleGroupMouseLeave();
  }

  const groups: {
    label: string;
    icon: any;
    items: { id: TabId; label: string; icon: any; desc?: string }[];
  }[] = [
    {
      label: 'Overview',
      icon: LayoutDashboard,
      items: [
        { id: 'system-dashboard', label: 'Dashboard',  icon: LayoutDashboard, desc: 'System analytics overview' },
        { id: 'system-monitor',   label: 'Monitoring', icon: Activity, desc: 'Metrics & telemetry graphs' },
      ],
    },
    {
      label: 'Packages',
      icon: Package,
      items: [
        { id: 'app-manager',   label: 'App Manager',    icon: LayoutGrid, desc: 'Flatpak & RPM applications' },
        { id: 'repo-manager',  label: 'Repo Manager',   icon: Database, desc: 'Software repositories' },
        { id: 'dnf-history',   label: 'DNF Manager',    icon: Package, desc: 'Package history & updates' },
        { id: 'copr-browser',  label: 'Copr Browser',   icon: Layers, desc: 'Fedora Copr repositories' },
      ],
    },
    {
      label: 'System',
      icon: Server,
      items: [
        { id: 'journal-logs',    label: 'Journal Logs',    icon: FileText, desc: 'Systemd Journal logs' },
        { id: 'service-manager', label: 'Service Manager', icon: Settings2, desc: 'System units & daemons' },
        { id: 'device-manager',  label: 'Device Manager',  icon: HardDrive, desc: 'Disks, SMART & hardware' },
        { id: 'grub-manager',    label: 'GRUB Bootloader', icon: Cpu, desc: 'Boot entries & kernel params' },
        { id: 'selinux-manager', label: 'SELinux Manager', icon: ShieldCheck, desc: 'Security policies & contexts' },
      ],
    },
    {
      label: 'Network & Security',
      icon: Wifi,
      items: [
        { id: 'network-manager',  label: 'Advanced Network', icon: Wifi, desc: 'Interfaces, IP & DNS' },
        { id: 'hosts-manager',    label: 'Hosts Manager',    icon: Globe, desc: 'Local DNS & /etc/hosts' },
        { id: 'firewall-manager', label: 'Firewall Manager', icon: Shield, desc: 'UFW & Firewalld rules' },
        { id: 'security-auditor', label: 'Security Auditor', icon: ShieldCheck, desc: 'CIS & STIG hardening' },
        { id: 'nginx-manager',    label: 'Nginx Manager',    icon: Server, desc: 'Web server & vhosts' },
      ],
    },
    {
      label: 'Users & Config',
      icon: Users,
      items: [
        { id: 'user-manager', label: 'Users & Groups',    icon: Users, desc: 'User accounts & privileges' },
        { id: 'env-manager',  label: 'Environment',       icon: FileText, desc: 'System environment vars' },
        { id: 'shell-env',    label: 'Shell Environment', icon: Terminal, desc: 'Zsh / Bash profile config' },
        { id: 'cron-manager', label: 'Scheduled Tasks',   icon: Clock, desc: 'Crontab jobs & timers' },
      ],
    },
  ];

  function getItemByTabId(id: TabId) {
    for (const g of groups) {
      const match = g.items.find(i => i.id === id);
      if (match) return match;
    }
    return null;
  }
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

  <!-- Search Input -->
  {#if !uiStore.sidebarCollapsed}
    <div class="sidebar-search">
      <Search size={14} class="search-icon" />
      <input
        type="text"
        placeholder="Filter modules…"
        bind:value={searchQuery}
        onkeydown={handleSearchKeydown}
      />
    </div>
  {/if}

  <!-- Grouped Navigation -->
  <nav class="sidebar-nav">
    {#if uiStore.sidebarCollapsed}
      <!-- Collapsed Mode: Category Icons with Nested Flyout Menu -->
      <div class="collapsed-categories-list">
        {#each groups as group}
          {@const isGroupActive = group.items.some(i => i.id === uiStore.activeTab)}
          <div
            class="collapsed-cat-wrapper"
            onmouseenter={() => handleGroupMouseEnter(group.label)}
            onmouseleave={handleGroupMouseLeave}
          >
            <button
              class="collapsed-cat-btn"
              class:active={isGroupActive}
              onclick={() => {
                uiStore.setActiveTab(group.items[0].id);
              }}
              aria-label={group.label}
            >
              <span class="collapsed-cat-icon" class:active={isGroupActive}>
                <group.icon size={18} />
              </span>
            </button>

            <!-- Nested Flyout Menu Panel -->
            {#if hoveredGroup === group.label}
              <div
                class="flyout-menu-panel"
                onmouseenter={handleFlyoutMouseEnter}
                onmouseleave={handleFlyoutMouseLeave}
                role="menu"
              >
                <div class="flyout-arrow"></div>
                <div class="flyout-header">
                  <span class="flyout-title">{group.label} Modules</span>
                </div>
                <div class="flyout-items">
                  {#each group.items as item}
                    {@const isItemActive = uiStore.activeTab === item.id}
                    <button
                      class="flyout-item"
                      class:active={isItemActive}
                      onclick={() => {
                        if (item.id === ('theme-settings' as any)) {
                          uiStore.toggleTheme();
                        } else {
                          uiStore.setActiveTab(item.id);
                        }
                        hoveredGroup = null;
                      }}
                      role="menuitem"
                    >
                      <span class="flyout-item-icon" class:active={isItemActive}>
                        <item.icon size={15} />
                      </span>
                      <div class="flyout-item-text">
                        <span class="flyout-item-label">{item.label}</span>
                        {#if item.desc}
                          <span class="flyout-item-desc">{item.desc}</span>
                        {/if}
                      </div>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}

        <!-- Collapsed Settings Gear Button -->
        <div class="collapsed-cat-divider"></div>
        <div class="collapsed-cat-wrapper">
          <button
            class="collapsed-cat-btn gear-btn"
            class:active={showSettingsPopover}
            onclick={() => showSettingsPopover = !showSettingsPopover}
            aria-label="Settings"
            title="Settings & Preferences"
          >
            <span class="collapsed-cat-icon">
              <Settings size={18} />
            </span>
          </button>
        </div>
      </div>
    {:else}
      <!-- Expanded Mode -->
      {#each groups as group}
        {@const filteredItems = group.items.filter(i => i.label.toLowerCase().includes(searchQuery.toLowerCase()))}
        {@const isGroupActive = group.items.some(i => i.id === uiStore.activeTab)}
        {#if filteredItems.length > 0}
          {#if !searchQuery}
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
          {:else}
            <div class="group-sep"></div>
          {/if}

          <div class="group-items-wrapper" class:collapsed-anim={expandedGroup !== group.label && !searchQuery}>
            <div class="group-items">
              {#each filteredItems as item, itemIdx}
                {@const isActive = uiStore.activeTab === item.id}
                {@const isFirstResult = searchQuery.trim() && itemIdx === 0}
                <button
                  class="nav-item"
                  class:active={isActive}
                  onclick={() => { uiStore.setActiveTab(item.id); searchQuery = ''; }}
                  aria-current={isActive ? 'page' : undefined}
                  use:captureFirstResult={isFirstResult}
                >
                  <span class="nav-icon" class:active={isActive}>
                    <item.icon size={16} />
                  </span>
                  <span class="nav-label">{item.label}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/each}
    {/if}
  </nav>

  <!-- Bottom Profile & Gear Button Row with Floating Popover Menu (Gemini Style) -->
  <div class="sidebar-settings-anchor" bind:this={settingsPopoverRef}>
    {#if !uiStore.sidebarCollapsed}
      <div 
        class="profile-settings-row"
        onclick={() => showSettingsPopover = !showSettingsPopover}
        role="button"
        tabindex="0"
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') showSettingsPopover = !showSettingsPopover; }}
      >
        <div class="profile-info">
          <div class="avatar-circle">{userInitial}</div>
          <div class="profile-text">
            <span class="profile-name">{currentUsername}</span>
            <span class="profile-role">System Admin</span>
          </div>
        </div>
        <button 
          class="gear-btn" 
          class:active={showSettingsPopover}
          onclick={(e) => { e.stopPropagation(); showSettingsPopover = !showSettingsPopover; }}
          title="Settings & Preferences"
          aria-label="Settings"
        >
          <Settings size={18} />
        </button>
      </div>
    {/if}

    <!-- Floating Popover Menu -->
    {#if showSettingsPopover}
      <div class="settings-popover-menu" role="menu">
        <div class="popover-header">
          <div style="display:flex; align-items:center; gap:8px;">
            <div class="avatar-circle small">{userInitial}</div>
            <div style="display:flex; flex-direction:column;">
              <span class="popover-user-name">{currentUsername}</span>
              <span class="popover-user-sub">Active Session</span>
            </div>
          </div>
        </div>

        <div class="popover-section">
          <button
            class="popover-menu-item"
            onclick={() => uiStore.toggleTheme()}
            role="menuitem"
          >
            <span class="popover-item-icon">
              {#if uiStore.theme === 'dark'}
                <Moon size={15} />
              {:else}
                <Sun size={15} class="sun-icon" />
              {/if}
            </span>
            <div class="popover-item-text">
              <span class="popover-item-label">
                Theme: {uiStore.theme === 'dark' ? 'Dark Mode' : 'Light Mode'}
              </span>
              <span class="popover-item-desc">
                {uiStore.theme === 'dark' ? 'Obsidian Terminal' : 'Eye-Comfort Cream'}
              </span>
            </div>
            <div class="theme-pill-dot" class:light={uiStore.theme === 'light'}></div>
          </button>
        </div>

        <div class="popover-divider"></div>

        <button
          class="popover-menu-item"
          onclick={() => {
            showSettingsPopover = false;
            uiStore.openSettingsModal();
          }}
          role="menuitem"
        >
          <span class="popover-item-icon">
            <Settings size={15} />
          </span>
          <div class="popover-item-text">
            <span class="popover-item-label">All Settings Dialog...</span>
            <span class="popover-item-desc">Preferences & options</span>
          </div>
        </button>
      </div>
    {/if}
  </div>
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
    overflow: visible;
  }

  .sidebar.collapsed .sidebar-nav {
    overflow: visible;
  }

  /* ── Collapsed Category List ───────────────────────────────────────── */
  .collapsed-categories-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 6px;
    align-items: center;
    width: 100%;
  }

  .collapsed-cat-wrapper {
    position: relative;
    width: 100%;
    display: flex;
    justify-content: center;
  }

  .collapsed-cat-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    position: relative;
    transition: all 0.18s ease;
  }

  .collapsed-cat-btn:hover {
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-text-primary);
  }

  .collapsed-cat-btn.active {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
  }

  /* Left vertical blue pill indicator for active group */
  .collapsed-cat-btn.active::before {
    content: '';
    position: absolute;
    left: -6px;
    top: 50%;
    transform: translateY(-50%);
    width: 3.5px;
    height: 20px;
    border-radius: 0 4px 4px 0;
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
  }

  .collapsed-cat-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: color 0.15s ease, transform 0.15s ease;
  }

  .collapsed-cat-btn:hover .collapsed-cat-icon,
  .collapsed-cat-icon.active {
    color: var(--color-accent);
    transform: scale(1.05);
  }

  /* ── Nested Flyout Menu Panel ─────────────────────────────────────── */
  .flyout-menu-panel {
    position: absolute;
    left: calc(100% + 10px);
    top: -4px;
    z-index: 1000;
    min-width: 210px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-active-border);
    border-radius: 10px;
    padding: 6px;
    box-shadow: 0 10px 32px rgba(0, 0, 0, 0.45), 0 0 12px var(--color-accent-glow);
    backdrop-filter: blur(12px);
    animation: flyout-appear 0.15s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes flyout-appear {
    from { opacity: 0; transform: translateX(-6px) scale(0.97); }
    to   { opacity: 1; transform: translateX(0) scale(1); }
  }

  .flyout-arrow {
    position: absolute;
    left: -6px;
    top: 14px;
    width: 10px;
    height: 10px;
    background: var(--color-bg-card);
    border-left: 1px solid var(--color-active-border);
    border-bottom: 1px solid var(--color-active-border);
    transform: rotate(45deg);
  }

  .flyout-header {
    padding: 6px 10px 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    margin-bottom: 4px;
  }

  .flyout-title {
    font-size: 10px;
    font-weight: 700;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-family: var(--font-sans);
  }

  .flyout-items {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .flyout-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-secondary);
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: all 0.12s ease;
    font-family: var(--font-sans);
  }

  .flyout-item:hover {
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-text-primary);
  }

  .flyout-item.active {
    background: rgba(0, 218, 243, 0.14);
    color: var(--color-accent);
    font-weight: 600;
  }

  .flyout-item-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: color 0.12s ease;
    flex-shrink: 0;
  }

  .flyout-item:hover .flyout-item-icon,
  .flyout-item-icon.active {
    color: var(--color-accent);
  }

  .flyout-item-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
  }

  .flyout-item-label {
    font-size: 12px;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .flyout-item-desc {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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

  /* ── Bottom Settings Profile & Gear Popover ───────────────────────── */
  .sidebar-settings-anchor {
    position: relative;
    width: 100%;
    margin-top: auto;
    padding: 8px 4px 4px;
    border-top: 1px solid var(--color-sidebar-border);
    background: var(--color-sidebar-bg);
    flex-shrink: 0;
    z-index: 200;
  }

  .profile-settings-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 6px;
    border-radius: 8px;
    transition: background 0.15s ease;
  }

  .profile-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .avatar-circle.small {
    width: 24px;
    height: 24px;
    font-size: 11px;
  }

  .popover-header {
    padding: 6px 8px 8px;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .popover-user-name {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.2;
  }

  .popover-user-sub {
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .popover-section {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 4px;
  }

  .popover-menu-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border-radius: 8px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    cursor: pointer;
    text-align: left;
    transition: background 0.15s ease;
  }

  .popover-menu-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }

  .popover-item-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
  }

  .popover-item-icon.sun-icon {
    color: #CB854F;
  }

  .popover-item-text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .popover-item-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .popover-item-desc {
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .theme-pill-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-accent);
  }

  .theme-pill-dot.light {
    background: #CB854F;
  }

  .popover-divider {
    height: 1px;
    background: var(--color-border-subtle);
    margin: 4px 0;
  }

  .collapsed-cat-divider {
    width: 24px;
    height: 1px;
    background: var(--color-sidebar-border);
    margin: 6px 0;
  }
</style>
