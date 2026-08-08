<script lang="ts">
  import { Search, X, LayoutDashboard, Activity, Server, Package, Layers, Globe, Shield, ShieldAlert, FileText, HardDrive, Terminal, Sliders, Lock, Cpu, User, FolderLock, Settings, Sparkles, Sun, Moon, ArrowRight, Zap, RefreshCw } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface SearchItem {
    id: string;
    title: string;
    description: string;
    category: 'Pages' | 'Actions' | 'Tools';
    icon: any;
    keywords: string;
    action: () => void;
  }

  let searchQuery = $state('');
  let selectedIndex = $state(0);
  let searchInputRef = $state<HTMLInputElement | null>(null);

  const searchItems: SearchItem[] = [
    // ── Pages & Modules ──────────────────────────────────────────────────────────
    {
      id: 'system-dashboard',
      title: 'System Overview Dashboard',
      description: 'Main system metrics, CPU/RAM usage, and quick status panel',
      category: 'Pages',
      icon: LayoutDashboard,
      keywords: 'overview dashboard summary status home',
      action: () => navigateTo('system-dashboard')
    },
    {
      id: 'system-monitor',
      title: 'System Monitor & Processes',
      description: 'Real-time CPU/RAM/Swap sparklines, disk usage, and process killer',
      category: 'Pages',
      icon: Activity,
      keywords: 'monitor process cpu ram memory kill stats graph',
      action: () => navigateTo('system-monitor')
    },
    {
      id: 'service-manager',
      title: 'Services & Systemd Manager',
      description: 'Manage systemd services, start, stop, restart, enable, and view logs',
      category: 'Pages',
      icon: Server,
      keywords: 'service systemd daemon start stop restart unit status',
      action: () => navigateTo('service-manager')
    },
    {
      id: 'app-manager',
      title: 'Applications & Software Manager',
      description: 'Installed RPM packages, Flatpaks, AppImages, and app permissions',
      category: 'Pages',
      icon: Package,
      keywords: 'apps software flatpak rpm appimage install uninstall',
      action: () => navigateTo('app-manager')
    },
    {
      id: 'repo-manager',
      title: 'RPM Repositories Manager',
      description: 'Enable, disable, or add DNF YUM repository files',
      category: 'Pages',
      icon: Layers,
      keywords: 'repo dnf yum repository copr fedora enable disable',
      action: () => navigateTo('repo-manager')
    },
    {
      id: 'dnf-history',
      title: 'DNF Package History',
      description: 'Audit DNF package install/update history and undo transactions',
      category: 'Pages',
      icon: Package,
      keywords: 'dnf history transaction undo rollback update install',
      action: () => navigateTo('dnf-history')
    },
    {
      id: 'copr-browser',
      title: 'COPR Repositories Browser',
      description: 'Discover and enable Fedora COPR community package builds',
      category: 'Pages',
      icon: Globe,
      keywords: 'copr fedora community repository search build',
      action: () => navigateTo('copr-browser')
    },
    {
      id: 'network-manager',
      title: 'Network & Interfaces',
      description: 'Network interfaces, IP addresses, ping test, DNS, and VPN profiles',
      category: 'Pages',
      icon: Globe,
      keywords: 'network interface ip address ethernet wifi vpn ping dns',
      action: () => navigateTo('network-manager')
    },
    {
      id: 'nginx-manager',
      title: 'NGINX Web Server Manager',
      description: 'Manage virtual hosts, reverse proxies, SSL certs, and access logs',
      category: 'Pages',
      icon: Server,
      keywords: 'nginx web server virtualhost reverse proxy ssl certbot log',
      action: () => navigateTo('nginx-manager')
    },
    {
      id: 'firewall-manager',
      title: 'Firewall & Firewalld Rules',
      description: 'Firewalld zones, open ports, rich rules, and emergency panic mode',
      category: 'Pages',
      icon: Shield,
      keywords: 'firewall firewalld port zone rich rule panic block',
      action: () => navigateTo('firewall-manager')
    },
    {
      id: 'selinux-manager',
      title: 'SELinux Security Policy',
      description: 'SELinux mode (Enforcing/Permissive), booleans, and denial audits',
      category: 'Pages',
      icon: ShieldAlert,
      keywords: 'selinux security enforcing permissive boolean denial audit',
      action: () => navigateTo('selinux-manager')
    },
    {
      id: 'security-auditor',
      title: 'Security Auditor & CIS Hardening',
      description: 'System security audit checks, compliance score, and AI 1-click remediation',
      category: 'Pages',
      icon: ShieldAlert,
      keywords: 'security audit cis compliance score hardening check fix ai',
      action: () => navigateTo('security-auditor')
    },
    {
      id: 'journal-logs',
      title: 'System Journal & Audit Logs',
      description: 'Live systemd journalctl viewer, auth events, and threat monitoring',
      category: 'Pages',
      icon: FileText,
      keywords: 'log journal journalctl audit auth login fail2ban error',
      action: () => navigateTo('journal-logs')
    },
    {
      id: 'hosts-manager',
      title: 'Hosts File Manager',
      description: 'Manage /etc/hosts domain mappings and IP overrides',
      category: 'Pages',
      icon: FileText,
      keywords: 'hosts domain ip DNS mapping override resolve',
      action: () => navigateTo('hosts-manager')
    },
    {
      id: 'user-manager',
      title: 'User Accounts & Privileges',
      description: 'Local system users, groups, sudo privileges, and SSH authorized keys',
      category: 'Pages',
      icon: User,
      keywords: 'user group sudo passwd root account permission',
      action: () => navigateTo('user-manager')
    },
    {
      id: 'cron-manager',
      title: 'Cron Jobs & Systemd Timers',
      description: 'Scheduled crontab tasks and systemd timer units',
      category: 'Pages',
      icon: Sliders,
      keywords: 'cron crontab schedule timer task recurring job',
      action: () => navigateTo('cron-manager')
    },
    {
      id: 'shell-env',
      title: 'Shell Environment & Variables',
      description: 'PATH entries, environment variables, export profiles, and sourcing',
      category: 'Pages',
      icon: Terminal,
      keywords: 'shell env environment variable path profile export bash zsh',
      action: () => navigateTo('shell-env')
    },
    {
      id: 'device-manager',
      title: 'Storage & Disk Devices',
      description: 'Block devices, disk partitions, mount points, and SMART disk health',
      category: 'Pages',
      icon: HardDrive,
      keywords: 'disk storage device partition mount smart nvme hdd ssd',
      action: () => navigateTo('device-manager')
    },
    {
      id: 'ssh-cert-manager',
      title: 'SSH Keys & SSL Certificates Vault',
      description: 'Generate SSH keys, SSHD hardening, and SSL/TLS certificate viewer',
      category: 'Pages',
      icon: Lock,
      keywords: 'ssh key ssl tls cert vault sshd security authorized_keys',
      action: () => navigateTo('ssh-cert-manager')
    },
    {
      id: 'grub-manager',
      title: 'GRUB Boot Configurator',
      description: 'Default boot kernel, timeout, and kernel command-line arguments',
      category: 'Pages',
      icon: Cpu,
      keywords: 'grub boot kernel cmdline timeout default fedora',
      action: () => navigateTo('grub-manager')
    },

    // ── Quick Actions ────────────────────────────────────────────────────────────
    {
      id: 'action-toggle-theme',
      title: 'Toggle Light / Dark Theme',
      description: 'Switch application UI theme between Obsidian Dark and Clean Light mode',
      category: 'Actions',
      icon: Sparkles,
      keywords: 'theme dark light color mode toggle switch',
      action: () => {
        uiStore.toggleTheme();
        uiStore.closeSearchModal();
      }
    },
    {
      id: 'action-open-settings',
      title: 'Open System Preferences & AI Settings',
      description: 'Configure Ollama endpoints, API keys, AI model selection, and app config',
      category: 'Actions',
      icon: Settings,
      keywords: 'settings preferences ai ollama gemini openai config key',
      action: () => {
        uiStore.closeSearchModal();
        uiStore.openSettingsModal();
      }
    },
    {
      id: 'action-security-audit',
      title: 'Run System Security Audit',
      description: 'Perform a fresh security scan and calculate compliance score',
      category: 'Actions',
      icon: Zap,
      keywords: 'security audit run scan refresh score check',
      action: () => {
        navigateTo('security-auditor');
        setTimeout(() => window.dispatchEvent(new CustomEvent('security-audit-run')), 150);
      }
    }
  ];

  function navigateTo(tabId: TabId) {
    uiStore.setActiveTab(tabId);
    uiStore.closeSearchModal();
  }

  let filteredItems = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return searchItems;

    return searchItems.filter(item => {
      return (
        item.title.toLowerCase().includes(query) ||
        item.description.toLowerCase().includes(query) ||
        item.keywords.toLowerCase().includes(query)
      );
    });
  });

  // Keep selected index within valid bounds when query changes
  $effect(() => {
    if (filteredItems.length === 0) {
      selectedIndex = 0;
    } else if (selectedIndex >= filteredItems.length) {
      selectedIndex = filteredItems.length - 1;
    }
  });

  // Auto-focus input when modal opens
  $effect(() => {
    if (uiStore.searchModalOpen) {
      searchQuery = '';
      selectedIndex = 0;
      setTimeout(() => {
        if (searchInputRef) searchInputRef.focus();
      }, 50);
    }
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (filteredItems.length > 0) {
        selectedIndex = (selectedIndex + 1) % filteredItems.length;
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (filteredItems.length > 0) {
        selectedIndex = (selectedIndex - 1 + filteredItems.length) % filteredItems.length;
      }
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filteredItems.length > 0 && filteredItems[selectedIndex]) {
        filteredItems[selectedIndex].action();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      uiStore.closeSearchModal();
    }
  }
</script>

<svelte:window onkeydown={(e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    uiStore.toggleSearchModal();
  }
}} />

