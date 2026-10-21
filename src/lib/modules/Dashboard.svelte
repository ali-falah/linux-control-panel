<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    LayoutDashboard, HardDrive, Wifi, Server, Activity, RefreshCw, Shield, 
    Cpu, Clock, Calendar, Laptop, Cable, Network, Lock, Disc, Sparkles, 
    AlertTriangle, ShieldAlert, Thermometer, ExternalLink, ChevronRight,
    Terminal, Info, CheckCircle2, AlertCircle, ArrowUpRight, ArrowDown, ArrowUp,
    Zap, Layers, Package, Sliders, Play, Square, RotateCcw, ShieldCheck, Database,
    FileText, HardDriveDownload, SlidersHorizontal, Plus, Trash2, Check, Search, X,
    Copy, Globe, Folder, FolderOpen, Flame, MoreHorizontal
  } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
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
        if (Array.isArray(parsed) && parsed.length > 0) return parsed.map(svc => ({ ...svc, status: 'unknown', subState: 'checking' }));
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
  let lastLogsCheck = $state<Date | null>(null);
  let lastServicesCheck = $state<Date | null>(null);
  let lastSecurityCheck = $state<Date | null>(null);

  // Service Action Execution State
  let serviceActionRunning = $state<string | null>(null);

  async function doServiceAction(serviceName: string, action: 'start' | 'stop' | 'restart') {
    serviceActionRunning = `${serviceName}-${action}`;
    try {
      await invoke('unit_action', { name: serviceName, action, userMode: false });
      uiStore.addToast(`Service ${serviceName} ${action}ed successfully`, 'success');
      await fetchServicesWatchdog();
    } catch (e) {
      uiStore.addToast(`Failed to ${action} ${serviceName}: ${e}`, 'error');
    } finally {
      serviceActionRunning = null;
    }
  }

  function confirmStopService(svc: ServiceDaemon) {
    closeAllContextMenus();
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
    closeAllContextMenus();
    uiStore.confirm(
      `Restart Service "${svc.label || svc.name}"?`,
      `Are you sure you want to restart "${svc.name}"? Connections or sessions handled by this service may reset.`,
      () => doServiceAction(svc.name, 'restart'),
      false
    );
  }

  function confirmRestartFromLog(serviceName: string) {
    closeAllContextMenus();
    const unit = serviceName.endsWith('.service') ? serviceName : `${serviceName}.service`;
    uiStore.confirm(
      `Restart Service "${unit}"?`,
      `Are you sure you want to restart "${unit}"?`,
      () => doServiceAction(unit, 'restart'),
      false
    );
  }

  function confirmUnpinService(serviceName: string) {
    closeAllContextMenus();
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

  function copyToClipboard(text: string, label = 'Copied to clipboard') {
    navigator.clipboard.writeText(text);
    uiStore.addToast(label, 'info');
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

  // 1. Service Watchdog Context Menu
  let serviceContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    svc: ServiceDaemon | null;
  }>({ x: 0, y: 0, show: false, svc: null });

  function handleServiceContextMenu(e: MouseEvent, svc: ServiceDaemon) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 250;
    const menuHeight = 320;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    serviceContextMenu = {
      x,
      y,
      show: true,
      svc
    };
  }

  // 2. System Events Metrics Context Menu
  let metricContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    type: 'errors' | 'warnings' | 'health';
    count: number;
  }>({ x: 0, y: 0, show: false, type: 'errors', count: 0 });

  function handleMetricContextMenu(e: MouseEvent, type: 'errors' | 'warnings' | 'health', count = 0) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 270;
    const menuHeight = 250;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    metricContextMenu = {
      x,
      y,
      show: true,
      type,
      count
    };
  }

  // 3. Log Ticker Line Context Menu
  let logContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    log: { time: string; service: string; level: string; message: string } | null;
  }>({ x: 0, y: 0, show: false, log: null });

  function handleLogContextMenu(e: MouseEvent, log: { time: string; service: string; level: string; message: string }) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 290;
    const menuHeight = 350;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    logContextMenu = {
      x,
      y,
      show: true,
      log
    };
  }

  // 4. Top Active Processes Context Menu
  let processContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    proc: ProcessItem | null;
  }>({ x: 0, y: 0, show: false, proc: null });

  function handleProcessContextMenu(e: MouseEvent, proc: ProcessItem) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 270;
    const menuHeight = 360;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    processContextMenu = {
      x,
      y,
      show: true,
      proc
    };
  }

  function confirmKillProcess(proc: ProcessItem, signal: number = 15) {
    closeAllContextMenus();
    if (proc.pid <= 100) {
      uiStore.addToast(`Cannot terminate system PID ${proc.pid} (protected process).`, 'error');
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
          await fetchSystemStats();
        } catch (e) {
          uiStore.addToast(`Failed to terminate process: ${e}`, 'error');
        }
      },
      isForce
    );
  }

  // 5. Storage & Disks Context Menu
  interface StorageContextItem {
    title: string;
    mount?: string;
    device: string;
    fs_type?: string;
    used_gb: number;
    total_gb: number;
    is_pool?: boolean;
    is_drive?: boolean;
  }

  let storageContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    item: StorageContextItem | null;
  }>({ x: 0, y: 0, show: false, item: null });

  function handleStorageContextMenu(e: MouseEvent, item: StorageContextItem) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 270;
    const menuHeight = 310;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    storageContextMenu = {
      x,
      y,
      show: true,
      item
    };
  }

  // 6. App & Disk Footprint Context Menu
  interface FootprintContextItem {
    title: string;
    path?: string;
    size: string;
    type: 'path' | 'flatpak' | 'rpm' | 'logs' | 'cache';
    used_gb?: number;
    total_gb?: number;
    fs_type?: string;
    device?: string;
  }

  let footprintContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    item: FootprintContextItem | null;
  }>({ x: 0, y: 0, show: false, item: null });

  function handleFootprintContextMenu(e: MouseEvent, item: FootprintContextItem) {
    e.preventDefault();
    e.stopPropagation();
    closeAllContextMenus();
    const menuWidth = 280;
    const menuHeight = 330;
    const x = Math.max(10, Math.min(e.clientX, window.innerWidth - menuWidth - 12));
    const y = Math.max(10, Math.min(e.clientY, window.innerHeight - menuHeight - 12));
    footprintContextMenu = {
      x,
      y,
      show: true,
      item
    };
  }

  function confirmCleanDnfCache() {
    closeAllContextMenus();
    uiStore.confirm(
      'Clean DNF Metadata & Package Caches?',
      'This will expire and purge DNF package caches in /var/cache to free up disk space. DNF will re-download fresh metadata on the next package check.',
      async () => {
        statusStore.setBusy('Cleaning DNF package caches...');
        try {
          await invoke('run_makecache');
          uiStore.addToast('DNF cache refreshed and cleaned successfully', 'success');
        } catch (e) {
          uiStore.addToast(`Failed to clean cache: ${e}`, 'error');
        } finally {
          statusStore.clearBusy();
        }
      },
      false
    );
  }

  function closeAllContextMenus() {
    serviceContextMenu.show = false;
    metricContextMenu.show = false;
    logContextMenu.show = false;
    processContextMenu.show = false;
    storageContextMenu.show = false;
    footprintContextMenu.show = false;
  }

  function activateOnKeyboard(e: KeyboardEvent, action: () => void) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      action();
    }
  }

  // Sparkline history tracking
  let cpuHistory = $state<number[]>([]);
  let ramHistory = $state<number[]>([]);

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

  let primaryAdapterLabel = $derived.by(() => {
    if (!networkInterfaces || networkInterfaces.length === 0) return 'eth0 (Ethernet)';
    const active = networkInterfaces.find((i: any) => i.is_up && i.iface_type !== 'loopback' && i.ip4)
      || networkInterfaces.find((i: any) => i.is_up && i.iface_type !== 'loopback')
      || networkInterfaces[0];
    if (!active) return 'eth0';
    const typeLabel = active.iface_type === 'wifi' ? 'Wi-Fi' : active.iface_type === 'ethernet' ? 'Ethernet' : active.iface_type;
    return `${active.name} (${typeLabel})`;
  });

  let cpuHigh = $derived(systemStats && systemStats.cpu_percent > 85);
  let ramHigh = $derived(systemStats && systemStats.ram_percent > 90);
  let hasFailedServices = $derived(failedServicesCount > 0);

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

  let cpuTempClass = $derived.by(() => {
    if (cpuTempNumeric !== null && cpuTempNumeric > 85) return 'temp-crit';
    if (cpuTempNumeric !== null && cpuTempNumeric > 70) return 'temp-warn';
    return 'temp-normal';
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

  let securityCriticalCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Critical' && !f.is_resolved && !mutedIds.includes(f.id)).length : 0);
  let securityWarningCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Warning' && !f.is_resolved && !mutedIds.includes(f.id)).length : 0);

  function getScoreColor(score: number) {
    if (score >= 80) return '#22C55E';
    if (score >= 50) return '#D97706';
    return '#EF4444';
  }

  function getScoreLabel(score: number) {
    if (score >= 90) return 'EXCELLENT';
    if (score >= 80) return 'GOOD';
    if (score >= 60) return 'FAIR';
    if (score >= 40) return 'POOR';
    return 'CRITICAL RISK';
  }



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
      const rawLogs: string[] = await invoke('get_journal_logs', { priority: 4 });
      if (Array.isArray(rawLogs) && rawLogs.length > 0) {
        recentLogStream = rawLogs.slice(0, 20).map(line => {
          try {
            const obj = JSON.parse(line);
            const ts = obj.__REALTIME_TIMESTAMP ? new Date(parseInt(obj.__REALTIME_TIMESTAMP) / 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }) : '—';
            const svc = obj.SYSLOG_IDENTIFIER || obj._COMM || 'systemd';
            const prio = Number.parseInt(obj.PRIORITY ?? '', 10);
            const level = Number.isNaN(prio) ? 'Info' : prio <= 3 ? 'Err' : prio === 4 ? 'Warn' : 'Info';
            return { time: ts, service: svc, level, message: obj.MESSAGE || '' };
          } catch {
            return { time: '—', service: 'system', level: 'Info', message: line };
          }
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

  function openStoragePathModal(mount: string, device = '/dev/sda3', used_gb = 17.3, total_gb = 235.9, fs_type = 'btrfs') {
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

  async function handleOpenInFileManager(path?: string) {
    if (!path) return;
    try {
      await invoke('open_folder', { path });
    } catch (e) {
      console.error('Failed to launch file manager for path:', path, e);
    }
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

  async function fetchData() {
    fetchRecentLogs();
    fetchServicesWatchdog();
    fetchTopProcesses();
    try {
      const [os, stats, disks, ifaces, lastUpdate, failedSvc, temp, traffic] = await Promise.all([
        invoke('get_os_info'),
        invoke('get_system_stats'),
        invoke('get_disk_usage'),
        invoke('get_network_interfaces'),
        invoke<string>('get_last_system_update').catch(() => ''),
        invoke<number>('get_failed_services_count').then(count => {
          failedServicesCountLoaded = true;
          return count;
        }).catch(() => {
          failedServicesCountLoaded = false;
          return 0;
        }),
        invoke<number | null>('get_cpu_temperature').catch(() => null),
        invoke<Array<{ interface: string; rx_bytes: number; tx_bytes: number }>>('get_network_traffic').catch(() => [])
      ]);

      osInfo = os;
      systemStats = stats;
      systemStatsError = false;
      systemStatsLoaded = true;
      lastDashboardCheck = new Date();
      diskUsage = disks as any[];
      networkInterfaces = ifaces as any[];
      lastSystemUpdate = lastUpdate;
      failedServicesCount = failedSvc;

      if (temp !== null && temp !== undefined && !isNaN(temp)) {
        cpuTemp = `${Math.round(temp)}°C`;
        cpuTempAvailable = true;
      } else {
        cpuTemp = '';
        cpuTempAvailable = false;
      }

      // Calculate real live network throughput
      trafficAvailable = Array.isArray(traffic) && traffic.length > 0;
      if (Array.isArray(traffic) && traffic.length > 0) {
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

      // Update sparkline histories
      if (systemStats) {
        cpuHistory = [...cpuHistory.slice(1), Math.round(systemStats.cpu_percent || 0)];
        ramHistory = [...ramHistory.slice(1), Math.round(systemStats.ram_percent || 0)];
      }

      fetchNetworkDetails();
      fetchSystemEvents();
    } catch (e) {
      systemStatsError = true;
      systemStatsLoaded = true;
      trafficAvailable = false;
      console.error("Dashboard fetch error:", e);
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
    if (!raw) return '17 May 2026, 15:21';
    try {
      const d = new Date(raw);
      if (!isNaN(d.getTime())) {
        const datePart = d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
        const timePart = d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', hour12: false });
        return `${datePart}, ${timePart}`;
      }
    } catch {}
    const cleaned = raw.replace(/^[A-Za-z]{3}\s+/, '').replace(/(:\d{2})\s*(AM|PM).*/i, '');
    return cleaned || '17 May 2026, 15:21';
  }

  let pollInterval: any = null;

  function stopPolling() {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function startPolling() {
    if (!pollInterval && !uiStore.isThrottled) {
      pollInterval = setInterval(fetchData, 4000);
    }
  }

  $effect(() => {
    if (uiStore.isThrottled) {
      stopPolling();
    } else {
      startPolling();
    }
  });

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

  // Header subtitle uses values from the live system response.
  let headerSubtitle = $derived.by(() => {
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

    let sync = '';
    if (lastSystemUpdate) {
      try {
        const d = new Date(lastSystemUpdate);
        if (!isNaN(d.getTime())) {
          sync = `last sync ${d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })}`;
        } else {
          sync = `last sync ${lastSystemUpdate.slice(0, 10)}`;
        }
      } catch {
        sync = `last sync ${lastSystemUpdate.slice(0, 10)}`;
      }
    }

    if (!osInfo && !systemStats) return systemStatsError ? 'System details unavailable' : 'System details loading…';
    return [distro, kernel, uptime, sync].filter(Boolean).join(' · ');
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
    action: () => void;
  }

  function formatCheckedAt(date: Date | null): string {
    return date ? `Checked ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}` : 'Check time unavailable';
  }

  let needsAttentionList = $derived.by<NeedsAttentionItem[]>(() => {
    const items: NeedsAttentionItem[] = [];

    // Security score
    if (securityReport && effectiveDashboardScore < 80) {
      const isCrit = effectiveDashboardScore < 50;
      items.push({
        id: 'security-score',
        severity: isCrit ? 'critical' : 'warning',
        label: 'Security Posture Audit',
        value: `${effectiveDashboardScore}/100 score (${isCrit ? 'Critical Risk' : 'Warnings Detected'})`,
        checkedAt: formatCheckedAt(lastSecurityCheck),
        action: () => uiStore.navigateTo('security-center', 'advisory')
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
        value: `${logErrCount} critical error${logErrCount > 1 ? 's' : ''} found`,
        checkedAt: formatCheckedAt(lastDashboardCheck),
        action: () => uiStore.navigateTo('journal-logs')
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
        checkedAt: formatCheckedAt(lastDashboardCheck),
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
      items.push({
        id: 'failed-services',
        severity: 'critical',
        label: 'System Services Failed',
        value: `${totalFailed} service${totalFailed > 1 ? 's' : ''} in failed state`,
        checkedAt: formatCheckedAt(lastServicesCheck),
        action: () => uiStore.navigateTo('service-manager')
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
          checkedAt: formatCheckedAt(lastDashboardCheck),
          action: () => uiStore.navigateTo('device-manager')
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

  let worstSeverity = $derived.by<'critical' | 'warning'>(() => {
    return needsAttentionList.some(i => i.severity === 'critical') ? 'critical' : 'warning';
  });

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
  let checksUnavailable = $derived(securityError || (logsError && systemEventsError) || servicesError || systemStatsError || (systemStatsLoaded && (!cpuTempAvailable || !rootDisk)));

</script>

<div class="dashboard-page">
  <!-- 1. Page Header (Fixed) -->
  <div class="dashboard-header-fixed">
    <div class="dashboard-header-row">
      <div class="dashboard-header-left">
        <h1 class="dashboard-title">System Overview</h1>
        <p class="dashboard-subtitle">{headerSubtitle}{lastDashboardCheck ? ` · Updated ${lastDashboardCheck.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}` : ''}{systemStatsError ? ' · Update failed; values may be stale' : ''}</p>
      </div>
      <div class="header-actions-group">
        <Button variant="ghost" size="sm" onclick={() => uiStore.navigateTo('app-manager')} title="Manage installed applications">
          <LayoutDashboard size={13} />
          <span>Applications</span>
        </Button>
        <Button variant="primary" size="sm" onclick={() => uiStore.navigateTo('dnf-history')} title="Check for system updates">
          <Package size={13} />
          <span>{uiStore.availableUpdatesCount > 0 ? `${uiStore.availableUpdatesCount} updates` : 'Check updates'}</span>
        </Button>
        <Button variant="ghost" size="sm" onclick={() => fetchData()} title="Refresh telemetry">
          <RefreshCw size={13} />
        </Button>
      </div>
    </div>
  </div>

  <!-- ── Dashboard Main Content ── -->
  <div class="dashboard-content">
    <!-- 2. Health Banner (Slim bar, amber/red, hidden when 0) -->
    {#if needsAttentionList.length > 0}
      <div class="health-banner {worstSeverity}" role="alert">
        <div class="health-banner-left">
          <span class="health-dot {worstSeverity}"></span>
          <span class="health-banner-text">
            <strong>{needsAttentionList.length}</strong> {needsAttentionList.length === 1 ? 'thing needs' : 'things need'} attention
          </span>
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
          <span class="metric-tile-label">CPU</span>
          <span class="metric-tile-sub">{cpuTempAvailable ? cpuTemp : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{systemStats ? `${systemStats.cpu_percent.toFixed(1)}%` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
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
          <span class="metric-tile-label">Memory</span>
          <span class="metric-tile-sub">{systemStats ? `${(systemStats.ram_used_mb / 1024).toFixed(1)} / ${(systemStats.ram_total_mb / 1024).toFixed(0)} GB` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{systemStats ? `${systemStats.ram_percent.toFixed(1)}%` : systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
      </button>

      <!-- Tile 3: Disk -->
      <button
        type="button"
        class="metric-tile"
        onclick={() => uiStore.navigateTo('device-manager')}
        title="Open Device Manager (Disks)"
      >
        <div class="metric-tile-header">
          <span class="metric-tile-label">Disk</span>
          <span class="metric-tile-sub">{rootDisk ? `${rootDisk.used_gb.toFixed(0)} / ${rootDisk.total_gb.toFixed(0)} GB` : systemStats || systemStatsError ? 'Unavailable' : 'Loading…'}</span>
        </div>
        <div class="metric-tile-body">
          <span class="metric-tile-num mono">{rootDisk ? `${rootDisk.percent.toFixed(0)}%` : systemStats || systemStatsError ? 'Unavailable' : 'Loading…'}</span>
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
          <span class="metric-tile-label">Network</span>
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
            {#if needsAttentionList.length > 0}
              <Badge variant={worstSeverity === 'critical' ? 'error' : 'warning'}>
                {needsAttentionList.length}
              </Badge>
            {/if}
          {/snippet}

          {#if needsAttentionList.length === 0}
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
                  {:else if needsAttentionList.length === 0}No warnings or critical conditions detected{/if}
                </span>
              </div>
            </div>
          {:else}
            <div class="card-row-list">
              {#each needsAttentionList as item (item.id)}
                <button
                  type="button"
                  class="data-row clickable-row"
                  onclick={item.action}
                  title="Click to resolve: {item.label}"
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
            {#if failedServicesCount > 0}
              <Badge variant="error">{failedServicesCount} failed</Badge>
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

          {#if !logsLoaded && !systemEventsLoaded}
            <div class="all-clear-state compact"><span class="all-clear-title">Loading recent events…</span></div>
          {:else if logsError && systemEventsError}
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
                  <span class="proc-stat mono" title="CPU usage">CPU {proc.cpu_percent.toFixed(1)}%</span>
                  <span class="proc-stat mono text-muted" title="Memory usage">RAM {proc.mem_percent !== undefined || proc.memory_percent !== undefined ? `${(proc.mem_percent ?? proc.memory_percent).toFixed(1)}%` : '—'}</span>
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
    >
      <Card collapsible={true} bind:collapsed={detailsCollapsed} title="Storage Breakdown & Footprint" class="details-card">
      <div class="details-grid">
        <!-- Sub-panel 1: Storage Breakdown -->
        <div class="details-subpanel">
          <div class="subpanel-title">Storage Breakdown</div>
          <div class="card-row-list">
            {#if diskUsage && diskUsage.length > 0}
              {#each diskUsage as disk (disk.mount)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="data-row hoverable-row"
                  role="button"
                  tabindex="0"
                  onclick={() => openStoragePathModal(disk.mount, disk.device, disk.used_gb, disk.total_gb, disk.fs_type)}
                  onkeydown={(e) => { if (e.target === e.currentTarget) activateOnKeyboard(e, () => openStoragePathModal(disk.mount, disk.device, disk.used_gb, disk.total_gb, disk.fs_type)); }}
                  oncontextmenu={(e) => handleStorageContextMenu(e, { title: `${disk.mount} Storage`, mount: disk.mount, device: disk.device, fs_type: disk.fs_type, used_gb: disk.used_gb, total_gb: disk.total_gb })}
                  title="Inspect {disk.mount} storage"
                >
                  <div class="data-row-left">
                    <HardDrive size={14} class="row-icon text-muted" />
                    <span class="data-row-label mono">{disk.mount}</span>
                    <span class="text-muted" style="font-size:11px;">({disk.device})</span>
                  </div>
                  <div class="data-row-right">
                    <span class="data-row-value mono">{(disk.used_gb ?? 0).toFixed(1)} / {(disk.total_gb ?? 0).toFixed(0)} GB</span>
                    <span class="pct-badge mono">{(disk.percent ?? 0).toFixed(0)}%</span>
                  </div>
                </div>
              {/each}
            {:else}
              <div class="all-clear-state compact"><span class="all-clear-title">{systemStats || systemStatsError ? 'Storage data unavailable' : 'Loading storage…'}</span></div>
            {/if}
          </div>
        </div>

        <!-- Sub-panel 2: Application & Disk Footprint -->
        <div class="details-subpanel">
          <div class="subpanel-title">Application &amp; System Footprint</div>
          <div class="footprint-summary-list">
            {#each ['Flatpak', 'RPM'] as source}
              {@const stats = appFootprint?.[source as 'Flatpak' | 'RPM']}
              <Button
                variant="ghost"
                size="sm"
                class="footprint-summary-row clickable-row"
                onclick={() => uiStore.navigateTo('app-manager', source)}
                oncontextmenu={(e) => handleFootprintContextMenu(e, { title: `${source} applications`, size: stats ? `${stats.count} apps · ${formatAppSize(stats.sizeBytes)}` : 'Application size summary', type: source === 'Flatpak' ? 'flatpak' : 'rpm' })}
              >
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
                <ChevronRight size={14} />
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
          <AlertTriangle size={20} style="color: {worstSeverity === 'critical' ? 'var(--color-error)' : 'var(--color-warning)'};" />
          <div>
            <h3 style="margin:0; font-size:16px; font-weight:700; color:var(--color-text-primary);">
              Needs Attention ({needsAttentionList.length})
            </h3>
            <span style="font-size:11.5px; color:var(--color-text-muted);">
              System conditions that exceed normal thresholds and require administrative review
            </span>
          </div>
        </div>
        <button type="button" class="close-modal-btn" onclick={() => showAttentionModal = false}>&times;</button>
      </div>

      <div class="attention-modal-list">
        {#each needsAttentionList as item (item.id)}
          <div class="attention-modal-item {item.severity}">
            <div class="attention-item-left">
              <span class="status-dot {item.severity}"></span>
              <div class="attention-item-info">
                <span class="attention-item-label">{item.label}</span>
                <span class="attention-item-val mono">{item.value}</span>
                <span class="attention-item-val">{item.checkedAt}</span>
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              onclick={() => {
                showAttentionModal = false;
                item.action();
              }}
            >
              <span>Take Action &rarr;</span>
            </Button>
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
    <div class="modal-glass-card modal-storage-card">
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
  <div use:portal class="modal-backdrop" onclick={() => showWatchdogModal = false} role="presentation">
    <div class="modal-glass-card modal-watchdog-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === 'Escape') showWatchdogModal = false; }} role="dialog" aria-modal="true" tabindex="-1">
      <div class="modal-header-row">
        <div style="display:flex; align-items:center; gap:10px;">
          <ShieldCheck size={20} style="color:#22c55e;" />
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
                {filteredAvailableUnits.length} available
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
                  <RefreshCw size={16} class="spin" style="margin-right:6px;" /> Loading installed service units…
                </div>
              {:else if filteredAvailableUnits.length === 0}
                <div style="padding:32px; text-align:center; color:var(--color-text-muted); font-size:12.5px;">
                  {watchdogSearchQuery ? `No service found matching "${watchdogSearchQuery}". You can add it manually below.` : 'No services available.'}
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

<!-- ══ Global Window Listeners for Context Menus ══ -->
<svelte:window 
  onclick={(e) => {
    if (!(e.target as HTMLElement)?.closest('.custom-context-menu')) {
      closeAllContextMenus();
    }
  }}
  oncontextmenu={(e) => {
    if (!(e.target as HTMLElement)?.closest('.watchdog-item, .metric-btn, .log-item-line, .process-row-item, .clickable-storage-row, .footprint-row-item, .custom-context-menu')) {
      closeAllContextMenus();
    }
  }}
  onkeydown={(e) => { if (e.key === 'Escape') closeAllContextMenus(); }} 
/>

<!-- ══ Services Watchdog Context Menu ══ -->
{#if serviceContextMenu.show && serviceContextMenu.svc}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {serviceContextMenu.x}px; top: {serviceContextMenu.y}px; z-index: 99999; min-width: 250px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
        <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 140px;" title={serviceContextMenu.svc.name}>
          {serviceContextMenu.svc.name}
        </span>
        {#if isCriticalService(serviceContextMenu.svc.name)}
          <span class="protection-badge critical" style="font-size: 9px; padding: 1px 4px;">Core</span>
        {/if}
      </div>
      <span class="badge {serviceContextMenu.svc.status === 'active' ? 'badge-success' : serviceContextMenu.svc.status === 'failed' ? 'badge-error' : 'badge-muted'}" style="font-size: 9.5px; padding: 1px 5px;">
        {serviceContextMenu.svc.status}
      </span>
    </div>
    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => confirmRestartService(serviceContextMenu.svc!)}
      disabled={!!serviceActionRunning}
    >
      <RotateCcw size={14} style="color: var(--color-warning);" />
      <span>Restart Service</span>
    </button>

    {#if serviceContextMenu.svc.status !== 'active'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => { const svc = serviceContextMenu.svc!; closeAllContextMenus(); doServiceAction(svc.name, 'start'); }}
        disabled={!!serviceActionRunning}
      >
        <Play size={14} style="color: var(--color-success);" />
        <span>Start Service</span>
      </button>
    {:else}
      <button
        type="button"
        class="context-menu-item text-danger"
        onclick={() => confirmStopService(serviceContextMenu.svc!)}
        disabled={isCriticalService(serviceContextMenu.svc.name) || !!serviceActionRunning}
        title={isCriticalService(serviceContextMenu.svc.name) ? 'Cannot stop critical OS core unit' : ''}
      >
        <Square size={14} style="color: var(--color-error);" />
        <span>{isCriticalService(serviceContextMenu.svc.name) ? 'Stop Service (Locked)' : 'Stop Service'}</span>
      </button>
    {/if}

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const name = serviceContextMenu.svc!.name;
        closeAllContextMenus();
        uiStore.serviceFilter = 'all';
        uiStore.serviceSearchQuery = name.replace(/\.service$/, '');
        uiStore.navigateTo('service-manager');
      }}
    >
      <ArrowUpRight size={14} style="color: var(--color-accent);" />
      <span>Inspect in Service Manager</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const name = serviceContextMenu.svc!.name;
        closeAllContextMenus();
        uiStore.preAppliedJournalPriority = 'all';
        uiStore.preAppliedJournalSearch = name;
        uiStore.navigateTo('journal-logs', 'journal');
      }}
    >
      <FileText size={14} style="color: var(--color-accent);" />
      <span>View Logs in Journal</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const name = serviceContextMenu.svc!.name;
        closeAllContextMenus();
        copyToClipboard(`systemctl status ${name}`, 'Copied systemctl status command');
      }}
    >
      <Copy size={14} />
      <span>Copy Unit Name / CLI Command</span>
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item text-danger"
      onclick={() => confirmUnpinService(serviceContextMenu.svc!.name)}
    >
      <Trash2 size={14} />
      <span>Unpin from Dashboard</span>
    </button>
  </div>
{/if}

<!-- ══ System Events Metrics Context Menu ══ -->
{#if metricContextMenu.show}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {metricContextMenu.x}px; top: {metricContextMenu.y}px; z-index: 99999; min-width: 260px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="padding: 6px 8px; font-size: 11.5px; font-weight: 700; color: var(--color-text-primary); border-bottom: 1px solid var(--color-border); margin-bottom: 4px;">
      {#if metricContextMenu.type === 'errors'}
        <span class="text-danger">Critical &amp; Emergency Logs</span>
      {:else if metricContextMenu.type === 'warnings'}
        <span class="text-warn">System Warnings</span>
      {:else}
        <span class="text-success">System Health Telemetry</span>
      {/if}
    </div>

    {#if metricContextMenu.type === 'errors'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.preAppliedJournalPriority = '3';
          uiStore.preAppliedJournalSearch = '';
          uiStore.navigateTo('journal-logs', 'journal');
        }}
      >
        <FileText size={14} style="color: var(--color-error);" />
        <span>Open Journal Filtered to Critical (0-3)</span>
      </button>
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard(`Critical System Errors: ${metricContextMenu.count}`, 'Copied error count summary');
        }}
      >
        <Copy size={14} />
        <span>Copy Error Count Summary</span>
      </button>
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('security-auditor');
        }}
      >
        <ShieldCheck size={14} style="color: var(--color-accent);" />
        <span>Run Full Security Audit</span>
      </button>
    {:else if metricContextMenu.type === 'warnings'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.preAppliedJournalPriority = '4';
          uiStore.preAppliedJournalSearch = '';
          uiStore.navigateTo('journal-logs', 'journal');
        }}
      >
        <FileText size={14} style="color: var(--color-warning);" />
        <span>Open Journal Filtered to Warnings (4)</span>
      </button>
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard(`System Warnings: ${metricContextMenu.count}`, 'Copied warning summary');
        }}
      >
        <Copy size={14} />
        <span>Copy Warning Count</span>
      </button>
    {:else}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('system-monitor', 'overview');
        }}
      >
        <Activity size={14} style="color: var(--color-accent);" />
        <span>Open System Monitor &amp; Resources</span>
      </button>
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('security-auditor');
        }}
      >
        <ShieldCheck size={14} style="color: var(--color-success);" />
        <span>Inspect Security Audit Report</span>
      </button>
    {/if}
  </div>
{/if}

<!-- ══ Log Ticker Line Context Menu ══ -->
{#if logContextMenu.show && logContextMenu.log}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {logContextMenu.x}px; top: {logContextMenu.y}px; z-index: 99999; min-width: 280px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px; border-bottom: 1px solid var(--color-border); margin-bottom: 4px;">
      <span style="font-size: 11px; font-weight: 700; font-family: var(--font-mono); color: var(--color-text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 170px;" title={logContextMenu.log.service}>
        [{logContextMenu.log.time}] [{logContextMenu.log.service}]
      </span>
      <span class="badge {logContextMenu.log.level === 'Err' || logContextMenu.log.level === 'Crit' ? 'badge-error' : logContextMenu.log.level === 'Warn' ? 'badge-warning' : 'badge-info'}" style="font-size: 9px; padding: 1px 4px;">
        {logContextMenu.log.level}
      </span>
    </div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const svc = logContextMenu.log!.service;
        closeAllContextMenus();
        uiStore.preAppliedJournalPriority = 'all';
        uiStore.preAppliedJournalSearch = svc;
        uiStore.navigateTo('journal-logs', 'journal');
      }}
    >
      <FileText size={14} style="color: var(--color-accent);" />
      <span>Filter Journal by '{logContextMenu.log.service}'</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const msg = logContextMenu.log!.message;
        closeAllContextMenus();
        copyToClipboard(msg, 'Copied log message');
      }}
    >
      <Copy size={14} />
      <span>Copy Log Message</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const l = logContextMenu.log!;
        closeAllContextMenus();
        copyToClipboard(`[${l.time}] [${l.service}] [${l.level}] ${l.message}`, 'Copied full log line');
      }}
    >
      <Copy size={14} />
      <span>Copy Full Log Entry</span>
    </button>

    {#if logContextMenu.log.service && logContextMenu.log.service !== 'kernel' && logContextMenu.log.service !== 'system'}
      <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const svc = logContextMenu.log!.service;
          closeAllContextMenus();
          uiStore.serviceFilter = 'all';
          uiStore.serviceSearchQuery = svc.replace(/\.service$/, '');
          uiStore.navigateTo('service-manager');
        }}
      >
        <ArrowUpRight size={14} style="color: var(--color-accent);" />
        <span>Inspect '{logContextMenu.log.service}' in Service Manager</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => confirmRestartFromLog(logContextMenu.log!.service)}
      >
        <RotateCcw size={14} style="color: var(--color-warning);" />
        <span>Restart '{logContextMenu.log.service}'</span>
      </button>
    {/if}

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const query = `${logContextMenu.log!.service} ${logContextMenu.log!.message}`.trim();
        closeAllContextMenus();
        searchWeb(query);
      }}
    >
      <Globe size={14} style="color: var(--color-text-secondary);" />
      <span>Search Error on Web</span>
    </button>
  </div>
{/if}

<!-- ══ Top Active Processes Context Menu ══ -->
{#if processContextMenu.show && processContextMenu.proc}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {processContextMenu.x}px; top: {processContextMenu.y}px; z-index: 99999; min-width: 270px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
        <Activity size={14} style="color: #38bdf8; flex-shrink: 0;" />
        <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 130px;" title={processContextMenu.proc.name}>
          {processContextMenu.proc.name}
        </span>
      </div>
      <div style="display: flex; align-items: center; gap: 4px;">
        <span class="badge badge-info" style="font-size: 9.5px; padding: 1px 5px; font-family: var(--font-mono);">PID {processContextMenu.proc.pid}</span>
        <span class="badge badge-muted" style="font-size: 9px; padding: 1px 4px;">{processContextMenu.proc.user}</span>
      </div>
    </div>

    <div style="padding: 2px 8px 6px; font-size: 11px; color: var(--color-text-secondary); display: flex; gap: 8px; font-family: var(--font-mono);">
      <span style="color: {(processContextMenu.proc.cpu_percent || 0) > 10 ? 'var(--color-warning)' : 'var(--color-text-muted)'};">
        {(processContextMenu.proc.cpu_percent || 0).toFixed(1)}% CPU
      </span>
      <span>·</span>
      <span style="color: var(--color-text-muted);">
        {(processContextMenu.proc.mem_percent ?? processContextMenu.proc.memory_percent ?? 0).toFixed(1)}% RAM
      </span>
    </div>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const proc = processContextMenu.proc!;
        closeAllContextMenus();
        uiStore.processSearchQuery = proc.name;
        uiStore.navigateTo('system-monitor', 'processes');
      }}
    >
      <ArrowUpRight size={14} style="color: var(--color-accent);" />
      <span>Inspect in System Monitor</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const proc = processContextMenu.proc!;
        closeAllContextMenus();
        uiStore.journalUnitFilter = proc.name;
        uiStore.navigateTo('journal-logs', 'journal');
        uiStore.addToast(`Jumped to Journal logs for ${proc.name}`, 'info');
      }}
    >
      <FileText size={14} style="color: #38bdf8;" />
      <span>View Logs in Journal</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const name = processContextMenu.proc!.name;
        closeAllContextMenus();
        searchWeb(name + ' linux process');
      }}
    >
      <Globe size={14} style="color: var(--color-text-secondary);" />
      <span>Search Process on Web</span>
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const pid = processContextMenu.proc!.pid;
        closeAllContextMenus();
        copyToClipboard(pid.toString(), `Copied PID ${pid}`);
      }}
    >
      <Copy size={14} />
      <span>Copy PID ({processContextMenu.proc.pid})</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const proc = processContextMenu.proc!;
        closeAllContextMenus();
        copyToClipboard(proc.cmdline || proc.name, `Copied command line`);
      }}
    >
      <Terminal size={14} />
      <span>Copy Command Line</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const pid = processContextMenu.proc!.pid;
        closeAllContextMenus();
        copyToClipboard(`ps -fp ${pid}`, `Copied CLI inspection command`);
      }}
    >
      <Copy size={14} />
      <span>Copy `ps -fp {processContextMenu.proc.pid}`</span>
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item text-danger"
      onclick={() => {
        const proc = processContextMenu.proc!;
        confirmKillProcess(proc, 15);
      }}
    >
      <Square size={14} />
      <span>Terminate (SIGTERM)</span>
    </button>

    <button
      type="button"
      class="context-menu-item text-danger"
      onclick={() => {
        const proc = processContextMenu.proc!;
        confirmKillProcess(proc, 9);
      }}
    >
      <Flame size={14} />
      <span>Force Kill (SIGKILL)</span>
    </button>
  </div>
{/if}

