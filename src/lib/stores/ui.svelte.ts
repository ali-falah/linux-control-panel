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
  | 'journal-logs'
  | 'pm2-manager';
export interface VisitedItem {
  id: string;
  title: string;
  subtitle?: string;
  tabId: TabId;
  subTab?: string;
  category: string;
}

export type AccentColor = 'cyan' | 'emerald' | 'purple' | 'amber' | 'rose' | 'indigo' | 'sapphire' | 'mint' | 'slate';

class UIStore {
  activeTab = $state<TabId>('system-dashboard');
  /** Recently visited tabs history for sidebar submenu */
  recentTabs = $state<TabId[]>([]);
  /** Navigation history stack — used by PageHeader back button */
  private navHistory = $state<TabId[]>([]);
  sidebarCollapsed = $state(true);
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light'>('dark');
  accentColor = $state<AccentColor>('cyan');
  isOled = $state(false);
  isDrawerDocked = $state(false);
  tableDensity = $state<'compact' | 'normal' | 'spacious'>('normal');
  settingsModalOpen = $state(false);
  searchModalOpen = $state(false);
  /** Target subtab to activate when navigating into a module */
  targetSubTab = $state<string | null>(null);
  /** Payload passed during cross-module navigation */
  navigationPayload = $state<any>(null);
  /** Recent search queries */
  recentSearches = $state<string[]>([]);
  /** Recently visited pages and subtabs */
  recentVisitedItems = $state<VisitedItem[]>([]);
  version = $state<string>(typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0');
  isWindowFocused = $state(true);
  isDocumentVisible = $state(true);

  initVisibilityListener() {
    if (typeof window !== 'undefined') {
      this.isDocumentVisible = !document.hidden;
      this.isWindowFocused = document.hasFocus ? document.hasFocus() : true;

      document.addEventListener('visibilitychange', () => {
        this.isDocumentVisible = !document.hidden;
      });
      window.addEventListener('focus', () => {
        this.isWindowFocused = true;
        this.isDocumentVisible = true;
      });
      window.addEventListener('blur', () => {
        this.isWindowFocused = false;
      });
    }
  }

  get isThrottled(): boolean {
    return !this.isDocumentVisible || !this.isWindowFocused;
  }

  openSettingsModal() {
    this.settingsModalOpen = true;
  }

  closeSettingsModal() {
    this.settingsModalOpen = false;
  }

  openSearchModal() {
    this.searchModalOpen = true;
  }

  closeSearchModal() {
    this.searchModalOpen = false;
  }

  toggleSearchModal() {
    this.searchModalOpen = !this.searchModalOpen;
  }

  navigateTo(tab: TabId, subTab?: string, payload?: any) {
    if (subTab) {
      this.targetSubTab = subTab;
    }
    if (payload !== undefined) {
      this.navigationPayload = payload;
    }
    this.setActiveTab(tab);
  }

  initSearchHistory() {
    if (typeof window !== 'undefined') {
      try {
        const rawSearches = localStorage.getItem('app_recent_searches');
        if (rawSearches) {
          const parsed = JSON.parse(rawSearches);
          if (Array.isArray(parsed)) this.recentSearches = parsed;
        }
        const rawVisited = localStorage.getItem('app_recent_visited_items');
        if (rawVisited) {
          const parsed = JSON.parse(rawVisited);
          if (Array.isArray(parsed)) this.recentVisitedItems = parsed;
        }
      } catch {
        this.recentSearches = [];
        this.recentVisitedItems = [];
      }
    }
  }

  recordRecentSearch(query: string) {
    const trimmed = query.trim();
    if (!trimmed) return;
    const current = Array.isArray(this.recentSearches) ? this.recentSearches : [];
    const filtered = current.filter(s => s.toLowerCase() !== trimmed.toLowerCase());
    this.recentSearches = [trimmed, ...filtered].slice(0, 8);
    if (typeof window !== 'undefined') {
      try {
        localStorage.setItem('app_recent_searches', JSON.stringify(this.recentSearches));
      } catch {}
    }
  }

  recordVisitedItem(item: { id: string; title: string; subtitle?: string; tabId: TabId; subTab?: string; category: string }) {
    const current = Array.isArray(this.recentVisitedItems) ? this.recentVisitedItems : [];
    const filtered = current.filter(i => i.id !== item.id);
    this.recentVisitedItems = [item, ...filtered].slice(0, 8);
    if (typeof window !== 'undefined') {
      try {
        localStorage.setItem('app_recent_visited_items', JSON.stringify(this.recentVisitedItems));
      } catch {}
    }
  }

  clearRecentSearches() {
    this.recentSearches = [];
    if (typeof window !== 'undefined') {
      try {
        localStorage.removeItem('app_recent_searches');
      } catch {}
    }
  }

  clearRecentVisited() {
    this.recentVisitedItems = [];
    if (typeof window !== 'undefined') {
      try {
        localStorage.removeItem('app_recent_visited_items');
      } catch {}
    }
  }

  initTableDensity() {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('app_table_density') as 'compact' | 'normal' | 'spacious';
      if (saved === 'compact' || saved === 'normal' || saved === 'spacious') {
        this.tableDensity = saved;
      }
    }
  }