{#if uiStore.searchModalOpen}
  <div
    class="search-backdrop"
    role="button"
    tabindex="0"
    onclick={() => uiStore.closeSearchModal()}
    onkeydown={(e) => { if (e.key === 'Escape') uiStore.closeSearchModal(); }}
  >
    <div
      class="search-modal"
      role="document"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeyDown}
    >
      <!-- Search Bar Header -->
      <div class="search-header">
        <Search size={18} class="search-input-icon" />
        <input
          bind:this={searchInputRef}
          type="text"
          bind:value={searchQuery}
          placeholder="Search pages, system tools, actions... (Use ↑ ↓ Enter)"
          class="search-input"
        />
        <button
          type="button"
          class="search-close-btn"
          onclick={() => uiStore.closeSearchModal()}
          title="Close search modal (Esc)"
        >
          <X size={16} />
        </button>
      </div>

      <!-- Results Body -->
      <div class="search-results-list">
        {#if filteredItems.length > 0}
          {#each filteredItems as item, idx (item.id)}
            {@const ItemIcon = item.icon}
            {@const isSelected = idx === selectedIndex}
            <button
              type="button"
              class="search-item-row"
              class:selected={isSelected}
              onclick={item.action}
              onmouseenter={() => selectedIndex = idx}
            >
              <div class="item-icon-box">
                <ItemIcon size={16} class="item-icon" />
              </div>
              <div class="item-details">
                <div class="item-title-row">
                  <span class="item-title">{item.title}</span>
                  <span class="item-category-tag">{item.category}</span>
                </div>
                <div class="item-desc">{item.description}</div>
              </div>
              <ArrowRight size={14} class="item-arrow-icon" />
            </button>
          {/each}
        {:else}
          <div class="search-empty-state">
            <Search size={28} class="empty-icon" />
            <div class="empty-title">No matching pages or tools found</div>
            <div class="empty-desc">Try searching for keywords like "network", "firewall", "service", or "security"</div>
          </div>
        {/if}
      </div>

      <!-- Footer Help Bar -->
      <div class="search-footer">
        <div class="kbd-shortcuts">
          <span class="kbd-pill"><kbd>↑</kbd> <kbd>↓</kbd> Navigate</span>
          <span class="kbd-pill"><kbd>↵</kbd> Select</span>
          <span class="kbd-pill"><kbd>Esc</kbd> Close</span>
        </div>
        <span class="search-hint">Global Control Panel Search</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .search-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(8px);
    z-index: 10000;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
    animation: fadeIn 0.15s ease-out;
  }

  .search-modal {
    width: 100%;
    max-width: 620px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.4), 0 0 0 1px rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: slideDown 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .search-header {
    display: flex;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--color-border);
    gap: 12px;
    background: rgba(0, 0, 0, 0.1);
  }

  .search-input-icon {
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 14px;
    font-weight: 500;
    font-family: var(--font-sans);
  }

  .search-input::placeholder {
    color: var(--color-text-muted);
  }

  .search-close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .search-close-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .search-results-list {
    max-height: 380px;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .search-item-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 10px;
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
  }

  .search-item-row.selected {
    background: rgba(var(--color-accent-rgb, 59, 130, 246), 0.12);
    border-color: rgba(var(--color-accent-rgb, 59, 130, 246), 0.3);
  }

  .item-icon-box {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--color-text-secondary);
    transition: all 0.12s ease;
  }

  .search-item-row.selected .item-icon-box {
    background: var(--color-accent);
    color: #ffffff;
  }

  .item-details {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .item-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .item-category-tag {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-muted);
  }

  .item-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-arrow-icon {
    color: var(--color-text-muted);
    opacity: 0;
    transform: translateX(-4px);
    transition: all 0.12s ease;
  }

  .search-item-row.selected .item-arrow-icon {
    opacity: 1;
    transform: translateX(0);
    color: var(--color-accent);
  }

  .search-empty-state {
    padding: 36px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    color: var(--color-text-muted);
  }

  .empty-icon {
    color: var(--color-text-muted);
    opacity: 0.5;
  }

  .empty-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .empty-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    max-width: 320px;
  }

  .search-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-top: 1px solid var(--color-border);
    background: rgba(0, 0, 0, 0.15);
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .kbd-shortcuts {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .kbd-pill {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  kbd {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 1px 5px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
  }

  /* ── Light Mode Overrides ─────────────────────────────────────────────────── */
  :global(html.light-mode) .search-modal {
    background: #FFFFFF !important;
    border-color: #E2E8F0 !important;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.15) !important;
  }

  :global(html.light-mode) .search-header {
    background: #F8FAFC !important;
    border-bottom-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .search-footer {
    background: #F8FAFC !important;
    border-top-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .item-icon-box {
    background: #F1F5F9 !important;
    color: #475569 !important;
  }

  :global(html.light-mode) kbd {
    background: #F1F5F9 !important;
    border-color: #CBD5E1 !important;
    color: #334155 !important;
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-12px) scale(0.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
</style>
