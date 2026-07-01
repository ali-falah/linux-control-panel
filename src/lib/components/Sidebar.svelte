<script lang="ts">
  import {
    Package, History, LayoutGrid, Layers, Rocket, Settings, Globe, Users, Shield, TerminalSquare, ShieldAlert,
    Clock, FileText, ChevronLeft, ChevronRight
  } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';

  const tabs: { id: TabId; label: string; icon: any; description: string }[] = [
    { id: 'repo-manager',     label: 'Repo Manager',     icon: Package,    description: 'Manage DNF repositories' },
    { id: 'dnf-history',      label: 'DNF Manager',      icon: History,    description: 'View & rollback transactions' },
    { id: 'copr-browser',     label: 'Copr Browser',     icon: LayoutGrid, description: 'Browse Fedora Copr projects' },
    { id: 'flatpak-rpm',      label: 'Flatpak vs RPM',   icon: Layers,     description: 'Detect duplicate packages' },
    { id: 'startup-manager',  label: 'Startup Manager',  icon: Rocket,     description: 'Manage autostart entries' },
    { id: 'service-manager',  label: 'Service Manager',  icon: Settings,   description: 'Manage systemd services' },
    { id: 'hosts-manager',    label: 'Hosts Manager',    icon: Globe,      description: 'Edit /etc/hosts entries' },
    { id: 'user-manager',     label: 'Users & Groups',   icon: Users,      description: 'Manage users & groups' },
    { id: 'firewall-manager', label: 'Firewall Manager', icon: Shield,     description: 'Manage firewalld rules' },
    { id: 'grub-manager',     label: 'GRUB Bootloader',  icon: TerminalSquare, description: 'Configure boot menu' },
    { id: 'selinux-manager',  label: 'SELinux Manager',  icon: ShieldAlert,description: 'Security policies & denials' },
    { id: 'cron-manager',     label: 'Scheduled Tasks',  icon: Clock,      description: 'Manage cron jobs' },
    { id: 'env-manager',      label: 'Global Environment',icon: FileText,   description: 'Edit /etc/environment' },
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
        <span class="logo-version">v0.1.0</span>
      </div>
    {/if}
  </div>

  <div class="sidebar-divider"></div>

  <!-- Navigation -->
  <nav class="sidebar-nav">
    {#each tabs as tab}
      {@const isActive = uiStore.activeTab === tab.id}
      <button
        class="nav-item"
        class:active={isActive}
        onclick={() => uiStore.setActiveTab(tab.id)}
        title={uiStore.sidebarCollapsed ? tab.label : ''}
        aria-current={isActive ? 'page' : undefined}
      >
        <span class="nav-icon" class:active={isActive}>
          <tab.icon size={18} />
        </span>
        {#if !uiStore.sidebarCollapsed}
          <span class="nav-label">{tab.label}</span>
          {#if isActive}
            <span class="nav-active-dot"></span>
          {/if}
        {/if}
      </button>
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
      <ChevronRight size={16} />
    {:else}
      <ChevronLeft size={16} />
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
    transition: width 0.25s cubic-bezier(0.4, 0, 0.2, 1),
                min-width 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
    padding: 12px 8px;
    gap: 2px;
  }

  .sidebar.collapsed {
    width: 56px;
    min-width: 56px;
    padding: 12px 8px;
  }

  .sidebar-logo {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 6px 12px;
    overflow: hidden;
    white-space: nowrap;
  }

  .logo-icon {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .logo-icon svg {
    width: 18px;
    height: 18px;
  }

  .logo-text {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .logo-title {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }

  .logo-version {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .sidebar-divider {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0 8px;
    flex-shrink: 0;
  }

  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 8px;
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
    min-height: 38px;
  }

  .nav-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .nav-item.active {
    background: var(--color-active-bg);
    color: var(--color-text-accent);
  }

  .nav-icon {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
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

  .nav-active-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-accent);
    flex-shrink: 0;
  }

  .sidebar-spacer { flex: 0; }

  .collapse-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text-muted);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    overflow: hidden;
    margin-top: 8px;
    flex-shrink: 0;
  }

  .collapse-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
    border-color: var(--color-border-hover);
  }
</style>
