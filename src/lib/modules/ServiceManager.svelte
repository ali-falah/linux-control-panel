<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Settings, RefreshCw, Search, Play, Square, RotateCcw,
    FileText, ShieldBan, ShieldCheck, ShieldAlert, Rocket, ChevronRight, User, Server, Activity, Network, GitFork, Link2,
    Copy, Edit3, Lock, Unlock, Clock, Cpu, HardDrive, Layers, ArrowUpRight, GitBranch, ListOrdered, Timer
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import ContextMenu from '../components/ui/ContextMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Card from '../components/ui/Card.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import BulkActionBar from '../components/ui/BulkActionBar.svelte';
  import ServiceManagerModals from './service-manager/ServiceManagerModals.svelte';
  import ServiceDrawer from './service-manager/ServiceDrawer.svelte';
  import type { AutostartEntry, ServiceAction, ServiceUnit } from './service-manager/types';
  import { loadRecoveryWarning, persistRecoveryWarning, reduceRecoveryWarning } from '../utils/serviceRecovery';
  import { bootEnableState } from '../utils/serviceUnits';

  let recoveryWarning = $state<string | null>(
    typeof localStorage !== 'undefined' ? loadRecoveryWarning(localStorage) : null
  );

  function setRecoveryWarning(next: string | null) {
    recoveryWarning = next;
    if (typeof localStorage !== 'undefined') persistRecoveryWarning(localStorage, next);
  }

  async function retryDaemonReload() {
    try {
      await invoke<string>('unit_daemon_reload');
      setRecoveryWarning(
        reduceRecoveryWarning(recoveryWarning, { kind: 'recovery-succeeded' })
      );
      uiStore.addToast('systemd daemon reloaded successfully', 'success');
    } catch (e) {
      setRecoveryWarning(
        reduceRecoveryWarning(recoveryWarning, { kind: 'recovery-failed', error: e })
      );
      uiStore.addToast(`daemon-reload retry failed: ${e}`, 'error');
    }
  }
  import {
    Plus, Trash2
  } from '@lucide/svelte';

  // ─── Tab ──────────────────────────────────────────────────────────────────
  type MainTab = 'services' | 'autostart' | 'boot_analyzer';
  let mainTab = $state<MainTab>(
    (uiStore.targetSubTab === 'boot_analyzer' || uiStore.targetSubTab === 'autostart')
      ? uiStore.targetSubTab
      : 'services'
  );

  // ─── Service Manager state ─────────────────────────────────────────────────
  let showCreateUnitModal = $state(false);
  let autostartModalEntry = $state<AutostartEntry | null | undefined>(undefined);

  let units = $state<ServiceUnit[]>([]);
  let loading = $state(false);
  let filter = $state(uiStore.serviceSearchQuery || '');
  let statusFilter = $state<'active' | 'failed' | 'all'>(
    uiStore.serviceFilter === 'failed' ? 'failed' : 'all'
  );

  $effect(() => {
    // Service Manager deep links are one-shot: apply the pending navigation
    // state, then consume it. Leaving it in the store makes the effect
    // override the user's own tab/filter/search choices on every change.
    const pendingSubTab = uiStore.targetSubTab;
    if (pendingSubTab === 'services' || pendingSubTab === 'autostart' || pendingSubTab === 'boot_analyzer') {
      mainTab = pendingSubTab;
      uiStore.targetSubTab = null;
    }

    const pendingSearch = uiStore.serviceSearchQuery;
    if (pendingSearch !== null) {
      filter = pendingSearch;
      mainTab = 'services';
      uiStore.serviceSearchQuery = null;
    }

    const pendingFilter = uiStore.serviceFilter;
    if (pendingFilter) {
      statusFilter =
        pendingFilter === 'failed' ? 'failed' :
        pendingFilter === 'active' ? 'active' : 'all';
      mainTab = 'services';
      uiStore.serviceFilter = null;
    }
  });

  let selectedUnit = $state<ServiceUnit | null>(null);
  let panelScope = $state(false);
  let highlightedUnit = $state<string | null>(null);
  let activePanel = $state<'logs' | 'editor' | 'dependencies' | null>(null);
  let panelOpen = $state(false);
  let actionInProgress = $state<string | null>(null);
  let bulkInProgress = $state(false);
  let selectedUnitNames = $state<Set<string>>(new Set());

  // Context Menu State for Services
  let contextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    unit: ServiceUnit | null;
  }>({ x: 0, y: 0, show: false, unit: null });

  function handleServiceContextMenu(e: MouseEvent, unit: ServiceUnit) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = { x: e.clientX, y: e.clientY, show: true, unit };
  }

  function serviceContextItems(unit: ServiceUnit): Array<{
    label: string;
    icon?: any;
    action?: () => void;
    danger?: boolean;
    disabled?: boolean;
    divider?: boolean;
    color?: string;
  }> {
    const items = [];
    items.push({
      label: 'Restart Service',
      icon: RotateCcw,
      color: 'var(--color-warning)',
      disabled: !!actionInProgress,
      action: () => confirmDoAction(unit, 'restart'),
    });
    if (unit.active_state !== 'active') {
      items.push({
        label: 'Start Service',
        icon: Play,
        color: 'var(--color-success)',
        disabled: !!actionInProgress,
        action: () => confirmDoAction(unit, 'start'),
      });
    } else {
      const locked = unit.protection_level === 'critical';
      items.push({
        label: locked ? 'Stop Service (Locked)' : 'Stop Service',
        icon: Square,
        danger: true,
        disabled: locked || !!actionInProgress,
        action: () => confirmDoAction(unit, 'stop'),
      });
    }
    const masked = unit.unit_file_state === 'masked' || unit.unit_file_state === 'masked-runtime';
    const bootState = bootEnableState(unit.unit_file_state);
    if (bootState === 'unavailable') {
      items.push({
        label: masked ? 'Unmask First to Enable at Boot' : 'Boot Autostart Unavailable',
        icon: ShieldBan,
        color: 'var(--color-text-muted)',
        disabled: true,
      });
    } else if (bootState === 'enabled-runtime') {
      items.push({
        label: 'Enabled for This Boot Only',
        icon: Clock,
        color: 'var(--color-info)',
        disabled: true,
      });
    } else {
      const bootLocked = bootState === 'enabled' && unit.is_protected;
      items.push({
        label: bootLocked ? 'Disable at Boot (Locked)' : (bootState === 'enabled' ? 'Disable at Boot' : 'Enable at Boot'),
        icon: ShieldCheck,
        color: bootLocked ? 'var(--color-text-muted)' : 'var(--color-accent)',
        disabled: bootLocked || !!actionInProgress,
        action: () => confirmDoAction(unit, bootState === 'enabled' ? 'disable' : 'enable'),
      });
    }
    items.push({
      label: 'View Service Logs (Journalctl)',
      icon: Activity,
      color: 'var(--color-info)',
      action: () => { uiStore.jumpToJournalService(unit.name); uiStore.setActiveTab('journal-logs'); },
    });
    items.push({ label: '', divider: true });
    items.push({
      label: 'Edit Unit Override',
      icon: Edit3,
      action: () => openEditor(unit),
    });
    items.push({
      label: 'Inspect Dependencies',
      icon: GitFork,
      action: () => openDependencies(unit),
    });
    items.push({
      label: masked ? 'Unmask Service' : (unit.is_protected ? 'Mask Service (Locked)' : 'Mask Service Unit'),
      icon: masked ? Unlock : Lock,
      color: masked ? 'var(--color-success)' : 'var(--color-warning)',
      danger: !masked,
      disabled: (!masked && unit.is_protected) || !!actionInProgress,
      action: () => confirmDoAction(unit, masked ? 'unmask' : 'mask'),
    });
    items.push({
      label: 'Copy Unit Name',
      icon: Copy,
      action: () => {
        navigator.clipboard.writeText(unit.name);
        uiStore.addToast(`Copied unit name: ${unit.name}`, 'info');
      },
    });
    return items;
  }

  function serviceContextBadges(unit: ServiceUnit): Array<{
    text: string;
    variant: 'success' | 'error' | 'warning' | 'muted';
  }> {
    const badges: Array<{ text: string; variant: 'success' | 'error' | 'warning' | 'muted' }> = [];
    if (unit.is_protected) {
      badges.push({
        text: unit.protection_level === 'critical' ? 'Core' : 'Protected',
        variant: unit.protection_level === 'critical' ? 'error' : 'warning',
      });
    }
    badges.push({
      text: unit.active_state,
      variant:
        unit.active_state === 'active'
          ? 'success'
          : unit.active_state === 'failed'
            ? 'error'
            : 'muted',
    });
    return badges;
  }

  function closeContextMenu() {
    contextMenu.show = false;
  }

  // Service Dependencies State

  // System vs User scope
  let userScope = $state(false);

  // Boot Analyzer state
  interface BlameEntry {
    time_ms: number;
    time_str: string;
    name: string;
    unit_type: string;
    is_service: boolean;
    is_protected: boolean;
    protection_level: string;
    protection_reason: string | null;
  }

  interface BootTimeBreakdown {
    firmware_ms: number;
    firmware_str: string;
    loader_ms: number;
    loader_str: string;
    kernel_ms: number;
    kernel_str: string;
    initrd_ms: number;
    initrd_str: string;
    userspace_ms: number;
    userspace_str: string;
    total_ms: number;
    total_str: string;
    target_reached_str: string;
    raw_summary: string;
  }

  interface CriticalChainEntry {
    line: string;
    unit: string;
    active_at: string;
    duration: string;
    depth: number;
  }

  let blameEntries = $state<BlameEntry[]>([]);
  let bootTimes = $state<BootTimeBreakdown | null>(null);
  let criticalChain = $state<CriticalChainEntry[]>([]);
  let loadingBlame = $state(false);
  let blameViewMode = $state<'blame' | 'critical-chain'>('blame');
  let blameFilter = $state<'all' | 'services' | 'slow' | 'critical'>('all');
  let blameSearch = $state('');

  // Boot Logs Drawer State
  let bootLogsUnit = $state<string | null>(null);
  let bootLogsOpen = $state(false);
  let bootLogsLoading = $state(false);
  let bootLogsContent = $state('');

  const filteredBlame = $derived.by(() => {
    let list = blameEntries;
    if (blameFilter === 'services') {
      list = list.filter(b => b.is_service);
    } else if (blameFilter === 'slow') {
      list = list.filter(b => b.time_ms >= 2000);
    } else if (blameFilter === 'critical') {
      list = list.filter(b => b.time_ms >= 5000);
    }

    if (blameSearch.trim()) {
      const q = blameSearch.toLowerCase().trim();
      list = list.filter(b => b.name.toLowerCase().includes(q));
    }
    return list;
  });

  let blamePage = $state(1);
  const blameItemsPerPage = 30;
  let blameSortKey = $state<'time' | 'unit'>('time');
  let blameSortAsc = $state(false);
  const sortedBlame = $derived.by(() => {
    const list = [...filteredBlame];
    const dir = blameSortAsc ? 1 : -1;
    list.sort((a, b) =>
      blameSortKey === 'time'
        ? (a.time_ms - b.time_ms) * dir
        : a.name.localeCompare(b.name) * dir,
    );
    return list;
  });
  const blameTotalPages = $derived(Math.ceil(sortedBlame.length / blameItemsPerPage) || 1);
  const paginatedBlame = $derived(sortedBlame.slice((blamePage - 1) * blameItemsPerPage, blamePage * blameItemsPerPage));

  function sortBlame(key: 'time' | 'unit') {
    if (blameSortKey === key) {
      blameSortAsc = !blameSortAsc;
    } else {
      blameSortKey = key;
      blameSortAsc = key === 'unit';
    }
    blamePage = 1;
  }

  $effect(() => {
    blameSearch;
    blameFilter;
    blamePage = 1;
  });
  $effect(() => {
    if (blamePage > blameTotalPages) blamePage = blameTotalPages;
  });

  const filteredUnits = $derived(
    units.filter(u => {
      if (statusFilter === 'active' && u.active_state !== 'active') return false;
      if (statusFilter === 'failed' && u.active_state !== 'failed') return false;
      const q = filter.toLowerCase();
      return !q || u.name.toLowerCase().includes(q) || u.description.toLowerCase().includes(q);
    })
  );

  let currentPage = $state(1);
  const itemsPerPage = 30;
  let serviceSortKey = $state<'name' | 'state' | 'file'>('name');
  let serviceSortAsc = $state(true);
  const sortedUnits = $derived.by(() => {
    const list = [...filteredUnits];
    const dir = serviceSortAsc ? 1 : -1;
    const value = (unit: ServiceUnit) =>
      serviceSortKey === 'name'
        ? unit.name
        : serviceSortKey === 'state'
          ? unit.active_state
          : unit.unit_file_state;
    list.sort((a, b) => String(value(a)).localeCompare(String(value(b))) * dir);
    return list;
  });
  const totalPages = $derived(Math.ceil(sortedUnits.length / itemsPerPage) || 1);
  const paginatedUnits = $derived(sortedUnits.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage));

  function sortServices(key: 'name' | 'state' | 'file') {
    if (serviceSortKey === key) {
      serviceSortAsc = !serviceSortAsc;
    } else {
      serviceSortKey = key;
      serviceSortAsc = true;
    }
    currentPage = 1;
  }

  function sortIndicator(key: string, active: string, asc: boolean): string {
    return key === active ? (asc ? '↑' : '↓') : '↕';
  }

  function ariaSort(key: string, active: string, asc: boolean): 'ascending' | 'descending' | 'none' {
    if (key !== active) return 'none';
    return asc ? 'ascending' : 'descending';
  }

  $effect(() => { filter; statusFilter; currentPage = 1; selectedUnitNames = new Set(); });
  $effect(() => {
    if (currentPage > totalPages) currentPage = totalPages;
  });
  $effect(() => {
    currentPage;
    selectedUnitNames = new Set();
  });

  function activeStateBadge(state: string): string {
    switch (state) {
      case 'active': return 'badge-success';
      case 'failed': return 'badge-error';
      case 'activating': case 'deactivating': return 'badge-warning';
      default: return 'badge-muted';
    }
  }

  async function load() {
    selectedUnitNames = new Set();
    highlightedUnit = null;
    loading = true;
    statusStore.setBusy(`Loading ${userScope ? 'user' : 'system'} service units…`);
    try {
      units = await invoke<ServiceUnit[]>('list_all_units', { filter: null, userMode: userScope });
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} list-units`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load units: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} list-units`, 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  const ACTION_CONFIRM_LABELS: Record<string, string> = {
    start: 'Start',
    stop: 'Stop',
    restart: 'Restart',
    enable: 'Enable',
    disable: 'Disable',
    mask: 'Mask',
    unmask: 'Unmask',
    reload: 'Reload',
  };

  function confirmDoAction(unit: ServiceUnit, action: ServiceAction) {
    if (!userScope && (action === 'stop' || action === 'restart' || action === 'mask' || action === 'disable')) {
      const label = ACTION_CONFIRM_LABELS[action] ?? action;
      const warning = unit.is_protected
        ? `\n\nWARNING: ${unit.name} is a protected ${unit.protection_level === 'critical' ? 'core' : 'infrastructure'} system service. This action can cause instability or loss of functionality.`
        : '';
      uiStore.confirm(
        `Confirm ${label}`,
        `Are you sure you want to ${action} ${unit.name}?${warning}`,
        () => doAction(unit, action),
        true
      );
    } else {
      doAction(unit, action);
    }
  }

  async function doAction(unit: ServiceUnit, action: ServiceAction) {
    actionInProgress = `${unit.name}-${action}`;
    statusStore.setBusy(`${action} ${unit.name}…`);
    
    // Store backup for rollback on error
    const prevActive = unit.active_state;
    const prevSub = unit.sub_state;
    const prevFileState = unit.unit_file_state;

    // Optimistic UI state update
    if (action === 'start') {
      unit.active_state = 'activating';
      unit.sub_state = 'starting';
    } else if (action === 'stop') {
      unit.active_state = 'deactivating';
      unit.sub_state = 'stopping';
    } else if (action === 'restart') {
      unit.active_state = 'activating';
      unit.sub_state = 'restarting';
    } else if (action === 'enable') {
      unit.unit_file_state = 'enabled';
    } else if (action === 'disable') {
      unit.unit_file_state = 'disabled';
    } else if (action === 'mask') {
      unit.unit_file_state = 'masked';
    } else if (action === 'unmask') {
      unit.unit_file_state = 'disabled';
    }

    try {
      await invoke<string>('unit_action', { name: unit.name, action, userMode: userScope });
      uiStore.addToast(`${unit.name}: ${action} succeeded`, 'success');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} ${action} ${unit.name}`, 0, true);
      
      // Fast single-unit sync via get_services_status (~15ms)
      try {
        const statuses = await invoke<Array<{ name: string; active_state: string; sub_state: string; unit_file_state: string }>>(
          'get_services_status', 
          { names: [unit.name], userMode: userScope }
        );
        if (statuses && statuses.length > 0) {
          unit.active_state = statuses[0].active_state || (action === 'start' ? 'active' : 'inactive');
          unit.sub_state = statuses[0].sub_state || (action === 'start' ? 'running' : 'dead');
          if (statuses[0].unit_file_state && statuses[0].unit_file_state !== 'unknown') {
            unit.unit_file_state = statuses[0].unit_file_state;
          }
        } else {
          if (action === 'start' || action === 'restart') {
            unit.active_state = 'active';
            unit.sub_state = 'running';
          } else if (action === 'stop') {
            unit.active_state = 'inactive';
            unit.sub_state = 'dead';
          }
        }
      } catch {
        if (action === 'start' || action === 'restart') {
          unit.active_state = 'active';
          unit.sub_state = 'running';
        } else if (action === 'stop') {
          unit.active_state = 'inactive';
          unit.sub_state = 'dead';
        }
      }
    } catch (e) {
      unit.active_state = prevActive;
      unit.sub_state = prevSub;
      unit.unit_file_state = prevFileState;
      uiStore.addToast(`${action} failed: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} ${action} ${unit.name}`, 1, false);
    } finally {
      actionInProgress = null;
      statusStore.clearBusy();
    }
  }

  function openPanel(unit: ServiceUnit, panel: 'logs' | 'editor' | 'dependencies') {
    panelScope = userScope;
    selectedUnit = unit;
    activePanel = panel;
    panelOpen = true;
  }

  function openLogs(unit: ServiceUnit) { openPanel(unit, 'logs'); }
  function openEditor(unit: ServiceUnit) { openPanel(unit, 'editor'); }
  function openDependencies(unit: ServiceUnit) { openPanel(unit, 'dependencies'); }

  function toggleUnitSelection(name: string) {
    if (selectedUnitNames.has(name)) {
      selectedUnitNames.delete(name);
    } else {
      selectedUnitNames.add(name);
    }
    selectedUnitNames = new Set(selectedUnitNames);
  }

  function toggleSelectAllUnits() {
    if (selectedUnitNames.size === paginatedUnits.length) {
      selectedUnitNames = new Set();
    } else {
      selectedUnitNames = new Set(paginatedUnits.map(u => u.name));
    }
  }

  async function executeBulkServiceAction(action: ServiceAction) {
    if (selectedUnitNames.size === 0) return;
    const names = Array.from(selectedUnitNames);
    const count = names.length;
    const protectedCount = units.filter(u => selectedUnitNames.has(u.name) && u.is_protected).length;

    if (action === 'stop' || action === 'mask' || (action === 'restart' && !userScope)) {
      let warning = `Are you sure you want to ${action} ${count} selected systemd service${count > 1 ? 's' : ''}?\n\n${names.map(n => `• ${n}`).join('\n')}`;
      if (protectedCount > 0) {
        warning += `\n\n⚠️ CAUTION: ${protectedCount} of the selected units are protected core services. Modifying them may affect system stability.`;
      }
      uiStore.confirm(
        `Confirm Bulk ${action.toUpperCase()} Services`,
        warning,
        () => doExecuteBulkServiceAction(action),
        true
      );
    } else {
      await doExecuteBulkServiceAction(action);
    }
  }

  async function doExecuteBulkServiceAction(action: ServiceAction) {
    const names = Array.from(selectedUnitNames);
    const count = names.length;
    const failures: string[] = [];
    bulkInProgress = true;
    actionInProgress = 'bulk';
    uiStore.addToast(`Executing ${action} on ${count} services…`, 'info');
    try {
      for (const name of names) {
        try {
          await invoke('unit_action', { name, action, userMode: userScope });
        } catch (e) {
          failures.push(`${name}: ${e}`);
        }
      }
      const successCount = count - failures.length;
      const detail = failures.slice(0, 3).join('; ');
      const more = failures.length > 3 ? ` (+${failures.length - 3} more)` : '';
      const message = failures.length === 0
        ? `Completed ${action}: ${successCount}/${count} succeeded`
        : `Completed ${action}: ${successCount}/${count} succeeded; failed: ${detail}${more}`;
      uiStore.addToast(message, failures.length === 0 ? 'success' : 'warning');
      selectedUnitNames = new Set();
      await load();
    } finally {
      actionInProgress = null;
      bulkInProgress = false;
    }
  }

  let isRefreshing = $state(false);

  async function handleRefresh() {
    if (isRefreshing) return;
    isRefreshing = true;
    try {
      if (mainTab === 'services') {
        await load();
      } else if (mainTab === 'autostart') {
        await loadAutostart();
      } else {
        await loadBlame(true);
      }
    } finally {
      isRefreshing = false;
    }
  }

  async function loadBlame(force = false) {
    if (untrack(() => blameEntries.length) === 0 || force) {
      loadingBlame = true;
    }
    if (force) {
      statusStore.setBusy('Refreshing boot latency and critical chain…');
    }
    
    try {
      // 1. Fetch boot breakdown times first (14ms ultra fast - renders KPI cards immediately)
      invoke<BootTimeBreakdown>('get_boot_times', { force })
        .then(res => {
          bootTimes = res;
        })
        .catch(e => console.warn('get_boot_times error:', e));

      // 2. Fetch ranked blame entries (fast ~1s)
      const blamePromise = invoke<BlameEntry[]>('get_boot_blame', { force })
        .then(res => {
          blameEntries = res;
          statusStore.setLastCommand('systemd-analyze blame', 0, true);
          if (force) {
            uiStore.addToast(`Boot latency refreshed (${res.length} units)`, 'success');
          }
        })
        .catch(e => {
          console.error('get_boot_blame error:', e);
          if (blameEntries.length === 0) {
            uiStore.addToast(`Boot latency: ${e}`, 'warning');
          }
        });

      // 3. Fetch critical chain in parallel background
      const chainPromise = invoke<CriticalChainEntry[]>('get_boot_critical_chain', { force })
        .then(res => {
          criticalChain = res;
        })
        .catch(e => console.warn('get_boot_critical_chain error:', e));

      await Promise.allSettled([blamePromise, chainPromise]);
    } finally {
      loadingBlame = false;
      if (force) statusStore.clearBusy();
    }
  }

  async function openBootLogs(unitName: string) {
    bootLogsUnit = unitName;
    bootLogsOpen = true;
    bootLogsLoading = true;
    bootLogsContent = '';
    try {
      bootLogsContent = await invoke<string>('get_service_logs', { name: unitName, lines: 150, userMode: false, boot: true });
      statusStore.setLastCommand(`journalctl -b -u ${unitName} -n 150`, 0, true);
    } catch (e) {
      bootLogsContent = `Failed to load logs: ${e}`;
    } finally {
      bootLogsLoading = false;
    }
  }

  function jumpToService(unitName: string) {
    mainTab = 'services';
    statusFilter = 'all';
    filter = unitName;
    highlightedUnit = unitName;
  }

  function parseBlameTime(s: string): number {
    if (!s) return 0;
    let totalMs = 0;
    for (const part of s.split(/\s+/)) {
      if (part.endsWith('min')) {
        const val = parseFloat(part.replace('min', ''));
        if (!isNaN(val)) totalMs += val * 60000;
      } else if (part.endsWith('ms')) {
        const val = parseFloat(part.replace('ms', ''));
        if (!isNaN(val)) totalMs += val;
      } else if (part.endsWith('s')) {
        const val = parseFloat(part.replace('s', ''));
        if (!isNaN(val)) totalMs += val * 1000;
      }
    }
    return totalMs;
  }

  $effect(() => {
    if (!panelOpen) {
      if (activePanel !== null) activePanel = null;
      if (selectedUnit !== null) selectedUnit = null;
    }
  });
  $effect(() => {
    if (mainTab !== 'services') panelOpen = false;
  });

  // ─── XDG Autostart state ───────────────────────────────────────────────────
  let autostartEntries = $state<AutostartEntry[]>([]);
  let autostartLoading = $state(false);
  let autostartFilter = $state('');
  let togglingId = $state<string | null>(null);
  let autostartVisibleLimit = $state(50);

  const filteredAutostart = $derived(
    autostartEntries.filter(e => {
      const q = autostartFilter.toLowerCase();
      return !q || e.name.toLowerCase().includes(q) || e.exec.toLowerCase().includes(q);
    })
  );
  let autostartCurrentPage = $state(1);
  let autostartSortKey = $state<'name' | 'exec' | 'comment' | 'enabled'>('name');
  let autostartSortAsc = $state(true);
  const autostartItemsPerPage = 30;
  const sortedAutostart = $derived.by(() => {
    const list = [...filteredAutostart];
    list.sort((a, b) => {
      const dir = autostartSortAsc ? 1 : -1;
      if (autostartSortKey === 'enabled') {
        return (Number(a.enabled) - Number(b.enabled)) * dir;
      }
      return String(a[autostartSortKey] ?? '').localeCompare(String(b[autostartSortKey] ?? '')) * dir;
    });
    return list;
  });
  const autostartTotalPages = $derived(Math.ceil(sortedAutostart.length / autostartItemsPerPage) || 1);
  const paginatedAutostart = $derived(sortedAutostart.slice((autostartCurrentPage - 1) * autostartItemsPerPage, autostartCurrentPage * autostartItemsPerPage));

  function sortAutostart(key: 'name' | 'exec' | 'comment' | 'enabled') {
    if (autostartSortKey === key) {
      autostartSortAsc = !autostartSortAsc;
    } else {
      autostartSortKey = key;
      autostartSortAsc = true;
    }
    autostartCurrentPage = 1;
  }

  $effect(() => { autostartFilter; autostartCurrentPage = 1; });
  $effect(() => {
    if (autostartCurrentPage > autostartTotalPages) autostartCurrentPage = autostartTotalPages;
  });

  async function handleUnitCreated(name: string) {
    await load();
    jumpToService(name);
  }

  async function loadAutostart() {
    autostartLoading = true;
    statusStore.setBusy('Loading XDG autostart entries…');
    try {
      autostartEntries = (await invoke<AutostartEntry[]>('list_autostart_entries'))
        .sort((a, b) => a.name.localeCompare(b.name));
      statusStore.setLastCommand('ls ~/.config/autostart', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load autostart entries: ${e}`, 'error');
      statusStore.setLastCommand('ls ~/.config/autostart', 1, false);
    } finally {
      autostartLoading = false;
      statusStore.clearBusy();
    }
  }

  async function toggleAutostart(entry: AutostartEntry) {
    togglingId = entry.file_path;
    const newEnabled = !entry.enabled;
    try {
      await invoke('toggle_autostart', { filePath: entry.file_path, enabled: newEnabled });
      entry.enabled = newEnabled;
      autostartEntries = [...autostartEntries];
      uiStore.addToast(`Autostart "${entry.name}" ${newEnabled ? 'enabled' : 'disabled'}`, 'success');
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle autostart: ${e}`, 'error');
      statusStore.setLastCommand(`sed -i 's/Hidden=.*/Hidden=${!newEnabled}/' ${entry.file_path}`, 1, false);
    } finally {
      togglingId = null;
    }
  }

  // ─── Autostart Add / Edit Modal State & Actions ────────────────────────────
  function deleteAutostart(entry: AutostartEntry) {
    uiStore.confirm(
      'Delete Autostart Entry',
      `Are you sure you want to remove "${entry.name}" from autostart? This will permanently delete its .desktop configuration file.`,
      async () => {
        try {
          await invoke('delete_autostart_entry', { filePath: entry.file_path });
          uiStore.addToast(`Autostart "${entry.name}" deleted`, 'success');
          await loadAutostart();
        } catch (e) {
          uiStore.addToast(`Failed to delete autostart entry: ${e}`, 'error');
        }
      },
      true
    );
  }

  // ─── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    if (mainTab !== 'services') {
      selectedUnitNames = new Set();
      highlightedUnit = null;
    }
    if (mainTab === 'services') {
      load();
    } else if (mainTab === 'autostart') {
      selectedUnitNames = new Set();
      loadAutostart();
    } else if (mainTab === 'boot_analyzer') {
      loadBlame();
    }
  });
</script>

{#snippet refreshButton()}
  <button
    type="button"
    class="header-refresh-btn"
    class:refreshing={isRefreshing || loading || autostartLoading || loadingBlame}
    disabled={isRefreshing || loading || autostartLoading || loadingBlame}
    onclick={handleRefresh}
    title={(isRefreshing || loading || autostartLoading || loadingBlame) ? "Refreshing data…" : "Refresh"}
  >
    <RefreshCw size={12} class={(isRefreshing || loading || autostartLoading || loadingBlame) ? 'spin-refresh' : ''} />
    <span>{(isRefreshing || loading || autostartLoading || loadingBlame) ? 'Refreshing…' : 'Refresh'}</span>
  </button>
{/snippet}

<div class="module-page">
  <PageHeader title="Service Manager" subtitle="Browse, control, and inspect systemd service units" icon={Settings}>
    <div class="service-header-actions">
      <div class="header-tab-bar">
        <button class="header-tab-btn" class:active={mainTab === 'services'} onclick={() => mainTab = 'services'}>
          <Settings size={11} /> Services
          <span class="header-tab-count">{units.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'autostart'} onclick={() => mainTab = 'autostart'}>
          <Rocket size={11} /> Autostart
          <span class="header-tab-count">{autostartEntries.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'boot_analyzer'} onclick={() => mainTab = 'boot_analyzer'}>
          <Activity size={11} /> Boot Analyzer
          <span class="header-tab-count">{blameEntries.length}</span>
        </button>
      </div>
    </div>
  </PageHeader>

  {#if recoveryWarning}
    <div
      role="alert"
      style="display:flex; gap:12px; align-items:flex-start; margin:16px 0; padding:14px 16px; border:1px solid var(--color-error); border-left-width:5px; border-radius:8px; background:var(--color-bg-card);"
    >
      <ShieldAlert size={22} color="var(--color-error)" style="flex-shrink:0; margin-top:2px;" />
      <div style="flex:1; min-width:0;">
        <div style="font-weight:700; color:var(--color-error); font-size:13px;">
          systemd reload recovery required
        </div>
        <div style="font-size:12px; color:var(--color-text-secondary); margin-top:4px;">
          The unit file on disk was written or rolled back, but systemd could still hold the
          previous in-memory configuration. Run <strong>Retry daemon-reload</strong> to reload
          it without rewriting any configuration. This warning stays until that reload is confirmed successful.
        </div>
        <pre style="white-space:pre-wrap; word-break:break-word; font-size:11px; margin:8px 0; padding:8px; background:var(--color-bg-base); border-radius:6px; color:var(--color-text-muted);">{recoveryWarning}</pre>
        <Button variant="outline" class="btn-sm" onclick={retryDaemonReload}>
          <RefreshCw size={13} /> Retry daemon-reload
        </Button>
      </div>
    </div>
  {/if}

  {#if mainTab === 'services'}
    <!-- Filter & search row: Search on Left, Tabs & Action on Right -->
    <div class="controls-row">
      <button
        class="scope-toggle-btn"
        class:active={userScope}
        onclick={() => { userScope = !userScope; }}
        title={userScope ? "Viewing User Services (--user). Click to switch to System Services." : "Viewing System Services. Click to switch to User Services."}
      >
        {#if userScope}
          <User size={11} />
          <span>User Scope</span>
        {:else}
          <Server size={11} />
          <span>System Scope</span>
        {/if}
      </button>

      <SearchBar 
        bind:value={filter} 
        count={filteredUnits.length} 
        total={units.length} 
        placeholder="Filter services by name or description…" 
        style="min-width:260px; max-width:380px; margin:0;" 
      />

      <div class="header-spacer"></div>

      <div style="display:flex; align-items:center; gap:10px;">
        <div class="filter-pills">
          <button 
            class="pill-btn {statusFilter === 'all' ? 'active' : ''}" 
            onclick={() => statusFilter = 'all'}
          >
            All ({units.length})
          </button>
          <button 
            class="pill-btn {statusFilter === 'active' ? 'active' : ''}" 
            onclick={() => statusFilter = 'active'}
          >
            Active ({units.filter(u => u.active_state === 'active').length})
          </button>
          <button 
            class="pill-btn {statusFilter === 'failed' ? 'active' : ''}" 
            onclick={() => statusFilter = 'failed'}
          >
            Failed ({units.filter(u => u.active_state === 'failed').length})
          </button>
        </div>

        {@render refreshButton()}

        <Button
          variant="primary"
          size="sm"
          onclick={() => showCreateUnitModal = true}
          title="Create a new systemd service unit file"
        >
          <Plus size={14} />
          <span>New Service</span>
        </Button>
      </div>
    </div>
  {:else if mainTab === 'autostart'}
    <div class="controls-row">
      <SearchBar 
        bind:value={autostartFilter} 
        count={filteredAutostart.length} 
        total={autostartEntries.length} 
        placeholder="Filter autostart entries…" 
        style="min-width:260px; max-width:380px; margin:0;" 
      />
      <div class="header-spacer"></div>
      {@render refreshButton()}
      <Button
        variant="primary"
        size="sm"
        onclick={() => autostartModalEntry = null}
        title="Add new autostart application"
      >
        <Plus size={14} />
        <span>Add Autostart</span>
      </Button>
    </div>
  {:else if mainTab === 'boot_analyzer'}
    <div class="controls-row">
      <div style="display:flex; align-items:center; gap:8px;">
        <!-- View switch -->
        <div class="filter-pills">
          <button 
            class="pill-btn {blameViewMode === 'blame' ? 'active' : ''}" 
            onclick={() => blameViewMode = 'blame'}
            title="Ranked Blame List"
          >
            Ranked Blame
          </button>
          <button 
            class="pill-btn {blameViewMode === 'critical-chain' ? 'active' : ''}" 
            onclick={() => blameViewMode = 'critical-chain'}
            title="Critical Chain Dependency Hierarchy"
          >
            Critical Chain
          </button>
        </div>

        {#if blameViewMode === 'blame'}
          <!-- Filter pills -->
          <div class="filter-pills">
            <button 
              class="pill-btn {blameFilter === 'all' ? 'active' : ''}" 
              onclick={() => blameFilter = 'all'}
            >
              All ({blameEntries.length})
            </button>
            <button 
              class="pill-btn {blameFilter === 'services' ? 'active' : ''}" 
              onclick={() => blameFilter = 'services'}
            >
              Services ({blameEntries.filter(b => b.is_service).length})
            </button>
            <button 
              class="pill-btn {blameFilter === 'slow' ? 'active' : ''}" 
              onclick={() => blameFilter = 'slow'}
            >
              Slow &gt;2s ({blameEntries.filter(b => b.time_ms >= 2000).length})
            </button>
            <button 
              class="pill-btn {blameFilter === 'critical' ? 'active' : ''}" 
              onclick={() => blameFilter = 'critical'}
            >
              Critical &gt;5s ({blameEntries.filter(b => b.time_ms >= 5000).length})
            </button>
          </div>
        {/if}
      </div>

      <div class="header-spacer"></div>

      {#if blameViewMode === 'blame'}
        <SearchBar 
          bind:value={blameSearch} 
          count={filteredBlame.length} 
          total={blameEntries.length} 
          placeholder="Filter boot units…" 
          style="min-width:240px; max-width:340px; margin:0;" 
        />
      {/if}
      {@render refreshButton()}
    </div>
  {/if}

  {#if mainTab === 'services'}
    <!-- Side panel: Logs, Editor, or Dependencies -->
    <ServiceDrawer
      bind:open={panelOpen}
      unit={selectedUnit}
      panel={activePanel}
      scope={panelScope}
      recoveryWarning={recoveryWarning}
      onwarning={setRecoveryWarning}
      onretry={retryDaemonReload}
    />

    <!-- Service List -->
    <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden;">
      <div style="flex:1; display:flex; flex-direction:column; min-height:0; overflow:hidden;">
        {#if loading && units.length === 0}
          <div class="cyber-loading-matrix">
            <div class="cyber-scanner-hero">
              <div class="cyber-radar-orb">
                <div class="radar-sweep"></div>
                <Server size={24} class="radar-core-icon" />
              </div>
              <div class="cyber-scan-text">
                <div class="cyber-scan-title">Querying Systemd Unit Registry</div>
                <div class="cyber-scan-sub">Synchronizing service states, unit configurations, and security guardrails…</div>
              </div>
            </div>
            <div class="cyber-skeleton-rows">
              {#each [1, 2, 3, 4, 5, 6, 7] as _idx}
                <div class="cyber-shimmer-row" style="animation-delay: {_idx * 0.12}s">
                  <div class="shimmer-col-name">
                    <div class="shimmer-pill-title"></div>
                    <div class="shimmer-pill-sub"></div>
                  </div>
                  <div class="shimmer-col-badge"></div>
                  <div class="shimmer-col-state"></div>
                  <div class="shimmer-col-actions"></div>
                </div>
              {/each}
            </div>
          </div>
      {:else if filteredUnits.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <Settings size={32} class="empty-state-icon" style="margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No results found</span>
          <span style="color:var(--color-text-muted); margin-top:8px;">Try adjusting your search criteria.</span>
        </div>
      {:else}
        <Table tableAction={tableFeatures} actionParams={{ sortable: false }}>
          <thead>
            <tr>
              <th style="width: 40px; text-align: center;">
                <input
                  type="checkbox"
                  class="form-checkbox"
                  aria-label="Select all services on this page"
                  checked={paginatedUnits.length > 0 && selectedUnitNames.size === paginatedUnits.length}
                  onclick={toggleSelectAllUnits}
                />
              </th>
              <th aria-sort={ariaSort('name', serviceSortKey, serviceSortAsc)}>
                <button type="button" class="th-sort-btn" onclick={() => sortServices('name')}>
                  Service {sortIndicator('name', serviceSortKey, serviceSortAsc)}
                </button>
              </th>
              <th aria-sort={ariaSort('state', serviceSortKey, serviceSortAsc)}>
                <button type="button" class="th-sort-btn" onclick={() => sortServices('state')}>
                  State {sortIndicator('state', serviceSortKey, serviceSortAsc)}
                </button>
              </th>
              <th aria-sort={ariaSort('file', serviceSortKey, serviceSortAsc)}>
                <button type="button" class="th-sort-btn" onclick={() => sortServices('file')}>
                  Unit File {sortIndicator('file', serviceSortKey, serviceSortAsc)}
                </button>
              </th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each paginatedUnits as unit (unit.name)}
              <tr class:selected-unit={highlightedUnit === unit.name || selectedUnit?.name === unit.name || selectedUnitNames.has(unit.name)} oncontextmenu={(e) => handleServiceContextMenu(e, unit)}>
                <td style="text-align: center;" onclick={(e) => e.stopPropagation()}>
                  <input
                    type="checkbox"
                    class="form-checkbox"
                    checked={selectedUnitNames.has(unit.name)}
                    onchange={() => toggleUnitSelection(unit.name)}
                  />
                </td>
                <td style="min-width:240px">
                  <div style="display:flex; align-items:center; gap:6px;">
                    <span style="font-weight:600;color:var(--color-text-primary);font-family:var(--font-mono);font-size:12px">{unit.name}</span>
                    {#if unit.is_protected}
                      {#if unit.protection_level === 'critical'}
                        <span class="protection-badge critical" title={unit.protection_reason || 'Critical operating system core component. Destructive actions are strictly locked.'}>
                          <ShieldAlert size={10} /> Core
                        </span>
                      {:else}
                        <span class="protection-badge essential" title={unit.protection_reason || 'Essential infrastructure service. Guarded against masking or disabling.'}>
                          <ShieldCheck size={10} /> Protected
                        </span>
                      {/if}
                    {/if}
                  </div>
                  {#if unit.description}
                    <div style="font-size:11px;color:var(--color-text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:240px">{unit.description}</div>
                  {/if}
                </td>
                <td>
                  <span class="badge {activeStateBadge(unit.active_state)}">{unit.active_state}</span>
                  <div style="font-size:10px;color:var(--color-text-muted);margin-top:2px">{unit.sub_state}</div>
                </td>
                <td>
                  {#if unit.unit_file_state}
                    <span class="badge {(unit.unit_file_state === 'enabled' || unit.unit_file_state === 'enabled-runtime') ? 'badge-success' : (unit.unit_file_state === 'masked' || unit.unit_file_state === 'masked-runtime') ? 'badge-error' : 'badge-muted'}">
                      {unit.unit_file_state}
                    </span>
                  {:else}
                    <span class="badge badge-muted">—</span>
                  {/if}
                </td>
                <td style="width: 140px;">
                  <div style="display:flex;gap:8px; align-items:center">
                    {#if actionInProgress?.startsWith(`${unit.name}-`)}
                      <button class="action-btn" disabled title="Processing action…">
                        <RefreshCw size={13} class="animate-spin-slow" style="color:var(--color-accent)" />
                      </button>
                    {:else if unit.active_state !== 'active'}
                      <button class="action-btn action-start" onclick={() => confirmDoAction(unit, 'start')} title="Start" disabled={!!actionInProgress}><Play size={14}/></button>
                    {:else}
                      <button 
                        class="action-btn action-stop"
                        onclick={() => confirmDoAction(unit, 'stop')} 
                        title={unit.protection_level === 'critical' ? 'Cannot stop critical system unit' : 'Stop'} 
                        disabled={unit.protection_level === 'critical' || !!actionInProgress}
                        style={unit.protection_level === 'critical' ? 'opacity: 0.4; cursor: not-allowed;' : ''}
                      >
                        <Square size={14}/>
                      </button>
                      <button class="action-btn action-restart" onclick={() => confirmDoAction(unit, 'restart')} title="Restart" disabled={!!actionInProgress}><RotateCcw size={14}/></button>
                    {/if}
                    <KebabMenu align="right">
                      <button role="menuitem" class="menu-item text-info" onclick={() => openLogs(unit)}>
                        <FileText size={14} /> View Inline Logs
                      </button>
                      <button role="menuitem" class="menu-item text-info" onclick={() => uiStore.jumpToJournalService(unit.name)}>
                        <FileText size={14} /> Open in Journal Logs ↗
                      </button>
                      <button role="menuitem" class="menu-item text-accent" onclick={() => openDependencies(unit)}>
                        <GitFork size={14} /> Inspect Dependencies
                      </button>
                      <button role="menuitem" class="menu-item text-warning" onclick={() => openEditor(unit)}>
                        <Settings size={14} /> Edit Unit Override
                      </button>
                      <div style="height:1px; background:color-mix(in srgb, var(--color-text-muted) 35%, transparent); margin:4px 0;"></div>
                      {#if bootEnableState(unit.unit_file_state) === 'unavailable'}
                        <button
                          role="menuitem"
                          class="menu-item"
                          disabled
                          title="This unit cannot be enabled at boot (masked, static, generated, or indirect)"
                        >
                          <ShieldBan size={14} /> {(unit.unit_file_state === 'masked' || unit.unit_file_state === 'masked-runtime') ? 'Unmask First to Enable at Boot' : 'Boot Autostart Unavailable'}
                        </button>
                      {:else if bootEnableState(unit.unit_file_state) === 'enabled-runtime'}
                        <button
                          role="menuitem"
                          class="menu-item"
                          disabled
                          title="This unit is enabled for the current boot only; it will not start automatically after a reboot"
                        >
                          <Clock size={14} /> Enabled for This Boot Only
                        </button>
                      {:else if bootEnableState(unit.unit_file_state) === 'enabled'}
                        <button 
                          role="menuitem"
                          class="menu-item text-warning"
                          onclick={() => confirmDoAction(unit, 'disable')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be disabled' : ''}
                        >
                          <ShieldBan size={14} /> {unit.is_protected ? 'Disable at Boot (locked)' : 'Disable at Boot'}
                        </button>
                      {:else}
                        <button
                          role="menuitem"
                          class="menu-item text-success"
                          onclick={() => confirmDoAction(unit, 'enable')}
                          disabled={!!actionInProgress}
                        >
                          <ShieldCheck size={14} /> Enable at Boot
                        </button>
                      {/if}
                      {#if unit.unit_file_state === 'masked' || unit.unit_file_state === 'masked-runtime'}
                        <button 
                          role="menuitem"
                          class="menu-item text-success"
                          onclick={() => confirmDoAction(unit, 'unmask')}
                          disabled={!!actionInProgress}
                        >
                          <Unlock size={14} /> Unmask Service
                        </button>
                      {:else}
                        <button 
                          role="menuitem"
                          class="menu-item danger"
                          onclick={() => confirmDoAction(unit, 'mask')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be masked' : ''}
                        >
                          <Lock size={14} /> {unit.is_protected ? 'Mask Service (locked)' : 'Mask Service Unit'}
                        </button>
                      {/if}
                    </KebabMenu>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </Table>
      {/if}
      </div>
      {#if !loading && filteredUnits.length > 0 && totalPages > 1}
        <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:12px; border-top:1px solid var(--color-border); flex-shrink:0;">
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === 1} onclick={() => currentPage--}>Previous</Button>
          <span style="font-size:12px; color:var(--color-text-secondary);">Page {currentPage} of {totalPages}</span>
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={currentPage === totalPages} onclick={() => currentPage++}>Next</Button>
        </div>
      {/if}
    </div>

  <!-- ── XDG Autostart Tab ─────────────────────────────────────────────────── -->
  {:else if mainTab === 'autostart'}
    <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0;">
      <div style="flex:1; overflow-y:auto; min-height:0;">
      {#if autostartLoading}
        <div class="loading-state">
          <RefreshCw size={14} class="animate-spin-slow" /> Loading…
        </div>
      {:else if filteredAutostart.length === 0}
        <div class="empty-state">No XDG autostart entries in ~/.config/autostart/</div>
      {:else}
        <Table tableAction={tableFeatures} actionParams={{ sortable: false }}>
          <thead>
            <tr>
              <th aria-sort={ariaSort('name', autostartSortKey, autostartSortAsc)}><button type="button" class="th-sort-btn" onclick={() => sortAutostart('name')}>Application {sortIndicator('name', autostartSortKey, autostartSortAsc)}</button></th>
              <th aria-sort={ariaSort('exec', autostartSortKey, autostartSortAsc)}><button type="button" class="th-sort-btn" onclick={() => sortAutostart('exec')}>Command {sortIndicator('exec', autostartSortKey, autostartSortAsc)}</button></th>
              <th aria-sort={ariaSort('comment', autostartSortKey, autostartSortAsc)}><button type="button" class="th-sort-btn" onclick={() => sortAutostart('comment')}>Comment {sortIndicator('comment', autostartSortKey, autostartSortAsc)}</button></th>
              <th style="text-align:center" aria-sort={ariaSort('enabled', autostartSortKey, autostartSortAsc)}><button type="button" class="th-sort-btn" style="text-align:center" onclick={() => sortAutostart('enabled')}>Enabled {sortIndicator('enabled', autostartSortKey, autostartSortAsc)}</button></th>
              <th style="text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each paginatedAutostart as entry (entry.file_path)}
              <tr>
                <td>
                  <div class="col-name">{entry.name}</div>
                  <div class="col-subtext">{entry.file_path.split('/').pop()}</div>
                </td>
                <td><code class="col-code">{entry.exec}</code></td>
                <td class="col-muted">{entry.comment || '—'}</td>
                <td style="text-align:center">
                  <button
                    class="ui-toggle"
                    class:on={entry.enabled}
                    onclick={() => toggleAutostart(entry)}
                    disabled={togglingId === entry.file_path}
                    title="{entry.enabled ? 'Disable' : 'Enable'} autostart"
                    aria-checked={entry.enabled}
                    role="switch"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                </td>
                <td style="text-align:right">
                  <div style="display:flex; justify-content:flex-end; gap:4px; align-items:center;">
                    <Button
                      variant="ghost"
                      size="sm"
                      onclick={() => autostartModalEntry = entry}
                      title="Edit autostart entry"
                      style="padding:4px 6px;"
                    >
                      <Edit3 size={13} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onclick={() => deleteAutostart(entry)}
                      title="Delete autostart entry"
                      style="padding:4px 6px; color:var(--color-error, #f43f5e);"
                    >
                      <Trash2 size={13} />
                    </Button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </Table>
      {/if}
      </div>
      {#if !autostartLoading && filteredAutostart.length > 0 && autostartTotalPages > 1}
        <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:12px; border-top:1px solid var(--color-border); flex-shrink:0;">
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={autostartCurrentPage === 1} onclick={() => autostartCurrentPage--}>Previous</Button>
          <span style="font-size:12px; color:var(--color-text-secondary);">Page {autostartCurrentPage} of {autostartTotalPages}</span>
          <Button variant="outline" style="padding:4px 10px; font-size:12px;" disabled={autostartCurrentPage === autostartTotalPages} onclick={() => autostartCurrentPage++}>Next</Button>
        </div>
      {/if}
    </div>
  {:else if mainTab === 'boot_analyzer'}
    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:12px;">
      <!-- KPI Row -->
      {#if bootTimes}
        <div class="boot-kpi-grid">
          <KpiCard
            icon={Timer}
            value={bootTimes.total_str || '—'}
            label="Total Boot Time"
            subtext={bootTimes.target_reached_str || 'Firmware to desktop target'}
            title={`Total Boot Time: ${bootTimes.total_str || '—'} — ${bootTimes.target_reached_str || 'Firmware to desktop target reached'}`}
            statusText="Complete"
            statusType="success"
          />
          <KpiCard
            icon={Cpu}
            value={bootTimes.firmware_str || '—'}
            label="Firmware / UEFI"
            subtext="Hardware POST & EFI init"
            title={`Firmware / UEFI Time: ${bootTimes.firmware_str || '—'} — Hardware POST, EFI setup, and motherboard ACPI initialization`}
            iconBg="rgba(0, 218, 243, 0.12)"
            iconColor="var(--color-accent)"
          />
          <KpiCard
            icon={Layers}
            value={bootTimes.loader_str || '—'}
            label="Loader / GRUB"
            subtext="Bootloader & kernel handoff"
            title={`Loader / GRUB Time: ${bootTimes.loader_str || '—'} — Bootloader menu selection, kernel decompression & handoff`}
            iconBg="rgba(245, 158, 11, 0.12)"
            iconColor="var(--color-warning)"
          />
          <KpiCard
            icon={HardDrive}
            value={bootTimes.kernel_str || '—'}
            label="Kernel & Initrd"
            subtext={`Initrd: ${bootTimes.initrd_str || '0s'}`}
            title={`Kernel & Initrd Time: ${bootTimes.kernel_str || '—'} — Linux kernel initialization & drivers (Initrd: ${bootTimes.initrd_str || '0s'})`}
            iconBg="rgba(16, 185, 129, 0.12)"
            iconColor="var(--color-success)"
          />
          <KpiCard
            icon={Rocket}
            value={bootTimes.userspace_str || '—'}
            label="Userspace Services"
            subtext="Systemd daemons & login"
            title={`Userspace Services Time: ${bootTimes.userspace_str || '—'} — Systemd units, background services, graphical desktop & login manager`}
            iconBg="rgba(239, 68, 68, 0.12)"
            iconColor="var(--color-danger)"
          />
        </div>
      {/if}

      {#if loadingBlame && blameEntries.length === 0}
        <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden;">
          <div class="cyber-loading-matrix">
            <div class="cyber-scanner-hero">
              <div class="cyber-radar-orb">
                <div class="radar-sweep"></div>
                <Rocket size={24} class="radar-core-icon" />
              </div>
              <div class="cyber-scan-text">
                <div class="cyber-scan-title">Profiling Boot Latency & Critical Chain</div>
                <div class="cyber-scan-sub">Analyzing systemd startup timings, unit bottlenecks, and initialization stages…</div>
              </div>
            </div>
            <div class="cyber-skeleton-rows">
              {#each [1, 2, 3, 4, 5, 6, 7] as _idx}
                <div class="cyber-shimmer-row" style="animation-delay: {_idx * 0.12}s">
                  <div class="shimmer-col-name">
                    <div class="shimmer-pill-title"></div>
                    <div class="shimmer-pill-sub"></div>
                  </div>
                  <div class="shimmer-col-badge"></div>
                  <div class="shimmer-col-state"></div>
                  <div class="shimmer-col-actions"></div>
                </div>
              {/each}
            </div>
          </div>
        </div>
      {:else if blameViewMode === 'blame'}
        <!-- Ranked Blame Table -->
        <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden;">
          <div style="flex:1; display:flex; flex-direction:column; min-height:0; overflow:hidden;">
            {#if filteredBlame.length === 0}
              <div class="empty-state" style="padding: 48px 32px;">
                <Settings size={28} class="empty-state-icon" style="margin:0 auto 12px;" />
                <span style="font-size:14px; font-weight:600; color:var(--color-text-primary)">No boot units matched</span>
                <span style="color:var(--color-text-muted); font-size:12px; margin-top:4px;">Try clearing your search query or filter.</span>
              </div>
            {:else}
              <Table tableAction={tableFeatures} actionParams={{ sortable: false }} style="overflow-y:auto; border:none; border-radius:0;">
                <thead>
                  <tr>
                    <th style="width:130px;" aria-sort={ariaSort('time', blameSortKey, blameSortAsc)}>
                      <button type="button" class="th-sort-btn" onclick={() => sortBlame('time')}>Startup Time {sortIndicator('time', blameSortKey, blameSortAsc)}</button>
                    </th>
                    <th aria-sort={ariaSort('unit', blameSortKey, blameSortAsc)}>
                      <button type="button" class="th-sort-btn" onclick={() => sortBlame('unit')}>Unit / Service {sortIndicator('unit', blameSortKey, blameSortAsc)}</button>
                    </th>
                    <th style="width:100px; text-align:center;">Severity</th>
                    <th style="width:180px; text-align:right;">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {#each paginatedBlame as entry (entry.name)}
                    <tr>
                      <td>
                        <code style="font-family:var(--font-mono); font-weight:700; font-size:12px; color:var(--color-text-primary);">
                          {entry.time_str}
                        </code>
                      </td>
                      <td>
                        <div style="display:flex; align-items:center; gap:8px; flex-wrap:wrap;">
                          <span style="font-family:var(--font-mono); font-weight:600; font-size:12.5px; color:var(--color-text-primary);">{entry.name}</span>
                          <span class="badge badge-muted" style="font-size:9.5px; text-transform:uppercase;">{entry.unit_type}</span>
                          {#if entry.is_protected}
                            <span 
                              class="badge badge-warning" 
                              style="font-size:9.5px; display:inline-flex; align-items:center; gap:3px;"
                              title={entry.protection_reason || 'Protected core system component'}
                            >
                              <Lock size={10} /> PROTECTED
                            </span>
                          {/if}
                        </div>
                      </td>
                      <td style="text-align:center;">
                        {#if entry.time_ms >= 5000}
                          <span class="badge badge-error">CRITICAL</span>
                        {:else if entry.time_ms >= 2000}
                          <span class="badge badge-warning">SLOW</span>
                        {:else}
                          <span class="badge badge-success">FAST</span>
                        {/if}
                      </td>
                      <td style="text-align:right;">
                        <div style="display:inline-flex; align-items:center; gap:6px;">
                          {#if entry.is_service}
                            <Button 
                              variant="ghost" 
                              style="padding:2px 8px; height:24px; font-size:11px;"
                              onclick={() => jumpToService(entry.name)}
                              title="Inspect service details and dependencies in Services tab"
                            >
                              <ArrowUpRight size={12} /> Inspect
                            </Button>
                          {/if}
                          <Button 
                            variant="outline" 
                            style="padding:2px 8px; height:24px; font-size:11px;"
                            onclick={() => openBootLogs(entry.name)}
                            title="View boot startup journal logs"
                          >
                            <FileText size={12} /> Logs
                          </Button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </Table>
            {/if}
          </div>

          {#if filteredBlame.length > 0 && blameTotalPages > 1}
            <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:10px; border-top:1px solid var(--color-border); flex-shrink:0; background:var(--color-bg-base);">
              <Button variant="outline" style="padding:2px 10px; height:26px; font-size:11.5px;" disabled={blamePage === 1} onclick={() => blamePage--}>Previous</Button>
              <span style="font-size:12px; color:var(--color-text-secondary);">Page {blamePage} of {blameTotalPages} ({filteredBlame.length} items)</span>
              <Button variant="outline" style="padding:2px 10px; height:26px; font-size:11.5px;" disabled={blamePage === blameTotalPages} onclick={() => blamePage++}>Next</Button>
            </div>
          {/if}
        </div>
      {:else}
        <!-- Critical Chain View -->
        <div class="card" style="padding:16px; display:flex; flex-direction:column; gap:12px; overflow-y:auto; max-height:calc(100vh - 280px);">
          <div style="display:flex; justify-content:space-between; align-items:center; border-bottom:1px solid var(--color-border); padding-bottom:8px;">
            <div style="font-size:13px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
              <GitBranch size={16} class="text-accent" /> Bootloader Critical Dependency Chain
            </div>
            <span style="font-size:11.5px; color:var(--color-text-muted);">
              @ = activated at time | + = startup duration
            </span>
          </div>

          {#if criticalChain.length === 0}
            <div class="empty-state" style="padding:32px;">No critical chain data returned.</div>
          {:else}
            <div style="display:flex; flex-direction:column; gap:4px; font-family:var(--font-mono); font-size:12px;">
              {#each criticalChain as node}
                <div style="display:flex; align-items:center; gap:8px; padding:6px 10px 6px {10 + (node.depth || 0) * 12}px; border-radius:6px; background:rgba(0,0,0,0.15); border:1px solid var(--color-border);">
                  <span style="color:var(--color-text-muted);">{node.line.replace(node.unit, '').replace(`@${node.active_at}`, '').replace(`+${node.duration}`, '')}</span>
                  <span style="font-weight:600; color:var(--color-text-primary); flex:1;">{node.unit}</span>
                  {#if node.active_at}
                    <span class="badge badge-muted" style="font-size:10px;">@{node.active_at}</span>
                  {/if}
                  {#if node.duration}
                    <span class="badge badge-warning" style="font-size:10px;">
                      +{node.duration}
                    </span>
                  {/if}
                  {#if node.unit.endsWith('.service')}
                    <Button variant="ghost" style="padding:1px 6px; height:20px; font-size:10px;" onclick={() => jumpToService(node.unit)}>
                      Inspect
                    </Button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- Boot Logs Drawer -->
<SideDrawer
  bind:isOpen={bootLogsOpen}
  title={`Boot Startup Journal — ${bootLogsUnit || ''}`}
  width="640px"
>
  {#snippet headerActions()}
    {#if bootLogsUnit}
      <Button variant="outline" class="btn-sm" onclick={() => uiStore.jumpToJournalService(bootLogsUnit!)}>
        <FileText size={13} /> Full Journal ↗
      </Button>
    {/if}
  {/snippet}

  {#if bootLogsLoading}
    <div style="padding:32px; text-align:center; color:var(--color-text-muted);">
      <RefreshCw size={20} class="animate-spin-slow" /> Loading boot logs…
    </div>
  {:else}
    <div style="height:calc(100vh - 120px); overflow:auto; background:var(--color-bg-base); padding:12px; border-radius:6px; border:1px solid var(--color-border);">
      <pre style="margin:0; font-family:var(--font-mono); font-size:11.5px; color:var(--color-text-primary); white-space:pre-wrap; line-height:1.4;">{bootLogsContent || 'No boot log events recorded for this unit.'}</pre>
    </div>
  {/if}
</SideDrawer>

{#if contextMenu.unit}
  <ContextMenu
    bind:isOpen={contextMenu.show}
    x={contextMenu.x}
    y={contextMenu.y}
    title={contextMenu.unit.name}
    subtitle={contextMenu.unit.is_protected
      ? (contextMenu.unit.protection_reason || 'Protected system service')
      : ''}
    badges={serviceContextBadges(contextMenu.unit)}
    items={serviceContextItems(contextMenu.unit)}
    onclose={() => { contextMenu.show = false; }}
  />
{/if}

<ServiceManagerModals
  bind:createOpen={showCreateUnitModal}
  bind:autostartEntry={autostartModalEntry}
  userScope={userScope}
  oncreated={handleUnitCreated}
  onautostartsaved={loadAutostart}
/>

<style>/* ── Right-hand push for the shared controls row ──────────────────────── */ .header-spacer {
    flex: 1;
  }/* ── Action icon hover colors ───────────────────────────────────────── */ .action-btn.action-start:hover:not(:disabled) {
    background: var(--color-success-muted);
    color: var(--color-success);
  }.action-btn.action-stop:hover:not(:disabled) {
    background: var(--color-error-muted);
    color: var(--color-error);
  }.action-btn.action-restart:hover:not(:disabled) {
    background: var(--color-warning-muted);
    color: var(--color-warning);
  }

/* ── Stats ───────────────────────────────────────────────────────────── */ .stats-row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }.stat-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 16px;
    background: var(--color-bg-surface, #FFFFFF);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: 0 2px 8px -1px rgba(0, 0, 0, 0.06), 0 1px 3px rgba(0, 0, 0, 0.04);
    color: var(--color-text-primary);
    font-family: inherit;
    font-size: inherit;
    cursor: pointer;
    transition: all 0.2s ease;
  }.stat-chip:hover {
    border-color: var(--color-border-hover);
    box-shadow: 0 4px 14px -2px rgba(0, 0, 0, 0.10);
  }.stat-chip.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.25) !important;
  }.stat-chip.active :global(.stat-num), .stat-chip.active :global(.stat-label) {
    color: #FFFFFF !important;
    font-weight: 700;
  }.stat-num {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1;
  }.stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }/* ── Autostart tab layout ────────────────────────────────────────────── */ .table-area {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }.loading-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 13px;
  }/* Cell helpers */ .col-name {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    font-weight: 500;
  }.col-muted {
    color: var(--color-text-muted);
    font-size: 12px;
  }.col-subtext {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }.col-code {
    font-size: 11px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }/* Toggle switch */ .ui-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    border: none;
    background: rgba(255, 255, 255, 0.12);
    cursor: pointer;
    transition: background 0.2s ease;
    padding: 0;
    flex-shrink: 0;
  }.ui-toggle.on { background: var(--color-accent); }.ui-toggle:disabled { opacity: 0.35; cursor: not-allowed; }.ui-toggle-thumb {
    position: absolute;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.2s ease;
    pointer-events: none;
  }.ui-toggle.on .ui-toggle-thumb { transform: translateX(16px); }.selected-unit {
    background: var(--color-active-bg) !important;
  }/* Header controls optimization */ .service-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }.scope-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }.scope-toggle-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
  }.scope-toggle-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }.header-tab-bar {
    display: inline-flex;
    align-items: center;
    background: var(--color-tab-bar-bg, rgba(0, 0, 0, 0.2));
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }.header-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }.header-tab-btn:hover {
    color: var(--color-text-primary);
  }.header-tab-btn.active {
    background: var(--color-accent);
    color: var(--color-text-on-accent);
    font-weight: 600;
  }.header-tab-count {
    font-size: 9px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 8px;
    background: color-mix(in srgb, currentColor 18%, transparent);
    color: inherit;
  }.header-tab-btn.active .header-tab-count {
    background: color-mix(in srgb, currentColor 22%, transparent);
    color: inherit;
  }.filter-pills {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--color-tab-bar-bg);
    border: 1px solid var(--color-border);
    padding: 3px;
    border-radius: 8px;
  }.pill-btn {
    border: none;
    background: transparent;
    padding: 4px 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
  }.pill-btn:hover {
    color: var(--color-text-primary);
  }.pill-btn.active {
    background: var(--color-accent);
    color: var(--color-text-on-accent);
    font-weight: 700;
  }/* ── Security Protection Badges & Alerts ───────────────────────────── */ .protection-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex-shrink: 0;
  }.protection-badge.critical {
    background: rgba(239, 68, 68, 0.14);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
  }.protection-badge.essential {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.3);
  }/* ── Cybernetic Pulse Loading Animation ───────────────────────────── */ .cyber-loading-matrix {
    display: flex;
    flex-direction: column;
    padding: 16px;
    gap: 14px;
    height: 100%;
    min-height: 380px;
    background: var(--color-bg-base);
  }.cyber-scanner-hero {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 8px;
    background: linear-gradient(135deg, rgba(0, 218, 243, 0.07) 0%, rgba(37, 99, 235, 0.04) 100%);
    border: 1px solid rgba(0, 218, 243, 0.22);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.12), inset 0 0 16px rgba(0, 218, 243, 0.03);
  }.cyber-radar-orb {
    position: relative;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: rgba(0, 218, 243, 0.08);
    border: 1.5px solid rgba(0, 218, 243, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    box-shadow: 0 0 14px rgba(0, 218, 243, 0.25);
    overflow: hidden;
  }.radar-sweep {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: conic-gradient(from 0deg, transparent 0deg, rgba(0, 218, 243, 0.3) 300deg, rgba(0, 218, 243, 0.75) 360deg);
    animation: radar-rotate 1.8s linear infinite;
  }
@keyframes radar-rotate {from { transform: rotate(0deg); }to { transform: rotate(360deg); }
}.radar-core-icon {
    position: relative;
    z-index: 2;
    color: var(--color-accent);
    filter: drop-shadow(0 0 4px var(--color-accent));
  }.cyber-scan-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }.cyber-scan-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }.cyber-scan-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }.cyber-skeleton-rows {
    display: flex;
    flex-direction: column;
    gap: 7px;
    flex: 1;
  }.cyber-shimmer-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-radius: 6px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    position: relative;
    overflow: hidden;
    gap: 14px;
  }.cyber-shimmer-row::after {
    content: '';
    position: absolute;
    top: 0;
    left: -150%;
    width: 150%;
    height: 100%;
    background: linear-gradient(90deg, transparent 0%, rgba(0, 218, 243, 0.09) 50%, transparent 100%);
    animation: cyber-shimmer 1.6s infinite ease-in-out;
  }
@keyframes cyber-shimmer {0% { transform: translateX(0); }100% { transform: translateX(200%); }
}.shimmer-col-name {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
  }.shimmer-pill-title {
    width: 140px;
    height: 13px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }.shimmer-pill-sub {
    width: 220px;
    height: 9px;
    border-radius: 3px;
    background: var(--color-bg-hover);
  }.shimmer-col-badge {
    width: 54px;
    height: 18px;
    border-radius: 999px;
    background: var(--color-bg-raised);
  }.shimmer-col-state {
    width: 50px;
    height: 16px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }.shimmer-col-actions {
    width: 65px;
    height: 24px;
    border-radius: 6px;
    background: var(--color-bg-raised);
  }.boot-kpi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
    margin-bottom: 2px;
  }.scope-toggle-btn:focus-visible, .header-tab-btn:focus-visible,
  .trigger-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
@media (max-width: 1100px) {
    .service-header-actions {
      flex-wrap: wrap;
      justify-content: flex-end;
    }
  }  .text-secondary { color: var(--color-text-secondary); }
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 12px;
  }
  .empty-state-icon { color: var(--color-text-muted); margin-bottom: 8px; }
  .btn-text {
    background: transparent;
    border: none;
    color: var(--color-accent);
    cursor: pointer;
    font: inherit;
  }

  .th-sort-btn {
    display: inline-flex;
    align-items: center;
    justify-content: inherit;
    gap: 4px;
    width: 100%;
    padding: 0;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-align: inherit;
    cursor: pointer;
  }
  .th-sort-btn:hover { color: var(--color-text-primary); }
</style>
