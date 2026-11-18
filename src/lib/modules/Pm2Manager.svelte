<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    Layers, Play, Square, RotateCw, Trash2, Plus, Terminal, 
    FileCode, Settings, Cpu, HardDrive, Shield, AlertTriangle, 
    CheckCircle2, Copy, Check, Eye, RefreshCcw, Save, Zap,
    FileText, ArrowDown, Search, Folder, TerminalSquare, Info,
    Activity, Clock, ChevronRight, Hash, Box, KeyRound, Database, Sparkles,
    LayoutGrid, Table2, Pause, Download
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
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
    namespace?: string;
    interpreter?: string;
    node_version?: string;
    max_memory_restart?: string;
    is_module?: boolean;
  }

  interface Pm2DumpInfo {
    exists: boolean;
    modified?: number | null;
    size: number;
  }

  type Pm2ActivityTone = 'info' | 'success' | 'warning' | 'error';

  interface Pm2Activity {
    id: number;
    time: number;
    title: string;
    detail?: string;
    tone: Pm2ActivityTone;
    pmId?: number;
    kind: string;
  }

  interface ProcessSnapshot {
    name: string;
    status: string;
    restarts: number;
    unstable: number;
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

  interface Pm2LogFileInfo {
    name: string;
    size: number;
  }

  interface Pm2LogOverview {
    directory: string;
    total_size: number;
    files: Pm2LogFileInfo[];
    hidden_files?: number;
    logrotate_installed: boolean;
    logrotate_version?: string | null;
  }

  interface Pm2SavedDumpApp {
    name: string;
    namespace: string;
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
    context?: string;
    message: string;
  }

  // Active Top Navigation Scope. A GlobalSearch request captured at mount
  // always wins over a persisted tab.
  const initialTargetSubTab = uiStore.targetSubTab;
  let activeTab = $state<'fleet' | 'ecosystem' | 'daemon'>(
    initialTargetSubTab === 'ecosystem' || initialTargetSubTab === 'daemon'
      ? initialTargetSubTab
      : 'fleet'
  );

  // Loading and State
  let loading = $state(true);
  let refreshing = $state(false);
  let fleetError = $state<string | null>(null);

  // Dump drift, activity and process operations
  let dumpInfo = $state<Pm2DumpInfo | null>(null);
  let showActivityDrawer = $state(false);
  let activityLog = $state<Pm2Activity[]>([]);
  let showScaleModal = $state(false);
  let scaleTarget = $state<Pm2Process | null>(null);
  let scaleInstances = $state(1);
  let scaling = $state(false);
  let restartEvents = $state<Record<number, number[]>>({});
  let dismissedStorms = $state<Set<number>>(new Set());

  let activitySeq = 0;
  const ACTIVITY_KEY = 'lcp_pm2_activity_v1';
  const RESTART_WINDOW_MS = 5 * 60 * 1000;
  const RESTART_THRESHOLD = 3;
  let lastProcessSnapshot = new Map<number, ProcessSnapshot>();
  let hasProcessBaseline = false;
  let suppressProcessEvents = new Map<number, number>();
  let suppressAllProcessEventsUntil = 0;
  let showEcosystemDiffModal = $state(false);
  let systemStatus = $state<Pm2SystemStatus | null>(null);
  let processes = $state<Pm2Process[]>([]);
  let ecosystemFiles = $state<Pm2EcosystemFile[]>([]);
  let startupStatus = $state<Pm2StartupStatus | null>(null);
  let savedDumpApps = $state<Pm2SavedDumpApp[]>([]);
  let savedDumpLoaded = $state(false);

  // Master-Detail & SideDrawer Selection State
  let selectedProcessId = $state<number | null>(null);
  let showProcessDrawer = $state(false);
  let showLogsDrawer = $state(false);
  let logFollow = $state(true);
  let drawerTab = $state<'vitals' | 'env'>('vitals');
  let selectedInspectorTab = $state<'logs' | 'vitals' | 'env'>('logs');
  let procContextMenu = $state<{
    show: boolean;
    x: number;
    y: number;
    process: Pm2Process | null;
  }>({ show: false, x: 0, y: 0, process: null });

  // Filters & Search
  let searchQuery = $state('');
  let statusFilter = $state<'all' | 'online' | 'stopped' | 'errored'>('all');
  let namespaceFilter = $state('all');
  let fleetView = $state<'cards' | 'table'>('cards');
  let processSortKey = $state<'name' | 'status' | 'cpu' | 'memory' | 'restarts' | 'uptime' | 'instances'>('name');
  let processSortAsc = $state(true);
  let processPollPaused = $state(false);
  let lastProcessUpdate = $state<Date | null>(null);
  let processHistory = $state<Record<number, { cpu: number[]; mem: number[] }>>({});
  let revealedEnvKeys = $state<Set<string>>(new Set());
  let envSearchQuery = $state('');

  let uiStateLoaded = $state(false);
  const UI_STATE_KEY = 'lcp_pm2_ui_v1';
  const HISTORY_LIMIT = 40;

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

  /** Folders that sit between a project root and its real entrypoint. */
  const GENERIC_SCRIPT_DIRS = new Set([
    'dist',
    'build',
    'out',
    'src',
    'public',
    'static',
    'bin',
    'lib',
    'coverage',
    '.next',
    'node_modules',
  ]);

  let launchPathLooksLikeFolder = $derived.by(() => {
    const raw = launchForm.script_path.trim();
    if (!raw) return false;
    if (raw.endsWith('/')) return true;
    const base = raw.split('/').filter(Boolean).pop() || '';
    // An ecosystem file is a file even though it is not an executable.
    if (base.includes('.')) return false;
    return GENERIC_SCRIPT_DIRS.has(base.toLowerCase());
  });

  function slugName(value: string): string {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '');
  }

  /**
   * Derive a sane process name and working directory from an entrypoint,
   * skipping bundler output folders such as dist/ or build/.
   */
  function inferLaunchHints(forceName = false, forceCwd = false) {
    const raw = launchForm.script_path.trim().replace(/\/+$/, '');
    if (!raw) return;
    const segments = raw.split('/').filter(Boolean);
    const fileName = segments.pop() || '';
    if (!fileName) return;

    const dirSegments = [...segments];
    while (
      dirSegments.length > 0 &&
      GENERIC_SCRIPT_DIRS.has((dirSegments[dirSegments.length - 1] || '').toLowerCase())
    ) {
      dirSegments.pop();
    }
    const dirPath = raw.startsWith('/') ? `/${dirSegments.join('/')}` : '';

    if (dirPath && (forceCwd || !launchForm.cwd.trim())) {
      launchForm.cwd = dirPath;
    }

    if (forceName || !launchForm.name.trim()) {
      const baseName = fileName.replace(/\.[^/.]+$/, '');
      const parent = dirSegments[dirSegments.length - 1] || '';
      let inferred = baseName;
      if (['index', 'main', 'app', 'server', 'start', 'cli'].includes(baseName.toLowerCase())) {
        inferred = parent || baseName;
      }
      let slug = slugName(inferred);
      if (!slug) slug = slugName(baseName);
      if (slug) launchForm.name = slug;
    }
  }

  let launchDuplicateError = $derived.by(() => {
    const raw = launchForm.script_path.trim();
    if (!raw) return '';
    const name = launchForm.name.trim();
    const sameScript = processes.find((proc) => proc.script_path === raw);
    if (sameScript) {
      return `That entrypoint is already managed as "${sameScript.name}" (#${sameScript.pm_id}). Use Restart instead of launching a duplicate.`;
    }
    if (name) {
      const sameName = processes.find((proc) => proc.name === name);
      if (sameName) {
        return `The name "${name}" is already in use by ${sameName.script_path}. Choose another name.`;
      }
    }
    return '';
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
  let ecosystemOnly = $state('');
  let ecosystemEnv = $state('');
  let ecosystemValidationError = $state('');
  let ecosystemValidating = $state(false);
  let ecosystemLoadToken = 0;
  let showNewEcosystemModal = $state(false);
  let newEcosystemPath = $state('');
  let newEcosystemTemplate = $state<'express' | 'nest' | 'next' | 'microservice'>('express');

  // Logs Stream State
  let selectedLogType = $state<'combined' | 'err'>('combined');
  let rawLogText = $state<string>('');
  let logSearchQuery = $state('');
  let logAutoRefresh = $state(true);
  let logErrorsOnly = $state(false);
  let logAtBottom = $state(true);
  let logOverview = $state<Pm2LogOverview | null>(null);
  let logOverviewLoading = $state(false);
  let exportingLog = $state(false);
  let logRefreshInterval = $state(2); // seconds
  let logTailLines = $state('200');
  let logLoading = $state(false);
  let logContainerRef = $state<HTMLDivElement | null>(null);
  let logLoadToken = 0;

  // Timers
  let refreshTimer: any = null;
  let logStreamTimer: any = null;
  let processesInFlight = false;

  // Aggregate Metrics Computed
  let totalApps = $derived(processes.length);
  let onlineApps = $derived(processes.filter(p => p.status === 'online').length);
  let stoppedApps = $derived(processes.filter(p => p.status === 'stopped').length);
  let erroredApps = $derived(processes.filter(p => p.status === 'errored').length);
  let totalMemoryBytes = $derived(processes.reduce((acc, p) => acc + (p.memory || 0), 0));
  let totalCpuPercent = $derived(processes.reduce((acc, p) => acc + (p.cpu || 0), 0));

  // Filtered Processes for Master Pane
  let namespaceOptions = $derived.by(() => {
    const names = new Set(processes.map((proc) => proc.namespace || 'default'));
    return [...names].sort();
  });

  function toggleProcessSort(key: typeof processSortKey) {
    if (processSortKey === key) {
      processSortAsc = !processSortAsc;
    } else {
      processSortKey = key;
      processSortAsc = key === 'name' || key === 'status';
    }
  }

  function processSortValue(proc: Pm2Process, key: typeof processSortKey): number | string {
    switch (key) {
      case 'status':
        return proc.status;
      case 'cpu':
        return proc.cpu || 0;
      case 'memory':
        return proc.memory || 0;
      case 'restarts':
        return proc.restarts || 0;
      case 'uptime':
        return proc.uptime ? Date.now() - proc.uptime : 0;
      case 'instances':
        return proc.instances || 0;
      default:
        return proc.name;
    }
  }

  let filteredProcesses = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    const list = processes.filter((proc) => {
      if (statusFilter === 'online' && proc.status !== 'online') return false;
      if (statusFilter === 'stopped' && proc.status !== 'stopped') return false;
      if (statusFilter === 'errored' && proc.status !== 'errored') return false;
      if (namespaceFilter !== 'all' && (proc.namespace || 'default') !== namespaceFilter) {
        return false;
      }
      if (query) {
        return (
          proc.name.toLowerCase().includes(query) ||
          proc.pm_id.toString().includes(query) ||
          proc.pid?.toString().includes(query) ||
          proc.script_path.toLowerCase().includes(query)
        );
      }
      return true;
    });

    return list.sort((a, b) => {
      const left = processSortValue(a, processSortKey);
      const right = processSortValue(b, processSortKey);
      let result =
        typeof left === 'string' && typeof right === 'string'
          ? left.localeCompare(right)
          : Number(left) - Number(right);
      if (result === 0) result = a.pm_id - b.pm_id;
      return processSortAsc ? result : -result;
    });
  });

  // Selected process must belong to the current filter, otherwise the detail
  // pane would show an app the list says is filtered out.
  let selectedProcess = $derived.by<Pm2Process | null>(() => {
    // Follow the explicit selection only. Falling back to the first filtered
    // process would silently retarget the drawer's actions (stop, restart,
    // scale, delete) at an app the user never selected when a filter hides
    // the current one.
    if (selectedProcessId === null) return null;
    return processes.find(p => p.pm_id === selectedProcessId) ?? null;
  });

  // Restart storms (3+ restarts in five minutes) and dump drift.
  let restartStorms = $derived.by(() => {
    const now = Date.now();
    return processes
      .filter((proc) => !proc.is_module && !dismissedStorms.has(proc.pm_id))
      .map((proc) => ({
        process: proc,
        count: (restartEvents[proc.pm_id] || []).filter(
          (stamp) => now - stamp <= RESTART_WINDOW_MS
        ).length,
      }))
      .filter((item) => item.count >= RESTART_THRESHOLD)
      .sort((a, b) => b.count - a.count);
  });

  let dumpDrift = $derived.by(() => {
    if (!savedDumpLoaded) {
      // Without a successful dump read, every live app would look unsaved and
      // the banner would invite a save that drops saved-but-stopped apps.
      return { hasDrift: false, notSaved: [] as Pm2Process[], notRunning: [], noDump: false };
    }
    const live = processes.filter((proc) => !proc.is_module);
    const saved = savedDumpApps.filter((app) => !app.name.startsWith('pm2-'));
    const savedKeys = new Set(
      saved.map((app) => `${app.namespace || 'default'}:${app.name}`)
    );
    const savedScripts = new Set(
      saved.map((app) => app.script_path).filter((path) => Boolean(path))
    );
    const liveKeys = new Set(
      live.map((proc) => `${proc.namespace || 'default'}:${proc.name}`)
    );
    const liveScripts = new Set(
      live.map((proc) => proc.script_path).filter((path) => Boolean(path))
    );
    const notSaved = live.filter(
      (proc) =>
        !savedKeys.has(`${proc.namespace || 'default'}:${proc.name}`) &&
        !(proc.script_path && savedScripts.has(proc.script_path))
    );
    const notRunning = saved.filter(
      (app) =>
        !liveKeys.has(`${app.namespace || 'default'}:${app.name}`) &&
        !(app.script_path && liveScripts.has(app.script_path))
    );
    return {
      hasDrift: notSaved.length > 0 || notRunning.length > 0,
      notSaved,
      notRunning,
      noDump: dumpInfo !== null && !dumpInfo.exists,
    };
  });

  // Filtered Environment Variables
  let filteredEnvVars = $derived.by<[string, string][]>(() => {
    if (!selectedProcess?.env_vars) return [];
    const entries = Object.entries(selectedProcess.env_vars);
    if (!envSearchQuery.trim()) return entries;
    const q = envSearchQuery.toLowerCase();
    return entries.filter(
      ([k, v]) =>
        k.toLowerCase().includes(q) ||
        ((!envIsSensitive(k) || revealedEnvKeys.has(k)) && v.toLowerCase().includes(q))
    );
  });

  function stripAnsi(value: string): string {
    return value
      .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, '')
      .replace(/\u001b\][^\u0007]*(?:\u0007|\u001b\\)/g, '')
      // Some log files have had the ESC byte removed but keep the SGR codes.
      .replace(/\[\d{1,3}(?:;\d{1,3})*m/g, '');
  }

  // Parsed log lines with level, timestamp and Nest context.
  let parsedLogs = $derived.by<ParsedLogLine[]>(() => {
    if (!rawLogText || rawLogText.trim().length === 0) return [];
    const lines = rawLogText.split('\n');
    const result: ParsedLogLine[] = [];
    const search = logSearchQuery.trim().toLowerCase();

    for (let i = 0; i < lines.length; i++) {
      const line = stripAnsi(lines[i]);
      if (!line && i === lines.length - 1) continue;

      let level: 'error' | 'warn' | 'info' = 'info';
      let timestamp: string | undefined;
      let context: string | undefined;
      let message = line;

      const nest = line.match(
        /^\[Nest\]\s+\d+\s+-\s+(.+?)\s+(LOG|ERROR|WARN|DEBUG|VERBOSE|FATAL)\s+(?:\[([^\]]+)\]\s+)?(.*)$/i
      );
      if (nest) {
        timestamp = nest[1].trim();
        const nestLevel = nest[2].toUpperCase();
        level = nestLevel === 'ERROR' || nestLevel === 'FATAL' ? 'error' : nestLevel === 'WARN' ? 'warn' : 'info';
        context = nest[3]?.trim();
        message = nest[4] || '';
      } else {
        // Word-boundary matching, so "preferred", "stderr" or a user name
        // containing "err" are not mistaken for error lines.
        if (
          /\b(error|err|fatal|exception|failure|failed|critical|panic)\b|\[(error|fatal)\]|\bERR!/i.test(
            line
          )
        ) {
          level = 'error';
        } else if (/\b(warn|warning)\b|\[warn\]/i.test(line)) {
          level = 'warn';
        }

        const tsMatch = line.match(
          /^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?):?\s*(.*)$/
        );
        if (tsMatch) {
          timestamp = tsMatch[1];
          message = tsMatch[2] || '';
        }
      }

      if (search && !line.toLowerCase().includes(search)) {
        continue;
      }
      if (logErrorsOnly && level !== 'error') {
        continue;
      }

      result.push({
        lineNumber: i + 1,
        raw: line,
        level,
        timestamp,
        context,
        message,
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

  function formatRelativeTime(unixSeconds?: number | null): string {
    if (!unixSeconds) return 'never';
    const seconds = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
    if (seconds < 10) return 'just now';
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }

  function sparklinePoints(
    values: number[],
    width: number,
    height: number,
    maximum?: number
  ): string {
    if (values.length < 2) return '';
    const ceiling = maximum && maximum > 0 ? maximum : Math.max(...values, 1);
    return values
      .map((value, index) => {
        const x = (index / (values.length - 1)) * width;
        const y = height - Math.min(1, Math.max(0, value / ceiling)) * height;
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(' ');
  }

  function parseMemoryLimit(value?: string | null): number | null {
    if (!value) return null;
    const match = value.trim().match(/^(\d+(?:\.\d+)?)\s*([KMG])?$/i);
    if (!match) return null;
    const unit = (match[2] || 'M').toUpperCase();
    const factor = unit === 'G' ? 1024 ** 3 : unit === 'M' ? 1024 ** 2 : unit === 'K' ? 1024 : 1;
    return Math.round(parseFloat(match[1]) * factor);
  }

  function memoryLimitLabel(value?: string | null): string {
    const bytes = parseMemoryLimit(value);
    return bytes ? formatBytes(bytes) : '';
  }

  function envIsSensitive(key: string): boolean {
    return /(SECRET|TOKEN|PASSWORD|PASSWD|PRIVATE|CREDENTIAL|AUTH|API[_-]?KEY|ACCESS[_-]?KEY|(^|_)KEY($|_))/i.test(
      key
    );
  }

  function envDisplayValue(key: string, value: string): string {
    return envIsSensitive(key) && !revealedEnvKeys.has(key) ? '••••••••' : value;
  }

  function toggleEnvReveal(key: string) {
    const next = new Set(revealedEnvKeys);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    revealedEnvKeys = next;
  }

  async function copyPm2Command(proc: Pm2Process) {
    const command = `pm2 restart ${proc.name}`;
    try {
      await navigator.clipboard.writeText(command);
      uiStore.showToast(`Copied ${command}`, 'info');
    } catch {
      uiStore.showToast('Could not copy the PM2 command', 'error');
    }
  }

  function loadUiState() {
    try {
      const raw = localStorage.getItem(UI_STATE_KEY);
      if (!raw) return;
      const state = JSON.parse(raw);
      if (state.fleetView === 'cards' || state.fleetView === 'table') fleetView = state.fleetView;
      if (['all', 'online', 'stopped', 'errored'].includes(state.statusFilter)) {
        statusFilter = state.statusFilter;
      }
      if (typeof state.searchQuery === 'string') searchQuery = state.searchQuery;
      if (typeof state.namespaceFilter === 'string') namespaceFilter = state.namespaceFilter;
      if (typeof state.processPollPaused === 'boolean') processPollPaused = state.processPollPaused;
      if (
        ['name', 'status', 'cpu', 'memory', 'restarts', 'uptime', 'instances'].includes(
          state.processSortKey
        )
      ) {
        processSortKey = state.processSortKey;
      }
      if (typeof state.processSortAsc === 'boolean') processSortAsc = state.processSortAsc;
      if (state.selectedProcessId === null || typeof state.selectedProcessId === 'number') {
        selectedProcessId = state.selectedProcessId;
      }
      if (
        !initialTargetSubTab &&
        !uiStore.targetSubTab &&
        ['fleet', 'ecosystem', 'daemon'].includes(state.activeTab)
      ) {
        activeTab = state.activeTab;
      }
    } catch {
      // Ignore a corrupt UI state cache.
    }
  }

  $effect(() => {
    const snapshot = {
      fleetView,
      statusFilter,
      searchQuery,
      namespaceFilter,
      processPollPaused,
      processSortKey,
      processSortAsc,
      selectedProcessId,
      activeTab,
    };
    if (!uiStateLoaded) return;
    try {
      localStorage.setItem(UI_STATE_KEY, JSON.stringify(snapshot));
    } catch {
      // A full/disabled storage must not break the page.
    }
  });

  $effect(() => {
    const target = uiStore.targetSubTab;
    if (!target) return;
    if (target === 'ecosystem' || target === 'daemon') {
      activeTab = target;
    } else if (target === 'fleet' || target === 'processes' || target === 'logs') {
      activeTab = 'fleet';
    }
    uiStore.targetSubTab = null;
  });

  // Keep unsaved ecosystem edits from disappearing on app close or reload.
  // Switching tabs keeps the editor state, so only file switches need a modal.
  $effect(() => {
    if (!isEcosystemDirty || typeof window === 'undefined') return;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', handleBeforeUnload);
    return () => window.removeEventListener('beforeunload', handleBeforeUnload);
  });

  // Also guard in-app module switches: leaving unmounts this page and would
  // silently discard the draft.
  $effect(() => {
    if (!isEcosystemDirty) return;
    return uiStore.registerLeaveGuard(() => isEcosystemDirty);
  });

  $effect(() => {
    // Wait for the first process scan before deciding a saved namespace no
    // longer exists.
    if (processes.length === 0) return;
    if (namespaceFilter !== 'all' && !namespaceOptions.includes(namespaceFilter)) {
      namespaceFilter = 'all';
    }
  });

  let envRevealOwner: number | null = null;
  $effect(() => {
    // A revealed secret belongs to one process only; never carry it over to
    // another app that happens to use the same variable name.
    if (selectedProcessId === envRevealOwner) return;
    envRevealOwner = selectedProcessId;
    if (revealedEnvKeys.size > 0) revealedEnvKeys = new Set();
  });

  function formatActivityTime(time: number): string {
    return new Date(time).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function persistActivity() {
    try {
      localStorage.setItem(ACTIVITY_KEY, JSON.stringify(activityLog.slice(0, 50)));
    } catch {
      // A full/disabled storage must not break PM2 actions.
    }
  }

  function loadActivity() {
    try {
      const raw = localStorage.getItem(ACTIVITY_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return;
      activityLog = parsed
        .filter(
          (event) =>
            event &&
            typeof event.id === 'number' &&
            typeof event.time === 'number' &&
            typeof event.title === 'string'
        )
        .slice(0, 50);
      activitySeq = activityLog.reduce(
        (max, event) => Math.max(max, event.id || 0),
        0
      );
    } catch {
      // Ignore a corrupt activity cache.
    }
  }

  function recordActivity(
    title: string,
    detail: string | undefined,
    tone: Pm2ActivityTone = 'info',
    pmId?: number,
    kind = 'event'
  ) {
    activityLog = [
      { id: ++activitySeq, time: Date.now(), title, detail, tone, pmId, kind },
      ...activityLog,
    ].slice(0, 50);
    persistActivity();
  }

  function clearActivity() {
    activityLog = [];
    persistActivity();
  }

  function dismissStorm(pmId: number) {
    dismissedStorms = new Set(dismissedStorms).add(pmId);
  }

  function viewStormProcess(proc: Pm2Process) {
    openLogsForProcess(proc);
  }

  function openActivityProcess(event: Pm2Activity) {
    if (event.pmId == null) return;
    const proc = processes.find((item) => item.pm_id === event.pmId);
    if (!proc) {
      uiStore.showToast('That process is no longer running.', 'warning');
      return;
    }
    showActivityDrawer = false;
    openLogsForProcess(proc);
  }

  function openScaleModal(proc: Pm2Process) {
    scaleTarget = proc;
    scaleInstances = Math.max(1, proc.instances || 1);
    showScaleModal = true;
  }

  async function submitScale() {
    if (!scaleTarget) return;
    const instances = Number(scaleInstances);
    if (!Number.isInteger(instances) || instances < 1 || instances > 64) {
      uiStore.showToast('Instances must be an integer between 1 and 64.', 'warning');
      return;
    }
    scaling = true;
    try {
      const name = scaleTarget.name;
      const pmId = scaleTarget.pm_id;
      const msg = await invoke<string>('pm2_scale', { name, instances });
      uiStore.showToast(msg, 'success');
      recordActivity(`Scaled ${name}`, `${instances} instance(s)`, 'success', pmId, 'scale');
      showScaleModal = false;
      await loadProcesses(false);
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to scale application', 'error');
    } finally {
      scaling = false;
    }
  }

  /** Compare the new poll with the previous one to surface crashes/restarts. */
  function detectProcessChanges(next: Pm2Process[]) {
    if (!hasProcessBaseline) {
      lastProcessSnapshot = new Map(
        next.map((proc) => [
          proc.pm_id,
          {
            name: proc.name,
            status: proc.status,
            restarts: proc.restarts,
            unstable: proc.unstable_restarts,
          },
        ])
      );
      hasProcessBaseline = true;
      return;
    }

    const now = Date.now();
    const nextRestartEvents = { ...restartEvents };

    for (const proc of next) {
      if (proc.is_module) continue;
      const old = lastProcessSnapshot.get(proc.pm_id);
      if (!old) continue;

      const suppressed =
        now < (suppressProcessEvents.get(proc.pm_id) ?? 0) ||
        now < suppressAllProcessEventsUntil;

      if (!suppressed && proc.restarts > old.restarts) {
        const delta = proc.restarts - old.restarts;
        const stamps = [
          ...(nextRestartEvents[proc.pm_id] || []).filter(
            (stamp) => now - stamp <= RESTART_WINDOW_MS
          ),
          ...Array.from({ length: Math.min(delta, 10) }, () => now),
        ];
        nextRestartEvents[proc.pm_id] = stamps;
        if (dismissedStorms.has(proc.pm_id)) {
          const nextDismissed = new Set(dismissedStorms);
          nextDismissed.delete(proc.pm_id);
          dismissedStorms = nextDismissed;
        }
        recordActivity(
          `Restarted ${proc.name}`,
          `${delta} restart${delta === 1 ? '' : 's'} recorded`,
          'warning',
          proc.pm_id,
          'restart'
        );
      }

      if (!suppressed && proc.unstable_restarts > old.unstable) {
        recordActivity(
          `Unstable restarts: ${proc.name}`,
          `${proc.unstable_restarts} unstable restart(s)`,
          'error',
          proc.pm_id,
          'unstable'
        );
      }

      if (!suppressed && proc.status !== old.status) {
        if (proc.status === 'errored') {
          recordActivity(
            `${proc.name} is errored`,
            `PID ${proc.pid ?? '—'}`,
            'error',
            proc.pm_id,
            'status'
          );
        } else if (proc.status === 'stopped') {
          recordActivity(
            `${proc.name} stopped`,
            `Previous state: ${old.status}`,
            'warning',
            proc.pm_id,
            'status'
          );
        } else if (proc.status === 'online' && old.status !== 'online') {
          recordActivity(
            `${proc.name} is online`,
            `PID ${proc.pid ?? '—'}`,
            'success',
            proc.pm_id,
            'status'
          );
        }
      }
    }

    restartEvents = nextRestartEvents;
    lastProcessSnapshot = new Map(
      next.map((proc) => [
        proc.pm_id,
        {
          name: proc.name,
          status: proc.status,
          restarts: proc.restarts,
          unstable: proc.unstable_restarts,
        },
      ])
    );
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
    navigator.clipboard.writeText(stripAnsi(rawLogText)).then(() => {
      copiedLogs = true;
      setTimeout(() => copiedLogs = false, 2000);
    });
  }

  function handleLogScroll() {
    const container = logContainerRef;
    if (!container) return;
    const atBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight <= 24;
    logAtBottom = atBottom;
    // Pause following while the user reads older lines, and resume once they
    // scroll back to the newest output.
    logFollow = atBottom;
  }

  function jumpToLatest() {
    logFollow = true;
    logAtBottom = true;
    if (logContainerRef) {
      logContainerRef.scrollTop = logContainerRef.scrollHeight;
    }
  }

  function currentLogPath(): string {
    if (!selectedProcess) return '';
    return selectedLogType === 'err'
      ? selectedProcess.err_log_path
      : selectedProcess.out_log_path;
  }

  async function openLogFolder() {
    const home = systemStatus?.pm2_home;
    const directory = logOverview?.directory || (home ? `${home}/logs` : '');
    if (!directory) {
      uiStore.showToast('The PM2 log folder is not known yet.', 'warning');
      return;
    }
    try {
      await invoke('open_folder', { path: directory });
    } catch (err: any) {
      uiStore.showToast(String(err) || 'Could not open the log folder', 'error');
    }
  }

  async function downloadLogFile() {
    const logPath = currentLogPath();
    if (!logPath) {
      uiStore.showToast('Select a process with a log file first.', 'warning');
      return;
    }
    exportingLog = true;
    try {
      const saved = await invoke<string>('pm2_export_log', { logPath });
      uiStore.showToast(`Saved log to ${saved}`, 'success');
    } catch (err: any) {
      uiStore.showToast(String(err) || 'Could not export the log file', 'error');
    } finally {
      exportingLog = false;
    }
  }

  async function loadLogOverview() {
    logOverviewLoading = true;
    try {
      logOverview = await invoke<Pm2LogOverview>('pm2_get_log_overview');
    } catch {
      logOverview = null;
    } finally {
      logOverviewLoading = false;
    }
  }

  /** Merge the live process list into the saved dump metadata. */
  function mergeSavedDumpState(list: Pm2SavedDumpApp[]): Pm2SavedDumpApp[] {
    return list.map((app) => {
      const appNamespace = app.namespace || 'default';
      const match = processes.find(
        (proc) =>
          (proc.name === app.name &&
            (proc.namespace || 'default') === appNamespace) ||
          (!!app.script_path && proc.script_path === app.script_path)
      );
      return {
        ...app,
        is_currently_running: match?.status === 'online',
        current_pm_id: match?.pm_id,
      };
    });
  }

  /** Cheap telemetry refresh: only the live process list. */
  async function loadProcesses(silent = true) {
    if (processesInFlight) return;
    processesInFlight = true;
    try {
      const procList = await invoke<Pm2Process[]>('pm2_list_processes');
      detectProcessChanges(procList);

      const nextHistory = { ...processHistory };
      const currentIds = new Set(procList.map((proc) => proc.pm_id));
      for (const proc of procList) {
        const previous = nextHistory[proc.pm_id] ?? { cpu: [], mem: [] };
        nextHistory[proc.pm_id] = {
          cpu: [...previous.cpu, proc.cpu].slice(-HISTORY_LIMIT),
          mem: [...previous.mem, proc.memory].slice(-HISTORY_LIMIT),
        };
      }
      for (const key of Object.keys(nextHistory)) {
        if (!currentIds.has(Number(key))) delete nextHistory[Number(key)];
      }
      processHistory = nextHistory;

      processes = procList;
      lastProcessUpdate = new Date();
      fleetError = null;
      if (procList.length > 0) {
        if (selectedProcessId === null || !procList.some((p) => p.pm_id === selectedProcessId)) {
          selectedProcessId = procList[0].pm_id;
        }
      } else {
        selectedProcessId = null;
      }
      if (savedDumpApps.length > 0) {
        savedDumpApps = mergeSavedDumpState(savedDumpApps);
      }
    } catch (err: any) {
      fleetError = err?.message || String(err);
      if (!silent) throw err;
    } finally {
      processesInFlight = false;
    }
  }

  /** Slower data: ecosystem files, startup service and saved dump. */
  async function loadAuxiliary() {
    logOverviewLoading = true;
    const [ecoList, startStatus, savedList, dump, measuredLogs] = await Promise.all([
      invoke<Pm2EcosystemFile[]>('pm2_list_ecosystem_files', { customDirs: null }).catch(() => null),
      invoke<Pm2StartupStatus>('pm2_get_startup_status').catch(() => null),
      invoke<Pm2SavedDumpApp[]>('pm2_get_saved_dump_apps').catch(() => null),
      invoke<Pm2DumpInfo>('pm2_get_dump_info').catch(() => null),
      invoke<Pm2LogOverview>('pm2_get_log_overview').catch(() => null)
    ]);
    // Keep the previous model when a call fails. Replacing a failed saved-dump
    // read with an empty list would make every live app look unsaved and invite
    // a "Save dump" that drops the saved-but-stopped apps.
    if (ecoList) ecosystemFiles = ecoList;
    if (startStatus) startupStatus = startStatus;
    if (savedList) {
      savedDumpApps = mergeSavedDumpState(savedList);
      savedDumpLoaded = true;
    }
    if (dump) dumpInfo = dump;
    if (measuredLogs) logOverview = measuredLogs;
    logOverviewLoading = false;

    if (ecoList && ecoList.length > 0 && !selectedEcosystemFile) {
      selectEcosystemFile(ecoList[0]);
    }
  }

  // Full load: status once, processes, then the slower ecosystem/dump data.
  async function loadData(silent = false) {
    if (!silent) refreshing = true;
    try {
      const status = await invoke<Pm2SystemStatus>('pm2_get_system_status');
      systemStatus = status;
      if (status.installed) {
        await loadProcesses(false);
        await loadAuxiliary();
      }
    } catch (err: any) {
      if (!silent) {
        uiStore.showToast(err.message || 'Failed to communicate with PM2', 'error');
      } else {
        fleetError = err?.message || String(err);
      }
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  // Lifecycle Actions
  async function handleProcessAction(action: string, target: string | number, confirmed = false) {
    if (!confirmed && (action === 'stop' || action === 'restart') && target !== 'all') {
      const proc = processes.find((process) => process.pm_id === Number(target));
      if (proc) {
        const stopping = action === 'stop';
        uiStore.confirm(
          `${stopping ? 'Stop' : 'Restart'} "${proc.name}"?`,
          stopping
            ? `This stops ${proc.name} (ID #${proc.pm_id}) until it is started again. All of its instances will stop.`
            : `This briefly interrupts ${proc.name} (ID #${proc.pm_id}) while PM2 restarts it. Use Reload for zero downtime instead.`,
          () => handleProcessAction(action, target, true),
          stopping
        );
        return;
      }
    }

    // Silence poll-derived events for this app while the action settles; the
    // action itself is recorded explicitly below.
    const suppressUntil = Date.now() + 8000;
    if (target === 'all') {
      suppressAllProcessEventsUntil = suppressUntil;
    } else {
      suppressProcessEvents.set(Number(target), suppressUntil);
    }
    try {
      const msg = await invoke<string>('pm2_process_action', {
        action,
        target: target.toString()
      });
      uiStore.showToast(msg, 'success');

      const targetId = target === 'all' ? null : Number(target);
      const proc = targetId != null ? processes.find((p) => p.pm_id === targetId) : null;
      const labels: Record<string, string> = {
        start: 'Started',
        stop: 'Stopped',
        restart: 'Restarted',
        reload: 'Reloaded',
        delete: 'Deleted',
        reset: 'Reset restart counters for',
      };
      const tone: Pm2ActivityTone =
        action === 'delete' ? 'error' : action === 'stop' ? 'warning' : action === 'reset' ? 'info' : 'success';
      recordActivity(
        `${labels[action] || action} ${proc?.name || (target === 'all' ? 'all processes' : `#${target}`)}`,
        undefined,
        tone,
        targetId ?? undefined,
        'action'
      );
      await loadProcesses(false);
      if (activeTab === 'fleet' && showLogsDrawer && selectedInspectorTab === 'logs') {
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
      recordActivity('Saved process dump', `${processes.length} app(s)`, 'success', undefined, 'dump');
      await loadAuxiliary();
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to save PM2 dump', 'error');
    }
  }

  async function handleResurrect() {
    try {
      const msg = await invoke<string>('pm2_resurrect_dump');
      uiStore.showToast(msg, 'success');
      recordActivity('Resurrected saved dump', `${savedDumpApps.length} saved app(s)`, 'success', undefined, 'dump');
      suppressAllProcessEventsUntil = Date.now() + 8000;
      await loadProcesses(false);
      await loadAuxiliary();
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to resurrect PM2 dump', 'error');
    }
  }

  async function handleStartSavedApp(app: Pm2SavedDumpApp) {
    try {
      const msg = await invoke<string>('pm2_start_saved_app', {
        name: app.name,
        scriptPath: app.script_path,
        namespace: app.namespace || 'default',
        cwd: app.cwd || null,
        execMode: app.exec_mode || null,
        instances: app.instances || null,
        watch: app.watch || null,
        maxMemoryRestart: app.max_memory_restart || null,
        args: app.args && app.args.length > 0 ? app.args : null,
        envVars: app.env_vars && Object.keys(app.env_vars).length > 0 ? app.env_vars : null
      });
      uiStore.showToast(msg, 'success');
      recordActivity(`Started saved app ${app.name}`, app.script_path, 'success', undefined, 'action');
      await loadProcesses(false);
      await loadAuxiliary();
    } catch (err: any) {
      uiStore.showToast(err || `Failed to start saved app ${app.name}`, 'error');
    }
  }

  async function handleDeleteSavedApp(app: Pm2SavedDumpApp) {
    try {
      const msg = await invoke<string>('pm2_delete_saved_app', {
        name: app.name,
        namespace: app.namespace || 'default'
      });
      uiStore.showToast(msg, 'success');
      recordActivity(`Removed ${app.name} from dump`, undefined, 'warning', undefined, 'dump');
      await loadAuxiliary();
    } catch (err: any) {
      uiStore.showToast(err || `Failed to delete ${app.name} from dump`, 'error');
    }
  }

  async function handleFlushLogs(target?: string | number) {
    const hasTarget = target !== undefined && target !== null && target !== '';
    try {
      const msg = await invoke<string>('pm2_flush_logs', {
        target: hasTarget ? target.toString() : null
      });
      uiStore.showToast(msg, 'success');
      recordActivity(
        'Flushed logs',
        hasTarget ? `Process #${target}` : 'All PM2 logs',
        'info',
        typeof target === 'number' ? target : undefined,
        'logs'
      );
      loadLogs();
      void loadLogOverview();
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
      proc ? 'Flush Application Logs' : 'Flush All PM2 Logs',
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

  function promptDeleteSavedApp(app: Pm2SavedDumpApp) {
    const label = app.namespace && app.namespace !== 'default'
      ? `${app.name} (${app.namespace})`
      : app.name;
    uiStore.confirm(
      `Remove "${label}" from Saved Dump`,
      `Are you sure you want to remove "${label}" from ~/.pm2/dump.pm2? It will no longer be restored automatically on system reboot.`,
      () => handleDeleteSavedApp(app),
      true
    );
  }

  // Logs stream handling for currently selected process
  async function loadLogs() {
    if (!systemStatus?.installed) return;
    const token = ++logLoadToken;
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
        if (token !== logLoadToken) return;
        rawLogText = content;
        await tick();
        if (logContainerRef) {
          if (logFollow) {
            logContainerRef.scrollTop = logContainerRef.scrollHeight;
            logAtBottom = true;
          } else {
            logAtBottom =
              logContainerRef.scrollHeight -
                logContainerRef.scrollTop -
                logContainerRef.clientHeight <=
              24;
          }
        }
      } else {
        // Never leave another process's text under this app's header.
        rawLogText = selectedProcess
          ? `[No log file is configured for "${selectedProcess.name}".]`
          : '';
      }
    } catch (err: any) {
      if (token !== logLoadToken) return;
      rawLogText = `[Error reading logs: ${err}]`;
    } finally {
      if (token === logLoadToken) logLoading = false;
    }
  }

  // Select a process for the live logs drawer. The details drawer is explicit.
  function handleSelectProcess(id: number, openDetails = false) {
    selectedProcessId = id;
    if (openDetails) {
      showLogsDrawer = false;
      showProcessDrawer = true;
    } else {
      selectedInspectorTab = 'logs';
      showLogsDrawer = true;
      logFollow = true;
      logAtBottom = true;
      loadLogs();
    }
  }

  function openLogsForProcess(proc: Pm2Process) {
    statusFilter = 'all';
    searchQuery = '';
    namespaceFilter = 'all';
    logSearchQuery = '';
    logErrorsOnly = false;
    selectedProcessId = proc.pm_id;
    selectedInspectorTab = 'logs';
    activeTab = 'fleet';
    showProcessDrawer = false;
    showLogsDrawer = true;
    logFollow = true;
    logAtBottom = true;
    loadLogs();
  }

  function openProcessContextMenu(event: MouseEvent, proc: Pm2Process) {
    event.preventDefault();
    event.stopPropagation();
    procContextMenu = {
      show: true,
      x: Math.min(event.clientX, window.innerWidth - 250),
      y: Math.min(event.clientY, window.innerHeight - 340),
      process: proc,
    };
  }

  function closeProcessContextMenu() {
    if (procContextMenu.show) {
      procContextMenu = { ...procContextMenu, show: false, process: null };
    }
  }

  // Launch custom process
  async function launchProcess() {
    if (!launchForm.script_path.trim()) {
      uiStore.showToast('Please specify a script or entry file path', 'warning');
      return;
    }

    inferLaunchHints();

    if (launchPathLooksLikeFolder) {
      uiStore.showToast(
        'That path looks like a folder. Select the entry file inside it, such as dist/main.js.',
        'warning'
      );
      return;
    }
    if (launchDuplicateError) {
      uiStore.showToast(launchDuplicateError, 'warning');
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
      recordActivity(
        `Launched ${launchForm.name.trim() || launchForm.script_path.trim()}`,
        launchForm.script_path.trim(),
        'success',
        undefined,
        'action'
      );
      showLaunchModal = false;
      resetLaunchForm();
      await loadProcesses(false);
      await loadAuxiliary();
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
        inferLaunchHints();
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
  async function applyEcosystemFile(file: Pm2EcosystemFile) {
    const token = ++ecosystemLoadToken;
    try {
      const content = await invoke<string>('pm2_read_ecosystem_file', { path: file.path });
      // A slower read from a previously-clicked file must not overwrite the
      // file the user selected last.
      if (token !== ecosystemLoadToken) return;
      // Only switch the editor after the content is in hand. Switching first
      // would leave the old file's text paired with the new file's path, and a
      // later save could write it to the wrong config.
      selectedEcosystemFile = file;
      ecosystemContent = content;
      originalEcosystemContent = content;
      ecosystemOnly = '';
      ecosystemEnv = '';
      ecosystemValidationError = '';
    } catch (err: any) {
      if (token !== ecosystemLoadToken) return;
      uiStore.showToast(`Failed to read ${file.name}: ${err}`, 'error');
    }
  }

  function selectEcosystemFile(file: Pm2EcosystemFile) {
    if (selectedEcosystemFile?.path === file.path) return;
    if (isEcosystemDirty) {
      uiStore.confirm(
        'Unsaved Ecosystem Changes',
        `${selectedEcosystemFile?.name ?? 'The current config'} has unsaved changes. Opening ${file.name} will discard them. Continue?`,
        () => applyEcosystemFile(file),
        true
      );
      return;
    }
    void applyEcosystemFile(file);
  }

  async function validateEcosystemContent(): Promise<boolean> {
    if (!selectedEcosystemFile) return false;
    ecosystemValidating = true;
    ecosystemValidationError = '';
    try {
      await invoke('pm2_validate_ecosystem', {
        path: selectedEcosystemFile.path,
        content: ecosystemContent,
      });
      return true;
    } catch (err: any) {
      ecosystemValidationError = String(err);
      return false;
    } finally {
      ecosystemValidating = false;
    }
  }

  async function saveEcosystemFile(): Promise<boolean> {
    const file = selectedEcosystemFile;
    if (!file) return false;
    const content = ecosystemContent;
    try {
      await invoke<string>('pm2_write_ecosystem_file', {
        path: file.path,
        content,
        createOnly: false,
      });
      // Do not clear the dirty state of a different file if the user managed
      // to switch while the write was in flight.
      if (selectedEcosystemFile?.path === file.path) {
        originalEcosystemContent = content;
        ecosystemValidationError = '';
      }
      uiStore.showToast(`Saved ${file.name}`, 'success');
      recordActivity(`Saved ${file.name}`, file.path, 'success', undefined, 'ecosystem');
      return true;
    } catch (err: any) {
      if (selectedEcosystemFile?.path === file.path) {
        ecosystemValidationError = String(err);
        showEcosystemDiffModal = false;
      }
      uiStore.showToast('The ecosystem file could not be saved. See the editor error.', 'error');
      return false;
    }
  }

  async function openSaveEcosystemModal() {
    if (!selectedEcosystemFile) return;
    if (!(await validateEcosystemContent())) {
      uiStore.showToast('Fix the syntax errors before saving.', 'error');
      return;
    }
    showEcosystemDiffModal = true;
  }

  async function startEcosystemFile() {
    if (!selectedEcosystemFile) return;
    if (isEcosystemDirty) {
      uiStore.confirm(
        'Save Before Starting',
        `${selectedEcosystemFile.name} has unsaved changes. Save them before PM2 starts this config?`,
        async () => {
          if (await saveEcosystemFile()) {
            await startEcosystemFile();
          }
        },
        false
      );
      return;
    }

    const only = ecosystemOnly
      .split(',')
      .map((name) => name.trim())
      .filter(Boolean)
      .join(',');
    const environment = ecosystemEnv.trim();
    try {
      const msg = await invoke<string>('pm2_start_ecosystem', {
        path: selectedEcosystemFile.path,
        only: only || null,
        envName: environment || null
      });
      ecosystemValidationError = '';
      uiStore.showToast(msg, 'success');
      const detail = [
        selectedEcosystemFile.path,
        only ? `--only ${only}` : '',
        environment ? `--env ${environment}` : ''
      ].filter(Boolean).join(' · ');
      recordActivity(
        `Started ecosystem ${selectedEcosystemFile.name}`,
        detail,
        'success',
        undefined,
        'ecosystem'
      );
      await loadProcesses(false);
      await loadAuxiliary();
    } catch (err: any) {
      const message = String(err);
      if (/syntax|invalid json|unsupported ecosystem/i.test(message)) {
        ecosystemValidationError = message;
      }
      uiStore.showToast(message || 'Failed to start ecosystem file', 'error');
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
        content,
        createOnly: true
      });
      uiStore.showToast('Ecosystem configuration created successfully', 'success');
      recordActivity('Created ecosystem config', newEcosystemPath.trim(), 'success', undefined, 'ecosystem');
      showNewEcosystemModal = false;
      await loadAuxiliary();
    } catch (err: any) {
      uiStore.showToast(err || 'Failed to create ecosystem file', 'error');
    }
  }

  onMount(() => {
    loadUiState();
    uiStateLoaded = true;
    loadActivity();
    loadData();

    // 3s Live Telemetry Refresh
    refreshTimer = setInterval(() => {
      if (systemStatus?.installed && activeTab === 'fleet' && !processPollPaused) {
        // Only live telemetry; ecosystem scans and systemctl do not belong on
        // a three-second loop.
        loadProcesses(true);
      }
    }, 3000);

    // 2s Logs Auto-Stream Refresh
    logStreamTimer = setInterval(() => {
      if (
        systemStatus?.installed &&
        activeTab === 'fleet' &&
        showLogsDrawer &&
        selectedInspectorTab === 'logs' &&
        logAutoRefresh
      ) {
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
      onchange={async (t: string) => {
        if (t === 'fleet') {
          if (showLogsDrawer && selectedInspectorTab === 'logs') loadLogs();
        } else if (t === 'daemon') {
          await loadProcesses(true);
          await loadAuxiliary();
        } else if (t === 'ecosystem') {
          await loadAuxiliary();
        }
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
  <!-- 3. COMPACT FLEET HEALTH STRIP -->
  <!-- ═════════════════════════════════════════════════════════════════════════ -->
  {#if systemStatus?.installed}
    <div class="fleet-strip">
      <div class="fleet-status-group" role="group" aria-label="Filter processes by status">
        <button
          type="button"
          class="fleet-stat"
          class:active={statusFilter === 'all'}
          aria-pressed={statusFilter === 'all'}
          onclick={() => statusFilter = 'all'}
          title="Show every process"
        >
          <Layers size={13} />
          <strong>{totalApps}</strong>
          <span>Apps</span>
        </button>
        <button
          type="button"
          class="fleet-stat"
          class:active={statusFilter === 'online'}
          aria-pressed={statusFilter === 'online'}
          onclick={() => statusFilter = 'online'}
          title="Show online processes"
        >
          <span class="status-dot online"></span>
          <strong>{onlineApps}</strong>
          <span>Online</span>
        </button>
        <button
          type="button"
          class="fleet-stat"
          class:active={statusFilter === 'stopped'}
          aria-pressed={statusFilter === 'stopped'}
          onclick={() => statusFilter = 'stopped'}
          title="Show stopped processes"
        >
          <span class="status-dot stopped"></span>
          <strong>{stoppedApps}</strong>
          <span>Stopped</span>
        </button>
        <button
          type="button"
          class="fleet-stat"
          class:active={statusFilter === 'errored'}
          aria-pressed={statusFilter === 'errored'}
          onclick={() => statusFilter = 'errored'}
          title="Show errored processes"
        >
          <span class="status-dot errored"></span>
          <strong>{erroredApps}</strong>
          <span>Errored</span>
        </button>
      </div>

      <span class="strip-sep"></span>

      <span class="fleet-metric" title="Aggregate CPU across managed processes">
        <Cpu size={13} /><strong>{totalCpuPercent.toFixed(1)}%</strong><span>CPU</span>
      </span>
      <span class="fleet-metric" title="Combined resident memory">
        <HardDrive size={13} /><strong>{formatBytes(totalMemoryBytes)}</strong><span>RAM</span>
      </span>

      {#if dumpInfo}
        <span
          class="fleet-metric"
          class:stale={dumpInfo.modified
            ? Date.now() / 1000 - dumpInfo.modified > 24 * 3600
            : false}
          title={dumpInfo.exists
            ? `dump.pm2 · ${dumpInfo.size} bytes${dumpInfo.modified && Date.now() / 1000 - dumpInfo.modified > 24 * 3600 ? ' · older than 24 hours' : ''}`
            : 'No dump file has been saved yet'}
        >
          <Database size={13} />
          {#if dumpInfo.exists}
            <strong>{savedDumpApps.length}</strong><span>saved · {formatRelativeTime(dumpInfo.modified)}</span>
          {:else}
            <span>No dump</span>
          {/if}
        </span>
      {/if}

      {#if lastProcessUpdate}
        <span
          class="fleet-metric"
          title={`Last process update at ${lastProcessUpdate.toLocaleTimeString()}`}
        >
          <Clock size={13} />
          <span>{processPollPaused ? 'paused' : formatRelativeTime(lastProcessUpdate.getTime() / 1000)}</span>
        </span>
      {/if}

      <div class="strip-actions">
        <Button
          variant={processPollPaused ? 'primary' : 'ghost'}
          size="xs"
          onclick={() => processPollPaused = !processPollPaused}
          title={processPollPaused ? 'Resume process telemetry' : 'Pause process telemetry'}
        >
          {#if processPollPaused}<Play size={12} /> Resume{:else}<Pause size={12} /> Pause{/if}
        </Button>
        <Button
          variant="ghost"
          size="xs"
          onclick={() => showActivityDrawer = true}
          title="Recent PM2 actions and incidents"
        >
          <Clock size={12} /> Activity
          {#if activityLog.length > 0}<span class="activity-count">{activityLog.length}</span>{/if}
        </Button>
        <Button
          variant="ghost"
          size="xs"
          onclick={promptReloadAll}
          disabled={onlineApps === 0}
          title="Zero-downtime reload for all online apps"
        >
          <Zap size={12} /> Reload all
        </Button>
        <Button
          variant="ghost"
          size="xs"
          onclick={promptStopAll}
          disabled={onlineApps === 0}
          title="Stop every online app"
        >
          <Square size={12} /> Stop all
        </Button>
        <KebabMenu align="right" title="Fleet actions">
          <button class="menu-item" onclick={() => handleProcessAction('start', 'all')}>
            <Play size={14} class="text-emerald" />
            <span>Start all</span>
          </button>
          <button class="menu-item" onclick={handleSaveDump}>
            <Save size={14} class="text-accent" />
            <span>Save process list</span>
          </button>
          <button class="menu-item" onclick={promptResurrect}>
            <RefreshCcw size={14} class="text-purple" />
            <span>Resurrect saved dump</span>
          </button>
          <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>
          <button class="menu-item" onclick={() => promptFlushLogs(null)}>
            <Trash2 size={14} class="text-amber" />
            <span>Flush all logs</span>
          </button>
        </KebabMenu>
      </div>
    </div>

    {#if fleetError}
      <div class="fleet-error" role="alert">
        <AlertTriangle size={14} />
        <span>PM2 is not responding: {fleetError}</span>
        <Button variant="outline" size="xs" onclick={() => loadData()}>Retry</Button>
      </div>
    {/if}

    {#if dumpInfo && dumpDrift.hasDrift}
      <div class="drift-banner" role="status">
        <Database size={14} />
        <div class="drift-copy">
          <strong>{dumpDrift.noDump ? 'No saved dump yet' : 'Saved dump is out of date'}</strong>
          <span>
            {#if dumpDrift.notSaved.length > 0}
              {dumpDrift.notSaved.length} live app{dumpDrift.notSaved.length === 1 ? '' : 's'} not in the dump.
            {/if}
            {#if dumpDrift.notRunning.length > 0}
              {dumpDrift.notRunning.length} saved app{dumpDrift.notRunning.length === 1 ? '' : 's'} not running.
            {/if}
          </span>
        </div>
        <div class="drift-actions">
          {#if dumpDrift.notSaved.length > 0 || dumpDrift.noDump}
            <Button variant="primary" size="xs" onclick={handleSaveDump}>
              <Save size={12} /> Save dump
            </Button>
          {/if}
          {#if dumpDrift.notRunning.length > 0}
            <Button variant="outline" size="xs" onclick={promptResurrect}>
              <RefreshCcw size={12} /> Resurrect
            </Button>
          {/if}
          <Button variant="ghost" size="xs" onclick={() => activeTab = 'daemon'}>Review</Button>
        </div>
      </div>
    {/if}

    {#each restartStorms as storm (storm.process.pm_id)}
      <div class="storm-banner" role="alert">
        <AlertTriangle size={14} />
        <div class="storm-copy">
          <strong>{storm.process.name}</strong> restarted {storm.count} times in the last 5 minutes.
        </div>
        <div class="storm-actions">
          <Button variant="outline" size="xs" onclick={() => viewStormProcess(storm.process)}>
            View logs
          </Button>
          <Button variant="ghost" size="xs" onclick={() => dismissStorm(storm.process.pm_id)}>
            Dismiss
          </Button>
        </div>
      </div>
    {/each}
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
        {#if processes.length === 0}
          <div class="fleet-empty">
            <div class="fleet-empty-hero">
              <div class="fleet-empty-icon"><Layers size={34} /></div>
              <h2>No PM2 applications yet</h2>
              <p>
                Launch your first Node.js app, restore a saved process list, or start an ecosystem
                config. PM2 keeps it running and restarts it automatically if it crashes.
              </p>
              <div class="fleet-empty-actions">
                <Button variant="primary" onclick={() => showLaunchModal = true}>
                  <Plus size={15} /> Launch application
                </Button>
                {#if savedDumpApps.length > 0}
                  <Button variant="outline" onclick={promptResurrect}>
                    <RefreshCcw size={14} /> Restore saved dump ({savedDumpApps.length})
                  </Button>
                {/if}
                <Button variant="outline" onclick={() => activeTab = 'ecosystem'}>
                  <FileCode size={14} /> Browse ecosystem configs
                </Button>
              </div>
            </div>

            {#if savedDumpApps.length > 0}
              <div class="fleet-empty-saved">
                <div class="fleet-empty-saved-head">
                  <span><Database size={13} /> Saved processes ({savedDumpApps.length})</span>
                  <Button variant="ghost" size="xs" onclick={promptResurrect}>
                    <RefreshCcw size={12} /> Resurrect all
                  </Button>
                </div>
                {#each savedDumpApps as app (app.namespace + ':' + app.name)}
                  <div class="fleet-empty-saved-row">
                    <div class="fleet-empty-saved-info">
                      <strong>{app.name}</strong>
                      <span title={app.script_path}>{app.script_path.split('/').pop() || app.script_path}</span>
                    </div>
                    <Button variant="primary" size="xs" onclick={() => handleStartSavedApp(app)}>
                      <Play size={11} /> Run
                    </Button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <div class="fleet-workspace">
          <!-- ── PROCESSES PANE ─────────────────────────────────────────────── -->
          <div class="fleet-master-pane">
            <!-- Search & Filter Header -->
            <div class="master-header">
              <div class="master-search-row">
                <SearchBar
                  bind:value={searchQuery}
                  placeholder="Filter by name, PID, ID…"
                  style="margin: 0; flex: 1; min-width: 0;"
                />
                <div class="master-sort-wrap">
                  <Select
                    value={processSortKey}
                    size="sm"
                    ariaLabel="Sort processes by"
                    style="width: 100%;"
                    onchange={(event) => {
                      const key = event.target.value as typeof processSortKey;
                      processSortKey = key;
                      processSortAsc = key === 'name' || key === 'status';
                    }}
                  >
                    <option value="name">Name</option>
                    <option value="status">Status</option>
                    <option value="cpu">CPU</option>
                    <option value="memory">Memory</option>
                    <option value="restarts">Restarts</option>
                    <option value="uptime">Uptime</option>
                    <option value="instances">Instances</option>
                  </Select>
                </div>
                <button
                  type="button"
                  class="sort-dir-btn"
                  onclick={() => processSortAsc = !processSortAsc}
                  title={processSortAsc ? 'Sort ascending' : 'Sort descending'}
                  aria-label={processSortAsc ? 'Sort ascending' : 'Sort descending'}
                >
                  {processSortAsc ? '↑' : '↓'}
                </button>
              </div>
              <div class="master-toolbar-row">
                {#if namespaceOptions.length > 1}
                  <div class="master-ns-wrap">
                    <Select
                      bind:value={namespaceFilter}
                      size="sm"
                      ariaLabel="Filter by namespace"
                      style="width: 100%;"
                    >
                      <option value="all">All namespaces</option>
                      {#each namespaceOptions as namespace}
                        <option value={namespace}>{namespace}</option>
                      {/each}
                    </Select>
                  </div>
                {/if}

                <div class="view-toggle" role="group" aria-label="Fleet view">
                  <button
                    type="button"
                    class:active={fleetView === 'cards'}
                    aria-pressed={fleetView === 'cards'}
                    onclick={() => fleetView = 'cards'}
                    title="Card view"
                  >
                    <LayoutGrid size={13} />
                  </button>
                  <button
                    type="button"
                    class:active={fleetView === 'table'}
                    aria-pressed={fleetView === 'table'}
                    onclick={() => fleetView = 'table'}
                    title="Table view"
                  >
                    <Table2 size={13} />
                  </button>
                </div>

                <span class="master-count">{filteredProcesses.length} / {processes.length}</span>
              </div>
            </div>

            <!-- Scrollable Process Cards List -->
            <div class="master-cards-list" class:table-list={fleetView === 'table'}>
              {#if filteredProcesses.length === 0}
                <div class="empty-list-container">
                  <EmptyState
                    icon={Layers}
                    title="No apps match"
                    description="No processes match the current search or status filter."
                    actionLabel="Clear filters"
                    actionIcon={RefreshCcw}
                    onAction={() => { searchQuery = ''; statusFilter = 'all'; namespaceFilter = 'all'; }}
                  />
                </div>
              {:else if fleetView === 'table'}
                <Table class="fleet-table" style="border:none; border-radius:0; width:100%;">
                  <thead>
                    <tr>
                      <th class="sortable-th" onclick={() => toggleProcessSort('name')}>
                        <div class="th-content">App <span class="th-sort-icon">{processSortKey === 'name' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th class="sortable-th" onclick={() => toggleProcessSort('status')}>
                        <div class="th-content">Status <span class="th-sort-icon">{processSortKey === 'status' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th class="sortable-th" onclick={() => toggleProcessSort('cpu')}>
                        <div class="th-content">CPU <span class="th-sort-icon">{processSortKey === 'cpu' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th class="sortable-th" onclick={() => toggleProcessSort('memory')}>
                        <div class="th-content">RAM <span class="th-sort-icon">{processSortKey === 'memory' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th class="sortable-th" onclick={() => toggleProcessSort('restarts')}>
                        <div class="th-content">RST <span class="th-sort-icon">{processSortKey === 'restarts' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th class="sortable-th" onclick={() => toggleProcessSort('uptime')}>
                        <div class="th-content">Up <span class="th-sort-icon">{processSortKey === 'uptime' ? (processSortAsc ? '↑' : '↓') : '↕'}</span></div>
                      </th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each filteredProcesses as p (p.pm_id)}
                      {@const memoryLimit = parseMemoryLimit(p.max_memory_restart)}
                      {@const memoryRatio = memoryLimit ? (p.memory || 0) / memoryLimit : 0}
                      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                      <tr
                        class:selected={selectedProcess?.pm_id === p.pm_id}
                        tabindex="0"
                        class="fleet-row"
                        onclick={() => handleSelectProcess(p.pm_id)}
                        oncontextmenu={(event) => openProcessContextMenu(event, p)}
                        onkeydown={(event) => {
                          if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault();
                            handleSelectProcess(p.pm_id);
                          }
                        }}
                      >
                        <td>
                          <div class="fleet-app-cell">
                            <span class="status-dot {p.status}"></span>
                            <div class="fleet-app-info">
                              <span class="fleet-app-name" title={p.name}>{p.name}</span>
                              <span class="fleet-app-sub">
                                #{p.pm_id} · {p.exec_mode === 'cluster_mode' ? `cluster×${p.instances}` : 'fork'}
                                {#if p.namespace && p.namespace !== 'default'} · {p.namespace}{/if}
                                {#if p.is_module} · module{/if}
                              </span>
                            </div>
                          </div>
                        </td>
                        <td>
                          <span class="status-text {p.status}">{p.status}</span>
                        </td>
                        <td class="fleet-num">{p.cpu.toFixed(1)}%</td>
                        <td
                          class="fleet-num"
                          title={memoryLimit
                            ? `${formatBytes(p.memory)} of ${memoryLimitLabel(p.max_memory_restart)}`
                            : formatBytes(p.memory)}
                        >
                          {formatBytes(p.memory)}
                          {#if memoryLimit}
                            <span class="fleet-limit" class:warn={memoryRatio > 0.8}>
                              /{memoryLimitLabel(p.max_memory_restart)}
                            </span>
                          {/if}
                        </td>
                        <td class="fleet-num" class:text-rose={p.unstable_restarts > 0}>
                          {p.restarts}{#if p.unstable_restarts > 0} ⚠{/if}
                        </td>
                        <td class="fleet-num">{p.status === 'online' ? formatUptime(p.uptime) : '—'}</td>
                        <td
                          class="fleet-actions"
                          onclick={(event) => event.stopPropagation()}
                          onkeydown={(event) => event.stopPropagation()}
                        >
                          <KebabMenu align="right" title="Process actions">
                            <button class="menu-item" onclick={() => openLogsForProcess(p)}>
                              <Terminal size={14} class="text-accent" />
                              <span>View logs</span>
                            </button>
                            <button class="menu-item" onclick={() => handleSelectProcess(p.pm_id, true)}>
                              <Info size={14} class="text-accent" />
                              <span>Details &amp; environment</span>
                            </button>
                            <button class="menu-item" onclick={() => copyPm2Command(p)}>
                              <Copy size={14} class="text-accent" />
                              <span>Copy pm2 command</span>
                            </button>
                            {#if p.status === 'online'}
                              <button class="menu-item" onclick={() => handleProcessAction('reload', p.pm_id)}>
                                <Zap size={14} class="text-accent" />
                                <span>Reload (zero downtime)</span>
                              </button>
                              <button class="menu-item" onclick={() => handleProcessAction('restart', p.pm_id)}>
                                <RotateCw size={14} />
                                <span>Restart</span>
                              </button>
                              <button class="menu-item" onclick={() => handleProcessAction('stop', p.pm_id)}>
                                <Square size={14} class="text-amber" />
                                <span>Stop</span>
                              </button>
                            {:else}
                              <button class="menu-item" onclick={() => handleProcessAction('start', p.pm_id)}>
                                <Play size={14} class="text-emerald" />
                                <span>Start</span>
                              </button>
                            {/if}
                            {#if p.exec_mode === 'cluster_mode'}
                              <button class="menu-item" onclick={() => openScaleModal(p)}>
                                <Layers size={14} class="text-accent" />
                                <span>Scale instances…</span>
                              </button>
                            {/if}
                            <button class="menu-item" onclick={() => handleProcessAction('reset', p.pm_id)}>
                              <RefreshCcw size={14} class="text-accent" />
                              <span>Reset restart counters</span>
                            </button>
                            <button class="menu-item" onclick={() => promptFlushLogs(p)}>
                              <Trash2 size={14} class="text-amber" />
                              <span>Flush logs</span>
                            </button>
                            <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>
                            <button class="menu-item text-danger" onclick={() => promptDeleteProcess(p)}>
                              <Trash2 size={14} class="text-error" />
                              <span>Delete process</span>
                            </button>
                          </KebabMenu>
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </Table>
              {:else}
                {#each filteredProcesses as p (p.pm_id)}
                  {@const isSelected = selectedProcess?.pm_id === p.pm_id}
                  {@const isOnline = p.status === 'online'}
                  {@const isErrored = p.status === 'errored'}
                  {@const memoryLimit = parseMemoryLimit(p.max_memory_restart)}
                  {@const memoryRatio = memoryLimit ? (p.memory || 0) / memoryLimit : 0}
                  {@const history = processHistory[p.pm_id]}

                  <div 
                    class="proc-card"
                    class:selected={isSelected}
                    class:errored={isErrored}
                    role="button"
                    tabindex="0"
                    onclick={() => handleSelectProcess(p.pm_id)}
                    oncontextmenu={(event) => openProcessContextMenu(event, p)}
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
                        {#if p.is_module}
                          <span class="proc-version-tag">module</span>
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
                        <svg class="meter-spark cpu-spark" viewBox="0 0 100 16" preserveAspectRatio="none">
                          <polyline points={sparklinePoints(history?.cpu || [], 100, 16, 100)} />
                        </svg>
                      </div>

                      <div class="meter-col">
                        <div class="meter-label-row">
                          <span class="m-label">RAM</span>
                          <span class="m-val font-mono">
                            {formatBytes(p.memory)}
                            {#if memoryLimit}<span class="limit-hint"> / {memoryLimitLabel(p.max_memory_restart)}</span>{/if}
                          </span>
                        </div>
                        <div class="mini-track">
                          <div
                            class="mini-bar mem-bar"
                            style="width: {memoryLimit
                              ? Math.min(100, memoryRatio * 100)
                              : Math.min(100, (p.memory / (1024 * 1024 * 1024)) * 100)}%; background: {memoryRatio > 0.9
                              ? 'var(--color-error)'
                              : memoryRatio > 0.8
                                ? 'var(--color-warning)'
                                : '#a855f7'};"
                          ></div>
                        </div>
                        <svg class="meter-spark mem-spark" viewBox="0 0 100 16" preserveAspectRatio="none">
                          <polyline
                            points={sparklinePoints(history?.mem || [], 100, 16, memoryLimit || undefined)}
                          />
                        </svg>
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
                        <button
                          class="card-action-btn"
                          onclick={() => handleSelectProcess(p.pm_id, true)}
                          title="Inspect application details"
                        >
                          <Info size={12} />
                        </button>
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
                        <KebabMenu align="right" title="More actions">
                          <button class="menu-item" onclick={() => openLogsForProcess(p)}>
                            <Terminal size={14} class="text-accent" />
                            <span>View logs</span>
                          </button>
                          <button class="menu-item" onclick={() => handleSelectProcess(p.pm_id, true)}>
                            <Info size={14} class="text-accent" />
                            <span>Details &amp; environment</span>
                          </button>
                          <button class="menu-item" onclick={() => copyPm2Command(p)}>
                            <Copy size={14} class="text-accent" />
                            <span>Copy pm2 command</span>
                          </button>
                          {#if p.exec_mode === 'cluster_mode'}
                            <button class="menu-item" onclick={() => openScaleModal(p)}>
                              <Layers size={14} class="text-accent" />
                              <span>Scale instances…</span>
                            </button>
                          {/if}
                          <button class="menu-item" onclick={() => handleProcessAction('reset', p.pm_id)}>
                            <RefreshCcw size={14} class="text-accent" />
                            <span>Reset restart counters</span>
                          </button>
                          <button class="menu-item" onclick={() => promptFlushLogs(p)}>
                            <Trash2 size={14} class="text-amber" />
                            <span>Flush logs</span>
                          </button>
                          <div style="height: 1px; background: var(--color-border-subtle); margin: 4px 0;"></div>
                          <button class="menu-item text-danger" onclick={() => promptDeleteProcess(p)}>
                            <Trash2 size={14} class="text-error" />
                            <span>Delete process</span>
                          </button>
                        </KebabMenu>
                      </div>
                    </div>
                  </div>
                {/each}
              {/if}
            </div>
          </div>

          <aside class="fleet-saved-pane">
            <div class="saved-pane-head">
              <div class="saved-pane-title">
                <Database size={15} />
                <span>Saved processes</span>
              </div>
              <Button variant="ghost" size="xs" onclick={() => activeTab = 'daemon'} title="Open the saved apps manager">
                Manage
              </Button>
            </div>
            <div class="saved-pane-meta">
              {#if dumpInfo?.exists}
                <span>{savedDumpApps.length} app{savedDumpApps.length === 1 ? '' : 's'} · saved {formatRelativeTime(dumpInfo.modified)}</span>
              {:else}
                <span>No dump file yet</span>
              {/if}
              {#if dumpDrift.hasDrift}
                <span class="saved-pane-warn">
                  {#if dumpDrift.notRunning.length > 0}{dumpDrift.notRunning.length} not running{/if}
                  {#if dumpDrift.notRunning.length > 0 && dumpDrift.notSaved.length > 0} · {/if}
                  {#if dumpDrift.notSaved.length > 0}{dumpDrift.notSaved.length} not saved{/if}
                </span>
              {/if}
            </div>
            <div class="saved-pane-actions">
              <Button variant="outline" size="xs" onclick={handleSaveDump}>
                <Save size={12} /> Save
              </Button>
              <Button
                variant="outline"
                size="xs"
                onclick={promptResurrect}
                disabled={savedDumpApps.length === 0}
              >
                <RefreshCcw size={12} /> Resurrect
              </Button>
            </div>

            {#if savedDumpApps.length === 0}
              <p class="saved-pane-empty">No saved apps yet. Save the current fleet to persist it across reboots.</p>
            {:else}
              <div class="saved-pane-list">
                {#each savedDumpApps as app (app.namespace + ':' + app.name)}
                  <div class="saved-pane-row">
                    <span class="status-dot {app.is_currently_running ? 'online' : 'stopped'}"></span>
                    <div class="saved-pane-info">
                      <strong title={app.name}>{app.name}</strong>
                      <span title={app.script_path}>{app.script_path.split('/').pop() || app.script_path}</span>
                    </div>
                    {#if app.is_currently_running && app.current_pm_id != null}
                      <Button
                        variant="ghost"
                        size="xs"
                        onclick={() => {
                          const proc = processes.find((item) => item.pm_id === app.current_pm_id);
                          if (proc) openLogsForProcess(proc);
                        }}
                        title="Open this app's logs"
                      >
                        Logs
                      </Button>
                    {:else}
                      <Button
                        variant="primary"
                        size="xs"
                        onclick={() => handleStartSavedApp(app)}
                        title="Start this saved app"
                      >
                        <Play size={11} /> Run
                      </Button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          </aside>
          </div>

          <SideDrawer
            bind:isOpen={showLogsDrawer}
            title={`Logs — ${selectedProcess?.name ?? 'PM2'}`}
            width="900px"
          >
            <div class="logs-drawer">
            {#if selectedProcess}
              {@const detailMemoryLimit = parseMemoryLimit(selectedProcess.max_memory_restart)}
              <div class="detail-app-head">
                <div class="detail-app-identity">
                  <span class="status-dot {selectedProcess.status}"></span>
                  <strong class="detail-app-name" title={selectedProcess.name}>{selectedProcess.name}</strong>
                  <span class="proc-id-tag">#{selectedProcess.pm_id}</span>
                  <span class="mode-tag {selectedProcess.exec_mode === 'cluster_mode' ? 'cluster' : 'fork'}">
                    {selectedProcess.exec_mode === 'cluster_mode' ? `cluster (${selectedProcess.instances})` : 'fork'}
                  </span>
                </div>
                <div class="detail-app-meta">
                  <span>CPU <strong>{selectedProcess.cpu.toFixed(1)}%</strong></span>
                  <span>
                    RAM <strong>{formatBytes(selectedProcess.memory)}</strong>
                    {#if detailMemoryLimit}<span class="limit-hint"> / {memoryLimitLabel(selectedProcess.max_memory_restart)}</span>{/if}
                  </span>
                  <span>{selectedProcess.status === 'online' ? formatUptime(selectedProcess.uptime) : 'stopped'}</span>
                  {#if selectedProcess.node_version}
                    <span title="Node.js version">node {selectedProcess.node_version}</span>
                  {/if}
                  {#if selectedProcess.interpreter && selectedProcess.interpreter !== 'node'}
                    <span title="Interpreter">{selectedProcess.interpreter}</span>
                  {/if}
                  {#if selectedProcess.namespace && selectedProcess.namespace !== 'default'}
                    <span title="PM2 namespace">{selectedProcess.namespace}</span>
                  {/if}
                </div>
                <div class="detail-app-actions">
                  {#if selectedProcess.status === 'online'}
                    <button
                      class="card-action-btn action-reload"
                      onclick={() => handleProcessAction('reload', selectedProcess!.pm_id)}
                      title="Reload (zero downtime)"
                    >
                      <Zap size={13} />
                    </button>
                    <button
                      class="card-action-btn action-restart"
                      onclick={() => handleProcessAction('restart', selectedProcess!.pm_id)}
                      title="Restart"
                    >
                      <RotateCw size={13} />
                    </button>
                    <button
                      class="card-action-btn action-stop"
                      onclick={() => handleProcessAction('stop', selectedProcess!.pm_id)}
                      title="Stop"
                    >
                      <Square size={13} />
                    </button>
                  {:else}
                    <button
                      class="card-action-btn action-start"
                      onclick={() => handleProcessAction('start', selectedProcess!.pm_id)}
                      title="Start"
                    >
                      <Play size={13} />
                    </button>
                  {/if}
                  <button
                    class="card-action-btn"
                    onclick={() => showProcessDrawer = true}
                    title="Inspect details and environment"
                  >
                    <Info size={13} />
                  </button>
                  <KebabMenu align="right" title="Application options">
                    <button class="menu-item" onclick={() => showProcessDrawer = true}>
                      <Info size={14} class="text-accent" />
                      <span>Details &amp; environment</span>
                    </button>
                    <button class="menu-item" onclick={() => copyPm2Command(selectedProcess!)}>
                      <Copy size={14} class="text-accent" />
                      <span>Copy pm2 command</span>
                    </button>
                    {#if selectedProcess.exec_mode === 'cluster_mode'}
                      <button class="menu-item" onclick={() => openScaleModal(selectedProcess!)}>
                        <Layers size={14} class="text-accent" />
                        <span>Scale instances…</span>
                      </button>
                    {/if}
                    <button class="menu-item" onclick={() => handleProcessAction('reset', selectedProcess!.pm_id)}>
                      <RefreshCcw size={14} class="text-accent" />
                      <span>Reset restart counters</span>
                    </button>
                    <button class="menu-item" onclick={() => promptFlushLogs(selectedProcess)}>
                      <Trash2 size={14} class="text-error" />
                      <span>Flush logs</span>
                    </button>
                    <button class="menu-item text-danger" onclick={() => promptDeleteProcess(selectedProcess!)}>
                      <Trash2 size={14} class="text-error" />
                      <span>Delete process</span>
                    </button>
                  </KebabMenu>
                </div>
              </div>
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
                        size="sm"
                        style="width: 95px;"
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
                      variant={logErrorsOnly ? 'primary' : 'outline'}
                      size="sm"
                      onclick={() => logErrorsOnly = !logErrorsOnly}
                      title="Show only ERROR lines"
                    >
                      <AlertTriangle size={12} />
                      <span>Errors</span>
                    </Button>

                    <Button
                      variant={logAutoRefresh ? 'primary' : 'outline'}
                      size="sm"
                      onclick={() => logAutoRefresh = !logAutoRefresh}
                      title="Toggle live auto polling"
                    >
                      <RotateCw size={12} class={logAutoRefresh ? 'spin' : ''} />
                      <span>{logAutoRefresh ? 'Live (2s)' : 'Paused'}</span>
                    </Button>

                    <Button
                      variant={logFollow ? 'outline' : 'ghost'}
                      size="sm"
                      onclick={() => {
                        if (logFollow) {
                          logFollow = false;
                        } else {
                          jumpToLatest();
                        }
                      }}
                      title="Auto-scroll to the newest log line"
                    >
                      <ArrowDown size={12} />
                      <span>{logFollow ? 'Following' : 'Follow'}</span>
                    </Button>

                    <KebabMenu align="right" title="Application & Log Options">
                      <button class="menu-item" onclick={() => showProcessDrawer = true}>
                        <Info size={14} class="text-accent" />
                        <span>App Details & Telemetry</span>
                      </button>

                      <button class="menu-item" onclick={openLogFolder}>
                        <Folder size={14} class="text-accent" />
                        <span>Open log folder</span>
                      </button>

                      <button class="menu-item" onclick={downloadLogFile} disabled={exportingLog}>
                        <Download size={14} class="text-accent" />
                        <span>{exportingLog ? 'Saving log…' : 'Download log'}</span>
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
                      <button class="menu-item text-danger" onclick={() => promptFlushLogs(null)}>
                        <Trash2 size={14} class="text-error" />
                        <span>Flush all logs</span>
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
                    <div class="log-header-actions">
                      <button
                        type="button"
                        class="log-header-btn"
                        onclick={openLogFolder}
                        title="Open log folder"
                      >
                        <Folder size={12} />
                      </button>
                      <button
                        type="button"
                        class="log-header-btn"
                        onclick={downloadLogFile}
                        disabled={exportingLog}
                        title="Download the full log file"
                      >
                        <Download size={12} />
                      </button>
                      <span class="log-count-tag font-mono">
                        {parsedLogs.length} lines
                      </span>
                    </div>
                  </div>

                  <div class="log-lines-container" bind:this={logContainerRef} onscroll={handleLogScroll}>
                    {#if logLoading && parsedLogs.length === 0}
                      <div class="log-state-msg">
                        <RotateCw size={22} class="spin text-accent" />
                        <span>Streaming PM2 log output…</span>
                      </div>
                    {:else if parsedLogs.length === 0}
                      <div class="log-state-msg">
                        <Terminal size={28} class="text-muted" />
                        <p>
                          {logSearchQuery.trim() || logErrorsOnly
                            ? 'No log lines match the current filters.'
                            : 'No log output recorded in this file yet.'}
                        </p>
                      </div>
                    {:else}
                      <div class="log-table">
                        <div class="log-table-head">
                          <span>#</span>
                          <span>Time</span>
                          <span>Level</span>
                          <span>Context</span>
                          <span>Message</span>
                        </div>
                        {#each parsedLogs as entry}
                          <div class="log-row {entry.level === 'error' ? 'row-error' : entry.level === 'warn' ? 'row-warn' : ''}">
                            <span class="log-line-no font-mono">{entry.lineNumber}</span>
                            <span class="log-ts font-mono">{entry.timestamp || '—'}</span>
                            <span class="log-level-badge {entry.level === 'error' ? 'level-err' : entry.level === 'warn' ? 'level-warn' : 'level-info'}">
                              {entry.level === 'error' ? 'ERROR' : entry.level === 'warn' ? 'WARN' : 'INFO'}
                            </span>
                            <span class="log-context font-mono" title={entry.context || ''}>{entry.context || '—'}</span>
                            <span class="log-msg font-mono">{entry.message}</span>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>
                  {#if !logAtBottom}
                    <button
                      type="button"
                      class="jump-latest-btn"
                      onclick={jumpToLatest}
                      title="Jump to the newest log line"
                    >
                      <ArrowDown size={14} />
                      <span>Jump to latest</span>
                    </button>
                  {/if}
                </div>
              </div>
            {:else}
              <div class="no-selection-wrap">
                <Layers size={40} class="text-muted" />
                <h3>No application matches</h3>
                <p>Clear the search or status filter to bring your applications back.</p>
                <Button variant="outline" onclick={() => { searchQuery = ''; statusFilter = 'all'; namespaceFilter = 'all'; }}>
                  <RefreshCcw size={14} /> Clear filters
                </Button>
              </div>
            {/if}
            </div>
          </SideDrawer>
        {/if}

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
                    disabled={!isEcosystemDirty || ecosystemValidating}
                  >
                    <Save size={13} />
                    <span>{ecosystemValidating ? 'Checking…' : 'Save'}</span>
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

              <div class="ecosystem-start-options">
                <label class="eco-option-field">
                  <span>Only apps</span>
                  <input
                    class="form-input form-input-sm"
                    bind:value={ecosystemOnly}
                    list="ecosystem-app-names"
                    placeholder="api,worker"
                    title="Comma-separated app names; leave empty to start every app in the config"
                  />
                </label>
                <label class="eco-option-field">
                  <span>Environment</span>
                  <input
                    class="form-input form-input-sm"
                    bind:value={ecosystemEnv}
                    placeholder="e.g. production"
                  />
                </label>
                <span class="eco-option-hint">
                  Optional <code>--only</code> and <code>--env</code> flags for <code>pm2 start</code>
                </span>
                <datalist id="ecosystem-app-names">
                  {#each selectedEcosystemFile.app_names as appName}
                    <option value={appName}></option>
                  {/each}
                </datalist>
              </div>

              {#if ecosystemValidationError}
                <div class="ecosystem-validation-error" role="alert">
                  <AlertTriangle size={13} class="flex-shrink-0" />
                  <pre>{ecosystemValidationError}</pre>
                  <button
                    type="button"
                    class="eco-error-close"
                    onclick={() => ecosystemValidationError = ''}
                    aria-label="Dismiss validation error"
                  >
                    ×
                  </button>
                </div>
              {/if}

              <div class="code-editor-wrap">
                <CodeEditor
                  value={ecosystemContent}
                  language="javascript"
                  height="100%"
                  onchange={(newVal) => { ecosystemContent = newVal; ecosystemValidationError = ''; }}
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
        <div class="pm2-scope-note">
          <Info size={14} class="flex-shrink-0" />
          <div>
            <strong>Scope: current user only</strong>
            <p>
              This view reads the PM2 instance for
              <code>{startupStatus?.user || 'the current user'}</code> in
              <code>{systemStatus.pm2_home}</code>. Apps started by root or another Linux user
              are not visible from this page.
            </p>
          </div>
        </div>

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

          <!-- Log Health & Rotation Card -->
          <div class="daemon-card log-health-card">
            <div class="daemon-card-header">
              <div class="icon-bubble" style="background: rgba(56, 189, 248, 0.12); color: #38bdf8;">
                <FileText size={18} />
              </div>
              <div>
                <h3>Log Health &amp; Rotation</h3>
                <p>
                  {logOverview?.directory || `${systemStatus.pm2_home}/logs`}
                </p>
              </div>
            </div>

            {#if logOverviewLoading && !logOverview}
              <p class="daemon-note">
                <RotateCw size={13} class="spin" /> Measuring log files…
              </p>
            {:else if logOverview}
              <div class="log-health-summary">
                <div>
                  <span class="log-health-label">Total log size</span>
                  <strong class="log-health-value font-mono">{formatBytes(logOverview.total_size)}</strong>
                </div>
                <div>
                  <span class="log-health-label">pm2-logrotate</span>
                  {#if logOverview.logrotate_installed}
                    <span class="badge badge-success">
                      Installed{logOverview.logrotate_version ? ` v${logOverview.logrotate_version}` : ''}
                    </span>
                  {:else}
                    <span class="badge badge-neutral">Not installed</span>
                  {/if}
                </div>
              </div>

              {#if logOverview.files.length > 0}
                <div class="log-file-list">
                  {#each logOverview.files as file}
                    <div class="log-file-row">
                      <span class="log-file-name font-mono" title={file.name}>{file.name}</span>
                      <span class="log-file-size font-mono">{formatBytes(file.size)}</span>
                    </div>
                  {/each}
                  {#if logOverview.hidden_files}
                    <div class="log-file-row log-file-more">
                      <span>+{logOverview.hidden_files} more files</span>
                    </div>
                  {/if}
                </div>
              {:else}
                <p class="daemon-note">No log files have been written yet.</p>
              {/if}

              {#if !logOverview.logrotate_installed}
                <div class="logrotate-install">
                  <p>Install rotation to stop logs from growing without limit:</p>
                  <div class="code-terminal">
                    <code>pm2 install pm2-logrotate</code>
                    <button
                      class="copy-btn"
                      onclick={() => copyToClipboard('pm2 install pm2-logrotate', 'logrotate')}
                      title="Copy install command"
                    >
                      {#if copiedCommand === 'logrotate'}
                        <Check size={14} class="text-emerald" />
                      {:else}
                        <Copy size={14} />
                      {/if}
                    </button>
                  </div>
                </div>
              {/if}
            {:else}
              <p class="daemon-note">Log metrics are unavailable.</p>
            {/if}

            <div class="card-footer-actions">
              <Button variant="outline" size="sm" onclick={openLogFolder} title="Open the PM2 log directory">
                <Folder size={12} />
                <span>Open folder</span>
              </Button>
              <Button
                variant="outline"
                size="sm"
                onclick={loadLogOverview}
                disabled={logOverviewLoading}
                title="Measure log files again"
              >
                <RefreshCcw size={12} class={logOverviewLoading ? 'spin' : ''} />
                <span>Refresh</span>
              </Button>
              <Button variant="outline" size="sm" onclick={() => promptFlushLogs(null)} title="Empty every PM2 log file">
                <Trash2 size={12} class="text-amber" />
                <span>Flush all</span>
              </Button>
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
                  <p>
                    Applications persisted in <code>{systemStatus.pm2_home}/dump.pm2</code>
                    ({savedDumpApps.length} apps saved)
                    {#if dumpInfo?.exists}
                      · last saved {formatRelativeTime(dumpInfo.modified)}
                      ({(dumpInfo.size / 1024).toFixed(1)} KB)
                    {:else}
                      · no dump file yet
                    {/if}
                  </p>
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
                        {#if app.namespace && app.namespace !== 'default'}
                          <span class="mode-badge" title="PM2 namespace">{app.namespace}</span>
                        {/if}
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
                          onclick={() => promptDeleteSavedApp(app)}
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
    {:else if loading}
      <div class="pm2-loading">
        <RotateCw size={22} class="spin" />
        <span>Connecting to PM2…</span>
      </div>
    {/if}
  </div>
</div>

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- SIDE DRAWER: APPLICATION DETAILS INSPECTOR -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if selectedProcess}
  {@const selectedHistory = processHistory[selectedProcess.pm_id]}
  {@const selectedMemoryLimit = parseMemoryLimit(selectedProcess.max_memory_restart)}
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
            <svg class="vital-spark cpu-spark" viewBox="0 0 100 18" preserveAspectRatio="none">
              <polyline points={sparklinePoints(selectedHistory?.cpu || [], 100, 18, 100)} />
            </svg>
            <span class="vital-sub">Multi-core process load</span>
          </div>

          <div class="vital-card">
            <div class="vital-card-top">
              <span class="vital-card-title">Memory Heap</span>
              <HardDrive size={15} class="text-purple" />
            </div>
            <span class="vital-value font-mono text-purple">
              {formatBytes(selectedProcess.memory)}
              {#if selectedMemoryLimit}<span class="vital-limit">/ {memoryLimitLabel(selectedProcess.max_memory_restart)}</span>{/if}
            </span>
            <div class="v-track">
              <div
                class="v-bar"
                style="width: {Math.min(100, selectedMemoryLimit ? (selectedProcess.memory / selectedMemoryLimit) * 100 : (selectedProcess.memory / (1024 * 1024 * 1024)) * 100)}%; background: {selectedMemoryLimit && selectedProcess.memory / selectedMemoryLimit > 0.8 ? 'var(--color-warning)' : '#a855f7'};"
              ></div>
            </div>
            <svg class="vital-spark mem-spark" viewBox="0 0 100 18" preserveAspectRatio="none">
              <polyline
                points={sparklinePoints(selectedHistory?.mem || [], 100, 18, selectedMemoryLimit || undefined)}
              />
            </svg>
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
              <span class="detail-label">Namespace</span>
              <span class="detail-val font-mono">{selectedProcess.namespace || 'default'}</span>
            </div>

            {#if selectedProcess.node_version}
              <div class="detail-row">
                <span class="detail-label">Node.js Version</span>
                <span class="detail-val font-mono">{selectedProcess.node_version}</span>
              </div>
            {/if}

            {#if selectedProcess.interpreter}
              <div class="detail-row">
                <span class="detail-label">Interpreter</span>
                <span class="detail-val font-mono">{selectedProcess.interpreter}</span>
              </div>
            {/if}

            {#if selectedProcess.max_memory_restart}
              <div class="detail-row">
                <span class="detail-label">Memory Limit</span>
                <span class="detail-val font-mono">{selectedProcess.max_memory_restart}</span>
              </div>
            {/if}

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
              style="margin: 0; flex: 1; min-width: 0;"
            />
            <span class="env-mask-note" title="Keys such as TOKEN, SECRET, PASSWORD and API_KEY are masked">
              <Eye size={12} /> Sensitive values are masked
            </span>
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
                      <td
                        class="font-mono"
                        style="word-break: break-all; vertical-align: top; padding-top: 9px;"
                        title={envIsSensitive(k) && !revealedEnvKeys.has(k) ? 'Sensitive value hidden' : v || ''}
                      >
                        {#if v === '' || v === undefined || v === null}
                          <span class="empty-env-tag font-mono">(empty string)</span>
                        {:else}
                          <span>{envDisplayValue(k, v)}</span>
                        {/if}
                      </td>
                      <td style="text-align: right; vertical-align: top; padding-top: 6px; white-space: nowrap;">
                        {#if envIsSensitive(k)}
                          <button
                            class="action-btn"
                            onclick={() => toggleEnvReveal(k)}
                            title={revealedEnvKeys.has(k) ? 'Hide value' : 'Reveal value'}
                          >
                            <Eye size={13} />
                          </button>
                        {/if}
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

<SideDrawer bind:isOpen={showActivityDrawer} title="PM2 Activity" width="520px">
  <div class="activity-drawer">
    <div class="activity-head">
      <div>
        <strong>Recent activity</strong>
        <span>Actions and incidents, newest first (last 50).</span>
      </div>
      <Button variant="outline" size="xs" onclick={clearActivity} disabled={activityLog.length === 0}>
        <Trash2 size={12} /> Clear
      </Button>
    </div>

    {#if activityLog.length === 0}
      <div class="activity-empty">
        <Clock size={28} />
        <p>No activity recorded yet.</p>
      </div>
    {:else}
      <div class="activity-list">
        {#each activityLog as event (event.id)}
          <div class="activity-row tone-{event.tone}">
            <span class="activity-dot"></span>
            <div class="activity-body">
              <div class="activity-title">{event.title}</div>
              {#if event.detail}
                <div class="activity-detail">{event.detail}</div>
              {/if}
              <div class="activity-time">{formatActivityTime(event.time)}</div>
            </div>
            {#if event.pmId != null}
              <Button
                variant="outline"
                size="xs"
                onclick={() => openActivityProcess(event)}
                title="Open logs for this process"
              >
                <Terminal size={12} />
                <span>Open logs</span>
              </Button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</SideDrawer>

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: LAUNCH NEW APP (3-STEP GUIDED WIZARD) -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showLaunchModal}
  <div use:portal class="modal-backdrop" onclick={() => showLaunchModal = false} role="presentation">
    <div class="modal-card modal-wizard-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
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
                  onblur={() => inferLaunchHints()}
                  class="form-input"
                />
                <Button variant="outline" onclick={browseScriptFile} title="Browse for executable script file">
                  <Folder size={14} class="text-accent" />
                  <span>Browse...</span>
                </Button>
              </div>

              <small class="form-help">
                Select the entry <strong>file</strong> (for example <code>dist/main.js</code>), not its folder.
              </small>

              {#if launchPathLooksLikeFolder}
                <div class="launch-warning">
                  <AlertTriangle size={13} class="flex-shrink-0" />
                  <span>
                    That looks like a folder or build directory. Pick the entry file inside it, such as
                    <code>dist/main.js</code>.
                  </span>
                </div>
              {:else if launchDuplicateError}
                <div class="launch-warning">
                  <AlertTriangle size={13} class="flex-shrink-0" />
                  <span>{launchDuplicateError}</span>
                </div>
              {/if}

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
              <span class="form-label">Execution Mode</span>
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
                <span class="form-label">File Watch Mode</span>
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
              disabled={!launchForm.script_path.trim() || launchPathLooksLikeFolder || !!launchDuplicateError}
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
                if (launchPathLooksLikeFolder) {
                  uiStore.showToast(
                    'Select the entry file inside that folder, such as dist/main.js.',
                    'warning'
                  );
                  return;
                }
                if (launchDuplicateError) {
                  uiStore.showToast(launchDuplicateError, 'warning');
                  return;
                }
                inferLaunchHints();
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
<!-- MODAL: SCALE APPLICATION -->
{#if showScaleModal && scaleTarget}
  <div use:portal class="modal-backdrop" onclick={() => showScaleModal = false} role="presentation">
    <div
      class="modal-card scale-modal-card"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal="true"
      aria-labelledby="scale-modal-title"
      tabindex="-1"
    >
      <div class="modal-header">
        <div class="modal-title-wrap">
          <Layers size={18} class="text-accent" />
          <h3 id="scale-modal-title">Scale {scaleTarget.name}</h3>
        </div>
        <button class="modal-close-btn" onclick={() => showScaleModal = false}>×</button>
      </div>
      <div class="modal-body">
        <p class="scale-copy">
          This application is running in cluster mode with
          <strong>{scaleTarget.instances}</strong> instance(s). PM2 will add or remove workers without
          a full restart.
        </p>
        <label class="form-label" for="scale-instances">Instances</label>
        <div class="scale-input-row">
          <input
            id="scale-instances"
            class="form-input"
            type="number"
            min="1"
            max="64"
            bind:value={scaleInstances}
          />
          <span class="scale-hint">1–64 workers</span>
        </div>
      </div>
      <div class="modal-footer">
        <Button variant="outline" onclick={() => showScaleModal = false} disabled={scaling}>
          Cancel
        </Button>
        <Button variant="primary" onclick={submitScale} disabled={scaling}>
          {scaling ? 'Scaling…' : 'Apply'}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- MODAL: NEW ECOSYSTEM FILE -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if showNewEcosystemModal}
  <div use:portal class="modal-backdrop" onclick={() => showNewEcosystemModal = false} role="presentation">
    <div class="modal-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
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

<!-- Process right-click context menu -->
{#if procContextMenu.show && procContextMenu.process}
  <div
    class="process-context-menu"
    style="left: {procContextMenu.x}px; top: {procContextMenu.y}px;"
    role="menu"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
    onkeydown={(event) => event.stopPropagation()}
    oncontextmenu={(event) => event.preventDefault()}
  >
    <div class="ctx-menu-title" title={procContextMenu.process.name}>
      <span class="status-dot {procContextMenu.process.status}"></span>
      <span>{procContextMenu.process.name}</span>
      <span class="ctx-menu-id">#{procContextMenu.process.pm_id}</span>
    </div>

    <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); openLogsForProcess(proc); }}>
      <Terminal size={14} class="text-accent" />
      <span>View logs</span>
    </button>
    <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleSelectProcess(proc.pm_id, true); }}>
      <Info size={14} class="text-accent" />
      <span>Details &amp; environment</span>
    </button>
    <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); copyPm2Command(proc); }}>
      <Copy size={14} class="text-accent" />
      <span>Copy pm2 command</span>
    </button>

    <div class="ctx-sep"></div>

    {#if procContextMenu.process.status === 'online'}
      <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleProcessAction('reload', proc.pm_id); }}>
        <Zap size={14} class="text-accent" />
        <span>Reload (zero downtime)</span>
      </button>
      <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleProcessAction('restart', proc.pm_id); }}>
        <RotateCw size={14} />
        <span>Restart</span>
      </button>
      <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleProcessAction('stop', proc.pm_id); }}>
        <Square size={14} class="text-amber" />
        <span>Stop</span>
      </button>
    {:else}
      <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleProcessAction('start', proc.pm_id); }}>
        <Play size={14} class="text-emerald" />
        <span>Start</span>
      </button>
    {/if}

    {#if procContextMenu.process.exec_mode === 'cluster_mode'}
      <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); openScaleModal(proc); }}>
        <Layers size={14} class="text-accent" />
        <span>Scale instances…</span>
      </button>
    {/if}
    <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); handleProcessAction('reset', proc.pm_id); }}>
      <RefreshCcw size={14} class="text-accent" />
      <span>Reset restart counters</span>
    </button>
    <button class="ctx-item" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); promptFlushLogs(proc); }}>
      <Trash2 size={14} class="text-amber" />
      <span>Flush logs</span>
    </button>

    <div class="ctx-sep"></div>

    <button class="ctx-item ctx-danger" role="menuitem" onclick={() => { const proc = procContextMenu.process!; closeProcessContextMenu(); promptDeleteProcess(proc); }}>
      <Trash2 size={14} />
      <span>Delete process</span>
    </button>
  </div>
{/if}

<svelte:window
  onclick={closeProcessContextMenu}
  oncontextmenu={() => {
    if (procContextMenu.show) closeProcessContextMenu();
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') closeProcessContextMenu();
  }}
  onblur={closeProcessContextMenu}
/>

{#if selectedEcosystemFile}
  <ConfigDiffModal
    bind:show={showEcosystemDiffModal}
    filePath={selectedEcosystemFile.path}
    title={`Review ${selectedEcosystemFile.name} Changes`}
    oldContent={originalEcosystemContent}
    newContent={ecosystemContent}
    warningMessage="Ensure module export syntax and process names are valid before saving."
    onconfirm={async () => {
      if (await saveEcosystemFile()) showEcosystemDiffModal = false;
    }}
    oncancel={() => showEcosystemDiffModal = false}
  />
{/if}

<style>/* Process right-click context menu */ .process-context-menu {
    position: fixed;
    z-index: 10000;
    min-width: 232px;
    padding: 5px;
    background: var(--color-bg-popover, var(--color-bg-card));
    border: 1px solid var(--color-border);
    border-radius: 9px;
    box-shadow: 0 12px 30px -8px rgba(0, 0, 0, 0.28);
  }.ctx-menu-title {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-bottom: 1px solid var(--color-border-subtle);
    margin-bottom: 4px;
  }.ctx-menu-title > span:nth-child(2) {
    overflow: hidden;
    text-overflow: ellipsis;
  }.ctx-menu-id {
    margin-left: auto;
    font-size: 9.5px;
    font-weight: 500;
    color: var(--color-text-muted);
  }.ctx-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-primary);
    font-family: inherit;
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
  }.ctx-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }.ctx-item.ctx-danger {
    color: var(--color-error);
  }.ctx-item.ctx-danger:hover {
    background: rgba(244, 63, 94, 0.1);
    color: var(--color-error);
  }.ctx-sep {
    height: 1px;
    background: var(--color-border-subtle);
    margin: 4px 2px;
  }/* Root Module Page */ .module-page {
    display: flex;
    flex-direction: column;
    gap: var(--page-content-gap);
    padding: 20px;
    position: absolute;
    inset: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    background: var(--color-bg-base);
    box-sizing: border-box;
  }.header-action-btns {
    display: flex;
    align-items: center;
    gap: 8px;
  }/* Version Badges Group */ .version-badges-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }.v-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }.v-pill.success {
    background: var(--color-success-muted, rgba(16, 185, 129, 0.12));
    color: var(--color-success, #10b981);
    border: 1px solid var(--color-success, #10b981);
  }.v-pill.neutral {
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }/* Controls Row */ .controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }/* Compact fleet health strip */ .fleet-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 6px 10px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    flex-shrink: 0;
  }.fleet-status-group {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-wrap: wrap;
  }.fleet-stat {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 11px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }.fleet-stat strong {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-primary);
  }.fleet-stat:hover {
    background: var(--color-bg-hover);
  }.fleet-stat.active {
    background: var(--color-bg-hover);
    border-color: var(--color-border);
    color: var(--color-text-secondary);
  }.fleet-stat .status-dot {
    width: 6px;
    height: 6px;
  }.strip-sep {
    width: 1px;
    height: 18px;
    background: var(--color-border);
  }.fleet-metric {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--color-text-muted);
  }.fleet-metric strong {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-primary);
  }.fleet-metric.stale {
    color: var(--color-warning);
  }.fleet-metric.stale strong {
    color: var(--color-warning);
  }.strip-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }.fleet-error {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid rgba(244, 63, 94, 0.3);
    border-radius: 8px;
    background: rgba(244, 63, 94, 0.08);
    color: var(--color-warning);
    font-size: 11px;
    flex-shrink: 0;
  }.fleet-error span {
    flex: 1;
    min-width: 0;
  }.activity-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--color-text-secondary);
  }.drift-banner,
  .storm-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 11.5px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }.drift-banner {
    border: 1px solid rgba(245, 158, 11, 0.35);
    background: rgba(245, 158, 11, 0.08);
    color: var(--color-warning);
  }.drift-copy,
  .storm-copy {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }.drift-copy strong,
  .storm-copy strong {
    color: var(--color-text-primary);
    font-size: 11.5px;
  }.drift-copy span {
    color: var(--color-text-muted);
    font-size: 10.5px;
  }.drift-actions,
  .storm-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }.storm-banner {
    border: 1px solid rgba(244, 63, 94, 0.35);
    background: rgba(244, 63, 94, 0.08);
    color: var(--color-error);
  }.storm-copy {
    color: var(--color-text-secondary);
  }/* Main Workspace Body */ .pm2-workspace-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }.pm2-loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--color-text-muted);
    font-size: 12px;
  }/* ═════════════════════════════════════════════════════════════════════════ */ /* FLEET WORKSPACE: PROCESSES + SAVED DUMP */ /* ═════════════════════════════════════════════════════════════════════════ */ .fleet-workspace {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }/* ── PROCESSES PANE ── */ .fleet-master-pane {
    flex: 1;
    min-width: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  }.master-header {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }.master-search-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }.master-sort-wrap {
    width: 112px;
    flex-shrink: 0;
  }.master-ns-wrap {
    width: 132px;
    flex-shrink: 0;
  }.master-toolbar-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }.master-count {
    margin-left: auto;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }.sort-dir-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    font-size: 12px;
    cursor: pointer;
  }.sort-dir-btn:hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }.view-toggle {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-surface);
  }.view-toggle button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }.view-toggle button.active {
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }.empty-list-container {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }/* Process Cards List */ .master-cards-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    align-content: start;
    gap: 8px;
  }.master-cards-list.table-list {
    display: flex;
    flex-direction: column;
    padding: 0;
    gap: 0;
  }@media (max-width: 760px) {
    .master-cards-list:not(.table-list) {
      grid-template-columns: 1fr;
    }
  }/* Dense fleet table */.fleet-table :global(table) {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
  }.fleet-table :global(thead th) {
    padding: 5px 6px;
    font-size: 9.5px;
    white-space: nowrap;
  }.fleet-table :global(tbody td) {
    padding: 5px 6px;
    font-size: 10.5px;
  }.fleet-table :global(thead th.sortable-th) {
    cursor: pointer;
  }.fleet-table :global(.th-content) {
    display: flex;
    align-items: center;
    gap: 3px;
  }.fleet-table :global(.th-sort-icon) {
    font-size: 9px;
    color: var(--color-accent);
    opacity: 0.7;
  }.fleet-row {
    cursor: pointer;
  }.fleet-row.selected {
    background: var(--color-bg-hover);
    box-shadow: inset 2px 0 0 var(--color-accent);
  }.fleet-row:hover {
    background: var(--color-bg-hover);
  }.fleet-app-cell {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }.fleet-app-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 0;
  }.fleet-app-name {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.fleet-app-sub {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.fleet-num {
    font-family: var(--font-mono);
    font-size: 10px;
    text-align: right;
    white-space: nowrap;
  }.fleet-limit {
    font-size: 9px;
    color: var(--color-text-muted);
  }.fleet-limit.warn {
    color: var(--color-warning);
  }.status-text {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }.status-text.online {
    color: var(--color-success);
  }.status-text.stopped {
    color: var(--color-text-muted);
  }.status-text.errored {
    color: var(--color-error);
  }.fleet-actions {
    text-align: right;
    white-space: nowrap;
  }.fleet-table :global(th:last-child),
  .fleet-table :global(td:last-child) {
    width: 76px;
  }/* Single Process Card */ .proc-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    padding: 9px 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
    text-align: left;
    position: relative;
  }.proc-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border);
  }.proc-card.selected {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
    box-shadow: 0 0 0 1px var(--color-accent), 0 4px 12px rgba(0, 218, 243, 0.08);
  }.proc-card.errored {
    border-color: rgba(244, 63, 94, 0.4);
  }.proc-card.errored.selected {
    border-color: var(--color-error);
    box-shadow: 0 0 0 1px var(--color-error), 0 4px 12px rgba(244, 63, 94, 0.1);
  }.proc-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }.proc-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }.proc-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.proc-id-tag {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-surface);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--color-border-subtle);
  }.proc-version-tag {
    font-size: 10px;
    color: var(--color-text-secondary);
    background: var(--color-bg-surface);
    padding: 1px 4px;
    border-radius: 4px;
  }.mode-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }.mode-tag.cluster {
    background: rgba(168, 85, 247, 0.12);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }.mode-tag.fork {
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
  }.proc-path-line {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }/* Mini Meters */ .proc-meters-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    background: var(--color-bg-surface);
    padding: 4px 6px;
    border-radius: 6px;
    border: 1px solid var(--color-border-subtle);
  }.meter-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }.meter-label-row {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
  }.m-label {
    color: var(--color-text-muted);
    font-weight: 500;
  }.m-val {
    color: var(--color-text-secondary);
    font-weight: 600;
  }.mini-track {
    height: 4px;
    background: var(--color-bg-raised);
    border-radius: 2px;
    overflow: hidden;
  }.mini-bar {
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
  }.mem-bar {
    background: #a855f7;
  }/* Card Footer */ .proc-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 3px;
    border-top: 1px solid var(--color-border-subtle);
  }.proc-meta-tags {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 11px;
    color: var(--color-text-muted);
  }.meta-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }.proc-hover-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    opacity: 0.85;
  }.proc-card:hover .proc-hover-actions {
    opacity: 1;
  }.card-action-btn {
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
  }.card-action-btn:hover {
    transform: scale(1.12);
  }/* Specific Action Colors & Glow on Hover */ .card-action-btn.action-restart:hover {
    color: #38bdf8;
    background: rgba(56, 189, 248, 0.14);
    border-color: rgba(56, 189, 248, 0.35);
    box-shadow: 0 0 8px rgba(56, 189, 248, 0.2);
  }.card-action-btn.action-reload:hover {
    color: var(--color-accent, #00daf3);
    background: rgba(0, 218, 243, 0.14);
    border-color: rgba(0, 218, 243, 0.35);
    box-shadow: 0 0 8px rgba(0, 218, 243, 0.2);
  }.card-action-btn.action-stop:hover {
    color: #f59e0b;
    background: rgba(245, 158, 11, 0.14);
    border-color: rgba(245, 158, 11, 0.35);
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.2);
  }.card-action-btn.action-start:hover {
    color: #10b981;
    background: rgba(16, 185, 129, 0.14);
    border-color: rgba(16, 185, 129, 0.35);
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.2);
  }.card-action-btn.action-delete:hover {
    color: #f43f5e;
    background: rgba(244, 63, 94, 0.14);
    border-color: rgba(244, 63, 94, 0.35);
    box-shadow: 0 0 8px rgba(244, 63, 94, 0.2);
  }/* Status Dot */ .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    position: relative;
  }.status-dot.online {
    background: var(--color-success, #10b981);
  }.status-dot.stopped {
    background: var(--color-text-muted, #71717a);
  }.status-dot.errored {
    background: var(--color-error, #f43f5e);
  }.dot-pulse {
    position: absolute;
    inset: -3px;
    border-radius: 50%;
    border: 1px solid var(--color-success, #10b981);
    animation: ping 2s cubic-bezier(0, 0, 0.2, 1) infinite;
  }
@keyframes ping {75%, 100% {
      transform: scale(2);
      opacity: 0;
    }
}@media (max-width: 980px) {
  .fleet-workspace {
    flex-direction: column;
  }
  .fleet-master-pane {
    width: 100%;
    min-height: 0;
  }
  .fleet-saved-pane {
    width: 100%;
    max-height: 38%;
    flex-shrink: 0;
  }
}
.detail-app-head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 10px;
    margin-bottom: 8px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    flex-shrink: 0;
  }.detail-app-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }.detail-app-name {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }.detail-app-meta {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 10.5px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    flex-wrap: wrap;
  }.detail-app-meta strong {
    color: var(--color-text-secondary);
    font-weight: 600;
  }.detail-app-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }/* Full-width empty fleet onboarding */ .fleet-empty {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 20px;
    padding: 24px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }.fleet-empty-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
    max-width: 560px;
  }.fleet-empty-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 68px;
    height: 68px;
    border-radius: 18px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    color: var(--color-accent);
    margin-bottom: 2px;
  }.fleet-empty-hero h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    color: var(--color-text-primary);
  }.fleet-empty-hero p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--color-text-muted);
  }.fleet-empty-actions {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }.fleet-empty-saved {
    width: min(560px, 100%);
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
  }.fleet-empty-saved-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-bottom: 2px;
  }.fleet-empty-saved-head span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }.fleet-empty-saved-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 2px;
    border-bottom: 1px solid var(--color-border-subtle);
  }.fleet-empty-saved-row:last-child {
    border-bottom: none;
  }.fleet-empty-saved-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }.fleet-empty-saved-info strong {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.fleet-empty-saved-info span {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }/* ── SAVED PROCESSES SIDE PANE ── */ .fleet-saved-pane {
    width: 274px;
    flex-shrink: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  }.saved-pane-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 9px 10px 7px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }.saved-pane-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 700;
    color: var(--color-text-primary);
  }.saved-pane-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 7px 10px;
    font-size: 10px;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border-subtle);
  }.saved-pane-warn {
    color: var(--color-warning);
  }.saved-pane-actions {
    display: flex;
    gap: 6px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--color-border-subtle);
  }.saved-pane-empty {
    padding: 14px 12px;
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--color-text-muted);
  }.saved-pane-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }.saved-pane-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 7px;
    border: 1px solid var(--color-border-subtle);
    border-radius: 7px;
    background: var(--color-bg-raised);
  }.saved-pane-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    gap: 1px;
  }.saved-pane-info strong {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.saved-pane-info span {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.logs-proc-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    border-right: 1px solid var(--color-border-subtle);
  }.logs-proc-name {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.logs-proc-id {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--color-border-subtle);
  }.logs-proc-pid {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }/* Drawer Styles */ .drawer-actions-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }.drawer-body-wrap {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 0;
  }.drawer-proc-overview {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }.drawer-proc-main {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }.drawer-proc-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }.drawer-proc-path {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }.drawer-subtabs {
    display: flex;
    gap: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
    padding-bottom: 8px;
  }.drawer-tab-btn {
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
  }.drawer-tab-btn:hover {
    color: var(--color-text-primary);
  }.drawer-tab-btn.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
    border-color: var(--color-accent);
  }.detail-meta-chip {
    font-size: 11px;
    color: var(--color-text-secondary);
    background: var(--color-bg-raised);
    padding: 1px 6px;
    border-radius: 4px;
  }.watch-badge {
    font-size: 10px;
    font-weight: 600;
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.3);
    padding: 1px 6px;
    border-radius: 4px;
  }/* ── TAB 1: LOGS DECK ── */ .logs-drawer {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }.logs-deck-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }.logs-deck-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
    min-height: 32px;
    position: relative;
    z-index: 30;
  }.logs-controls-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    overflow: visible;
  }.logs-controls-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }.menu-item {
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
  }.menu-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }.menu-item.text-danger {
    color: var(--color-error);
  }.menu-item.text-danger:hover {
    background: rgba(244, 63, 94, 0.12);
    color: var(--color-error);
  }.select-field {
    display: flex;
    align-items: center;
    gap: 6px;
  }.field-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }.segmented-control {
    display: flex;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
  }.seg-btn {
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
  }.seg-btn.active {
    background: var(--color-bg-surface);
    color: var(--color-accent);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
  }/* Terminal Log Card */ .log-stream-card {
    position: relative;
    flex: 1;
    min-height: 0;
    --log-bg: #090d13;
    --log-header-bg: #111722;
    --log-border: rgba(255, 255, 255, 0.08);
    --log-divider: rgba(255, 255, 255, 0.06);
    --log-head-bg: #0d131c;
    --log-text: #cbd5e1;
    --log-muted: #94a3b8;
    --log-ts: #64748b;
    --log-faint: #475569;
    --log-hover: rgba(255, 255, 255, 0.035);
    --log-error-text: #fda4af;
    --log-error-bg: rgba(244, 63, 94, 0.06);
    --log-warn-text: #fde047;
    --log-warn-bg: rgba(234, 179, 8, 0.05);
    --log-info-bg: rgba(56, 189, 248, 0.12);
    --log-info-text: #7dd3fc;
    --log-info-border: rgba(56, 189, 248, 0.25);
    --log-err-bg: rgba(244, 63, 94, 0.2);
    --log-err-text: #fb7185;
    --log-err-border: rgba(244, 63, 94, 0.4);
    --log-warn-bg-badge: rgba(234, 179, 8, 0.2);
    --log-warn-text-badge: #facc15;
    --log-warn-border: rgba(234, 179, 8, 0.4);
    background: var(--log-bg);
    border: 1px solid var(--log-border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }:global([data-theme="light"]) .log-stream-card,
  :global(html.light-mode) .log-stream-card {
    --log-bg: #ffffff;
    --log-header-bg: #f8fafc;
    --log-border: #e2e8f0;
    --log-divider: #e2e8f0;
    --log-head-bg: #f1f5f9;
    --log-text: #334155;
    --log-muted: #64748b;
    --log-ts: #64748b;
    --log-faint: #94a3b8;
    --log-hover: #f1f5f9;
    --log-error-text: #b91c1c;
    --log-error-bg: #fef2f2;
    --log-warn-text: #a16207;
    --log-warn-bg: #fffbeb;
    --log-info-bg: #e0f2fe;
    --log-info-text: #0369a1;
    --log-info-border: #bae6fd;
    --log-err-bg: #fee2e2;
    --log-err-text: #b91c1c;
    --log-err-border: #fecaca;
    --log-warn-bg-badge: #fef3c7;
    --log-warn-text-badge: #b45309;
    --log-warn-border: #fde68a;
  }.log-stream-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--log-header-bg);
    border-bottom: 1px solid var(--log-divider);
  }.log-source-info {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--log-muted);
    overflow: hidden;
  }.log-source-path {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.log-count-tag {
    font-size: 10px;
    color: var(--log-ts);
  }.log-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }.log-header-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--log-muted);
    cursor: pointer;
  }.log-header-btn:hover:not(:disabled) {
    border-color: var(--log-border);
    color: var(--color-accent);
  }.log-header-btn:disabled {
    opacity: 0.4;
    cursor: wait;
  }.log-lines-container {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px 12px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 11.5px;
    line-height: 1.55;
  }.log-table {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 640px;
  }.log-table-head {
    display: grid;
    grid-template-columns: 38px 138px 56px 96px minmax(0, 1fr);
    gap: 8px;
    padding: 4px 6px;
    margin: 0 -2px 2px;
    position: sticky;
    top: -8px;
    z-index: 2;
    background: var(--log-head-bg);
    border-bottom: 1px solid var(--log-border);
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--log-ts);
  }.log-row {
    display: grid;
    grid-template-columns: 38px 138px 56px 96px minmax(0, 1fr);
    align-items: start;
    gap: 8px;
    color: var(--log-text);
    padding: 2px 6px;
    border-radius: 3px;
  }.log-row:hover {
    background: var(--log-hover);
  }.row-error {
    color: var(--log-error-text);
    background: var(--log-error-bg);
  }.row-warn {
    color: var(--log-warn-text);
    background: var(--log-warn-bg);
  }.log-line-no {
    color: var(--log-faint);
    text-align: right;
    user-select: none;
    font-size: 10px;
  }.log-ts {
    color: var(--log-ts);
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.log-level-badge {
    font-size: 8.5px;
    font-weight: 700;
    line-height: 15px;
    text-align: center;
    padding: 0 4px;
    border-radius: 3px;
  }.level-err {
    background: var(--log-err-bg);
    color: var(--log-err-text);
    border: 1px solid var(--log-err-border);
  }.level-warn {
    background: var(--log-warn-bg-badge);
    color: var(--log-warn-text-badge);
    border: 1px solid var(--log-warn-border);
  }.level-info {
    background: var(--log-info-bg);
    color: var(--log-info-text);
    border: 1px solid var(--log-info-border);
  }.log-context {
    font-size: 10px;
    color: var(--log-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.log-msg {
    min-width: 0;
    word-break: break-word;
    white-space: pre-wrap;
  }.log-state-msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 180px;
    gap: 10px;
    color: var(--log-ts);
    font-size: 12px;
  }.jump-latest-btn {
    position: absolute;
    right: 16px;
    bottom: 14px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border: 1px solid var(--color-border);
    border-radius: 999px;
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-family: inherit;
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.18);
  }.jump-latest-btn:hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }/* ── TAB 2: VITALS & METRICS ── */ .vitals-tab-wrap {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-right: 4px;
  }.vitals-kpi-row {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }
@media (max-width: 1200px) {.vitals-kpi-row {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
}.vital-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }.vital-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }.vital-card-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }.vital-value {
    font-size: 20px;
    font-weight: 700;
  }.vital-sub {
    font-size: 11px;
    color: var(--color-text-muted);
  }.v-track {
    height: 5px;
    background: var(--color-bg-surface);
    border-radius: 3px;
    overflow: hidden;
    margin-top: 2px;
  }.v-bar {
    height: 100%;
    border-radius: 3px;
    transition: width 0.3s ease;
  }.vitals-sections-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
@media (max-width: 1100px) {.vitals-sections-grid {
      grid-template-columns: 1fr;
    }
}.vitals-info-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }.card-section-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }.card-section-header h3 {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0;
  }.details-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }.detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    gap: 12px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border-subtle);
  }.detail-label {
    color: var(--color-text-muted);
    font-weight: 500;
    flex-shrink: 0;
  }.detail-val {
    color: var(--color-text-primary);
    font-weight: 600;
    text-align: right;
  }.detail-code-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }.detail-code {
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
  }.mini-copy-btn {
    background: transparent;
    border: none;
    padding: 2px 4px;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 3px;
  }.mini-copy-btn:hover {
    color: var(--color-accent);
    background: var(--color-bg-hover);
  }/* ── TAB 3: ENVIRONMENT TAB ── */ .env-tab-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }.env-tab-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }.env-count-pill {
    font-size: 11px;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid var(--color-border-subtle);
  }.env-table-wrap {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    background: var(--color-bg-raised);
  }.env-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }.env-key {
    color: var(--color-accent);
    font-weight: 600;
  }.env-val {
    color: var(--color-text-primary);
    word-break: break-all;
  }.empty-env-tag {
    color: var(--color-text-muted);
    font-style: italic;
    font-size: 11px;
    opacity: 0.7;
  }.env-copy-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 3px;
    border-radius: 4px;
  }.env-copy-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-hover);
  }.empty-env-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 180px;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 12px;
  }/* No Selection Wrap */ .no-selection-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    text-align: center;
    color: var(--color-text-muted);
  }.no-selection-wrap h3 {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0;
  }.no-selection-wrap p {
    font-size: 13px;
    max-width: 360px;
    margin: 0;
  }/* ═════════════════════════════════════════════════════════════════════════ */ /* STATUS BADGES & PILLS */ /* ═════════════════════════════════════════════════════════════════════════ */ .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: capitalize;
  }.status-online {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }.status-stopped {
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border);
  }.status-errored {
    background: rgba(244, 63, 94, 0.12);
    color: #fb7185;
    border: 1px solid rgba(244, 63, 94, 0.3);
  }.pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.5s ease-in-out infinite;
  }