<!-- ══ Storage & Disks Context Menu ══ -->
{#if storageContextMenu.show && storageContextMenu.item}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {storageContextMenu.x}px; top: {storageContextMenu.y}px; z-index: 99999; min-width: 270px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
        <HardDrive size={14} style="color: #3b82f6; flex-shrink: 0;" />
        <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 140px;" title={storageContextMenu.item.title}>
          {storageContextMenu.item.title}
        </span>
      </div>
      {#if storageContextMenu.item.fs_type}
        <span class="badge badge-info" style="font-size: 9.5px; padding: 1px 5px; text-transform: uppercase;">
          {storageContextMenu.item.fs_type}
        </span>
      {:else if storageContextMenu.item.is_drive}
        <span class="badge badge-success" style="font-size: 9px; padding: 1px 4px;">NVMe/SSD</span>
      {/if}
    </div>

    <div style="padding: 2px 8px 6px; font-size: 11px; color: var(--color-text-secondary); font-family: var(--font-mono);">
      {formatStorageBytes(storageContextMenu.item.used_gb)} used of {formatStorageBytes(storageContextMenu.item.total_gb)} ({((storageContextMenu.item.used_gb / storageContextMenu.item.total_gb) * 100).toFixed(1)}%)
    </div>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const it = storageContextMenu.item!;
        closeAllContextMenus();
        openStoragePathModal(it.mount || it.device, it.device, it.used_gb, it.total_gb, it.fs_type || 'btrfs');
      }}
    >
      <Search size={14} style="color: var(--color-accent);" />
      <span>Inspect Storage Path Breakdown</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        closeAllContextMenus();
        uiStore.navigateTo('device-manager', 'list');
      }}
    >
      <HardDrive size={14} style="color: #3b82f6;" />
      <span>Open Device &amp; Disk Manager</span>
    </button>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        closeAllContextMenus();
        uiStore.navigateTo('device-manager', 'list');
        uiStore.addToast('Jumped to Disk Health & Diagnostics', 'info');
      }}
    >
      <Activity size={14} style="color: var(--color-success);" />
      <span>Check SMART Health &amp; Diagnostics</span>
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const it = storageContextMenu.item!;
        closeAllContextMenus();
        copyToClipboard(it.device, `Copied device node ${it.device}`);
      }}
    >
      <Copy size={14} />
      <span>Copy Device Node ({storageContextMenu.item.device})</span>
    </button>

    {#if storageContextMenu.item.mount}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const it = storageContextMenu.item!;
          closeAllContextMenus();
          copyToClipboard(it.mount!, `Copied mount point ${it.mount}`);
        }}
      >
        <Folder size={14} />
        <span>Copy Mount Point ({storageContextMenu.item.mount})</span>
      </button>
    {/if}

    <button
      type="button"
      class="context-menu-item"
      onclick={() => {
        const it = storageContextMenu.item!;
        closeAllContextMenus();
        const cmd = it.mount ? `df -h ${it.mount}` : `lsblk -f ${it.device}`;
        copyToClipboard(cmd, `Copied command ${cmd}`);
      }}
    >
      <Terminal size={14} />
      <span>Copy `{storageContextMenu.item.mount ? `df -h ${storageContextMenu.item.mount}` : `lsblk -f ${storageContextMenu.item.device}`}`</span>
    </button>
  </div>
{/if}

