<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    LayoutDashboard, HardDrive, Server, Activity, RefreshCw,
    AlertTriangle, ChevronRight,
    Terminal, CheckCircle2, AlertCircle, ArrowUpRight, ArrowDown, ArrowUp,
    Zap, Layers, Package, Boxes, Play, Pause, Square, RotateCcw, ShieldCheck, Database,
    FileText, SlidersHorizontal, Plus, Trash2, Check, Search, X,
    Copy, Globe, Folder, Flame, MoreHorizontal
  } from '@lucide/svelte';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import ContextMenu, { type ContextMenuItem } from '../components/ui/ContextMenu.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { portal } from '../actions/portal.ts';
  import { open as openUrl } from '@tauri-apps/plugin-shell';



  interface ProcessItem {
    pid: number;
    name: string;
    cpu_percent: number;
    mem_percent?: number;
    memory_percent?: number;
    mem_rss_mb?: number;
    user: string;
    cmdline?: string;
    status?: string;
    owned?: boolean;
  }

  interface ServiceDaemon {
    name: string;
    label: string;
    status: 'active' | 'inactive' | 'failed' | 'unknown';
    subState: string;
  }

  let osInfo = $state<any>(null);
  let systemStats = $state<any>(null);
  let diskUsage = $state<any[]>([]);
  let networkInterfaces = $state<any[]>([]);
  let appFootprint = $state<Record<'Flatpak' | 'RPM', { count: number; sizeBytes: number; sizesLoaded: number }> | null>(null);
  let appFootprintError = $state(false);

  // Initial Top Processes (Guarantees immediate rich rendering without empty layout flash)
  let topProcesses = $state<ProcessItem[]>([]);
  let processesLoaded = $state(false);
  let processesError = $state(false);

  const DEFAULT_WATCHDOG_SERVICES: ServiceDaemon[] = [
    { name: 'systemd-journald.service', label: 'Journal Logging', status: 'unknown', subState: 'checking' },
    { name: 'firewalld.service', label: 'Firewall Daemon', status: 'unknown', subState: 'checking' },
    { name: 'sshd.service', label: 'OpenSSH Server', status: 'unknown', subState: 'checking' },
    { name: 'NetworkManager.service', label: 'Network Manager', status: 'unknown', subState: 'checking' },
    { name: 'crond.service', label: 'Cron Scheduler', status: 'unknown', subState: 'checking' },
    { name: 'nginx.service', label: 'NGINX Web Server', status: 'unknown', subState: 'checking' }
  ];

  function getInitialWatchdogServices(): ServiceDaemon[] {
    try {
      const stored = localStorage.getItem('dashboard_watchdog_services');
      if (stored) {
        const parsed = JSON.parse(stored);
        // An empty list is a valid choice: the user unpinned every service and
        // does not want the defaults back on the next launch.
        if (Array.isArray(parsed)) {
          return parsed.map(svc => ({ ...svc, status: 'unknown', subState: 'checking' }));
        }
      }
    } catch {}
    return DEFAULT_WATCHDOG_SERVICES;
  }

  // Watchdog Services
  let watchdogServices = $state<ServiceDaemon[]>(getInitialWatchdogServices());
  let servicesLoaded = $state(false);
  let servicesError = $state(false);
  let showWatchdogModal = $state(false);
  let showAttentionModal = $state(false);
  let availableUnits = $state<Array<{ name: string; description: string; active_state: string }>>([]);
  let loadingUnits = $state(false);
  let watchdogSearchQuery = $state('');
  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  function handleDragStart(e: DragEvent, index: number) {
    draggedIndex = index;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', String(index));
    }
  }

  function handleDragOver(e: DragEvent, index: number) {
    e.preventDefault();
    if (draggedIndex === null || draggedIndex === index) return;
    dragOverIndex = index;
  }

  function handleDragLeave(index: number) {
    if (dragOverIndex === index) {
      dragOverIndex = null;
    }
  }

  function handleDrop(e: DragEvent, targetIndex: number) {
    e.preventDefault();
    if (draggedIndex === null || draggedIndex === targetIndex) {
      draggedIndex = null;
      dragOverIndex = null;
      return;
    }
    const updated = [...watchdogServices];
    const [movedItem] = updated.splice(draggedIndex, 1);
    updated.splice(targetIndex, 0, movedItem);
    saveWatchdogServices(updated);
    draggedIndex = null;
    dragOverIndex = null;
    uiStore.addToast(`Reordered watchdog: "${movedItem.label}" moved to position #${targetIndex + 1}`, 'info');
  }

  function handleDragEnd() {
    draggedIndex = null;
    dragOverIndex = null;
  }

  function moveWatchdogService(index: number, direction: -1 | 1) {
    const targetIndex = index + direction;
    if (targetIndex < 0 || targetIndex >= watchdogServices.length) return;
    const updated = [...watchdogServices];
    const [moved] = updated.splice(index, 1);
    updated.splice(targetIndex, 0, moved);
    saveWatchdogServices(updated);
  }

  function saveWatchdogServices(newList: ServiceDaemon[]) {
    watchdogServices = newList;
    try {
      localStorage.setItem('dashboard_watchdog_services', JSON.stringify(newList));
    } catch (e) {
      console.warn('Failed to persist watchdog services:', e);
    }
    fetchServicesWatchdog();
  }

  function togglePinService(name: string, defaultLabel?: string, active_state?: string) {
    const isPinned = watchdogServices.some(s => s.name === name);
    if (isPinned) {
      saveWatchdogServices(watchdogServices.filter(s => s.name !== name));
      uiStore.addToast(`Removed ${name} from Watchdog`, 'info');
    } else {
      const formattedLabel = defaultLabel || name.replace(/\.service$/, '').replace(/[-_]/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
      const newService: ServiceDaemon = {
        name,
        label: formattedLabel,
        status: (active_state === 'active' ? 'active' : active_state === 'failed' ? 'failed' : 'unknown') as any,
        subState: active_state || 'checking'
      };
      saveWatchdogServices([newService, ...watchdogServices]);
      uiStore.addToast(`Pinned ${formattedLabel} to Watchdog`, 'success');
    }
  }

  function resetWatchdogDefaults() {
    saveWatchdogServices(DEFAULT_WATCHDOG_SERVICES);
    uiStore.addToast('Reset Watchdog to default core services', 'info');
  }

  function addPresetBundle(bundle: { name: string; label: string }[]) {
    const currentNames = new Set(watchdogServices.map(s => s.name));
    const toAdd: ServiceDaemon[] = [];
    for (const b of bundle) {
      if (!currentNames.has(b.name)) {
        toAdd.push({
          name: b.name,
          label: b.label,
          status: 'unknown',
          subState: 'checking'
        });
      }
    }
    if (toAdd.length > 0) {
      saveWatchdogServices([...watchdogServices, ...toAdd]);
      uiStore.addToast(`Added ${toAdd.length} services from preset`, 'success');
    } else {
      uiStore.addToast('All services in this preset are already pinned', 'info');
    }
  }

  async function loadAvailableUnitsForModal() {
    if (availableUnits.length > 0) return;
    loadingUnits = true;
    try {
      const units = await invoke<Array<any>>('list_all_units', { userMode: false });
      availableUnits = units
        .filter(u => u.name && u.name.endsWith('.service'))
        .map(u => ({
          name: u.name,
          description: u.description || '',
          active_state: u.active_state || 'unknown'
        }));
    } catch {
      availableUnits = [];
    } finally {
      loadingUnits = false;
    }
  }

  const filteredAvailableUnits = $derived.by(() => {
    const q = watchdogSearchQuery.trim().toLowerCase();
    if (!q) {
      return availableUnits.slice(0, 35);
    }
    return availableUnits.filter(u => 
      u.name.toLowerCase().includes(q) || (u.description && u.description.toLowerCase().includes(q))
    ).slice(0, 60);
  });

  let networkDetails = $state<any>(null);
  let gatewayPing = $state<string>('');
  let systemEvents = $state<any>(null);
  let systemEventsLoaded = $state(false);
  let systemEventsError = $state(false);
  let lastSystemUpdate = $state<string>('');
  let failedServicesCount = $state<number>(0);
  let failedServicesCountLoaded = $state(false);
  let recentLogStream = $state<Array<{ time: string; service: string; level: string; message: string }>>([]);
  let logsLoaded = $state(false);
  let logsError = $state(false);
  let systemStatsError = $state(false);
  let systemStatsLoaded = $state(false);
  let lastDashboardCheck = $state<Date | null>(null);
  /** Timestamp of the last SLOW tier fetch — the freshness of the data behind
   *  the attention items (logs, disk, temperature) */
  let lastSlowCheck = $state<Date | null>(null);
  /** True when the storage read failed — distinguishes "no data" from "error" */
  let diskUsageError = $state(false);
  /** True once the slow tier has answered for disks (success or failure) */
  let diskUsageLoaded = $state(false);
  let lastLogsCheck = $state<Date | null>(null);
  let lastServicesCheck = $state<Date | null>(null);
  let lastSecurityCheck = $state<Date | null>(null);

  // Service Action Execution State
  let serviceActionRunning = $state<string | null>(null);

  async function doServiceAction(serviceName: string, action: 'start' | 'stop' | 'restart') {
    serviceActionRunning = `${serviceName}-${action}`;
    try {
      await invoke('unit_action', { name: serviceName, action, userMode: false });
      const pastTense = action === 'start' ? 'started' : action === 'stop' ? 'stopped' : 'restarted';
      uiStore.addToast(`Service ${serviceName} ${pastTense} successfully`, 'success');
      await fetchServicesWatchdog();
    } catch (e) {
      uiStore.addToast(`Failed to ${action} ${serviceName}: ${e}`, 'error');
    } finally {
      serviceActionRunning = null;
    }
  }

  async function doAllServicesAction(action: 'start' | 'stop') {
    if (serviceActionRunning) return;

    const targets = displayServices.filter(svc =>
      action === 'start'
        ? svc.status !== 'active'
        : svc.status === 'active' && !isCriticalService(svc.name)
    );
    const skipped = action === 'stop'
      ? displayServices.filter(svc => svc.status === 'active' && isCriticalService(svc.name)).length
      : 0;

    if (targets.length === 0) {
      uiStore.addToast(`No services to ${action}`, 'info');
      return;
    }

    serviceActionRunning = 'bulk';
    const failures: string[] = [];
    try {
      for (const svc of targets) {
        try {
          await invoke('unit_action', { name: svc.name, action, userMode: false });
        } catch (e) {
          failures.push(`${svc.name}: ${e}`);
        }
      }

      const succeeded = targets.length - failures.length;
      const skippedNote = skipped > 0
        ? ` (${skipped} critical service${skipped === 1 ? '' : 's'} skipped)`
        : '';
      const verb = action === 'start' ? 'Started' : 'Stopped';

      if (failures.length === 0) {
        uiStore.addToast(`${verb} ${succeeded}/${targets.length} services${skippedNote}`, 'success');
      } else {
        const detail = failures.slice(0, 3).join('; ');
        const more = failures.length > 3 ? ` (+${failures.length - 3} more)` : '';
        uiStore.addToast(
          `${verb} ${succeeded}/${targets.length}; failed: ${detail}${more}${skippedNote}`,
          'warning'
        );
      }

      await fetchServicesWatchdog();
    } finally {
      serviceActionRunning = null;
    }
  }

  function confirmStopAllServices() {
    const running = displayServices.filter(
      svc => svc.status === 'active' && !isCriticalService(svc.name)
    );
    if (running.length === 0) {
      uiStore.addToast('No running services to stop', 'info');
      return;
    }
    uiStore.confirm(
      'Stop All Services',
      `Stop ${running.length} running pinned service${running.length === 1 ? '' : 's'}? Critical core services are skipped.`,
      () => doAllServicesAction('stop'),
      true
    );
  }

  function confirmStopService(svc: ServiceDaemon) {
    if (isCriticalService(svc.name)) {
      uiStore.addToast(`Cannot stop core critical system service "${svc.name}"`, 'error');
      return;
    }
    uiStore.confirm(
      `Stop Service "${svc.label || svc.name}"?`,
      `Are you sure you want to stop "${svc.name}"? Active processes relying on this daemon will terminate immediately.`,
      () => doServiceAction(svc.name, 'stop'),
      true
    );
  }

  function confirmRestartService(svc: ServiceDaemon) {
    uiStore.confirm(
      `Restart Service "${svc.label || svc.name}"?`,
      `Are you sure you want to restart "${svc.name}"? Connections or sessions handled by this service may reset.`,
      () => doServiceAction(svc.name, 'restart'),
      false
    );
  }

  function confirmRestartFromLog(serviceName: string) {
    const unit = serviceName.endsWith('.service') ? serviceName : `${serviceName}.service`;
    uiStore.confirm(
      `Restart Service "${unit}"?`,
      `Are you sure you want to restart "${unit}"?`,
      () => doServiceAction(unit, 'restart'),
      false
    );
  }

  function confirmUnpinService(serviceName: string) {
    uiStore.confirm(
      `Unpin "${serviceName}"?`,
      `Remove "${serviceName}" from your Dashboard watchdog services grid? You can re-pin it anytime using the "+" button.`,
      () => togglePinService(serviceName),
      false
    );
  }

  function isCriticalService(name: string): boolean {
    const clean = name.replace(/\.service$/, '').toLowerCase();
    return ['dbus', 'dbus-broker', 'polkit', 'systemd-journald', 'systemd-logind', 'systemd-udevd', 'init'].includes(clean);
  }

  async function copyToClipboard(text: string, label = 'Copied to clipboard') {
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast(label, 'info');
    } catch (e) {
      uiStore.addToast(`Could not copy to the clipboard: ${e}`, 'error');
    }
  }

  async function searchWeb(query: string) {
    if (!query.trim()) return;
    const url = `https://www.google.com/search?q=${encodeURIComponent(query.trim())}`;
    try {
      await openUrl(url);
      uiStore.addToast('Opening web search in default browser...', 'info');
    } catch {
      try {
        window.open(url, '_blank');
      } catch (e) {
        uiStore.addToast(`Failed to open web browser: ${e}`, 'error');
      }
    }
  }

  // ── Context menus ─────────────────────────────────────────────────────────
  // All five use the shared ContextMenu component: it owns positioning, boundary
  // clamping, focus, keyboard navigation and dismissal. Only the row-specific
  // data and actions live here.

  type MenuBadge = {
    text: string;
    variant: 'success' | 'info' | 'warning' | 'error' | 'muted';
  };

  type DashboardLogEntry = {
    time: string;
    service: string;
    level: string;
    message: string;
  };

  /**
   * Where a menu should open. A context-menu event carries the pointer
   * position; a kebab activated from the keyboard emits a click with no
   * coordinates, so anchor that one to the button.
   */
  function menuPoint(e: MouseEvent): { x: number; y: number } {
    const fromKeyboard = e.type === 'click' && e.clientX === 0 && e.clientY === 0;
    const rect = fromKeyboard
      ? (e.currentTarget as HTMLElement | null)?.getBoundingClientRect()
      : null;
    return rect
      ? { x: rect.left, y: rect.bottom + 4 }
      : { x: e.clientX, y: e.clientY };
  }

  /**
   * Only one menu may be open at a time. Rows stop the click/contextmenu event
   * from reaching the window, so the shared menus cannot close each other
   * through their outside-click listener; do it explicitly.
   */
  function closeContextMenus() {
    serviceContextMenu.open = false;
    logContextMenu.open = false;
    processContextMenu.open = false;
    storageContextMenu.open = false;
    footprintContextMenu.open = false;
  }

  // 1. Service Watchdog
  let serviceContextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    svc: ServiceDaemon | null;
  }>({ open: false, x: 0, y: 0, svc: null });

  function handleServiceContextMenu(e: MouseEvent, svc: ServiceDaemon) {
    e.preventDefault();
    e.stopPropagation();
    closeContextMenus();
    serviceContextMenu = { ...menuPoint(e), open: true, svc };
  }

  const serviceMenuBadges = $derived.by<MenuBadge[]>(() => {
    const svc = serviceContextMenu.svc;
    if (!svc) return [];
    const badges: MenuBadge[] = [];
    if (isCriticalService(svc.name)) badges.push({ text: 'Core', variant: 'error' });
    badges.push({
      text: svc.status,
      variant: svc.status === 'active' ? 'success' : svc.status === 'failed' ? 'error' : 'muted',
    });
    return badges;
  });

  const serviceMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const svc = serviceContextMenu.svc;
    if (!svc) return [];
    const busy = !!serviceActionRunning;
    const critical = isCriticalService(svc.name);
    return [
      {
        label: 'Restart Service',
        icon: RotateCcw,
        color: 'var(--color-warning)',
        disabled: busy,
        action: () => confirmRestartService(svc),
      },
      svc.status === 'active'
        ? {
            label: critical ? 'Stop Service (Locked)' : 'Stop Service',
            icon: Square,
            color: 'var(--color-error)',
            danger: true,
            disabled: critical || busy,
            action: () => confirmStopService(svc),
          }
        : {
            label: 'Start Service',
            icon: Play,
            color: 'var(--color-success)',
            disabled: busy,
            action: () => doServiceAction(svc.name, 'start'),
          },
      { label: '', divider: true },
      {
        label: 'Inspect in Service Manager',
        icon: ArrowUpRight,
        color: 'var(--color-accent)',
        action: () => {
          uiStore.serviceFilter = 'all';
          uiStore.serviceSearchQuery = svc.name.replace(/\.service$/, '');
          uiStore.navigateTo('service-manager');
        },
      },
      {
        label: 'View Logs in Journal',
        icon: FileText,
        color: 'var(--color-accent)',
        action: () => {
          uiStore.preAppliedJournalPriority = 'all';
          uiStore.preAppliedJournalSearch = svc.name;
          uiStore.navigateTo('journal-logs', 'journal');
        },
      },
      {
        label: 'Copy Unit Name / CLI Command',
        icon: Copy,
        action: () =>
          copyToClipboard(`systemctl status ${svc.name}`, 'Copied systemctl status command'),
      },
      { label: '', divider: true },
      {
        label: 'Unpin from Dashboard',
        icon: Trash2,
        danger: true,
        action: () => confirmUnpinService(svc.name),
      },
    ];
  });

  // 2. Journal line
  let logContextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    log: DashboardLogEntry | null;
  }>({ open: false, x: 0, y: 0, log: null });

  function handleLogContextMenu(e: MouseEvent, log: DashboardLogEntry) {
    e.preventDefault();
    e.stopPropagation();
    closeContextMenus();
    logContextMenu = { ...menuPoint(e), open: true, log };
  }

  const logMenuBadge = $derived.by<MenuBadge | undefined>(() => {
    const level = logContextMenu.log?.level;
    if (!level) return undefined;
    return {
      text: level,
      variant: level === 'Err' || level === 'Crit' ? 'error' : level === 'Warn' ? 'warning' : 'info',
    };
  });

  const logMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const log = logContextMenu.log;
    if (!log) return [];
    const items: ContextMenuItem[] = [
      {
        label: `Filter Journal by '${log.service}'`,
        icon: FileText,
        color: 'var(--color-accent)',
        action: () => {
          uiStore.preAppliedJournalPriority = 'all';
          uiStore.preAppliedJournalSearch = log.service;
          uiStore.navigateTo('journal-logs', 'journal');
        },
      },
      {
        label: 'Copy Log Message',
        icon: Copy,
        action: () => copyToClipboard(log.message, 'Copied log message'),
      },
      {
        label: 'Copy Full Log Entry',
        icon: Copy,
        action: () =>
          copyToClipboard(
            `[${log.time}] [${log.service}] [${log.level}] ${log.message}`,
            'Copied full log line'
          ),
      },
    ];

    if (log.service && log.service !== 'kernel' && log.service !== 'system') {
      items.push(
        { label: '', divider: true },
        {
          label: `Inspect '${log.service}' in Service Manager`,
          icon: ArrowUpRight,
          color: 'var(--color-accent)',
          action: () => {
            uiStore.serviceFilter = 'all';
            uiStore.serviceSearchQuery = log.service.replace(/\.service$/, '');
            uiStore.navigateTo('service-manager');
          },
        },
        {
          label: `Restart '${log.service}'`,
          icon: RotateCcw,
          color: 'var(--color-warning)',
          action: () => confirmRestartFromLog(log.service),
        }
      );
    }

    items.push(
      { label: '', divider: true },
      {
        label: 'Search Error on Web',
        icon: Globe,
        color: 'var(--color-text-secondary)',
        action: () => searchWeb(`${log.service} ${log.message}`.trim()),
      }
    );
    return items;
  });

  // 3. Top process
  let processContextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    proc: ProcessItem | null;
  }>({ open: false, x: 0, y: 0, proc: null });

  function handleProcessContextMenu(e: MouseEvent, proc: ProcessItem) {
    e.preventDefault();
    e.stopPropagation();
    closeContextMenus();
    processContextMenu = { ...menuPoint(e), open: true, proc };
  }

  const processMenuBadges = $derived.by<MenuBadge[]>(() => {
    const proc = processContextMenu.proc;
    return proc
      ? [
          { text: `PID ${proc.pid}`, variant: 'info' },
          { text: proc.user, variant: 'muted' },
        ]
      : [];
  });

  const processMenuSubtitle = $derived.by(() => {
    const proc = processContextMenu.proc;
    if (!proc) return '';
    const ram = (proc.mem_percent ?? proc.memory_percent ?? 0).toFixed(1);
    return `${(proc.cpu_percent || 0).toFixed(1)}% CPU · ${ram}% RAM`;
  });

  const processMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const proc = processContextMenu.proc;
    if (!proc) return [];
    return [
      {
        label: 'Inspect in System Monitor',
        icon: ArrowUpRight,
        color: 'var(--color-accent)',
        action: () => {
          uiStore.processSearchQuery = proc.name;
          uiStore.navigateTo('system-monitor', 'processes');
        },
      },
      {
        label: 'View Logs in Journal',
        icon: FileText,
        color: 'var(--color-accent)',
        action: () => {
          uiStore.preAppliedJournalPriority = 'all';
          uiStore.preAppliedJournalSearch = proc.name;
          uiStore.navigateTo('journal-logs', 'journal');
          uiStore.addToast(`Jumped to Journal logs for ${proc.name}`, 'info');
        },
      },
      {
        label: 'Search Process on Web',
        icon: Globe,
        color: 'var(--color-text-secondary)',
        action: () => searchWeb(`${proc.name} linux process`),
      },
      { label: '', divider: true },
      {
        label: `Copy PID (${proc.pid})`,
        icon: Copy,
        action: () => copyToClipboard(proc.pid.toString(), `Copied PID ${proc.pid}`),
      },
      {
        label: 'Copy Command Line',
        icon: Terminal,
        action: () => copyToClipboard(proc.cmdline || proc.name, 'Copied command line'),
      },
      {
        label: `Copy \`ps -fp ${proc.pid}\``,
        icon: Copy,
        action: () => copyToClipboard(`ps -fp ${proc.pid}`, 'Copied CLI inspection command'),
      },
      { label: '', divider: true },
      {
        label: 'Terminate (SIGTERM)',
        icon: Square,
        danger: true,
        action: () => confirmKillProcess(proc, 15),
      },
      {
        label: 'Force Kill (SIGKILL)',
        icon: Flame,
        danger: true,
        action: () => confirmKillProcess(proc, 9),
      },
    ];
  });

  function confirmKillProcess(proc: ProcessItem, signal: number = 15) {
    if (proc.pid <= 100) {
      uiStore.addToast(`Cannot terminate system PID ${proc.pid} (protected process).`, 'error');
      return;
    }
    if (proc.owned === false) {
      uiStore.addToast(
        `PID ${proc.pid} belongs to ${proc.user}; administrator rights are required to signal it.`,
        'error'
      );
      return;
    }
    const isForce = signal === 9;
    const signalName = isForce ? 'SIGKILL (Force Kill)' : 'SIGTERM (Safe Terminate)';
    uiStore.confirm(
      `${isForce ? 'Force Kill' : 'Terminate'} Process "${proc.name}" (PID ${proc.pid})?`,
      `Are you sure you want to send ${signalName} to process "${proc.name}" (PID ${proc.pid}, user: ${proc.user})? Unsaved progress in this process will be terminated.`,
      async () => {
        try {
          const res = await invoke<string>('kill_process', { pid: proc.pid, signal });
          uiStore.addToast(res || `Process ${proc.name} (PID ${proc.pid}) terminated`, 'success');
          await fetchFastData();
        } catch (e) {
          uiStore.addToast(`Failed to terminate process: ${e}`, 'error');
        }
      },
      isForce
    );
  }

  // 4. Storage & disks
  interface StorageContextItem {
    title: string;
    mount?: string;
    device: string;
    fs_type?: string;
    used_gb: number;
    total_gb: number;
  }

  let storageContextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    item: StorageContextItem | null;
  }>({ open: false, x: 0, y: 0, item: null });

  function handleStorageContextMenu(e: MouseEvent, item: StorageContextItem) {
    e.preventDefault();
    e.stopPropagation();
    closeContextMenus();
    storageContextMenu = { ...menuPoint(e), open: true, item };
  }

  const storageMenuBadge = $derived.by<MenuBadge | undefined>(() => {
    const fsType = storageContextMenu.item?.fs_type;
    return fsType ? { text: fsType.toUpperCase(), variant: 'info' } : undefined;
  });

  const storageMenuSubtitle = $derived.by(() => {
    const item = storageContextMenu.item;
    if (!item) return '';
    const pct = item.total_gb > 0 ? ((item.used_gb / item.total_gb) * 100).toFixed(1) : '0.0';
    return `${formatStorageBytes(item.used_gb)} used of ${formatStorageBytes(item.total_gb)} (${pct}%)`;
  });

  const storageMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const item = storageContextMenu.item;
    if (!item) return [];
    const mount = item.mount;
    const items: ContextMenuItem[] = [
      {
        label: 'Inspect Storage Path Breakdown',
        icon: Search,
        color: 'var(--color-accent)',
        action: () =>
          openStoragePathModal(
            mount || item.device,
            item.device,
            item.used_gb,
            item.total_gb,
            item.fs_type
          ),
      },
      {
        label: 'Open Device & Disk Manager',
        icon: HardDrive,
        color: 'var(--color-accent)',
        action: () => uiStore.navigateTo('device-manager', 'list'),
      },
      {
        label: 'Check SMART Health & Diagnostics',
        icon: Activity,
        color: 'var(--color-success)',
        action: () => {
          uiStore.navigateTo('device-manager', 'smart');
          uiStore.addToast('Jumped to Disk Health & Diagnostics', 'info');
        },
      },
      { label: '', divider: true },
      {
        label: `Copy Device Node (${item.device})`,
        icon: Copy,
        action: () => copyToClipboard(item.device, `Copied device node ${item.device}`),
      },
    ];

    if (mount) {
      items.push({
        label: `Copy Mount Point (${mount})`,
        icon: Folder,
        action: () => copyToClipboard(mount, `Copied mount point ${mount}`),
      });
    }

    const cmd = mount ? `df -h ${mount}` : `lsblk -f ${item.device}`;
    items.push({
      label: `Copy \`${cmd}\``,
      icon: Terminal,
      action: () => copyToClipboard(cmd, `Copied command ${cmd}`),
    });
    return items;
  });

  // 5. App & disk footprint
  interface FootprintContextItem {
    title: string;
    size: string;
    type: 'flatpak' | 'rpm';
  }

  let footprintContextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    item: FootprintContextItem | null;
  }>({ open: false, x: 0, y: 0, item: null });

  function handleFootprintContextMenu(e: MouseEvent, item: FootprintContextItem) {
    e.preventDefault();
    e.stopPropagation();
    closeContextMenus();
    footprintContextMenu = { ...menuPoint(e), open: true, item };
  }

  const footprintMenuBadge = $derived.by<MenuBadge | undefined>(() =>
    footprintContextMenu.item
      ? { text: footprintContextMenu.item.size, variant: 'info' }
      : undefined
  );

  const footprintMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const item = footprintContextMenu.item;
    if (!item) return [];
    if (item.type === 'flatpak') {
      return [
        {
          label: 'Open Flatpak Apps in App Manager',
          icon: Package,
          color: 'var(--color-accent)',
          action: () => uiStore.navigateTo('app-manager', 'Flatpak'),
        },
        {
          label: 'Scan for Unused / Duplicate Flatpaks',
          icon: Zap,
          color: 'var(--color-warning)',
          action: () => uiStore.navigateTo('app-manager', 'Duplicates'),
        },
        {
          label: 'Copy `flatpak list` Command',
          icon: Terminal,
          action: () =>
            copyToClipboard(
              'flatpak list --app --columns=name,size,application',
              'Copied flatpak list command'
            ),
        },
      ];
    }
    return [
      {
        label: 'Open Native RPM Packages',
        icon: Package,
        color: 'var(--color-warning)',
        action: () => uiStore.navigateTo('app-manager', 'RPM'),
      },
      {
        label: 'Manage DNF / RPM Repositories',
        icon: Database,
        color: 'var(--color-accent)',
        action: () => uiStore.navigateTo('repo-manager'),
      },
      {
        label: 'Copy Largest RPMs Query Command',
        icon: Terminal,
        action: () =>
          copyToClipboard(
            'rpm -qa --qf "%{SIZE} %{NAME}\\n" | sort -rn | head -20',
            'Copied RPM size query command'
          ),
      },
    ];
  });

  function activateOnKeyboard(e: KeyboardEvent, action: () => void) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      action();
    }
  }

  // Sparkline history tracking
  let cpuHistory = $state<number[]>([]);
  let ramHistory = $state<number[]>([]);
  /** How many samples the metric sparklines keep */
  const SPARK_POINTS = 30;

  let cpuTemp = $state<string>('');
  let cpuTempAvailable = $state(false);
  let currentRxRate = $state<string>('0.0 KB/s');
  let currentTxRate = $state<string>('0.0 KB/s');
  let prevTrafficTime = $state(0);
  let prevTotalRx = $state(0);
  let prevTotalTx = $state(0);
  let trafficAvailable = $state(false);
  let trafficPolls = $state(0);

  function formatNetRate(bytesPerSec: number): string {
    if (!bytesPerSec || bytesPerSec <= 0) return '0.0 KB/s';
    if (bytesPerSec < 1024) return `${bytesPerSec.toFixed(0)} B/s`;
    if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
    return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  }

  let cpuTempNumeric = $derived.by(() => {
    if (cpuTemp) {
      const match = cpuTemp.match(/(\d+(\.\d+)?)/);
      if (match) {
        const val = parseFloat(match[1]);
        if (!isNaN(val)) return val;
      }
    }
    return null;
  });

  let securityReport = $state<any>(null);
  let loadingSecurity = $state(false);
  let securityError = $state(false);
  let mutedIds = $state<string[]>([]);

  $effect(() => {
    try {
      const raw = localStorage.getItem('security_muted_findings');
      if (raw) mutedIds = JSON.parse(raw);
    } catch {}
  });

  let effectiveDashboardScore = $derived.by(() => {
    if (!securityReport) return null;
    if (!mutedIds || mutedIds.length === 0) return securityReport.score;
    const activeFindings = securityReport.findings.filter((f: any) => !mutedIds.includes(f.id));
    if (activeFindings.length === 0) return 100;
    const hasUnmutedCritical = activeFindings.some((f: any) => f.severity === 'Critical' && !f.is_resolved);
    let totalCur = 0;
    let totalMax = 0;
    for (const cs of securityReport.category_scores) {
      const catFindings = activeFindings.filter((f: any) => f.category === cs.category);
      const catPassed = catFindings.filter((f: any) => f.is_resolved).length;
      if (catFindings.length > 0) {
        const catPct = Math.round((catPassed / catFindings.length) * 100);
        totalCur += (catPct * cs.max_score) / 100;
        totalMax += cs.max_score;
      }
    }
    const rawScore = totalMax > 0 ? Math.round((totalCur / totalMax) * 100) : 100;
    return hasUnmutedCritical ? Math.min(rawScore, 60) : rawScore;
  });

  async function fetchSecurityReport(forceRefresh: boolean | MouseEvent = false) {
    loadingSecurity = true;
    securityError = false;
    const shouldForce = typeof forceRefresh === 'boolean' ? forceRefresh : false;
    try {
      securityReport = await invoke('security_run_audit', { forceRefresh: shouldForce });
      lastSecurityCheck = new Date();
    } catch (e) {
      securityError = true;
      console.error("Error fetching security report:", e);
    } finally {
      loadingSecurity = false;
    }
  }

  async function fetchSystemEvents() {
    try {
      systemEvents = await invoke('get_system_events');
      systemEventsLoaded = true;
      systemEventsError = false;
    } catch (e) {
      systemEventsError = true;
      console.error("Error fetching system events:", e);
    } finally {
      systemEventsLoaded = true;
    }
  }

  async function fetchNetworkDetails() {
    try {
      networkDetails = await invoke('get_network_details');
      updateGatewayPing();
    } catch (e) {
      console.error("Error fetching network details:", e);
    }
  }

  async function updateGatewayPing() {
    if (networkDetails && networkDetails.gateway) {
      invoke<string>('ping_gateway', { ip: networkDetails.gateway })
        .then(latency => {
          gatewayPing = latency;
        })
        .catch(() => {
          gatewayPing = 'timeout';
        });
    }
  }

  async function fetchRecentLogs() {
    logsError = false;
    try {
      // The backend returns a page object with compact, pre-parsed entries
      // (newest first). The card only shows the 5 newest warnings/errors,
      // so there is no point pulling a larger set.
      const page = await invoke<any>('get_journal_logs', { priority: 4, maxEntries: 20 });
      const entries: any[] = Array.isArray(page) ? page : (page?.entries ?? []);
      if (entries.length > 0) {
        recentLogStream = entries.slice(0, 20).map(obj => {
          const ts = obj.__REALTIME_TIMESTAMP
            ? new Date(parseInt(obj.__REALTIME_TIMESTAMP) / 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
            : '—';
          const svc = obj.SYSLOG_IDENTIFIER || obj._COMM || obj._SYSTEMD_UNIT || 'systemd';
          const prio = Number(obj.PRIORITY ?? 6);
          const level = Number.isNaN(prio) ? 'Info' : prio <= 3 ? 'Err' : prio === 4 ? 'Warn' : 'Info';
          return { time: ts, service: svc, level, message: obj.MESSAGE || '' };
        });
      } else {
        recentLogStream = [];
      }
    } catch {
      recentLogStream = [];
      logsError = true;
    } finally {
      logsLoaded = true;
      lastLogsCheck = new Date();
    }
  }

  let selectedStorageDetail = $state<{
    title: string;
    device: string;
    mount?: string;
    fs_type?: string;
    total_gb: number;
    used_gb: number;
    free_gb: number;
    percent: number;
  } | null>(null);

  function openStoragePathModal(
    mount: string,
    device: string,
    used_gb: number,
    total_gb: number,
    fs_type?: string
  ) {
    const free = Math.max(0, total_gb - used_gb);
    selectedStorageDetail = {
      title: `Storage Path — ${mount}`,
      device,
      mount,
      fs_type,
      total_gb,
      used_gb,
      free_gb: free,
      percent: (used_gb / (total_gb || 1)) * 100
    };
  }

  function formatStorageBytes(gb: number) {
    if (!gb || gb <= 0) return '0 B';
    if (gb < 1.0) {
      const mb = gb * 1024;
      return `${mb.toFixed(0)} MB`;
    }
    return `${gb.toFixed(1)} GB`;
  }

  function formatAppSize(bytes: number) {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function fetchServicesWatchdog() {
    try {
      const names = watchdogServices.map(s => s.name);
      const statuses = await invoke<Array<{ name: string; active_state: string; sub_state: string }>>(
        'get_services_status',
        { names, userMode: false }
      );
      if (Array.isArray(statuses) && statuses.length > 0) {
        watchdogServices = watchdogServices.map(svc => {
          const match = statuses.find(s => s.name === svc.name || s.name === svc.name.replace('.service', ''));
          if (match) {
            return {
              ...svc,
              status: match.active_state === 'active' ? 'active' : match.active_state === 'failed' ? 'failed' : match.active_state === 'inactive' ? 'inactive' : 'unknown',
              subState: match.sub_state || match.active_state
            };
          }
          return svc;
        });
      }
      servicesLoaded = true;
      servicesError = false;
      lastServicesCheck = new Date();
    } catch (e) {
      servicesError = true;
      watchdogServices = watchdogServices.map(svc => ({ ...svc, status: 'unknown', subState: 'unavailable' }));
      servicesLoaded = true;
      lastServicesCheck = new Date();
      console.warn("Could not fetch units watchdog:", e);
    }
  }

  async function fetchTopProcesses() {
    try {
      const procList = await invoke<ProcessItem[]>('get_process_list');
      topProcesses = Array.isArray(procList) ? procList
        .filter(p => p.name && p.name !== 'systemd' && p.pid !== 1)
        .sort((a, b) => (b.cpu_percent || 0) - (a.cpu_percent || 0))
        .slice(0, 5) : [];
      processesLoaded = true;
      processesError = false;
    } catch (e) {
      processesError = true;
      topProcesses = [];
      console.warn("Could not fetch top processes:", e);
    }
  }

  /**
   * Fast tier — cheap, high-frequency telemetry that drives the metric tiles.
   * Runs every 4 s: two cheap reads only.
   */
  async function fetchFastData() {
    try {
      const [stats, traffic] = await Promise.all([
        invoke('get_system_stats'),
        invoke<Array<{ interface: string; rx_bytes: number; tx_bytes: number }>>('get_network_traffic').catch(() => [])
      ]);

      systemStats = stats;
      systemStatsError = false;
      systemStatsLoaded = true;
      lastDashboardCheck = new Date();

      // Calculate real live network throughput
      trafficAvailable = Array.isArray(traffic) && traffic.length > 0;
      if (trafficAvailable) {
        trafficPolls += 1;
        const now = Date.now();
        let totalRx = 0;
        let totalTx = 0;
        for (const item of traffic) {
          if (item.interface !== 'lo' && !item.interface.startsWith('vir') && !item.interface.startsWith('docker')) {
            totalRx += item.rx_bytes;
            totalTx += item.tx_bytes;
          }
        }

        if (prevTrafficTime > 0 && now > prevTrafficTime) {
          const dt = (now - prevTrafficTime) / 1000;
          const rxRate = Math.max(0, (totalRx - prevTotalRx) / dt);
          const txRate = Math.max(0, (totalTx - prevTotalTx) / dt);

          currentRxRate = formatNetRate(rxRate);
          currentTxRate = formatNetRate(txRate);
        }
        prevTrafficTime = now;
        prevTotalRx = totalRx;
        prevTotalTx = totalTx;
      }

      // Update sparkline histories (rolling window — slice(-(N-1)) keeps the
      // last N points; slice(1) would pin the array at a single element and
      // the sparkline would never draw)
      if (systemStats) {
        cpuHistory = [...cpuHistory.slice(-(SPARK_POINTS - 1)), Math.round(systemStats.cpu_percent || 0)];
        ramHistory = [...ramHistory.slice(-(SPARK_POINTS - 1)), Math.round(systemStats.ram_percent || 0)];
      }
    } catch (e) {
      systemStatsError = true;
      systemStatsLoaded = true;
      trafficAvailable = false;
      console.error("Dashboard fetch error:", e);
    }
  }

  /**
   * Slow tier — expensive data (journal, services, processes, disks).
   * Runs every 30 s so we are not re-running journalctl every 4 seconds.
   */
  async function fetchSlowData() {
    await Promise.all([
      fetchRecentLogs(),
      fetchServicesWatchdog(),
      fetchTopProcesses(),
    ]);
    try {
      // Each read is isolated: one failing command (e.g. df) must not discard
      // the rest of the slow tier.
      const [os, disks, ifaces, lastUpdate, failedSvc, temp] = await Promise.all([
        invoke('get_os_info').catch(() => null),
        invoke('get_disk_usage').catch(() => null),
        invoke<any[]>('get_network_interfaces').catch(() => null),
        invoke<string>('get_last_system_update').catch(() => ''),
        invoke<number>('get_failed_services_count').then(count => {
          failedServicesCountLoaded = true;
          return count;
        }).catch(() => {
          failedServicesCountLoaded = false;
          return 0;
        }),
        invoke<number | null>('get_cpu_temperature').catch(() => null)
      ]);

      if (os) osInfo = os;
      if (ifaces) networkInterfaces = ifaces;
      if (lastUpdate) lastSystemUpdate = lastUpdate;
      failedServicesCount = failedSvc;

      if (disks) {
        diskUsage = disks as any[];
        diskUsageError = false;
      } else {
        diskUsageError = true;
      }
      diskUsageLoaded = true;

      // Names of the failed units — lets the attention card link straight to
      // the journal entries of the unit that broke.
      invoke<string[]>('get_failed_services')
        .then(units => { failedServiceUnits = units || []; })
        .catch(() => { failedServiceUnits = []; });

      if (temp !== null && temp !== undefined && !isNaN(temp)) {
        cpuTemp = `${Math.round(temp)}°C`;
        cpuTempAvailable = true;
      } else {
        cpuTemp = '';
        cpuTempAvailable = false;
      }

      lastSlowCheck = new Date();

      await Promise.all([fetchNetworkDetails(), fetchSystemEvents()]);
    } catch (e) {
      console.error("Dashboard slow fetch error:", e);
    }
  }

  /** Manual refresh — both tiers. */
  async function fetchData() {
    if (refreshing) return;
    refreshing = true;
    try {
      await Promise.all([fetchFastData(), fetchSlowData()]);
    } finally {
      refreshing = false;
    }
  }

  async function fetchAppFootprint() {
    try {
      const apps = await invoke<Array<{ source: string; package_id: string | null }>>('list_desktop_apps');
      const packageIds = {
        Flatpak: [...new Set(apps.filter(app => app.source === 'Flatpak' && app.package_id).map(app => app.package_id!))],
        RPM: [...new Set(apps.filter(app => app.source === 'RPM' && app.package_id).map(app => app.package_id!))]
      };
      const summaries = await Promise.all((['Flatpak', 'RPM'] as const).map(async source => {
        const results = await Promise.allSettled(packageIds[source].map(packageId =>
          invoke<{ size_bytes: number }>('get_app_meta', { packageId, source })
        ));
        return [source, {
          count: packageIds[source].length,
          sizeBytes: results.reduce((total, result) => total + (result.status === 'fulfilled' ? result.value.size_bytes : 0), 0),
          sizesLoaded: results.filter(result => result.status === 'fulfilled').length
        }] as const;
      }));
      appFootprint = Object.fromEntries(summaries) as typeof appFootprint;
      appFootprintError = false;
    } catch (error) {
      appFootprintError = true;
      console.warn('Could not fetch application footprint:', error);
    }
  }

  function generateSparklinePath(data: number[], width = 80, height = 20): string {
    if (!data || data.length < 2) return `M 0 ${height/2} L ${width} ${height/2}`;
    const min = Math.min(...data);
    const max = Math.max(...data) || 1;
    const range = (max - min) || 1;
    const pts = data.map((val, idx) => {
      const x = (idx / (data.length - 1)) * width;
      const y = height - ((val - min) / range) * (height - 6) - 3;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
    return `M ${pts.join(' L ')}`;
  }

  function formatShortDate(raw: string): string {
    if (!raw) return '—';
    try {
      const d = new Date(raw);
      if (!isNaN(d.getTime())) {
        const datePart = d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
        const timePart = d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', hour12: false });
        return `${datePart}, ${timePart}`;
      }
    } catch {}
    const cleaned = raw.replace(/^[A-Za-z]{3}\s+/, '').replace(/(:\d{2})\s*(AM|PM).*/i, '');
    return cleaned || '—';
  }

  // ── Refresh tiers ─────────────────────────────────────────────────────────
  // Fast tier: cheap telemetry (CPU / RAM / network) — every 4 s
  // Slow tier: expensive reads (journal, services, processes, disks) — every 30 s
  let pollInterval: any = null;
  let slowInterval: any = null;
  /** When false the dashboard freezes on the last known values */
  let liveUpdates = $state(true);
  /** True while a manual refresh is in flight */
  let refreshing = $state(false);

  function stopPolling() {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
    if (slowInterval) {
      clearInterval(slowInterval);
      slowInterval = null;
    }
  }

  function startPolling() {
    if (pollInterval || slowInterval || uiStore.isThrottled || !liveUpdates) return;
    pollInterval = setInterval(fetchFastData, 4000);
    slowInterval = setInterval(fetchSlowData, 30000);
  }

  function toggleLiveUpdates() {
    liveUpdates = !liveUpdates;
    if (liveUpdates) {
      // catch up immediately instead of waiting for the next tick
      fetchFastData();
      fetchSlowData();
    }
  }

  $effect(() => {
    if (uiStore.isThrottled || !liveUpdates) {
      stopPolling();
    } else {
      startPolling();
    }
  });

  // The poll must not outlive the page
  onDestroy(() => stopPolling());

  onMount(() => {
    fetchData();
    fetchAppFootprint();
    // Stagger heavy security audit after initial telemetry renders
    setTimeout(() => {
      fetchSecurityReport();
    }, 250);
    startPolling();
  });

    // ── REDESIGNED DASHBOARD DERIVED STATES ──
  let detailsCollapsed = $state(true);
  let detailsSectionRef = $state<HTMLDivElement | null>(null);
  let storageSpotlight = $state(false);

  /**
   * The Disk tile measures space usage, so it drills into the storage
   * breakdown on this page — all mounts with their capacity bars — instead of
   * leaving the user on a hardware inventory where nothing is expanded.
   */
  function openStorageOverview() {
    detailsCollapsed = false;
    storageSpotlight = true;
    // The details card may still be hidden by the attention modal that is
    // closing at the same time, so scroll once that has settled.
    setTimeout(() => {
      detailsSectionRef?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      // Move focus with the view, otherwise keyboard and screen-reader users
      // stay on the tile with no idea what just opened below it.
      detailsSectionRef
        ?.querySelector<HTMLElement>('.card-title-header')
        ?.focus({ preventScroll: true });
    }, 160);
    setTimeout(() => (storageSpotlight = false), 1900);
  }

  // Header subtitle uses values from the live system response.
  let headerSubtitle = $derived.by(() => {
    // Which machine am I on? — the single most useful fact at a glance
    let host = '';
    if (osInfo?.hostname) {
      host = osInfo.hostname.trim();
    }

    let distro = osInfo ? 'System' : 'System details';
    if (osInfo) {
      const cleanName = (osInfo.name || 'Fedora').replace(/Linux/i, '').replace(/\([^)]*\)/g, '').trim();
      const ver = (osInfo.os_version || '').trim();
      if (ver && !cleanName.includes(ver)) {
        distro = `${cleanName} ${ver}`.trim();
      } else {
        distro = cleanName || 'Fedora';
      }
    }

    let kernel = '';
    if (osInfo?.kernel_version) {
      const kMatch = osInfo.kernel_version.match(/^(\d+\.\d+(\.\d+)?)/);
      kernel = `Kernel ${kMatch ? kMatch[1] : osInfo.kernel_version.split('-')[0]}`;
    }

    let uptime = '';
    if (systemStats?.uptime_seconds !== undefined && systemStats?.uptime_seconds !== null) {
      const sec = systemStats.uptime_seconds;
      if (sec >= 3600) {
        uptime = `up ${(sec / 3600).toFixed(1)} h`;
      } else {
        uptime = `up ${Math.floor(sec / 60)} m`;
      }
    }

    if (!osInfo && !systemStats) return systemStatsError ? 'System details unavailable' : 'System details loading…';
    return [host, distro, kernel, uptime].filter(Boolean).join(' · ');
  });

  /** Full system facts live in the tooltip so the header stays on one line. */
  let headerSubtitleTooltip = $derived.by(() => {
    const parts = [headerSubtitle];
    if (lastSystemUpdate) parts.push(`Last package sync: ${formatShortDate(lastSystemUpdate)}`);
    if (osInfo?.kernel_version) parts.push(osInfo.kernel_version);
    return parts.join('\n');
  });

  // Root disk partition (matches DiskMount: { mount, device, fs_type, total_gb, used_gb, free_gb, percent })
  let rootDisk = $derived.by(() => {
    if (!diskUsage || diskUsage.length === 0) return null;
    return diskUsage.find((d: any) => d.mount === '/') || null;
  });

  // 2. Computed Needs Attention List:
  // - security score < 80 (< 50 critical, 50-79 warning)
  // - critical log errors > 0 (critical)
  // - CPU temp > 80°C (> 85 critical, 81-85 warning)
  // - failed services (critical)
  // - disk > 85% (> 92% critical, 86-92% warning)
  interface NeedsAttentionItem {
    id: string;
    severity: 'critical' | 'warning';
    label: string;
    value: string;
    checkedAt: string;
    /** Short imperative label for the primary button */
    actionLabel: string;
    /** One sentence telling the user what to do about it */
    guidance: string;
    /** Where the problem is actually fixed (remediation) */
    action: () => void;
    /** The journal entries that prove this problem (diagnosis) */
    logFilter?: { search?: string; unit?: string; priority?: string; boot?: boolean };
  }

  /**
   * Open the Journal Logs pre-filtered for a specific problem.
   * The Journal Viewer consumes these as one-shot deep links.
   */
  function openLogsFor(filter: {
    search?: string;
    unit?: string;
    priority?: string;
    boot?: boolean;
  }) {
    uiStore.preAppliedJournalSearch = filter.search ?? '';
    uiStore.preAppliedJournalUnit = filter.unit ?? '';
    uiStore.preAppliedJournalPriority = filter.priority ?? 'all';
    uiStore.preAppliedJournalBoot = !!filter.boot;
    uiStore.navigateTo('journal-logs', 'journal');
  }

  function formatCheckedAt(date: Date | null): string {
    // Minute precision — the dashboard refreshes every few seconds, so
    // second-level timestamps would churn on every tick.
    return date ? `Checked ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : 'Check time unavailable';
  }

  /** Capacity colour ramp — matches the attention thresholds (85% / 92%). */
  function diskSeverityClass(percent: number): string {
    if (percent > 92) return 'is-critical';
    if (percent > 85) return 'is-warning';
    return '';
  }

  /** Names of the systemd units currently in a failed state (for exact log filters) */
  let failedServiceUnits = $state<string[]>([]);

  let needsAttentionList = $derived.by<NeedsAttentionItem[]>(() => {    const items: NeedsAttentionItem[] = [];

    // Security score
    if (securityReport && effectiveDashboardScore < 80) {
      const isCrit = effectiveDashboardScore < 50;
      items.push({
        id: 'security-score',
        severity: isCrit ? 'critical' : 'warning',
        label: 'Security Posture Audit',
        value: `${effectiveDashboardScore}/100 score (${isCrit ? 'Critical Risk' : 'Warnings Detected'})`,
        checkedAt: formatCheckedAt(lastSecurityCheck),
        actionLabel: 'Review findings',
        guidance: isCrit
          ? 'Open the critical findings in Security Auditor and apply their recommended fixes.'
          : 'Open the flagged findings in Security Auditor and resolve the warnings you can.',
        action: () => {
          uiStore.securitySeverityFilter = isCrit ? 'Critical' : 'Warning';
          uiStore.navigateTo('security-auditor');
        }
      });
    }

    // Critical log errors
    const critLogs = recentLogStream.filter(l => l.level === 'Err');
    const systemErrorCount = systemEventsLoaded && !systemEventsError
      ? systemEvents?.error_count
      : logsLoaded && !logsError ? critLogs.length : null;
    const logErrCount = systemErrorCount ?? 0;
    if (logErrCount > 0) {
      items.push({
        id: 'critical-logs',
        severity: 'critical',
        label: systemEventsLoaded && !systemEventsError ? 'Journal Errors Since Boot' : 'Recent Journal Errors',
        value: `${logErrCount} error${logErrCount > 1 ? 's' : ''} recorded`,
        checkedAt: formatCheckedAt(lastSlowCheck),
        actionLabel: 'Triage errors',
        guidance: 'Start with the newest errors; repeated messages usually point to one failing service or driver.',
        action: () => openLogsFor({ priority: '3', boot: true })
      });
    }

    // CPU temp
    if (cpuTempAvailable && cpuTempNumeric !== null && cpuTempNumeric > 80) {
      const isCrit = cpuTempNumeric > 85;
      items.push({
        id: 'cpu-temp',
        severity: isCrit ? 'critical' : 'warning',
        label: 'CPU Thermal Alert',
        value: `${Math.round(cpuTempNumeric)}°C (Thermal threshold exceeded)`,
        checkedAt: formatCheckedAt(lastSlowCheck),
        actionLabel: 'Inspect thermal load',
        guidance: 'Check which processes are using the CPU and review the temperature history in System Monitor.',
        // the kernel reports throttling as "Thermal throttled"
        logFilter: { search: 'thermal' },
        action: () => uiStore.navigateTo('system-monitor', 'overview')
      });
    }

    // Failed services
    const failedWatchdog = watchdogServices.filter(s => s.status === 'failed');
    const totalFailed = Math.max(
      servicesLoaded && !servicesError ? failedWatchdog.length : 0,
      failedServicesCountLoaded ? failedServicesCount : 0
    );
    if (totalFailed > 0) {
      // when exactly one unit failed we can filter the journal exactly
      const failedUnit =
        failedServiceUnits.length === 1
          ? failedServiceUnits[0]
          : failedWatchdog.length === 1
            ? failedWatchdog[0].name
            : '';
      items.push({
        id: 'failed-services',
        severity: 'critical',
        label: 'System Services Failed',
        value: failedUnit
          ? `${failedUnit} — ${totalFailed} service${totalFailed > 1 ? 's' : ''} in failed state`
          : `${totalFailed} service${totalFailed > 1 ? 's' : ''} in failed state`,
        checkedAt: formatCheckedAt(lastServicesCheck),
        actionLabel: failedUnit ? 'Open failed service' : 'Open failed services',
        guidance: failedUnit
          ? `Open ${failedUnit} in Service Manager to see why it failed, then restart it.`
          : 'Open the failed units in Service Manager, check their status, then restart the broken ones.',
        logFilter: { unit: failedUnit, priority: '3' },
        action: () => {
          uiStore.serviceFilter = 'failed';
          uiStore.serviceSearchQuery = failedUnit ? failedUnit.replace(/\.service$/, '') : '';
          uiStore.navigateTo('service-manager', 'services');
        }
      });
    }

    // Disk usage > 85%
    if (rootDisk && !systemStatsError) {
      const diskPct = rootDisk.percent ?? 0;
      if (diskPct > 85) {
        const isCrit = diskPct > 92;
        items.push({
          id: 'disk-full',
          severity: isCrit ? 'critical' : 'warning',
          label: `Root Partition Storage (${rootDisk.mount || '/'})`,
          value: `${diskPct.toFixed(0)}% utilized (${(rootDisk.free_gb ?? 0).toFixed(1)} GB free)`,
          checkedAt: formatCheckedAt(lastSlowCheck),
          actionLabel: 'Review storage',
          guidance: 'Open the storage breakdown, find the fullest mount, and remove old packages, logs, or caches from it.',
          // the kernel reports this as "No space left on device"
          logFilter: { search: 'no space' },
          action: () => openStorageOverview()
        });
      }
    }

    // Sort by severity (critical first, then warning)
    return items.sort((a, b) => {
      if (a.severity === 'critical' && b.severity !== 'critical') return -1;
      if (a.severity !== 'critical' && b.severity === 'critical') return 1;
      return 0;
    });
  });

  /**
   * Single source of truth for the attention surfaces. The banner, the card and
   * the modal all read this one snapshot, so the headline item, the count and
   * the severity can never disagree with each other.
   */
  let attentionSummary = $derived.by(() => {
    const list = needsAttentionList;
    return {
      list,
      top: list[0] ?? null,
      count: list.length,
      severity: (list.some(i => i.severity === 'critical') ? 'critical' : 'warning') as 'critical' | 'warning'
    };
  });

  // Failed units among the services this card actually lists. The system-wide
  // count belongs to Needs Attention; showing it here produced a "1 failed"
  // badge with no failed row in sight.
  let failedPinnedCount = $derived(watchdogServices.filter(s => s.status === 'failed').length);

  // Show all pinned services, with failed services first.
  let displayServices = $derived.by<ServiceDaemon[]>(() => {
    const list = [...watchdogServices];
    list.sort((a, b) => {
      if (a.status === 'failed' && b.status !== 'failed') return -1;
      if (a.status !== 'failed' && b.status === 'failed') return 1;
      return 0;
    });
    return list;
  });

  // 4. Display Events: warnings and errors ONLY, max 5 rows. No Info entries.
  let displayEvents = $derived.by(() => {
    return recentLogStream
      .filter(l => l.level === 'Err' || l.level === 'Warn')
      .slice(0, 5);
  });

  // 5. Display Top Processes: top 3 by CPU
  let displayTopProcesses = $derived.by<ProcessItem[]>(() => {
    return [...topProcesses]
      .sort((a, b) => (b.cpu_percent || 0) - (a.cpu_percent || 0))
      .slice(0, 3);
  });

  let checksPending = $derived((!securityReport && !securityError) || (!logsLoaded && !systemEventsLoaded) || !servicesLoaded || !systemStatsLoaded);
  // A missing CPU sensor is normal on VMs and must not paint the whole
  // dashboard as unavailable; the storage read only counts once it has answered.
  let checksUnavailable = $derived(
    securityError ||
      (logsError && systemEventsError) ||
      servicesError ||
      systemStatsError ||
      (diskUsageLoaded && !rootDisk)
  );

</script>

<div class="dashboard-page">
  <!-- 1. Page Header (Fixed) -->
  <div class="dashboard-header-fixed">
    <div class="dashboard-header-row">
      <div class="dashboard-header-left">
        <h1 class="dashboard-title">System Overview</h1>
        <p class="dashboard-subtitle" title={headerSubtitleTooltip}>
          {headerSubtitle}{lastDashboardCheck ? ` · updated ${lastDashboardCheck.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : ''}{systemStatsError ? ' · update failed' : ''}
        </p>
      </div>
      <div class="header-actions-group">
        <Button variant="ghost" size="sm" onclick={() => uiStore.navigateTo('app-manager')} title="Manage installed applications">
          <LayoutDashboard size={13} />
          <span>Applications</span>
        </Button>
        <Button
          variant="primary"
          size="sm"
          onclick={() => uiStore.navigateTo('dnf-history')}
          title={lastSystemUpdate ? `Open the update manager — last package sync ${formatShortDate(lastSystemUpdate)}` : 'Open the update manager'}
        >
          <Package size={13} />
          <span>{uiStore.availableUpdatesCount > 0 ? `${uiStore.availableUpdatesCount} updates` : 'Updates'}</span>
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onclick={toggleLiveUpdates}
          disabled={refreshing}
          title={liveUpdates ? 'Pause live updates' : 'Resume live updates'}
          aria-label={liveUpdates ? 'Pause live updates' : 'Resume live updates'}
        >
          {#if refreshing}
            <RefreshCw size={13} class="animate-spin-slow" />
          {:else if liveUpdates}
            <Pause size={13} />
          {:else}
            <Play size={13} />
          {/if}
        </Button>
        <button class="header-refresh-btn"
          onclick={() => fetchData()}
          disabled={refreshing}
          title="Refresh telemetry"
          aria-label="Refresh telemetry"
        >
          <RefreshCw size={13} class={refreshing ? 'animate-spin-slow' : ''} />
        </button>
      </div>
    </div>
  </div>

  <!-- ── Dashboard Main Content ── -->
  <div class="dashboard-content">
    <!-- 2. Health Banner (Slim bar, amber/red, hidden when 0) -->
    {#if attentionSummary.count > 0}
      {@const topItem = attentionSummary.top}
      <div class="health-banner {attentionSummary.severity}" role="alert">
        <div class="health-banner-left">
          <span class="health-dot {attentionSummary.severity}"></span>
          <span class="health-banner-text">
            <strong>{topItem.label}</strong>
            <span class="health-banner-value">{topItem.value}</span>
            <span class="health-banner-checked">{topItem.checkedAt}</span>
          </span>
          {#if attentionSummary.count > 1}
            <span class="health-banner-more">+{attentionSummary.count - 1} more</span>
          {/if}
        </div>
        <button
          type="button"
          class="health-banner-link"
          onclick={() => showAttentionModal = true}
          title="View all attention items"
        >
          <span>View all</span>
          <ChevronRight size={13} />
        </button>
      </div>
    {/if}

    <!-- 3. Metric Strip (ONE row of 4 compact tiles) -->
    <div class="metric-strip-grid">
      <!-- Tile 1: CPU -->
      <button
        type="button"
        class="metric-tile"
        onclick={() => {
          uiStore.processSearchQuery = '';
          uiStore.navigateTo('system-monitor', 'overview');
        }}
        title="Open System Monitor (CPU)"
      >
        <div class="metric-tile-header">
          <span class="metric-tile-label">CPU<ArrowUpRight size={12} class="metric-tile-arrow" /></span>
          <span class="metric-tile-sub">{cpuTempAvailable ? cpuTemp : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{systemStats ? `${Math.round(systemStats.cpu_percent)}%` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
          <svg viewBox="0 0 60 16" class="metric-tile-sparkline">
            {#if cpuHistory.length > 1}<path d={generateSparklinePath(cpuHistory, 60, 16)} fill="none" stroke="var(--color-accent, #00daf3)" stroke-width="1.75" stroke-linecap="round" />{/if}
          </svg>
        </div>
      </button>

      <!-- Tile 2: Memory -->
      <button
        type="button"
        class="metric-tile"
        onclick={() => {
          uiStore.processSearchQuery = '';
          uiStore.navigateTo('system-monitor', 'overview');
        }}
        title="Open System Monitor (Memory)"
      >
        <div class="metric-tile-header">
          <span class="metric-tile-label">Memory<ArrowUpRight size={12} class="metric-tile-arrow" /></span>
          <span class="metric-tile-sub">{systemStats ? `${(systemStats.ram_used_mb / 1024).toFixed(1)} / ${(systemStats.ram_total_mb / 1024).toFixed(0)} GB` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{systemStats ? `${Math.round(systemStats.ram_percent)}%` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
          <svg viewBox="0 0 60 16" class="metric-tile-sparkline">
            {#if ramHistory.length > 1}<path d={generateSparklinePath(ramHistory, 60, 16)} fill="none" stroke="var(--color-accent, #00daf3)" stroke-width="1.75" stroke-linecap="round" />{/if}
          </svg>
        </div>
      </button>

      <!-- Tile 3: Disk -->
      <button
        type="button"
        class="metric-tile"
        onclick={openStorageOverview}
        title="Show the storage breakdown for all mounted filesystems"
      >
        <div class="metric-tile-header">
          <span class="metric-tile-label">Disk<ArrowUpRight size={12} class="metric-tile-arrow" /></span>
          <span class="metric-tile-sub">{rootDisk ? `${rootDisk.used_gb.toFixed(0)} / ${rootDisk.total_gb.toFixed(0)} GB` : systemStats || systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{rootDisk ? `${Math.round(rootDisk.percent)}%` : systemStats || systemStatsError ? 'Unavailable' : 'Loading…'}</span>
          <div class="metric-tile-bar">
            <span style="width: {rootDisk ? Math.min(100, Math.round(rootDisk.percent)) : 0}%"></span>
          </div>
        </div>
      </button>

      <!-- Tile 4: Network -->
      <button
        type="button"
        class="metric-tile"
        onclick={() => uiStore.navigateTo('network-manager', 'interfaces')}
        title="Open Network Manager"
      >
        <div class="metric-tile-header">
          <span class="metric-tile-label">Network<ArrowUpRight size={12} class="metric-tile-arrow" /></span>
          <span class="metric-tile-sub">{gatewayPing || (systemStats ? (networkInterfaces.length ? 'Checking' : 'No interface') : systemStatsError ? 'Unavailable' : 'Loading…')}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{trafficAvailable ? (trafficPolls > 1 ? `↓${currentRxRate} ↑${currentTxRate}` : 'Sampling…') : systemStats ? 'Unavailable' : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
      </button>
    </div>

    <!-- 4. Two-Column Row (1:1): Needs Attention & Services -->
    <div class="two-col-grid col-1-1">
      <!-- "Needs attention" card -->
      <div id="needs-attention-card" class="card-wrap">
        <Card title="Needs Attention">
          {#snippet headerActions()}
            {#if attentionSummary.count > 0}
              <Badge variant={attentionSummary.severity === 'critical' ? 'error' : 'warning'}>
                {attentionSummary.count}
              </Badge>
            {/if}
          {/snippet}

          {#if attentionSummary.count === 0}
            <div class="all-clear-state">
              {#if checksPending}
                <RefreshCw size={18} class="all-clear-icon" />
              {:else if checksUnavailable}
                <AlertCircle size={18} class="all-clear-icon" />
              {:else}
                <CheckCircle2 size={22} class="all-clear-icon" />
              {/if}
              <div class="all-clear-text">
                <span class="all-clear-title">
                  {#if checksPending}Checking system health…
                  {:else if checksUnavailable}Some checks are unavailable
                  {:else}All systems normal{/if}
                </span>
                <span class="all-clear-sub">
                  {#if checksUnavailable}Unavailable checks are not counted as alerts.
                  {:else if attentionSummary.count === 0}No warnings or critical conditions detected{/if}
                </span>
              </div>
            </div>
          {:else}
            <div class="card-row-list">
              {#each attentionSummary.list as item (item.id)}
                <button
                  type="button"
                  class="data-row clickable-row"
                  onclick={() => item.action()}
                  title={item.guidance}
                >
                  <div class="data-row-left">
                    <span class="status-dot {item.severity}"></span>
                    <span class="data-row-label">{item.label}</span>
                  </div>
                  <span class="data-row-value mono">{item.value}<span class="attention-checked-at">{item.checkedAt}</span></span>
                </button>
              {/each}
            </div>
          {/if}
        </Card>
      </div>

      <!-- "Services" card -->
      <div class="card-wrap service-card-wrap">
        <Card title="Services">
          {#snippet headerActions()}
            {#if failedPinnedCount > 0}
              <Badge variant="error">{failedPinnedCount} failed</Badge>
            {/if}
            {#if displayServices.length > 0}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => doAllServicesAction('start')}
                disabled={!!serviceActionRunning}
                title="Start all pinned services"
              >
                <Play size={12} />
                <span>Start All</span>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onclick={confirmStopAllServices}
                disabled={!!serviceActionRunning}
                title="Stop all running pinned services"
              >
                <Square size={12} />
                <span>Stop All</span>
              </Button>
            {/if}
            <button
              type="button"
              class="icon-ghost-btn"
              onclick={() => {
                showWatchdogModal = true;
                loadAvailableUnitsForModal();
              }}
              title="Configure Watchdog Services"
            >
              <SlidersHorizontal size={13} />
            </button>
            <button
              type="button"
              class="card-header-link"
              onclick={() => {
                uiStore.serviceSearchQuery = '';
                uiStore.serviceFilter = 'all';
                uiStore.navigateTo('service-manager');
              }}
            >
              Show all
            </button>
          {/snippet}

          <div class="card-row-list services-card-list">
            {#if servicesError}
              <div class="all-clear-state compact"><span class="all-clear-title">Service status unavailable</span></div>
            {:else if !servicesLoaded}
              <div class="all-clear-state compact"><span class="all-clear-title">Loading services…</span></div>
            {:else if displayServices.length === 0}
              <div class="all-clear-state compact"><span class="all-clear-title">No services pinned</span></div>
            {:else}
            {#each displayServices as svc (svc.name)}
              <div
                class="data-row hoverable-row"
                role="button"
                tabindex="0"
                onclick={() => {
                  uiStore.serviceFilter = svc.status === 'failed' ? 'failed' : 'all';
                  uiStore.serviceSearchQuery = svc.name.replace(/\.service$/, '');
                  uiStore.navigateTo('service-manager');
                }}
                onkeydown={(e) => { if (e.target === e.currentTarget) activateOnKeyboard(e, () => { uiStore.serviceFilter = svc.status === 'failed' ? 'failed' : 'all'; uiStore.serviceSearchQuery = svc.name.replace(/\.service$/, ''); uiStore.navigateTo('service-manager'); }); }}
                oncontextmenu={(e) => handleServiceContextMenu(e, svc)}
                title="Open service in Service Manager"
              >
                <div class="data-row-left">
                  <span class="status-dot {svc.status === 'active' ? 'active' : svc.status === 'failed' ? 'critical' : 'inactive'}"></span>
                  <span class="data-row-label">{svc.label || svc.name}</span>
                  {#if svc.status === 'inactive' || svc.subState === 'dead' || svc.subState === 'stopped'}
                    <span class="status-muted-text">stopped</span>
                  {:else if svc.status === 'unknown'}
                    <span class="status-muted-text">{svc.subState}</span>
                  {/if}
                </div>
                <div class="data-row-right">
                  <button
                    type="button"
                    class="row-kebab-trigger"
                    onclick={(e) => {
                      e.stopPropagation();
                      handleServiceContextMenu(e, svc);
                    }}
                    title="Service actions"
                    aria-label="Service actions"
                  >
                    <MoreHorizontal size={14} />
                  </button>
                </div>
              </div>
            {/each}
            {/if}
          </div>
        </Card>
      </div>
    </div>

    <!-- 5. Two-Column Row (3:2): Recent Events & Top Processes -->
    <div class="two-col-grid col-3-2">
      <!-- "Recent events" card (3) -->
      <div class="card-wrap flex-3">
        <Card title="Recent Events">
          {#snippet headerActions()}
            <button
              type="button"
              class="card-header-link"
              onclick={() => {
                uiStore.preAppliedJournalPriority = '4';
                uiStore.preAppliedJournalSearch = '';
                uiStore.navigateTo('journal-logs', 'journal');
              }}
            >
              Full logs
            </button>
          {/snippet}

          {#if !logsLoaded}
            <div class="all-clear-state compact"><span class="all-clear-title">Loading recent events…</span></div>
          {:else if logsError}
            <div class="all-clear-state compact"><span class="all-clear-title">Recent events unavailable</span></div>
          {:else if displayEvents.length === 0}
            <div class="all-clear-state compact">
              <CheckCircle2 size={18} class="all-clear-icon" />
              <span class="all-clear-title">No recent warnings or errors</span>
            </div>
          {:else}
            <div class="card-row-list">
              {#each displayEvents as log}
                <div
                  class="data-row hoverable-row event-row"
                  role="button"
                  tabindex="0"
                  onclick={() => {
                    uiStore.preAppliedJournalPriority = log.level === 'Err' ? '3' : '4';
                    uiStore.preAppliedJournalSearch = log.service || '';
                    uiStore.navigateTo('journal-logs', 'journal');
                  }}
                  onkeydown={(e) => { if (e.target === e.currentTarget) activateOnKeyboard(e, () => { uiStore.preAppliedJournalPriority = log.level === 'Err' ? '3' : '4'; uiStore.preAppliedJournalSearch = log.service || ''; uiStore.navigateTo('journal-logs', 'journal'); }); }}
                  oncontextmenu={(e) => handleLogContextMenu(e, log)}
                  title="Open event in Journal"
                >
                  <div class="event-row-main">
                    <Badge variant={log.level === 'Err' ? 'error' : 'warning'} class="event-badge">
                      {log.level}
                    </Badge>
                    <span class="event-time mono">{log.time}</span>
                    <span class="event-msg mono" title={log.message}>{log.message}</span>
                  </div>
                  <div class="data-row-right">
                    <button
                      type="button"
                      class="row-kebab-trigger"
                      onclick={(e) => {
                        e.stopPropagation();
                        handleLogContextMenu(e, log);
                      }}
                      title="Log actions"
                      aria-label="Log actions"
                    >
                      <MoreHorizontal size={14} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </Card>
      </div>

      <!-- "Top processes" card (2) -->
      <div class="card-wrap flex-2">
        <Card title="Top Processes">
          {#snippet headerActions()}
            <button
              type="button"
              class="card-header-link"
              onclick={() => {
                uiStore.processSearchQuery = '';
                uiStore.navigateTo('system-monitor');
              }}
            >
              All processes
            </button>
          {/snippet}

          <div class="card-row-list">
            {#if processesError}
              <div class="all-clear-state compact"><span class="all-clear-title">Process data unavailable</span></div>
            {:else if !processesLoaded}
              <div class="all-clear-state compact"><span class="all-clear-title">Loading processes…</span></div>
            {:else if displayTopProcesses.length === 0}
              <div class="all-clear-state compact"><span class="all-clear-title">No processes to show</span></div>
            {:else}
            {#each displayTopProcesses as proc (proc.pid)}
              <div
                class="data-row hoverable-row"
                role="button"
                tabindex="0"
                onclick={() => {
                  uiStore.processSearchQuery = proc.name;
                  uiStore.navigateTo('system-monitor');
                }}
                onkeydown={(e) => { if (e.target === e.currentTarget) activateOnKeyboard(e, () => { uiStore.processSearchQuery = proc.name; uiStore.navigateTo('system-monitor'); }); }}
                oncontextmenu={(e) => handleProcessContextMenu(e, proc)}
                title="Open {proc.name} in System Monitor"
              >
                <div class="data-row-left">
                  <span class="proc-pid mono">{proc.pid}</span>
                  <span class="data-row-label">{proc.name}</span>
                </div>
                <div class="data-row-right">
                  <span class="proc-stat mono" title="CPU usage">CPU {(proc.cpu_percent ?? 0).toFixed(1)}%</span>
                  <span class="proc-stat mono text-muted" title="Memory usage">RAM {proc.mem_percent !== undefined || proc.memory_percent !== undefined ? `${((proc.mem_percent ?? proc.memory_percent) ?? 0).toFixed(1)}%` : '—'}</span>
                  <button
                    type="button"
                    class="row-kebab-trigger"
                    onclick={(e) => {
                      e.stopPropagation();
                      handleProcessContextMenu(e, proc);
                    }}
                    title="Process actions"
                    aria-label="Process actions"
                  >
                    <MoreHorizontal size={14} />
                  </button>
                </div>
              </div>
            {/each}
            {/if}
          </div>
        </Card>
      </div>
    </div>

    <!-- 6. Collapsed "Details" section (collapsed by default) -->
    <div
      class="details-card-container"
      class:spotlight={storageSpotlight}
      bind:this={detailsSectionRef}
    >
      <Card collapsible={true} bind:collapsed={detailsCollapsed} title="Storage Breakdown & Footprint" class="details-card">
      <div class="details-grid">
        <!-- Sub-panel 1: Storage Breakdown -->
        <div class="subpanel">
          <div class="subpanel-head">
            <span class="subpanel-chip"><HardDrive size={13} /></span>
            <span class="subpanel-title">Storage Breakdown</span>
            <span class="subpanel-meta">
              {diskUsage?.length ? `${diskUsage.length} mount${diskUsage.length === 1 ? '' : 's'}` : ''}
            </span>
          </div>
          <div class="card-row-list storage-list">
            {#if diskUsage && diskUsage.length > 0}
              {#each diskUsage as disk (disk.mount)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="data-row hoverable-row storage-row"
                  role="button"
                  tabindex="0"
                  onclick={() => openStoragePathModal(disk.mount, disk.device, disk.used_gb, disk.total_gb, disk.fs_type)}
                  onkeydown={(e) => { if (e.target === e.currentTarget) activateOnKeyboard(e, () => openStoragePathModal(disk.mount, disk.device, disk.used_gb, disk.total_gb, disk.fs_type)); }}
                  oncontextmenu={(e) => handleStorageContextMenu(e, { title: `${disk.mount} Storage`, mount: disk.mount, device: disk.device, fs_type: disk.fs_type, used_gb: disk.used_gb, total_gb: disk.total_gb })}
                  title="Inspect {disk.mount} storage"
                >
                  <div class="data-row-left">
                    <span class="row-icon-wrap"><HardDrive size={13} /></span>
                    <span class="data-row-label mono">{disk.mount}</span>
                    <span class="storage-device">({disk.device})</span>
                  </div>
                  <div class="data-row-right storage-row-right">
                    <span class="capacity-bar" title="{disk.percent ?? 0}% used">
                      <span class="capacity-fill {diskSeverityClass(disk.percent ?? 0)}" style="width: {Math.min(100, disk.percent ?? 0)}%"></span>
                    </span>
                    <span class="data-row-value mono">{(disk.used_gb ?? 0).toFixed(1)} / {(disk.total_gb ?? 0).toFixed(0)} GB</span>
                    <span class="pct-badge mono {diskSeverityClass(disk.percent ?? 0)}">{(disk.percent ?? 0).toFixed(0)}%</span>
                  </div>
                </div>
              {/each}
            {:else}
              <div class="panel-empty">
                <span class="panel-empty-icon">
                  {#if diskUsageError}
                    <AlertCircle size={18} />
                  {:else}
                    <RefreshCw size={18} class="animate-spin-slow" />
                  {/if}
                </span>
                <span class="panel-empty-title">
                  {diskUsageError ? 'Storage data unavailable' : 'Loading storage…'}
                </span>
                <span class="panel-empty-sub">
                  {diskUsageError
                    ? 'The mounted filesystems could not be read. Use Reload in the header to try again.'
                    : 'Reading mounted filesystems and their capacity…'}
                </span>
              </div>
            {/if}
          </div>
        </div>

        <!-- Sub-panel 2: Application & Disk Footprint -->
        <div class="subpanel">
          <div class="subpanel-head">
            <span class="subpanel-chip"><Layers size={13} /></span>
            <span class="subpanel-title">Application &amp; System Footprint</span>
          </div>
          <div class="footprint-summary-list">
            {#each ['Flatpak', 'RPM'] as source}
              {@const stats = appFootprint?.[source as 'Flatpak' | 'RPM']}
              <Button
                variant="ghost"
                class="footprint-summary-row"
                onclick={() => uiStore.navigateTo('app-manager', source)}
                oncontextmenu={(e) => handleFootprintContextMenu(e, { title: `${source} applications`, size: stats ? `${stats.count} apps · ${formatAppSize(stats.sizeBytes)}` : 'Application size summary', type: source === 'Flatpak' ? 'flatpak' : 'rpm' })}
              >
                <span class="footprint-chip">
                  {#if source === 'Flatpak'}<Boxes size={13} />{:else}<Package size={13} />{/if}
                </span>
                <span class="footprint-summary-main">
                  <span class="footprint-summary-name">{source} applications</span>
                  <span class="footprint-summary-meta">
                    {#if stats}
                      {stats.count} {stats.count === 1 ? 'app' : 'apps'}
                      {#if stats.sizesLoaded < stats.count} · size partly unavailable{/if}
                    {:else if appFootprintError}
                      Details unavailable
                    {:else}
                      Loading details…
                    {/if}
                  </span>
                </span>
                <span class="footprint-summary-size">{stats ? formatAppSize(stats.sizeBytes) : '—'}</span>
                <ChevronRight size={14} class="footprint-chevron" />
              </Button>
            {/each}
            <span class="status-muted-text footprint-hint">Installed size total for detected desktop apps.</span>
          </div>
        </div>
      </div>
    </Card>
    </div>
  </div>
</div>

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: NEEDS ATTENTION DETAILS -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showAttentionModal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div use:portal class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) showAttentionModal = false; }} role="presentation">
    <div class="modal-glass-card modal-attention-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
      <div class="modal-header-row">
        <div style="display:flex; align-items:center; gap:10px;">
          <AlertTriangle size={20} style="color: {attentionSummary.severity === 'critical' ? 'var(--color-error)' : 'var(--color-warning)'};" />
          <div>
            <h3 style="margin:0; font-size:16px; font-weight:700; color:var(--color-text-primary);">
              Needs Attention ({attentionSummary.count})
            </h3>
            <span style="font-size:11.5px; color:var(--color-text-muted);">
              System conditions that exceed normal thresholds and require administrative review
            </span>
          </div>
        </div>
        <button type="button" class="close-modal-btn" onclick={() => showAttentionModal = false}>&times;</button>
      </div>

      <div class="attention-modal-list">
        {#each attentionSummary.list as item (item.id)}
          <div class="attention-modal-item {item.severity}">
            <div class="attention-item-left">
              <span class="status-dot {item.severity}"></span>
              <div class="attention-item-info">
                <span class="attention-item-label">{item.label}</span>
                <span class="attention-item-val mono">{item.value}</span>
                <span class="attention-item-val">{item.checkedAt}</span>
                <span class="attention-item-guidance">{item.guidance}</span>
              </div>
            </div>
            <div class="attention-item-buttons">
              {#if item.logFilter}
                <Button
                  variant="ghost"
                  size="sm"
                  title="Open the journal entries that prove this problem"
                  onclick={() => {
                    showAttentionModal = false;
                    openLogsFor(item.logFilter!);
                  }}
                >
                  <FileText size={13} /> View evidence
                </Button>
              {/if}
              <Button
                variant="primary"
                size="sm"
                onclick={() => {
                  showAttentionModal = false;
                  item.action();
                }}
              >
                <span>{item.actionLabel} &rarr;</span>
              </Button>
            </div>
          </div>
        {/each}
      </div>

      <div style="margin-top:16px; display:flex; justify-content:flex-end;">
        <Button variant="default" size="sm" onclick={() => showAttentionModal = false}>Close</Button>
      </div>
    </div>
  </div>
{/if}

<!-- Storage Details Modal -->
{#if selectedStorageDetail}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div use:portal class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) selectedStorageDetail = null; }}>
    <div
      class="modal-glass-card modal-storage-card"
      role="dialog"
      aria-modal="true"
      aria-label={selectedStorageDetail.title}
      tabindex="-1"
    >
      <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:16px;">
        <h3 style="margin:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px; font-size:15px; font-weight:700;">
          <HardDrive size={18} style="color:var(--color-accent)"/>
          {selectedStorageDetail.title}
        </h3>
        <button type="button" class="close-modal-btn" onclick={() => selectedStorageDetail = null}>&times;</button>
      </div>

      <div class="storage-modal-details">
        <div class="info-row"><span>Device Node</span><strong style="color:var(--color-accent); font-family:var(--font-mono);">{selectedStorageDetail.device}</strong></div>
        {#if selectedStorageDetail.mount}<div class="info-row"><span>Storage Path / Target</span><strong style="color:var(--color-text-primary); font-family:var(--font-mono);">{selectedStorageDetail.mount}</strong></div>{/if}
        {#if selectedStorageDetail.fs_type}<div class="info-row"><span>File System</span><span style="font-family:var(--font-mono); text-transform:uppercase;">{selectedStorageDetail.fs_type}</span></div>{/if}
        <div class="info-row"><span>Total Disk Space</span><strong style="font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.total_gb)}</strong></div>
        <div class="info-row"><span>Used Space</span><strong style="color:var(--color-text-primary); font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.used_gb)} ({selectedStorageDetail.percent.toFixed(1)}%)</strong></div>
        <div class="info-row"><span>Available Free</span><strong style="color:var(--color-success); font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.free_gb)}</strong></div>
      </div>

      <div style="margin-top:20px; display:flex; justify-content:flex-end;">
        <button type="button" class="modal-action-btn" onclick={() => selectedStorageDetail = null}>Close</button>
      </div>
    </div>
  </div>
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: CUSTOMIZE WATCHDOG SERVICES (EXPANDED 2-COLUMN WORKSPACE) -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showWatchdogModal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div use:portal class="modal-backdrop" onclick={() => showWatchdogModal = false} role="presentation">
    <div class="modal-glass-card modal-watchdog-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
      <div class="modal-header-row">
        <div style="display:flex; align-items:center; gap:10px;">
          <ShieldCheck size={20} style="color:var(--color-success);" />
          <div>
            <h3 style="margin:0; font-size:16px; font-weight:700; color:var(--color-text-primary);">Customize Watchdog Services</h3>
            <span style="font-size:11.5px; color:var(--color-text-muted);">Select and configure critical system services to monitor on your dashboard</span>
          </div>
        </div>
        <button type="button" class="close-modal-btn" onclick={() => showWatchdogModal = false}>&times;</button>
      </div>

      <div class="watchdog-modal-body-2col">
        <!-- LEFT COLUMN: System Directory Search & Custom Adder (58% width) -->
        <div class="watchdog-col-left">
          <div class="watchdog-section" style="flex:1; min-height:0; display:flex; flex-direction:column;">
            <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
              <label for="watchdog-search-input" style="font-size:12.5px; font-weight:700; color:var(--color-text-primary);">
                System Services Directory
              </label>
              <span style="font-size:11px; color:var(--color-text-muted);">
                {loadingUnits ? 'Loading…' : `${availableUnits.length} available`}
              </span>
            </div>

            <div class="search-input-wrap">
              <Search size={14} class="search-icon" />
              <input
                id="watchdog-search-input"
                type="text"
                class="form-input"
                placeholder="Search installed services (e.g. docker, redis, nginx, firewalld, ufw)..."
                bind:value={watchdogSearchQuery}
              />
            </div>

            <div class="available-services-list-expanded">
              {#if loadingUnits}
                <div style="padding:32px; text-align:center; color:var(--color-text-muted); font-size:12.5px;">
                  <RefreshCw size={16} class="animate-spin-slow" style="margin-right:6px;" /> Loading installed service units…
                </div>
              {:else if filteredAvailableUnits.length === 0}
                <div style="padding:32px; text-align:center; color:var(--color-text-muted); font-size:12.5px;">
                  {watchdogSearchQuery ? `No service found matching "${watchdogSearchQuery}".` : 'No services available.'}
                </div>
              {:else}
                {#each filteredAvailableUnits as u}
                  {@const isPinned = watchdogServices.some(s => s.name === u.name)}
                  <div class="avail-unit-row-card" class:is-pinned-row={isPinned}>
                    <div class="avail-unit-meta">
                      <div style="display:flex; align-items:center; gap:8px;">
                        <span class="avail-unit-name font-mono">{u.name}</span>
                        <span class="unit-state-tag" class:active={u.active_state === 'active'} class:failed={u.active_state === 'failed'}>
                          {u.active_state}
                        </span>
                      </div>
                      {#if u.description}
                        <span class="avail-unit-desc">{u.description}</span>
                      {/if}
                    </div>
                    <button
                      type="button"
                      class="pin-toggle-btn"
                      class:pinned={isPinned}
                      onclick={() => togglePinService(u.name, u.description, u.active_state)}
                    >
                      {#if isPinned}
                        <Check size={13} />
                        <span>Pinned</span>
                      {:else}
                        <Plus size={13} />
                        <span>Pin to Watchdog</span>
                      {/if}
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        </div>

        <!-- RIGHT COLUMN: Pinned Services & Quick Presets (42% width) -->
        <div class="watchdog-col-right">
          <!-- Section 1: Active Pinned Services -->
          <div class="watchdog-section" style="flex:1; min-height:0; display:flex; flex-direction:column;">
            <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
              <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary);">
                Pinned on Dashboard ({watchdogServices.length})
              </span>
              <button
                type="button"
                onclick={resetWatchdogDefaults}
                style="font-size:11px; color:var(--color-text-muted); background:transparent; border:none; cursor:pointer; display:flex; align-items:center; gap:4px;"
                title="Reset to default 6 services"
              >
                <RotateCcw size={11} /> Reset Defaults
              </button>
            </div>

            <div class="pinned-services-scroll-panel">
              {#if watchdogServices.length === 0}
                <div style="padding:24px; text-align:center; color:var(--color-text-muted); font-size:12px; font-style:italic;">
                  No services currently pinned.<br />Pick services from the left or click a preset below.
                </div>
              {:else}
                {#each watchdogServices as svc, i (svc.name)}
                  <div
                    class="pinned-service-row"
                    class:is-dragging={draggedIndex === i}
                    class:is-dragover={dragOverIndex === i}
                    role="listitem"
                    title="Drag to reorder. Use the up and down buttons with a keyboard."
                    draggable="true"
                    ondragstart={(e) => handleDragStart(e, i)}
                    ondragover={(e) => handleDragOver(e, i)}
                    ondragleave={() => handleDragLeave(i)}
                    ondrop={(e) => handleDrop(e, i)}
                    ondragend={handleDragEnd}
                  >
                    <div class="service-reorder-buttons" aria-label="Reorder service">
                      <Button variant="ghost" size="sm" class="drag-handle" aria-label="Move {svc.label} up" title="Move up" disabled={i === 0} onclick={() => moveWatchdogService(i, -1)}><ArrowUp size={11} /></Button>
                      <Button variant="ghost" size="sm" class="drag-handle" aria-label="Move {svc.label} down" title="Move down" disabled={i === watchdogServices.length - 1} onclick={() => moveWatchdogService(i, 1)}><ArrowDown size={11} /></Button>
                    </div>
                    <div class="pinned-row-left">
                      <span class="chip-status-dot" class:active={svc.status === 'active'} class:failed={svc.status === 'failed'}></span>
                      <div class="chip-info">
                        <span class="chip-label">{svc.label}</span>
                        <span class="chip-unit font-mono">{svc.name}</span>
                      </div>
                    </div>
                    <button
                      type="button"
                      class="chip-remove-btn"
                      onclick={() => togglePinService(svc.name)}
                      title="Unpin {svc.name}"
                    >
                      <X size={13} />
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </div>

          <!-- Section 2: Quick Presets -->
          <div class="watchdog-section" style="border-top:1px solid var(--color-border); padding-top:12px; margin-top:10px;">
            <span style="font-size:11.5px; font-weight:600; color:var(--color-text-secondary); margin-bottom:6px; display:block;">
              Quick Preset Bundles
            </span>
            <div class="preset-buttons-grid">
              <button
                type="button"
                class="preset-add-card"
                onclick={() => addPresetBundle([
                  { name: 'systemd-journald.service', label: 'Journal Logging' },
                  { name: 'firewalld.service', label: 'Firewall Daemon' },
                  { name: 'sshd.service', label: 'OpenSSH Server' },
                  { name: 'NetworkManager.service', label: 'Network Manager' },
                  { name: 'crond.service', label: 'Cron Scheduler' }
                ])}
              >
                <Plus size={12} />
                <span>Core Daemons</span>
              </button>

              <button
                type="button"
                class="preset-add-card"
                onclick={() => addPresetBundle([
                  { name: 'nginx.service', label: 'NGINX Web Server' },
                  { name: 'caddy.service', label: 'Caddy Server' },
                  { name: 'apache2.service', label: 'Apache HTTP' },
                  { name: 'httpd.service', label: 'HTTPD Daemon' }
                ])}
              >
                <Plus size={12} />
                <span>Web &amp; Proxy</span>
              </button>

              <button
                type="button"
                class="preset-add-card"
                onclick={() => addPresetBundle([
                  { name: 'postgresql.service', label: 'PostgreSQL Database' },
                  { name: 'mariadb.service', label: 'MariaDB Server' },
                  { name: 'mysqld.service', label: 'MySQL Database' },
                  { name: 'redis.service', label: 'Redis In-Memory Cache' }
                ])}
              >
                <Plus size={12} />
                <span>Databases</span>
              </button>

              <button
                type="button"
                class="preset-add-card"
                onclick={() => addPresetBundle([
                  { name: 'docker.service', label: 'Docker Engine' },
                  { name: 'podman.service', label: 'Podman Container Manager' },
                  { name: 'containerd.service', label: 'containerd Runtime' }
                ])}
              >
                <Plus size={12} />
                <span>Containers</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <div style="margin-top:14px; display:flex; justify-content:flex-end; border-top:1px solid var(--color-border); padding-top:12px;">
        <Button variant="primary" onclick={() => showWatchdogModal = false}>
          Done
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ══ Context menus (shared: positioning, focus, keyboard, dismissal) ══ -->
<ContextMenu
  bind:isOpen={serviceContextMenu.open}
  x={serviceContextMenu.x}
  y={serviceContextMenu.y}
  icon={Server}
  title={serviceContextMenu.svc?.name ?? ''}
  badges={serviceMenuBadges}
  items={serviceMenuItems}
  minWidth="250px"
/>

<ContextMenu
  bind:isOpen={logContextMenu.open}
  x={logContextMenu.x}
  y={logContextMenu.y}
  icon={FileText}
  title={logContextMenu.log ? `[${logContextMenu.log.time}] [${logContextMenu.log.service}]` : ''}
  badge={logMenuBadge}
  items={logMenuItems}
  minWidth="280px"
/>

<ContextMenu
  bind:isOpen={processContextMenu.open}
  x={processContextMenu.x}
  y={processContextMenu.y}
  icon={Activity}
  title={processContextMenu.proc?.name ?? ''}
  subtitle={processMenuSubtitle}
  badges={processMenuBadges}
  items={processMenuItems}
  minWidth="270px"
/>

<ContextMenu
  bind:isOpen={storageContextMenu.open}
  x={storageContextMenu.x}
  y={storageContextMenu.y}
  icon={HardDrive}
  title={storageContextMenu.item?.title ?? ''}
  subtitle={storageMenuSubtitle}
  badge={storageMenuBadge}
  items={storageMenuItems}
  minWidth="270px"
/>

<ContextMenu
  bind:isOpen={footprintContextMenu.open}
  x={footprintContextMenu.x}
  y={footprintContextMenu.y}
  icon={Layers}
  title={footprintContextMenu.item?.title ?? ''}
  badge={footprintMenuBadge}
  items={footprintMenuItems}
  minWidth="280px"
/>

<!-- Escape closes the topmost modal; the context menus handle their own. -->
<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape') return;
    if (selectedStorageDetail) selectedStorageDetail = null;
    else if (showAttentionModal) showAttentionModal = false;
    else if (showWatchdogModal) showWatchdogModal = false;
  }}
/>

<style>

  .dashboard-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    width: 100%;
    max-width: 100%;
    overflow: hidden;
    box-sizing: border-box;
    background: var(--color-bg-base);
  }

  .dashboard-header-fixed {
    flex-shrink: 0;
    padding: 14px 24px 12px 24px;
    background: var(--color-bg-base);
    border-bottom: 1px solid var(--color-border);
    z-index: 20;
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
  }

  .dashboard-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    width: 100%;
  }

  .dashboard-header-left {
    display: flex;
    flex-direction: column;
    gap: 3px;
    /* let the subtitle truncate instead of pushing the action buttons off-screen */
    flex: 1 1 auto;
    min-width: 0;
  }

  .dashboard-title {
    font-size: 19px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin: 0;
    line-height: 1.2;
    letter-spacing: -0.01em;
  }

  .dashboard-subtitle {
    font-size: 12.5px;
    color: var(--color-text-muted);
    margin: 0;
    line-height: 1.3;
    /* keep the header on one line — the full facts live in the tooltip */
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-actions-group {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .dashboard-content {
    flex: 1;
    min-height: 0;
    width: 100%;
    max-width: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 16px 24px 24px 24px;
    display: flex;
    flex-direction: column;
    gap: 11px;
    box-sizing: border-box;
  }

  /* ── 2. Health Banner ── */
  .health-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13px;
    border: 1px solid transparent;
    width: 100%;
    box-sizing: border-box;
  }

  .health-banner.warning {
    background: var(--color-warning-muted);
    border-color: rgba(245, 158, 11, 0.25);
    color: var(--color-warning);
  }

  .health-banner.critical {
    background: var(--color-error-muted);
    border-color: rgba(239, 68, 68, 0.25);
    color: var(--color-error);
  }

  .health-banner-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .health-banner-text {
    font-size: 13px;
    color: var(--color-text-primary);
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .health-banner-text strong {
    font-family: var(--font-mono);
    font-weight: 700;
    white-space: nowrap;
  }

  .health-banner-value {
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .health-banner-more {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .health-banner-checked {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .health-banner-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    font-size: 12px;
    font-weight: 600;
    color: inherit;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    transition: opacity 0.15s ease;
  }

  .health-banner-link:hover {
    text-decoration: underline;
    opacity: 0.85;
  }

  /* ── 3. Metric Strip (ONE row of 4 compact tiles) ── */
  .metric-strip-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 17px;
    width: 100%;
    box-sizing: border-box;
  }

  .metric-tile {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 16px;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.15s ease, background 0.15s ease;
    min-height: 74px;
    min-width: 0;
    box-sizing: border-box;
    overflow: hidden;
    position: relative;
  }

  /* "clickable" affordance — the tiles navigate somewhere */
  .metric-tile-arrow {
    color: var(--color-text-muted);
    opacity: 0;
    vertical-align: middle;
    transition: opacity 0.15s ease, color 0.15s ease;
  }

  .metric-tile:hover .metric-tile-arrow {
    opacity: 1;
    color: var(--color-accent);
  }

  .metric-tile:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border-hover);
  }

  .metric-tile-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 4px;
    min-width: 0;
  }

  .metric-tile-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    /* keep the hover arrow on the same line as the label */
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .metric-tile-sub {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .metric-tile-body {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }

  /* Capacity bar for metrics that have no history to plot */
  .metric-tile-bar {
    width: 60px;
    height: 4px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    overflow: hidden;
    flex-shrink: 0;
    align-self: center;
  }

  .metric-tile-bar > span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-accent);
    transition: width 0.3s ease;
  }

  .metric-tile-num {
    font-family: var(--font-mono);
    font-size: 19px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.2;
    white-space: nowrap;
  }

  .metric-tile-sparkline {
    width: 60px;
    height: 16px;
    flex-shrink: 0;
  }

  /* ── Two-Column Grids (17px gutter, with strict minmax for responsiveness) ── */
  .two-col-grid {
    display: grid;
    gap: 17px;
    align-items: stretch;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }

  .two-col-grid.col-1-1 {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .two-col-grid.col-3-2 {
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
  }

  .card-wrap {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
  }

  .card-wrap :global(.card) {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    max-width: 100%;
    padding: 12px 16px 8px 16px;
    box-sizing: border-box;
    overflow: hidden;
  }

  .service-card-wrap {
    align-self: start;
    width: 100%;
  }

  .service-card-wrap :global(.card) {
    height: auto;
  }

  .card-wrap :global(.card .card-header-row) {
    margin-bottom: 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
    min-width: 0;
  }

  .card-wrap :global(.card-title-header) {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .icon-ghost-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 3px;
    border-radius: 4px;
    transition: color 0.15s ease, background 0.15s ease;
    flex-shrink: 0;
  }

  .icon-ghost-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-hover);
  }

  .card-header-link {
    background: none;
    border: none;
    font-size: 12px;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: color 0.15s ease;
    flex-shrink: 0;
    white-space: nowrap;
  }

  .card-header-link:hover {
    color: var(--color-text-primary);
    text-decoration: underline;
  }

  /* ── Card Row List & Dividers ── */
  .card-row-list {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    width: 100%;
    overflow: hidden;
  }

  .services-card-list {
    flex: 0 1 auto;
    min-height: 0;
    max-height: 182px;
    overflow-y: auto;
    scrollbar-width: thin;
    overscroll-behavior: contain;
  }

  .services-card-list .data-row {
    flex: 0 0 auto;
    min-height: 26px;
    padding: 4px 8px;
  }

  .data-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 8px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: transparent;
    border-top: none;
    border-left: none;
    border-right: none;
    text-align: left;
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    font-size: 12.5px;
    color: var(--color-text-primary);
    border-radius: 4px;
    transition: background 0.12s ease;
    overflow: hidden;
  }

  .data-row:last-child {
    border-bottom: none;
  }

  .data-row.hoverable-row,
  .data-row.clickable-row {
    cursor: pointer;
  }

  .data-row.hoverable-row:hover,
  .data-row.clickable-row:hover {
    background: var(--color-bg-hover);
  }

  .data-row-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
  }

  .data-row-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }

  .data-row-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .data-row-value {
    font-size: 12px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Status Dots (Semantic green/amber/red only) */
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-dot.active {
    background: var(--color-success);
  }

  .status-dot.critical {
    background: var(--color-error);
  }

  .status-dot.warning {
    background: var(--color-warning);
  }

  .status-dot.inactive {
    background: #6b7280;
  }

  .health-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .health-dot.critical {
    background: var(--color-error);
  }

  .health-dot.warning {
    background: var(--color-warning);
  }

  .status-muted-text {
    font-size: 11px;
    color: var(--color-text-muted);
    font-style: italic;
  }

  .row-kebab-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s ease, background 0.12s ease, color 0.12s ease;
    flex-shrink: 0;
  }

  .data-row:hover .row-kebab-trigger,
  .row-kebab-trigger:focus {
    opacity: 1;
  }

  @media (hover: none) {
    .row-kebab-trigger { opacity: 1; }
  }

  .row-kebab-trigger:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-primary);
  }

  /* All Clear Empty State */
  .all-clear-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px 16px;
    flex: 1;
  }

  .all-clear-state.compact {
    padding: 16px;
    gap: 8px;
  }

  .all-clear-icon {
    color: var(--color-success);
    flex-shrink: 0;
  }

  .all-clear-text {
    display: flex;
    flex-direction: column;
  }

  .all-clear-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .all-clear-sub {
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  /* Recent Events Row (Guaranteed Responsiveness & Truncation) */
  .event-row {
    padding: 6px 8px;
    min-width: 0;
    width: 100%;
    box-sizing: border-box;
    overflow: hidden;
  }

  .event-row-main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 0%;
    overflow: hidden;
  }

  .event-badge {
    flex-shrink: 0;
  }

  .event-time {
    font-size: 11.5px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .event-msg {
    font-size: 12px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex: 1 1 0%;
  }

  /* Top Processes Specific */
  .proc-pid {
    font-size: 11.5px;
    color: var(--color-text-muted);
    width: 44px;
    flex-shrink: 0;
  }

  .proc-stat {
    font-size: 12px;
    color: var(--color-text-primary);
    min-width: 44px;
    text-align: right;
  }

  /* ── 6. Details Section (Whole Row Clickable) ── */
  .details-card-container {
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
    scroll-margin-top: 12px;
    border-radius: var(--radius, 10px);
  }

  /* Brief outline after the Disk tile jumps here, so the eye lands on it. */
  .details-card-container.spotlight {
    animation: storage-spotlight 1.8s ease-out;
  }

  @keyframes storage-spotlight {
    0% {
      box-shadow: 0 0 0 2px var(--color-accent);
    }
    100% {
      box-shadow: 0 0 0 2px transparent;
    }
  }

  .details-card-container :global(.card-header-row) {
    user-select: none;
    border-radius: 6px;
    padding: 4px 6px;
    margin: -4px -6px 8px -6px;
    transition: background 0.15s ease;
  }

  .details-card-container :global(.card-title-header:hover) {
    color: var(--color-accent);
  }

  .details-card-container :global(.card-collapse-btn) {
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease, transform 0.15s ease;
  }

  .details-card-container :global(.card-collapse-btn:hover) {
    transform: scale(1.06);
  }

  :global(.details-card) {
    background: var(--color-bg-card) !important;
  }

  .details-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 17px;
    padding: 8px 0 4px 0;
  }

  .attention-checked-at {
    display: block;
    margin-top: 2px;
    color: var(--color-text-muted);
    font-size: 10px;
    font-family: var(--font-sans);
  }

  @media (max-width: 1050px) {
    .dashboard-content { padding: 14px 16px 20px; }
    .metric-strip-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
    .two-col-grid.col-1-1,
    .two-col-grid.col-3-2 { grid-template-columns: minmax(0, 1fr); gap: 10px; }
    .details-grid { grid-template-columns: minmax(0, 1fr); gap: 8px; }
  }

  @media (max-width: 620px) {
    .dashboard-header-fixed { padding: 12px 14px; }
    .dashboard-header-row { align-items: flex-start; flex-direction: column; gap: 10px; }
    .header-actions-group { width: 100%; flex-wrap: wrap; gap: 6px; }
    .dashboard-content { padding: 12px; gap: 8px; }
    .metric-strip-grid { gap: 8px; }
    .metric-tile { padding: 10px; }
    .metric-tile-num { font-size: 16px; }
    .dashboard-subtitle { overflow-wrap: anywhere; }
    .health-banner { align-items: flex-start; gap: 8px; padding: 8px 10px; }
    .health-banner-link { flex-shrink: 0; }
  }

  /* Sub-panel header — a raised band so it reads as a header, not as content */
  .subpanel {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--color-bg-card);
  }

  .subpanel-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 10px 14px;
    background: var(--color-bg-raised);
    border-bottom: 1px solid var(--color-border);
  }

  .subpanel-chip {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: var(--color-module-icon-bg);
    border: 1px solid var(--color-module-icon-border);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .subpanel-title {
    font-size: 11.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--color-text-primary);
  }

  .subpanel-meta {
    margin-left: auto;
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  /* ── Sub-panel empty state ───────────────────────────────────── */
  .panel-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 30px 20px;
    text-align: center;
  }

  .panel-empty-icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    margin-bottom: 2px;
  }

  .panel-empty-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .panel-empty-sub {
    font-size: 11.5px;
    color: var(--color-text-muted);
    max-width: 330px;
    line-height: 1.5;
  }

  .footprint-summary-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
  }

  /* Ghost button + row styling. :global() is required because the <Button>
     element carries the Button component's scope hash, not this component's.
     The default button size is used on purpose — the global .btn-sm rules
     force a 28px height with !important, which a row cannot live with. */
  .footprint-summary-list :global(.footprint-summary-row) {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    width: 100%;
    min-height: 52px;
    padding: 9px 10px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    text-align: left;
    font-size: 12px;
    transition: background 0.15s ease, border-color 0.15s ease;
  }

  .footprint-summary-list :global(.footprint-summary-row:hover) {
    background: var(--color-accent-muted);
    border-color: var(--color-accent-glow);
  }

  .footprint-chip {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: color 0.15s ease, border-color 0.15s ease;
  }

  .footprint-summary-list :global(.footprint-summary-row:hover .footprint-chip) {
    color: var(--color-accent);
    border-color: var(--color-accent-glow);
  }

  .footprint-summary-main {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  .footprint-summary-name {
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 600;
  }

  .footprint-summary-meta {
    color: var(--color-text-muted);
    font-size: 10.5px;
  }

  .footprint-summary-size {
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
  }

  .footprint-chevron {
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: transform 0.15s ease, color 0.15s ease;
  }

  .footprint-summary-list :global(.footprint-summary-row:hover .footprint-chevron) {
    color: var(--color-accent);
    transform: translateX(2px);
  }

  .footprint-hint {
    padding: 10px 10px 4px;
  }

  /* ── Storage rows ─────────────────────────────────────────────────── */
  .storage-list {
    padding: 4px 6px;
  }

  .storage-row {
    gap: 10px;
    border-radius: 8px;
    border-bottom: none;
    padding: 9px 8px;
  }

  .storage-row:hover {
    background: var(--color-accent-muted);
    box-shadow: inset 2px 0 0 var(--color-accent);
  }

  .row-icon-wrap {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: color 0.15s ease;
  }

  .storage-row:hover .row-icon-wrap {
    color: var(--color-accent);
  }

  .storage-device {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .storage-row-right {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .capacity-bar {
    display: block;
    width: 90px;
    height: 5px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    overflow: hidden;
    flex-shrink: 0;
  }

  .capacity-fill {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-accent);
    transition: width 0.3s ease, background 0.2s ease;
  }

  .capacity-fill.is-warning {
    background: var(--color-warning);
  }

  .capacity-fill.is-critical {
    background: var(--color-error);
  }

  .pct-badge {
    font-size: 11px;
    color: var(--color-text-secondary);
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    white-space: nowrap;
  }

  .pct-badge.is-warning {
    color: var(--color-warning);
    background: var(--color-warning-muted);
    border-color: rgba(245, 158, 11, 0.3);
  }

  .pct-badge.is-critical {
    color: var(--color-error);
    background: var(--color-error-muted);
    border-color: rgba(239, 68, 68, 0.3);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .text-muted {
    color: var(--color-text-muted);
  }

  /* ── Needs Attention Modal Styles ── */
  .modal-glass-card.modal-attention-card {
    width: 900px !important;
    max-width: calc(100vw - 40px) !important;
    max-height: 88vh !important;
    display: flex !important;
    flex-direction: column !important;
    box-sizing: border-box !important;
    padding: 22px !important;
  }

  .attention-modal-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 14px;
    overflow-y: auto;
    max-height: 60vh;
    padding-right: 4px;
  }

  .attention-item-buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .attention-modal-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    padding: 10px 14px;
    border-radius: 8px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-subtle);
    gap: 10px 12px;
  }

  .attention-modal-item.critical {
    border-left: 3.5px solid var(--color-error);
  }

  .attention-modal-item.warning {
    border-left: 3.5px solid var(--color-warning);
  }

  .attention-item-left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }

  .attention-item-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .attention-item-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .attention-item-val {
    font-size: 11.5px;
    color: var(--color-text-secondary);
  }

  .attention-item-guidance {
    margin-top: 3px;
    font-size: 11.5px;
    line-height: 1.35;
    color: var(--color-text-secondary);
    max-width: 52ch;
  }

  .modal-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--color-border);
  }

  .modal-glass-card.modal-watchdog-card,
  .modal-watchdog-card {
    width: 1060px !important;
    max-width: calc(100vw - 40px) !important;
    height: 680px !important;
    max-height: 90vh !important;
    display: flex !important;
    flex-direction: column !important;
    box-sizing: border-box !important;
    padding: 22px !important;
  }

  .watchdog-modal-body-2col {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1.35fr 1fr;
    gap: 22px;
    overflow: hidden;
    padding-top: 6px;
  }

  .watchdog-col-left,
  .watchdog-col-right {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    overflow: hidden;
  }

  .watchdog-section {
    display: flex;
    flex-direction: column;
  }

  .available-services-list-expanded {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 5px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
    padding: 6px;
    scrollbar-width: thin;
  }

  .avail-unit-row-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    border-radius: 7px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border-subtle, var(--color-border));
    transition: all 0.12s ease;
  }
  .avail-unit-row-card:hover {
    border-color: var(--color-accent);
    background: rgba(255, 255, 255, 0.03);
  }
  .avail-unit-row-card.is-pinned-row {
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.04);
  }

  .unit-state-tag {
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-muted);
    text-transform: uppercase;
  }
  .unit-state-tag.active {
    background: var(--color-success-muted);
    color: var(--color-success);
  }
  .unit-state-tag.failed {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .pinned-services-scroll-panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 5px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
    padding: 6px;
    scrollbar-width: thin;
  }

  .pinned-service-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 10px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 7px;
    cursor: grab;
    transition: transform 0.15s ease, background 0.15s ease, border-color 0.15s ease, opacity 0.15s ease;
    user-select: none;
  }
  .pinned-service-row:hover {
    border-color: var(--color-accent);
    background: rgba(255, 255, 255, 0.03);
  }
  .pinned-service-row:active {
    cursor: grabbing;
  }
  .pinned-service-row.is-dragging {
    opacity: 0.35;
    border-style: dashed;
    border-color: var(--color-accent);
  }
  .pinned-service-row.is-dragover {
    border-color: var(--color-accent);
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  }

  .drag-handle {
    color: var(--color-text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    background: transparent;
    border: 0;
    border-radius: 4px;
    cursor: pointer;
    opacity: 1;
    transition: opacity 0.15s ease, color 0.15s ease;
    flex-shrink: 0;
  }
  .service-reorder-buttons { display: flex; flex-direction: column; gap: 1px; }
  .drag-handle:disabled { opacity: 0.35; cursor: not-allowed; }
  .pinned-service-row:hover .drag-handle {
    opacity: 1;
    color: var(--color-accent);
  }

  .pinned-row-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .chip-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    flex-shrink: 0;
  }
  .chip-status-dot.active { background: var(--color-success); box-shadow: 0 0 6px rgba(34, 197, 94, 0.5); }
  .chip-status-dot.failed { background: var(--color-error); box-shadow: 0 0 6px rgba(239, 68, 68, 0.5); }

  .chip-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }

  .chip-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip-unit {
    font-size: 9.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip-remove-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 3px;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: all 0.15s ease;
  }
  .chip-remove-btn:hover {
    color: #ffffff;
    background: var(--color-error, #ef4444);
  }

  .preset-buttons-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .preset-add-card {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    border-radius: 7px;
    font-size: 11px;
    font-weight: 600;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle, var(--color-border));
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .preset-add-card:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.1));
  }

  .search-input-wrap {
    position: relative;
    margin-bottom: 6px;
  }
  :global(.search-input-wrap .search-icon) {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-text-muted);
    pointer-events: none;
  }
  .search-input-wrap .form-input {
    padding-left: 32px;
    width: 100%;
    box-sizing: border-box;
  }

  .form-input {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 12px;
    color: var(--color-text-primary);
    font-family: inherit;
    outline: none;
  }
  .form-input:focus {
    border-color: var(--color-accent);
  }

  .avail-unit-meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .avail-unit-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .avail-unit-desc {
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pin-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.12s ease;
    flex-shrink: 0;
  }
  .pin-toggle-btn:hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }
  .pin-toggle-btn.pinned {
    background: var(--color-success-muted);
    border-color: var(--color-success);
    color: var(--color-success);
  }

  /* ── Modal Backdrop ── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    z-index: 99990;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .modal-glass-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    padding: 20px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.45);
  }

  .modal-storage-card {
    width: 480px;
    max-width: 100%;
  }

  .close-modal-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 18px;
    cursor: pointer;
    padding: 0 4px;
  }

  .storage-modal-details {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 0;
    border-bottom: 1px solid var(--color-border);
  }

</style>