@keyframes pulse {0%, 100% { opacity: 1; transform: scale(1); }50% { opacity: 0.4; transform: scale(0.85); }
}/* ═════════════════════════════════════════════════════════════════════════ */ /* VIEW 2: ECOSYSTEM CONFIG STUDIO */ /* ═════════════════════════════════════════════════════════════════════════ */ .ecosystem-layout {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }.ecosystem-sidebar {
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
  }.ecosystem-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }.ecosystem-sidebar-header h3 {
    font-size: 13px;
    font-weight: 600;
    margin: 0;
  }.files-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    flex: 1;
  }.file-item-btn {
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
  }.file-item-btn:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border);
  }.file-item-btn.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
  }.file-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }.file-name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.file-path-sub {
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.ecosystem-editor-panel {
    flex: 1;
    min-width: 0;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }.editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }.editor-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }.editor-file-title {
    font-size: 12px;
    font-weight: 600;
  }.dirty-badge {
    font-size: 10px;
    font-weight: 600;
    background: rgba(234, 179, 8, 0.15);
    color: #eab308;
    border: 1px solid rgba(234, 179, 8, 0.3);
    padding: 1px 6px;
    border-radius: 4px;
  }.editor-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }.ecosystem-start-options {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-card);
  }.eco-option-field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
  }.eco-option-field .form-input {
    width: 170px;
    margin: 0;
  }.eco-option-hint {
    font-size: 10.5px;
    color: var(--color-text-muted);
    padding-bottom: 5px;
  }.eco-option-hint code {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-accent);
  }.ecosystem-validation-error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 8px 16px 0;
    padding: 7px 9px;
    border: 1px solid rgba(244, 63, 94, 0.35);
    border-radius: 7px;
    background: rgba(244, 63, 94, 0.08);
    color: var(--color-error);
    font-size: 11px;
  }.ecosystem-validation-error pre {
    flex: 1;
    min-width: 0;
    margin: 0;
    max-height: 130px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono);
    font-size: 10.5px;
    line-height: 1.45;
  }.eco-error-close {
    flex-shrink: 0;
    border: none;
    background: transparent;
    color: inherit;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 0 2px;
  }.code-editor-wrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }.editor-empty, .sidebar-empty {
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
  }.editor-empty h3 {
    font-size: 14px;
    color: var(--color-text-primary);
    margin: 0;
  }.editor-empty p, .sidebar-empty p {
    font-size: 12px;
    max-width: 320px;
    margin: 0;
  }/* ═════════════════════════════════════════════════════════════════════════ */ /* VIEW 3: DAEMON RUNTIME & STARTUP */ /* ═════════════════════════════════════════════════════════════════════════ */ .pm2-scope-note {
    flex-shrink: 0;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 12px;
    padding: 9px 12px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 11.5px;
  }.pm2-scope-note strong {
    display: block;
    margin-bottom: 1px;
    color: var(--color-text-primary);
  }.pm2-scope-note p {
    margin: 0;
    line-height: 1.45;
  }.pm2-scope-note code {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-accent);
  }.daemon-grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    overflow-y: auto;
  }
