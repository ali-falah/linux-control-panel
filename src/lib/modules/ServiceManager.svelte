<script lang="ts">
  import { onMount } from 'svelte';
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
  import Select from '../components/ui/Select.svelte';
  import { portal } from '../actions/portal.ts';
  import { open } from '@tauri-apps/plugin-dialog';
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
        lines.push(`Environment="${ev.key.trim()}=${ev.value.trim()}"`);
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

  async function handleCreateUnit(startImmediately = false) {
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

    isSubmittingUnit = true;
    try {
      await invoke('write_unit_file', {
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
    if (uiStore.targetSubTab === 'boot_analyzer' || uiStore.targetSubTab === 'autostart' || uiStore.targetSubTab === 'services') {
      mainTab = uiStore.targetSubTab;
    }
    if (uiStore.serviceSearchQuery !== undefined && uiStore.serviceSearchQuery !== null) {
      filter = uiStore.serviceSearchQuery;
      if (uiStore.serviceSearchQuery !== '') {
        mainTab = 'services';
      }
    }
    if (uiStore.serviceFilter) {
      if (uiStore.serviceFilter === 'failed') statusFilter = 'failed';
      else if (uiStore.serviceFilter === 'active') statusFilter = 'active';
      else if (uiStore.serviceFilter === 'all') statusFilter = 'all';
      mainTab = 'services';
    }
  });

  let selectedUnit = $state<ServiceUnit | null>(null);
  let activePanel = $state<'logs' | 'editor' | 'dependencies' | null>(null);
  let panelOpen = $state(false);
  let logs = $state('');
  let logsLoading = $state(false);
  let unitFileContent = $state('');
  let unitFileLoading = $state(false);
  let actionInProgress = $state<string | null>(null);
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
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      show: true,
      unit
    };
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
  const blameTotalPages = $derived(Math.ceil(filteredBlame.length / blameItemsPerPage) || 1);
  const paginatedBlame = $derived(filteredBlame.slice((blamePage - 1) * blameItemsPerPage, blamePage * blameItemsPerPage));

  $effect(() => {
    blameSearch;
    blameFilter;
    blamePage = 1;
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
  const totalPages = $derived(Math.ceil(filteredUnits.length / itemsPerPage) || 1);
  const paginatedUnits = $derived(filteredUnits.slice((currentPage - 1) * itemsPerPage, currentPage * itemsPerPage));

  $effect(() => { filter; statusFilter; currentPage = 1; });

  function activeStateBadge(state: string): string {
    switch (state) {
      case 'active': return 'badge-success';
      case 'failed': return 'badge-error';
      case 'activating': case 'deactivating': return 'badge-warning';
      default: return 'badge-muted';
    }
  }

  async function load() {
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

  function confirmDoAction(unit: ServiceUnit, action: ServiceAction) {
    if (!userScope && (action === 'stop' || action === 'restart')) {
      uiStore.confirm(
        `Confirm ${action === 'stop' ? 'Stop' : 'Restart'}`,
        `Are you sure you want to ${action} ${unit.name}?\n\nWARNING: Modifying critical system services can cause system instability or loss of network connectivity.`,
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
    selectedUnit = unit;
    activePanel = 'logs';
    panelOpen = true;
    logsLoading = true;
    logs = '';
    try {
      logs = await invoke<string>('get_service_logs', { name: unit.name, lines: 100, userMode: userScope });
      statusStore.setLastCommand(`journalctl ${userScope ? '--user' : ''} -u ${unit.name} -n 100`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load logs: ${e}`, 'error');
      statusStore.setLastCommand(`journalctl ${userScope ? '--user' : ''} -u ${unit.name} -n 100`, 1, false);
    } finally {
      logsLoading = false;
    }
  }

  async function openEditor(unit: ServiceUnit) {
    selectedUnit = unit;
    activePanel = 'editor';
    panelOpen = true;
    unitFileLoading = true;
    unitFileContent = '';
    try {
      unitFileContent = await invoke<string>('read_unit_file', { name: unit.name, userMode: userScope });
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} cat ${unit.name}`, 0, true);
    } catch (e) {
      unitFileContent = `# Error reading unit file: ${e}`;
      editedContent = unitFileContent;
      statusStore.setLastCommand(`systemctl ${userScope ? '--user' : ''} cat ${unit.name}`, 1, false);
    } finally {
      unitFileLoading = false;
    }
  }

  async function openDependencies(unit: ServiceUnit) {
    selectedUnit = unit;
    activePanel = 'dependencies';
    panelOpen = true;
    depsLoading = true;
    unitDeps = null;
    try {
      unitDeps = await invoke<UnitDeps>('get_unit_dependencies', { name: unit.name, userMode: userScope });
      statusStore.setLastCommand(`systemctl show ${unit.name} --property=Requires,Wants,After,Before`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load dependencies: ${e}`, 'error');
    } finally {
      depsLoading = false;
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

    if (action === 'stop' || action === 'mask') {
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
    uiStore.addToast(`Executing ${action} on ${count} services…`, 'info');
    let successCount = 0;
    for (const name of names) {
      try {
        await invoke('service_action', { name, action, userMode: userScope });
        successCount++;
      } catch (e) {
        console.error(`Failed ${action} on ${name}:`, e);
      }
    }
    uiStore.addToast(`Completed ${action}: ${successCount}/${count} succeeded`, successCount === count ? 'success' : 'warning');
    selectedUnitNames = new Set();
    await load();
  }

  function confirmSaveUnitFile() {
    showUnitDiffModal = true;
  }

  async function saveUnitFile() {
    if (!selectedUnit) return;
    saving = true;
    try {
      await invoke('write_unit_file', { name: selectedUnit.name, content: editedContent, userMode: userScope });
      const targetPath = userScope ? `~/.config/systemd/user/${selectedUnit.name}` : `/etc/systemd/system/${selectedUnit.name}`;
      statusStore.setLastCommand(`echo "..." > ${targetPath} && systemctl ${userScope ? '--user' : ''} daemon-reload`, 0, true);
      uiStore.addToast(`Unit file saved for ${selectedUnit.name}`, 'success');
      unitFileContent = editedContent;
    } catch (e) {
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      const targetPath = userScope ? `~/.config/systemd/user/${selectedUnit.name}` : `/etc/systemd/system/${selectedUnit.name}`;
      statusStore.setLastCommand(`echo "..." > ${targetPath}`, 1, false);
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
    if (blameEntries.length === 0 || force) {
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
    }
  }

  async function openBootLogs(unitName: string) {
    bootLogsUnit = unitName;
    bootLogsOpen = true;
    bootLogsLoading = true;
    bootLogsContent = '';
    try {
      bootLogsContent = await invoke<string>('get_service_logs', { name: unitName, lines: 150, userMode: false });
      statusStore.setLastCommand(`journalctl -b -u ${unitName} -n 150`, 0, true);
    } catch (e) {
      bootLogsContent = `Failed to load logs: ${e}`;
    } finally {
      bootLogsLoading = false;
    }
  }

  function jumpToService(unitName: string) {
    mainTab = 'services';
    filter = unitName;
    const found = units.find(u => u.name === unitName);
    if (found) {
      selectedUnit = found;
    }
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
    if (!panelOpen) { activePanel = null; selectedUnit = null; }
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
  const autostartItemsPerPage = 30;
  const autostartTotalPages = $derived(Math.ceil(filteredAutostart.length / autostartItemsPerPage) || 1);
  const paginatedAutostart = $derived(filteredAutostart.slice((autostartCurrentPage - 1) * autostartItemsPerPage, autostartCurrentPage * autostartItemsPerPage));

  $effect(() => { autostartFilter; autostartCurrentPage = 1; });

  async function loadAutostart() {
    autostartLoading = true;
    statusStore.setBusy('Loading XDG autostart entries…');
    try {
      autostartEntries = await invoke<AutostartEntry[]>('list_autostart_entries');
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

  // ─── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    if (mainTab === 'services') {
      load();
    } else if (mainTab === 'autostart') {
      loadAutostart();
    } else if (mainTab === 'boot_analyzer') {
      loadBlame();
    }
  });
</script>

<div class="module-page">
  <PageHeader title="Service Manager" icon={Settings}>
    <div style="display:flex; align-items:center; gap:10px;">
      {#if mainTab === 'services'}
        <!-- Single Toggleable Scope Button -->
        <button 
          class="scope-toggle-btn"
          class:active={userScope}
          onclick={() => { userScope = !userScope; load(); }}
          title={userScope ? "Viewing User Services (--user). Click to switch to System Services." : "Viewing System Services. Click to switch to User Services."}
        >
          {#if userScope}
            <User size={12} />
            <span>User Scope</span>
          {:else}
            <Server size={12} />
            <span>System Scope</span>
          {/if}
        </button>
      {/if}

      <div class="header-tab-bar">
        <button class="header-tab-btn" class:active={mainTab === 'services'} onclick={() => mainTab = 'services'}>
          <Settings size={12} /> Services
          <span class="header-tab-count" class:active-count={mainTab === 'services'}>{units.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'autostart'} onclick={() => mainTab = 'autostart'}>
          <Rocket size={12} /> Autostart
          <span class="header-tab-count" class:active-count={mainTab === 'autostart'}>{autostartEntries.length}</span>
        </button>
        <button class="header-tab-btn" class:active={mainTab === 'boot_analyzer'} onclick={() => mainTab = 'boot_analyzer'}>
          <Activity size={12} /> Boot Analyzer
          {#if blameEntries.length > 0}
            <span class="header-tab-count" class:active-count={mainTab === 'boot_analyzer'}>{blameEntries.length}</span>
          {/if}
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
        <RefreshCw size={13} class={(isRefreshing || loading || autostartLoading || loadingBlame) ? 'spin-refresh' : ''} />
        <span>{(isRefreshing || loading || autostartLoading || loadingBlame) ? 'Refreshing…' : 'Refresh'}</span>
      </button>
    </div>
  </PageHeader>

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
          onclick={() => { showCreateUnitModal = true; createUnitStep = 1; }}
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
      title={activePanel === 'logs' ? `Logs — ${selectedUnit?.name}` : (activePanel === 'dependencies' ? `Dependencies — ${selectedUnit?.name}` : `Unit File — ${selectedUnit?.name}`)}
      width="640px"
    >
      {#snippet headerActions()}
        {#if activePanel === 'logs' && selectedUnit}
          <Button variant="outline" class="btn-sm" onclick={() => uiStore.jumpToJournalService(selectedUnit!.name)}>
            <FileText size={13} /> Full Journal ↗
          </Button>
        {:else if activePanel === 'editor'}
          <Button variant="primary" class="btn-sm" onclick={confirmSaveUnitFile}
            disabled={saving || editedContent === unitFileContent}>
            {saving ? 'Saving…' : 'Save Override'}
          </Button>
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
        {#if unitFileLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading unit file…
          </div>
        {:else}
          <div style="display:flex; flex-direction:column; height: 100%;">
            <CodeEditor value={unitFileContent} height="100%" onchange={(v) => editedContent = v} />
          </div>
        {/if}
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
        <Table tableAction={tableFeatures}>
          <thead>
            <tr>
              <th style="width: 40px; text-align: center;">
                <input
                  type="checkbox"
                  class="form-checkbox"
                  checked={paginatedUnits.length > 0 && selectedUnitNames.size === paginatedUnits.length}
                  onclick={toggleSelectAllUnits}
                />
              </th>
              <th>Service</th>
              <th>State</th>
              <th>Unit File</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each paginatedUnits as unit (unit.name)}
              <tr class:selected-unit={selectedUnit?.name === unit.name || selectedUnitNames.has(unit.name)} oncontextmenu={(e) => handleServiceContextMenu(e, unit)}>
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
                    <span class="badge {unit.unit_file_state === 'enabled' ? 'badge-success' : unit.unit_file_state === 'masked' ? 'badge-error' : 'badge-muted'}">
                      {unit.unit_file_state}
                    </span>
                  {:else}
                    <span class="badge badge-muted">—</span>
                  {/if}
                </td>
                <td style="width: 140px;">
                  <div style="display:flex;gap:8px; align-items:center">
                    {#if actionInProgress === `${unit.name}-start` || actionInProgress === `${unit.name}-stop` || actionInProgress === `${unit.name}-restart`}
                      <button class="action-btn" disabled title="Processing action…">
                        <RefreshCw size={13} class="animate-spin-slow" style="color:var(--color-accent)" />
                      </button>
                    {:else if unit.active_state !== 'active'}
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'start')} title="Start" disabled={!!actionInProgress}><Play size={14}/></button>
                    {:else}
                      <button 
                        class="action-btn" 
                        onclick={() => confirmDoAction(unit, 'stop')} 
                        title={unit.protection_level === 'critical' ? 'Cannot stop critical system unit' : 'Stop'} 
                        disabled={unit.protection_level === 'critical' || !!actionInProgress}
                        style={unit.protection_level === 'critical' ? 'opacity: 0.4; cursor: not-allowed;' : ''}
                      >
                        <Square size={14}/>
                      </button>
                      <button class="action-btn" onclick={() => confirmDoAction(unit, 'restart')} title="Restart" disabled={!!actionInProgress}><RotateCcw size={14}/></button>
                    {/if}
                    <KebabMenu align="right">
                      <button class="menu-item" onclick={() => confirmDoAction(unit, 'restart')} disabled={!!actionInProgress}>
                        <RotateCcw size={14} /> Restart
                      </button>
                      <button class="menu-item" onclick={() => openLogs(unit)}>
                        <FileText size={14} /> View Inline Logs
                      </button>
                      <button class="menu-item" onclick={() => uiStore.jumpToJournalService(unit.name)}>
                        <FileText size={14} /> Open in Journal Logs ↗
                      </button>
                      <button class="menu-item" onclick={() => openDependencies(unit)}>
                        <GitFork size={14} /> Inspect Dependencies
                      </button>
                      <button class="menu-item" onclick={() => openEditor(unit)}>
                        <Settings size={14} /> Edit Unit File
                      </button>
                      <div style="height:1px; background:var(--color-border); margin:4px 0;"></div>
                      {#if unit.unit_file_state !== 'enabled'}
                        <button 
                          class="menu-item" 
                          onclick={() => confirmDoAction(unit, 'enable')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be modified' : ''}
                        >
                          <ShieldCheck size={14} /> Enable (Autostart)
                        </button>
                      {:else}
                        <button 
                          class="menu-item text-error" 
                          onclick={() => confirmDoAction(unit, 'disable')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be disabled' : ''}
                        >
                          <ShieldBan size={14} /> Disable (Autostart)
                        </button>
                      {/if}
                      {#if unit.unit_file_state === 'masked'}
                        <button 
                          class="menu-item" 
                          onclick={() => confirmDoAction(unit, 'unmask')}
                          disabled={!!actionInProgress}
                        >
                          <Unlock size={14} /> Unmask Service
                        </button>
                      {:else}
                        <button 
                          class="menu-item text-error" 
                          onclick={() => confirmDoAction(unit, 'mask')}
                          disabled={unit.is_protected || !!actionInProgress}
                          title={unit.is_protected ? 'Protected system service cannot be masked' : ''}
                        >
                          <Lock size={14} /> Mask Service
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
        <Table tableAction={tableFeatures}>
          <thead>
            <tr>
              <th>Application</th>
              <th>Command</th>
              <th>Comment</th>
              <th style="text-align:center">Enabled</th>
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
              <Table tableAction={tableFeatures} style="overflow-y:auto; border:none; border-radius:0;">
                <thead>
                  <tr>
                    <th style="width:130px;">Startup Time</th>
                    <th>Unit / Service</th>
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
                          <span class="badge badge-danger">CRITICAL</span>
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
                <div style="display:flex; align-items:center; gap:8px; padding:6px 10px; border-radius:6px; background:rgba(0,0,0,0.15); border:1px solid var(--color-border);">
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
    subtitle={contextMenu.unit.description || 'Systemd Service Unit'}
    badge={{ 
      text: `${contextMenu.unit.active_state}${contextMenu.unit.is_protected ? ` · ${contextMenu.unit.protection_level}` : ''}`, 
      variant: contextMenu.unit.active_state === 'active' ? 'success' : (contextMenu.unit.active_state === 'failed' ? 'error' : 'muted') 
    }}
    icon={Settings}
    items={[
      {
        label: 'Restart Service',
        icon: RotateCcw,
        disabled: !!actionInProgress,
        action: () => confirmDoAction(contextMenu.unit!, 'restart')
      },
      contextMenu.unit.active_state !== 'active' ? {
        label: 'Start Service',
        icon: Play,
        disabled: !!actionInProgress,
        action: () => confirmDoAction(contextMenu.unit!, 'start')
      } : {
        label: contextMenu.unit.protection_level === 'critical' ? 'Stop Service (Locked)' : 'Stop Service',
        icon: Square,
        danger: true,
        disabled: contextMenu.unit.protection_level === 'critical' || !!actionInProgress,
        action: () => confirmDoAction(contextMenu.unit!, 'stop')
      },
      {
        label: contextMenu.unit.is_protected 
          ? 'Boot Autostart (Locked)' 
          : (contextMenu.unit.unit_file_state === 'enabled' ? 'Disable at Boot' : 'Enable at Boot'),
        icon: ShieldCheck,
        disabled: contextMenu.unit.is_protected || !!actionInProgress,
        action: () => {
          const nextAction = contextMenu.unit!.unit_file_state === 'enabled' ? 'disable' : 'enable';
          confirmDoAction(contextMenu.unit!, nextAction);
        }
      },
      { divider: true, label: '' },
      {
        label: 'View Inline Logs',
        icon: FileText,
        action: () => openLogs(contextMenu.unit!)
      },
      {
        label: 'Open in Journal Logs',
        icon: ArrowUpRight,
        action: () => {
          uiStore.jumpToJournalService(contextMenu.unit!.name);
          uiStore.setActiveTab('journal-logs');
        }
      },
      {
        label: 'Edit Unit File',
        icon: Edit3,
        action: () => openEditor(contextMenu.unit!)
      },
      {
        label: 'Inspect Dependencies',
        icon: GitFork,
        action: () => openDependencies(contextMenu.unit!)
      },
      { divider: true, label: '' },
      {
        label: contextMenu.unit.unit_file_state === 'masked' ? 'Unmask Service' : (contextMenu.unit.is_protected ? 'Mask Service (Locked)' : 'Mask Service Unit'),
        icon: contextMenu.unit.unit_file_state === 'masked' ? Unlock : Lock,
        danger: contextMenu.unit.unit_file_state !== 'masked',
        disabled: contextMenu.unit.is_protected || !!actionInProgress,
        action: () => {
          const maskAction = contextMenu.unit!.unit_file_state === 'masked' ? 'unmask' : 'mask';
          confirmDoAction(contextMenu.unit!, maskAction);
        }
      },
      {
        label: `Copy Unit Name (${contextMenu.unit.name})`,
        icon: Copy,
        action: () => {
          navigator.clipboard.writeText(contextMenu.unit!.name);
          uiStore.addToast(`Copied unit name: ${contextMenu.unit!.name}`, 'info');
        }
      }
    ]}
  />
{/if}

<!-- Bulk Action Bar for Systemd Services -->
<BulkActionBar
  selectedCount={selectedUnitNames.size}
  itemLabel="services"
  onclear={() => selectedUnitNames = new Set()}
>
  <button
    type="button"
    class="btn-bulk-action btn-bulk-primary"
    onclick={() => executeBulkServiceAction('restart')}
  >
    <RotateCcw size={12} />
    <span>Restart ({selectedUnitNames.size})</span>
  </button>
  <button
    type="button"
    class="btn-bulk-action btn-bulk-outline"
    onclick={() => executeBulkServiceAction('start')}
  >
    <Play size={12} />
    <span>Start</span>
  </button>
  <button
    type="button"
    class="btn-bulk-action btn-bulk-danger"
    onclick={() => executeBulkServiceAction('stop')}
  >
    <Square size={12} />
    <span>Stop</span>
  </button>
</BulkActionBar>

<!-- Universal Config Diff Modal for Systemd Unit Files -->
{#if selectedUnit}
  <ConfigDiffModal
    bind:show={showUnitDiffModal}
    filePath={userScope ? `~/.config/systemd/user/${selectedUnit.name}` : `/etc/systemd/system/${selectedUnit.name}`}
    title={`Review ${selectedUnit.name} Unit File Changes`}
    oldContent={unitFileContent}
    newContent={editedContent}
    warningMessage={selectedUnit.is_protected ? 'CAUTION: This is a protected system unit. Changes may affect operating system services.' : 'Ensure systemd directives and ExecStart paths are valid before saving.'}
    isSaving={saving}
    onconfirm={async () => {
      await saveUnitFile();
      showUnitDiffModal = false;
    }}
    oncancel={() => showUnitDiffModal = false}
  />
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: NEW SYSTEMD UNIT FILE WIZARD -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showCreateUnitModal}
  <div use:portal class="modal-backdrop" onclick={() => showCreateUnitModal = false} role="presentation">
    <div class="modal-card modal-wizard-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <FileCode size={18} class="text-accent" />
          <h3>Create Systemd Service Unit</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showCreateUnitModal = false}>×</button>
      </div>

      <!-- Stepper Progress Bar -->
      <Stepper
        bind:currentStep={createUnitStep}
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
                <label class="form-label" style="margin:0;">Environment Variables</label>
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
              <label class="form-label">Startup Ordering (After=)</label>
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
              <label class="form-label">Soft Dependencies (Wants=)</label>
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

<style>
  /* ── Header row (tabs + stats + search) ─────────────────────────────── */
  .header-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }

  .header-spacer {
    flex: 1;
  }



  /* ── Stats ───────────────────────────────────────────────────────────── */
  .stats-row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }

  /* ── Scope Toggle Button ───────────────────────────────────────────── */
  .scope-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    background: var(--color-bg-card, #FFFFFF);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
    transition: all 0.2s ease;
  }
  .scope-toggle-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.08);
  }
  .scope-toggle-btn.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 2px 8px rgba(37, 99, 235, 0.25) !important;
  }

  .stat-chip {
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
  }

  .stat-chip:hover {
    border-color: var(--color-border-hover);
    box-shadow: 0 4px 14px -2px rgba(0, 0, 0, 0.10);
  }

  .stat-chip.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.25) !important;
  }

  .stat-chip.active .stat-num,
  .stat-chip.active .stat-label {
    color: #FFFFFF !important;
    font-weight: 700;
  }

  .stat-num {
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1;
  }

  .stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 600;
  }

  /* ── Autostart tab layout ────────────────────────────────────────────── */
  .table-area {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .loading-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  /* Cell helpers */
  .col-name {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
    font-weight: 500;
  }
  .col-muted {
    color: var(--color-text-muted);
    font-size: 12px;
  }
  .col-subtext {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-top: 2px;
  }
  .col-code {
    font-size: 11px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  /* Toggle switch */
  .ui-toggle {
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
  }
  .ui-toggle.on { background: var(--color-accent); }
  .ui-toggle:disabled { opacity: 0.35; cursor: not-allowed; }

  .ui-toggle-thumb {
    position: absolute;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.2s ease;
    pointer-events: none;
  }
  .ui-toggle.on .ui-toggle-thumb { transform: translateX(16px); }

  /* Service tab */
  .log-output {
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
  }

  .selected-unit {
    background: var(--color-active-bg) !important;
  }

  /* Header controls optimization */
  .scope-toggle-btn {
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
  }
  .scope-toggle-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
  }
  .scope-toggle-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .header-tab-bar {
    display: inline-flex;
    align-items: center;
    background: var(--color-tab-bar-bg, rgba(0, 0, 0, 0.2));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 3px;
    gap: 3px;
  }
  .header-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    font-size: 11.5px;
    font-weight: 500;
    white-space: nowrap;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .header-tab-btn:hover {
    color: var(--color-text-primary);
  }
  .header-tab-btn.active {
    background: var(--color-accent);
    color: #FFFFFF;
    font-weight: 600;
  }
  .header-tab-count {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.15);
    color: inherit;
  }
  .header-tab-btn.active .header-tab-count {
    background: rgba(255, 255, 255, 0.25);
    color: #FFFFFF;
  }

  .filter-pills {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    padding: 3px;
    border-radius: 8px;
  }

  :global(html.light-mode) .filter-pills {
    background: #F1F5F9;
    border-color: #E2E8F0;
  }

  .pill-btn {
    border: none;
    background: transparent;
    padding: 4px 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pill-btn:hover {
    color: var(--color-text-primary);
  }

  .pill-btn.active {
    background: var(--color-accent);
    color: #000000;
    font-weight: 700;
  }

  :global(html.light-mode) .pill-btn.active {
    background: var(--color-accent);
    color: var(--color-text-on-accent, #FFFFFF);
  }

  /* ── Custom Context Menu ────────────────────────────────────────── */
  .custom-context-menu {
    background: var(--color-bg-card, #131b26);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: calc(100vh - 24px);
    overflow-y: auto;
    overflow-x: hidden;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .context-menu-item:hover:not(:disabled) {
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--color-text-primary);
  }

  .context-menu-item:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .context-menu-item.text-danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: var(--color-error);
  }

  /* ── Security Protection Badges & Alerts ───────────────────────────── */
  .protection-badge {
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
  }
  .protection-badge.critical {
    background: rgba(239, 68, 68, 0.14);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
  }
  .protection-badge.essential {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.3);
  }

  .drawer-protection-alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 8px;
    margin-bottom: 16px;
  }
  .drawer-protection-alert.critical {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.25);
  }
  .drawer-protection-alert.essential {
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
  }

  .protection-alert-icon.critical {
    color: var(--color-error);
    margin-top: 2px;
    flex-shrink: 0;
  }
  .protection-alert-icon.essential {
    color: var(--color-accent);
    margin-top: 2px;
    flex-shrink: 0;
  }

  .protection-alert-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    line-height: 1.4;
  }

  /* ── Cybernetic Pulse Loading Animation ───────────────────────────── */
  .cyber-loading-matrix {
    display: flex;
    flex-direction: column;
    padding: 16px;
    gap: 14px;
    height: 100%;
    min-height: 380px;
    background: var(--color-bg-base);
  }

  .cyber-scanner-hero {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 8px;
    background: linear-gradient(135deg, rgba(0, 218, 243, 0.07) 0%, rgba(37, 99, 235, 0.04) 100%);
    border: 1px solid rgba(0, 218, 243, 0.22);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.12), inset 0 0 16px rgba(0, 218, 243, 0.03);
  }

  .cyber-radar-orb {
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
  }

  .radar-sweep {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: conic-gradient(from 0deg, transparent 0deg, rgba(0, 218, 243, 0.3) 300deg, rgba(0, 218, 243, 0.75) 360deg);
    animation: radar-rotate 1.8s linear infinite;
  }

  @keyframes radar-rotate {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .radar-core-icon {
    position: relative;
    z-index: 2;
    color: var(--color-accent);
    filter: drop-shadow(0 0 4px var(--color-accent));
  }

  .cyber-scan-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .cyber-scan-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }

  .cyber-scan-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .cyber-skeleton-rows {
    display: flex;
    flex-direction: column;
    gap: 7px;
    flex: 1;
  }

  .cyber-shimmer-row {
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
  }

  .cyber-shimmer-row::after {
    content: '';
    position: absolute;
    top: 0;
    left: -150%;
    width: 150%;
    height: 100%;
    background: linear-gradient(90deg, transparent 0%, rgba(0, 218, 243, 0.09) 50%, transparent 100%);
    animation: cyber-shimmer 1.6s infinite ease-in-out;
  }

  @keyframes cyber-shimmer {
    0% { transform: translateX(0); }
    100% { transform: translateX(200%); }
  }

  .shimmer-col-name {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
  }

  .shimmer-pill-title {
    width: 140px;
    height: 13px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }

  .shimmer-pill-sub {
    width: 220px;
    height: 9px;
    border-radius: 3px;
    background: var(--color-bg-hover);
  }

  .shimmer-col-badge {
    width: 54px;
    height: 18px;
    border-radius: 999px;
    background: var(--color-bg-raised);
  }

  .shimmer-col-state {
    width: 50px;
    height: 16px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }

  .shimmer-col-actions {
    width: 65px;
    height: 24px;
    border-radius: 6px;
    background: var(--color-bg-raised);
  }

  .boot-kpi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
    margin-bottom: 2px;
  }

  .header-refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    height: 30px;
    border-radius: var(--radius-md, 6px);
    font-size: 12px;
    font-family: var(--font-sans);
    font-weight: 600;
    color: var(--color-text-secondary);
    background: transparent;
    border: 1px solid var(--color-border);
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .header-refresh-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-primary);
    border-color: rgba(255, 255, 255, 0.18);
  }

  .header-refresh-btn:disabled,
  .header-refresh-btn.refreshing {
    pointer-events: none !important;
    cursor: not-allowed !important;
    opacity: 0.8 !important;
    background: rgba(0, 218, 243, 0.1) !important;
    border-color: rgba(0, 218, 243, 0.45) !important;
    color: var(--color-accent) !important;
    box-shadow: 0 0 16px rgba(0, 218, 243, 0.3) !important;
    animation: cyber-pulse-glow 1.1s ease-in-out infinite alternate !important;
  }

  @keyframes cyber-pulse-glow {
    0% {
      box-shadow: 0 0 4px rgba(0, 218, 243, 0.2);
      border-color: rgba(0, 218, 243, 0.3);
      transform: scale(0.98);
    }
    100% {
      box-shadow: 0 0 18px rgba(0, 218, 243, 0.5);
      border-color: rgba(0, 218, 243, 0.85);
      transform: scale(1);
    }
  }

  .spin-refresh {
    animation: spin-cw 0.75s linear infinite !important;
    color: var(--color-accent) !important;
  }

  @keyframes spin-cw {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  /* ── Service Creation Wizard Modal Styles ── */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 99990;
    padding: 20px;
  }

  .modal-card {
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
  }

  .modal-wizard-card {
    width: 920px;
    max-width: calc(100vw - 40px);
    height: 620px;
    max-height: 92vh;
  }

  .modal-header {
    padding: 14px 20px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border-subtle);
    border-top-left-radius: 13px;
    border-top-right-radius: 13px;
  }

  .modal-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .modal-title-wrap h3 {
    font-size: 15px;
    font-weight: 600;
    margin: 0;
  }

  .modal-close-btn {
    background: transparent;
    border: none;
    font-size: 20px;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .modal-body {
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .wizard-body {
    flex: 1;
    min-height: 0;
    padding: 18px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .step-pane {
    display: flex;
    flex-direction: column;
    gap: 13px;
    flex: 1;
  }

  .step-info-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    margin-top: auto;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .form-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .form-input {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12.5px;
    color: var(--color-text-primary);
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-accent-muted);
  }

  .form-input.input-error {
    border-color: var(--color-error, #f43f5e) !important;
    box-shadow: 0 0 0 2px rgba(244, 63, 94, 0.18) !important;
  }

  .form-error-msg {
    font-size: 11px;
    font-weight: 500;
    color: var(--color-error, #f43f5e);
    margin-top: 2px;
  }

  .form-help {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  .input-browse-wrap {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .input-browse-wrap .form-input {
    flex: 1;
    min-width: 0;
  }

  .preset-pill-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }

  .preset-pill {
    padding: 3px 9px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 500;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .preset-pill:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }

  .preset-pill.active-pill {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    color: var(--color-accent, #00daf3);
    border-color: var(--color-accent, #00daf3);
    font-weight: 600;
  }

  .terminal-preview-card {
    background: #08111e;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 14px;
    max-height: 250px;
    overflow-y: auto;
  }

  .btn-copy-syntax {
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
  }

  .btn-copy-syntax:hover {
    color: var(--color-text-primary);
  }

  .modal-footer {
    padding: 12px 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
    border-bottom-left-radius: 13px;
    border-bottom-right-radius: 13px;
  }

  :global(html.light-mode) .modal-wizard-card {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.15);
  }

  :global(html.light-mode) .modal-header,
  :global(html.light-mode) .modal-footer {
    background: #f8fafc;
    border-color: #e2e8f0;
  }

  :global(html.light-mode) .terminal-preview-card {
    background: #f8fafc;
    border-color: #cbd5e1;
  }

  :global(html.light-mode) .terminal-preview-card pre {
    color: #0f172a !important;
  }
</style>
