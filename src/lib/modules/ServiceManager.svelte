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
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import ContextMenu from '../components/ui/ContextMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Card from '../components/ui/Card.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import BulkActionBar from '../components/ui/BulkActionBar.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';
  import Stepper from '../components/ui/Stepper.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';
  import { portal } from '../actions/portal.ts';
  import { open } from '@tauri-apps/plugin-dialog';
  import { loadRecoveryWarning, persistRecoveryWarning, reduceRecoveryWarning } from '../utils/serviceRecovery';
  import { bootEnableState } from '../utils/serviceUnits';

  let recoveryWarning = $state<string | null>(
    typeof localStorage !== 'undefined' ? loadRecoveryWarning(localStorage) : null
  );

  function setRecoveryWarning(next: string | null) {
    recoveryWarning = next;
    if (typeof localStorage !== 'undefined') persistRecoveryWarning(localStorage, next);
  }
  import {
    Plus, Folder, Check, Sparkles, FileCode, Trash2
  } from '@lucide/svelte';

  // ─── Tab ──────────────────────────────────────────────────────────────────
  type MainTab = 'services' | 'autostart' | 'boot_analyzer';
  let mainTab = $state<MainTab>(
    (uiStore.targetSubTab === 'boot_analyzer' || uiStore.targetSubTab === 'autostart')
      ? uiStore.targetSubTab
      : 'services'
  );

  // ─── Service Manager state ─────────────────────────────────────────────────
  interface ServiceUnit {
    name: string;
    load_state: string;
    active_state: string;
    sub_state: string;
    description: string;
    unit_file_state: string;
    is_protected?: boolean;
    protection_level?: string;
    protection_reason?: string;
  }

  type ServiceAction = 'start' | 'stop' | 'restart' | 'enable' | 'disable' | 'mask' | 'unmask' | 'reload';

  // ─── New Unit Creation Wizard State ──────────────────────────────────────────
  let showCreateUnitModal = $state(false);
  let createUnitStep = $state(1);
  let unitForm = $state({
    name: '',
    description: '',
    execStart: '',
    workingDir: '',
    user: 'root',
    group: 'root',
    type: 'simple',
    restartPolicy: 'on-failure',
    restartSec: '5s',
    timeoutSec: '30s',
    afterTargets: ['network.target', 'network-online.target'],
    wantsTargets: ['network-online.target'],
    wantedBy: 'multi-user.target',
    envVars: [{ key: 'NODE_ENV', value: 'production' }]
  });
  let isSubmittingUnit = $state(false);
  let copiedUnitCode = $state(false);

  let unitFormTouched = $state({
    name: false,
    execStart: false,
    workingDir: false,
    user: false,
    restartSec: false,
    timeoutSec: false
  });

  let unitFormErrors = $derived.by(() => {
    const errors: Record<string, string> = {};

    // Unit Name
    const rawName = unitForm.name.trim();
    if (!rawName) {
      errors.name = 'Unit name is required.';
    } else if (/\s/.test(rawName)) {
      errors.name = 'Unit name cannot contain whitespace.';
    } else if (!/^[a-zA-Z0-9_\-@\.]+$/.test(rawName)) {
      errors.name = 'Invalid characters (only letters, numbers, -, _, @, . allowed).';
    }

    // ExecStart
    const exec = unitForm.execStart.trim();
    if (!exec) {
      errors.execStart = 'ExecStart command / binary path is required.';
    } else if (!exec.startsWith('/') && !exec.startsWith('@') && !exec.startsWith('-') && !exec.startsWith(':')) {
      errors.execStart = 'ExecStart should start with an absolute path (e.g. /usr/bin/node ...).';
    }

    // Working Directory
    const cwd = unitForm.workingDir.trim();
    if (cwd && !cwd.startsWith('/') && !cwd.startsWith('~')) {
      errors.workingDir = 'Working directory must be an absolute path (starts with /).';
    }

    // Execution User
    const usr = unitForm.user.trim();
    if (usr && !/^[a-z_][a-z0-9_-]*[$]?$/i.test(usr)) {
      errors.user = 'Invalid username format (alphanumeric and dashes only).';
    }

    // RestartSec
    const rsec = unitForm.restartSec.trim();
    if (rsec && !/^\d+(\.\d+)?(ms|s|min|h|d|w)?$/.test(rsec)) {
      errors.restartSec = 'Invalid time span (e.g. 5s, 500ms, 2min).';
    }

    // TimeoutSec
    const tsec = unitForm.timeoutSec.trim();
    if (tsec && !/^\d+(\.\d+)?(ms|s|min|h|d|w)?$/.test(tsec)) {
      errors.timeoutSec = 'Invalid time span (e.g. 30s, 1min).';
    }

    // Env vars
    for (const ev of unitForm.envVars) {
      const k = ev.key.trim();
      if (k && !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(k)) {
        errors.envVars = `Invalid environment variable name "${k}". Must be a valid identifier.`;
        break;
      }
    }

    return errors;
  });

  let isStep1Valid = $derived(!unitFormErrors.name && !unitFormErrors.execStart && !unitFormErrors.workingDir && !unitFormErrors.user);
  let isStep2Valid = $derived(!unitFormErrors.restartSec && !unitFormErrors.timeoutSec && !unitFormErrors.envVars);

  // Generate standard systemd .service unit content
  let generatedUnitContent = $derived.by(() => {
    const lines: string[] = [];
    lines.push('[Unit]');
    lines.push(`Description=${unitForm.description || unitForm.name || 'Custom Systemd Service'}`);
    if (unitForm.afterTargets.length > 0) {
      lines.push(`After=${unitForm.afterTargets.join(' ')}`);
    }
    if (unitForm.wantsTargets.length > 0) {
      lines.push(`Wants=${unitForm.wantsTargets.join(' ')}`);
    }
    lines.push('');
    lines.push('[Service]');
    lines.push(`Type=${unitForm.type}`);
    if (unitForm.user && !userScope) lines.push(`User=${unitForm.user}`);
    if (unitForm.group && !userScope) lines.push(`Group=${unitForm.group}`);
    if (unitForm.workingDir) lines.push(`WorkingDirectory=${unitForm.workingDir}`);
    lines.push(`ExecStart=${unitForm.execStart || '/usr/bin/executable'}`);
    lines.push(`Restart=${unitForm.restartPolicy}`);
    if (unitForm.restartPolicy !== 'no') {
      lines.push(`RestartSec=${unitForm.restartSec || '5s'}`);
    }
    if (unitForm.timeoutSec) {
      lines.push(`TimeoutSec=${unitForm.timeoutSec}`);
    }
    for (const ev of unitForm.envVars) {
      if (ev.key.trim()) {
        const escaped = ev.value.trim().replace(/\\/g, '\\\\').replace(/"/g, '\\"');
        lines.push(`Environment="${ev.key.trim()}=${escaped}"`);
      }
    }
    lines.push('');
    lines.push('[Install]');
    lines.push(`WantedBy=${unitForm.wantedBy || 'multi-user.target'}`);
    lines.push('');
    return lines.join('\n');
  });

  async function browseExecStartBinary() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Executable Binary / Script'
      });
      if (selected && typeof selected === 'string') {
        unitForm.execStart = selected;
        if (!unitForm.name.trim()) {
          const base = selected.split('/').pop()?.replace(/\.[^/.]+$/, '') || '';
          if (base) unitForm.name = base;
        }
        if (!unitForm.workingDir.trim()) {
          const dir = selected.substring(0, selected.lastIndexOf('/'));
          if (dir) unitForm.workingDir = dir;
        }
      }
    } catch (err) {
      console.warn('Dialog error:', err);
    }
  }

  async function browseUnitWorkingDir() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: 'Select Working Directory'
      });
      if (selected && typeof selected === 'string') {
        unitForm.workingDir = selected;
      }
    } catch (err) {
      console.warn('Dialog error:', err);
    }
  }

  function resetUnitForm() {
    unitForm = {
      name: '',
      description: '',
      execStart: '',
      workingDir: '',
      user: 'root',
      group: 'root',
      type: 'simple',
      restartPolicy: 'on-failure',
      restartSec: '5s',
      timeoutSec: '30s',
      afterTargets: ['network.target', 'network-online.target'],
      wantsTargets: ['network-online.target'],
      wantedBy: 'multi-user.target',
      envVars: [{ key: 'NODE_ENV', value: 'production' }],
    };
    unitFormTouched = {
      name: false,
      execStart: false,
      workingDir: false,
      user: false,
      restartSec: false,
      timeoutSec: false,
    };
    copiedUnitCode = false;
    createUnitStep = 1;
  }

  async function handleCreateUnit(startImmediately = false) {
    if (Object.keys(unitFormErrors).length > 0) {
      unitFormTouched = {
        name: true,
        execStart: true,
        workingDir: true,
        user: true,
        restartSec: true,
        timeoutSec: true,
      };
      uiStore.addToast(Object.values(unitFormErrors)[0] ?? 'Fix the highlighted fields first.', 'warning');
      createUnitStep = 1;
      return;
    }
    if (!unitForm.name.trim()) {
      uiStore.addToast('Please specify a unit name (e.g. my-app.service)', 'warning');
      createUnitStep = 1;
      return;
    }
    if (!unitForm.execStart.trim()) {
      uiStore.addToast('Please specify the ExecStart command', 'warning');
      createUnitStep = 1;
      return;
    }

    let fullName = unitForm.name.trim();
    if (!fullName.endsWith('.service')) {
      fullName += '.service';
    }

    try {
      const exists = await invoke<boolean>('unit_file_exists', { name: fullName, userMode: userScope });
      if (exists) {
        uiStore.addToast(
          `A unit file named "${fullName}" already exists. Edit its override instead.`,
          'error'
        );
        createUnitStep = 1;
        return;
      }
    } catch (e) {
      uiStore.addToast(`Could not check whether ${fullName} exists: ${e}`, 'error');
      return;
    }

    if (!userScope) {
      uiStore.confirm(
        'Create System Unit as Root',
        `This will write a root-owned systemd unit file for "${fullName}" and run daemon-reload.\n\nOnly continue if you trust this unit definition.\n\n${generatedUnitContent}`,
        () => doCreateUnit(fullName, startImmediately),
        true
      );
      return;
    }
    await doCreateUnit(fullName, startImmediately);
  }

  async function doCreateUnit(fullName: string, startImmediately: boolean) {
    isSubmittingUnit = true;
    try {
      await invoke('create_unit_file', {
        name: fullName,
        content: generatedUnitContent,
        userMode: userScope
      });

      uiStore.addToast(`Unit "${fullName}" created successfully!`, 'success');
      showCreateUnitModal = false;

      if (startImmediately) {
        try {
          await invoke('unit_action', { name: fullName, action: 'start', userMode: userScope });
          uiStore.addToast(`Started "${fullName}"`, 'success');
        } catch (err) {
          uiStore.addToast(`Failed to start unit: ${err}`, 'warning');
        }
      }

      await load();
      jumpToService(fullName);
    } catch (e: any) {
      uiStore.addToast(`Failed to create unit file: ${e}`, 'error');
    } finally {
      isSubmittingUnit = false;
    }
  }

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
  let panelRequestId = 0;
  let highlightedUnit = $state<string | null>(null);
  let activePanel = $state<'logs' | 'editor' | 'dependencies' | null>(null);
  let panelOpen = $state(false);
  let logs = $state('');
  let logsLoading = $state(false);
  let unitFileContent = $state('');
  let unitFileLoading = $state(false);
  let editorTab = $state<'override' | 'effective'>('override');
  let effectiveUnitContent = $state('');
  let effectiveUnitLoading = $state(false);
  let effectiveUnitError = $state<string | null>(null);
  let actionInProgress = $state<string | null>(null);
  let bulkInProgress = $state(false);
  let editedContent = $state('');
  let saving = $state(false);
  let selectedUnitNames = $state<Set<string>>(new Set());
  let showUnitDiffModal = $state(false);

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
  interface UnitDeps {
    requires: string[];
    wants: string[];
    after: string[];
    before: string[];
  }
  let unitDeps = $state<UnitDeps | null>(null);
  let depsLoading = $state(false);
  let depsError = $state<string | null>(null);

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

  async function openLogs(unit: ServiceUnit) {
    const requestId = ++panelRequestId;
    const name = unit.name;
    const scope = userScope;
    panelScope = scope;
    selectedUnit = unit;
    activePanel = 'logs';
    panelOpen = true;
    logsLoading = true;
    logs = '';
    try {
      const content = await invoke<string>('get_service_logs', { name, lines: 100, userMode: scope });
      if (requestId !== panelRequestId) return;
      logs = content;
      statusStore.setLastCommand(`journalctl ${scope ? '--user' : ''} -u ${name} -n 100`, 0, true);
    } catch (e) {
      if (requestId !== panelRequestId) return;
      logs = `Failed to load logs: ${e}`;
      uiStore.addToast(`Failed to load logs: ${e}`, 'error');
      statusStore.setLastCommand(`journalctl ${scope ? '--user' : ''} -u ${name} -n 100`, 1, false);
    } finally {
      if (requestId === panelRequestId) logsLoading = false;
    }
  }

  async function openEditor(unit: ServiceUnit) {
    const requestId = ++panelRequestId;
    const name = unit.name;
    const scope = userScope;
    panelScope = scope;
    selectedUnit = unit;
    activePanel = 'editor';
    panelOpen = true;
    unitFileLoading = true;
    unitFileContent = '';
    editorTab = 'override';
    effectiveUnitContent = '';
    effectiveUnitError = null;
    effectiveUnitLoading = false;
    try {
      const content = await invoke<string>('read_unit_override', { name, userMode: scope });
      if (requestId !== panelRequestId) return;
      unitFileContent = content;
      editedContent = content;
      statusStore.setLastCommand(`cat ${scope ? '~/.config/systemd/user' : '/etc/systemd/system'}/${name}.d/override.conf`, 0, true);
    } catch (e) {
      if (requestId !== panelRequestId) return;
      unitFileContent = '';
      editedContent = '';
      uiStore.addToast(`Failed to read override for ${name}: ${e}`, 'error');
      statusStore.setLastCommand(`cat ${scope ? '~/.config/systemd/user' : '/etc/systemd/system'}/${name}.d/override.conf`, 1, false);
    } finally {
      if (requestId === panelRequestId) unitFileLoading = false;
    }
  }

  async function loadEffectiveUnit() {
    if (!selectedUnit || effectiveUnitLoading) return;
    const name = selectedUnit.name;
    const scope = panelScope;
    effectiveUnitLoading = true;
    effectiveUnitError = null;
    try {
      const content = await invoke<string>('read_unit_file', { name, userMode: scope });
      if (selectedUnit?.name !== name) return;
      effectiveUnitContent = content;
      statusStore.setLastCommand(`systemctl ${scope ? '--user ' : ''}cat ${name}`, 0, true);
    } catch (e) {
      if (selectedUnit?.name !== name) return;
      effectiveUnitError = `Failed to read effective unit: ${e}`;
      statusStore.setLastCommand(`systemctl ${scope ? '--user ' : ''}cat ${name}`, 1, false);
    } finally {
      effectiveUnitLoading = false;
    }
  }

  async function openDependencies(unit: ServiceUnit) {
    const requestId = ++panelRequestId;
    const name = unit.name;
    const scope = userScope;
    panelScope = scope;
    selectedUnit = unit;
    activePanel = 'dependencies';
    panelOpen = true;
    depsLoading = true;
    unitDeps = null;
    depsError = null;
    try {
      const deps = await invoke<UnitDeps>('get_unit_dependencies', { name, userMode: scope });
      if (requestId !== panelRequestId) return;
      unitDeps = deps;
      statusStore.setLastCommand(`systemctl show ${name} --property=Requires,Wants,After,Before`, 0, true);
    } catch (e) {
      if (requestId !== panelRequestId) return;
      depsError = `Failed to load dependencies: ${e}`;
      uiStore.addToast(depsError, 'error');
      statusStore.setLastCommand(`systemctl show ${name} --property=Requires,Wants,After,Before`, 1, false);
    } finally {
      if (requestId === panelRequestId) depsLoading = false;
    }
  }

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

  function confirmSaveUnitFile() {
    showUnitDiffModal = true;
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

  function overridePath(name: string, scope: boolean): string {
    return scope
      ? `~/.config/systemd/user/${name}.d/override.conf`
      : `/etc/systemd/system/${name}.d/override.conf`;
  }

  async function saveUnitFile() {
    if (!selectedUnit) return;
    saving = true;
    try {
      await invoke('write_unit_file', {
        name: selectedUnit.name,
        content: editedContent,
        userMode: panelScope,
      });
      const targetPath = overridePath(selectedUnit.name, panelScope);
      statusStore.setLastCommand(`echo "..." > ${targetPath} && systemctl ${panelScope ? '--user' : ''} daemon-reload`, 0, true);
      uiStore.addToast(`Unit override saved for ${selectedUnit.name}`, 'success');
      setRecoveryWarning(
        reduceRecoveryWarning(recoveryWarning, { kind: 'recovery-succeeded' })
      );
      unitFileContent = editedContent;
      effectiveUnitContent = '';
    } catch (e) {
      setRecoveryWarning(
        reduceRecoveryWarning(recoveryWarning, { kind: 'write-failed', error: e })
      );
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      statusStore.setLastCommand(`echo "..." > ${overridePath(selectedUnit.name, panelScope)}`, 1, false);
    } finally {
      saving = false;
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

  function closePanel() {
    panelOpen = false;
    activePanel = null;
    selectedUnit = null;
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
  interface AutostartEntry {
    name: string;
    exec: string;
    comment: string;
    enabled: boolean;
    file_path: string;
    icon: string | null;
  }

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
  let showAutostartModal = $state(false);
  let isEditingAutostart = $state(false);
  let autostartSaving = $state(false);
  let autostartOriginalFilePath = $state<string | null>(null);

  let autostartForm = $state({
    name: '',
    exec: '',
    comment: '',
    enabled: true
  });

  let autostartFormTouched = $state({
    name: false,
    exec: false
  });

  let autostartFormErrors = $derived.by(() => {
    const errors: Record<string, string> = {};
    const name = autostartForm.name.trim();
    if (!name) {
      errors.name = 'Application name is required.';
    } else if (name.includes('\n') || name.includes('\r')) {
      errors.name = 'Application name cannot contain newline characters.';
    }

    const exec = autostartForm.exec.trim();
    if (!exec) {
      errors.exec = 'Command / Executable path is required.';
    } else if (exec.includes('\n') || exec.includes('\r')) {
      errors.exec = 'Command cannot contain newline characters.';
    }

    if (autostartForm.comment.includes('\n') || autostartForm.comment.includes('\r')) {
      errors.comment = 'Comment cannot contain newline characters.';
    }

    return errors;
  });

  let isAutostartFormValid = $derived(!autostartFormErrors.name && !autostartFormErrors.exec && !autostartFormErrors.comment);

  function openAddAutostartModal() {
    isEditingAutostart = false;
    autostartOriginalFilePath = null;
    autostartForm = {
      name: '',
      exec: '',
      comment: '',
      enabled: true
    };
    autostartFormTouched = { name: false, exec: false };
    showAutostartModal = true;
  }

  function openEditAutostartModal(entry: AutostartEntry) {
    isEditingAutostart = true;
    autostartOriginalFilePath = entry.file_path;
    autostartForm = {
      name: entry.name,
      exec: entry.exec,
      comment: entry.comment || '',
      enabled: entry.enabled
    };
    autostartFormTouched = { name: false, exec: false };
    showAutostartModal = true;
  }

  async function browseAutostartExecBinary() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Executable Binary / Script'
      });
      if (selected && typeof selected === 'string') {
        autostartForm.exec = selected;
        autostartFormTouched.exec = true;
        if (!autostartForm.name.trim()) {
          const base = selected.split('/').pop()?.replace(/\.[^/.]+$/, '') || '';
          if (base) {
            autostartForm.name = base;
            autostartFormTouched.name = true;
          }
        }
      }
    } catch (err) {
      console.warn('Autostart browse dialog error:', err);
    }
  }

  async function saveAutostart() {
    autostartFormTouched.name = true;
    autostartFormTouched.exec = true;
    if (!isAutostartFormValid) return;

    autostartSaving = true;
    try {
      await invoke('save_autostart_entry', {
        originalFilePath: autostartOriginalFilePath,
        name: autostartForm.name.trim(),
        exec: autostartForm.exec.trim(),
        comment: autostartForm.comment.trim(),
        enabled: autostartForm.enabled
      });
      uiStore.addToast(
        isEditingAutostart
          ? `Autostart "${autostartForm.name.trim()}" updated successfully`
          : `Autostart "${autostartForm.name.trim()}" created successfully`,
        'success'
      );
      showAutostartModal = false;
      await loadAutostart();
    } catch (e) {
      uiStore.addToast(`Failed to save autostart entry: ${e}`, 'error');
    } finally {
      autostartSaving = false;
    }
  }

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