  setTableDensity(density: 'compact' | 'normal' | 'spacious') {
    this.tableDensity = density;
    if (typeof window !== 'undefined') {
      localStorage.setItem('app_table_density', density);
    }
  }

  toggleTableDensity() {
    if (this.tableDensity === 'compact') this.setTableDensity('normal');
    else if (this.tableDensity === 'normal') this.setTableDensity('spacious');
    else this.setTableDensity('compact');
  }

  preAppliedJournalPriority = $state<string>('all');
  preAppliedJournalSearch = $state<string>('');
  preAppliedJournalUnit = $state<string>('');
  availableUpdatesCount = $state<number>(0);

  jumpToJournalService(serviceName: string) {
    this.preAppliedJournalUnit = serviceName;
    this.preAppliedJournalSearch = '';
    this.navigateTo('journal-logs');
  }

  serviceFilter = $state<string | null>(null);
  serviceSearchQuery = $state<string>('');
  processSearchQuery = $state<string>('');
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
      const savedTheme = localStorage.getItem('app_theme') as 'dark' | 'light';
      if (savedTheme) {
        this.theme = savedTheme;
      } else {
        this.theme = 'dark';
      }

      const savedAccent = localStorage.getItem('app_accent_color') as 'cyan' | 'emerald' | 'purple' | 'amber' | 'slate';
      if (savedAccent) {
        this.accentColor = savedAccent;
      }

      const savedOled = localStorage.getItem('app_oled_mode');
      if (savedOled !== null) {
        this.isOled = savedOled === 'true';
      }

      const savedDocked = localStorage.getItem('ui_drawer_docked');
      if (savedDocked !== null) {
        this.isDrawerDocked = savedDocked === 'true';
      }

      this.applyTheme();
    }
  }

  setAccentColor(accent: AccentColor) {
    this.accentColor = accent;
    if (typeof window !== 'undefined') {
      localStorage.setItem('app_accent_color', accent);
      this.applyTheme();
    }
  }

  toggleOled(enabled?: boolean) {
    this.isOled = enabled !== undefined ? enabled : !this.isOled;
    if (typeof window !== 'undefined') {
      localStorage.setItem('app_oled_mode', String(this.isOled));
      this.applyTheme();
    }
  }

  toggleDrawerDocked(docked?: boolean) {
    this.isDrawerDocked = docked !== undefined ? docked : !this.isDrawerDocked;
    if (typeof window !== 'undefined') {
      localStorage.setItem('ui_drawer_docked', String(this.isDrawerDocked));
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

      document.documentElement.setAttribute('data-accent', this.accentColor);
      document.documentElement.setAttribute('data-oled', this.isOled ? 'true' : 'false');
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