<!-- ══ App & Disk Footprint Context Menu ══ -->
{#if footprintContextMenu.show && footprintContextMenu.item}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="custom-context-menu"
    style="position: fixed; left: {footprintContextMenu.x}px; top: {footprintContextMenu.y}px; z-index: 99999; min-width: 280px;"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
        <Layers size={14} style="color: #a855f7; flex-shrink: 0;" />
        <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 170px;" title={footprintContextMenu.item.title}>
          {footprintContextMenu.item.title}
        </span>
      </div>
      <span class="badge badge-info" style="font-size: 10px; padding: 1px 6px; font-family: var(--font-mono); font-weight: 600;">
        {footprintContextMenu.item.size}
      </span>
    </div>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    {#if footprintContextMenu.item.type === 'flatpak'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('app-manager', 'Flatpak');
        }}
      >
        <Package size={14} style="color: #38bdf8;" />
        <span>Open Flatpak Apps in App Manager</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('app-manager', 'Duplicates');
        }}
      >
        <Zap size={14} style="color: var(--color-warning);" />
        <span>Scan for Unused / Duplicate Flatpaks</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard('flatpak list --app --columns=name,size,application', 'Copied flatpak list command');
        }}
      >
        <Terminal size={14} />
        <span>Copy `flatpak list` Command</span>
      </button>

    {:else if footprintContextMenu.item.type === 'rpm'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('app-manager', 'RPM');
        }}
      >
        <Package size={14} style="color: #f59e0b;" />
        <span>Open Native RPM Packages</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('repo-manager');
        }}
      >
        <Database size={14} style="color: var(--color-accent);" />
        <span>Manage DNF / RPM Repositories</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard('rpm -qa --qf "%{SIZE} %{NAME}\\n" | sort -rn | head -20', 'Copied RPM size query command');
        }}
      >
        <Terminal size={14} />
        <span>Copy Largest RPMs Query Command</span>
      </button>

    {:else if footprintContextMenu.item.type === 'logs'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('journal-logs', 'journal');
        }}
      >
        <FileText size={14} style="color: #ec4899;" />
        <span>Open System Journal Viewer</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const it = footprintContextMenu.item!;
          closeAllContextMenus();
          openStoragePathModal(it.path || '/var/log', it.device || '/dev/sda3', it.used_gb || 1.4, it.total_gb || 235.9, it.fs_type || 'btrfs');
        }}
      >
        <Search size={14} style="color: var(--color-accent);" />
        <span>Inspect `/var/log` Space Breakdown</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard('journalctl --disk-usage', 'Copied journalctl disk usage command');
        }}
      >
        <Terminal size={14} />
        <span>Copy `journalctl --disk-usage` Command</span>
      </button>

    {:else if footprintContextMenu.item.type === 'cache'}
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('repo-manager');
        }}
      >
        <Database size={14} style="color: #14b8a6;" />
        <span>Open Repository Manager</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => confirmCleanDnfCache()}
      >
        <RefreshCw size={14} style="color: var(--color-warning);" />
        <span>Clean DNF Package &amp; Metadata Cache</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const it = footprintContextMenu.item!;
          closeAllContextMenus();
          openStoragePathModal(it.path || '/var/cache', it.device || '/dev/sda3', it.used_gb || 2.1, it.total_gb || 235.9, it.fs_type || 'btrfs');
        }}
      >
        <Search size={14} style="color: var(--color-accent);" />
        <span>Inspect `/var/cache` Space Breakdown</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          copyToClipboard('dnf clean all', 'Copied dnf clean all command');
        }}
      >
        <Terminal size={14} />
        <span>Copy `dnf clean all` Command</span>
      </button>

    {:else}
      <!-- type === 'path' (/home, /usr) -->
      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const it = footprintContextMenu.item!;
          closeAllContextMenus();
          openStoragePathModal(it.path || '/home', it.device || '/dev/sda3', it.used_gb || 17.3, it.total_gb || 235.9, it.fs_type || 'btrfs');
        }}
      >
        <Search size={14} style="color: var(--color-accent);" />
        <span>Inspect Storage Path Breakdown</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          closeAllContextMenus();
          uiStore.navigateTo('device-manager', 'list');
        }}
      >
        <HardDrive size={14} style="color: #3b82f6;" />
        <span>Open Storage &amp; Device Manager</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const p = footprintContextMenu.item!.path || '/home';
          closeAllContextMenus();
          copyToClipboard(p, `Copied directory path ${p}`);
        }}
      >
        <Folder size={14} />
        <span>Copy Directory Path ({footprintContextMenu.item.path})</span>
      </button>

      <button
        type="button"
        class="context-menu-item"
        onclick={() => {
          const p = footprintContextMenu.item!.path || '/home';
          closeAllContextMenus();
          copyToClipboard(`du -sh ${p}/* | sort -rh | head -10`, 'Copied du breakdown command');
        }}
      >
        <Terminal size={14} />
        <span>Copy `du -sh {footprintContextMenu.item.path}/*` Command</span>
      </button>
    {/if}
  </div>
{/if}

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
    gap: 16px;
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
  }

  .health-banner-text strong {
    font-family: var(--font-mono);
    font-weight: 700;
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
    gap: 24px;
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

  .metric-tile-num {
    font-family: var(--font-mono);
    font-size: 19px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1.2;
    white-space: nowrap;
  }

  .net-tx {
    font-size: 13.5px;
    font-weight: normal;
    color: var(--color-text-secondary);
    margin-left: 4px;
  }

  .metric-tile-sparkline {
    width: 60px;
    height: 16px;
    flex-shrink: 0;
  }

  /* ── Two-Column Grids (24px gutter, with strict minmax for responsiveness) ── */
  .two-col-grid {
    display: grid;
    gap: 24px;
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
    flex: 0 0 182px;
    height: 182px;
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
    background: #22c55e;
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
    color: #22c55e;
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
  }

  .details-card-container :global(.card-header-row) {
    cursor: pointer;
    user-select: none;
    border-radius: 6px;
    padding: 4px 6px;
    margin: -4px -6px 8px -6px;
    transition: background 0.12s ease;
  }

  .details-card-container :global(.card-header-row:hover) {
    background: var(--color-bg-hover);
  }

  :global(.details-card) {
    background: var(--color-bg-card) !important;
  }

  .details-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
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
    .metric-strip-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
    .two-col-grid.col-1-1,
    .two-col-grid.col-3-2 { grid-template-columns: minmax(0, 1fr); gap: 14px; }
    .details-grid { grid-template-columns: minmax(0, 1fr); gap: 12px; }
  }

  @media (max-width: 620px) {
    .dashboard-header-fixed { padding: 12px 14px; }
    .dashboard-header-row { align-items: flex-start; flex-direction: column; gap: 10px; }
    .header-actions-group { width: 100%; flex-wrap: wrap; gap: 6px; }
    .dashboard-content { padding: 12px; gap: 12px; }
    .metric-strip-grid { gap: 8px; }
    .metric-tile { padding: 10px; }
    .metric-tile-num { font-size: 16px; }
    .dashboard-subtitle { overflow-wrap: anywhere; }
    .health-banner { align-items: flex-start; gap: 8px; padding: 8px 10px; }
    .health-banner-link { flex-shrink: 0; }
  }

  .details-subpanel {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    overflow: hidden;
  }

  .subpanel-title {
    font-size: 11.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-muted);
    padding: 6px 12px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: rgba(255, 255, 255, 0.02);
  }

  .footprint-summary-list {
    display: flex;
    flex-direction: column;
    padding: 6px 10px;
  }

  .footprint-summary-row {
    display: flex;
    justify-content: flex-start;
    gap: 10px;
    min-height: 54px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--color-border-subtle);
    text-align: left;
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

  .footprint-hint {
    padding: 8px 10px 4px;
  }

  .pct-badge {
    font-size: 11px;
    color: var(--color-text-muted);
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
  }

  .row-icon {
    flex-shrink: 0;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .text-muted {
    color: var(--color-text-muted);
  }

  /* ── Needs Attention Modal Styles ── */
  .modal-glass-card.modal-attention-card {
    width: 600px !important;
    max-width: calc(100vw - 40px) !important;
    max-height: 80vh !important;
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
    max-height: 52vh;
    padding-right: 4px;
  }

  .attention-modal-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-radius: 8px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-subtle);
    gap: 12px;
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
    background: rgba(34, 197, 94, 0.14);
    color: #22c55e;
  }
  .unit-state-tag.failed {
    background: rgba(239, 68, 68, 0.14);
    color: #ef4444;
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
  .chip-status-dot.active { background: #22c55e; box-shadow: 0 0 6px rgba(34, 197, 94, 0.5); }
  .chip-status-dot.failed { background: #ef4444; box-shadow: 0 0 6px rgba(239, 68, 68, 0.5); }

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
    background: rgba(34, 197, 94, 0.12);
    border-color: #22c55e;
    color: #22c55e;
  }

  .watchdog-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    text-align: left;
    width: 100%;
  }
  .watchdog-left {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .status-indicator-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
  }
  .status-indicator-dot.active {
    background: #22c55e;
    box-shadow: 0 0 6px rgba(34, 197, 94, 0.6);
  }
  .status-indicator-dot.failed {
    background: #ef4444;
    box-shadow: 0 0 6px rgba(239, 68, 68, 0.6);
  }

  .watchdog-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .watchdog-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .watchdog-unit {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }
  .watchdog-state-pill {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-muted);
    text-transform: capitalize;
  }
  .watchdog-state-pill.active {
    background: rgba(34, 197, 94, 0.12);
    color: #22c55e;
  }
  .watchdog-state-pill.failed {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
  }

  /* ── 5. System Events & Log Ticker ── */
  .events-card-container {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .proportion-bar {
    height: 10px;
    border-radius: 5px;
    overflow: hidden;
    display: flex;
    width: 100%;
    gap: 2px;
  }
  .prop-segment {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    font-weight: 700;
    color: white;
  }
  .green-seg { background: #22c55e; }
  .orange-seg { background: #f59e0b; }
  .red-seg { background: #ef4444; }

  .event-metrics-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .metric-btn {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    padding: 6px 8px;
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }
  .metric-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    transform: translateY(-2px);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.3);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }
  :global(html.light-mode) .metric-btn:hover,
  :global([data-theme="light"]) .metric-btn:hover {
    background: #f1f5f9;
    border-color: #cbd5e1;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
  }
  .metric-num {
    font-size: 16px;
    font-weight: 800;
    font-family: var(--font-mono);
  }
  .text-success { color: #22c55e; }
  .text-warn { color: #f59e0b; }
  .text-danger { color: #ef4444; }
  .metric-desc {
    font-size: 9.5px;
    color: var(--color-text-muted);
    font-weight: 500;
  }

  .log-stream-box {
    background: rgba(0, 0, 0, 0.15);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .log-stream-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 10.5px;
    font-weight: 700;
    color: var(--color-text-muted);
  }
  .view-all-link {
    background: transparent;
    border: none;
    color: var(--color-accent);
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .view-all-link:hover {
    text-decoration: underline;
  }

  .log-stream-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-family: var(--font-mono);
    font-size: 10px;
  }
  .log-item-line {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    padding: 4px 8px;
    border-radius: 5px;
    background: transparent;
    border: 1px solid transparent;
    text-align: left;
    width: 100%;
  }
  .log-ts { color: var(--color-text-muted); flex-shrink: 0; }
  .log-svc { color: #38bdf8; font-weight: 600; flex-shrink: 0; }
  .log-lvl.err { color: #ef4444; font-weight: 700; flex-shrink: 0; }
  .log-lvl.warn { color: #f59e0b; font-weight: 700; flex-shrink: 0; }
  .log-lvl.info { color: #22c55e; flex-shrink: 0; }
  .log-msg {
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
    font-family: var(--font-sans);
    font-size: 10.5px;
  }

  /* ── 6. App & Disk Footprint ── */
  .footprint-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .footprint-row-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    text-align: left;
    width: 100%;
  }

  .footprint-label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11.5px;
  }
  .row-name-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row-val-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .footprint-name {
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .footprint-val {
    font-family: var(--font-mono);
    font-weight: 700;
    color: var(--color-text-secondary);
    font-size: 11px;
  }

  /* ── Card Footer Action ── */
  .card-footer-action {
    margin-top: auto;
    padding-top: 4px;
    border-top: 1px solid var(--color-border);
  }

  .footer-jump-link {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    background: transparent;
    border: none;
    color: var(--color-accent);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    padding: 4px 0;
    transition: opacity 0.12s ease;
  }
  .footer-jump-link:hover {
    opacity: 0.8;
    text-decoration: underline;
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

  /* ── Light Mode Custom Styling ── */
  :global(html.light-mode) .dash-card-wrapper,
  :global(html.light-mode) .kpi-card {
    background: #FFFFFF !important;
    border-color: #E2E8F0 !important;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04), 0 6px 16px rgba(0, 0, 0, 0.02) !important;
  }

  :global(html.light-mode) .action-pill-btn {
    background: #FFFFFF !important;
    border-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .overview-row {
    background: #F8FAFC !important;
    border-color: #E2E8F0 !important;
  }
  :global(html.light-mode) .overview-row.uptime-row {
    background: #DCFCE7 !important;
    border-color: #BBF7D0 !important;
  }

  :global(html.light-mode) .process-row-item,
  :global(html.light-mode) .watchdog-item,
  :global(html.light-mode) .footprint-row-item,
  :global(html.light-mode) .btrfs-pool-subcard,
  :global(html.light-mode) .metric-btn,
  :global(html.light-mode) .quick-chip {
    background: #F8FAFC !important;
    border-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .log-stream-box {
    background: #F8FAFC !important;
    border-color: #E2E8F0 !important;
  }

  :global(html.light-mode) .custom-context-menu {
    background: #FFFFFF !important;
    border-color: #E2E8F0 !important;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.15) !important;
  }

  :global(html.light-mode) .context-menu-item:hover:not(:disabled) {
    background: #F1F5F9 !important;
  }

  /* Context Menu Styles */
  .custom-context-menu {
    background: rgba(15, 23, 42, 0.95);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: calc(100vh - 24px);
    overflow-y: auto;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background 0.15s ease;
  }

  .context-menu-item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }

  .context-menu-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .context-menu-item.text-danger {
    color: var(--color-error);
  }

  .context-menu-item.text-danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
  }

  .protection-badge.critical {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 4px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
</style>
