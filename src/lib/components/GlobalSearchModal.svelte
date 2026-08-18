<script lang="ts">
  import { 
    Search, X, LayoutDashboard, Activity, Server, Package, Layers, Globe, Shield, 
    ShieldAlert, FileText, HardDrive, Terminal, Sliders, Lock, Cpu, User, FolderLock, 
    Settings, Sparkles, Sun, Moon, ArrowRight, Zap, RefreshCw, Clock, History,
    CheckCircle2, AlertTriangle, Play, Flame, CornerDownLeft, Trash2, Key, Database,
    SlidersHorizontal, Compass, Timer
  } from '@lucide/svelte';
  import { uiStore, type TabId } from '../stores/ui.svelte.ts';
  import { dnfStore } from '../stores/dnfStore.svelte.ts';

  interface SearchItem {
    id: string;
    title: string;
    description: string;
    category: 'Pages' | 'Tabs' | 'Actions' | 'Tools';
    breadcrumb?: string;
    icon: any;
    keywords: string;
    tabId?: TabId;
    subTab?: string;
    action: () => void;
  }

  let searchQuery = $state('');
  let selectedIndex = $state(0);
  let searchInputRef = $state<HTMLInputElement | null>(null);

  function executeItem(item: SearchItem) {
    uiStore.recordRecentSearch(searchQuery.trim() || item.title);
    if (item.tabId) {
      uiStore.recordVisitedItem({
        id: item.id,
        title: item.title,
        subtitle: item.breadcrumb || item.description,
        tabId: item.tabId,
        subTab: item.subTab,
        category: item.category
      });
    }
    item.action();
  }

  function handleSelectRecentSearch(query: string) {
    searchQuery = query;
    selectedIndex = 0;
    if (searchInputRef) searchInputRef.focus();
  }

  const searchItems: SearchItem[] = [
    // ═════════════════════════════════════════════════════════════════════════════
    // 1. TOP-LEVEL PAGES & MODULES
    // ═════════════════════════════════════════════════════════════════════════════
    {
      id: 'page-system-dashboard',
      title: 'System Overview Dashboard',
      description: 'Main system metrics, CPU/RAM usage, and quick status panel',
      category: 'Pages',
      icon: LayoutDashboard,
      keywords: 'overview dashboard summary status home hardware uptime os',
      tabId: 'system-dashboard',
      action: () => { uiStore.navigateTo('system-dashboard'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-system-monitor',
      title: 'System Monitor & Resources',
      description: 'Real-time CPU/RAM/Swap sparklines, disk usage, and process tree',
      category: 'Pages',
      icon: Activity,
      keywords: 'monitor process cpu ram memory kill stats graph usage task manager',
      tabId: 'system-monitor',
      action: () => { uiStore.navigateTo('system-monitor'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-service-manager',
      title: 'Services & Systemd Units',
      description: 'Manage systemd services, start, stop, restart, enable, and view logs',
      category: 'Pages',
      icon: Server,
      keywords: 'service systemd daemon start stop restart unit status daemon socket',
      tabId: 'service-manager',
      action: () => { uiStore.navigateTo('service-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-app-manager',
      title: 'Applications & Software Manager',
      description: 'Installed RPM packages, Flatpaks, AppImages, and application installer',
      category: 'Pages',
      icon: Package,
      keywords: 'apps software flatpak rpm appimage install uninstall packages software store',
      tabId: 'app-manager',
      action: () => { uiStore.navigateTo('app-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-repo-manager',
      title: 'RPM Repositories Manager',
      description: 'Enable, disable, or add DNF YUM repository files',
      category: 'Pages',
      icon: Layers,
      keywords: 'repo dnf yum repository copr fedora enable disable mirror',
      tabId: 'repo-manager',
      action: () => { uiStore.navigateTo('repo-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-dnf-history',
      title: 'DNF Package Transaction History',
      description: 'Review installed, updated, or removed packages and undo/rollback transactions',
      category: 'Pages',
      icon: History,
      keywords: 'history dnf rpm rollback undo transaction packages audit install log',
      tabId: 'dnf-history',
      action: () => { uiStore.navigateTo('dnf-history'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-copr-browser',
      title: 'Fedora COPR Repositories Browser',
      description: 'Search, explore, and enable community-built COPR repositories',
      category: 'Pages',
      icon: Compass,
      keywords: 'copr community repos packages extra builds build system third-party',
      tabId: 'copr-browser',
      action: () => { uiStore.navigateTo('copr-browser'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-network-manager',
      title: 'Network & Interfaces Manager',
      description: 'Configure network adapters, Wi-Fi connections, DNS, routes, and active sockets',
      category: 'Pages',
      icon: Globe,
      keywords: 'network wifi ethernet ip dns gateway interface routing connections nmcli',
      tabId: 'network-manager',
      action: () => { uiStore.navigateTo('network-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-nginx-manager',
      title: 'NGINX Reverse Proxy & VHosts',
      description: 'Manage virtual hosts, upstream proxy passes, SSL certificates, and config tests',
      category: 'Pages',
      icon: Server,
      keywords: 'nginx vhost server web proxy ssl upstream reverse proxy config reload',
      tabId: 'nginx-manager',
      action: () => { uiStore.navigateTo('nginx-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-firewall-manager',
      title: 'Firewalld Rules & Zones',
      description: 'Configure active firewall zones, allowed ports, rich rules, and interface bindings',
      category: 'Pages',
      icon: Shield,
      keywords: 'firewall firewalld ports zones security rich rules allow block traffic',
      tabId: 'firewall-manager',
      action: () => { uiStore.navigateTo('firewall-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-selinux-manager',
      title: 'SELinux Security Policy & Booleans',
      description: 'Enforcing / Permissive mode toggle, SELinux booleans, file contexts, and AVC denials',
      category: 'Pages',
      icon: Lock,
      keywords: 'selinux security booleans enforcing permissive context avc denials audit2allow',
      tabId: 'selinux-manager',
      action: () => { uiStore.navigateTo('selinux-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-security-auditor',
      title: 'CIS Hardening & Security Auditor',
      description: 'Run automated Linux security benchmark scans, CIS checks, and fix vulnerabilities',
      category: 'Pages',
      icon: ShieldAlert,
      keywords: 'security audit cis hardening compliance vulnerability scan report check',
      tabId: 'security-auditor',
      action: () => { uiStore.navigateTo('security-auditor'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-journal-logs',
      title: 'Systemd Journal & Audit Logs',
      description: 'Live streaming system logs, authentication events, auditd logs, and runtime threats',
      category: 'Pages',
      icon: FileText,
      keywords: 'logs journal journalctl errors auth sudo threats audit tail stream',
      tabId: 'journal-logs',
      action: () => { uiStore.navigateTo('journal-logs'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-hosts-manager',
      title: 'Local DNS & /etc/hosts Manager',
      description: 'Manage IP domain mappings, local aliases, and ad-blocking blocklists',
      category: 'Pages',
      icon: Globe,
      keywords: 'hosts dns domain ip address mapping etc hosts alias loopback blocklist',
      tabId: 'hosts-manager',
      action: () => { uiStore.navigateTo('hosts-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-user-manager',
      title: 'User Accounts & Groups',
      description: 'Create and edit user accounts, sudo/wheel permissions, groups, and SSH keys',
      category: 'Pages',
      icon: User,
      keywords: 'users accounts groups sudo wheel root password shell permissions',
      tabId: 'user-manager',
      action: () => { uiStore.navigateTo('user-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-cron-manager',
      title: 'Cron & Systemd Timers',
      description: 'Scheduled cron jobs, crontab entries, and recurring systemd timers',
      category: 'Pages',
      icon: Clock,
      keywords: 'cron crontab schedule timers automation periodic recurring tasks job',
      tabId: 'cron-manager',
      action: () => { uiStore.navigateTo('cron-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-shell-env',
      title: 'Shell Environment & Variables',
      description: 'Inspect $PATH entries, environment variables, login shell, and terminal profiles',
      category: 'Pages',
      icon: Terminal,
      keywords: 'env environment variables path shell bash zsh profile exports',
      tabId: 'shell-env',
      action: () => { uiStore.navigateTo('shell-env'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-device-manager',
      title: 'Hardware Devices & Storage',
      description: 'Storage disks, partitions, PCI devices, USB peripherals, and GPU info',
      category: 'Pages',
      icon: HardDrive,
      keywords: 'devices hardware disk partition storage pci usb gpu cpu smart',
      tabId: 'device-manager',
      action: () => { uiStore.navigateTo('device-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-ssh-cert-manager',
      title: 'SSH & SSL Certificate Vault',
      description: 'Manage SSH key pairs, authorized keys, and SSL/TLS certificates',
      category: 'Pages',
      icon: Key,
      keywords: 'ssh ssl tls cert certificate keys authorized_keys vault encryption crypto',
      tabId: 'ssh-cert-manager',
      action: () => { uiStore.navigateTo('ssh-cert-manager'); uiStore.closeSearchModal(); }
    },
    {
      id: 'page-grub-manager',
      title: 'GRUB Bootloader & Kernels',
      description: 'Installed kernel boot entries, default boot kernel, timeout, and boot parameters',
      category: 'Pages',
      icon: Cpu,
      keywords: 'grub bootloader kernel boot timeout parameters uefi initrd efiboot',
      tabId: 'grub-manager',
      action: () => { uiStore.navigateTo('grub-manager'); uiStore.closeSearchModal(); }
    },

    // ═════════════════════════════════════════════════════════════════════════════
    // 2. SUB-TABS & VIEWS WITHIN PAGES
    // ═════════════════════════════════════════════════════════════════════════════
    // ── System Monitor Tabs ──
    {
      id: 'tab-monitor-processes',
      title: 'Processes & Process Tree',
      description: 'Inspect running system processes, CPU/RAM usage, and terminate PID',
      category: 'Tabs',
      breadcrumb: 'Monitoring › Processes',
      icon: Activity,
      keywords: 'processes process tree ps top htop kill terminate pid parent cpu memory task',
      tabId: 'system-monitor',
      subTab: 'processes',
      action: () => { uiStore.navigateTo('system-monitor', 'processes'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-monitor-overview',
      title: 'Resource Sparklines & Gauges',
      description: 'Live CPU temperature, RAM usage, swap space, and disk I/O metrics',
      category: 'Tabs',
      breadcrumb: 'Monitoring › Overview',
      icon: Activity,
      keywords: 'overview sparklines gauges live temp temperature ram swap disk io network traffic',
      tabId: 'system-monitor',
      subTab: 'overview',
      action: () => { uiStore.navigateTo('system-monitor', 'overview'); uiStore.closeSearchModal(); }
    },

    // ── Services Manager Tabs ──
    {
      id: 'tab-services-systemd',
      title: 'System Services & Daemons',
      description: 'Manage active, failed, and enabled systemd service units',
      category: 'Tabs',
      breadcrumb: 'Services Manager › Services',
      icon: Server,
      keywords: 'services systemd daemons active failed units start stop restart enable mask',
      tabId: 'service-manager',
      subTab: 'services',
      action: () => { uiStore.navigateTo('service-manager', 'services'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-services-autostart',
      title: 'Desktop Autostart Applications',
      description: 'Manage user and system session startup desktop entry files',
      category: 'Tabs',
      breadcrumb: 'Services Manager › Autostart',
      icon: Play,
      keywords: 'autostart startup desktop applications session login xdg entries',
      tabId: 'service-manager',
      subTab: 'autostart',
      action: () => { uiStore.navigateTo('service-manager', 'autostart'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-services-boot-analyzer',
      title: 'Boot Latency Analyzer & Critical Chain',
      description: 'Analyze systemd startup timings, firmware/kernel/userspace phases, and unit bottlenecks',
      category: 'Tabs',
      breadcrumb: 'Services Manager › Boot Analyzer',
      icon: Timer,
      keywords: 'boot analyzer latency blame critical chain startup speed bootloader uefi grub kernel time breakdown bottlenecks systemd-analyze',
      tabId: 'service-manager',
      subTab: 'boot_analyzer',
      action: () => { uiStore.navigateTo('service-manager', 'boot_analyzer'); uiStore.closeSearchModal(); }
    },

    // ── App Manager Tabs ──
    {
      id: 'tab-apps-rpm',
      title: 'RPM Native Packages',
      description: 'View and manage system native RPM packages installed via DNF',
      category: 'Tabs',
      breadcrumb: 'App Manager › RPM',
      icon: Package,
      keywords: 'rpm packages dnf native fedora software installed list',
      tabId: 'app-manager',
      subTab: 'RPM',
      action: () => { uiStore.navigateTo('app-manager', 'RPM'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-apps-flatpak',
      title: 'Flatpak Applications',
      description: 'Manage sandboxed Flatpak desktop applications and Flathub runtimes',
      category: 'Tabs',
      breadcrumb: 'App Manager › Flatpak',
      icon: Package,
      keywords: 'flatpak flathub sandbox desktop apps runtimes permissions',
      tabId: 'app-manager',
      subTab: 'Flatpak',
      action: () => { uiStore.navigateTo('app-manager', 'Flatpak'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-apps-appimage',
      title: 'AppImage Portable Packages',
      description: 'Scan and manage standalone portable AppImage binaries',
      category: 'Tabs',
      breadcrumb: 'App Manager › AppImage',
      icon: Package,
      keywords: 'appimage portable binary standalone executables',
      tabId: 'app-manager',
      subTab: 'AppImage',
      action: () => { uiStore.navigateTo('app-manager', 'AppImage'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-apps-duplicates',
      title: 'Duplicate Applications Detector',
      description: 'Find applications installed under multiple packaging formats (e.g. RPM + Flatpak)',
      category: 'Tabs',
      breadcrumb: 'App Manager › Duplicates',
      icon: Package,
      keywords: 'duplicates redundancy multiple formats rpm flatpak appimage clash cleaner',
      tabId: 'app-manager',
      subTab: 'Duplicates',
      action: () => { uiStore.navigateTo('app-manager', 'Duplicates'); uiStore.closeSearchModal(); }
    },

    // ── Journal & Audit Logs Tabs ──
    {
      id: 'tab-journal-stream',
      title: 'Systemd Journal Stream',
      description: 'Live streaming system logs with priority levels and unit filtering',
      category: 'Tabs',
      breadcrumb: 'Journal Logs › System Logs',
      icon: FileText,
      keywords: 'journalctl stream system logs priority error warning debug tail follow',
      tabId: 'journal-logs',
      subTab: 'journal',
      action: () => { uiStore.navigateTo('journal-logs', 'journal'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-journal-auth',
      title: 'Authentication & Security Events',
      description: 'Audit sudo command executions, SSH logins, and PAM auth failures',
      category: 'Tabs',
      breadcrumb: 'Journal Logs › Auth Events',
      icon: ShieldAlert,
      keywords: 'auth events sudo ssh logins pam authentication password failed invalid user',
      tabId: 'journal-logs',
      subTab: 'auth',
      action: () => { uiStore.navigateTo('journal-logs', 'auth'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-journal-audit',
      title: 'Auditd Command Audit Logs',
      description: 'View kernel audit events, root executions, and privileged command trails',
      category: 'Tabs',
      breadcrumb: 'Journal Logs › Audit Logs',
      icon: Shield,
      keywords: 'audit auditd ausearch aureport kernel audit rule execve command trail',
      tabId: 'journal-logs',
      subTab: 'audit',
      action: () => { uiStore.navigateTo('journal-logs', 'audit'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-journal-threats',
      title: 'Runtime Threat Detection',
      description: 'Real-time detection of brute-force attacks and tampering attempts',
      category: 'Tabs',
      breadcrumb: 'Journal Logs › Threat Detection',
      icon: Zap,
      keywords: 'threats attacks brute force intrusion detection tampering fail2ban real-time alerts',
      tabId: 'journal-logs',
      subTab: 'threats',
      action: () => { uiStore.navigateTo('journal-logs', 'threats'); uiStore.closeSearchModal(); }
    },

    // ── SSH & SSL Vault Tabs ──
    {
      id: 'tab-ssh-keys',
      title: 'SSH Key Pairs Vault',
      description: 'Generate, import, inspect, and copy ED25519 & RSA SSH keys',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › Key Pairs',
      icon: Key,
      keywords: 'ssh keys generate ed25519 rsa public private keygen passphrase',
      tabId: 'ssh-cert-manager',
      subTab: 'keys',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'keys'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-ssh-authorized',
      title: 'Authorized Keys Manager',
      description: 'Manage authorized SSH public keys for inbound remote login access',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › Authorized Keys',
      icon: Lock,
      keywords: 'authorized_keys inbound ssh remote login access add public key',
      tabId: 'ssh-cert-manager',
      subTab: 'authorized',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'authorized'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-ssh-client-config',
      title: 'SSH Client Config (~/.ssh/config)',
      description: 'Manage remote host aliases, port forwards, proxy jumps, and identity files',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › Client Config',
      icon: SlidersHorizontal,
      keywords: 'ssh client config host alias proxyjump identityfile port user ~/.ssh/config',
      tabId: 'ssh-cert-manager',
      subTab: 'client_config',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'client_config'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-ssh-known-hosts',
      title: 'Known Hosts Fingerprints',
      description: 'Inspect and manage remote server SSH host fingerprints (~/.ssh/known_hosts)',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › Known Hosts',
      icon: Globe,
      keywords: 'known_hosts fingerprints host keys remote servers verify ~/.ssh/known_hosts',
      tabId: 'ssh-cert-manager',
      subTab: 'known_hosts',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'known_hosts'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-ssh-certs',
      title: 'SSL / TLS Certificates Viewer',
      description: 'Inspect system SSL/TLS certificates, expiry dates, and issuer chains',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › SSL Certificates',
      icon: Lock,
      keywords: 'ssl tls certificates certs x509 expiration letsencrypt ca issuer validity',
      tabId: 'ssh-cert-manager',
      subTab: 'certs',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'certs'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-ssh-threats',
      title: 'Fail2ban Defenses & Banned IPs',
      description: 'Inspect Fail2ban SSH jail status, active banned IPs, and unban tools',
      category: 'Tabs',
      breadcrumb: 'SSH & SSL Vault › Fail2ban Defenses',
      icon: ShieldAlert,
      keywords: 'fail2ban banned ip jail unban sshd brute force attack defense',
      tabId: 'ssh-cert-manager',
      subTab: 'threats',
      action: () => { uiStore.navigateTo('ssh-cert-manager', 'threats'); uiStore.closeSearchModal(); }
    },

    // ── Network Tabs ──
    {
      id: 'tab-network-interfaces',
      title: 'Network Adapters & IP Addresses',
      description: 'Inspect Ethernet, Wi-Fi, virtual bridges, IPv4/IPv6, and MAC addresses',
      category: 'Tabs',
      breadcrumb: 'Network › Interfaces',
      icon: Globe,
      keywords: 'interfaces adapters ip ipv4 ipv6 ethernet wifi mac subnet gateway',
      tabId: 'network-manager',
      subTab: 'interfaces',
      action: () => { uiStore.navigateTo('network-manager', 'interfaces'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-network-connections',
      title: 'Active Network Connections & Sockets',
      description: 'View active listening ports, remote connections, and established sockets',
      category: 'Tabs',
      breadcrumb: 'Network › Connections',
      icon: Activity,
      keywords: 'connections sockets ss netstat listening ports tcp udp foreign address',
      tabId: 'network-manager',
      subTab: 'connections',
      action: () => { uiStore.navigateTo('network-manager', 'connections'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-network-vpn',
      title: 'VPN & WireGuard Profiles',
      description: 'Manage NetworkManager VPN profiles, WireGuard, and OpenVPN tunnels',
      category: 'Tabs',
      breadcrumb: 'Network › VPN Profiles',
      icon: Lock,
      keywords: 'vpn wireguard openvpn tunnel secure profile connect disconnect',
      tabId: 'network-manager',
      subTab: 'vpn',
      action: () => { uiStore.navigateTo('network-manager', 'vpn'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-network-dns',
      title: 'DNS Resolver & Nameservers',
      description: 'Configure DNS nameservers, search domains, and systemd-resolved status',
      category: 'Tabs',
      breadcrumb: 'Network › DNS Resolver',
      icon: Globe,
      keywords: 'dns nameserver resolver resolv.conf systemd-resolved 1.1.1.1 8.8.8.8 domain lookup',
      tabId: 'network-manager',
      subTab: 'dns',
      action: () => { uiStore.navigateTo('network-manager', 'dns'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-network-speedtest',
      title: 'Ping Diagnostics & Speedtest',
      description: 'Test ping latency to global DNS servers and benchmark connection speed',
      category: 'Tabs',
      breadcrumb: 'Network › Diagnostics & Speed',
      icon: Zap,
      keywords: 'speedtest ping latency test benchmark internet speed bandwidth packet loss',
      tabId: 'network-manager',
      subTab: 'speedtest',
      action: () => { uiStore.navigateTo('network-manager', 'speedtest'); uiStore.closeSearchModal(); }
    },

    // ── Firewall Tabs ──
    {
      id: 'tab-firewall-rules',
      title: 'Allowed Services & Ports',
      description: 'Manage open firewall ports and allowed services across firewalld zones',
      category: 'Tabs',
      breadcrumb: 'Firewall › Basic Rules',
      icon: Shield,
      keywords: 'firewall rules open port allow service http https ssh 80 443 22 zone',
      tabId: 'firewall-manager',
      subTab: 'rules',
      action: () => { uiStore.navigateTo('firewall-manager', 'rules'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-firewall-rich',
      title: 'Firewall Rich Rules & Custom Filtering',
      description: 'Configure advanced custom rate-limits, source IP logging, and drop rules',
      category: 'Tabs',
      breadcrumb: 'Firewall › Rich Rules',
      icon: ShieldAlert,
      keywords: 'rich rules custom filtering rate limit log source ip reject drop',
      tabId: 'firewall-manager',
      subTab: 'rich',
      action: () => { uiStore.navigateTo('firewall-manager', 'rich'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-firewall-interfaces',
      title: 'Interface Zone Bindings',
      description: 'Bind network adapters (eth0, wlan0) to specific security zones',
      category: 'Tabs',
      breadcrumb: 'Firewall › Interface Bindings',
      icon: Globe,
      keywords: 'zone binding interface eth0 wlan0 public trusted internal drop dmz',
      tabId: 'firewall-manager',
      subTab: 'interfaces',
      action: () => { uiStore.navigateTo('firewall-manager', 'interfaces'); uiStore.closeSearchModal(); }
    },

    // ── Storage & Disks Tabs ──
    {
      id: 'tab-device-disks',
      title: 'Physical Disks & Partitions',
      description: 'Inspect NVMe, SSD, and HDD physical drives, filesystem types, and mount points',
      category: 'Tabs',
      breadcrumb: 'Storage Devices › Disks & Partitions',
      icon: HardDrive,
      keywords: 'disks partitions nvme ssd hdd block devices ext4 btrfs xfs size mounts',
      tabId: 'device-manager',
      subTab: 'list',
      action: () => { uiStore.navigateTo('device-manager', 'list'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-device-smart',
      title: 'SMART Disk Health Diagnostics',
      description: 'View drive health status, power-on hours, bad sectors, and drive temperature',
      category: 'Tabs',
      breadcrumb: 'Storage Devices › SMART Health',
      icon: Activity,
      keywords: 'smart health diagnostics disk health bad sectors temperature ssd wear lifespan',
      tabId: 'device-manager',
      subTab: 'smart',
      action: () => { uiStore.navigateTo('device-manager', 'smart'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-device-topology',
      title: 'Hardware Device Topology Tree',
      description: 'Full hardware tree hierarchy including PCI devices, USB buses, and bridges',
      category: 'Tabs',
      breadcrumb: 'Storage Devices › Hardware Tree',
      icon: Cpu,
      keywords: 'topology hardware tree lshw pci usb bridge memory motherboard bus',
      tabId: 'device-manager',
      subTab: 'topology',
      action: () => { uiStore.navigateTo('device-manager', 'topology'); uiStore.closeSearchModal(); }
    },

    // ── NGINX Tabs ──
    {
      id: 'tab-nginx-overview',
      title: 'NGINX Overview & Service Status',
      description: 'Web server daemon status, active worker processes, uptime, and quick actions',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › Overview',
      icon: Server,
      keywords: 'nginx overview status service daemon reload restart test config web server',
      tabId: 'nginx-manager',
      subTab: 'overview',
      action: () => { uiStore.navigateTo('nginx-manager', 'overview'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-sites',
      title: 'NGINX Virtual Hosts & Sites',
      description: 'Manage NGINX server blocks, virtual hosts, and reverse proxy routes',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › Sites',
      icon: Globe,
      keywords: 'nginx sites vhosts virtual hosts server blocks reverse proxy sites-enabled proxy_pass domain',
      tabId: 'nginx-manager',
      subTab: 'sites',
      action: () => { uiStore.navigateTo('nginx-manager', 'sites'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-editor',
      title: 'NGINX Config File Editor',
      description: 'Directly edit nginx.conf configuration files with syntax checking and diff',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › Config Editor',
      icon: FileText,
      keywords: 'nginx config editor nginx.conf configuration syntax check test reload backups diff',
      tabId: 'nginx-manager',
      subTab: 'editor',
      action: () => { uiStore.navigateTo('nginx-manager', 'editor'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-www',
      title: 'NGINX WWW Web Root Files',
      description: 'Browse, manage, view, and organize static HTML/JS web files in /var/www',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › WWW Files',
      icon: FolderLock,
      keywords: 'nginx www web root html files /var/www document root static assets website files',
      tabId: 'nginx-manager',
      subTab: 'www',
      action: () => { uiStore.navigateTo('nginx-manager', 'www'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-logs',
      title: 'NGINX Access & Error Logs',
      description: 'Real-time structured HTTP access logs, status codes, and error diagnostics',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › Logs',
      icon: Terminal,
      keywords: 'nginx logs access.log error.log http requests status 404 500 visitor traffic stream',
      tabId: 'nginx-manager',
      subTab: 'logs',
      action: () => { uiStore.navigateTo('nginx-manager', 'logs'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-analytics',
      title: 'NGINX Traffic & Request Analytics',
      description: 'HTTP status codes breakdown (2xx, 3xx, 4xx, 5xx), top client IPs, and metrics',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › Analytics',
      icon: Activity,
      keywords: 'nginx analytics stats requests unique ips traffic bandwidth chart top clients hits',
      tabId: 'nginx-manager',
      subTab: 'analytics',
      action: () => { uiStore.navigateTo('nginx-manager', 'analytics'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-nginx-ssl',
      title: 'NGINX SSL / TLS Certificates',
      description: 'Inspect SSL certificates, expiry timelines, Let’s Encrypt certbot integration, and renew certs',
      category: 'Tabs',
      breadcrumb: 'NGINX Manager › SSL Certificates',
      icon: Lock,
      keywords: 'nginx ssl tls certificates letsencrypt certbot renew https encryption expiry domains',
      tabId: 'nginx-manager',
      subTab: 'ssl',
      action: () => { uiStore.navigateTo('nginx-manager', 'ssl'); uiStore.closeSearchModal(); }
    },

    // ── Security Auditor Categories ──
    {
      id: 'tab-audit-ssh',
      title: 'SSH Hardening Audit Checks',
      description: 'Audit SSH protocol 2, root login, password auth, and idle timeouts',
      category: 'Tabs',
      breadcrumb: 'Security Auditor › SSH Hardening',
      icon: Shield,
      keywords: 'ssh hardening audit root login permitrootlogin password authentication idle timeout',
      tabId: 'security-auditor',
      subTab: 'SSH Hardening',
      action: () => { uiStore.navigateTo('security-auditor', 'SSH Hardening'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-audit-kernel',
      title: 'Kernel & Sysctl Hardening Checks',
      description: 'Audit ASLR, SYN cookies, dmesg restrictions, and core dump parameters',
      category: 'Tabs',
      breadcrumb: 'Security Auditor › Kernel Hardening',
      icon: Cpu,
      keywords: 'kernel sysctl aslr syn cookies dmesg kptr core dump sysctl.conf parameters',
      tabId: 'security-auditor',
      subTab: 'Kernel Hardening',
      action: () => { uiStore.navigateTo('security-auditor', 'Kernel Hardening'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-audit-auth',
      title: 'User & Authentication Security Checks',
      description: 'Audit password expiration, empty passwords, duplicate UIDs, and sudo security',
      category: 'Tabs',
      breadcrumb: 'Security Auditor › User & Auth',
      icon: User,
      keywords: 'user auth password expiration empty password duplicate uid sudoers wheel',
      tabId: 'security-auditor',
      subTab: 'User & Auth',
      action: () => { uiStore.navigateTo('security-auditor', 'User & Auth'); uiStore.closeSearchModal(); }
    },

    // ── User Management Tabs ──
    {
      id: 'tab-users-accounts',
      title: 'User Accounts & Sudo Members',
      description: 'Create, modify, and delete system user accounts and sudo privileges',
      category: 'Tabs',
      breadcrumb: 'Users & Groups › User Accounts',
      icon: User,
      keywords: 'users accounts create user password sudo wheel group lock account',
      tabId: 'user-manager',
      subTab: 'users',
      action: () => { uiStore.navigateTo('user-manager', 'users'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-users-groups',
      title: 'System Groups Management',
      description: 'Create and manage system groups and group member assignments',
      category: 'Tabs',
      breadcrumb: 'Users & Groups › Groups',
      icon: User,
      keywords: 'groups system groups create group members gid groupadd',
      tabId: 'user-manager',
      subTab: 'groups',
      action: () => { uiStore.navigateTo('user-manager', 'groups'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-users-sessions',
      title: 'Active Login Sessions',
      description: 'View live user sessions, login seats, TTYs, and session states via loginctl',
      category: 'Tabs',
      breadcrumb: 'Users & Groups › Active Sessions',
      icon: Activity,
      keywords: 'sessions active sessions loginctl seat tty who w login state',
      tabId: 'user-manager',
      subTab: 'sessions',
      action: () => { uiStore.navigateTo('user-manager', 'sessions'); uiStore.closeSearchModal(); }
    },

    // ── Scheduled Tasks Tabs ──
    {
      id: 'tab-cron-jobs',
      title: 'User & Root Crontabs',
      description: 'Create and manage automated cron jobs for user and root accounts',
      category: 'Tabs',
      breadcrumb: 'Scheduled Tasks › Crontab',
      icon: Clock,
      keywords: 'crontab cron jobs recurring schedule periodic automated task',
      tabId: 'cron-manager',
      subTab: 'cron',
      action: () => { uiStore.navigateTo('cron-manager', 'cron'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-cron-timers',
      title: 'Systemd Timers Units',
      description: 'Inspect active systemd timer units, next run triggers, and timer schedules',
      category: 'Tabs',
      breadcrumb: 'Scheduled Tasks › Systemd Timers',
      icon: Sliders,
      keywords: 'systemd timers timer units systemctl list-timers scheduled next trigger',
      tabId: 'cron-manager',
      subTab: 'timers',
      action: () => { uiStore.navigateTo('cron-manager', 'timers'); uiStore.closeSearchModal(); }
    },

    // ── Shell Environment Tabs ──
    {
      id: 'tab-shell-vars',
      title: 'Environment Variables Editor',
      description: 'Inspect, edit, and add global and session environment variables',
      category: 'Tabs',
      breadcrumb: 'Shell Environment › Variables',
      icon: Terminal,
      keywords: 'environment variables env set export global session shell vars',
      tabId: 'shell-env',
      subTab: 'variables',
      action: () => { uiStore.navigateTo('shell-env', 'variables'); uiStore.closeSearchModal(); }
    },
    {
      id: 'tab-shell-path',
      title: 'PATH Directories Auditor',
      description: 'Audit, reorder, and add executable directories in $PATH',
      category: 'Tabs',
      breadcrumb: 'Shell Environment › PATH Entries',
      icon: Terminal,
      keywords: 'path entries directories bin usr/bin local/bin export path audit',
      tabId: 'shell-env',
      subTab: 'path',
      action: () => { uiStore.navigateTo('shell-env', 'path'); uiStore.closeSearchModal(); }
    },

    // ═════════════════════════════════════════════════════════════════════════════
    // 3. QUICK ACTIONS & TOOLS
    // ═════════════════════════════════════════════════════════════════════════════
    {
      id: 'action-toggle-theme',
      title: 'Toggle Light / Dark Mode',
      description: 'Switch between Obsidian Dark theme and Clean Light theme',
      category: 'Actions',
      icon: Sparkles,
      keywords: 'theme dark light mode toggle switch appearance color scheme',
      action: () => {
        uiStore.toggleTheme();
        uiStore.closeSearchModal();
      }
    },
    {
      id: 'action-open-settings',
      title: 'Open Preferences & AI Assistant Settings',
      description: 'Configure Ollama endpoints, API keys, AI model selection, and app preferences',
      category: 'Actions',
      icon: Settings,
      keywords: 'settings preferences ai ollama gemini openai config key model',
      action: () => {
        uiStore.closeSearchModal();
        uiStore.openSettingsModal();
      }
    },
    {
      id: 'action-security-audit',
      title: 'Run System Security Audit Scan',
      description: 'Perform a comprehensive live CIS security scan and recalculate score',
      category: 'Actions',
      icon: Zap,
      keywords: 'security audit run scan refresh score check cis hardening',
      tabId: 'security-auditor',
      action: () => {
        uiStore.navigateTo('security-auditor');
        setTimeout(() => window.dispatchEvent(new CustomEvent('security-audit-run')), 150);
        uiStore.closeSearchModal();
      }
    },
    {
      id: 'action-dnf-upgrade',
      title: 'Open DNF Global Package Upgrade',
      description: 'Inspect available package updates and perform system-wide DNF upgrade',
      category: 'Actions',
      icon: RefreshCw,
      keywords: 'dnf upgrade update packages system update dnf upgrade -y software updates',
      action: () => {
        uiStore.closeSearchModal();
        dnfStore.openDrawer();
      }
    },
    {
      id: 'action-toggle-density',
      title: 'Toggle Table Density (Compact / Spacious)',
      description: 'Switch data tables between high-density rows and spacious layout',
      category: 'Actions',
      icon: Sliders,
      keywords: 'table density compact spacious rows view format',
      action: () => {
        uiStore.toggleTableDensity();
        uiStore.closeSearchModal();
      }
    }
  ];

  // Ranked Filtering: Matches title, breadcrumb, description, keywords
  // Shows searched page at top, then below it its tabs in exact UI sequence
  let filteredItems = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return searchItems;

    const terms = query.split(/\s+/).filter(Boolean);

    // 1. Calculate relevance score for all items
    const scoredEntries = searchItems.map((item, originalIndex) => {
      let score = 0;
      const titleLower = item.title.toLowerCase();
      const breadcrumbLower = (item.breadcrumb || '').toLowerCase();
      const descLower = item.description.toLowerCase();
      const kwLower = item.keywords.toLowerCase();

      // Exact match boosts
      if (titleLower === query) score += 120;
      if (titleLower.startsWith(query)) score += 60;
      if (breadcrumbLower.includes(query)) score += 40;
      if (titleLower.includes(query)) score += 35;

      // Category page boost if query matches page name
      if (item.category === 'Pages' && (titleLower.includes(query) || kwLower.includes(query))) {
        score += 30;
      }

      // Individual term matches
      for (const term of terms) {
        if (titleLower.includes(term)) score += 25;
        if (breadcrumbLower.includes(term)) score += 15;
        if (kwLower.includes(term)) score += 10;
        if (descLower.includes(term)) score += 5;
      }

      return { item, score, originalIndex };
    });

    const matchedEntries = scoredEntries.filter(entry => entry.score > 0);
    if (matchedEntries.length === 0) return [];

    // 2. Group by tabId (page/module) so that the Page is always on top, followed by its tabs in UI order
    const groups: Map<string, {
      pageItem?: { item: SearchItem; score: number; originalIndex: number };
      tabs: { item: SearchItem; score: number; originalIndex: number }[];
      maxScore: number;
    }> = new Map();

    const standaloneItems: { item: SearchItem; score: number; originalIndex: number }[] = [];

    for (const entry of matchedEntries) {
      if (!entry.item.tabId) {
        standaloneItems.push(entry);
        continue;
      }

      const key = entry.item.tabId;
      if (!groups.has(key)) {
        groups.set(key, { tabs: [], maxScore: 0 });
      }
      const grp = groups.get(key)!;
      grp.maxScore = Math.max(grp.maxScore, entry.score);

      if (entry.item.category === 'Pages') {
        grp.pageItem = entry;
      } else {
        grp.tabs.push(entry);
      }
    }

    // 3. Sort groups by their highest score
    const sortedGroups = Array.from(groups.values()).sort((a, b) => b.maxScore - a.maxScore);

    // 4. Flatten: Page item first, then its tabs in UI original sequence
    const result: SearchItem[] = [];

    for (const grp of sortedGroups) {
      if (grp.pageItem) {
        result.push(grp.pageItem.item);
      }
      // Sort matching tabs by their original index in searchItems (which matches UI order)
      grp.tabs.sort((a, b) => a.originalIndex - b.originalIndex);
      for (const t of grp.tabs) {
        result.push(t.item);
      }
    }

    // Append standalone actions / tools sorted by score
    standaloneItems.sort((a, b) => b.score - a.score);
    for (const s of standaloneItems) {
      result.push(s.item);
    }

    return result;
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
      }, 60);
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
        executeItem(filteredItems[selectedIndex]);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      uiStore.closeSearchModal();
    }
  }
</script>

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
      <!-- ── Search Input Header ── -->
      <div class="search-header">
        <Search size={19} class="search-input-icon" />
        <input
          bind:this={searchInputRef}
          type="text"
          bind:value={searchQuery}
          placeholder="Search pages, specific tabs (e.g. processes, auth logs, ports), actions..."
          class="search-input"
        />
        {#if searchQuery}
          <button
            type="button"
            class="search-clear-query-btn"
            onclick={() => { searchQuery = ''; searchInputRef?.focus(); }}
            title="Clear query"
          >
            <X size={14} />
          </button>
        {/if}
        <button
          type="button"
          class="search-close-btn"
          onclick={() => uiStore.closeSearchModal()}
          title="Close search modal (Esc)"
        >
          <kbd>ESC</kbd>
        </button>
      </div>

      <!-- ── Results & Suggestions Body ── -->
      <div class="search-results-list">
        {#if searchQuery.trim()}
          <!-- Active typing suggestions -->
          {#if filteredItems.length > 0}
            <div class="results-section-label">
              <span>Suggestions &amp; Matches</span>
              <span class="count-tag">{filteredItems.length}</span>
            </div>
            {#each filteredItems as item, idx (item.id)}
              {@const ItemIcon = item.icon}
              {@const isSelected = idx === selectedIndex}
              <button
                type="button"
                class="search-item-row"
                class:selected={isSelected}
                onclick={() => executeItem(item)}
                onmouseenter={() => selectedIndex = idx}
              >
                <div class="item-icon-box" class:is-tab={item.category === 'Tabs'} class:is-action={item.category === 'Actions'}>
                  <ItemIcon size={16} />
                </div>
                <div class="item-details">
                  <div class="item-title-row">
                    <span class="item-title">{item.title}</span>
                    {#if item.breadcrumb}
                      <span class="item-breadcrumb-badge">{item.breadcrumb}</span>
                    {/if}
                    <span class="item-category-tag cat-{item.category.toLowerCase()}">{item.category}</span>
                  </div>
                  <div class="item-desc">{item.description}</div>
                </div>
                <div class="item-action-indicator">
                  <CornerDownLeft size={13} />
                </div>
              </button>
            {/each}
          {:else}
            <div class="search-empty-state">
              <Search size={32} class="empty-icon" />
              <div class="empty-title">No matching pages, tabs, or actions found</div>
              <div class="empty-desc">Try keywords like "processes", "firewall", "authorized keys", "auth logs", or "smart"</div>
            </div>
          {/if}

        {:else}
          <!-- Empty Query: Recent Visited + Quick Suggestions -->

          <!-- 1. Recent Searches History (if any) -->
          {#if (uiStore.recentSearches?.length ?? 0) > 0}
            <div class="search-history-container">
              <div class="results-section-header">
                <div class="section-title-group">
                  <History size={13} style="color:var(--color-accent);" />
                  <span>Recent Searches</span>
                </div>
                <button
                  type="button"
                  class="clear-history-btn"
                  onclick={() => uiStore.clearRecentSearches()}
                  title="Clear recent search history"
                >
                  <Trash2 size={11} /> Clear
                </button>
              </div>
              <div class="recent-search-pills">
                {#each (uiStore.recentSearches || []) as query}
                  <button
                    type="button"
                    class="recent-search-pill"
                    onclick={() => handleSelectRecentSearch(query)}
                  >
                    <Clock size={11} />
                    <span>{query}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          <!-- 2. Recently Visited Menus & Tabs (if any) -->
          {#if (uiStore.recentVisitedItems?.length ?? 0) > 0}
            <div class="visited-section">
              <div class="results-section-header">
                <div class="section-title-group">
                  <Compass size={13} style="color:var(--color-info, #0284c7);" />
                  <span>Recently Visited Menus &amp; Tabs</span>
                </div>
                <button
                  type="button"
                  class="clear-history-btn"
                  onclick={() => uiStore.clearRecentVisited()}
                  title="Clear visited history"
                >
                  <Trash2 size={11} /> Clear
                </button>
              </div>
              <div class="visited-grid">
                {#each (uiStore.recentVisitedItems || []).slice(0, 6) as item}
                  <button
                    type="button"
                    class="visited-card"
                    onclick={() => {
                      uiStore.navigateTo(item.tabId, item.subTab);
                      uiStore.closeSearchModal();
                    }}
                  >
                    <div class="visited-card-title">{item.title}</div>
                    {#if item.subtitle}
                      <div class="visited-card-sub">{item.subtitle}</div>
                    {/if}
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          <!-- 3. Suggested Popular Tabs & Shortcuts -->
          <div class="results-section-label">
            <span>Recommended Pages &amp; Key Subtabs</span>
          </div>
          {#each searchItems.slice(0, 10) as item, idx (item.id)}
            {@const ItemIcon = item.icon}
            {@const isSelected = idx === selectedIndex}
            <button
              type="button"
              class="search-item-row"
              class:selected={isSelected}
              onclick={() => executeItem(item)}
              onmouseenter={() => selectedIndex = idx}
            >
              <div class="item-icon-box" class:is-tab={item.category === 'Tabs'} class:is-action={item.category === 'Actions'}>
                <ItemIcon size={16} />
              </div>
              <div class="item-details">
                <div class="item-title-row">
                  <span class="item-title">{item.title}</span>
                  {#if item.breadcrumb}
                    <span class="item-breadcrumb-badge">{item.breadcrumb}</span>
                  {/if}
                  <span class="item-category-tag cat-{item.category.toLowerCase()}">{item.category}</span>
                </div>
                <div class="item-desc">{item.description}</div>
              </div>
              <div class="item-action-indicator">
                <CornerDownLeft size={13} />
              </div>
            </button>
          {/each}
        {/if}
      </div>

      <!-- ── Footer Bar ── -->
      <div class="search-footer">
        <div class="kbd-shortcuts">
          <span class="kbd-pill"><kbd>↑</kbd> <kbd>↓</kbd> Navigate</span>
          <span class="kbd-pill"><kbd>↵</kbd> Open / Select</span>
          <span class="kbd-pill"><kbd>Esc</kbd> Close</span>
        </div>
        <div class="footer-status-label">
          {#if searchQuery.trim()}
            <span>{filteredItems.length} result{filteredItems.length !== 1 ? 's' : ''}</span>
          {:else}
            <span>Search pages, tabs &amp; tools</span>
          {/if}
        </div>
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
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    z-index: 10000;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    animation: fadeIn 0.15s ease-out;
  }

  .search-modal {
    width: 100%;
    max-width: 680px;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 16px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.08);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: slideDown 0.18s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .search-header {
    display: flex;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    gap: 12px;
    background: rgba(0, 0, 0, 0.15);
  }

  :global(.search-input-icon) {
    color: var(--color-accent, #00daf3);
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary, #ffffff);
    font-size: 14.5px;
    font-weight: 500;
    font-family: var(--font-sans);
  }

  .search-input::placeholder {
    color: var(--color-text-muted, #94a3b8);
    font-size: 13.5px;
  }

  .search-clear-query-btn {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 3px 6px;
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.12s ease;
  }
  .search-clear-query-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.12);
  }

  .search-close-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
  }

  .search-results-list {
    max-height: 440px;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .search-results-list::-webkit-scrollbar {
    width: 6px;
  }
  .search-results-list::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.12);
    border-radius: 3px;
  }

  .results-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 4px 8px;
    margin-top: 4px;
    margin-bottom: 6px;
  }

  .section-title-group {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .clear-history-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    transition: all 0.12s ease;
  }
  .clear-history-btn:hover {
    color: var(--color-error, #ef4444);
    background: rgba(239, 68, 68, 0.1);
  }

  .results-section-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 4px 8px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .count-tag {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-muted);
  }

  /* ── Recent Searches Pills ── */
  .search-history-container {
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    margin-bottom: 6px;
  }

  .recent-search-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 0 4px;
  }

  .recent-search-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 20px;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .recent-search-pill:hover {
    background: rgba(var(--color-accent-rgb, 0, 218, 243), 0.12);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.3);
    color: var(--color-text-primary);
  }

  /* ── Visited Grid ── */
  .visited-section {
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    margin-bottom: 6px;
  }

  .visited-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    padding: 0 4px;
  }

  .visited-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .visited-card:hover {
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(255, 255, 255, 0.12);
  }

  .visited-card-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .visited-card-sub {
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Search Items ── */
  .search-item-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 10px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .search-item-row.selected {
    background: rgba(0, 218, 243, 0.08);
    border-color: rgba(0, 218, 243, 0.28);
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

  .item-icon-box.is-tab {
    color: #38bdf8;
    background: rgba(56, 189, 248, 0.1);
  }
  .item-icon-box.is-action {
    color: #f59e0b;
    background: rgba(245, 158, 11, 0.1);
  }

  .search-item-row.selected .item-icon-box {
    background: var(--color-accent, #00daf3);
    color: #0b1726;
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
    gap: 8px;
    flex-wrap: wrap;
  }

  .item-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .item-breadcrumb-badge {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-accent, #00daf3);
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.18);
    padding: 1px 7px;
    border-radius: 4px;
  }

  .item-category-tag {
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: 4px;
    margin-left: auto;
  }
  .cat-pages { background: rgba(59, 130, 246, 0.12); color: #60a5fa; }
  .cat-tabs { background: rgba(168, 85, 247, 0.12); color: #c084fc; }
  .cat-actions { background: rgba(245, 158, 11, 0.12); color: #fbbf24; }
  .cat-tools { background: rgba(16, 185, 129, 0.12); color: #34d399; }

  .item-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-action-indicator {
    opacity: 0;
    color: var(--color-accent, #00daf3);
    transition: opacity 0.12s ease;
    padding-left: 4px;
  }

  .search-item-row.selected .item-action-indicator {
    opacity: 1;
  }

  .search-empty-state {
    padding: 40px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    color: var(--color-text-muted);
  }

  :global(.empty-icon) {
    color: var(--color-text-muted);
    opacity: 0.4;
  }

  .empty-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .empty-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    max-width: 360px;
  }

  .search-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    border-top: 1px solid var(--color-border, rgba(255, 255, 255, 0.08));
    background: rgba(0, 0, 0, 0.2);
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .kbd-shortcuts {
    display: flex;
    gap: 12px;
    align-items: center;
  }

  .kbd-pill {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  kbd {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
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
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.15) !important;
  }

  :global(html.light-mode) .search-header {
    background: #F8FAFC !important;
    border-bottom-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .search-footer {
    background: #F8FAFC !important;
    border-top-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .search-item-row.selected {
    background: rgba(2, 132, 199, 0.08) !important;
    border-color: rgba(2, 132, 199, 0.25) !important;
  }

  :global(html.light-mode) .item-icon-box {
    background: #F1F5F9 !important;
    color: #475569 !important;
  }

  :global(html.light-mode) .item-breadcrumb-badge {
    color: #0284c7 !important;
    background: #e0f2fe !important;
    border-color: #bae6fd !important;
  }

  :global(html.light-mode) .recent-search-pill {
    background: #F1F5F9 !important;
    border-color: #E2E8F0 !important;
    color: #475569 !important;
  }
  :global(html.light-mode) .recent-search-pill:hover {
    background: #E0F2FE !important;
    border-color: #BAE6FD !important;
    color: #0284C7 !important;
  }

  :global(html.light-mode) .visited-card {
    background: #F8FAFC !important;
    border-color: #E2E8F0 !important;
  }
  :global(html.light-mode) .visited-card:hover {
    background: #F1F5F9 !important;
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