@media (max-width: 1000px) {.daemon-grid {
      grid-template-columns: 1fr;
    }
}.daemon-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }.daemon-card-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }.icon-bubble {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: var(--color-accent-muted);
    display: flex;
    align-items: center;
    justify-content: center;
  }.daemon-card-header h3 {
    font-size: 14px;
    font-weight: 600;
    margin: 0;
  }.daemon-card-header p {
    font-size: 12px;
    color: var(--color-text-muted);
    margin: 0;
  }.card-footer-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
    flex-wrap: wrap;
  }.log-health-summary {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }.log-health-card {
    grid-column: 1 / -1;
  }.log-health-label {
    display: block;
    margin-bottom: 2px;
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }.log-health-value {
    font-size: 16px;
    color: var(--color-text-primary);
  }.log-file-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 150px;
    overflow-y: auto;
    padding: 6px 8px;
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    background: var(--color-bg-surface);
  }.log-file-row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 10.5px;
  }.log-file-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-secondary);
  }.log-file-size {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }.log-file-more {
    padding-top: 3px;
    border-top: 1px dashed var(--color-border-subtle);
    color: var(--color-text-muted);
    font-size: 10px;
  }.logrotate-install {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }.logrotate-install p {
    margin: 0;
    font-size: 10.5px;
    color: var(--color-text-muted);
  }.daemon-note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 11px;
    color: var(--color-text-muted);
  }.startup-hint-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    padding: 12px;
  }.hint-title {
    font-size: 12px;
    color: var(--color-text-secondary);
    margin: 0;
  }.badge {
    display: inline-flex;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
  }.badge-success {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }.badge-neutral {
    background: var(--color-bg-surface);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-subtle);
  }/* Dump Catalog Full-Width Card */ .dump-catalog-card {
    grid-column: 1 / -1;
  }.catalog-header-flex {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    gap: 12px;
  }.catalog-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }.catalog-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 30px 20px;
    text-align: center;
    gap: 8px;
    color: var(--color-text-muted);
  }.catalog-empty h4 {
    font-size: 14px;
    color: var(--color-text-primary);
    margin: 0;
  }.catalog-empty p {
    font-size: 12px;
    margin: 0;
  }.saved-apps-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }.saved-app-card {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: all 0.15s ease;
  }.saved-app-card:hover {
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
  }.saved-app-card.running {
    border-color: rgba(16, 185, 129, 0.3);
  }.saved-app-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }.saved-app-identity {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }.saved-app-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.state-pill {
    font-size: 10.5px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 6px;
    white-space: nowrap;
  }.state-pill.online {
    background: rgba(16, 185, 129, 0.12);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }.state-pill.inactive {
    background: var(--color-bg-surface);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-subtle);
  }.saved-app-paths {
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    padding: 8px 10px;
  }.path-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    overflow: hidden;
  }.path-label {
    font-weight: 600;
    color: var(--color-text-muted);
    width: 38px;
    flex-shrink: 0;
  }.path-code {
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
  }.saved-app-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: 2px;
  }.saved-app-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }/* ═════════════════════════════════════════════════════════════════════════ */ /* NOT INSTALLED HERO CARD */ /* ═════════════════════════════════════════════════════════════════════════ */ .not-installed-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-width: 800px;
    margin: 0 auto;
  }.not-installed-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
  }.hero-icon-ring {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: rgba(234, 179, 8, 0.12);
    display: flex;
    align-items: center;
    justify-content: center;
  }.not-installed-hero h2 {
    font-size: 18px;
    font-weight: 700;
    margin: 0;
  }.hero-subtext {
    font-size: 13px;
    color: var(--color-text-muted);
    max-width: 540px;
    margin: 0;
    line-height: 1.5;
  }.install-commands-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }.command-box {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }.command-box-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }.command-box-title {
    font-size: 12px;
    font-weight: 600;
  }.command-tag {
    font-size: 10px;
    background: var(--color-bg-surface);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--color-text-muted);
  }.command-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
    margin: 0;
  }.code-terminal {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #090d13;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 6px 10px;
  }.code-terminal code {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: #38bdf8;
  }.copy-btn {
    background: transparent;
    border: none;
    color: #94a3b8;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
  }.copy-btn:hover {
    color: #f8fafc;
  }.not-installed-footer {
    display: flex;
    justify-content: center;
    padding-top: 8px;
  }/* ═════════════════════════════════════════════════════════════════════════ */ /* MODALS & STEPPER WIZARD */ /* ═════════════════════════════════════════════════════════════════════════ */ .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
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
    overflow: hidden;
  }.modal-wizard-card {
    width: 704px;
    max-width: calc(100vw - 32px);
    height: 600px;
    max-height: 92vh;
  }.modal-header {
    padding: 14px 20px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border-subtle);
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
  }/* Mode selection cards */ .mode-card-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }.mode-card {
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
  }.mode-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border-hover);
  }.mode-card.active {
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
    box-shadow: 0 0 12px rgba(0, 218, 243, 0.15);
  }.mode-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text-primary);
  }.mode-card-desc {
    font-size: 11px;
    color: var(--color-text-secondary);
    line-height: 1.35;
  }.toggle-card-label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
  }.toggle-card-label strong {
    font-size: 12px;
    color: var(--color-text-primary);
  }/* Pre-flight Terminal box */ .terminal-preview-box {
    background: #0b1320;
    border: 1px solid #1e293b;
    border-radius: 10px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: auto;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }.terminal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #94a3b8;
  }.terminal-copy-btn {
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
  }.terminal-copy-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.25);
  }.terminal-code {
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
  }.terminal-prompt {
    color: #38bdf8;
    font-weight: 700;
    user-select: none;
    flex-shrink: 0;
  }.terminal-cmd {
    color: #f1f5f9;
    word-break: break-all;
    white-space: pre-wrap;
    font-weight: 500;
  }.runtime-badge-pill {
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
  }.bulk-env-box {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px;
    margin-bottom: 6px;
  }.form-textarea {
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
  }.env-rows-scroll {
    max-height: 120px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
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
  }.form-input-sm {
    padding: 5px 8px;
    font-size: 11.5px;
  }.form-help {
    font-size: 11px;
    color: var(--color-text-muted);
  }.launch-warning {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px 8px;
    border: 1px solid rgba(245, 158, 11, 0.35);
    border-radius: 6px;
    background: rgba(245, 158, 11, 0.08);
    color: var(--color-warning);
    font-size: 11px;
    line-height: 1.45;
  }.launch-warning code {
    font-family: var(--font-mono);
    font-size: 10.5px;
  }.checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    cursor: pointer;
  }.env-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--color-bg-raised);
    padding: 10px;
    border-radius: 8px;
    border: 1px solid var(--color-border-subtle);
  }.env-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }.modal-env-row {
    display: grid;
    grid-template-columns: 1fr 1fr 24px;
    gap: 6px;
    align-items: center;
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
    margin-top: 5px;
  }.preset-pill {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 10.5px;
    font-weight: 500;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
    cursor: pointer;
    transition: all 0.12s ease;
  }.preset-pill:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-bg-hover);
  }.preset-pill.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
    border-color: var(--color-accent);
    font-weight: 600;
  }.btn-remove-row {
    background: transparent;
    border: none;
    font-size: 16px;
    cursor: pointer;
  }.modal-footer {
    padding: 12px 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
  }.meter-spark {
    width: 100%;
    height: 14px;
    margin-top: 2px;
  }.meter-spark polyline {
    fill: none;
    stroke-width: 1.2px;
    stroke-linejoin: round;
    stroke-linecap: round;
  }.meter-spark.cpu-spark polyline {
    stroke: var(--color-info, #38bdf8);
  }.meter-spark.mem-spark polyline {
    stroke: #a855f7;
  }.limit-hint {
    font-size: 9px;
    color: var(--color-text-muted);
    font-weight: 500;
  }.vital-spark {
    width: 100%;
    height: 20px;
    margin-top: 4px;
  }.vital-spark polyline {
    fill: none;
    stroke-width: 1.5px;
    stroke-linejoin: round;
    stroke-linecap: round;
  }.vital-spark.cpu-spark polyline {
    stroke: var(--color-accent);
  }.vital-spark.mem-spark polyline {
    stroke: #a855f7;
  }.vital-limit {
    font-size: 10px;
    color: var(--color-text-muted);
    font-weight: 500;
  }.env-mask-note {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }/* Activity drawer */ .activity-drawer {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }.activity-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }.activity-head > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }.activity-head strong {
    font-size: 12px;
    color: var(--color-text-primary);
  }.activity-head span {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }.activity-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px 16px;
    color: var(--color-text-muted);
    font-size: 11px;
  }.activity-list {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
  }.activity-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--color-border-subtle);
  }.activity-row:last-child {
    border-bottom: none;
  }.activity-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    margin-top: 4px;
    flex-shrink: 0;
  }.activity-row.tone-success .activity-dot {
    background: var(--color-success);
  }.activity-row.tone-warning .activity-dot {
    background: var(--color-warning);
  }.activity-row.tone-error .activity-dot {
    background: var(--color-error);
  }.activity-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }.activity-title {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }.activity-detail {
    font-size: 10.5px;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }.activity-time {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--color-text-muted);
  }/* Scale modal */ .scale-modal-card {
    max-width: 440px;
    width: 100%;
  }.scale-copy {
    margin: 0 0 14px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--color-text-secondary);
  }.scale-input-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }.scale-input-row .form-input {
    max-width: 120px;
  }.scale-hint {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }</style>
