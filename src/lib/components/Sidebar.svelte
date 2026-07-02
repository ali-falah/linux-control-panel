<script lang="ts">
  import {
    Package, History, LayoutGrid, Layers, Rocket, Settings2, Globe,
    Users, Shield, Cpu, ShieldCheck, Clock, FileText, Server,
    ChevronLeft, ChevronRight, Database
  } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';

  const groups: {
    label: string;
    items: { id: TabId; label: string; icon: any }[];
  }[] = [
    {
      label: 'Packages',
      items: [
        { id: 'repo-manager',  label: 'Repo Manager',   icon: Database },
        { id: 'dnf-history',   label: 'DNF Manager',    icon: Package },
        { id: 'copr-browser',  label: 'Copr Browser',   icon: LayoutGrid },
        { id: 'flatpak-rpm',   label: 'Flatpak vs RPM', icon: Layers },
      ],
    },
    {
      label: 'System',
      items: [
        { id: 'startup-manager', label: 'Startup Manager', icon: Rocket },
        { id: 'service-manager', label: 'Service Manager', icon: Settings2 },
        { id: 'grub-manager',    label: 'GRUB Bootloader', icon: Cpu },
        { id: 'selinux-manager', label: 'SELinux Manager', icon: ShieldCheck },
      ],
    },
    {
      label: 'Network & Security',
      items: [
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
        <span class="logo-version">v0.1.0</span>
      </div>
    {/if}
  </div>

  <div class="sidebar-divider"></div>

  <!-- Grouped Navigation -->
  <nav class="sidebar-nav">
    {#each groups as group}
      {#if !uiStore.sidebarCollapsed}
        <div class="group-label">{group.label}</div>
      {:else}
        <div class="group-sep"></div>
      {/if}

      {#each group.items as item}
        {@const isActive = uiStore.activeTab === item.id}
        <button
          class="nav-item"
          class:active={isActive}
          onclick={() => uiStore.setActiveTab(item.id)}
          title={uiStore.sidebarCollapsed ? item.label : ''}
          aria-current={isActive ? 'page' : undefined}
        >
          <span class="nav-icon" class:active={isActive}>
            <item.icon size={16} />
          </span>
          {#if !uiStore.sidebarCollapsed}
            <span class="nav-label">{item.label}</span>
            {#if isActive}
              <span class="nav-active-dot"></span>
            {/if}
          {/if}
        </button>
      {/each}
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
    background: rgba(12, 12, 20, 0.75);
    border-right: 1px solid rgba(255, 255, 255, 0.05);
    backdrop-filter: blur(16px);
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

  /* ── Grouped nav ──────────────────────────────────────────────────── */
  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    /* hide scrollbar */
    scrollbar-width: none;
  }
  .sidebar-nav::-webkit-scrollbar { display: none; }

  .group-label {
    padding: 10px 8px 4px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #475569;
    white-space: nowrap;
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
    background: rgba(99, 102, 241, 0.15);
    color: #a5b4fc;
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
    color: var(--color-accent);
  }

  .nav-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .nav-active-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--color-accent);
    flex-shrink: 0;
    box-shadow: 0 0 6px var(--color-accent-glow);
  }

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
