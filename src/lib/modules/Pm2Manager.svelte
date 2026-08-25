<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    Layers, Play, Square, RotateCw, Trash2, Plus, Terminal, 
    FileCode, Settings, Cpu, HardDrive, Shield, AlertTriangle, 
    CheckCircle2, Copy, Check, Eye, RefreshCcw, Save, Zap,
    FileText, ArrowDown, Search, Folder, TerminalSquare, Info,
    Activity, Clock, ChevronRight, Hash, Box, KeyRound, Database, Sparkles
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Select from '../components/ui/Select.svelte';
  import Button from '../components/ui/Button.svelte';
  import Stepper from '../components/ui/Stepper.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import CodeEditor from '../components/CodeEditor.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Table from '../components/ui/Table.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';
  import { tableFeatures } from '../actions/tableFeatures.ts';
  import { portal } from '../actions/portal.ts';
  import { open } from '@tauri-apps/plugin-dialog';

  // Types
  interface Pm2SystemStatus {
    installed: boolean;
    version?: string;
    node_version?: string;
    npm_version?: string;
    pm2_home: string;
    daemon_running: boolean;
    executable_path?: string;
  }

  interface Pm2Process {
    pm_id: number;
    name: string;
    pid?: number;
    status: string;
    cpu: number;
    memory: number;
    uptime?: number;
    restarts: number;
    unstable_restarts: number;
    exec_mode: string;
    instances: number;
    watch: boolean;
    script_path: string;
    cwd: string;
    out_log_path: string;
    err_log_path: string;
    node_args: string[];
    args: string[];
    env_vars: Record<string, string>;
    version?: string;
    created_at?: number;
  }

  interface Pm2EcosystemFile {
    path: string;
    name: string;
    size: number;
    modified: string;
    app_names: string[];
  }

  interface Pm2StartupStatus {
    is_enabled: boolean;
    service_name?: string;
    user: string;
    startup_command_hint: string;
  }

  interface Pm2SavedDumpApp {
    name: string;
    script_path: string;
    cwd: string;
    exec_mode: string;
    instances: number;
    watch: boolean;
    max_memory_restart?: string;
    args: string[];
    env_vars: Record<string, string>;
    saved_status: string;
    is_currently_running: boolean;
    current_pm_id?: number;
    created_at?: number;
  }

  interface ParsedLogLine {
    lineNumber: number;
    raw: string;
    level: 'error' | 'warn' | 'info';
    timestamp?: string;
    message: string;
  }

  // Active Top Navigation Scope
  let activeTab = $state<'fleet' | 'ecosystem' | 'daemon'>('fleet');

  // Loading and State
  let loading = $state(true);
  let refreshing = $state(false);
  let showEcosystemDiffModal = $state(false);
  let systemStatus = $state<Pm2SystemStatus | null>(null);
  let processes = $state<Pm2Process[]>([]);
  let ecosystemFiles = $state<Pm2EcosystemFile[]>([]);
  let startupStatus = $state<Pm2StartupStatus | null>(null);
  let savedDumpApps = $state<Pm2SavedDumpApp[]>([]);

  // Master-Detail & SideDrawer Selection State
  let selectedProcessId = $state<number | null>(null);
  let showProcessDrawer = $state(false);
  let drawerTab = $state<'vitals' | 'env'>('vitals');
  let selectedInspectorTab = $state<'logs' | 'vitals' | 'env'>('logs');

  // Filters & Search
  let searchQuery = $state('');
  let statusFilter = $state<'all' | 'online' | 'stopped' | 'errored'>('all');
  let envSearchQuery = $state('');

  // Copy Feedback
  let copiedCommand = $state<string | null>(null);
  let copiedLogs = $state(false);
  let copiedField = $state<string | null>(null);

  // Launch App Modal & Stepper State
  let showLaunchModal = $state(false);
  let launchStep = $state<1 | 2 | 3>(1);
  let bulkEnvText = $state('');
  let showBulkEnvInput = $state(false);

  let launchForm = $state({
    script_path: '',
    name: '',
    cwd: '',
    exec_mode: 'fork',
    instances: 1,
    watch: false,
    max_memory_restart: '500M',
    args: '',
    env_vars: [{ key: 'NODE_ENV', value: 'production' }]
  });

  let detectedRuntime = $derived.by(() => {
    const p = launchForm.script_path.toLowerCase().trim();
    if (!p) return null;
    if (p.endsWith('.py')) return { label: 'Python Script', badge: '🐍 Python', type: 'python' };
    if (p.endsWith('.sh') || p.endsWith('.bash')) return { label: 'Shell Script', badge: '🐚 Shell', type: 'shell' };
    if (p.endsWith('.json') || p.includes('ecosystem')) return { label: 'PM2 Ecosystem', badge: '📦 Ecosystem', type: 'ecosystem' };
    if (p.endsWith('.ts') || p.endsWith('.tsx')) return { label: 'TypeScript App', badge: '🔷 TypeScript', type: 'ts' };
    if (p.endsWith('.js') || p.endsWith('.mjs') || p.endsWith('.cjs')) return { label: 'Node.js App', badge: '🟩 Node.js', type: 'node' };
    return { label: 'Generic Executable', badge: '⚙️ Binary / Script', type: 'generic' };
  });

  let livePm2Command = $derived.by(() => {
    let cmd = `pm2 start "${launchForm.script_path || '<script_path>'}"`;
    if (launchForm.name && launchForm.name.trim()) cmd += ` --name "${launchForm.name.trim()}"`;
    if (launchForm.cwd && launchForm.cwd.trim()) cmd += ` --cwd "${launchForm.cwd.trim()}"`;
    if (launchForm.exec_mode === 'cluster') {
      cmd += ` -i ${launchForm.instances || 'max'}`;
    }
    if (launchForm.max_memory_restart && launchForm.max_memory_restart.trim()) {
      cmd += ` --max-memory-restart ${launchForm.max_memory_restart.trim()}`;
    }
    if (launchForm.watch) cmd += ` --watch`;
    if (launchForm.args && launchForm.args.trim()) {
      cmd += ` -- ${launchForm.args.trim()}`;
    }
    return cmd;
  });

  // Ecosystem Editor
  let selectedEcosystemFile = $state<Pm2EcosystemFile | null>(null);
  let ecosystemContent = $state('');
  let originalEcosystemContent = $state('');
  let isEcosystemDirty = $derived(ecosystemContent !== originalEcosystemContent);
  let showNewEcosystemModal = $state(false);
  let newEcosystemPath = $state('');
  let newEcosystemTemplate = $state<'express' | 'nest' | 'next' | 'microservice'>('express');

  // Logs Stream State
  let selectedLogType = $state<'combined' | 'err'>('combined');
  let rawLogText = $state<string>('');
  let logSearchQuery = $state('');
  let logAutoRefresh = $state(true);
  let logRefreshInterval = $state(2); // seconds
  let logTailLines = $state('200');
  let logLoading = $state(false);
  let logContainerRef = $state<HTMLDivElement | null>(null);

  // Timers
  let refreshTimer: any = null;
  let logStreamTimer: any = null;

  // Aggregate Metrics Computed
  let totalApps = $derived(processes.length);
  let onlineApps = $derived(processes.filter(p => p.status === 'online').length);
  let stoppedApps = $derived(processes.filter(p => p.status === 'stopped').length);
  let erroredApps = $derived(processes.filter(p => p.status === 'errored').length);
  let totalMemoryBytes = $derived(processes.reduce((acc, p) => acc + (p.memory || 0), 0));
  let totalCpuPercent = $derived(processes.reduce((acc, p) => acc + (p.cpu || 0), 0));

  // Filtered Processes for Master Pane
  let filteredProcesses = $derived(
    processes.filter(p => {
      if (statusFilter === 'online' && p.status !== 'online') return false;
      if (statusFilter === 'stopped' && p.status !== 'stopped') return false;
      if (statusFilter === 'errored' && p.status !== 'errored') return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchName = p.name.toLowerCase().includes(q);
        const matchId = p.pm_id.toString().includes(q);
        const matchPid = p.pid?.toString().includes(q);
        const matchPath = p.script_path.toLowerCase().includes(q);
        return matchName || matchId || matchPid || matchPath;
      }
      return true;
    })
  );

  // Active Selected Process in Detail View
  let selectedProcess = $derived.by<Pm2Process | null>(() => {
    if (selectedProcessId === null) {
      return filteredProcesses.length > 0 ? filteredProcesses[0] : (processes.length > 0 ? processes[0] : null);
    }
    const found = processes.find(p => p.pm_id === selectedProcessId);
    if (found) return found;
    return filteredProcesses.length > 0 ? filteredProcesses[0] : (processes.length > 0 ? processes[0] : null);
  });

  // Filtered Environment Variables
  let filteredEnvVars = $derived.by<[string, string][]>(() => {
    if (!selectedProcess?.env_vars) return [];
    const entries = Object.entries(selectedProcess.env_vars);
    if (!envSearchQuery.trim()) return entries;
    const q = envSearchQuery.toLowerCase();
    return entries.filter(([k, v]) => k.toLowerCase().includes(q) || v.toLowerCase().includes(q));
  });

  // Parsed Log Lines with Level & Timestamp Detection
  let parsedLogs = $derived.by<ParsedLogLine[]>(() => {
    if (!rawLogText || rawLogText.trim().length === 0) return [];
    const lines = rawLogText.split('\n');
    const result: ParsedLogLine[] = [];
    const search = logSearchQuery.trim().toLowerCase();

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (!line && i === lines.length - 1) continue;

      if (search && !line.toLowerCase().includes(search)) {
        continue;
      }

      let level: 'error' | 'warn' | 'info' = 'info';
      const lower = line.toLowerCase();
      if (lower.includes('err') || lower.includes('error') || lower.includes('failed') || lower.includes('exception') || lower.includes('fatal')) {
        level = 'error';
      } else if (lower.includes('warn') || lower.includes('warning')) {
        level = 'warn';
      }

      let timestamp: string | undefined = undefined;
      let message = line;
      const tsMatch = line.match(/^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?):?\s*(.*)$/);
      if (tsMatch) {
        timestamp = tsMatch[1];
        message = tsMatch[2] || '';
      }

      result.push({
        lineNumber: i + 1,
        raw: line,
        level,
        timestamp,
        message
      });
    }
    return result;
  });

  // Format Helpers
  function formatBytes(bytes: number): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatUptime(uptimeMs?: number): string {
    if (!uptimeMs) return '–';
    const now = Date.now();
    const diff = Math.max(0, Math.floor((now - uptimeMs) / 1000));
    if (diff < 60) return `${diff}s`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ${diff % 60}s`;
    const hours = Math.floor(diff / 3600);
    const mins = Math.floor((diff % 3600) / 60);
    if (hours < 24) return `${hours}h ${mins}m`;
    const days = Math.floor(hours / 24);
    return `${days}d ${hours % 24}h`;
  }

  function copyToClipboard(text: string, identifier: string) {
    navigator.clipboard.writeText(text).then(() => {
      copiedCommand = identifier;
      setTimeout(() => {
        if (copiedCommand === identifier) copiedCommand = null;
      }, 2000);
    });
  }

  function copyValue(val: string, label: string) {
    navigator.clipboard.writeText(val).then(() => {
      copiedField = label;
      uiStore.showToast(`Copied ${label}`, 'info');
      setTimeout(() => {
        if (copiedField === label) copiedField = null;
      }, 2000);
    });
  }

  function copyCurrentLogs() {
    if (!rawLogText) return;
    navigator.clipboard.writeText(rawLogText).then(() => {
      copiedLogs = true;
      setTimeout(() => copiedLogs = false, 2000);
    });
  }

  // Load all PM2 Data
  async function loadData(silent = false) {
    if (!silent) refreshing = true;
    try {
      const status = await invoke<Pm2SystemStatus>('pm2_get_system_status');
      systemStatus = status;

      if (status.installed) {
        const [procList, ecoList, startStatus, savedList] = await Promise.all([
          invoke<Pm2Process[]>('pm2_list_processes').catch(() => []),
          invoke<Pm2EcosystemFile[]>('pm2_list_ecosystem_files', { customDirs: null }).catch(() => []),
          invoke<Pm2StartupStatus>('pm2_get_startup_status').catch(() => null),
          invoke<Pm2SavedDumpApp[]>('pm2_get_saved_dump_apps').catch(() => [])
        ]);
        processes = procList;
        ecosystemFiles = ecoList;
        startupStatus = startStatus;
        savedDumpApps = savedList;

        // Auto-select first process if none selected or selected was removed
        if (procList.length > 0) {
          if (selectedProcessId === null || !procList.some(p => p.pm_id === selectedProcessId)) {
            selectedProcessId = procList[0].pm_id;
          }
        } else {
          selectedProcessId = null;
        }

        if (ecoList.length > 0 && !selectedEcosystemFile) {
          selectEcosystemFile(ecoList[0]);
        }
      }
    } catch (err: any) {
      uiStore.showToast(err.message || 'Failed to communicate with PM2', 'error');
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  // Lifecycle Actions
  async function handleProcessAction(action: string, target: string | number) {
    try {
      const msg = await invoke<string>('pm2_process_action', {
        action,
        target: target.toString()
      });
      uiStore.showToast(msg, 'success');
      await loadData(true);
      if (activeTab === 'fleet' && selectedInspectorTab === 'logs') {
        loadLogs();
      }
    } catch (err: any) {
      uiStore.showToast(err || `Failed to ${action} process`, 'error');
    }
  }

  async function handleSaveDump() {
    try {
      const msg = await invoke<string>('pm2_save_dump');
      uiStore.showToast(msg, 'success');
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to save PM2 dump', 'error');
    }
  }

  async function handleResurrect() {
    try {
      const msg = await invoke<string>('pm2_resurrect_dump');
      uiStore.showToast(msg, 'success');
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to resurrect PM2 dump', 'error');
    }
  }

  async function handleStartSavedApp(app: Pm2SavedDumpApp) {
    try {
      const msg = await invoke<string>('pm2_start_saved_app', {
        name: app.name,
        scriptPath: app.script_path,
        cwd: app.cwd || null,
        execMode: app.exec_mode || null,
        instances: app.instances || null,
        watch: app.watch || null,
        args: app.args && app.args.length > 0 ? app.args : null
      });
      uiStore.showToast(msg, 'success');
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || `Failed to start saved app ${app.name}`, 'error');
    }
  }

  async function handleDeleteSavedApp(name: string) {
    try {
      const msg = await invoke<string>('pm2_delete_saved_app', { name });
      uiStore.showToast(msg, 'success');
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || `Failed to delete ${name} from dump`, 'error');
    }
  }

  async function handleFlushLogs(target?: string | number) {
    try {
      const msg = await invoke<string>('pm2_flush_logs', {
        target: target ? target.toString() : null
      });
      uiStore.showToast(msg, 'success');
      loadLogs();
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to flush logs', 'error');
    }
  }

  // ═════════════════════════════════════════════════════════════════════════
  // CONFIRMATION DIALOG PROMPTS (UX SAFETY)
  // ═════════════════════════════════════════════════════════════════════════
  function promptDeleteProcess(proc: Pm2Process) {
    uiStore.confirm(
      `Delete Process "${proc.name}"`,
      `Are you sure you want to stop and delete "${proc.name}" (ID #${proc.pm_id}, PID: ${proc.pid ?? 'None'}) from PM2? All running instances will be terminated.`,
      () => handleProcessAction('delete', proc.pm_id),
      true
    );
  }

  function promptFlushLogs(proc?: Pm2Process | null) {
    const targetName = proc ? `"${proc.name}" (ID #${proc.pm_id})` : 'all PM2 processes';
    const targetId = proc ? proc.pm_id : undefined;
    uiStore.confirm(
      'Flush Application Logs',
      `Are you sure you want to flush/clear the stdout & stderr log files for ${targetName}? Existing log history on disk will be emptied.`,
      () => handleFlushLogs(targetId),
      true
    );
  }

  function promptStopAll() {
    uiStore.confirm(
      'Stop Entire PM2 Fleet',
      'Are you sure you want to stop all active Node.js processes across this cluster?',
      () => handleProcessAction('stop', 'all'),
      true
    );
  }

  function promptReloadAll() {
    uiStore.confirm(
      'Reload Entire Cluster',
      'This will perform a zero-downtime reload across all running PM2 applications. Proceed?',
      () => handleProcessAction('reload', 'all'),
      false
    );
  }

  function promptResurrect() {
    const count = savedDumpApps.length;
    uiStore.confirm(
      'Resurrect Applications from Dump',
      `Restore ${count > 0 ? `${count} saved applications` : 'all applications'} from ~/.pm2/dump.pm2 into active PM2 memory?`,
      () => handleResurrect(),
      false
    );
  }

  function promptDeleteSavedApp(name: string) {
    uiStore.confirm(
      `Remove "${name}" from Saved Dump`,
      `Are you sure you want to remove "${name}" from ~/.pm2/dump.pm2? It will no longer be restored automatically on system reboot.`,
      () => handleDeleteSavedApp(name),
      true
    );
  }

  // Logs stream handling for currently selected process
  async function loadLogs() {
    if (!systemStatus?.installed) return;
    logLoading = true;
    try {
      let logPath = '';
      if (!selectedProcess) {
        logPath = `${systemStatus.pm2_home}/pm2.log`;
      } else {
        logPath = selectedLogType === 'err' ? selectedProcess.err_log_path : selectedProcess.out_log_path;
      }

      if (logPath) {
        const content = await invoke<string>('pm2_read_logs', {
          logPath,
          lines: parseInt(logTailLines, 10) || 200
        });
        rawLogText = content;
      }
    } catch (err: any) {
      rawLogText = `[Error reading logs: ${err}]`;
    } finally {
      logLoading = false;
    }
  }

  // Select a process in the Master list (1-click opens details drawer)
  function handleSelectProcess(id: number, openDrawer = true) {
    selectedProcessId = id;
    if (openDrawer) {
      showProcessDrawer = true;
    }
    loadLogs();
  }

  // Launch custom process
  async function launchProcess() {
    if (!launchForm.script_path.trim()) {
      uiStore.showToast('Please specify a script or entry file path', 'warning');
      return;
    }

    try {
      const envMap: Record<string, string> = {};
      for (const env of launchForm.env_vars) {
        if (env.key.trim()) envMap[env.key.trim()] = env.value;
      }

      const argsArr = launchForm.args.trim()
        ? launchForm.args.split(/\s+/).filter(Boolean)
        : null;

      const msg = await invoke<string>('pm2_start_custom_process', {
        options: {
          script_path: launchForm.script_path.trim(),
          name: launchForm.name.trim() || null,
          cwd: launchForm.cwd.trim() || null,
          instances: launchForm.exec_mode === 'cluster' ? launchForm.instances : null,
          exec_mode: launchForm.exec_mode,
          watch: launchForm.watch,
          max_memory_restart: launchForm.max_memory_restart.trim() || null,
          args: argsArr,
          env_vars: Object.keys(envMap).length > 0 ? envMap : null
        }
      });

      uiStore.showToast(msg, 'success');
      showLaunchModal = false;
      resetLaunchForm();
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to start process', 'error');
    }
  }

  // File & Directory Browsing for Launch Modal
  async function browseScriptFile() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Node.js / PM2 Entrypoint Script',
        filters: [
          { name: 'JavaScript & TypeScript (*.js, *.ts, *.mjs, *.cjs)', extensions: ['js', 'mjs', 'cjs', 'ts', 'jsx', 'tsx'] },
          { name: 'Python & Shell Scripts (*.py, *.sh)', extensions: ['py', 'sh', 'bash'] },
          { name: 'JSON & Ecosystem Configs (*.json, *.yml)', extensions: ['json', 'yaml', 'yml'] },
          { name: 'All Files (*.*)', extensions: ['*'] }
        ]
      });
      if (selected && typeof selected === 'string') {
        launchForm.script_path = selected;

        // Auto-infer CWD if empty
        const pathSegments = selected.split('/');
        const fileName = pathSegments.pop() || '';
        const dirPath = pathSegments.join('/') || '/';

        if (!launchForm.cwd || launchForm.cwd.trim() === '') {
          launchForm.cwd = dirPath;
        }

        // Auto-infer App Name if empty
        if (!launchForm.name || launchForm.name.trim() === '') {
          const baseName = fileName.replace(/\.[^/.]+$/, '');
          if (['index', 'main', 'app', 'server', 'dist', 'bin'].includes(baseName.toLowerCase()) && pathSegments.length > 0) {
            const parentDir = pathSegments[pathSegments.length - 1];
            launchForm.name = parentDir || baseName;
          } else {
            launchForm.name = baseName || 'app';
          }
        }
      }
    } catch (e) {
      uiStore.showToast(`File selection error: ${e}`, 'error');
    }
  }

  async function browseWorkingDir() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: 'Select Working Directory (CWD)'
      });
      if (selected && typeof selected === 'string') {
        launchForm.cwd = selected;
      }
    } catch (e) {
      uiStore.showToast(`Directory selection error: ${e}`, 'error');
    }
  }

  async function browseEcosystemDestination() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: 'Select Destination Directory for Ecosystem Config'
      });
      if (selected && typeof selected === 'string') {
        newEcosystemPath = `${selected.replace(/\/+$/, '')}/ecosystem.config.js`;
      }
    } catch (e) {
      uiStore.showToast(`Directory selection error: ${e}`, 'error');
    }
  }

  function addEnvPreset(key: string, value: string) {
    const existing = launchForm.env_vars.find(e => e.key === key);
    if (existing) {
      existing.value = value;
      launchForm.env_vars = [...launchForm.env_vars];
    } else {
      launchForm.env_vars = [...launchForm.env_vars, { key, value }];
    }
  }

  function parseAndApplyBulkEnv() {
    if (!bulkEnvText.trim()) return;
    const lines = bulkEnvText.split('\n');
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const eqIdx = trimmed.indexOf('=');
      if (eqIdx > 0) {
        const key = trimmed.slice(0, eqIdx).trim();
        let val = trimmed.slice(eqIdx + 1).trim();
        if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
          val = val.slice(1, -1);
        }
        if (key) {
          const existing = launchForm.env_vars.find(e => e.key === key);
          if (existing) {
            existing.value = val;
          } else {
            launchForm.env_vars.push({ key, value: val });
          }
        }
      }
    }
    launchForm.env_vars = [...launchForm.env_vars];
    bulkEnvText = '';
    showBulkEnvInput = false;
    uiStore.showToast('Environment variables imported', 'success');
  }

  function resetLaunchForm() {
    launchStep = 1;
    bulkEnvText = '';
    showBulkEnvInput = false;
    launchForm = {
      script_path: '',
      name: '',
      cwd: '',
      exec_mode: 'fork',
      instances: 1,
      watch: false,
      max_memory_restart: '500M',
      args: '',
      env_vars: [{ key: 'NODE_ENV', value: 'production' }]
    };
  }

  function addEnvRow() {
    launchForm.env_vars = [...launchForm.env_vars, { key: '', value: '' }];
  }

  function removeEnvRow(index: number) {
    launchForm.env_vars = launchForm.env_vars.filter((_, i) => i !== index);
  }

  // Ecosystem file handling
  async function selectEcosystemFile(file: Pm2EcosystemFile) {
    selectedEcosystemFile = file;
    try {
      const content = await invoke<string>('pm2_read_ecosystem_file', { path: file.path });
      ecosystemContent = content;
      originalEcosystemContent = content;
    } catch (err: any) {
      uiStore.showToast(`Failed to read ${file.name}: ${err}`, 'error');
    }
  }

  function openSaveEcosystemModal() {
    if (!selectedEcosystemFile) return;
    showEcosystemDiffModal = true;
  }

  async function startEcosystemFile() {
    if (!selectedEcosystemFile) return;
    try {
      const msg = await invoke<string>('pm2_start_ecosystem', {
        path: selectedEcosystemFile.path,
        only: null,
        envName: null
      });
      uiStore.showToast(msg, 'success');
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to start ecosystem file', 'error');
    }
  }

  function generateTemplateContent(type: string): string {
    if (type === 'express') {
      return `module.exports = {
  apps: [
    {
      name: 'express-api',
      script: './src/server.js',
      instances: 'max',
      exec_mode: 'cluster',
      watch: false,
      max_memory_restart: '500M',
      env: {
        NODE_ENV: 'development',
        PORT: 3000
      },
      env_production: {
        NODE_ENV: 'production',
        PORT: 8080
      }
    }
  ]
};`;
    } else if (type === 'nest') {
      return `module.exports = {
  apps: [
    {
      name: 'nest-service',
      script: 'dist/main.js',
      instances: 2,
      exec_mode: 'cluster',
      env: {
        NODE_ENV: 'production'
      }
    }
  ]
};`;
    } else if (type === 'next') {
      return `module.exports = {
  apps: [
    {
      name: 'nextjs-web',
      script: 'node_modules/next/dist/bin/next',
      args: 'start -p 3000',
      instances: 'max',
      exec_mode: 'cluster',
      env: {
        NODE_ENV: 'production'
      }
    }
  ]
};`;
    } else {
      return `module.exports = {
  apps: [
    {
      name: 'background-worker',
      script: './worker.js',
      instances: 1,
      exec_mode: 'fork',
      restart_delay: 5000,
      env: {
        NODE_ENV: 'production'
      }
    }
  ]
};`;
    }
  }

  async function createNewEcosystemFile() {
    if (!newEcosystemPath.trim()) {
      uiStore.showToast('Please specify a valid path for ecosystem.config.js', 'warning');
      return;
    }

    try {
      const content = generateTemplateContent(newEcosystemTemplate);
      await invoke<string>('pm2_write_ecosystem_file', {
        path: newEcosystemPath.trim(),
        content
      });
      uiStore.showToast('Ecosystem configuration created successfully', 'success');
      showNewEcosystemModal = false;
      await loadData(true);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to create ecosystem file', 'error');
    }
  }

  onMount(() => {
    loadData();

    // 3s Live Telemetry Refresh
    refreshTimer = setInterval(() => {
      if (systemStatus?.installed && activeTab === 'fleet') {
        loadData(true);
      }
    }, 3000);

    // 2s Logs Auto-Stream Refresh
    logStreamTimer = setInterval(() => {
      if (systemStatus?.installed && activeTab === 'fleet' && selectedInspectorTab === 'logs' && logAutoRefresh) {
        loadLogs();
      }
    }, logRefreshInterval * 1000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
    if (logStreamTimer) clearInterval(logStreamTimer);
  });
</script>

<div class="module-page">
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <!-- 1. PAGE HEADER (TITLE + GLOBAL ACTIONS) -->
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <PageHeader 
    title="PM2 Manager" 
    subtitle="Node.js process monitoring, cluster balancing, and real-time logs" 
    icon={Layers}
  >
    <div class="header-action-btns">
      <Button 
        variant="outline" 
        size="sm" 
        onclick={() => loadData()} 
        disabled={refreshing}
        title="Refresh telemetry"
      >
        <RotateCw size={13} class={refreshing ? 'spin' : ''} />
        <span>Refresh</span>
      </Button>

      {#if systemStatus?.installed}
        <Button 
          variant="primary" 
          size="sm" 
          onclick={() => showLaunchModal = true}
          title="Launch a new Node.js process"
        >
          <Plus size={14} />
          <span>Launch App</span>
        </Button>
      {/if}
    </div>
  </PageHeader>

  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <!-- 2. SCOPE NAVIGATION BAR (FLEET CONSOLE | ECOSYSTEM | DAEMON) -->
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <div class="controls-row">
    <TabGroup
      tabs={[
        { id: 'fleet', label: 'Fleet Console', count: processes.length },
        { id: 'ecosystem', label: 'Ecosystem Configs', count: ecosystemFiles.length },
        { id: 'daemon', label: 'Daemon & Saved Apps', count: savedDumpApps.length }
      ]}
      bind:activeTab
      onchange={(t: any) => {
        if (t === 'fleet' && selectedInspectorTab === 'logs') loadLogs();
      }}
    />

    <div class="tab-actions">
      {#if systemStatus?.installed}
        <div class="version-badges-group">
          <span class="v-pill success">
            <span class="pulse-dot"></span> PM2 v{systemStatus.version || 'Active'}
          </span>
          {#if systemStatus.node_version}
            <span class="v-pill neutral">Node {systemStatus.node_version}</span>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <!-- 3. TOP TELEMETRY KPI CARDS BANNER -->
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  {#if systemStatus?.installed}
    <div class="kpi-grid">
      <KpiCard
        icon={Layers}
        value={totalApps}
        label="Managed Apps"
        subtext="{onlineApps} running"
        statusType="info"
      />

      <KpiCard
        icon={CheckCircle2}
        value={onlineApps}
        label="Online"
        subtext={stoppedApps + erroredApps === 0 ? 'All healthy' : `${stoppedApps + erroredApps} stopped/errored`}
        statusType={erroredApps > 0 ? 'error' : 'success'}
      />

      <KpiCard
        icon={Cpu}
        value="{totalCpuPercent.toFixed(1)}%"
        label="Cluster CPU"
        subtext="Aggregate load"
        statusType="info"
      />

      <KpiCard
        icon={HardDrive}
        value={formatBytes(totalMemoryBytes)}
        label="Memory Heap"
        subtext="Combined RAM"
        statusType="info"
      />
    </div>
  {/if}

  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <!-- 4. MAIN WORKSPACE BODY -->
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  <div class="pm2-workspace-body">
    {#if !loading && (!systemStatus || !systemStatus.installed)}
      <!-- PM2 Missing Hero Card -->
      <div class="not-installed-card">
        <div class="not-installed-hero">
          <div class="hero-icon-ring">
            <AlertTriangle size={36} class="text-amber" />
          </div>
          <h2>PM2 is not installed on this system</h2>
          <p class="hero-subtext">
            PM2 (Production Process Manager for Node.js) is required to monitor, restart, and scale your backend applications.
            Install it globally using one of the commands below:
          </p>
        </div>

        <div class="install-commands-grid">
          <div class="command-box">
            <div class="command-box-header">
              <span class="command-box-title">NPM Global (Recommended)</span>
              <span class="command-tag">Standard</span>
            </div>
            <p class="command-desc">Install PM2 globally for all system users via Node Package Manager:</p>
            <div class="code-terminal">
              <code>sudo npm install -g pm2</code>
              <button
                class="copy-btn"
                onclick={() => copyToClipboard('sudo npm install -g pm2', 'npm')}
                title="Copy command"
              >
                {#if copiedCommand === 'npm'}
                  <Check size={14} class="text-emerald" />
                {:else}
                  <Copy size={14} />
                {/if}
              </button>
            </div>
          </div>

          <div class="command-box">
            <div class="command-box-header">
              <span class="command-box-title">PNPM / Yarn</span>
              <span class="command-tag">Alternative</span>
            </div>
            <p class="command-desc">If using PNPM or Yarn modern package managers:</p>
            <div class="code-terminal">
              <code>pnpm add -g pm2</code>
              <button
                class="copy-btn"
                onclick={() => copyToClipboard('pnpm add -g pm2', 'pnpm')}
                title="Copy command"
              >
                {#if copiedCommand === 'pnpm'}
                  <Check size={14} class="text-emerald" />
                {:else}
                  <Copy size={14} />
                {/if}
              </button>
            </div>
          </div>

          <div class="command-box">
            <div class="command-box-header">
              <span class="command-box-title">Fedora / RHEL Prerequisites</span>
              <span class="command-tag">System</span>
            </div>
            <p class="command-desc">If Node.js and NPM are not yet installed on this machine:</p>
            <div class="code-terminal">
              <code>sudo dnf install -y nodejs npm</code>
              <button
                class="copy-btn"
                onclick={() => copyToClipboard('sudo dnf install -y nodejs npm', 'dnf')}
                title="Copy command"
              >
                {#if copiedCommand === 'dnf'}
                  <Check size={14} class="text-emerald" />
                {:else}
                  <Copy size={14} />
                {/if}
              </button>
            </div>
          </div>
        </div>

        <div class="not-installed-footer">
          <Button
            variant="primary"
            size="lg"
            onclick={() => loadData()}
            disabled={refreshing}
          >
            <RotateCw size={16} class={refreshing ? 'spin' : ''} />
            <span>Check Again / Refresh</span>
          </Button>
        </div>
      </div>

    {:else if systemStatus?.installed}
      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      <!-- VIEW 1: MASTER-DETAIL SPLIT CONSOLE (FLEET & LIVE INSPECTOR) -->
      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      {#if activeTab === 'fleet'}
        <div class="fleet-split-container">
          <!-- ── MASTER PANE: PROCESSES LIST (360px) ────────────────────────── -->
          <div class="fleet-master-pane">
            <!-- Search & Filter Header -->
            <div class="master-header">
              <div class="master-search-row">
                <SearchBar
                  bind:value={searchQuery}
                  placeholder="Filter apps by name, PID, ID..."
                  style="margin: 0; width: 100%;"
                />
              </div>

              <div class="master-pills-row">
                <div class="pills-group">
                  <button
                    class="pill-btn"
                    class:active={statusFilter === 'all'}
                    onclick={() => statusFilter = 'all'}
                  >
                    All ({processes.length})
                  </button>
                  <button
                    class="pill-btn"
                    class:active={statusFilter === 'online'}
                    onclick={() => statusFilter = 'online'}
                  >
                    Online ({onlineApps})
                  </button>
                  <button
                    class="pill-btn"
                    class:active={statusFilter === 'stopped'}
                    onclick={() => statusFilter = 'stopped'}
                  >
                    Stopped ({stoppedApps})
                  </button>
                  <button
                    class="pill-btn"
                    class:active={statusFilter === 'errored'}
                    onclick={() => statusFilter = 'errored'}
                  >
                    Errored ({erroredApps})
                  </button>
                </div>

                <KebabMenu align="left" title="Cluster Bulk Actions">
                  <button class="menu-item" onclick={() => handleProcessAction('start', 'all')}>
                    <Play size={14} class="text-emerald" />
                    <span>Start All Processes</span>
                  </button>

                  <button class="menu-item" onclick={promptReloadAll}>
                    <Zap size={14} class="text-accent" />
                    <span>Reload All (0-Downtime)</span>
                  </button>

                  <button class="menu-item" onclick={promptStopAll}>
                    <Square size={14} class="text-amber" />
                    <span>Stop All Processes</span>
                  </button>

                  <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>

                  <button class="menu-item" onclick={handleSaveDump}>
                    <Save size={14} class="text-accent" />
                    <span>Save Process List (pm2 save)</span>
                  </button>

                  <button class="menu-item" onclick={promptResurrect}>
                    <RefreshCcw size={14} class="text-purple" />
                    <span>Resurrect Saved Apps (pm2 resurrect)</span>
                  </button>
                </KebabMenu>
              </div>
            </div>

            <!-- Scrollable Process Cards List -->
            <div class="master-cards-list">
              {#if filteredProcesses.length === 0}
                <div class="empty-list-container">
                  <EmptyState
                    icon={Layers}
                    title="No active processes"
                    description={processes.length === 0 
                      ? "No Node.js applications are currently running in PM2 memory." 
                      : "No processes match the current search filter."}
                    actionLabel={processes.length === 0 ? "Launch App" : "Clear Filter"}
                    actionIcon={Plus}
                    onAction={() => {
                      if (processes.length === 0) showLaunchModal = true;
                      else { searchQuery = ''; statusFilter = 'all'; }
                    }}
                  />

                  {#if processes.length === 0 && savedDumpApps.length > 0}
                    <!-- Quick Saved Apps Restore Box -->
                    <div class="saved-dump-quick-section">
                      <div class="quick-section-header">
                        <div class="quick-section-title">
                          <Database size={13} class="text-accent" />
                          <span>Saved in dump ({savedDumpApps.length}):</span>
                        </div>
                        <Button variant="ghost" size="xs" onclick={promptResurrect} title="Restore all saved applications">
                          <RefreshCcw size={11} />
                          <span>Resurrect All</span>
                        </Button>
                      </div>
                      <div class="quick-dump-list">
                        {#each savedDumpApps as app}
                          <div class="quick-dump-item">
                            <div class="quick-item-info">
                              <span class="quick-item-name font-mono">{app.name}</span>
                              <span class="quick-item-path" title={app.script_path}>{app.script_path.split('/').pop() || app.script_path}</span>
                            </div>
                            <Button
                              variant="primary"
                              size="xs"
                              onclick={() => handleStartSavedApp(app)}
                              title="Start {app.name} now"
                            >
                              <Play size={11} />
                              <span>Run</span>
                            </Button>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {:else if processes.length === 0}
                    <div class="resurrect-hint-row">
                      <Button variant="outline" size="sm" onclick={promptResurrect}>
                        <RefreshCcw size={13} />
                        <span>Restore Saved Dump</span>
                      </Button>
                    </div>
                  {/if}
                </div>
              {:else}
                {#each filteredProcesses as p (p.pm_id)}
                  {@const isSelected = selectedProcess?.pm_id === p.pm_id}
                  {@const isOnline = p.status === 'online'}
                  {@const isErrored = p.status === 'errored'}

                  <div 
                    class="proc-card"
                    class:selected={isSelected}
                    class:errored={isErrored}
                    role="button"
                    tabindex="0"
                    onclick={() => handleSelectProcess(p.pm_id)}
                    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleSelectProcess(p.pm_id); }}
                  >
                    <!-- Card Top Line: Status Dot, Name, ID, Mode -->
                    <div class="proc-card-top">
                      <div class="proc-identity">
                        <span class="status-dot {p.status}">
                          {#if isOnline}<span class="dot-pulse"></span>{/if}
                        </span>
                        <span class="proc-name" title={p.name}>{p.name}</span>
                        <span class="proc-id-tag font-mono">#{p.pm_id}</span>
                        {#if p.version}
                          <span class="proc-version-tag">v{p.version}</span>
                        {/if}
                      </div>

                      <span class="mode-tag {p.exec_mode === 'cluster_mode' ? 'cluster' : 'fork'}">
                        {p.exec_mode === 'cluster_mode' ? `cluster (${p.instances})` : 'fork'}
                      </span>
                    </div>

                    <!-- Path Subtext -->
                    <div class="proc-path-line" title={p.script_path}>
                      {p.script_path}
                    </div>

                    <!-- Telemetry Meters Bar -->
                    <div class="proc-meters-grid">
                      <div class="meter-col">
                        <div class="meter-label-row">
                          <span class="m-label">CPU</span>
                          <span class="m-val font-mono">{p.cpu.toFixed(1)}%</span>
                        </div>
                        <div class="mini-track">
                          <div 
                            class="mini-bar cpu-bar" 
                            style="width: {Math.min(100, p.cpu)}%; background: {p.cpu > 80 ? 'var(--color-error)' : p.cpu > 40 ? 'var(--color-warning)' : 'var(--color-accent)'};"
                          ></div>
                        </div>
                      </div>

                      <div class="meter-col">
                        <div class="meter-label-row">
                          <span class="m-label">RAM</span>
                          <span class="m-val font-mono">{formatBytes(p.memory)}</span>
                        </div>
                        <div class="mini-track">
                          <div 
                            class="mini-bar mem-bar" 
                            style="width: {Math.min(100, (p.memory / (1024 * 1024 * 1024)) * 100)}%;"
                          ></div>
                        </div>
                      </div>
                    </div>

                    <!-- Card Footer: Uptime, Restarts, Quick Actions -->
                    <div class="proc-card-footer">
                      <div class="proc-meta-tags">
                        <span class="meta-item font-mono" title="Uptime">
                          <Clock size={11} /> {isOnline ? formatUptime(p.uptime) : 'stopped'}
                        </span>
                        {#if p.pid}
                          <span class="meta-item font-mono" title="System PID">
                            PID: {p.pid}
                          </span>
                        {/if}
                        <span class="meta-item font-mono {p.unstable_restarts > 0 ? 'text-rose' : ''}" title="{p.restarts} restarts">
                          {p.restarts} rst{#if p.unstable_restarts > 0} ⚠️{/if}
                        </span>
                      </div>

                      <div class="proc-hover-actions" onclick={(e) => e.stopPropagation()} role="presentation">
                        {#if isOnline}
                          <button
                            class="card-action-btn action-restart"
                            onclick={() => handleProcessAction('restart', p.pm_id)}
                            title="Restart process"
                          >
                            <RotateCw size={12} />
                          </button>
                          <button
                            class="card-action-btn action-reload"
                            onclick={() => handleProcessAction('reload', p.pm_id)}
                            title="0-downtime reload"
                          >
                            <Zap size={12} />
                          </button>
                          <button
                            class="card-action-btn action-stop"
                            onclick={() => handleProcessAction('stop', p.pm_id)}
                            title="Stop process"
                          >
                            <Square size={12} />
                          </button>
                        {:else}
                          <button
                            class="card-action-btn action-start"
                            onclick={() => handleProcessAction('start', p.pm_id)}
                            title="Start process"
                          >
                            <Play size={12} />
                          </button>
                        {/if}
                        <button
                          class="card-action-btn action-delete"
                          onclick={(e) => { e.stopPropagation(); promptDeleteProcess(p); }}
                          title="Delete process"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    </div>
                  </div>
                {/each}
              {/if}
            </div>
          </div>

          <!-- ── DETAIL PANE: LIVE REAL-TIME LOGS TERMINAL DECK ──────────────── -->
          <div class="fleet-detail-pane">
            {#if selectedProcess}
              <div class="logs-deck-wrap">
                <!-- Logs Top Controls Ribbon -->
                <div class="logs-deck-controls">
                  <div class="logs-controls-left">
                    <!-- Stdout / Stderr Segment -->
                    <div class="segmented-control">
                      <button
                        class="seg-btn"
                        class:active={selectedLogType === 'combined'}
                        onclick={() => { selectedLogType = 'combined'; loadLogs(); }}
                      >
                        Stdout
                      </button>
                      <button
                        class="seg-btn"
                        class:active={selectedLogType === 'err'}
                        onclick={() => { selectedLogType = 'err'; loadLogs(); }}
                      >
                        Stderr
                      </button>
                    </div>

                    <!-- Tail lines count -->
                    <div class="select-field">
                      <span class="field-label">Lines:</span>
                      <Select
                        bind:value={logTailLines}
                        onchange={() => loadLogs()}
                        style="width: 95px; height: 28px;"
                      >
                        <option value="100">100</option>
                        <option value="200">200</option>
                        <option value="500">500</option>
                        <option value="1000">1000</option>
                      </Select>
                    </div>
                  </div>

                  <div class="logs-controls-right">
                    <SearchBar
                      bind:value={logSearchQuery}
                      placeholder="Filter logs..."
                      style="margin: 0; width: 160px;"
                    />

                    <Button
                      variant={logAutoRefresh ? 'primary' : 'outline'}
                      size="sm"
                      onclick={() => logAutoRefresh = !logAutoRefresh}
                      title="Toggle live auto polling"
                    >
                      <RotateCw size={12} class={logAutoRefresh ? 'spin' : ''} />
                      <span>{logAutoRefresh ? 'Live (2s)' : 'Paused'}</span>
                    </Button>

                    <KebabMenu align="right" title="Application & Log Options">
                      <button class="menu-item" onclick={() => showProcessDrawer = true}>
                        <Info size={14} class="text-accent" />
                        <span>App Details & Telemetry</span>
                      </button>

                      <button class="menu-item" onclick={copyCurrentLogs}>
                        {#if copiedLogs}
                          <Check size={14} class="text-emerald" />
                          <span class="text-emerald">Copied to Clipboard!</span>
                        {:else}
                          <Copy size={14} />
                          <span>Copy Log Stream</span>
                        {/if}
                      </button>

                      <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>

                      <button class="menu-item text-danger" onclick={() => promptFlushLogs(selectedProcess)}>
                        <Trash2 size={14} class="text-error" />
                        <span>Flush Logs</span>
                      </button>
                    </KebabMenu>
                  </div>
                </div>

                <!-- Terminal Card -->
                <div class="log-stream-card">
                  <div class="log-stream-header">
                    <div class="log-source-info">
                      <FileText size={13} class="text-accent" />
                      <span class="log-source-path font-mono" title={selectedLogType === 'err' ? selectedProcess.err_log_path : selectedProcess.out_log_path}>
                        {selectedLogType === 'err' ? selectedProcess.err_log_path : selectedProcess.out_log_path}
                      </span>
                    </div>
                    <span class="log-count-tag font-mono">
                      {parsedLogs.length} lines rendered
                    </span>
                  </div>

                  <div class="log-lines-container" bind:this={logContainerRef}>
                    {#if logLoading && parsedLogs.length === 0}
                      <div class="log-state-msg">
                        <RotateCw size={22} class="spin text-accent" />
                        <span>Streaming PM2 log output…</span>
                      </div>
                    {:else if parsedLogs.length === 0}
                      <div class="log-state-msg">
                        <Terminal size={28} class="text-muted" />
                        <p>No log output recorded in this file yet.</p>
                      </div>
                    {:else}
                      <div class="log-table">
                        {#each parsedLogs as entry}
                          <div class="log-row {entry.level === 'error' ? 'row-error' : entry.level === 'warn' ? 'row-warn' : ''}">
                            <span class="log-line-no font-mono">{entry.lineNumber}</span>
                            
                            {#if entry.timestamp}
                              <span class="log-ts font-mono">{entry.timestamp}</span>
                            {/if}

                            {#if entry.level === 'error'}
                              <span class="log-level-badge level-err">ERR</span>
                            {:else if entry.level === 'warn'}
                              <span class="log-level-badge level-warn">WARN</span>
                            {/if}

                            <span class="log-msg font-mono">{entry.message}</span>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {:else}
              <!-- Empty Selection State -->
              <div class="no-selection-wrap">
                <Layers size={44} class="text-muted" />
                <h3>No Application Selected</h3>
                <p>Click on any Node.js process on the left to inspect its real-time logs and open application details in the side drawer.</p>
                <Button variant="primary" onclick={() => showLaunchModal = true}>
                  <Plus size={14} />
                  <span>Launch Application</span>
                </Button>
              </div>
            {/if}
          </div>
        </div>

      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      <!-- VIEW 2: ECOSYSTEM CONFIGURATIONS STUDIO -->
      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'ecosystem'}
        <div class="ecosystem-layout">
          <!-- Sidebar -->
          <div class="ecosystem-sidebar">
            <div class="ecosystem-sidebar-header">
              <h3>Discovered Files</h3>
              <Button
                variant="primary"
                size="sm"
                onclick={() => showNewEcosystemModal = true}
              >
                <Plus size={13} />
                <span>New</span>
              </Button>
            </div>

            {#if ecosystemFiles.length === 0}
              <div class="sidebar-empty">
                <FileCode size={24} class="text-muted" />
                <p>No ecosystem.config.js found in user directory or /var/www</p>
                <Button variant="outline" size="sm" onclick={() => showNewEcosystemModal = true}>
                  Create Config File
                </Button>
              </div>
            {:else}
              <div class="files-list">
                {#each ecosystemFiles as f}
                  <button
                    class="file-item-btn"
                    class:active={selectedEcosystemFile?.path === f.path}
                    onclick={() => selectEcosystemFile(f)}
                  >
                    <FileCode size={16} class="file-icon text-accent" />
                    <div class="file-info">
                      <span class="file-name">{f.name}</span>
                      <span class="file-path-sub">{f.path}</span>
                    </div>
                  </button>
                {/each}
              </div>
            {/if}
          </div>

          <!-- Code Editor Panel -->
          <div class="ecosystem-editor-panel">
            {#if selectedEcosystemFile}
              <div class="editor-toolbar">
                <div class="editor-title-row">
                  <span class="editor-file-title font-mono">{selectedEcosystemFile.path}</span>
                  {#if isEcosystemDirty}
                    <span class="dirty-badge">Unsaved changes</span>
                  {/if}
                </div>

                <div class="editor-actions">
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={openSaveEcosystemModal}
                    disabled={!isEcosystemDirty}
                  >
                    <Save size={13} />
                    <span>Save</span>
                  </Button>

                  <Button
                    variant="primary"
                    size="sm"
                    onclick={startEcosystemFile}
                    title="Launch apps configured in this ecosystem file"
                  >
                    <Play size={13} />
                    <span>Start from Config</span>
                  </Button>
                </div>
              </div>

              <div class="code-editor-wrap">
                <CodeEditor
                  value={ecosystemContent}
                  language="javascript"
                  height="100%"
                  onchange={(newVal) => ecosystemContent = newVal}
                />
              </div>
            {:else}
              <div class="editor-empty">
                <FileCode size={36} class="text-muted" />
                <h3>Select an ecosystem file to view and edit</h3>
                <p>You can configure cluster instances, environment variables, and watch settings.</p>
              </div>
            {/if}
          </div>
        </div>

      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      <!-- VIEW 3: DAEMON RUNTIME & SYSTEMD STARTUP -->
      <!-- ═══════════════════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'daemon'}
        <div class="daemon-grid">
          <!-- Daemon Status Card -->
          <div class="daemon-card">
            <div class="daemon-card-header">
              <div class="icon-bubble">
                <Settings size={18} class="text-accent" />
              </div>
              <div>
                <h3>PM2 Daemon Runtime</h3>
                <p>System runtime environment & directories</p>
              </div>
            </div>

            <div class="details-list">
              <div class="detail-row">
                <span class="detail-label">PM2 Home Directory</span>
                <code class="detail-code">{systemStatus.pm2_home}</code>
              </div>
              <div class="detail-row">
                <span class="detail-label">Executable Path</span>
                <code class="detail-code">{systemStatus.executable_path || 'Standard Path'}</code>
              </div>
              <div class="detail-row">
                <span class="detail-label">Node.js Version</span>
                <span class="detail-val font-mono">{systemStatus.node_version || '–'}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">NPM Version</span>
                <span class="detail-val font-mono">{systemStatus.npm_version || '–'}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">Daemon Active</span>
                <span class="detail-val text-emerald">
                  <CheckCircle2 size={14} />
                  {systemStatus.daemon_running ? 'Running' : 'Ready'}
                </span>
              </div>
            </div>

            <div class="card-footer-actions">
              <Button variant="outline" size="sm" onclick={handleSaveDump}>
                <Save size={13} />
                <span>Save Dump State</span>
              </Button>
              <Button variant="outline" size="sm" onclick={handleResurrect}>
                <RefreshCcw size={13} />
                <span>Resurrect State</span>
              </Button>
            </div>
          </div>

          <!-- Systemd Startup Hook Card -->
          <div class="daemon-card">
            <div class="daemon-card-header">
              <div class="icon-bubble" style="background: rgba(16, 185, 129, 0.12); color: #34d399;">
                <Shield size={18} />
              </div>
              <div>
                <h3>Systemd Startup Hook</h3>
                <p>Keep Node.js apps running automatically across system reboots</p>
              </div>
            </div>

            <div class="details-list">
              <div class="detail-row">
                <span class="detail-label">Startup Service Status</span>
                {#if startupStatus?.is_enabled}
                  <span class="badge badge-success">Enabled (pm2-{startupStatus.user})</span>
                {:else}
                  <span class="badge badge-neutral">Not Configured</span>
                {/if}
              </div>
              <div class="detail-row">
                <span class="detail-label">Active User</span>
                <span class="detail-val font-mono">{startupStatus?.user || 'user'}</span>
              </div>
            </div>

            <div class="startup-hint-box">
              <p class="hint-title">To enable PM2 systemd boot auto-start, run this command in terminal:</p>
              <div class="code-terminal">
                <code>{startupStatus?.startup_command_hint || `pm2 startup systemd -u ${startupStatus?.user || 'user'}`}</code>
                <button
                  class="copy-btn"
                  onclick={() => copyToClipboard(startupStatus?.startup_command_hint || 'pm2 startup', 'startup')}
                >
                  {#if copiedCommand === 'startup'}
                    <Check size={14} class="text-emerald" />
                  {:else}
                    <Copy size={14} />
                  {/if}
                </button>
              </div>
            </div>
          </div>

          <!-- Saved Applications Snapshot Catalog Card (Full Width) -->
          <div class="daemon-card dump-catalog-card">
            <div class="daemon-card-header">
              <div class="icon-bubble" style="background: rgba(168, 85, 247, 0.12); color: #c084fc;">
                <Database size={18} />
              </div>
              <div class="catalog-header-flex">
                <div>
                  <h3>Saved Applications Snapshot</h3>
                  <p>Applications persisted in <code>{systemStatus.pm2_home}/dump.pm2</code> ({savedDumpApps.length} apps saved)</p>
                </div>
                <div class="catalog-header-actions">
                  <Button variant="outline" size="sm" onclick={handleSaveDump} title="Overwrite dump with current running processes">
                    <Save size={13} />
                    <span>Save Current Fleet</span>
                  </Button>
                  <Button variant="primary" size="sm" onclick={promptResurrect} title="Restore all saved processes into active PM2 memory">
                    <RefreshCcw size={13} />
                    <span>Resurrect All ({savedDumpApps.length})</span>
                  </Button>
                </div>
              </div>
            </div>

            {#if savedDumpApps.length === 0}
              <div class="catalog-empty">
                <Layers size={36} class="text-muted" />
                <h4>No Applications Saved in PM2 Dump</h4>
                <p>Launch your applications in the Fleet Console and click <strong>"Save Dump"</strong> to persist them across system reboots.</p>
              </div>
            {:else}
              <div class="saved-apps-grid">
                {#each savedDumpApps as app}
                  <div class="saved-app-card" class:running={app.is_currently_running}>
                    <div class="saved-app-top">
                      <div class="saved-app-identity">
                        <span class="status-dot {app.is_currently_running ? 'online' : 'stopped'}"></span>
                        <span class="saved-app-name font-mono">{app.name}</span>
                        <span class="mode-badge {app.exec_mode.includes('cluster') ? 'cluster' : 'fork'}">
                          {app.exec_mode.includes('cluster') ? `cluster (${app.instances})` : 'fork'}
                        </span>
                      </div>
                      <span class="state-pill {app.is_currently_running ? 'online' : 'inactive'}">
                        {app.is_currently_running ? `● Online (#${app.current_pm_id ?? '–'})` : '○ Saved (Offline)'}
                      </span>
                    </div>

                    <div class="saved-app-paths">
                      <div class="path-row">
                        <span class="path-label">Script</span>
                        <code class="path-code font-mono" title={app.script_path}>{app.script_path}</code>
                      </div>
                      {#if app.cwd}
                        <div class="path-row">
                          <span class="path-label">CWD</span>
                          <code class="path-code font-mono" title={app.cwd}>{app.cwd}</code>
                        </div>
                      {/if}
                    </div>

                    <div class="saved-app-footer">
                      <div class="saved-app-actions">
                        {#if !app.is_currently_running}
                          <Button
                            variant="primary"
                            size="xs"
                            onclick={() => handleStartSavedApp(app)}
                            title="Launch this saved application"
                          >
                            <Play size={12} />
                            <span>Run App</span>
                          </Button>
                        {:else}
                          <Button
                            variant="outline"
                            size="xs"
                            onclick={() => handleProcessAction('restart', app.current_pm_id!)}
                            title="Restart application"
                          >
                            <RotateCw size={12} />
                            <span>Restart</span>
                          </Button>
                          <Button
                            variant="outline"
                            size="xs"
                            onclick={() => handleProcessAction('stop', app.current_pm_id!)}
                            title="Stop application"
                          >
                            <Square size={12} />
                            <span>Stop</span>
                          </Button>
                        {/if}

                        <Button
                          variant="ghost"
                          size="xs"
                          onclick={() => promptDeleteSavedApp(app.name)}
                          title="Remove this app entry from dump.pm2"
                        >
                          <Trash2 size={12} class="text-error" />
                        </Button>
                      </div>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- SIDE DRAWER: APPLICATION DETAILS INSPECTOR -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if selectedProcess}
  <SideDrawer
    bind:isOpen={showProcessDrawer}
    title="{selectedProcess.name} (#{selectedProcess.pm_id})"
    width="640px"
  >
    {#snippet headerActions()}
      <div class="drawer-actions-row">
        {#if selectedProcess.status === 'online'}
          <Button
            variant="outline"
            size="xs"
            onclick={() => handleProcessAction('restart', selectedProcess!.pm_id)}
            title="Quick restart process"
          >
            <RotateCw size={12} />
            <span>Restart</span>
          </Button>
        {:else}
          <Button
            variant="primary"
            size="xs"
            onclick={() => handleProcessAction('start', selectedProcess!.pm_id)}
            title="Start process"
          >
            <Play size={12} />
            <span>Start</span>
          </Button>
        {/if}

        <KebabMenu align="right" title="Process Control Actions">
          <button class="menu-item" onclick={() => handleProcessAction('restart', selectedProcess!.pm_id)}>
            <RotateCw size={14} />
            <span>Restart Process</span>
          </button>

          <button class="menu-item" onclick={() => handleProcessAction('reload', selectedProcess!.pm_id)}>
            <Zap size={14} class="text-accent" />
            <span>0-Downtime Reload</span>
          </button>

          {#if selectedProcess.status === 'online'}
            <button class="menu-item" onclick={() => handleProcessAction('stop', selectedProcess!.pm_id)}>
              <Square size={14} class="text-amber" />
              <span>Stop Process</span>
            </button>
          {:else}
            <button class="menu-item" onclick={() => handleProcessAction('start', selectedProcess!.pm_id)}>
              <Play size={14} class="text-emerald" />
              <span>Start Process</span>
            </button>
          {/if}

          <button class="menu-item" onclick={() => promptFlushLogs(selectedProcess)}>
            <Trash2 size={14} />
            <span>Flush Log Files</span>
          </button>

          <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>

          <button class="menu-item text-danger" onclick={() => promptDeleteProcess(selectedProcess!)}>
            <Trash2 size={14} class="text-error" />
            <span>Delete Process</span>
          </button>
        </KebabMenu>
      </div>
    {/snippet}

    <div class="drawer-body-wrap">
      <!-- Top Overview Banner -->
      <div class="drawer-proc-overview">
        <div class="drawer-proc-main">
          <span class="status-dot {selectedProcess.status}"></span>
          <span class="drawer-proc-name font-mono">{selectedProcess.name}</span>
          <span class="status-badge status-{selectedProcess.status}">{selectedProcess.status}</span>
          <span class="detail-meta-chip font-mono">{selectedProcess.exec_mode}</span>
          {#if selectedProcess.pid}
            <span class="detail-meta-chip font-mono">PID {selectedProcess.pid}</span>
          {/if}
          {#if selectedProcess.watch}
            <span class="watch-badge">watch</span>
          {/if}
        </div>
        <div class="drawer-proc-path font-mono" title={selectedProcess.script_path}>
          {selectedProcess.script_path}
        </div>
      </div>

      <!-- Drawer Subtabs Bar -->
      <div class="drawer-subtabs">
        <button
          class="drawer-tab-btn"
          class:active={drawerTab === 'vitals'}
          onclick={() => drawerTab = 'vitals'}
        >
          <Activity size={13} />
          <span>Metrics & Vitals</span>
        </button>
        <button
          class="drawer-tab-btn"
          class:active={drawerTab === 'env'}
          onclick={() => drawerTab = 'env'}
        >
          <KeyRound size={13} />
          <span>Environment ({Object.keys(selectedProcess.env_vars || {}).length})</span>
        </button>
      </div>

      {#if drawerTab === 'vitals'}
        <!-- 4 Live Gauge Cards -->
        <div class="vitals-kpi-row">
          <div class="vital-card">
            <div class="vital-card-top">
              <span class="vital-card-title">CPU Utilization</span>
              <Cpu size={15} class="text-accent" />
            </div>
            <span class="vital-value font-mono text-accent">{selectedProcess.cpu.toFixed(1)}%</span>
            <div class="v-track">
              <div
                class="v-bar"
                style="width: {Math.min(100, selectedProcess.cpu)}%; background: {selectedProcess.cpu > 80 ? 'var(--color-error)' : selectedProcess.cpu > 40 ? 'var(--color-warning)' : 'var(--color-accent)'};"
              ></div>
            </div>
            <span class="vital-sub">Multi-core process load</span>
          </div>

          <div class="vital-card">
            <div class="vital-card-top">
              <span class="vital-card-title">Memory Heap</span>
              <HardDrive size={15} class="text-purple" />
            </div>
            <span class="vital-value font-mono text-purple">{formatBytes(selectedProcess.memory)}</span>
            <div class="v-track">
              <div
                class="v-bar"
                style="width: {Math.min(100, (selectedProcess.memory / (1024 * 1024 * 1024)) * 100)}%; background: #a855f7;"
              ></div>
            </div>
            <span class="vital-sub">RSS resident memory</span>
          </div>

          <div class="vital-card">
            <div class="vital-card-top">
              <span class="vital-card-title">Process Uptime</span>
              <Clock size={15} class="text-emerald" />
            </div>
            <span class="vital-value font-mono text-emerald">{selectedProcess.status === 'online' ? formatUptime(selectedProcess.uptime) : '–'}</span>
            <span class="vital-sub">PID: {selectedProcess.pid || '–'}</span>
          </div>

          <div class="vital-card">
            <div class="vital-card-top">
              <span class="vital-card-title">Restarts & Reliability</span>
              <RotateCw size={15} class={selectedProcess.unstable_restarts > 0 ? 'text-rose' : 'text-muted'} />
            </div>
            <span class="vital-value font-mono {selectedProcess.unstable_restarts > 0 ? 'text-rose' : ''}">
              {selectedProcess.restarts}
            </span>
            <span class="vital-sub {selectedProcess.unstable_restarts > 0 ? 'text-rose font-semibold' : ''}">
              {selectedProcess.unstable_restarts > 0 ? `${selectedProcess.unstable_restarts} unstable restarts!` : 'Stable runtime'}
            </span>
          </div>
        </div>

        <!-- Deep Execution Info Card -->
        <div class="vitals-info-card">
          <div class="card-section-header">
            <FileCode size={14} class="text-accent" />
            <h3>Execution Paths & Configuration</h3>
          </div>

          <div class="details-list">
            <div class="detail-row">
              <span class="detail-label">Script Entrypoint</span>
              <div class="detail-code-wrap">
                <code class="detail-code">{selectedProcess.script_path}</code>
                <button class="mini-copy-btn" onclick={() => copyValue(selectedProcess!.script_path, 'Script Path')}>
                  <Copy size={11} />
                </button>
              </div>
            </div>

            <div class="detail-row">
              <span class="detail-label">Working Directory (CWD)</span>
              <div class="detail-code-wrap">
                <code class="detail-code">{selectedProcess.cwd || '–'}</code>
                {#if selectedProcess.cwd}
                  <button class="mini-copy-btn" onclick={() => copyValue(selectedProcess!.cwd, 'Working Directory')}>
                    <Copy size={11} />
                  </button>
                {/if}
              </div>
            </div>

            <div class="detail-row">
              <span class="detail-label">Execution Mode</span>
              <span class="detail-val font-mono">{selectedProcess.exec_mode} ({selectedProcess.instances} instances)</span>
            </div>

            <div class="detail-row">
              <span class="detail-label">Watch Mode</span>
              <span class="detail-val">{selectedProcess.watch ? 'Enabled (auto-restarts on code change)' : 'Disabled'}</span>
            </div>

            {#if selectedProcess.args && selectedProcess.args.length > 0}
              <div class="detail-row">
                <span class="detail-label">Arguments</span>
                <code class="detail-code">{selectedProcess.args.join(' ')}</code>
              </div>
            {/if}
          </div>
        </div>

        <!-- Log Files Card -->
        <div class="vitals-info-card">
          <div class="card-section-header">
            <FileText size={14} class="text-accent" />
            <h3>Log File Locations</h3>
          </div>

          <div class="details-list">
            <div class="detail-row">
              <span class="detail-label">Stdout Log Path</span>
              <div class="detail-code-wrap">
                <code class="detail-code">{selectedProcess.out_log_path}</code>
                <button class="mini-copy-btn" onclick={() => copyValue(selectedProcess!.out_log_path, 'Stdout Log Path')}>
                  <Copy size={11} />
                </button>
              </div>
            </div>

            <div class="detail-row">
              <span class="detail-label">Stderr Log Path</span>
              <div class="detail-code-wrap">
                <code class="detail-code">{selectedProcess.err_log_path}</code>
                <button class="mini-copy-btn" onclick={() => copyValue(selectedProcess!.err_log_path, 'Stderr Log Path')}>
                  <Copy size={11} />
                </button>
              </div>
            </div>

            <div class="detail-row">
              <span class="detail-label">PM2 Home Dir</span>
              <code class="detail-code">{systemStatus?.pm2_home}</code>
            </div>

            <div class="detail-row">
              <span class="detail-label">Node Runtime</span>
              <span class="detail-val font-mono">{systemStatus?.node_version || 'Node.js'}</span>
            </div>
          </div>
        </div>

      {:else if drawerTab === 'env'}
        <!-- Environment Variables -->
        <div class="env-tab-wrap">
          <div class="env-tab-toolbar">
            <SearchBar
              bind:value={envSearchQuery}
              placeholder="Search environment keys or values..."
              style="margin: 0; width: 100%;"
            />
          </div>

          {#if filteredEnvVars.length === 0}
            <div class="empty-env-box">
              <KeyRound size={26} class="text-muted" />
              <p>No environment variables match the search filter.</p>
            </div>
          {:else}
            <div class="env-table-wrap">
              <Table tableAction={tableFeatures} showDensityToggle={false} style="border:none; border-radius:0; table-layout: fixed; width: 100%;">
                <thead>
                  <tr>
                    <th style="width: 35%; min-width: 140px;">Variable Name</th>
                    <th style="width: 60%;">Value</th>
                    <th style="width: 5%; min-width: 40px; text-align: right;"></th>
                  </tr>
                </thead>
                <tbody>
                  {#each filteredEnvVars as [k, v]}
                    <tr>
                      <td class="font-mono" style="font-weight: 600; color: var(--color-accent); vertical-align: top; padding-top: 9px;">
                        {k}
                      </td>
                      <td class="font-mono" style="word-break: break-all; vertical-align: top; padding-top: 9px;" title={v || ''}>
                        {#if v === '' || v === undefined || v === null}
                          <span class="empty-env-tag font-mono">(empty string)</span>
                        {:else}
                          <span>{v}</span>
                        {/if}
                      </td>
                      <td style="text-align: right; vertical-align: top; padding-top: 6px;">
                        <button
                          class="action-btn"
                          onclick={() => copyValue(v || '', k)}
                          title="Copy value"
                        >
                          <Copy size={13} />
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </Table>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </SideDrawer>
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: LAUNCH NEW APP (3-STEP GUIDED WIZARD) -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showLaunchModal}
  <div use:portal class="modal-backdrop" onclick={() => showLaunchModal = false} role="presentation">
    <div class="modal-card modal-wizard-card" onclick={(e) => e.stopPropagation()} role="dialog">
      <!-- Modal Header -->
      <div class="modal-header">
        <div class="modal-title-wrap">
          <Plus size={18} class="text-accent" />
          <h3>Launch Application</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showLaunchModal = false}>×</button>
      </div>

      <!-- Stepper Progress Bar (Reusable Component) -->
      <Stepper
        bind:currentStep={launchStep}
        steps={[
          { label: 'Target & Script' },
          { label: 'Scaling & Limits' },
          { label: 'Environment & CLI' }
        ]}
        onchange={(s) => {
          if (s > 1 && !launchForm.script_path.trim()) {
            uiStore.showToast('Please specify a script path first', 'warning');
            launchStep = 1;
          }
        }}
      />

      <!-- Modal Body (Step Specific) -->
      <div class="modal-body wizard-body">
        {#if launchStep === 1}
          <!-- ─── STEP 1: TARGET & SCRIPT ────────────────────────────── -->
          <div class="step-pane">
            <div class="form-group">
              <label for="script-path-input" class="form-label">Script / Entrypoint Path <span class="text-rose">*</span></label>
              <div class="input-browse-wrap">
                <input
                  id="script-path-input"
                  type="text"
                  placeholder="/var/www/my-app/dist/main.js, ./server.js, or script.py"
                  bind:value={launchForm.script_path}
                  class="form-input"
                />
                <Button variant="outline" onclick={browseScriptFile} title="Browse for executable script file">
                  <Folder size={14} class="text-accent" />
                  <span>Browse...</span>
                </Button>
              </div>

              {#if detectedRuntime}
                <div class="runtime-badge-pill">
                  <span>Detected:</span>
                  <strong>{detectedRuntime.badge}</strong>
                  <span class="text-muted">({detectedRuntime.label})</span>
                </div>
              {:else}
                <small class="form-help">Supported: JavaScript (*.js, *.mjs, *.cjs), TypeScript (*.ts), Python (*.py), Shell (*.sh), or Ecosystem JSON.</small>
              {/if}
            </div>

            <div class="form-row">
              <!-- App Name -->
              <div class="form-group">
                <label for="app-name-input" class="form-label">Application Name</label>
                <input
                  id="app-name-input"
                  type="text"
                  placeholder="e.g. billing-backend"
                  bind:value={launchForm.name}
                  class="form-input"
                />
                <small class="form-help">Process name in PM2 table (auto-inferred from folder if empty)</small>
              </div>

              <!-- Working Directory with Browse Button -->
              <div class="form-group">
                <label for="app-cwd-input" class="form-label">Working Directory (CWD)</label>
                <div class="input-browse-wrap">
                  <input
                    id="app-cwd-input"
                    type="text"
                    placeholder="e.g. /var/www/my-app"
                    bind:value={launchForm.cwd}
                    class="form-input"
                  />
                  <Button variant="outline" onclick={browseWorkingDir} title="Browse for Working Directory">
                    <Folder size={14} />
                  </Button>
                </div>
                <small class="form-help">Root execution folder</small>
              </div>
            </div>

            <div class="step-info-card">
              <Sparkles size={16} class="text-accent flex-shrink-0" />
              <div class="text-xs text-secondary">
                Need to launch fast? Click <strong>Quick Launch</strong> to start immediately with standard defaults, or click <strong>Next</strong> to configure cluster instances, memory limits, and environment variables.
              </div>
            </div>
          </div>

        {:else if launchStep === 2}
          <!-- ─── STEP 2: SCALING & RESOURCES ────────────────────────── -->
          <div class="step-pane">
            <div class="form-group">
              <label class="form-label">Execution Mode</label>
              <div class="mode-card-grid">
                <button
                  type="button"
                  class="mode-card"
                  class:active={launchForm.exec_mode === 'fork'}
                  onclick={() => launchForm.exec_mode = 'fork'}
                >
                  <div class="mode-card-header">
                    <Zap size={16} class="text-amber" />
                    <strong>Fork Mode</strong>
                  </div>
                  <span class="mode-card-desc">Single-instance execution. Best for background workers, cron scripts, Python, or standard Node.js scripts.</span>
                </button>

                <button
                  type="button"
                  class="mode-card"
                  class:active={launchForm.exec_mode === 'cluster'}
                  onclick={() => launchForm.exec_mode = 'cluster'}
                >
                  <div class="mode-card-header">
                    <Layers size={16} class="text-accent" />
                    <strong>Cluster Mode</strong>
                  </div>
                  <span class="mode-card-desc">Multi-core load balancing with zero-downtime reloads. Best for HTTP APIs (NestJS, Express, etc.).</span>
                </button>
              </div>
            </div>

            {#if launchForm.exec_mode === 'cluster'}
              <div class="form-group" style="margin-top: 4px;">
                <label for="instances-input" class="form-label">CPU Instances (0 = All Cores / Max)</label>
                <div style="display: flex; align-items: center; gap: 10px;">
                  <input
                    id="instances-input"
                    type="number"
                    min="0"
                    max="64"
                    bind:value={launchForm.instances}
                    class="form-input font-mono"
                    style="width: 120px;"
                  />
                  <span class="text-xs text-muted">Set <code>0</code> or <code>max</code> to scale across all detected CPU cores automatically.</span>
                </div>
              </div>
            {/if}

            <div class="form-row" style="margin-top: 4px;">
              <div class="form-group">
                <label for="max-mem-input" class="form-label">Max Memory Restart Limit</label>
                <input
                  id="max-mem-input"
                  type="text"
                  placeholder="500M, 1G, 2G"
                  bind:value={launchForm.max_memory_restart}
                  class="form-input font-mono"
                />
                <div class="preset-pill-row">
                  {#each ['256M', '512M', '1G', '2G'] as mem}
                    <button
                      type="button"
                      class="preset-pill {launchForm.max_memory_restart === mem ? 'active' : ''}"
                      onclick={() => launchForm.max_memory_restart = mem}
                    >
                      {mem}
                    </button>
                  {/each}
                </div>
              </div>

              <div class="form-group">
                <label class="form-label">File Watch Mode</label>
                <label class="toggle-card-label">
                  <input type="checkbox" bind:checked={launchForm.watch} />
                  <div>
                    <strong>Watch Mode</strong>
                    <p class="text-xs text-muted" style="margin: 2px 0 0 0;">Auto-restart when files change in CWD</p>
                  </div>
                </label>
              </div>
            </div>
          </div>

        {:else if launchStep === 3}
          <!-- ─── STEP 3: ENVIRONMENT & CLI ──────────────────────────── -->
          <div class="step-pane">
            <div class="form-group">
              <label for="app-args-input" class="form-label">CLI Arguments (Passed to process)</label>
              <input
                id="app-args-input"
                type="text"
                placeholder="--port 8080 --verbose --env staging"
                bind:value={launchForm.args}
                class="form-input font-mono"
              />
            </div>

            <!-- Environment Variables Section with Presets & Bulk Import -->
            <div class="env-section">
              <div class="env-section-header">
                <div>
                  <span class="form-label" style="font-weight: 600;">Environment Variables</span>
                  <span class="text-muted" style="font-size: 11px; margin-left: 6px;">({launchForm.env_vars.length} configured)</span>
                </div>
                <div style="display: flex; gap: 6px;">
                  <Button variant="ghost" size="xs" onclick={() => showBulkEnvInput = !showBulkEnvInput}>
                    <FileText size={12} />
                    <span>{showBulkEnvInput ? 'Cancel Paste' : 'Paste .env'}</span>
                  </Button>
                  <Button variant="ghost" size="xs" onclick={addEnvRow}>+ Add Variable</Button>
                </div>
              </div>

              {#if showBulkEnvInput}
                <div class="bulk-env-box">
                  <textarea
                    bind:value={bulkEnvText}
                    placeholder="Paste .env contents here (e.g. PORT=3000&#10;NODE_ENV=production)"
                    class="form-textarea font-mono text-xs"
                    rows="4"
                  ></textarea>
                  <div style="display: flex; justify-content: flex-end; gap: 6px; margin-top: 6px;">
                    <Button variant="ghost" size="xs" onclick={() => showBulkEnvInput = false}>Close</Button>
                    <Button variant="primary" size="xs" onclick={parseAndApplyBulkEnv}>Import Variables</Button>
                  </div>
                </div>
              {/if}

              <!-- Quick Env Presets -->
              <div class="preset-pill-row">
                <span style="font-size: 10px; color: var(--color-text-muted); font-weight: 600;">Quick Presets:</span>
                <button type="button" class="preset-pill" onclick={() => addEnvPreset('NODE_ENV', 'production')}>+ NODE_ENV=production</button>
                <button type="button" class="preset-pill" onclick={() => addEnvPreset('PORT', '3000')}>+ PORT=3000</button>
                <button type="button" class="preset-pill" onclick={() => addEnvPreset('PORT', '8080')}>+ PORT=8080</button>
              </div>

              <div class="env-rows-scroll">
                {#each launchForm.env_vars as env, idx}
                  <div class="modal-env-row">
                    <input
                      type="text"
                      placeholder="KEY (e.g. PORT)"
                      bind:value={env.key}
                      class="form-input form-input-sm font-mono"
                    />
                    <input
                      type="text"
                      placeholder="VALUE (e.g. 3000)"
                      bind:value={env.value}
                      class="form-input form-input-sm font-mono"
                    />
                    <button class="btn-remove-row text-rose" onclick={() => removeEnvRow(idx)} title="Remove variable">×</button>
                  </div>
                {/each}
              </div>
            </div>

            <!-- Pre-flight PM2 Command Preview Box -->
            <div class="terminal-preview-box">
              <div class="terminal-header">
                <div style="display: flex; align-items: center; gap: 6px;">
                  <Terminal size={13} style="color: #38bdf8;" />
                  <span>Pre-flight Command Preview</span>
                </div>
                <button
                  type="button"
                  class="terminal-copy-btn"
                  onclick={() => copyToClipboard(livePm2Command, 'preview-cmd')}
                  title="Copy command to clipboard"
                >
                  {#if copiedCommand === 'preview-cmd'}
                    <Check size={12} style="color: #34d399;" />
                    <span style="color: #34d399;">Copied</span>
                  {:else}
                    <Copy size={12} />
                    <span>Copy</span>
                  {/if}
                </button>
              </div>
              <div class="terminal-code font-mono">
                <span class="terminal-prompt">$</span>
                <span class="terminal-cmd">{livePm2Command}</span>
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Modal Footer (Step-aware) -->
      <div class="modal-footer">
        {#if launchStep === 1}
          <Button variant="ghost" onclick={() => showLaunchModal = false}>Cancel</Button>
          <div style="display: flex; gap: 8px; margin-left: auto;">
            <Button
              variant="outline"
              onclick={launchProcess}
              disabled={!launchForm.script_path.trim()}
              title="Launch immediately with standard defaults"
            >
              <Zap size={13} class="text-amber" />
              <span>Quick Launch</span>
            </Button>
            <Button
              variant="primary"
              onclick={() => {
                if (!launchForm.script_path.trim()) {
                  uiStore.showToast('Please specify a script path first', 'warning');
                  return;
                }
                launchStep = 2;
              }}
            >
              <span>Next: Scaling &amp; Limits &rarr;</span>
            </Button>
          </div>
        {:else if launchStep === 2}
          <Button variant="outline" onclick={() => launchStep = 1}>&larr; Back</Button>
          <div style="display: flex; gap: 8px; margin-left: auto;">
            <Button
              variant="outline"
              onclick={launchProcess}
              title="Launch with settings configured so far"
            >
              <Zap size={13} class="text-amber" />
              <span>Launch Now</span>
            </Button>
            <Button variant="primary" onclick={() => launchStep = 3}>
              <span>Next: Environment &amp; CLI &rarr;</span>
            </Button>
          </div>
        {:else if launchStep === 3}
          <Button variant="outline" onclick={() => launchStep = 2}>&larr; Back</Button>
          <div style="display: flex; gap: 8px; margin-left: auto;">
            <Button variant="primary" onclick={launchProcess}>
              <Play size={13} />
              <span>Launch Application</span>
            </Button>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: NEW ECOSYSTEM FILE -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showNewEcosystemModal}
  <div use:portal class="modal-backdrop" onclick={() => showNewEcosystemModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <FileCode size={18} class="text-accent" />
          <h3>Create Ecosystem Configuration</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showNewEcosystemModal = false}>×</button>
      </div>

      <div class="modal-body">
        <div class="form-group">
          <label for="eco-path-input" class="form-label">Destination File Path *</label>
          <div class="input-browse-wrap">
            <input
              id="eco-path-input"
              type="text"
              placeholder="/var/www/my-project/ecosystem.config.js"
              bind:value={newEcosystemPath}
              class="form-input"
            />
            <Button variant="outline" onclick={browseEcosystemDestination} title="Browse destination directory">
              <Folder size={14} class="text-accent" />
              <span>Browse...</span>
            </Button>
          </div>
          <small class="form-help">Absolute path where ecosystem.config.js will be created.</small>
        </div>

        <div class="form-group">
          <label for="template-select" class="form-label">Preset Architecture Template</label>
          <Select id="template-select" bind:value={newEcosystemTemplate}>
            <option value="express">Express.js API Server (Cluster Mode)</option>
            <option value="nest">NestJS Microservice (Production Cluster)</option>
            <option value="next">Next.js SSR Application</option>
            <option value="microservice">Background Worker Script (Fork Mode)</option>
          </Select>
        </div>
      </div>

      <div class="modal-footer">
        <Button variant="outline" onclick={() => showNewEcosystemModal = false}>Cancel</Button>
        <Button variant="primary" onclick={createNewEcosystemFile}>
          <FileCode size={13} />
          <span>Create Configuration</span>
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- Universal Config Diff Modal for PM2 Ecosystem -->
{#if selectedEcosystemFile}
  <ConfigDiffModal
    bind:show={showEcosystemDiffModal}
    filePath={selectedEcosystemFile.path}
    title={`Review ${selectedEcosystemFile.name} Changes`}
    oldContent={originalEcosystemContent}
    newContent={ecosystemContent}
    warningMessage="Ensure module export syntax and process names are valid before saving."
    onconfirm={async () => {
      await saveEcosystemFile();
      showEcosystemDiffModal = false;
    }}
    oncancel={() => showEcosystemDiffModal = false}
  />
{/if}

<style>
  /* Root Module Page */
  .module-page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px;
    position: absolute;
    inset: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    background: var(--color-bg-base);
    box-sizing: border-box;
  }

  .header-action-btns {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Version Badges Group */
  .version-badges-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .v-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }

  .v-pill.success {
    background: var(--color-success-muted, rgba(16, 185, 129, 0.12));
    color: var(--color-success, #10b981);
    border: 1px solid var(--color-success, #10b981);
  }

  .v-pill.neutral {
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }

  /* Controls Row */
  .controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }

  /* 4 Top KPI Cards */
  .kpi-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    flex-shrink: 0;
  }

  @media (max-width: 1100px) {
    .kpi-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  /* Main Workspace Body */
  .pm2-workspace-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* MASTER-DETAIL SPLIT CONTAINER */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .fleet-split-container {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /* ── MASTER PANE (LEFT) ── */
  .fleet-master-pane {
    width: 360px;
    flex-shrink: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  }

  .master-header {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }

  .master-pills-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .pills-group {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    flex: 1;
    min-width: 0;
  }

  .pill-btn {
    padding: 3px 8px;
    border-radius: 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
    display: inline-flex;
    align-items: center;
    white-space: nowrap;
  }

  .pill-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .pill-btn.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
    border-color: var(--color-accent);
    font-weight: 600;
  }

  .empty-list-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .resurrect-hint-row {
    display: flex;
    justify-content: center;
  }

  /* Process Cards List */
  .master-cards-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Single Process Card */
  .proc-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
    text-align: left;
    position: relative;
  }

  .proc-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border);
  }

  .proc-card.selected {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
    box-shadow: 0 0 0 1px var(--color-accent), 0 4px 12px rgba(0, 218, 243, 0.08);
  }

  .proc-card.errored {
    border-color: rgba(244, 63, 94, 0.4);
  }

  .proc-card.errored.selected {
    border-color: var(--color-error);
    box-shadow: 0 0 0 1px var(--color-error), 0 4px 12px rgba(244, 63, 94, 0.1);
  }

  .proc-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .proc-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }

  .proc-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .proc-id-tag {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-surface);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--color-border-subtle);
  }

  .proc-version-tag {
    font-size: 10px;
    color: var(--color-text-secondary);
    background: var(--color-bg-surface);
    padding: 1px 4px;
    border-radius: 4px;
  }

  .mode-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }

  .mode-tag.cluster {
    background: rgba(168, 85, 247, 0.12);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }

  .mode-tag.fork {
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
  }

  .proc-path-line {
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Mini Meters */
  .proc-meters-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    background: var(--color-bg-surface);
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--color-border-subtle);
  }

  .meter-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .meter-label-row {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
  }

  .m-label {
    color: var(--color-text-muted);
    font-weight: 500;
  }

  .m-val {
    color: var(--color-text-secondary);
    font-weight: 600;
  }

  .mini-track {
    height: 4px;
    background: var(--color-bg-raised);
    border-radius: 2px;
    overflow: hidden;
  }

  .mini-bar {
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .mem-bar {
    background: #a855f7;
  }

  /* Card Footer */
  .proc-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 4px;
    border-top: 1px solid var(--color-border-subtle);
  }

  .proc-meta-tags {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .meta-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .proc-hover-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    opacity: 0.85;
  }

  .proc-card:hover .proc-hover-actions {
    opacity: 1;
  }

  .card-action-btn {
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 4px;
    color: var(--color-text-muted);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .card-action-btn:hover {
    transform: scale(1.12);
  }

  /* Specific Action Colors & Glow on Hover */
  .card-action-btn.action-restart:hover {
    color: #38bdf8;
    background: rgba(56, 189, 248, 0.14);
    border-color: rgba(56, 189, 248, 0.35);
    box-shadow: 0 0 8px rgba(56, 189, 248, 0.2);
  }

  .card-action-btn.action-reload:hover {
    color: var(--color-accent, #00daf3);
    background: rgba(0, 218, 243, 0.14);
    border-color: rgba(0, 218, 243, 0.35);
    box-shadow: 0 0 8px rgba(0, 218, 243, 0.2);
  }

  .card-action-btn.action-stop:hover {
    color: #f59e0b;
    background: rgba(245, 158, 11, 0.14);
    border-color: rgba(245, 158, 11, 0.35);
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.2);
  }

  .card-action-btn.action-start:hover {
    color: #10b981;
    background: rgba(16, 185, 129, 0.14);
    border-color: rgba(16, 185, 129, 0.35);
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.2);
  }

  .card-action-btn.action-delete:hover {
    color: #f43f5e;
    background: rgba(244, 63, 94, 0.14);
    border-color: rgba(244, 63, 94, 0.35);
    box-shadow: 0 0 8px rgba(244, 63, 94, 0.2);
  }

  /* Status Dot */
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    position: relative;
  }

  .status-dot.online {
    background: var(--color-success, #10b981);
  }

  .status-dot.stopped {
    background: var(--color-text-muted, #71717a);
  }

  .status-dot.errored {
    background: var(--color-error, #f43f5e);
  }

  .dot-pulse {
    position: absolute;
    inset: -3px;
    border-radius: 50%;
    border: 1px solid var(--color-success, #10b981);
    animation: ping 2s cubic-bezier(0, 0, 0.2, 1) infinite;
  }

  @keyframes ping {
    75%, 100% {
      transform: scale(2);
      opacity: 0;
    }
  }

  /* ── DETAIL PANE (RIGHT) ── */
  .fleet-detail-pane {
    flex: 1;
    min-width: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  }

  .logs-proc-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    border-right: 1px solid var(--color-border-subtle);
  }

  .logs-proc-name {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .logs-proc-id {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--color-border-subtle);
  }

  .logs-proc-pid {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  /* Drawer Styles */
  .drawer-actions-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .drawer-body-wrap {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 0;
  }

  .drawer-proc-overview {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .drawer-proc-main {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .drawer-proc-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .drawer-proc-path {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  .drawer-subtabs {
    display: flex;
    gap: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
    padding-bottom: 8px;
  }

  .drawer-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .drawer-tab-btn:hover {
    color: var(--color-text-primary);
  }

  .drawer-tab-btn.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
    border-color: var(--color-accent);
  }

  .detail-meta-chip {
    font-size: 11px;
    color: var(--color-text-secondary);
    background: var(--color-bg-raised);
    padding: 1px 6px;
    border-radius: 4px;
  }

  .watch-badge {
    font-size: 10px;
    font-weight: 600;
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.3);
    padding: 1px 6px;
    border-radius: 4px;
  }

  /* ── TAB 1: LOGS DECK ── */
  .logs-deck-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .logs-deck-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: nowrap;
    min-height: 32px;
    position: relative;
    z-index: 30;
  }

  .logs-controls-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: visible;
  }

  .logs-controls-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .menu-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    border-radius: 6px;
    transition: background 0.12s ease, color 0.12s ease;
    text-align: left;
    font-family: inherit;
    white-space: nowrap;
  }

  .menu-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .menu-item.text-danger {
    color: var(--color-error);
  }

  .menu-item.text-danger:hover {
    background: rgba(244, 63, 94, 0.12);
    color: var(--color-error);
  }

  .select-field {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .field-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .segmented-control {
    display: flex;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
  }

  .seg-btn {
    padding: 4px 10px;
    background: transparent;
    border: none;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .seg-btn.active {
    background: var(--color-bg-surface);
    color: var(--color-accent);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
  }

  /* Terminal Log Card */
  .log-stream-card {
    flex: 1;
    min-height: 0;
    background: #090d13;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .log-stream-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: #111722;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .log-source-info {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: #94a3b8;
    overflow: hidden;
  }

  .log-source-path {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .log-count-tag {
    font-size: 10px;
    color: #64748b;
  }

  .log-lines-container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 12px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .log-table {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .log-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: #cbd5e1;
    padding: 1px 0;
  }

  .log-row:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .row-error {
    color: #fda4af;
  }

  .row-warn {
    color: #fde047;
  }

  .log-line-no {
    color: #475569;
    min-width: 32px;
    text-align: right;
    user-select: none;
    font-size: 10.5px;
  }

  .log-ts {
    color: #64748b;
    font-size: 10.5px;
    flex-shrink: 0;
  }

  .log-level-badge {
    font-size: 9px;
    font-weight: 700;
    padding: 0 4px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .level-err {
    background: rgba(244, 63, 94, 0.2);
    color: #fb7185;
    border: 1px solid rgba(244, 63, 94, 0.4);
  }

  .level-warn {
    background: rgba(234, 179, 8, 0.2);
    color: #facc15;
    border: 1px solid rgba(234, 179, 8, 0.4);
  }

  .log-msg {
    flex: 1;
    word-break: break-all;
    white-space: pre-wrap;
  }

  .log-state-msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 180px;
    gap: 10px;
    color: #64748b;
    font-size: 12px;
  }

  /* ── TAB 2: VITALS & METRICS ── */
  .vitals-tab-wrap {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-right: 4px;
  }

  .vitals-kpi-row {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }

  @media (max-width: 1200px) {
    .vitals-kpi-row {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .vital-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .vital-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .vital-card-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .vital-value {
    font-size: 20px;
    font-weight: 700;
  }

  .vital-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .v-track {
    height: 5px;
    background: var(--color-bg-surface);
    border-radius: 3px;
    overflow: hidden;
    margin-top: 2px;
  }

  .v-bar {
    height: 100%;
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .vitals-sections-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  @media (max-width: 1100px) {
    .vitals-sections-grid {
      grid-template-columns: 1fr;
    }
  }

  .vitals-info-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-section-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-section-header h3 {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0;
  }

  .details-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    gap: 12px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .detail-label {
    color: var(--color-text-muted);
    font-weight: 500;
    flex-shrink: 0;
  }

  .detail-val {
    color: var(--color-text-primary);
    font-weight: 600;
    text-align: right;
  }

  .detail-code-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .detail-code {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    background: var(--color-bg-surface);
    padding: 2px 6px;
    border-radius: 4px;
    border: 1px solid var(--color-border-subtle);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 260px;
  }

  .mini-copy-btn {
    background: transparent;
    border: none;
    padding: 2px 4px;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 3px;
  }

  .mini-copy-btn:hover {
    color: var(--color-accent);
    background: var(--color-bg-hover);
  }

  /* ── TAB 3: ENVIRONMENT TAB ── */
  .env-tab-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .env-tab-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .env-count-pill {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid var(--color-border-subtle);
  }

  .env-table-wrap {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    background: var(--color-bg-raised);
  }

  .env-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .env-table th {
    padding: 8px 12px;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    font-weight: 600;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--color-border-subtle);
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .env-table td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-border-subtle);
    vertical-align: middle;
  }

  .env-key {
    color: var(--color-accent);
    font-weight: 600;
  }

  .env-val {
    color: var(--color-text-primary);
    word-break: break-all;
  }

  .empty-env-tag {
    color: var(--color-text-muted);
    font-style: italic;
    font-size: 11px;
    opacity: 0.7;
  }

  .env-copy-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 3px;
    border-radius: 4px;
  }

  .env-copy-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-hover);
  }

  .empty-env-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 180px;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 12px;
  }

  /* No Selection Wrap */
  .no-selection-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    text-align: center;
    color: var(--color-text-muted);
  }

  .no-selection-wrap h3 {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0;
  }

  .no-selection-wrap p {
    font-size: 13px;
    max-width: 360px;
    margin: 0;
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* STATUS BADGES & PILLS */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: capitalize;
  }

  .status-online {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }

  .status-stopped {
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border);
  }

  .status-errored {
    background: rgba(244, 63, 94, 0.12);
    color: #fb7185;
    border: 1px solid rgba(244, 63, 94, 0.3);
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.4; transform: scale(0.85); }
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* VIEW 2: ECOSYSTEM CONFIG STUDIO */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .ecosystem-layout {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .ecosystem-sidebar {
    width: 280px;
    flex-shrink: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
  }

  .ecosystem-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .ecosystem-sidebar-header h3 {
    font-size: 13px;
    font-weight: 600;
    margin: 0;
  }

  .files-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    flex: 1;
  }

  .file-item-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text-primary);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .file-item-btn:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border);
  }

  .file-item-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
  }

  .file-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .file-name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-path-sub {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ecosystem-editor-panel {
    flex: 1;
    min-width: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }

  .editor-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .editor-file-title {
    font-size: 12px;
    font-weight: 600;
  }

  .dirty-badge {
    font-size: 10px;
    font-weight: 600;
    background: rgba(234, 179, 8, 0.15);
    color: #eab308;
    border: 1px solid rgba(234, 179, 8, 0.3);
    padding: 1px 6px;
    border-radius: 4px;
  }

  .editor-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .code-editor-wrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .editor-empty, .sidebar-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 200px;
    gap: 8px;
    text-align: center;
    color: var(--color-text-muted);
    padding: 20px;
  }

  .editor-empty h3 {
    font-size: 14px;
    color: var(--color-text-primary);
    margin: 0;
  }

  .editor-empty p, .sidebar-empty p {
    font-size: 12px;
    max-width: 320px;
    margin: 0;
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* VIEW 3: DAEMON RUNTIME & STARTUP */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .daemon-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    overflow-y: auto;
  }

  @media (max-width: 1000px) {
    .daemon-grid {
      grid-template-columns: 1fr;
    }
  }

  .daemon-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .daemon-card-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .icon-bubble {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: var(--color-accent-muted);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .daemon-card-header h3 {
    font-size: 14px;
    font-weight: 600;
    margin: 0;
  }

  .daemon-card-header p {
    font-size: 12px;
    color: var(--color-text-muted);
    margin: 0;
  }

  .card-footer-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }

  .startup-hint-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    padding: 12px;
  }

  .hint-title {
    font-size: 12px;
    color: var(--color-text-secondary);
    margin: 0;
  }

  .badge {
    display: inline-flex;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }

  .badge-success {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }

  .badge-neutral {
    background: var(--color-bg-surface);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-subtle);
  }

  /* Dump Catalog Full-Width Card */
  .dump-catalog-card {
    grid-column: 1 / -1;
  }

  .catalog-header-flex {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    gap: 12px;
  }

  .catalog-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .catalog-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 30px 20px;
    text-align: center;
    gap: 8px;
    color: var(--color-text-muted);
  }

  .catalog-empty h4 {
    font-size: 14px;
    color: var(--color-text-primary);
    margin: 0;
  }

  .catalog-empty p {
    font-size: 12px;
    margin: 0;
  }

  .saved-apps-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }

  .saved-app-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: all 0.15s ease;
  }

  .saved-app-card:hover {
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
  }

  .saved-app-card.running {
    border-color: rgba(16, 185, 129, 0.3);
  }

  .saved-app-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .saved-app-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }

  .saved-app-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .state-pill {
    font-size: 10.5px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 6px;
    white-space: nowrap;
  }

  .state-pill.online {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }

  .state-pill.inactive {
    background: var(--color-bg-surface);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-subtle);
  }

  .saved-app-paths {
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    padding: 8px 10px;
  }

  .path-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    overflow: hidden;
  }

  .path-label {
    font-weight: 600;
    color: var(--color-text-muted);
    width: 38px;
    flex-shrink: 0;
  }

  .path-code {
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
  }

  .saved-app-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: 2px;
  }

  .saved-app-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* Quick Saved Dump Runner in Fleet Master List */
  .saved-dump-quick-section {
    width: 100%;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }

  .quick-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .quick-section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .quick-dump-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .quick-dump-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    padding: 6px 8px;
  }

  .quick-item-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
  }

  .quick-item-name {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .quick-item-path {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* NOT INSTALLED HERO CARD */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .not-installed-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-width: 800px;
    margin: 0 auto;
  }

  .not-installed-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
  }

  .hero-icon-ring {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: rgba(234, 179, 8, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .not-installed-hero h2 {
    font-size: 18px;
    font-weight: 700;
    margin: 0;
  }

  .hero-subtext {
    font-size: 13px;
    color: var(--color-text-muted);
    max-width: 540px;
    margin: 0;
    line-height: 1.5;
  }

  .install-commands-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .command-box {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .command-box-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .command-box-title {
    font-size: 12px;
    font-weight: 600;
  }

  .command-tag {
    font-size: 10px;
    background: var(--color-bg-surface);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--color-text-muted);
  }

  .command-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin: 0;
  }

  .code-terminal {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #090d13;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 6px 10px;
  }

  .code-terminal code {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: #38bdf8;
  }

  .copy-btn {
    background: transparent;
    border: none;
    color: #94a3b8;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
  }

  .copy-btn:hover {
    color: #f8fafc;
  }

  .not-installed-footer {
    display: flex;
    justify-content: center;
    padding-top: 8px;
  }

  /* ═════════════════════════════════════════════════════════════════════════ */
  /* MODALS & STEPPER WIZARD */
  /* ═════════════════════════════════════════════════════════════════════════ */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
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
    overflow: hidden;
  }

  .modal-wizard-card {
    width: 704px;
    max-width: calc(100vw - 32px);
    height: 600px;
    max-height: 92vh;
  }

  .modal-header {
    padding: 14px 20px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border-subtle);
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

  /* Mode selection cards */
  .mode-card-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .mode-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--color-bg-raised);
    border: 1.5px solid var(--color-border);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .mode-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border-hover);
  }

  .mode-card.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
    box-shadow: 0 0 12px rgba(0, 218, 243, 0.15);
  }

  .mode-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .mode-card-desc {
    font-size: 11px;
    color: var(--color-text-secondary);
    line-height: 1.35;
  }

  .toggle-card-label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
  }

  .toggle-card-label strong {
    font-size: 12px;
    color: var(--color-text-primary);
  }

  /* Pre-flight Terminal box */
  .terminal-preview-box {
    background: #0b1320;
    border: 1px solid #1e293b;
    border-radius: 10px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: auto;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .terminal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #94a3b8;
  }

  .terminal-copy-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 2px 7px;
    font-size: 10.5px;
    color: #cbd5e1;
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .terminal-copy-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.25);
  }

  .terminal-code {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 12px;
    line-height: 1.5;
    background: rgba(0, 0, 0, 0.35);
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.05);
    max-height: 80px;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .terminal-prompt {
    color: #38bdf8;
    font-weight: 700;
    user-select: none;
    flex-shrink: 0;
  }

  .terminal-cmd {
    color: #f1f5f9;
    word-break: break-all;
    white-space: pre-wrap;
    font-weight: 500;
  }

  .runtime-badge-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: 4px;
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
    color: var(--color-accent);
    font-size: 11px;
    font-weight: 500;
    margin-top: 4px;
    width: fit-content;
  }

  .bulk-env-box {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px;
    margin-bottom: 6px;
  }

  .form-textarea {
    width: 100%;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 8px;
    color: var(--color-text-primary);
    outline: none;
    resize: vertical;
    box-sizing: border-box;
    font-family: inherit;
  }

  .env-rows-scroll {
    max-height: 120px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
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
  }

  .form-input-sm {
    padding: 5px 8px;
    font-size: 11.5px;
  }

  .form-help {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    cursor: pointer;
  }

  .env-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--color-bg-raised);
    padding: 10px;
    border-radius: 8px;
    border: 1px solid var(--color-border-subtle);
  }

  .env-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal-env-row {
    display: grid;
    grid-template-columns: 1fr 1fr 24px;
    gap: 6px;
    align-items: center;
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
    margin-top: 5px;
  }

  .preset-pill {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 10.5px;
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
    background: var(--color-bg-hover);
  }

  .preset-pill.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
    border-color: var(--color-accent);
    font-weight: 600;
  }

  .btn-remove-row {
    background: transparent;
    border: none;
    font-size: 16px;
    cursor: pointer;
  }

  .modal-footer {
    padding: 12px 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }
</style>
