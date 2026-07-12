export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  duration?: number;
}

export type TabId =
  | 'system-monitor'
  | 'repo-manager'
  | 'dnf-history'
  | 'copr-browser'
  | 'flatpak-rpm'
  | 'startup-manager'
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
  | 'device-manager'
  | 'network-manager'
  | 'app-manager';

class UIStore {
  activeTab = $state<TabId>('system-monitor');
  sidebarCollapsed = $state(false);
  toasts = $state<Toast[]>([]);
  /** Interface name to auto-select when network-manager opens (set by IP popover deep-link) */
  selectedInterface = $state<string | null>(null);
  confirmDialog = $state<{
    isOpen: boolean;
    title: string;
    message: string;
    onConfirm: (() => void) | null;
    danger?: boolean;
  }>({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: null,
    danger: false,
  });

  setActiveTab(tab: TabId) {
    this.activeTab = tab;
  }

  /** Navigate to a tab and pre-select a network interface (used by IP popover deep-link). */
  setActiveTabWithInterface(tab: TabId, ifaceName: string) {
    this.selectedInterface = ifaceName;
    this.activeTab = tab;
  }

  clearSelectedInterface() {
    this.selectedInterface = null;
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
  }

  addToast(message: string, type: ToastType = 'info', duration = 4000) {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    this.toasts = [...this.toasts, { id, message, type, duration }];
    if (duration > 0) {
      setTimeout(() => this.removeToast(id), duration);
    }
    return id;
  }

  removeToast(id: string) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  confirm(title: string, message: string, onConfirm: () => void, danger = false) {
    this.confirmDialog = { isOpen: true, title, message, onConfirm, danger };
  }

  closeConfirm() {
    this.confirmDialog = { isOpen: false, title: '', message: '', onConfirm: null, danger: false };
  }
}

export const uiStore = new UIStore();