<div class="module-page">
  <PageHeader title="Service Manager" subtitle="Browse, control, and inspect systemd service units" icon={Settings}>
    <div class="service-header-actions">
      <div class="scope-slot" class:is-services={mainTab === 'services'}>
      {#if mainTab === 'services'}
        <!-- Single Toggleable Scope Button -->
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
      {/if}
      </div>

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
    <div class="header-row">
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

        <Button
          variant="primary"
          size="sm"
          onclick={() => { resetUnitForm(); showCreateUnitModal = true; }}
          title="Create a new systemd service unit file"
        >
          <Plus size={14} />
          <span>New Service</span>
        </Button>
      </div>
    </div>
  {:else if mainTab === 'autostart'}
    <div class="header-row">
      <SearchBar 
        bind:value={autostartFilter} 
        count={filteredAutostart.length} 
        total={autostartEntries.length} 
        placeholder="Filter autostart entries…" 
        style="min-width:260px; max-width:380px; margin:0;" 
      />
      <div class="header-spacer"></div>
      <Button
        variant="primary"
        size="sm"
        onclick={openAddAutostartModal}
        title="Add new autostart application"
      >
        <Plus size={14} />
        <span>Add Autostart</span>
      </Button>
    </div>
  {:else if mainTab === 'boot_analyzer'}
    <div class="header-row">
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
    </div>
  {/if}

  {#if mainTab === 'services'}
    <!-- Side panel: Logs, Editor, or Dependencies -->
    <SideDrawer
      bind:isOpen={panelOpen}
      title={activePanel === 'logs' ? `Logs — ${selectedUnit?.name}` : (activePanel === 'dependencies' ? `Dependencies — ${selectedUnit?.name}` : `Unit — ${selectedUnit?.name}`)}
      width="640px"
    >
      {#snippet headerActions()}
        {#if activePanel === 'logs' && selectedUnit}
          <Button variant="outline" class="btn-sm" onclick={() => uiStore.jumpToJournalService(selectedUnit!.name)}>
            <FileText size={13} /> Full Journal ↗
          </Button>
        {:else if activePanel === 'editor'}
          {#if editorTab === 'override'}
            <Button variant="outline" class="btn-sm" onclick={retryDaemonReload}>
              Retry daemon-reload
            </Button>
            <Button variant="primary" class="btn-sm" onclick={confirmSaveUnitFile}
              disabled={saving || editedContent === unitFileContent}>
              {saving ? 'Saving…' : 'Save Override'}
            </Button>
          {:else}
            <span class="badge badge-muted">Read-only</span>
          {/if}
        {/if}
      {/snippet}

      {#if selectedUnit?.is_protected}
        <div class="drawer-protection-alert {selectedUnit.protection_level}">
          {#if selectedUnit.protection_level === 'critical'}
            <ShieldAlert size={18} class="protection-alert-icon critical" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-error); font-size: 12.5px;">Critical Operating System Core</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{selectedUnit.protection_reason || 'This unit is critical for operating system integrity. Masking, disabling, or stopping is strictly locked.'}</span>
            </div>
          {:else}
            <ShieldCheck size={18} class="protection-alert-icon essential" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-accent); font-size: 12.5px;">Protected Infrastructure Service</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{selectedUnit.protection_reason || 'This unit provides essential security, network, or display services. Guarded against accidental masking or disablement.'}</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if activePanel === 'logs'}
        {#if logsLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading logs…
          </div>
        {:else}
          <pre class="log-output">{logs || 'No log output found.'}</pre>
        {/if}
      {:else if activePanel === 'dependencies'}
        {#if depsLoading}
          <div style="padding:24px;color:var(--color-text-muted);display:flex;align-items:center;justify-content:center;gap:8px">
            <RefreshCw size={18} class="animate-spin-slow" /> Querying systemd dependency tree…
          </div>
        {:else if depsError}
          <div class="empty-state" role="alert" style="padding:32px; color:var(--color-error);">{depsError}</div>
        {:else if unitDeps}
          <div style="display:flex; flex-direction:column; gap:16px; padding:4px 0;">
            <!-- Requires -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ShieldCheck size={14} style="color:var(--color-error);" /> Requires (Hard Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.requires.length}</span>
              </div>
              {#if unitDeps.requires.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.requires as req}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{req}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Wants -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <Activity size={14} style="color:var(--color-warning);" /> Wants (Weak Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.wants.length}</span>
              </div>
              {#if unitDeps.wants.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.wants as want}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{want}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- After -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-accent);" /> Starts After (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.after.length}</span>
              </div>
              {#if unitDeps.after.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.after as aft}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{aft}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Before -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-success);" /> Starts Before (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.before.length}</span>
              </div>
              {#if unitDeps.before.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.before as bef}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{bef}</code>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {:else if activePanel === 'editor'}
        <div style="display:flex; flex-direction:column; height:100%; min-height:0; gap:10px;">
          <TabGroup
            tabs={[
              { id: 'override', label: 'Editable Override' },
              { id: 'effective', label: 'Effective Unit (read-only)' },
            ]}
            bind:activeTab={editorTab}
            size="sm"
            onchange={(tab) => {
              if (tab === 'effective' && effectiveUnitContent === '' && !effectiveUnitError) {
                loadEffectiveUnit();
              }
            }}
          />

          {#if editorTab === 'override'}
            {#if unitFileLoading}
              <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
                <RefreshCw size={14} class="animate-spin-slow" /> Loading override…
              </div>
            {:else}
              <div style="display:flex; flex-direction:column; flex:1; min-height:0;">
                <CodeEditor value={unitFileContent} height="100%" onchange={(v) => editedContent = v} />
              </div>
            {/if}
          {:else if effectiveUnitLoading}
            <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
              <RefreshCw size={14} class="animate-spin-slow" /> Loading effective unit…
            </div>
          {:else if effectiveUnitError}
            <div class="empty-state" role="alert" style="padding:24px; color:var(--color-error); gap:10px;">
              <span>{effectiveUnitError}</span>
              <Button variant="outline" size="sm" onclick={loadEffectiveUnit}>Retry</Button>
            </div>
          {:else}
            <pre class="log-output" style="flex:1; min-height:0;">{effectiveUnitContent || 'No unit content returned.'}</pre>
          {/if}
        </div>
      {/if}
    </SideDrawer>

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
                      onclick={() => openEditAutostartModal(entry)}
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

<!-- MODAL: ADD / EDIT AUTOSTART APPLICATION -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showAutostartModal}
  <div use:portal class="modal-backdrop" onclick={() => { if (!autostartSaving) showAutostartModal = false; }} role="presentation">
    <div class="modal-card" style="max-width: 520px;" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-modal="true">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <Rocket size={18} class="text-accent" />
          <h3>{isEditingAutostart ? 'Edit Autostart Application' : 'Add Autostart Application'}</h3>
        </div>
        <button class="modal-close-btn" onclick={() => { if (!autostartSaving) showAutostartModal = false; }}>×</button>
      </div>

      <div class="modal-body">
        <!-- Application Name -->
        <div class="form-group">
          <label for="autostart-name-input" class="form-label">
            Application Name <span class="text-rose">*</span>
          </label>
          <input
            id="autostart-name-input"
            type="text"
            placeholder="e.g. Discord or Custom Script"
            bind:value={autostartForm.name}
            oninput={() => autostartFormTouched.name = true}
            class="form-input"
            class:input-error={autostartFormTouched.name && autostartFormErrors.name}
          />
          {#if autostartFormTouched.name && autostartFormErrors.name}
            <span class="form-error-msg">{autostartFormErrors.name}</span>
          {/if}
        </div>

        <!-- Command / Executable -->
        <div class="form-group">
          <label for="autostart-exec-input" class="form-label">
            Command / Executable <span class="text-rose">*</span>
          </label>
          <div class="input-browse-wrap">
            <input
              id="autostart-exec-input"
              type="text"
              placeholder="e.g. /usr/bin/discord or custom-daemon.sh"
              bind:value={autostartForm.exec}
              oninput={() => autostartFormTouched.exec = true}
              class="form-input font-mono"
              class:input-error={autostartFormTouched.exec && autostartFormErrors.exec}
            />
            <Button
              variant="outline"
              size="sm"
              onclick={browseAutostartExecBinary}
              title="Browse for executable binary or script"
              type="button"
            >
              <Folder size={13} />
              <span>Browse…</span>
            </Button>
          </div>
          {#if autostartFormTouched.exec && autostartFormErrors.exec}
            <span class="form-error-msg">{autostartFormErrors.exec}</span>
          {:else}
            <span class="form-help">Absolute path to executable binary, script, or command</span>
          {/if}
        </div>

        <!-- Comment / Description -->
        <div class="form-group">
          <label for="autostart-comment-input" class="form-label">
            Comment / Description <span class="text-muted" style="font-weight: normal;">(Optional)</span>
          </label>
          <input
            id="autostart-comment-input"
            type="text"
            placeholder="e.g. Automatically launch on session login"
            bind:value={autostartForm.comment}
            class="form-input"
            class:input-error={autostartFormErrors.comment}
          />
          {#if autostartFormErrors.comment}
            <span class="form-error-msg">{autostartFormErrors.comment}</span>
          {/if}
        </div>

        <!-- Enable Autostart Toggle -->
        <div style="display:flex; align-items:center; justify-content:space-between; padding:10px 12px; background:var(--color-bg-surface); border:1px solid var(--color-border-subtle); border-radius:8px; margin-top:4px;">
          <div style="display:flex; flex-direction:column; gap:2px;">
            <span style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Enable Autostart</span>
            <span style="font-size:11px; color:var(--color-text-muted);">Launch this application automatically when you log in</span>
          </div>
          <button
            type="button"
            class="ui-toggle"
            class:on={autostartForm.enabled}
            onclick={() => autostartForm.enabled = !autostartForm.enabled}
            aria-checked={autostartForm.enabled}
            role="switch"
            aria-label="Enable autostart"
          >
            <span class="ui-toggle-thumb"></span>
          </button>
        </div>
      </div>

      <div class="modal-footer">
        <Button
          variant="outline"
          onclick={() => showAutostartModal = false}
          disabled={autostartSaving}
        >
          Cancel
        </Button>
        <Button
          variant="primary"
          onclick={saveAutostart}
          disabled={!isAutostartFormValid || autostartSaving}
          loading={autostartSaving}
        >
          <Check size={14} />
          <span>{isEditingAutostart ? 'Save Changes' : 'Add Application'}</span>
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: NEW SYSTEMD UNIT FILE WIZARD -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showCreateUnitModal}
  <div use:portal class="modal-backdrop" onclick={() => { if (!isSubmittingUnit) showCreateUnitModal = false; }} role="presentation">
    <div class="modal-card modal-wizard-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-modal="true">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <FileCode size={18} class="text-accent" />
          <h3>Create Systemd Service Unit</h3>
        </div>
        <button class="modal-close-btn" onclick={() => { if (!isSubmittingUnit) showCreateUnitModal = false; }}>×</button>
      </div>

      <!-- Stepper Progress Bar -->
      <Stepper
        bind:currentStep={createUnitStep}
        allowClick={false}
        steps={[
          { label: 'Unit Identity' },
          { label: 'Lifecycle & Exec' },
          { label: 'Dependencies' },
          { label: 'Pre-flight Preview' }
        ]}
        onchange={(s) => {
          if (s > 1 && !unitForm.name.trim()) {
            uiStore.addToast('Please specify a unit name first', 'warning');
            createUnitStep = 1;
          }
        }}
      />

      <div class="modal-body wizard-body">
        {#if createUnitStep === 1}
          <!-- ─── STEP 1: UNIT IDENTITY & EXECUTABLE ─── -->
          <div class="step-pane">
            <div class="form-row">
              <div class="form-group">
                <label for="unit-name-input" class="form-label">Service Unit Name <span class="text-rose">*</span></label>
                <input
                  id="unit-name-input"
                  type="text"
                  placeholder="e.g. api-service or worker"
                  bind:value={unitForm.name}
                  oninput={() => unitFormTouched.name = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.name && unitFormErrors.name}
                />
                {#if unitFormTouched.name && unitFormErrors.name}
                  <small class="form-error-msg">{unitFormErrors.name}</small>
                {:else}
                  <small class="form-help">Will be saved as <code>{unitForm.name.trim() ? (unitForm.name.endsWith('.service') ? unitForm.name : unitForm.name + '.service') : 'name.service'}</code></small>
                {/if}
              </div>

              <div class="form-group">
                <label for="unit-desc-input" class="form-label">Description</label>
                <input
                  id="unit-desc-input"
                  type="text"
                  placeholder="e.g. My Production Backend Daemon"
                  bind:value={unitForm.description}
                  class="form-input"
                />
                <small class="form-help">Readable summary displayed in systemctl status</small>
              </div>
            </div>

            <div class="form-group">
              <label for="unit-exec-input" class="form-label">ExecStart Command <span class="text-rose">*</span></label>
              <div class="input-browse-wrap">
                <input
                  id="unit-exec-input"
                  type="text"
                  placeholder="/usr/bin/node /var/www/app/dist/main.js or /usr/bin/python3 main.py"
                  bind:value={unitForm.execStart}
                  oninput={() => unitFormTouched.execStart = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.execStart && unitFormErrors.execStart}
                />
                <Button variant="outline" onclick={browseExecStartBinary} title="Browse for executable file">
                  <Folder size={14} class="text-accent" />
                  <span>Browse...</span>
                </Button>
              </div>
              {#if unitFormTouched.execStart && unitFormErrors.execStart}
                <small class="form-error-msg">{unitFormErrors.execStart}</small>
              {:else}
                <small class="form-help">Absolute binary path with launch arguments</small>
              {/if}
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="unit-cwd-input" class="form-label">Working Directory (WorkingDirectory)</label>
                <div class="input-browse-wrap">
                  <input
                    id="unit-cwd-input"
                    type="text"
                    placeholder="/var/www/my-app"
                    bind:value={unitForm.workingDir}
                    oninput={() => unitFormTouched.workingDir = true}
                    class="form-input font-mono"
                    class:input-error={unitFormTouched.workingDir && unitFormErrors.workingDir}
                  />
                  <Button variant="outline" onclick={browseUnitWorkingDir} title="Browse working directory">
                    <Folder size={14} />
                  </Button>
                </div>
                {#if unitFormTouched.workingDir && unitFormErrors.workingDir}
                  <small class="form-error-msg">{unitFormErrors.workingDir}</small>
                {:else}
                  <small class="form-help">Working folder for the daemon process</small>
                {/if}
              </div>

              <div class="form-group">
                <label for="unit-user-input" class="form-label">Execution User & Group</label>
                <div style="display:grid; grid-template-columns:1fr 1fr; gap:8px;">
                  <input
                    id="unit-user-input"
                    type="text"
                    placeholder="User (e.g. root, node)"
                    bind:value={unitForm.user}
                    disabled={userScope}
                    oninput={() => unitFormTouched.user = true}
                    class="form-input"
                    class:input-error={unitFormTouched.user && unitFormErrors.user}
                  />
                  <input
                    type="text"
                    placeholder="Group (e.g. root, node)"
                    bind:value={unitForm.group}
                    disabled={userScope}
                    class="form-input"
                  />
                </div>
                {#if unitFormTouched.user && unitFormErrors.user}
                  <small class="form-error-msg">{unitFormErrors.user}</small>
                {:else}
                  <small class="form-help">{userScope ? 'User mode inherits current user' : 'Run daemon under non-root account for security'}</small>
                {/if}
              </div>
            </div>
          </div>

        {:else if createUnitStep === 2}
          <!-- ─── STEP 2: LIFECYCLE & EXECUTION ─── -->
          <div class="step-pane">
            <div class="form-row">
              <div class="form-group">
                <label for="unit-type-select" class="form-label">Service Type (Type=)</label>
                <Select id="unit-type-select" bind:value={unitForm.type}>
                  <option value="simple">simple — Standard daemon process (Default)</option>
                  <option value="forking">forking — Process calls fork() (e.g. traditional daemons)</option>
                  <option value="oneshot">oneshot — Short-lived batch script/task</option>
                  <option value="notify">notify — Daemon sends readiness signal with sd_notify()</option>
                </Select>
              </div>

              <div class="form-group">
                <label for="unit-restart-select" class="form-label">Restart Policy (Restart=)</label>
                <Select id="unit-restart-select" bind:value={unitForm.restartPolicy}>
                  <option value="on-failure">on-failure — Restart only on crash / non-zero exit (Recommended)</option>
                  <option value="always">always — Always restart unconditionally</option>
                  <option value="on-abort">on-abort — Restart on uncaught signal / abort</option>
                  <option value="no">no — Never restart automatically</option>
                </Select>
              </div>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="unit-restart-sec" class="form-label">Restart Delay (RestartSec=)</label>
                <input
                  id="unit-restart-sec"
                  type="text"
                  placeholder="5s"
                  bind:value={unitForm.restartSec}
                  oninput={() => unitFormTouched.restartSec = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.restartSec && unitFormErrors.restartSec}
                />
                {#if unitFormTouched.restartSec && unitFormErrors.restartSec}
                  <small class="form-error-msg">{unitFormErrors.restartSec}</small>
                {/if}
              </div>
              <div class="form-group">
                <label for="unit-timeout-sec" class="form-label">Stop Timeout (TimeoutSec=)</label>
                <input
                  id="unit-timeout-sec"
                  type="text"
                  placeholder="30s"
                  bind:value={unitForm.timeoutSec}
                  oninput={() => unitFormTouched.timeoutSec = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.timeoutSec && unitFormErrors.timeoutSec}
                />
                {#if unitFormTouched.timeoutSec && unitFormErrors.timeoutSec}
                  <small class="form-error-msg">{unitFormErrors.timeoutSec}</small>
                {/if}
              </div>
            </div>

            <!-- Environment Variables list -->
            <div class="form-group">
              <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:4px;">
                <span class="form-label" style="margin:0;">Environment Variables</span>
                <button
                  type="button"
                  class="btn-text"
                  onclick={() => unitForm.envVars = [...unitForm.envVars, { key: '', value: '' }]}
                  style="font-size:11.5px; color:var(--color-accent); display:flex; align-items:center; gap:4px; background:transparent; border:none; cursor:pointer;"
                >
                  <Plus size={12} /> Add Variable
                </button>
              </div>
              {#if unitFormErrors.envVars}
                <small class="form-error-msg" style="margin-bottom:6px;">{unitFormErrors.envVars}</small>
              {/if}
              <div style="display:flex; flex-direction:column; gap:6px; max-height:130px; overflow-y:auto; padding-right:2px;">
                {#each unitForm.envVars as ev, idx}
                  <div style="display:grid; grid-template-columns:1fr 1fr 28px; gap:8px; align-items:center;">
                    <input type="text" placeholder="KEY (e.g. PORT)" bind:value={ev.key} class="form-input font-mono" />
                    <input type="text" placeholder="VALUE (e.g. 3000)" bind:value={ev.value} class="form-input font-mono" />
                    <button
                      type="button"
                      onclick={() => unitForm.envVars = unitForm.envVars.filter((_, i) => i !== idx)}
                      style="background:transparent; border:none; color:var(--color-error); cursor:pointer; display:flex; align-items:center; justify-content:center;"
                      title="Remove variable"
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                {/each}
              </div>
            </div>
          </div>

        {:else if createUnitStep === 3}
          <!-- ─── STEP 3: DEPENDENCIES & TARGETS ─── -->
          <div class="step-pane">
            <div class="form-group">
              <span class="form-label">Startup Ordering (After=)</span>
              <div class="preset-pill-row">
                {#each ['network.target', 'network-online.target', 'docker.service', 'nginx.service', 'postgresql.service', 'mysqld.service', 'redis.service'] as target}
                  <button
                    type="button"
                    class="preset-pill"
                    class:active-pill={unitForm.afterTargets.includes(target)}
                    onclick={() => {
                      if (unitForm.afterTargets.includes(target)) {
                        unitForm.afterTargets = unitForm.afterTargets.filter(t => t !== target);
                      } else {
                        unitForm.afterTargets = [...unitForm.afterTargets, target];
                      }
                    }}
                  >
                    {target}
                  </button>
                {/each}
              </div>
              <small class="form-help">Selected services must finish starting before this unit begins execution</small>
            </div>

            <div class="form-group">
              <span class="form-label">Soft Dependencies (Wants=)</span>
              <div class="preset-pill-row">
                {#each ['network-online.target', 'docker.service', 'postgresql.service', 'redis.service'] as target}
                  <button
                    type="button"
                    class="preset-pill"
                    class:active-pill={unitForm.wantsTargets.includes(target)}
                    onclick={() => {
                      if (unitForm.wantsTargets.includes(target)) {
                        unitForm.wantsTargets = unitForm.wantsTargets.filter(t => t !== target);
                      } else {
                        unitForm.wantsTargets = [...unitForm.wantsTargets, target];
                      }
                    }}
                  >
                    {target}
                  </button>
                {/each}
              </div>
              <small class="form-help">Services started alongside this unit without hard failure dependencies</small>
            </div>

            <div class="form-group">
              <label for="wanted-by-select" class="form-label">Install Target (WantedBy=)</label>
              <Select id="wanted-by-select" bind:value={unitForm.wantedBy}>
                <option value="multi-user.target">multi-user.target — Normal boot / non-graphical runlevel (Standard)</option>
                <option value="graphical.target">graphical.target — Full graphical desktop environment</option>
                <option value="default.target">default.target — Current system default target</option>
              </Select>
            </div>
          </div>

        {:else if createUnitStep === 4}
          <!-- ─── STEP 4: PRE-FLIGHT SYNTAX PREVIEW ─── -->
          <div class="step-pane">
            <div style="display:flex; align-items:center; justify-content:space-between;">
              <span style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Generated Systemd Unit File Syntax</span>
              <button
                type="button"
                class="btn-copy-syntax"
                onclick={() => {
                  navigator.clipboard.writeText(generatedUnitContent);
                  copiedUnitCode = true;
                  setTimeout(() => copiedUnitCode = false, 1800);
                }}
              >
                {#if copiedUnitCode}
                  <Check size={12} style="color:var(--color-success);" />
                  <span>Copied!</span>
                {:else}
                  <Copy size={12} />
                  <span>Copy Syntax</span>
                {/if}
              </button>
            </div>

            <div class="terminal-preview-card font-mono">
              <pre style="margin:0; font-size:11.5px; line-height:1.5; color:#f1f5f9; white-space:pre-wrap; word-break:break-all;">{generatedUnitContent}</pre>
            </div>

            <div class="step-info-card">
              <Sparkles size={16} class="text-accent flex-shrink-0" />
              <div class="text-xs text-secondary">
                This unit file will be written to <code>{userScope ? '~/.config/systemd/user/' : '/etc/systemd/system/'}{unitForm.name.endsWith('.service') ? unitForm.name : unitForm.name + '.service'}</code> and systemd will automatically reload daemon.
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        {#if createUnitStep > 1}
          <Button variant="outline" onclick={() => createUnitStep--} disabled={isSubmittingUnit}>Back</Button>
        {:else}
          <Button variant="outline" onclick={() => showCreateUnitModal = false} disabled={isSubmittingUnit}>Cancel</Button>
        {/if}

        {#if createUnitStep < 4}
          <Button variant="primary" onclick={() => {
            if (createUnitStep === 1) {
              unitFormTouched.name = true;
              unitFormTouched.execStart = true;
              unitFormTouched.workingDir = true;
              unitFormTouched.user = true;
              if (!isStep1Valid) {
                uiStore.addToast(unitFormErrors.name || unitFormErrors.execStart || unitFormErrors.workingDir || 'Please fix step errors', 'warning');
                return;
              }
            } else if (createUnitStep === 2) {
              unitFormTouched.restartSec = true;
              unitFormTouched.timeoutSec = true;
              if (!isStep2Valid) {
                uiStore.addToast(unitFormErrors.restartSec || unitFormErrors.timeoutSec || unitFormErrors.envVars || 'Please fix step errors', 'warning');
                return;
              }
            }
            createUnitStep++;
          }}>
            <span>Next</span>
            <ChevronRight size={13} />
          </Button>
        {:else}
          <Button variant="outline" onclick={() => handleCreateUnit(false)} disabled={isSubmittingUnit}>
            <FileCode size={13} />
            <span>Create Unit File</span>
          </Button>
          <Button variant="primary" onclick={() => handleCreateUnit(true)} disabled={isSubmittingUnit}>
            <Play size={13} />
            <span>Create &amp; Start</span>
          </Button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>/* ── Header row (tabs + stats + search) ─────────────────────────────── */ .header-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }.header-spacer {
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
  }.ui-toggle.on .ui-toggle-thumb { transform: translateX(16px); }/* Service tab */ .log-output {
    font-size: 11px;
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    margin: 0;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: rgba(0,0,0,0.2);
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-all;
  }.selected-unit {
    background: var(--color-active-bg) !important;
  }/* Header controls optimization */ .service-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }.scope-slot {
    display: flex;
    align-items: center;
  }.scope-slot.is-services {
    min-width: 104px;
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
  }.drawer-protection-alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 8px;
    margin-bottom: 16px;
  }.drawer-protection-alert.critical {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.25);
  }.drawer-protection-alert.essential {
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
  }.protection-alert-icon.critical {
    color: var(--color-error);
    margin-top: 2px;
    flex-shrink: 0;
  }.protection-alert-icon.essential {
    color: var(--color-accent);
    margin-top: 2px;
    flex-shrink: 0;
  }.protection-alert-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    line-height: 1.4;
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
  }.header-refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    height: 28px;
    border-radius: var(--radius-md, 6px);
    font-size: 11px;
    font-family: var(--font-sans);
    font-weight: 600;
    color: var(--color-text-secondary);
    background: transparent;
    border: 1px solid var(--color-border);
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }.header-refresh-btn:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
    border-color: var(--color-border-hover);
  }.header-refresh-btn:disabled, .header-refresh-btn.refreshing {
    cursor: not-allowed !important;
    opacity: 0.8 !important;
    background: var(--color-accent-muted) !important;
    border-color: var(--color-accent-glow) !important;
    color: var(--color-accent) !important;
    box-shadow: 0 0 16px var(--color-accent-glow) !important;
    animation: cyber-pulse-glow 1.1s ease-in-out infinite alternate !important;
  }
@keyframes cyber-pulse-glow {0% {
      box-shadow: 0 0 4px var(--color-accent-glow);
      border-color: var(--color-accent-glow);
      transform: scale(0.98);
    }100% {
      box-shadow: 0 0 18px var(--color-accent-glow);
      border-color: var(--color-accent);
      transform: scale(1);
    }
}.scope-toggle-btn:focus-visible, .header-tab-btn:focus-visible,
  .header-refresh-btn:focus-visible, .trigger-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
@media (max-width: 1100px) {
    .service-header-actions {
      flex-wrap: wrap;
      justify-content: flex-end;
    }
  }.spin-refresh {
    animation: spin-cw 0.75s linear infinite !important;
    color: var(--color-accent) !important;
  }
@keyframes spin-cw {from { transform: rotate(0deg); }to { transform: rotate(360deg); }
}/* ── Service Creation Wizard Modal Styles ── */ .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 99990;
    padding: 20px;
  }.modal-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    width: 100%;
    max-width: 580px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
    position: relative;
  }.modal-wizard-card {
    width: 920px;
    max-width: calc(100vw - 40px);
    height: 620px;
    max-height: 92vh;
  }.modal-header {
    padding: 14px 20px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border-subtle);
    border-top-left-radius: 13px;
    border-top-right-radius: 13px;
  }.modal-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }.modal-title-wrap h3 {
    font-size: 15px;
    font-weight: 600;
    margin: 0;
  }.modal-close-btn {
    background: transparent;
    border: none;
    font-size: 20px;
    color: var(--color-text-muted);
    cursor: pointer;
  }.modal-body {
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }.wizard-body {
    flex: 1;
    min-height: 0;
    padding: 18px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }.step-pane {
    display: flex;
    flex-direction: column;
    gap: 13px;
    flex: 1;
  }.step-info-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    margin-top: auto;
  }.form-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }.form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }.form-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }.form-input {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12.5px;
    color: var(--color-text-primary);
    font-family: inherit;
    outline: none;
  }.form-input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-accent-muted);
  }.form-input.input-error {
    border-color: var(--color-error, #f43f5e) !important;
    box-shadow: 0 0 0 2px rgba(244, 63, 94, 0.18) !important;
  }.form-error-msg {
    font-size: 11px;
    font-weight: 500;
    color: var(--color-error, #f43f5e);
    margin-top: 2px;
  }.form-help {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }.input-browse-wrap {
    display: flex;
    gap: 8px;
    align-items: center;
  }.input-browse-wrap .form-input {
    flex: 1;
    min-width: 0;
  }.preset-pill-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }.preset-pill {
    padding: 3px 9px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 500;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
    cursor: pointer;
    transition: all 0.12s ease;
  }.preset-pill:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }.preset-pill.active-pill {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    color: var(--color-accent, #00daf3);
    border-color: var(--color-accent, #00daf3);
    font-weight: 600;
  }.terminal-preview-card {
    background: #08111e;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 14px;
    max-height: 250px;
    overflow-y: auto;
  }.btn-copy-syntax {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text-secondary);
    cursor: pointer;
  }.btn-copy-syntax:hover {
    color: var(--color-text-primary);
  }.modal-footer {
    padding: 12px 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
    border-bottom-left-radius: 13px;
    border-bottom-right-radius: 13px;
  }:global(html.light-mode) .modal-wizard-card {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.15);
  }:global(html.light-mode) .modal-header, :global(html.light-mode) .modal-footer {
    background: #f8fafc;
    border-color: #e2e8f0;
  }:global(html.light-mode) .terminal-preview-card {
    background: #f8fafc;
    border-color: #cbd5e1;
  }:global(html.light-mode) .terminal-preview-card pre {
    color: #0f172a !important;
  }
  .text-rose { color: var(--color-error); }
  .text-secondary { color: var(--color-text-secondary); }
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
