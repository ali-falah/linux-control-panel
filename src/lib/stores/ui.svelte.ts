export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  duration?: number;
  actionLabel?: string;
  onAction?: () => void;
}

export type TabId =
  | 'system-dashboard'
  | 'system-monitor'
  | 'repo-manager'
  | 'dnf-history'
  | 'copr-browser'
  | 'flatpak-rpm'

  | 'service-manager'
  | 'hosts-manager'
  | 'user-manager'
  | 'firewall-manager'
  | 'grub-manager'
  | 'selinux-manager'
  | 'cron-manager'
  | 'env-manager'
  | 'nginx-manager'
  | 'shell-env'
  | 'security-auditor'
  | 'ssh-cert-manager'
  | 'device-manager'
  | 'network-manager'
  | 'app-manager'
  | 'journal-logs';

class UIStore {
  activeTab = $state<TabId>('system-dashboard');
  /** Recently visited tabs history for sidebar submenu */
  recentTabs = $state<TabId[]>([]);
  /** Navigation history stack — used by PageHeader back button */
  private navHistory = $state<TabId[]>([]);
  sidebarCollapsed = $state(true);
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light'>('dark');
  settingsModalOpen = $state(false);

  openSettingsModal() {
    this.settingsModalOpen = true;
  }

  closeSettingsModal() {
    this.settingsModalOpen = false;
  }
  preAppliedJournalPriority = $state<string>('all');
  serviceFilter = $state<string | null>(null);
  appSourceFilter = $state<'All' | 'RPM' | 'Flatpak' | 'AppImage' | 'Duplicates' | null>(null);
  securitySeverityFilter = $state<'Critical' | 'Warning' | 'Good' | 'all' | null>(null);
  /** Category to pre-select in SecurityAuditor (set by deep-links from other modules) */
  securityCategoryFilter = $state<string | null>(null);
  /** Interface name to auto-select when network-manager opens (set by IP popover deep-link) */
  selectedInterface = $state<string | null>(null);
  confirmDialog = $state<{
    isOpen: boolean;
    title: string;
    message: string;
    onConfirm: (() => void | Promise<void>) | null;
    danger?: boolean;
  }>({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: null,
    danger: false,
  });

  initTheme() {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('app_theme') as 'dark' | 'light';
      if (saved) {
        this.theme = saved;
      } else {
        this.theme = 'dark';
      }
      this.applyTheme();
    }
  }

  toggleTheme() {
    this.theme = this.theme === 'dark' ? 'light' : 'dark';
    if (typeof window !== 'undefined') {
      localStorage.setItem('app_theme', this.theme);
      this.applyTheme();
    }
  }

  private applyTheme() {
    if (typeof document !== 'undefined') {
      if (this.theme === 'light') {
        document.documentElement.classList.add('light-mode');
        document.documentElement.setAttribute('data-theme', 'light');
      } else {
        document.documentElement.classList.remove('light-mode');
        document.documentElement.setAttribute('data-theme', 'dark');
      }
    }
  }

  setActiveTab(tab: TabId) {
    if (tab !== this.activeTab) {
      this.navHistory = [...this.navHistory, this.activeTab];
      if (tab !== 'system-dashboard') {
        const filtered = this.recentTabs.filter(t => t !== tab);
        this.recentTabs = [tab, ...filtered].slice(0, 5);
      }
    }
    this.activeTab = tab;
  }

  /** Centralized error handler detecting PolicyKit / sudo authorization cancellations */
  handleError(err: unknown, defaultMessage = 'An unexpected error occurred.') {
    let msg = defaultMessage;
    if (typeof err === 'string') {
      msg = err;
    } else if (err && typeof err === 'object') {
      if ('message' in err) msg = String((err as any).message);
      else if ('kind' in err && 'message' in (err as any)) msg = String((err as any).message);
      else msg = JSON.stringify(err);
    }

    const lower = msg.toLowerCase();
    if (
      lower.includes('polkit') ||
      lower.includes('not authorized') ||
      lower.includes('canceled') ||
      lower.includes('cancelled') ||
      lower.includes('dismissed') ||
      lower.includes('authentication failed')
    ) {
      this.addToast('Administrative Privileges Required: PolicyKit authorization was canceled or denied.', 'warning', 7000);
    } else {
      this.addToast(msg, 'error', 6000);
    }
  }

  goBack() {
    if (this.navHistory.length > 0) {
      const prev = this.navHistory[this.navHistory.length - 1];
      this.navHistory = this.navHistory.slice(0, -1);
      this.activeTab = prev;
    }
  }

  get canGoBack() {
    return this.navHistory.length > 0;
  }

  /** Navigate to a tab and pre-select a network interface (used by IP popover deep-link). */
  setActiveTabWithInterface(tab: TabId, ifaceName: string) {
    if (tab !== this.activeTab) {
      this.navHistory = [...this.navHistory, this.activeTab];
    }
    this.selectedInterface = ifaceName;
    this.activeTab = tab;
  }

  clearSelectedInterface() {
    this.selectedInterface = null;
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
  }

  addToast(message: string, type: ToastType = 'info', duration = 4000, actionLabel?: string, onAction?: () => void) {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    this.toasts = [...this.toasts, { id, message, type, duration, actionLabel, onAction }];
    if (duration > 0) {
      setTimeout(() => this.removeToast(id), duration);
    }
    return id;
  }

  showToast(message: string, type: ToastType = 'info', duration = 4000, actionLabel?: string, onAction?: () => void) {
    return this.addToast(message, type, duration, actionLabel, onAction);
  }

  removeToast(id: string) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  confirm(title: string, message: string, onConfirm: () => void | Promise<void>, danger = false) {
    this.confirmDialog = { isOpen: true, title, message, onConfirm, danger };
  }

  closeConfirm() {
    this.confirmDialog = { isOpen: false, title: '', message: '', onConfirm: null, danger: false };
  }
}

export const uiStore = new UIStore();
