<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Select from '../components/ui/Select.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import DatePicker from '../components/ui/DatePicker.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { 
    Server, Activity, Globe, FileCode, FolderOpen, FileText, Shield, 
    Play, Square, RotateCcw, RefreshCw, CheckCircle, XCircle, AlertTriangle, 
    Plus, Trash2, Eye, EyeOff, Upload, FolderPlus, Edit3, Download, Copy, ListFilter, 
    ChevronRight, ChevronDown, Lock, Clock, ArchiveRestore, Save, BarChart2, 
    TerminalSquare, Filter, Search, Sparkles, Bot, Zap, TrendingUp, Radio, HardDrive, Compass, Layers, PieChart, ArrowUpRight, WrapText
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import ContextMenu from '../components/ui/ContextMenu.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';
  import { portal } from '../actions/portal.ts';

  let aiNginxPrompt = $state('');
  let showAiNginxPromptBox = $state(false);

  function triggerAiNginxGen() {
    if (!aiNginxPrompt.trim()) return;
    aiStore.generateNginxRule(aiNginxPrompt);
    showAiNginxPromptBox = false;
    aiNginxPrompt = '';
  }

  // ─── Types ────────────────────────────────────────────────────────────────

  interface NginxInstallInfo { installed: boolean; version: string; }
  interface NginxServiceStatus { active: boolean; status: string; since: string; sub_state: string; }
  interface NginxTestResult { passed: boolean; output: string; timestamp: string; }
  interface NginxSite {
    name: string;
    path: string;
    enabled: boolean;
    source: string;
    domains?: string[];
    ports?: string[];
    proxies?: string[];
    has_ssl?: boolean;
    access_log?: string | null;
    error_log?: string | null;
  }
  interface NginxStats { sites_available: number; sites_enabled: number; sites_disabled: number; }
  interface NginxConfigFile { name: string; path: string; source: string; }
  interface NginxBackup { original_path: string; backup_path: string; timestamp: string; filename: string; }
  interface WwwEntry { name: string; path: string; is_dir: boolean; size: number; modified: string; children: WwwEntry[]; }
  interface SslCert { domain: string; cert_path: string; expiry: string; days_until_expiry: number; status: string; }
  interface NewSiteConfig {
    server_name: string; root_dir: string; port: number; is_proxy: boolean;
    proxy_url: string; index_file: string; enable_404: boolean; enable_50x: boolean;
  }
  interface NginxLogAnalytics {
    total_requests: number;
    unique_ips: number;
    total_bytes_sent: number;
    status_2xx: number;
    status_3xx: number;
    status_4xx: number;
    status_5xx: number;
    success_rate: number;
    error_rate: number;
    top_ips: [string, number][];
    top_paths: [string, number][];
    top_referrers: [string, number][];
    top_user_agents: [string, number][];
    top_methods: [string, number][];
    hourly_traffic: [string, number][];
  }

  // ─── State ────────────────────────────────────────────────────────────────

  let activeTab = $state<'overview'|'sites'|'editor'|'www'|'logs'|'analytics'|'ssl'>(
    uiStore.targetSubTab && ['overview', 'sites', 'editor', 'www', 'logs', 'analytics', 'ssl'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'overview'
  );
  if (uiStore.targetSubTab && ['overview', 'sites', 'editor', 'www', 'logs', 'analytics', 'ssl'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  let installInfo = $state<NginxInstallInfo | null>(null);
  let loading = $state(true);
  let hasCertbot = $state(false);

  // Proxy Wizard
  let showProxyWizard = $state(false);
  let proxyConfig = $state({ domain: '', target_ip: '127.0.0.1', target_port: '8080', enable_websockets: false });
  let proxyLoading = $state(false);
  let proxyConfigTouched = $state({
    domain: false,
    target_ip: false,
    target_port: false
  });

  let proxyConfigErrors = $derived.by(() => {
    const errors: Record<string, string> = {};

    // Domain name
    const d = proxyConfig.domain.trim();
    if (!d) {
      errors.domain = 'Domain name is required.';
    } else if (/\s/.test(d)) {
      errors.domain = 'Domain name cannot contain spaces.';
    } else if (!/^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$/.test(d) && d !== 'localhost') {
      errors.domain = 'Invalid domain format (e.g. app.example.com, myapp.local).';
    }

    // Target IP / Host
    const tip = proxyConfig.target_ip.trim();
    if (!tip) {
      errors.target_ip = 'Target IP or hostname is required.';
    } else if (/\s/.test(tip)) {
      errors.target_ip = 'Target IP/host cannot contain spaces.';
    }

    // Target Port
    const tport = proxyConfig.target_port.trim();
    if (!tport) {
      errors.target_port = 'Target port is required.';
    } else {
      const p = parseInt(tport, 10);
      if (isNaN(p) || p < 1 || p > 65535 || String(p) !== tport) {
        errors.target_port = 'Port must be an integer between 1 and 65535.';
      }
    }

    return errors;
  });

  let isProxyConfigValid = $derived(
    !proxyConfigErrors.domain && !proxyConfigErrors.target_ip && !proxyConfigErrors.target_port
  );

  interface Pm2ProxyItem {
    pm_id: number;
    name: string;
    status: string;
    pid?: number;
    script_path?: string;
    env_vars?: Record<string, string>;
  }
  let pm2ProcessesForProxy = $state<Pm2ProxyItem[]>([]);
  let loadingPm2Processes = $state(false);

  function extractPortFromPm2(p: Pm2ProxyItem): string {
    if (p.env_vars) {
      const portVal = p.env_vars['PORT'] || p.env_vars['port'] || p.env_vars['APP_PORT'] || p.env_vars['SERVER_PORT'] || p.env_vars['HTTP_PORT'];
      if (portVal && !isNaN(Number(portVal))) return String(portVal);
    }
    return '3000';
  }

  async function fetchPm2ProcessesForProxy() {
    loadingPm2Processes = true;
    try {
      const list = await invoke<any[]>('pm2_list_processes');
      pm2ProcessesForProxy = list.map(p => ({
        pm_id: p.pm_id,
        name: p.name,
        status: p.status,
        pid: p.pid,
        script_path: p.script_path,
        env_vars: p.env_vars
      }));
    } catch {
      pm2ProcessesForProxy = [];
    } finally {
      loadingPm2Processes = false;
    }
  }

  function linkPm2AppToProxy(p: Pm2ProxyItem) {
    const port = extractPortFromPm2(p);
    proxyConfig.domain = `${p.name}.local`;
    proxyConfig.target_ip = '127.0.0.1';
    proxyConfig.target_port = port;
    proxyConfig.enable_websockets = true;
    uiStore.addToast(`Auto-configured proxy for PM2 '${p.name}' (Port ${port})`, 'info');
  }

  $effect(() => {
    if (uiStore.targetSubTab && ['overview', 'sites', 'editor', 'www', 'logs', 'analytics', 'ssl'].includes(uiStore.targetSubTab)) {
      activeTab = uiStore.targetSubTab as any;
      uiStore.targetSubTab = null;
    }
  });

  $effect(() => {
    if (uiStore.navigationPayload && uiStore.activeTab === 'nginx-manager') {
      const payload = uiStore.navigationPayload;
      if (payload.initialDomain) {
        showProxyWizard = true;
        proxyConfig.domain = payload.initialDomain;
        if (payload.targetIp) proxyConfig.target_ip = payload.targetIp;
        if (payload.targetPort) proxyConfig.target_port = payload.targetPort;
      }
      uiStore.navigationPayload = null;
    }
  });

  $effect(() => {
    if (showProxyWizard) {
      fetchPm2ProcessesForProxy();
    }
  });

  // Analytics
  let analyticsData = $state<NginxLogAnalytics | null>(null);
  let analyticsLoading = $state(false);
  let analyticsLogFile = $state('/var/log/nginx/access.log');

  // Overview
  let serviceStatus = $state<NginxServiceStatus | null>(null);
  let testResult = $state<NginxTestResult | null>(null);
  let stats = $state<NginxStats | null>(null);
  let serviceLoading = $state(false);
  let testLoading = $state(false);

  // Sites
  let sites = $state<NginxSite[]>([]);
  let sitesLoading = $state(false);
  let showNewSiteForm = $state(false);
  let newSite = $state<NewSiteConfig>({
    server_name: '', root_dir: '/var/www/html', port: 80,
    is_proxy: false, proxy_url: '', index_file: 'index.html',
    enable_404: true, enable_50x: true,
  });
  let newSiteLoading = $state(false);
  let toggleLoadingFor = $state<string>('');

  // Site Context for Logs / Analytics
  let activeSiteContext = $state<NginxSite | null>(null);

  const detectedOverviewPorts = $derived.by(() => {
    const portSet = new Set<string>();
    for (const s of sites) {
      if (s.ports && s.ports.length > 0) {
        for (const p of s.ports) portSet.add(p);
      } else {
        portSet.add('80');
      }
    }
    return Array.from(portSet);
  });

  const totalProxiesCount = $derived.by(() => {
    let count = 0;
    for (const s of sites) {
      if (s.proxies) count += s.proxies.length;
    }
    return count;
  });

  // Site Drawer States
  let showInspectDrawer = $state(false);
  let inspectingSite = $state<NginxSite | null>(null);
  let inspectingContent = $state('');
  let inspectingLoading = $state(false);
  let inspectWrapLines = $state(false);

  let showCloneDrawer = $state(false);
  let cloningSite = $state<NginxSite | null>(null);
  let cloneNewName = $state('');
  let cloneNewDomain = $state('');
  let cloneLoading = $state(false);

  let showSslIssueDrawer = $state(false);
  let sslIssueSite = $state<NginxSite | null>(null);
  let sslIssueDomain = $state('');
  let sslIssueEmail = $state('');
  let sslIssueLoading = $state(false);

  // Config Editor
  let configs = $state<NginxConfigFile[]>([]);
  let selectedConfig = $state<NginxConfigFile | null>(null);
  let editorContent = $state('');
  let savedContent = $state('');
  let editorLoading = $state(false);
  let configSaving = $state(false);
  let showDiff = $state(false);
  let showConfigDiffModal = $state(false);
  let wordWrap = $state(true);
  let backups = $state<NginxBackup[]>([]);
  let showBackups = $state(false);
  let backupsLoading = $state(false);

  // WWW browser
  let wwwEntries = $state<WwwEntry[]>([]);
  let wwwLoading = $state(false);
  let expandedPaths = $state<Set<string>>(new Set());
  let selectedWwwEntry = $state<WwwEntry | null>(null);
  let wwwFileContent = $state('');
  let wwwFileLoading = $state(false);
  let wwwWrapLines = $state(false);
  let renamingEntry = $state<WwwEntry | null>(null);
  let renameValue = $state('');
  let newDirParent = $state('');
  let newDirName = $state('');
  let showNewDirForm = $state(false);

  // Logs
  let logFiles = $state<string[]>([]);
  let selectedLog = $state('');
  let logContent = $state('');
  let logLoading = $state(false);
  let logFilter = $state('');
  let logAutoRefresh = $state(false);
  let logInterval: ReturnType<typeof setInterval> | null = null;
  let logViewMode = $state<'structured' | 'raw'>('structured');
  let logStatusFilter = $state<'all' | '2xx' | '3xx' | '4xx' | '5xx'>('all');
  let timeRange = $state('all');
  let customStartDate = $state('');
  let customStartTime = $state('00:00');
  let customEndDate = $state('');
  let customEndTime = $state('23:59');
  let showCustomPopover = $state(false);
  let popoverContainer = $state<HTMLDivElement | null>(null);

  function formatDateLabel(dateStr: string) {
    if (!dateStr) return '';
    const parts = dateStr.split('-');
    if (parts.length === 3) {
      return `${parts[1]}/${parts[2]}/${parts[0]}`; // MM/DD/YYYY
    }
    return dateStr;
  }

  let customRangeLabel = $derived(
    customStartDate
      ? `${formatDateLabel(customStartDate)} - ${formatDateLabel(customEndDate || customStartDate)}`
      : 'Custom Range...'
  );

  function handleRangeChange() {
    if (timeRange === 'custom') {
      setTimeout(() => { showCustomPopover = true; }, 0);
    } else {
      showCustomPopover = false;
    }
  }

  function getCustomStart(): Date | null {
    if (timeRange !== 'custom' || !customStartDate) return null;
    return new Date(`${customStartDate}T${customStartTime || '00:00'}:00`);
  }

  function getCustomEnd(): Date | null {
    if (timeRange !== 'custom' || !customEndDate) return null;
    return new Date(`${customEndDate}T${customEndTime || '23:59'}:59`);
  }

  let expandedLogIndex = $state<number | null>(null);

  interface ParsedLogEntry {
    raw: string;
    type: 'access' | 'error' | 'generic';
    ip?: string;
    timestamp?: string;
    formattedTime?: string;
    date?: Date | null;
    method?: string;
    path?: string;
    httpVersion?: string;
    status?: number;
    statusCategory?: '2xx' | '3xx' | '4xx' | '5xx' | 'other';
    statusText?: string;
    bytes?: number;
    formattedSize?: string;
    referer?: string;
    userAgent?: string;
    clientBrowser?: string;
    logLevel?: 'error' | 'warn' | 'crit' | 'notice' | 'info' | 'alert' | 'emerg';
    errorMessage?: string;
    pid?: string;
  }

  function parseLogDate(rawTime: string): Date | null {
    if (!rawTime) return null;
    try {
      // Access log: "17/Aug/2026:13:40:00 +0300"
      const accessMatch = rawTime.match(/^(\d{1,2})\/([a-zA-Z]{3})\/(\d{4}):(\d{2}):(\d{2}):(\d{2})/);
      if (accessMatch) {
        const monthMap: Record<string, string> = {
          Jan: '01', Feb: '02', Mar: '03', Apr: '04', May: '05', Jun: '06',
          Jul: '07', Aug: '08', Sep: '09', Oct: '10', Nov: '11', Dec: '12'
        };
        const day = accessMatch[1].padStart(2, '0');
        const month = monthMap[accessMatch[2]] || '01';
        const year = accessMatch[3];
        const hour = accessMatch[4];
        const min = accessMatch[5];
        const sec = accessMatch[6];
        return new Date(`${year}-${month}-${day}T${hour}:${min}:${sec}`);
      }
      // Error log: "2026/08/17 13:40:00"
      const errMatch = rawTime.match(/^(\d{4})\/(\d{2})\/(\d{2})\s+(\d{2}):(\d{2}):(\d{2})/);
      if (errMatch) {
        return new Date(`${errMatch[1]}-${errMatch[2]}-${errMatch[3]}T${errMatch[4]}:${errMatch[5]}:${errMatch[6]}`);
      }
      const d = new Date(rawTime);
      return isNaN(d.getTime()) ? null : d;
    } catch {
      return null;
    }
  }

  function parseLogLine(line: string): ParsedLogEntry {
    const trimmed = line.trim();
    if (!trimmed) {
      return { raw: line, type: 'generic' };
    }

    // Access Log Regex: Combined & Standard Nginx log format
    const accessMatch = trimmed.match(/^(\S+)\s+\S+\s+\S+\s+\[([^\]]+)\]\s+"([A-Z]+)\s+([^"\s]+)(?:\s+([^"]*))?"\s+(\d{3})\s+(\d+|-)(?:\s+"([^"]*)"\s*"([^"]*)")?/);
    if (accessMatch) {
      const ip = accessMatch[1];
      const rawTime = accessMatch[2];
      const method = accessMatch[3];
      const path = accessMatch[4];
      const httpVer = accessMatch[5] || 'HTTP/1.1';
      const status = parseInt(accessMatch[6], 10);
      const rawBytes = accessMatch[7] === '-' ? 0 : parseInt(accessMatch[7], 10);
      const referer = accessMatch[8] && accessMatch[8] !== '-' ? accessMatch[8] : undefined;
      const userAgent = accessMatch[9] && accessMatch[9] !== '-' ? accessMatch[9] : undefined;

      let statusCategory: '2xx' | '3xx' | '4xx' | '5xx' | 'other' = 'other';
      if (status >= 200 && status < 300) statusCategory = '2xx';
      else if (status >= 300 && status < 400) statusCategory = '3xx';
      else if (status >= 400 && status < 500) statusCategory = '4xx';
      else if (status >= 500 && status < 600) statusCategory = '5xx';

      let statusText = '';
      if (status === 200) statusText = 'OK';
      else if (status === 201) statusText = 'Created';
      else if (status === 204) statusText = 'No Content';
      else if (status === 301) statusText = 'Moved';
      else if (status === 302) statusText = 'Found';
      else if (status === 304) statusText = 'Cached';
      else if (status === 400) statusText = 'Bad Request';
      else if (status === 401) statusText = 'Unauthorized';
      else if (status === 403) statusText = 'Forbidden';
      else if (status === 404) statusText = 'Not Found';
      else if (status === 405) statusText = 'Method Denied';
      else if (status === 429) statusText = 'Rate Limited';
      else if (status === 500) statusText = 'Server Error';
      else if (status === 502) statusText = 'Bad Gateway';
      else if (status === 503) statusText = 'Unavailable';
      else if (status === 504) statusText = 'Timeout';

      let formattedTime = rawTime;
      const timeParts = rawTime.split(':');
      if (timeParts.length >= 4) {
        formattedTime = `${timeParts[1]}:${timeParts[2]}:${timeParts[3].split(' ')[0]}`;
      }

      let clientBrowser = '';
      if (userAgent) {
        if (userAgent.includes('Firefox')) clientBrowser = 'Firefox';
        else if (userAgent.includes('Chrome') || userAgent.includes('CriOS')) clientBrowser = 'Chrome';
        else if (userAgent.includes('Safari')) clientBrowser = 'Safari';
        else if (userAgent.includes('Edge')) clientBrowser = 'Edge';
        else if (userAgent.includes('curl')) clientBrowser = 'curl';
        else if (userAgent.includes('Postman')) clientBrowser = 'Postman';
        else if (userAgent.includes('bot') || userAgent.includes('Spider') || userAgent.includes('Crawler')) clientBrowser = 'Bot';
        else clientBrowser = userAgent.split(' ')[0].substring(0, 15);
      }

      return {
        raw: line,
        type: 'access',
        ip,
        timestamp: rawTime,
        formattedTime,
        date: parseLogDate(rawTime),
        method,
        path,
        httpVersion: httpVer,
        status,
        statusCategory,
        statusText,
        bytes: rawBytes,
        formattedSize: formatSize(rawBytes),
        referer,
        userAgent,
        clientBrowser
      };
    }

    // Error Log Regex
    const errorMatch = trimmed.match(/^(\d{4}\/\d{2}\/\d{2}\s+\d{2}:\d{2}:\d{2})\s+\[([a-z]+)\]\s+(?:(\d+)#\d+:\s+)?(.*)$/i);
    if (errorMatch) {
      const timestamp = errorMatch[1];
      const level = errorMatch[2].toLowerCase() as any;
      const pid = errorMatch[3];
      const message = errorMatch[4];

      let clientIp = '';
      const clientMatch = message.match(/client:\s+([^,\s]+)/);
      if (clientMatch) clientIp = clientMatch[1];

      let req = '';
      const reqMatch = message.match(/request:\s+"([^"]+)"/);
      if (reqMatch) req = reqMatch[1];

      return {
        raw: line,
        type: 'error',
        timestamp,
        formattedTime: timestamp.split(' ')[1] || timestamp,
        date: parseLogDate(timestamp),
        logLevel: level,
        errorMessage: message,
        ip: clientIp || undefined,
        path: req || undefined,
        pid
      };
    }

    return {
      raw: line,
      type: 'generic'
    };
  }

  const parsedLogEntries = $derived.by(() => {
    if (!logContent || !logContent.trim()) return [];
    return logContent
      .split('\n')
      .filter(l => l.trim().length > 0)
      .map(parseLogLine)
      .reverse();
  });

  const filteredLogEntries = $derived.by(() => {
    let list = parsedLogEntries;
    if (logStatusFilter !== 'all') {
      list = list.filter(e => e.statusCategory === logStatusFilter);
    }
    if (timeRange !== 'all') {
      const now = new Date();
      const startDate = timeRange === 'custom' ? getCustomStart() : null;
      const endDate = timeRange === 'custom' ? getCustomEnd() : null;
      const days = timeRange !== 'custom' ? parseInt(timeRange, 10) : null;

      list = list.filter(e => {
        if (!e.date) return true;
        if (timeRange === 'custom') {
          if (startDate && e.date < startDate) return false;
          if (endDate && e.date > endDate) return false;
          return true;
        } else if (days !== null && !isNaN(days)) {
          const diffMs = now.getTime() - e.date.getTime();
          return diffMs >= 0 && diffMs <= days * 24 * 3600 * 1000;
        }
        return true;
      });
    }
    if (logFilter.trim()) {
      const q = logFilter.toLowerCase();
      list = list.filter(e => 
        e.raw.toLowerCase().includes(q) ||
        (e.path && e.path.toLowerCase().includes(q)) ||
        (e.ip && e.ip.toLowerCase().includes(q)) ||
        (e.method && e.method.toLowerCase().includes(q)) ||
        (e.status && String(e.status).includes(q)) ||
        (e.errorMessage && e.errorMessage.toLowerCase().includes(q))
      );
    }
    return list;
  });

  const logStats = $derived.by(() => {
    let count2xx = 0, count3xx = 0, count4xx = 0, count5xx = 0;
    for (const e of parsedLogEntries) {
      if (e.statusCategory === '2xx') count2xx++;
      else if (e.statusCategory === '3xx') count3xx++;
      else if (e.statusCategory === '4xx') count4xx++;
      else if (e.statusCategory === '5xx') count5xx++;
    }
    return {
      total: parsedLogEntries.length,
      filteredTotal: filteredLogEntries.length,
      count2xx,
      count3xx,
      count4xx,
      count5xx
    };
  });

  function copyLogLine(text: string) {
    navigator.clipboard.writeText(text);
    uiStore.addToast('Copied to clipboard', 'info');
  }

  // SSL
  let sslCerts = $state<SslCert[]>([]);
  let sslLoading = $state(false);
  let renewingCert = $state('');

  // Modals
  let showTestModal = $state(false);
  let modalTestResult = $state<NginxTestResult | null>(null);
  let showOutputModal = $state(false);
  let outputModalTitle = $state('');
  let outputModalContent = $state('');

  // ─── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    init();
    return () => {
      if (logInterval) clearInterval(logInterval);
    };
  });

  async function init() {
    loading = true;
    try {
      installInfo = await invoke<NginxInstallInfo>('nginx_check_installed');
      if (installInfo.installed) {
        hasCertbot = await invoke<boolean>('nginx_check_certbot');
        await Promise.all([
          loadServiceStatus(),
          loadTestResult(),
          loadStats(),
          loadSites(),
          loadLogFiles(),
        ]);
        statusStore.setLastCommand('nginx -v; systemctl is-active nginx', 0, true);
      } else {
        statusStore.setLastCommand('nginx -v', 0, true);
      }
    } catch (e) {
      uiStore.addToast(`Init error: ${e}`, 'error');
      statusStore.setLastCommand('nginx -v', 1, false);
    } finally {
      loading = false;
    }
  }

  // ─── Overview ──────────────────────────────────────────────────────────────

  async function loadServiceStatus() {
    try {
      serviceStatus = await invoke<NginxServiceStatus>('nginx_service_status');
      statusStore.setLastCommand('systemctl is-active nginx', 0, true);
    } catch {
      statusStore.setLastCommand('systemctl is-active nginx', 1, false);
    }
  }

  async function loadTestResult() {
    try {
      testResult = await invoke<NginxTestResult>('nginx_test_config');
      statusStore.setLastCommand('nginx -t', testResult.passed ? 0 : 1, testResult.passed);
    } catch {
      statusStore.setLastCommand('nginx -t', 1, false);
    }
  }

  async function loadStats() {
    try {
      stats = await invoke<NginxStats>('nginx_get_stats');
      statusStore.setLastCommand('curl -s http://localhost/nginx_status', 0, true);
    } catch {
      statusStore.setLastCommand('curl -s http://localhost/nginx_status', 1, false);
    }
  }

  function requestServiceAction(action: string) {
    if (action === 'stop') {
      uiStore.confirm(
        'Stop Web Server',
        'Stopping Nginx will immediately disconnect all active HTTP/HTTPS web traffic and take down all hosted virtual hosts and reverse proxies. Are you sure you want to stop Nginx?',
        () => doServiceAction('stop'),
        true
      );
    } else if (action === 'restart') {
      uiStore.confirm(
        'Restart Web Server',
        'Restarting Nginx will briefly interrupt active client connections and reload all configuration files from scratch. Proceed with restart?',
        () => doServiceAction('restart'),
        false
      );
    } else {
      doServiceAction(action);
    }
  }

  async function doServiceAction(action: string) {
    serviceLoading = true;
    statusStore.setBusy(`Running: systemctl ${action} nginx…`);
    const toastId = uiStore.addToast(`nginx -t passed → running ${action}…`, 'info', 0);
    try {
      await invoke<string>('nginx_service_action', { action });
      uiStore.removeToast(toastId);
      uiStore.addToast(`nginx ${action} succeeded ✓`, 'success');
      statusStore.setLastCommand(`systemctl ${action} nginx`, 0, true);
      await loadServiceStatus();
    } catch (e) {
      uiStore.removeToast(toastId);
      uiStore.addToast(`nginx ${action} failed: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${action} nginx`, 1, false);
    } finally {
      serviceLoading = false;
      statusStore.clearBusy();
    }
  }

  async function runTest() {
    testLoading = true;
    statusStore.setBusy('Running nginx -t…');
    try {
      testResult = await invoke<NginxTestResult>('nginx_test_config');
      statusStore.setLastCommand('nginx -t', testResult.passed ? 0 : 1, testResult.passed);
      if (testResult.passed) {
        uiStore.addToast('Nginx syntax test passed ✓', 'success');
      } else {
        uiStore.addToast('Nginx configuration error detected ✗', 'error');
      }
    } catch (e) {
      uiStore.addToast(`Test failed: ${e}`, 'error');
    } finally {
      testLoading = false;
      statusStore.clearBusy();
    }
  }

  async function testAndReload() {
    testLoading = true;
    statusStore.setBusy('Testing syntax and reloading Nginx…');
    try {
      const res = await invoke<NginxTestResult>('nginx_test_config');
      testResult = res;
      if (!res.passed) {
        uiStore.addToast('Cannot reload: nginx -t syntax test failed ✗', 'error');
        statusStore.setLastCommand('nginx -t', 1, false);
        return;
      }
      statusStore.setLastCommand('nginx -t', 0, true);
      await doServiceAction('reload');
    } catch (e) {
      uiStore.addToast(`Safe reload failed: ${e}`, 'error');
    } finally {
      testLoading = false;
      statusStore.clearBusy();
    }
  }

  // ─── Sites ─────────────────────────────────────────────────────────────────

  async function loadSites() {
    sitesLoading = true;
    try {
      sites = await invoke<NginxSite[]>('nginx_list_sites');
      statusStore.setLastCommand('ls /etc/nginx/conf.d/ /etc/nginx/sites-available/', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load sites: ${e}`, 'error');
      statusStore.setLastCommand('ls /etc/nginx/conf.d/', 1, false);
    } finally {
      sitesLoading = false;
    }
  }

  async function toggleSite(site: NginxSite) {
    const action = site.enabled ? 'disable' : 'enable';
    uiStore.confirm(
      `${action === 'enable' ? 'Enable' : 'Disable'} Virtual Host`,
      `Are you sure you want to ${action} virtual host "${site.name}"?\n\n${action === 'disable' ? 'Nginx will stop routing traffic to this domain.' : 'Nginx will validate configuration with nginx -t and activate routing.'}`,
      async () => {
        toggleLoadingFor = site.name;
        const toastId = uiStore.addToast(`nginx -t checking…`, 'info', 0);
        try {
          const result = await invoke<NginxTestResult>('nginx_toggle_site', {
            name: site.name,
            enable: !site.enabled,
          });
          uiStore.removeToast(toastId);
          statusStore.setLastCommand(!site.enabled ? `ln -s /etc/nginx/sites-available/${site.name} /etc/nginx/sites-enabled/${site.name} && nginx -t` : `rm /etc/nginx/sites-enabled/${site.name} && nginx -t`, result.passed ? 0 : 1, result.passed);

          if (result.passed) {
            uiStore.addToast(`Site "${site.name}" ${action}d and nginx reloaded ✓`, 'success');
          } else {
            uiStore.addToast(`nginx -t failed — change reverted`, 'error');
            showOutputModal = true;
            outputModalTitle = 'nginx -t Failed — Change Reverted';
            outputModalContent = result.output;
          }
          await loadSites();
          await loadStats();
        } catch (e) {
          uiStore.removeToast(toastId);
          uiStore.addToast(`Toggle failed: ${e}`, 'error');
        } finally {
          toggleLoadingFor = '';
        }
      },
      action === 'disable',
    );
  }

  async function createSite() {
    if (!newSite.server_name.trim()) {
      uiStore.addToast('Server name is required', 'warning');
      return;
    }
    newSiteLoading = true;
    try {
      const path = await invoke<string>('nginx_create_site', { config: newSite });
      statusStore.setLastCommand(`echo "..." > ${path}`, 0, true);
      uiStore.addToast(`Site created at ${path} ✓`, 'success');
      showNewSiteForm = false;
      newSite = { server_name: '', root_dir: '/var/www/html', port: 80, is_proxy: false, proxy_url: '', index_file: 'index.html', enable_404: true, enable_50x: true };
      await loadSites();
      await loadStats();
    } catch (e) {
      uiStore.addToast(`Create site failed: ${e}`, 'error');
      statusStore.setLastCommand(`echo "..." > /etc/nginx/sites-available/${newSite.server_name}.conf`, 1, false);
      showOutputModal = true;
      outputModalTitle = 'Site Creation Failed';
      outputModalContent = String(e);
    } finally {
      newSiteLoading = false;
    }
  }

  function confirmDeleteSite(site: NginxSite) {
    if (site.name === 'nginx.conf' || site.path === '/etc/nginx/nginx.conf') {
      uiStore.addToast('System Protection: Core nginx.conf file cannot be deleted!', 'error');
      return;
    }
    uiStore.confirm(
      'Delete Virtual Host',
      `Are you sure you want to permanently delete virtual host "${site.name}"?\nPath: ${site.path}\n\nThis will remove the configuration file and its active symlink. This action cannot be undone.`,
      async () => {
        try {
          await invoke('nginx_delete_site', { name: site.name, path: site.path });
          statusStore.setLastCommand(`rm -f ${site.path} /etc/nginx/sites-enabled/${site.name}`, 0, true);
          uiStore.addToast(`Site "${site.name}" deleted`, 'success');
          await loadSites();
          await loadStats();
        } catch (e) {
          uiStore.addToast(`Delete failed: ${e}`, 'error');
          statusStore.setLastCommand(`rm -f ${site.path} /etc/nginx/sites-enabled/${site.name}`, 1, false);
        }
      },
      true,
    );
  }

  let siteSearchQuery = $state('');
  let siteSourceFilter = $state<'all' | 'conf.d' | 'sites-available' | 'sites-enabled'>('all');

  let filteredSites = $derived.by(() => {
    let list = sites;
    if (siteSourceFilter !== 'all') {
      list = list.filter(s => s.source === siteSourceFilter);
    }
    const q = siteSearchQuery.trim().toLowerCase();
    if (q) {
      list = list.filter(s => 
        s.name.toLowerCase().includes(q) || 
        s.path.toLowerCase().includes(q) || 
        s.source.toLowerCase().includes(q) ||
        (s.domains && s.domains.some(d => d.toLowerCase().includes(q))) ||
        (s.proxies && s.proxies.some(p => p.toLowerCase().includes(q))) ||
        (s.ports && s.ports.some(p => p.toLowerCase().includes(q)))
      );
    }
    return list;
  });

  async function openSiteInEditor(site: NginxSite) {
    activeTab = 'editor';
    if (configs.length === 0) {
      await loadConfigs();
    }
    const matchingCfg = configs.find(c => c.path === site.path) || {
      name: site.name,
      path: site.path,
      source: site.source,
    };
    await selectConfig(matchingCfg);
    uiStore.addToast(`Opened ${site.name} in Config Editor`, 'info');
  }

  async function openSiteInspector(site: NginxSite) {
    inspectingSite = site;
    showInspectDrawer = true;
    inspectingLoading = true;
    try {
      inspectingContent = await invoke<string>('nginx_read_config', { path: site.path });
      statusStore.setLastCommand(`cat ${site.path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to read site config: ${e}`, 'error');
      inspectingContent = `# Failed to load: ${e}`;
    } finally {
      inspectingLoading = false;
    }
  }

  function openCloneModal(site: NginxSite) {
    cloningSite = site;
    cloneNewName = site.name.replace(/\.conf$/, '') + '-copy';
    cloneNewDomain = site.domains && site.domains.length > 0 ? `${site.domains[0]}` : '';
    showCloneDrawer = true;
  }

  async function executeCloneSite() {
    if (!cloningSite || !cloneNewName.trim()) {
      uiStore.addToast('Please specify a valid site name', 'warning');
      return;
    }
    cloneLoading = true;
    try {
      const newPath = await invoke<string>('nginx_clone_site', {
        sourcePath: cloningSite.path,
        newName: cloneNewName.trim(),
        newDomain: cloneNewDomain.trim() || null,
      });
      uiStore.addToast(`Site cloned to ${newPath} ✓`, 'success');
      showCloneDrawer = false;
      await loadSites();
      await loadStats();
      if (configs.length > 0) await loadConfigs();
    } catch (e) {
      uiStore.addToast(`Clone failed: ${e}`, 'error');
    } finally {
      cloneLoading = false;
    }
  }

  function jumpToSiteLogs(site: NginxSite, mode: 'logs' | 'analytics' = 'analytics') {
    activeSiteContext = site;
    if (mode === 'analytics') {
      if (site.access_log) {
        analyticsLogFile = site.access_log;
        uiStore.addToast(`Switched to dedicated log: ${site.access_log} (${site.name})`, 'success');
      } else if (site.error_log) {
        analyticsLogFile = site.error_log;
        uiStore.addToast(`Switched to dedicated error log: ${site.error_log} (${site.name})`, 'success');
      } else {
        analyticsLogFile = '/var/log/nginx/access.log';
        uiStore.addToast(`Viewing global log for ${site.name} (no custom access_log in config)`, 'info');
      }
      activeTab = 'analytics';
      loadAnalytics(true);
    } else {
      if (site.access_log) {
        selectedLog = site.access_log;
        logFilter = '';
        uiStore.addToast(`Switched to dedicated log: ${site.access_log}`, 'success');
      } else if (site.error_log) {
        selectedLog = site.error_log;
        logFilter = '';
        uiStore.addToast(`Switched to dedicated error log: ${site.error_log}`, 'success');
      } else {
        selectedLog = '/var/log/nginx/access.log';
        logFilter = (site.domains && site.domains.length > 0) ? site.domains[0] : site.name.replace(/\.conf$/, '');
        uiStore.addToast(`Viewing global log filtered for ${site.name}`, 'info');
      }
      activeTab = 'logs';
      loadLog();
    }
  }

  function openQuickSsl(site: NginxSite) {
    sslIssueSite = site;
    sslIssueDomain = (site.domains && site.domains.length > 0) ? site.domains[0] : site.name.replace(/\.conf$/, '');
    sslIssueEmail = '';
    showSslIssueDrawer = true;
  }

  async function executeIssueSsl() {
    if (!sslIssueDomain.trim()) {
      uiStore.addToast('Domain is required for SSL certificate', 'warning');
      return;
    }
    sslIssueLoading = true;
    const toastId = uiStore.addToast(`Requesting Let's Encrypt SSL for ${sslIssueDomain}…`, 'info', 0);
    try {
      await invoke<string>('nginx_request_cert', {
        domain: sslIssueDomain.trim(),
        email: sslIssueEmail.trim() || null,
      });
      uiStore.removeToast(toastId);
      uiStore.addToast(`SSL Certificate issued successfully for ${sslIssueDomain} ✓`, 'success');
      showSslIssueDrawer = false;
      await loadSites();
      if (hasCertbot) await loadSslCerts();
    } catch (e) {
      uiStore.removeToast(toastId);
      uiStore.addToast(`SSL request failed: ${e}`, 'error');
      showOutputModal = true;
      outputModalTitle = `Certbot SSL Failed — ${sslIssueDomain}`;
      outputModalContent = String(e);
    } finally {
      sslIssueLoading = false;
    }
  }

  // ─── Config Editor ─────────────────────────────────────────────────────────

  async function loadConfigs() {
    editorLoading = true;
    try {
      configs = await invoke<NginxConfigFile[]>('nginx_list_configs');
      statusStore.setLastCommand('find /etc/nginx -name "*.conf"', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load configs: ${e}`, 'error');
      statusStore.setLastCommand('find /etc/nginx -name "*.conf"', 1, false);
    } finally {
      editorLoading = false;
    }
  }

  async function selectConfig(cfg: NginxConfigFile) {
    editorLoading = true;
    selectedConfig = cfg;
    showDiff = false;
    try {
      const content = await invoke<string>('nginx_read_config', { path: cfg.path });
      editorContent = content;
      savedContent = content;
      statusStore.setLastCommand(`cat ${cfg.path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to read config: ${e}`, 'error');
      statusStore.setLastCommand(`cat ${cfg.path}`, 1, false);
    } finally {
      editorLoading = false;
    }
  }

  async function saveConfig() {
    if (!selectedConfig) return;
    showConfigDiffModal = true;
  }

  async function executeSaveConfig() {
    if (!selectedConfig) return;
    configSaving = true;
    const toastId = uiStore.addToast('Validating syntax with nginx -t…', 'info', 0);
    try {
      const result = await invoke<NginxTestResult>('nginx_write_config', {
        path: selectedConfig.path,
        content: editorContent,
      });
      uiStore.removeToast(toastId);
      statusStore.setLastCommand(`echo "..." > ${selectedConfig.path} && nginx -t`, result.passed ? 0 : 1, result.passed);

      if (result.passed) {
        uiStore.addToast('Config saved and verified with nginx -t ✓', 'success');
        savedContent = editorContent;
        showDiff = false;
      } else {
        uiStore.addToast('⚠️ nginx -t failed — changes reverted from backup', 'error');
        showOutputModal = true;
        outputModalTitle = 'Syntax Error — File Protected & Reverted';
        outputModalContent = `Nginx rejected this configuration syntax:\n\n${result.output}\n\n[Protection Safeguard]: The file was restored from backup so your web server did not crash.`;
        // Reload the reverted content
        const content = await invoke<string>('nginx_read_config', { path: selectedConfig.path });
        editorContent = content;
        savedContent = content;
      }
    } catch (e) {
      uiStore.removeToast(toastId);
      uiStore.addToast(`Save failed: ${e}`, 'error');
    } finally {
      configSaving = false;
    }
  }

  async function loadBackups() {
    backupsLoading = true;
    try {
      backups = await invoke<NginxBackup[]>('nginx_list_backups');
      statusStore.setLastCommand('ls -l /etc/nginx/backups', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load backups: ${e}`, 'error');
      statusStore.setLastCommand('ls -l /etc/nginx/backups', 1, false);
    } finally {
      backupsLoading = false;
    }
  }

  async function restoreBackup(backup: NginxBackup) {
    uiStore.confirm(
      'Restore Backup',
      `Restore "${backup.filename}" to "${backup.original_path}"? Current file will be backed up first.`,
      async () => {
        try {
          const result = await invoke<NginxTestResult>('nginx_restore_backup', {
            backupPath: backup.backup_path,
            originalPath: backup.original_path,
          });
          statusStore.setLastCommand(`cp ${backup.backup_path} ${backup.original_path} && nginx -t`, result.passed ? 0 : 1, result.passed);
          if (result.passed) {
            uiStore.addToast('Backup restored and nginx reloaded ✓', 'success');
          } else {
            uiStore.addToast('Backup restored but nginx -t failed', 'warning');
            showOutputModal = true;
            outputModalTitle = 'nginx -t Result After Restore';
            outputModalContent = result.output;
          }
          if (selectedConfig && selectedConfig.path === backup.original_path) {
            const content = await invoke<string>('nginx_read_config', { path: selectedConfig.path });
            editorContent = content;
            savedContent = content;
          }
        } catch (e) {
          uiStore.addToast(`Restore failed: ${e}`, 'error');
          statusStore.setLastCommand(`cp ${backup.backup_path} ${backup.original_path} && nginx -t`, 1, false);
        }
      },
    );
  }

  // Diff helper
  function getDiff(): { type: 'add'|'remove'|'same'; text: string }[] {
    const oldLines = savedContent.split('\n');
    const newLines = editorContent.split('\n');
    const result: { type: 'add'|'remove'|'same'; text: string }[] = [];
    const max = Math.max(oldLines.length, newLines.length);
    for (let i = 0; i < max; i++) {
      const o = oldLines[i];
      const n = newLines[i];
      if (o === undefined) {
        result.push({ type: 'add', text: n });
      } else if (n === undefined) {
        result.push({ type: 'remove', text: o });
      } else if (o === n) {
        result.push({ type: 'same', text: o });
      } else {
        result.push({ type: 'remove', text: o });
        result.push({ type: 'add', text: n });
      }
    }
    return result;
  }

  // ─── WWW Browser ───────────────────────────────────────────────────────────

  async function loadWww() {
    wwwLoading = true;
    try {
      wwwEntries = await invoke<WwwEntry[]>('nginx_list_www');
      statusStore.setLastCommand('ls -l /var/www', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load /var/www: ${e}`, 'error');
      statusStore.setLastCommand('ls -l /var/www', 1, false);
    } finally {
      wwwLoading = false;
    }
  }

  function toggleExpand(path: string) {
    const next = new Set(expandedPaths);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    expandedPaths = next;
  }

  async function viewWwwFile(entry: WwwEntry) {
    if (entry.is_dir) { toggleExpand(entry.path); return; }
    selectedWwwEntry = entry;
    wwwFileContent = '';
    wwwFileLoading = true;
    try {
      wwwFileContent = await invoke<string>('nginx_read_www_file', { path: entry.path });
      statusStore.setLastCommand(`cat ${entry.path}`, 0, true);
    } catch (e) {
      wwwFileContent = String(e);
      statusStore.setLastCommand(`cat ${entry.path}`, 1, false);
    } finally {
      wwwFileLoading = false;
    }
  }

  async function uploadFile(destDir: string) {
    try {
      const selected = await openDialog({ multiple: false, directory: false });
      if (!selected) return;
      const srcPath = typeof selected === 'string' ? selected : selected[0];
      await invoke('nginx_upload_www_file', { srcPath, destDir });
      statusStore.setLastCommand(`cp ${srcPath} ${destDir}`, 0, true);
      uiStore.addToast('File uploaded ✓', 'success');
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Upload failed: ${e}`, 'error');
      statusStore.setLastCommand(`cp ... ${destDir}`, 1, false);
    }
  }

  async function createDir() {
    if (!newDirName.trim()) return;
    const full = `${newDirParent}/${newDirName}`.replace(/\/\//g, '/');
    try {
      await invoke('nginx_create_www_dir', { path: full });
      statusStore.setLastCommand(`mkdir -p ${full}`, 0, true);
      uiStore.addToast(`Directory created: ${full}`, 'success');
      showNewDirForm = false;
      newDirName = '';
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Create dir failed: ${e}`, 'error');
      statusStore.setLastCommand(`mkdir -p ${full}`, 1, false);
    }
  }

  function confirmDeleteWww(entry: WwwEntry) {
    const trimmed = entry.path.replace(/\/+$/, '');
    if (trimmed === '/var/www' || trimmed === '/var/www/html' || trimmed === '') {
      uiStore.addToast('System Protection: Root /var/www and /var/www/html cannot be deleted!', 'error');
      return;
    }

    uiStore.confirm(
      `Delete ${entry.isDir ? 'Folder' : 'File'}`,
      `Are you sure you want to permanently delete "${entry.path}"${entry.isDir ? ' and all files within it? This will remove all web content permanently.' : '? This action cannot be undone.'}`,
      async () => {
        try {
          await invoke('nginx_delete_www_entry', { path: entry.path });
          statusStore.setLastCommand(`rm -rf ${entry.path}`, 0, true);
          uiStore.addToast(`Deleted "${entry.name}"`, 'success');
          if (selectedWwwEntry?.path === entry.path) selectedWwwEntry = null;
          await loadWww();
        } catch (e) {
          uiStore.addToast(`Delete failed: ${e}`, 'error');
          statusStore.setLastCommand(`rm -rf ${entry.path}`, 1, false);
        }
      },
      true,
    );
  }

  async function doRename() {
    if (!renamingEntry || !renameValue.trim()) return;
    const parentDir = renamingEntry.path.substring(0, renamingEntry.path.lastIndexOf('/'));
    const newPath = `${parentDir}/${renameValue}`;
    try {
      await invoke('nginx_rename_www_entry', { oldPath: renamingEntry.path, newPath });
      statusStore.setLastCommand(`mv ${renamingEntry.path} ${newPath}`, 0, true);
      uiStore.addToast('Renamed ✓', 'success');
      renamingEntry = null;
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Rename failed: ${e}`, 'error');
      statusStore.setLastCommand(`mv ${renamingEntry.path} ${newPath}`, 1, false);
    }
  }

  function formatSize(bytes: number): string {
    if (bytes === 0) return '—';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatBytes(bytes: number): string {
    if (!bytes || bytes === 0) return '0 B';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  // ─── WWW Context Menu ──────────────────────────────────────────────────────
  let wwwContextMenu = $state<{
    visible: boolean;
    x: number;
    y: number;
    entry: WwwEntry | null;
  }>({ visible: false, x: 0, y: 0, entry: null });

  function handleWwwContextMenu(e: MouseEvent, entry: WwwEntry | null) {
    e.preventDefault();
    e.stopPropagation();
    const menuWidth = 210;
    const menuHeight = 260;
    const x = Math.min(e.clientX, window.innerWidth - menuWidth - 12);
    const y = Math.min(e.clientY, window.innerHeight - menuHeight - 12);
    wwwContextMenu = {
      visible: true,
      x,
      y,
      entry,
    };
  }

  function closeWwwContextMenu() {
    wwwContextMenu.visible = false;
  }

  // Right-click Site Context Menu
  let siteContextMenu = $state<{
    visible: boolean;
    x: number;
    y: number;
    site: NginxSite | null;
  }>({ visible: false, x: 0, y: 0, site: null });

  function handleSiteContextMenu(e: MouseEvent, site: NginxSite) {
    e.preventDefault();
    e.stopPropagation();
    siteContextMenu = {
      visible: true,
      x: e.clientX,
      y: e.clientY,
      site,
    };
  }

  function closeSiteContextMenu() {
    siteContextMenu.visible = false;
  }

  // Global click & esc listener to close context menus and popovers
  $effect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (wwwContextMenu.visible) {
        closeWwwContextMenu();
      }
      if (siteContextMenu.visible) {
        closeSiteContextMenu();
      }
      if (showCustomPopover && popoverContainer && !popoverContainer.contains(e.target as Node)) {
        const target = e.target as HTMLElement;
        if (!target.closest('.custom-range-container') && !target.closest('.date-picker-container')) {
          showCustomPopover = false;
        }
      }
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        if (wwwContextMenu.visible) closeWwwContextMenu();
        if (siteContextMenu.visible) closeSiteContextMenu();
        if (showCustomPopover) showCustomPopover = false;
      }
    }
    window.addEventListener('click', handleOutsideClick);
    window.addEventListener('keydown', handleKey);
    return () => {
      window.removeEventListener('click', handleOutsideClick);
      window.removeEventListener('keydown', handleKey);
    };
  });

  // ─── Logs ──────────────────────────────────────────────────────────────────

  async function loadLogFiles() {
    try {
      logFiles = await invoke<string[]>('nginx_list_log_files');
      statusStore.setLastCommand('find /var/log/nginx -maxdepth 1 -type f', 0, true);
      if (logFiles.length > 0) {
        if (!selectedLog || !logFiles.includes(selectedLog)) {
          selectedLog = logFiles[0];
        }
        await loadLog();
      }
    } catch (e) {
      uiStore.addToast(`Failed to load log files: ${e}`, 'error');
      statusStore.setLastCommand('find /var/log/nginx -maxdepth 1 -type f', 1, false);
    }
  }

  async function loadLog() {
    if (!selectedLog && logFiles.length > 0) {
      selectedLog = logFiles[0];
    }
    if (!selectedLog) return;
    logLoading = true;
    try {
      logContent = await invoke<string>('nginx_read_log', {
        path: selectedLog,
        lines: 500,
        filter: null,
      });
      statusStore.setLastCommand(`tail -n 500 ${selectedLog}`, 0, true);
    } catch (e) {
      logContent = '';
      uiStore.addToast(`Failed to read log: ${e}`, 'error');
      statusStore.setLastCommand(`tail -n 500 ${selectedLog}`, 1, false);
    } finally {
      logLoading = false;
    }
  }

  function toggleAutoRefresh() {
    logAutoRefresh = !logAutoRefresh;
    if (logAutoRefresh) {
      logInterval = setInterval(() => loadLog(), 5000);
    } else {
      if (logInterval) { clearInterval(logInterval); logInterval = null; }
    }
  }

  function confirmClearLog() {
    if (!selectedLog) return;
    uiStore.confirm(
      'Truncate Log File',
      `Are you sure you want to clear and truncate "${selectedLog}" to 0 bytes?\n\nThis will permanently delete all records currently in this file. This action cannot be undone.`,
      async () => {
        try {
          await invoke('nginx_clear_log', { path: selectedLog });
          statusStore.setLastCommand(`truncate -s 0 ${selectedLog}`, 0, true);
          uiStore.addToast('Log cleared ✓', 'success');
          await loadLog();
        } catch (e) {
          uiStore.addToast(`Clear failed: ${e}`, 'error');
          statusStore.setLastCommand(`truncate -s 0 ${selectedLog}`, 1, false);
        }
      },
      true,
    );
  }

  async function exportLog() {
    const blob = new Blob([logContent], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = selectedLog.split('/').pop() ?? 'nginx.log';
    a.click();
    URL.revokeObjectURL(url);
  }

  // ─── SSL ───────────────────────────────────────────────────────────────────

  async function loadSslCerts() {
    sslLoading = true;
    try {
      sslCerts = await invoke<SslCert[]>('nginx_list_ssl_certs');
      statusStore.setLastCommand('certbot certificates', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load certs: ${e}`, 'error');
      statusStore.setLastCommand('certbot certificates', 1, false);
    } finally {
      sslLoading = false;
    }
  }

  async function renewCert(domain: string) {
    uiStore.confirm(
      'Renew Certificate',
      `Run certbot renew for "${domain}"?`,
      async () => {
        renewingCert = domain;
        try {
          const output = await invoke<string>('nginx_renew_cert', { domain });
          statusStore.setLastCommand(`certbot renew --cert-name ${domain}`, 0, true);
          uiStore.addToast(`Cert renewed for ${domain} ✓`, 'success');
          showOutputModal = true;
          outputModalTitle = 'certbot renew output';
          outputModalContent = output;
          await loadSslCerts();
        } catch (e) {
          uiStore.addToast(`Renewal failed: ${e}`, 'error');
          statusStore.setLastCommand(`certbot renew --cert-name ${domain}`, 1, false);
          showOutputModal = true;
          outputModalTitle = 'certbot renew failed';
          outputModalContent = String(e);
        } finally {
          renewingCert = '';
        }
      },
    );
  }

  let lastLoadedAnalyticsLog = $state('');

  async function loadAnalytics(notify = false) {
    analyticsLoading = true;
    try {
      analyticsData = await invoke<NginxLogAnalytics>('nginx_get_log_analytics', { path: analyticsLogFile });
      statusStore.setLastCommand(`tail -n 15000 ${analyticsLogFile}`, 0, true);
      lastLoadedAnalyticsLog = analyticsLogFile;
      if (notify) {
        uiStore.addToast(`Refreshed telemetry from ${analyticsLogFile.split('/').pop()}`, 'info');
      }
    } catch (e) {
      uiStore.addToast(`Failed to load analytics: ${e}`, 'error');
      statusStore.setLastCommand(`tail -n 15000 ${analyticsLogFile}`, 1, false);
    } finally {
      analyticsLoading = false;
    }
  }

  async function createProxy() {
    proxyConfigTouched.domain = true;
    proxyConfigTouched.target_ip = true;
    proxyConfigTouched.target_port = true;

    if (!isProxyConfigValid) {
      uiStore.addToast(proxyConfigErrors.domain || proxyConfigErrors.target_ip || proxyConfigErrors.target_port || 'Please resolve proxy form errors', 'warning');
      return;
    }
    proxyLoading = true;
    try {
      const confStr = await invoke<string>('nginx_generate_reverse_proxy', {
        domain: proxyConfig.domain.trim(),
        targetIp: proxyConfig.target_ip.trim(),
        targetPort: proxyConfig.target_port.trim(),
        enableWebsockets: proxyConfig.enable_websockets
      });
      const path = `/etc/nginx/sites-available/${proxyConfig.domain.trim()}.conf`;
      await invoke('nginx_write_config', { path, content: confStr });
      uiStore.addToast(`Reverse Proxy created at ${path} ✓`, 'success');
      showProxyWizard = false;
      proxyConfig = { domain: '', target_ip: '127.0.0.1', target_port: '8080', enable_websockets: false };
      proxyConfigTouched = { domain: false, target_ip: false, target_port: false };
      await loadSites();
      await loadStats();
    } catch (e) {
      uiStore.addToast(`Proxy creation failed: ${e}`, 'error');
      showOutputModal = true;
      outputModalTitle = 'Proxy Creation Failed';
      outputModalContent = String(e);
    } finally {
      proxyLoading = false;
    }
  }

  const tabDefs = $derived([
    { id: 'overview', label: 'Overview', icon: Activity },
    { id: 'sites',    label: 'Sites',    icon: Globe },
    { id: 'editor',   label: 'Config Editor', icon: FileCode },
    { id: 'www',      label: 'WWW Files', icon: FolderOpen },
    { id: 'logs',     label: 'Logs',     icon: FileText },
    { id: 'analytics',label: 'Analytics',icon: BarChart2 },
    { id: 'ssl',      label: 'SSL / Certs', icon: Lock },
  ] as { id: typeof activeTab; label: string; icon: any }[]);

  $effect(() => {
    if (uiStore.targetSubTab && ['overview', 'sites', 'editor', 'www', 'logs', 'analytics', 'ssl'].includes(uiStore.targetSubTab)) {
      activeTab = uiStore.targetSubTab as any;
      uiStore.targetSubTab = null;
    }
  });

  let loadedTabs = new Set<string>();
  let prevActiveTab = '';

  $effect(() => {
    const currentTab = activeTab;
    if (currentTab !== prevActiveTab) {
      prevActiveTab = currentTab;
      if (currentTab === 'editor' && !loadedTabs.has('editor')) {
        loadedTabs.add('editor');
        loadConfigs();
      } else if (currentTab === 'sites' && !loadedTabs.has('sites')) {
        loadedTabs.add('sites');
        loadSites();
      } else if (currentTab === 'www' && !loadedTabs.has('www')) {
        loadedTabs.add('www');
        loadWww();
      } else if (currentTab === 'logs' && !loadedTabs.has('logs')) {
        loadedTabs.add('logs');
        loadLogFiles();
      } else if (currentTab === 'analytics') {
        if (!analyticsData || analyticsLogFile !== lastLoadedAnalyticsLog) {
          loadAnalytics(false);
        }
      } else if (currentTab === 'ssl' && !loadedTabs.has('ssl')) {
        loadedTabs.add('ssl');
        if (hasCertbot) loadSslCerts();
      }
    }
  });

  function hasChanges() {
    return editorContent !== savedContent;
  }
</script>

<!-- ─── Page ──────────────────────────────────────────────────────────── -->
<div class="module-page">
  <!-- Header -->
  <PageHeader 
    title="Nginx Manager" 
    icon={Server} 
  >
    {#if activeTab === 'logs'}
      <!-- Range Selector (Journal Logs Style) placed in Page Header -->
      <div class="custom-range-container">
        <Select bind:value={timeRange} onchange={handleRangeChange} style="height: 30px; width: 140px;">
          <option value="all">All Logs</option>
          <option value="1">Last 24 Hours</option>
          <option value="3">Last 3 Days</option>
          <option value="7">Last 7 Days</option>
          <option value="30">Last 30 Days</option>
          <option value="custom">{customRangeLabel}</option>
        </Select>

        {#if timeRange === 'custom' && showCustomPopover}
          <div bind:this={popoverContainer} class="custom-range-popover">
            <div class="popover-row">
              <span class="popover-label">From</span>
              <DatePicker bind:value={customStartDate} placeholder="Start date" />
              <input
                type="time"
                bind:value={customStartTime}
                class="log-dt"
              />
            </div>
            <div class="popover-row" style="margin-top: 10px;">
              <span class="popover-label">To</span>
              <DatePicker bind:value={customEndDate} placeholder="End date" />
              <input
                type="time"
                bind:value={customEndTime}
                class="log-dt"
              />
            </div>
            <div class="popover-actions">
              <button
                type="button"
                class="popover-btn apply-btn"
                onclick={() => {
                  showCustomPopover = false;
                }}
              >
                Apply
              </button>
              <button
                type="button"
                class="popover-btn cancel-btn"
                onclick={() => {
                  showCustomPopover = false;
                  if (!customStartDate) timeRange = 'all';
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    {#if aiStore.enabled && installInfo?.installed}
      <Button variant="outline" size="sm" onclick={() => showAiNginxPromptBox = !showAiNginxPromptBox} title="Generate NGINX configuration block with AI">
        <Sparkles size={14} style="color:var(--color-accent);" /> AI Config Generator
      </Button>
    {/if}
  </PageHeader>

  {#if showAiNginxPromptBox}
    <div style="margin: 0 0 16px 0; padding: 14px 16px; background: rgba(0,218,243,0.06); border: 1px solid rgba(0,218,243,0.2); border-radius: 10px; display: flex; flex-direction: column; gap: 10px;">
      <div style="display: flex; align-items: center; justify-content: space-between;">
        <span style="font-size: 13px; font-weight: 600; color: var(--color-text-primary); display: flex; align-items: center; gap: 6px;">
          <Sparkles size={15} style="color: var(--color-accent);" /> AI NGINX Server Block Generator
        </span>
        <button type="button" onclick={() => showAiNginxPromptBox = false} style="background:none; border:none; color:var(--color-text-muted); cursor:pointer; font-size:12px;">Cancel</button>
      </div>
      <div style="display: flex; gap: 8px;">
        <input
          type="text"
          bind:value={aiNginxPrompt}
          placeholder="e.g. Create a reverse proxy for Node.js app on port 3000 with SSL redirect for example.com..."
          onkeydown={(e) => { if (e.key === 'Enter') triggerAiNginxGen(); }}
          style="flex: 1; padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12.5px;"
        />
        <Button variant="primary" size="sm" onclick={triggerAiNginxGen} disabled={!aiNginxPrompt.trim()}>
          Generate Config
        </Button>
      </div>
    </div>
  {/if}

  {#if loading}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Checking nginx installation…</span>
    </div>
  {:else if !installInfo?.installed}
    <!-- ─── Not Installed State ─── -->
    <div class="not-installed">
      <div class="ni-icon"><Server size={48} /></div>
      <h2>nginx is not installed on your system</h2>
      <p>Install nginx to use this module.</p>
      <div class="ni-cmds">
        <div class="ni-cmd">
          <span class="ni-cmd-label">Fedora / RHEL:</span>
          <code>sudo dnf install nginx</code>
        </div>
        <div class="ni-cmd">
          <span class="ni-cmd-label">Debian / Ubuntu:</span>
          <code>sudo apt install nginx</code>
        </div>
        <div class="ni-cmd">
          <span class="ni-cmd-label">Arch:</span>
          <code>sudo pacman -S nginx</code>
        </div>
      </div>
      <Button variant="primary" class="" onclick={init}>
        <RefreshCw size={14} /> Recheck
      </Button>
    </div>
  {:else}
    <!-- ─── Controls: Tabs & Actions ─── -->
    <div class="controls-row">
      <div class="tab-bar">
        {#each tabDefs as tab}
          <button class="tab-btn { activeTab === tab.id ? 'active' : '' }"
            onclick={() => (activeTab = tab.id)}
            id={`nginx-tab-${tab.id}`}
          >
            <tab.icon size={14} />
            {tab.label}
          </button>
        {/each}
      </div>

      <div class="tab-actions">
        {#if activeTab === 'sites'}
          <Button variant="outline" class="btn-sm" onclick={loadSites} id="nginx-refresh-sites">
            <RefreshCw size={13} /> Refresh
          </Button>
          <Button variant="primary" class="btn-sm" onclick={() => (showNewSiteForm = true)} id="nginx-new-site">
            <Plus size={13} /> New Site
          </Button>
          <Button variant="primary" class="btn-sm" onclick={() => (showProxyWizard = true)} id="nginx-new-proxy">
            <Globe size={13} /> New Proxy
          </Button>
        {:else if activeTab === 'editor'}
          <Button variant="outline" class="btn-sm" onclick={loadConfigs} id="nginx-refresh-configs">
            <RefreshCw size={13} /> Refresh
          </Button>
          <Button variant="outline" class="btn-sm" onclick={() => { showBackups = !showBackups; if (showBackups) loadBackups(); }} id="nginx-show-backups">
            <ArchiveRestore size={13} /> Backups ({backups.length})
          </Button>
        {:else if activeTab === 'www'}
          <Button variant="outline" class="btn-sm" onclick={loadWww} id="nginx-refresh-www"><RefreshCw size={13} /> Refresh</Button>
          <Button variant="primary" class="btn-sm" onclick={() => { showNewDirForm = !showNewDirForm; newDirParent = '/var/www'; }} id="nginx-new-dir">
            <FolderPlus size={13} /> New Dir
          </Button>
        {:else if activeTab === 'logs'}
          <div style="width: 260px; max-width: 280px; flex-shrink: 0;">
            <Select bind:value={selectedLog} onchange={loadLog} id="nginx-log-select">
              {#if selectedLog && !logFiles.includes(selectedLog)}
                <option value={selectedLog}>{selectedLog.split('/').pop()} • Custom</option>
              {/if}
              {#each logFiles as lf}
                {@const name = lf.split('/').pop() || lf}
                {@const isRotated = name.includes('-') || name.endsWith('.gz') || name.includes('.1')}
                <option value={lf}>{name} {isRotated ? '• Archive' : '• Active'}</option>
              {/each}
            </Select>
          </div>
          <div style="flex: 1; min-width: 200px;">
            <SearchBar bind:value={logFilter} placeholder="Filter logs (e.g. /api, 404, 192.168, GET)…" style="margin:0; width: 100%;" />
          </div>
          <div style="display:flex; gap:6px; align-items:center; flex-shrink:0;">
            <Button variant="outline" class="btn-sm" onclick={loadLog} id="nginx-log-refresh">
              <RefreshCw size={13} class={logLoading ? 'animate-spin-slow' : ''} /> Refresh
            </Button>
            <Button class="btn-sm {logAutoRefresh ? 'btn-primary' : '-outline'}" onclick={toggleAutoRefresh} id="nginx-log-auto">
              <Clock size={13} /> {logAutoRefresh ? 'Live: On' : 'Live: Off'}
            </Button>
            <Button variant="outline" class="btn-sm" onclick={exportLog} id="nginx-log-export">
              <Download size={13} /> Export
            </Button>
            <Button variant="danger" class="btn-sm" onclick={confirmClearLog} id="nginx-log-clear">
              <Trash2 size={13} /> Clear
            </Button>
          </div>
        {:else if activeTab === 'ssl'}
          <Button variant="outline" class="btn-sm" onclick={loadSslCerts} id="nginx-refresh-ssl">
            <RefreshCw size={13} /> Refresh
          </Button>
        {/if}
      </div>
    </div>

    <!-- ─── Tab Content ─── -->
    <div class="tab-content module-content-scroll">

      <!-- ══ OVERVIEW ══════════════════════════════════════════════════════ -->
      {#if activeTab === 'overview'}
        <div class="tab-section overview-section" style="display:flex; flex-direction:column; gap:20px;">
          <!-- Top Action Ribbon -->
          <div class="ov-action-ribbon">
            <div class="ov-ribbon-left">
              <div class="ov-service-pill {serviceStatus?.active ? 'active' : 'inactive'}">
                <span class="status-dot {serviceStatus?.active ? 'dot-active' : 'dot-inactive'}"></span>
                <span style="font-weight:600; font-size:12.5px;">
                  {serviceStatus?.active ? 'Nginx Active & Serving Traffic' : 'Nginx Service Stopped'}
                </span>
                {#if serviceStatus?.since}
                  <span class="ov-uptime-tag">Started: {serviceStatus.since}</span>
                {/if}
              </div>
            </div>

            <div class="ov-ribbon-right">
              <Button 
                variant="primary" 
                class="btn-sm" 
                onclick={testAndReload} 
                disabled={testLoading || serviceLoading} 
                title="Test configuration syntax with nginx -t and safely reload service"
              >
                {#if testLoading}
                  <div class="spinner-sm"></div>
                {:else}
                  <Zap size={13} />
                {/if}
                <span>Test &amp; Reload Safe</span>
              </Button>

              <Button 
                variant="outline" 
                class="btn-sm" 
                onclick={() => { activeTab = 'sites'; showNewSiteForm = true; }}
                title="Create a new virtual host"
              >
                <Plus size={13} />
                <span>New Site</span>
              </Button>

              <Button 
                variant="outline" 
                class="btn-sm" 
                onclick={() => { newSite.is_proxy = true; activeTab = 'sites'; showNewSiteForm = true; }}
                title="Create a reverse proxy config"
              >
                <ArrowUpRight size={13} />
                <span>New Proxy</span>
              </Button>

              <Button 
                variant="outline" 
                class="btn-sm" 
                onclick={() => Promise.all([loadServiceStatus(), loadTestResult(), loadStats(), loadSites()])}
                title="Refresh all overview statistics"
              >
                <RefreshCw size={13} class={serviceLoading || testLoading ? 'animate-spin' : ''} />
                <span>Refresh</span>
              </Button>
            </div>
          </div>

          <!-- Hero 2-Column Grid -->
          <div class="overview-grid">
            <!-- Card 1: Service Lifecycle -->
            <div class="card ov-card">
              <div class="ov-card-header">
                <div class="ov-card-title">
                  <Activity size={16} class="text-accent" />
                  <span>Service Control</span>
                </div>
                <span class="badge {serviceStatus?.active ? 'badge-success' : 'badge-error'}">
                  {serviceStatus ? `${serviceStatus.status} (${serviceStatus.sub_state})` : 'Checking…'}
                </span>
              </div>

              <div class="service-control-body">
                <p class="ov-since" style="margin-bottom: 12px;">
                  systemd service: <code>nginx.service</code>
                  {#if serviceStatus?.since}
                    • Uptime: <strong>{serviceStatus.since}</strong>
                  {/if}
                </p>

                <div class="service-btns">
                  {#each [['start','Start',false,Play], ['stop','Stop',true,Square], ['restart','Restart',false,RotateCcw], ['reload','Reload',false,RefreshCw]] as [action, label, isDanger, Icon]}
                    <Button
                      variant={isDanger ? 'danger' : 'outline'}
                      class="btn-sm"
                      onclick={() => requestServiceAction(action as string)}
                      disabled={serviceLoading}
                      id={`nginx-svc-${action}`}
                    >
                      {#if serviceLoading}
                        <div class="spinner-sm"></div>
                      {:else}
                        <Icon size={12} />
                      {/if}
                      <span>{label}</span>
                    </Button>
                  {/each}
                </div>
              </div>
            </div>

            <!-- Card 2: Configuration Syntax Test (nginx -t) -->
            <div class="card ov-card">
              <div class="ov-card-header">
                <div class="ov-card-title">
                  <TerminalSquare size={16} class="text-accent" />
                  <span>Config Test (nginx -t)</span>
                </div>
                <Button 
                  variant="outline" 
                  class="btn-sm" 
                  onclick={runTest} 
                  disabled={testLoading} 
                  id="nginx-run-test"
                  title="Run nginx -t syntax validation without dialog"
                >
                  {#if testLoading}
                    <div class="spinner-sm"></div>
                  {:else}
                    <RefreshCw size={12} />
                  {/if}
                  <span>Run Test</span>
                </Button>
              </div>

              {#if testResult}
                <div class="test-result {testResult.passed ? 'test-pass' : 'test-fail'}">
                  {#if testResult.passed}
                    <CheckCircle size={17} /> <span>Configuration Syntax OK</span>
                  {:else}
                    <XCircle size={17} /> <span>Configuration Syntax Error</span>
                  {/if}
                  <span style="margin-left:auto; font-size:11px; opacity:0.8; font-weight:normal;">{testResult.timestamp}</span>
                </div>

                {#if testResult.output && testResult.output.trim()}
                  <div class="test-output-wrap">
                    <pre class="test-output">{testResult.output}</pre>
                    <button 
                      type="button" 
                      class="inspect-copy-icon-btn" 
                      style="position:absolute; top:6px; right:8px;"
                      onclick={() => { navigator.clipboard.writeText(testResult!.output); uiStore.addToast('Copied test output', 'info'); }}
                      title="Copy test output"
                    >
                      <Copy size={12} />
                    </button>
                  </div>
                {/if}
              {:else}
                <p class="ov-since">Click "Run Test" to validate configuration syntax.</p>
              {/if}
            </div>
          </div>

          <!-- Middle 3-Column Metrics Grid -->
          <div class="overview-tri-grid">
            <!-- Tri 1: Virtual Hosts & Routing -->
            <div class="card ov-card">
              <div class="ov-card-header">
                <div class="ov-card-title">
                  <Globe size={15} class="text-accent" />
                  <span>Virtual Hosts</span>
                </div>
                <button type="button" class="ov-link-btn" onclick={() => activeTab = 'sites'}>
                  <span>Manage</span>
                  <ArrowUpRight size={12} />
                </button>
              </div>

              <div class="stats-grid">
                <div class="stat-item">
                  <span class="stat-value">{stats?.sites_available ?? sites.length}</span>
                  <span class="stat-label">Total</span>
                </div>
                <div class="stat-item stat-enabled">
                  <span class="stat-value">{stats?.sites_enabled ?? sites.filter(s => s.enabled).length}</span>
                  <span class="stat-label">Active</span>
                </div>
                <div class="stat-item stat-disabled">
                  <span class="stat-value">{stats?.sites_disabled ?? sites.filter(s => !s.enabled).length}</span>
                  <span class="stat-label">Disabled</span>
                </div>
              </div>

              <div style="display:flex; align-items:center; gap:6px; flex-wrap:wrap; margin-top:4px;">
                <span style="font-size:11px; color:var(--color-text-muted); font-weight:600;">Ports:</span>
                {#each detectedOverviewPorts as p}
                  <span class="port-badge {p.includes('SSL') ? 'ssl' : 'plain'}">{p}</span>
                {/each}
                {#if totalProxiesCount > 0}
                  <span class="site-meta-pill proxy" style="margin-left:auto;">
                    <ArrowUpRight size={10} /> {totalProxiesCount} Prox{totalProxiesCount > 1 ? 'ies' : 'y'}
                  </span>
                {/if}
              </div>
            </div>

            <!-- Tri 2: SSL & Encryption -->
            <div class="card ov-card">
              <div class="ov-card-header">
                <div class="ov-card-title">
                  <Lock size={15} class="text-accent" />
                  <span>SSL &amp; Security</span>
                </div>
                <button type="button" class="ov-link-btn" onclick={() => activeTab = 'ssl'}>
                  <span>View Certs</span>
                  <ArrowUpRight size={12} />
                </button>
              </div>

              <div style="display:flex; flex-direction:column; gap:8px;">
                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <span style="font-size:12px; color:var(--color-text-secondary);">Certbot Engine</span>
                  <span class="badge {hasCertbot ? 'badge-success' : 'badge-warning'}">
                    {hasCertbot ? '✓ Ready' : '⚠ Not Installed'}
                  </span>
                </div>

                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <span style="font-size:12px; color:var(--color-text-secondary);">Managed Certificates</span>
                  <span style="font-weight:600; font-size:12.5px; color:var(--color-text-primary);">{sslCerts.length} active</span>
                </div>

                {#if !hasCertbot}
                  <div style="display:flex; align-items:center; justify-content:space-between; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:6px; padding:6px 10px; margin-top:2px;">
                    <code style="font-family:var(--font-mono); font-size:10.5px; color:var(--color-accent); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">sudo dnf install certbot python3-certbot-nginx</code>
                    <button 
                      type="button" 
                      class="inspect-copy-icon-btn" 
                      onclick={() => { navigator.clipboard.writeText('sudo dnf install certbot python3-certbot-nginx'); uiStore.addToast('Copied installation command', 'info'); }}
                      title="Copy install command"
                    >
                      <Copy size={12} />
                    </button>
                  </div>
                {:else}
                  <div class="ov-ssl-ok-note">
                    <CheckCircle size={13} class="text-success" />
                    <span>Automatic HTTPS &amp; renewal engine ready</span>
                  </div>
                {/if}
              </div>
            </div>

            <!-- Tri 3: Engine & System Paths -->
            <div class="card ov-card">
              <div class="ov-card-header">
                <div class="ov-card-title">
                  <Server size={15} class="text-accent" />
                  <span>Environment Paths</span>
                </div>
                {#if installInfo.version}
                  <span class="version-display" style="padding:2px 8px; font-size:11px;">{installInfo.version}</span>
                {/if}
              </div>

              <div class="ov-env-list">
                <div class="ov-env-item">
                  <span class="ov-env-label">Main Config:</span>
                  <button type="button" class="ov-env-btn" onclick={() => activeTab = 'editor'}>
                    <FileCode size={11} />
                    <code>/etc/nginx/nginx.conf</code>
                  </button>
                </div>

                <div class="ov-env-item">
                  <span class="ov-env-label">Web Root:</span>
                  <button type="button" class="ov-env-btn" onclick={() => activeTab = 'www'}>
                    <FolderOpen size={11} />
                    <code>/var/www</code>
                  </button>
                </div>

                <div class="ov-env-item">
                  <span class="ov-env-label">Log Stream:</span>
                  <button type="button" class="ov-env-btn" onclick={() => activeTab = 'logs'}>
                    <FileText size={11} />
                    <code>/var/log/nginx/</code>
                  </button>
                </div>
              </div>
            </div>
          </div>

          <!-- Bottom: Configured Virtual Hosts Preview Table -->
          <div class="card ov-card">
            <div class="ov-card-header">
              <div class="ov-card-title">
                <Globe size={16} class="text-accent" />
                <span>Configured Virtual Hosts</span>
                <span class="badge badge-muted" style="margin-left:6px;">{sites.length} total</span>
              </div>
              <Button variant="outline" class="btn-sm" onclick={() => activeTab = 'sites'}>
                <span>Open Sites Manager</span>
                <ArrowUpRight size={13} />
              </Button>
            </div>

            {#if sites.length === 0}
              <div class="empty-state" style="padding:24px;">No virtual hosts loaded yet.</div>
            {:else}
              <div class="ov-sites-mini-table-wrap">
                <table class="ov-sites-mini-table">
                  <thead>
                    <tr>
                      <th>Virtual Host</th>
                      <th>Ports &amp; SSL</th>
                      <th>Source</th>
                      <th>Status</th>
                      <th style="text-align:right;">Quick Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each sites as site}
                      <tr 
                        class="ov-site-preview-row" 
                        oncontextmenu={(e) => handleSiteContextMenu(e, site)}
                        onclick={() => openSiteInspector(site)}
                        style="cursor: pointer;"
                        title="Right-click for actions or click to inspect {site.name}"
                      >
                        <td>
                          <div style="display:flex; align-items:center; gap:8px;">
                            <Globe size={13} class="text-accent" />
                            <strong>{site.name}</strong>
                            {#if site.domains && site.domains.length > 0}
                              {#each site.domains as dom}
                                <span class="site-meta-pill domain" style="font-size:10px;">{dom}</span>
                              {/each}
                            {/if}
                            {#if site.proxies && site.proxies.length > 0}
                              {#each site.proxies as prx}
                                <span class="site-meta-pill proxy" style="font-size:10px;">{prx}</span>
                              {/each}
                            {/if}
                          </div>
                        </td>
                        <td>
                          <div style="display:flex; align-items:center; gap:4px;">
                            {#if site.ports && site.ports.length > 0}
                              {#each site.ports as p}
                                <span class="port-badge {p.includes('SSL') || site.has_ssl ? 'ssl' : 'plain'}" style="font-size:10px; padding:1px 5px;">
                                  {#if p.includes('SSL') || site.has_ssl}<Lock size={9} />{/if}
                                  {p}
                                </span>
                              {/each}
                            {:else}
                              <span class="port-badge plain" style="font-size:10px; padding:1px 5px;">80</span>
                            {/if}
                          </div>
                        </td>
                        <td><span class="badge badge-muted" style="font-size:10.5px;">{site.source}</span></td>
                        <td>
                          <span class="badge {site.enabled ? 'badge-success' : 'badge-error'}" style="font-size:10.5px;">
                            {site.enabled ? 'Enabled' : 'Disabled'}
                          </span>
                        </td>
                        <td style="text-align:right;" onclick={(e) => e.stopPropagation()}>
                          <div style="display:flex; justify-content:flex-end;">
                            <KebabMenu align="right" title={`Actions for ${site.name}`}>
                              <button class="menu-item" onclick={() => openSiteInspector(site)}>
                                <Eye size={13} />
                                <span>Quick Inspect</span>
                              </button>

                              <button class="menu-item" onclick={() => openSiteInEditor(site)}>
                                <FileCode size={13} />
                                <span>Edit Configuration</span>
                              </button>

                              <button class="menu-item" onclick={() => openCloneModal(site)}>
                                <Copy size={13} />
                                <span>Clone / Duplicate Site</span>
                              </button>

                              <button class="menu-item" onclick={() => jumpToSiteLogs(site, 'analytics')}>
                                <BarChart2 size={13} />
                                <span>View Logs &amp; Analytics</span>
                              </button>

                              {#if !site.has_ssl}
                                <button class="menu-item" onclick={() => openQuickSsl(site)}>
                                  <Sparkles size={13} class="text-accent" />
                                  <span>Issue Let's Encrypt SSL</span>
                                </button>
                              {/if}

                              {#if site.source === 'sites-available'}
                                <button 
                                  class="menu-item" 
                                  onclick={() => toggleSite(site)}
                                  disabled={toggleLoadingFor === site.name}
                                >
                                  {#if site.enabled}
                                    <EyeOff size={13} />
                                    <span>Disable Site</span>
                                  {:else}
                                    <Eye size={13} />
                                    <span>Enable Site</span>
                                  {/if}
                                </button>
                              {/if}

                              <button class="menu-item" onclick={() => { navigator.clipboard.writeText(site.path); uiStore.addToast('Copied site path to clipboard', 'info'); }}>
                                <Copy size={13} />
                                <span>Copy Config Path</span>
                              </button>

                              <div class="menu-divider"></div>

                              <button class="menu-item danger" onclick={() => confirmDeleteSite(site)}>
                                <Trash2 size={13} />
                                <span>Delete Virtual Host</span>
                              </button>
                            </KebabMenu>
                          </div>
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {/if}
          </div>
        </div>

      <!-- ══ SITES ══════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'sites'}
        <div class="tab-section">


          {#if showNewSiteForm}
            <div class="card new-site-form">
              <h4 class="form-title">Create New Site</h4>
              <div class="form-grid">
                <label class="form-field">
                  <span>Server Name (domain)</span>
                  <input type="text" bind:value={newSite.server_name} placeholder="example.com" id="nginx-site-name" />
                </label>
                <label class="form-field">
                  <span>Port</span>
                  <input type="number" bind:value={newSite.port} min={1} max={65535} id="nginx-site-port" />
                </label>
                <label class="form-field form-toggle">
                  <span>Reverse Proxy</span>
                  <button
                    class="ui-toggle"
                    class:on={newSite.is_proxy}
                    onclick={() => newSite.is_proxy = !newSite.is_proxy}
                    type="button"
                    role="switch"
                    aria-checked={newSite.is_proxy}
                    aria-label="Toggle Reverse Proxy"
                    id="nginx-site-proxy"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                </label>
                {#if newSite.is_proxy}
                  <label class="form-field form-full">
                    <span>Proxy Target URL</span>
                    <input type="text" bind:value={newSite.proxy_url} placeholder="http://localhost:3000" id="nginx-site-proxy-url" />
                  </label>
                {:else}
                  <label class="form-field">
                    <span>Root Directory</span>
                    <input type="text" bind:value={newSite.root_dir} placeholder="/var/www/html" id="nginx-site-root" />
                  </label>
                  <label class="form-field">
                    <span>Index File</span>
                    <input type="text" bind:value={newSite.index_file} id="nginx-site-index" />
                  </label>
                {/if}
                <label style="display:flex; align-items:center; gap:8px; font-size:12px; color:var(--color-text-secondary); cursor:pointer;">
                  <button
                    class="ui-toggle"
                    class:on={newSite.enable_404}
                    onclick={() => newSite.enable_404 = !newSite.enable_404}
                    type="button"
                    role="switch"
                    aria-checked={newSite.enable_404}
                    aria-label="Toggle custom 404 page"
                    id="nginx-site-404"
                    style="transform: scale(0.8);"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                  Include 404 error page
                </label>
                <label style="display:flex; align-items:center; gap:8px; font-size:12px; color:var(--color-text-secondary); cursor:pointer;">
                  <button
                    class="ui-toggle"
                    class:on={newSite.enable_50x}
                    onclick={() => newSite.enable_50x = !newSite.enable_50x}
                    type="button"
                    role="switch"
                    aria-checked={newSite.enable_50x}
                    aria-label="Toggle custom 50x page"
                    id="nginx-site-50x"
                    style="transform: scale(0.8);"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                  Include 50x error pages
                </label>
              </div>
              <div class="form-actions">
                <Button variant="ghost" class="" onclick={() => (showNewSiteForm = false)}>Cancel</Button>
                <Button variant="primary" class="" onclick={createSite} disabled={newSiteLoading} id="nginx-create-site-submit">
                  {#if newSiteLoading}<div class="spinner-sm"></div>{/if}
                  Create Site
                </Button>
              </div>
            </div>
          {/if}

          {#if sitesLoading}
            <div class="center-state"><div class="spinner"></div></div>
          {:else if sites.length === 0}
            <div class="empty-state">No site configurations found</div>
          {:else}
            <!-- Sites Toolbar -->
            <div class="sites-toolbar">
              <div class="sites-search-wrap">
                <SearchBar 
                  bind:value={siteSearchQuery} 
                  placeholder="Search sites by name, source, or path…" 
                />
              </div>
              <div class="sites-filter-pills">
                <button 
                  type="button" 
                  class="filter-pill" 
                  class:active={siteSourceFilter === 'all'} 
                  onclick={() => siteSourceFilter = 'all'}
                >
                  All ({sites.length})
                </button>
                <button 
                  type="button" 
                  class="filter-pill" 
                  class:active={siteSourceFilter === 'conf.d'} 
                  onclick={() => siteSourceFilter = 'conf.d'}
                >
                  conf.d ({sites.filter(s => s.source === 'conf.d').length})
                </button>
                <button 
                  type="button" 
                  class="filter-pill" 
                  class:active={siteSourceFilter === 'sites-available'} 
                  onclick={() => siteSourceFilter = 'sites-available'}
                >
                  sites-available ({sites.filter(s => s.source === 'sites-available').length})
                </button>
              </div>
            </div>

            <div class="table-wrap">
              <table use:tableFeatures>
                <thead>
                  <tr>
                    <th>Site &amp; Routing</th>
                    <th>Ports &amp; SSL</th>
                    <th>Source</th>
                    <th>Status</th>
                    <th>Configuration Path</th>
                    <th style="text-align:right;">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {#if filteredSites.length === 0}
                    <tr>
                      <td colspan="6" style="text-align:center; padding:32px; color:var(--color-text-muted);">
                        No sites matching "{siteSearchQuery}"
                      </td>
                    </tr>
                  {:else}
                    {#each filteredSites as site}
                      <tr class="site-row" oncontextmenu={(e) => handleSiteContextMenu(e, site)}>
                        <!-- Column 1: Site Name & Routing -->
                        <td class="site-name-cell-wrapper">
                          <div style="display:flex; flex-direction:column; gap:4px;">
                            <button 
                              type="button" 
                              class="site-name-btn" 
                              onclick={() => openSiteInEditor(site)} 
                              title="Open {site.name} in Config Editor"
                            >
                              <div class="site-name-icon-box">
                                <Globe size={14} class="site-name-icon" />
                              </div>
                              <div class="site-name-info">
                                <span class="site-name-title">{site.name}</span>
                              </div>
                            </button>

                            <!-- Detected Domains & Proxy Targets -->
                            {#if (site.domains && site.domains.length > 0) || (site.proxies && site.proxies.length > 0)}
                              <div class="site-routing-tags">
                                {#if site.domains}
                                  {#each site.domains as dom}
                                    <span class="site-meta-pill domain" title="Configured Server Name">
                                      <Globe size={10} />
                                      <span>{dom}</span>
                                    </span>
                                  {/each}
                                {/if}
                                {#if site.proxies}
                                  {#each site.proxies as prx}
                                    <span class="site-meta-pill proxy" title="Reverse Proxy Target">
                                      <ArrowUpRight size={10} />
                                      <span>{prx}</span>
                                    </span>
                                  {/each}
                                {/if}
                              </div>
                            {/if}
                          </div>
                        </td>

                        <!-- Column 2: Ports & SSL -->
                        <td>
                          <div class="site-ports-cell">
                            {#if site.ports && site.ports.length > 0}
                              {#each site.ports as p}
                                <span class="port-badge {p.includes('SSL') || site.has_ssl ? 'ssl' : 'plain'}">
                                  {#if p.includes('SSL') || site.has_ssl}
                                    <Lock size={10} />
                                  {/if}
                                  <span>{p}</span>
                                </span>
                              {/each}
                            {:else}
                              <span class="port-badge plain">80</span>
                            {/if}

                            {#if !site.has_ssl}
                              <button 
                                type="button" 
                                class="btn-quick-ssl-badge" 
                                onclick={() => openQuickSsl(site)} 
                                title="Issue automated Let's Encrypt SSL certificate via Certbot"
                              >
                                <Sparkles size={10} />
                                <span>Get SSL</span>
                              </button>
                            {/if}
                          </div>
                        </td>

                        <!-- Column 3: Source -->
                        <td><span class="badge badge-muted">{site.source}</span></td>

                        <!-- Column 4: Status -->
                        <td>
                          <span class="badge {site.enabled ? 'badge-success' : 'badge-error'}">
                            {site.enabled ? 'Enabled' : 'Disabled'}
                          </span>
                        </td>

                        <!-- Column 5: Path -->
                        <td>
                          <div 
                            class="site-path-wrap" 
                            title="Click to copy path" 
                            onclick={() => { navigator.clipboard.writeText(site.path); uiStore.addToast('Copied site path', 'info'); }}
                          >
                            <code class="path-code">{site.path}</code>
                            <Copy size={11} class="path-copy-icon" />
                          </div>
                        </td>

                        <!-- Column 6: Actions -->
                        <td style="text-align: right;">
                          <div class="row-actions" style="justify-content: flex-end;">
                            <KebabMenu align="right" title={`Actions for ${site.name}`}>
                              <button class="menu-item" onclick={() => openSiteInspector(site)}>
                                <Eye size={13} />
                                <span>Quick Inspect</span>
                              </button>

                              <button class="menu-item" onclick={() => openSiteInEditor(site)}>
                                <FileCode size={13} />
                                <span>Edit in Full Editor</span>
                              </button>

                              <button class="menu-item" onclick={() => openCloneModal(site)}>
                                <Copy size={13} />
                                <span>Clone / Duplicate Site</span>
                              </button>

                              <button class="menu-item" onclick={() => jumpToSiteLogs(site, 'analytics')}>
                                <BarChart2 size={13} />
                                <span>View Logs &amp; Analytics</span>
                              </button>

                              {#if !site.has_ssl}
                                <button class="menu-item" onclick={() => openQuickSsl(site)}>
                                  <Sparkles size={13} class="text-accent" />
                                  <span>Issue Let's Encrypt SSL</span>
                                </button>
                              {/if}

                              {#if site.source === 'sites-available'}
                                <button 
                                  class="menu-item" 
                                  onclick={() => toggleSite(site)}
                                  disabled={toggleLoadingFor === site.name}
                                >
                                  {#if site.enabled}
                                    <EyeOff size={13} />
                                    <span>Disable Site</span>
                                  {:else}
                                    <Eye size={13} />
                                    <span>Enable Site</span>
                                  {/if}
                                </button>
                              {/if}

                              <button 
                                class="menu-item" 
                                onclick={() => { navigator.clipboard.writeText(site.path); uiStore.addToast('Copied site path to clipboard', 'info'); }}
                              >
                                <Copy size={13} />
                                <span>Copy File Path</span>
                              </button>

                              <div style="height:1px; background:var(--color-border); margin:4px 0;"></div>

                              <button 
                                class="menu-item danger" 
                                onclick={() => confirmDeleteSite(site)}
                              >
                                <Trash2 size={13} />
                                <span>Delete Site</span>
                              </button>
                            </KebabMenu>
                          </div>
                        </td>
                      </tr>
                    {/each}
                  {/if}
                </tbody>
              </table>
            </div>
          {/if}
        </div>

      <!-- ══ CONFIG EDITOR ══════════════════════════════════════════════════ -->
      {:else if activeTab === 'editor'}
        <div class="editor-layout">
          <!-- File List -->
          <div class="editor-sidebar">


            {#if editorLoading && configs.length === 0}
              <div class="center-state"><div class="spinner-sm"></div></div>
            {:else}
              {#each ['nginx', 'conf.d', 'sites-available'] as src}
                {@const group = configs.filter(c => c.source === src)}
                {#if group.length > 0}
                  <div class="file-group-label">{src}</div>
                  {#each group as cfg}
                    <button
                      class="file-item"
                      class:selected={selectedConfig?.path === cfg.path}
                      onclick={() => selectConfig(cfg)}
                      id={`nginx-cfg-${cfg.name}`}
                    >
                      <FileCode size={12} />
                      <span>{cfg.name}</span>
                    </button>
                  {/each}
                {/if}
              {/each}
            {/if}


          </div>

          <!-- Editor Panel -->
          <div class="editor-main">
            {#if selectedConfig}
              <div class="editor-toolbar">
                <span class="editor-filename"><FileCode size={14} />{selectedConfig.name}</span>
                <div class="editor-tools">
                  <Button variant="ghost" class=" btn-sm" onclick={() => (wordWrap = !wordWrap)} id="nginx-word-wrap">
                    {wordWrap ? 'Wrap: On' : 'Wrap: Off'}
                  </Button>
                  {#if hasChanges()}
                    <Button variant="ghost" class=" btn-sm" onclick={() => (showDiff = !showDiff)} id="nginx-show-diff">
                      {showDiff ? 'Hide Diff' : 'Show Diff'}
                    </Button>
                  {/if}
                  <Button
                    variant="primary" class=" btn-sm"
                    onclick={saveConfig}
                    disabled={configSaving || !hasChanges()}
                    id="nginx-save-config"
                  >
                    {#if configSaving}<div class="spinner-sm"></div>{:else}<Save size={12} />{/if}
                    Save
                  </Button>
                </div>
              </div>

              {#if hasChanges()}
                <div class="unsaved-warning">
                  <AlertTriangle size={13} /> Unsaved changes — will run nginx -t before writing
                </div>
              {/if}

              {#if showDiff}
                <div class="diff-view">
                  {#each getDiff() as line}
                    <div class="diff-line diff-{line.type}">
                      <span class="diff-marker">{line.type === 'add' ? '+' : line.type === 'remove' ? '-' : ' '}</span>
                      <span class="diff-text">{line.text}</span>
                    </div>
                  {/each}
                </div>
              {:else}
                <textarea
                  class="code-editor"
                  class:wrap={wordWrap}
                  bind:value={editorContent}
                  spellcheck={false}
                  id="nginx-editor-textarea"
                ></textarea>
              {/if}
            {:else}
              <div class="editor-empty">
                <FileCode size={40} />
                <p>Select a config file to edit</p>
              </div>
            {/if}
          </div>

          <!-- Backups panel -->
          {#if showBackups}
            <div class="backups-panel">
              <div class="editor-sidebar-header">
                <span>Backups</span>
                <Button variant="ghost" class=" btn-sm" onclick={() => (showBackups = false)}>✕</Button>
              </div>
              {#if backupsLoading}
                <div class="center-state"><div class="spinner-sm"></div></div>
              {:else if backups.length === 0}
                <p class="empty-state">No backups yet</p>
              {:else}
                {#each backups as bk}
                  <div class="backup-item">
                    <div class="backup-name">{bk.filename}</div>
                    <div class="backup-ts">{bk.timestamp}</div>
                    <Button class="btn btn-sm -outline" onclick={() => restoreBackup(bk)} id={`nginx-restore-${bk.filename}`}>
                      <ArchiveRestore size={11} /> Restore
                    </Button>
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>

      <!-- ══ WWW FILES ══════════════════════════════════════════════════════ -->
      {:else if activeTab === 'www'}
        <div class="www-layout">
          <!-- Tree -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="www-tree" oncontextmenu={(e) => handleWwwContextMenu(e, null)}>
            <!-- Tree Header Toolbar -->
            <div class="www-tree-header">
              <span class="www-tree-title">
                <FolderOpen size={14} class="text-accent" />
                <span>/var/www Root</span>
              </span>
              <div class="www-tree-header-actions">
                <Button 
                  variant="ghost" 
                  class="btn-xs" 
                  onclick={() => { showNewDirForm = true; newDirParent = '/var/www'; }} 
                  title="New Folder" 
                  id="nginx-new-folder-top"
                >
                  <FolderPlus size={12} />
                </Button>
                <Button 
                  variant="ghost" 
                  class="btn-xs" 
                  onclick={() => uploadFile('/var/www')} 
                  title="Upload File" 
                  id="nginx-upload-top"
                >
                  <Upload size={12} />
                </Button>
                <Button 
                  variant="ghost" 
                  class="btn-xs" 
                  onclick={loadWww} 
                  title="Refresh Files" 
                  id="nginx-refresh-www-top"
                >
                  <RefreshCw size={12} class={wwwLoading ? 'animate-spin-slow' : ''} />
                </Button>
              </div>
            </div>

            {#if showNewDirForm}
              <div class="new-dir-form">
                <input type="text" bind:value={newDirName} placeholder="Folder name..." id="nginx-new-dir-name" />
                <Button variant="primary" class="btn-xs" onclick={createDir} id="nginx-create-dir">Create</Button>
                <Button variant="ghost" class="btn-xs" onclick={() => (showNewDirForm = false)}>✕</Button>
              </div>
            {/if}
            {#if wwwLoading}
              <div class="center-state"><div class="spinner-sm"></div></div>
            {:else if wwwEntries.length === 0}
              <div class="empty-state">/var/www is empty or does not exist</div>
            {:else}
              <div class="tree-list">
                {#each wwwEntries as entry}
                  {@render wwwTreeNode(entry, 0)}
                {/each}
              </div>
            {/if}
          </div>

          <!-- File / Folder Viewer -->
          <div class="www-viewer">
            {#if selectedWwwEntry && !selectedWwwEntry.is_dir}
              <div class="viewer-header">
                <div class="viewer-header-info">
                  <FileText size={15} class="text-accent" />
                  <span class="editor-filename">{selectedWwwEntry.name}</span>
                  <span class="badge badge-muted">{formatSize(selectedWwwEntry.size)}</span>
                  <span class="viewer-path-tag" title="Click to copy path" onclick={() => { navigator.clipboard.writeText(selectedWwwEntry!.path); uiStore.addToast('Copied path', 'info'); }}>
                    {selectedWwwEntry.path}
                  </span>
                </div>
                <div class="header-actions">
                  <Button 
                    variant="outline" 
                    class="btn-sm" 
                    onclick={() => { renamingEntry = selectedWwwEntry; renameValue = selectedWwwEntry!.name; }} 
                    title="Rename File"
                  >
                    <Edit3 size={12} /> Rename
                  </Button>
                  <Button 
                    variant="outline" 
                    class="btn-sm" 
                    onclick={() => { navigator.clipboard.writeText(selectedWwwEntry!.path); uiStore.addToast('Copied path to clipboard', 'info'); }} 
                    title="Copy Path"
                  >
                    <Copy size={12} /> Copy Path
                  </Button>
                  <Button 
                    variant="danger" 
                    class="btn-sm" 
                    onclick={() => confirmDeleteWww(selectedWwwEntry!)} 
                    id="nginx-delete-www-selected"
                  >
                    <Trash2 size={12} /> Delete
                  </Button>
                </div>
              </div>

              {#if wwwFileLoading}
                <div class="center-state"><div class="spinner"></div></div>
              {:else}
                {@const lines = (wwwFileContent || '').split('\n')}
                <div class="file-reader-frame">
                  <!-- Meta Status Toolbar -->
                  <div class="file-reader-meta">
                    <div class="file-reader-meta-info">
                      <span class="file-stat-chip"><FileText size={11} /> {lines.length} lines</span>
                      <span class="file-stat-chip">{formatSize(selectedWwwEntry.size)}</span>
                      <span class="file-stat-chip">{(wwwFileContent || '').length.toLocaleString()} chars</span>
                    </div>
                    <div class="file-reader-tools">
                      <button 
                        type="button" 
                        class="reader-tool-btn" 
                        class:active={wwwWrapLines}
                        onclick={() => wwwWrapLines = !wwwWrapLines}
                        title="Toggle Line Wrap"
                      >
                        <WrapText size={12} />
                        <span>Wrap</span>
                      </button>
                      <button 
                        type="button" 
                        class="reader-tool-btn" 
                        onclick={() => { navigator.clipboard.writeText(wwwFileContent || ''); uiStore.addToast('Copied content to clipboard', 'info'); }}
                        title="Copy full file content"
                      >
                        <Copy size={12} />
                        <span>Copy Content</span>
                      </button>
                    </div>
                  </div>

                  <!-- Code Viewport with Gutter -->
                  <div class="file-code-viewport" class:wrap-lines={wwwWrapLines}>
                    <div class="code-rows-container">
                      {#each lines as line, i}
                        <div class="code-row">
                          <div class="gutter-num" aria-hidden="true">{i + 1}</div>
                          <div class="code-line">{line || '\u00A0'}</div>
                        </div>
                      {/each}
                    </div>
                  </div>
                </div>
              {/if}
            {:else if selectedWwwEntry && selectedWwwEntry.is_dir}
              <div class="viewer-header">
                <div class="viewer-header-info">
                  <FolderOpen size={15} class="text-accent" />
                  <span class="editor-filename">{selectedWwwEntry.name}</span>
                  <span class="badge badge-info">{selectedWwwEntry.children ? selectedWwwEntry.children.length : 0} items</span>
                  <span class="viewer-path-tag">{selectedWwwEntry.path}</span>
                </div>
                <div class="header-actions">
                  <Button 
                    variant="outline" 
                    class="btn-sm" 
                    onclick={() => { showNewDirForm = true; newDirParent = selectedWwwEntry!.path; }} 
                    title="New Subfolder"
                  >
                    <FolderPlus size={12} /> Subfolder
                  </Button>
                  <Button 
                    variant="outline" 
                    class="btn-sm" 
                    onclick={() => uploadFile(selectedWwwEntry!.path)} 
                    title="Upload File Here"
                  >
                    <Upload size={12} /> Upload
                  </Button>
                  <Button 
                    variant="outline" 
                    class="btn-sm" 
                    onclick={() => { renamingEntry = selectedWwwEntry; renameValue = selectedWwwEntry!.name; }} 
                    title="Rename Folder"
                  >
                    <Edit3 size={12} /> Rename
                  </Button>
                  <Button 
                    variant="danger" 
                    class="btn-sm" 
                    onclick={() => confirmDeleteWww(selectedWwwEntry!)} 
                    id="nginx-delete-www-dir"
                  >
                    <Trash2 size={12} /> Delete
                  </Button>
                </div>
              </div>
              <div class="folder-overview-body">
                <div class="folder-quick-shortcuts">
                  <div class="shortcut-card" onclick={() => { showNewDirForm = true; newDirParent = selectedWwwEntry!.path; }}>
                    <FolderPlus size={20} class="text-accent" />
                    <span>Create Subfolder</span>
                  </div>
                  <div class="shortcut-card" onclick={() => uploadFile(selectedWwwEntry!.path)}>
                    <Upload size={20} class="text-accent" />
                    <span>Upload File</span>
                  </div>
                </div>
                <p class="ov-since" style="margin-top:16px;">Right-click any file or directory in the tree to access quick contextual actions.</p>
              </div>
            {:else}
              <div class="editor-empty">
                <FolderOpen size={44} style="color:var(--color-accent); opacity:0.7;" />
                <p style="font-weight:600; font-size:14px; margin-top:8px;">No File Selected</p>
                <p class="ov-since">Select a file from the tree to view contents, or right-click any item to open context actions.</p>
              </div>
            {/if}
          </div>
        </div>

      <!-- ══ LOGS ══════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'logs'}
        <div class="tab-section log-tab-container">
          {#if activeSiteContext}
            <div class="site-context-banner">
              <div class="context-banner-left">
                <Globe size={14} class="text-accent" />
                <span class="context-banner-text">
                  Site Context: <strong>{activeSiteContext.name}</strong>
                </span>
                {#if activeSiteContext.access_log}
                  <span class="site-context-pill dedicated">
                    <FileText size={12} class="context-success-icon" />
                    <span>Dedicated Log File</span>
                  </span>
                {:else}
                  <span class="site-context-pill shared" title="Filtered lines in global log">
                    <AlertTriangle size={12} class="context-warn-icon" />
                    <span>Shared Global Log (Filtered by Site)</span>
                  </span>
                {/if}
              </div>
              <button type="button" class="btn-clear-context" onclick={() => { activeSiteContext = null; logFilter = ''; }} title="Clear site context filter">
                <span>Clear Site Context</span>
                <XCircle size={13} />
              </button>
            </div>
          {/if}

          <!-- Log Filter Bar & View Toggle -->
          <div class="log-control-ribbon">
            <div class="log-status-pills">
              <button 
                class="log-pill-btn" 
                class:active={logStatusFilter === 'all'} 
                onclick={() => logStatusFilter = 'all'}
              >
                All <span class="pill-badge">{logStats.total}</span>
              </button>
              {#if logStats.count2xx > 0}
                <button 
                  class="log-pill-btn status-2xx-pill" 
                  class:active={logStatusFilter === '2xx'} 
                  onclick={() => logStatusFilter = '2xx'}
                >
                  <span class="pill-dot green"></span> 2xx OK <span class="pill-badge">{logStats.count2xx}</span>
                </button>
              {/if}
              {#if logStats.count3xx > 0}
                <button 
                  class="log-pill-btn status-3xx-pill" 
                  class:active={logStatusFilter === '3xx'} 
                  onclick={() => logStatusFilter = '3xx'}
                >
                  <span class="pill-dot blue"></span> 3xx Redirect <span class="pill-badge">{logStats.count3xx}</span>
                </button>
              {/if}
              {#if logStats.count4xx > 0}
                <button 
                  class="log-pill-btn status-4xx-pill" 
                  class:active={logStatusFilter === '4xx'} 
                  onclick={() => logStatusFilter = '4xx'}
                >
                  <span class="pill-dot amber"></span> 4xx Client Err <span class="pill-badge">{logStats.count4xx}</span>
                </button>
              {/if}
              {#if logStats.count5xx > 0}
                <button 
                  class="log-pill-btn status-5xx-pill" 
                  class:active={logStatusFilter === '5xx'} 
                  onclick={() => logStatusFilter = '5xx'}
                >
                  <span class="pill-dot red"></span> 5xx Server Err <span class="pill-badge">{logStats.count5xx}</span>
                </button>
              {/if}
            </div>

            <div class="log-view-toggle">
              <button 
                class="view-mode-btn" 
                class:active={logViewMode === 'structured'} 
                onclick={() => logViewMode = 'structured'}
                title="Structured Human-Readable Stream"
              >
                <ListFilter size={13} /> Structured
              </button>
              <button 
                class="view-mode-btn" 
                class:active={logViewMode === 'raw'} 
                onclick={() => logViewMode = 'raw'}
                title="Raw Monospace Console"
              >
                <TerminalSquare size={13} /> Raw
              </button>
            </div>
          </div>

          {#if logLoading}
            <div class="center-state"><div class="spinner"></div></div>
          {:else if filteredLogEntries.length === 0}
            <div class="editor-empty" style="min-height: 280px;">
              <FileText size={36} style="color: var(--color-text-muted);" />
              <p style="font-size: 14px; font-weight: 600; margin-top: 8px;">No log entries found</p>
              <p class="ov-since">Try changing your search query or status filter.</p>
            </div>
          {:else if logViewMode === 'structured'}
            <div class="structured-log-stream">
              {#each filteredLogEntries as entry, idx}
                {#if entry.type === 'access'}
                  <div 
                    class="log-row-card access-card"
                    class:expanded={expandedLogIndex === idx}
                    onclick={() => expandedLogIndex = expandedLogIndex === idx ? null : idx}
                    role="button"
                    tabindex="0"
                    onkeydown={(e) => { if (e.key === 'Enter') expandedLogIndex = expandedLogIndex === idx ? null : idx; }}
                  >
                    <div class="log-row-main">
                      <!-- Status Code Badge -->
                      <span class="status-code-badge status-{entry.statusCategory}">
                        {entry.status} {entry.statusText}
                      </span>

                      <!-- HTTP Method -->
                      <span class="method-badge method-{entry.method?.toLowerCase()}">
                        {entry.method}
                      </span>

                      <!-- Path -->
                      <span class="log-path" title={entry.path}>
                        {entry.path}
                      </span>

                      <!-- IP & Client -->
                      <div class="log-client-group">
                        <span class="log-ip">{entry.ip}</span>
                        {#if entry.clientBrowser}
                          <span class="client-badge">{entry.clientBrowser}</span>
                        {/if}
                      </div>

                      <!-- Size -->
                      <span class="log-size">{entry.formattedSize}</span>

                      <!-- Time -->
                      <span class="log-time" title={entry.timestamp}>{entry.formattedTime}</span>

                      <ChevronDown size={14} class="log-expand-icon" style={expandedLogIndex === idx ? 'transform: rotate(180deg);' : ''} />
                    </div>

                    <!-- Expanded Detailed View -->
                    {#if expandedLogIndex === idx}
                      <div class="log-row-expanded" onclick={(e) => e.stopPropagation()}>
                        <div class="expanded-grid">
                          <div class="exp-item">
                            <span class="exp-label">Request URI:</span>
                            <span class="exp-value font-mono">{entry.path}</span>
                          </div>
                          <div class="exp-item">
                            <span class="exp-label">Remote Client IP:</span>
                            <span class="exp-value font-mono">{entry.ip}</span>
                          </div>
                          {#if entry.referer}
                            <div class="exp-item full-width">
                              <span class="exp-label">Referer:</span>
                              <span class="exp-value">{entry.referer}</span>
                            </div>
                          {/if}
                          {#if entry.userAgent}
                            <div class="exp-item full-width">
                              <span class="exp-label">User-Agent:</span>
                              <span class="exp-value">{entry.userAgent}</span>
                            </div>
                          {/if}
                          <div class="exp-item full-width raw-line-box">
                            <span class="exp-label">Raw Entry:</span>
                            <div class="raw-copy-row">
                              <code>{entry.raw}</code>
                              <button class="raw-copy-btn" onclick={() => copyLogLine(entry.raw)} title="Copy raw line">
                                <Copy size={12} />
                              </button>
                            </div>
                          </div>
                        </div>
                      </div>
                    {/if}
                  </div>
                {:else if entry.type === 'error'}
                  <div 
                    class="log-row-card error-card"
                    class:expanded={expandedLogIndex === idx}
                    onclick={() => expandedLogIndex = expandedLogIndex === idx ? null : idx}
                    role="button"
                    tabindex="0"
                    onkeydown={(e) => { if (e.key === 'Enter') expandedLogIndex = expandedLogIndex === idx ? null : idx; }}
                  >
                    <div class="log-row-main">
                      <span class="status-code-badge status-5xx">
                        [{entry.logLevel?.toUpperCase()}]
                      </span>
                      <span class="log-error-msg" title={entry.errorMessage}>
                        {entry.errorMessage}
                      </span>
                      {#if entry.ip}
                        <span class="log-ip">{entry.ip}</span>
                      {/if}
                      <span class="log-time" title={entry.timestamp}>{entry.formattedTime}</span>
                      <ChevronDown size={14} class="log-expand-icon" style={expandedLogIndex === idx ? 'transform: rotate(180deg);' : ''} />
                    </div>

                    {#if expandedLogIndex === idx}
                      <div class="log-row-expanded" onclick={(e) => e.stopPropagation()}>
                        <div class="raw-copy-row">
                          <code>{entry.raw}</code>
                          <button class="raw-copy-btn" onclick={() => copyLogLine(entry.raw)} title="Copy raw line">
                            <Copy size={12} />
                          </button>
                        </div>
                      </div>
                    {/if}
                  </div>
                {:else}
                  <div class="log-row-card generic-card">
                    <div class="raw-copy-row">
                      <code>{entry.raw}</code>
                      <button class="raw-copy-btn" onclick={() => copyLogLine(entry.raw)} title="Copy raw line">
                        <Copy size={12} />
                      </button>
                    </div>
                  </div>
                {/if}
              {/each}
            </div>
          {:else}
            <!-- Raw Mode -->
            <div class="raw-terminal-view">
              <div class="raw-terminal-header">
                <span>{selectedLog} — {filteredLogEntries.length} entries</span>
                <Button variant="outline" class="btn-sm" onclick={() => copyLogLine(logContent)}>
                  <Copy size={12} /> Copy All
                </Button>
              </div>
              <pre class="log-view raw-code-body">{logContent || '(empty)'}</pre>
            </div>
          {/if}
        </div>

      <!-- ══ ANALYTICS ══════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'analytics'}
        <div class="tab-section analytics-section" style="display:flex; flex-direction:column; gap:20px;">
          <!-- Analytics Top Header / Filter Bar -->
          <div class="analytics-header-bar">
            <div class="analytics-header-left">
              <div class="analytics-title-wrap">
                <BarChart2 size={16} class="text-accent" />
                <span class="analytics-title">Nginx Traffic &amp; Request Telemetry</span>
              </div>
              <span class="analytics-log-source-tag">
                <FileText size={12} /> {analyticsLogFile}
              </span>
            </div>

            <div class="analytics-header-right">
              <div style="display:flex; align-items:center; gap:8px;">
                <span style="font-size:11px; color:var(--color-text-muted); font-weight:600;">Log Source:</span>
                <Select bind:value={analyticsLogFile} onchange={() => loadAnalytics(true)} style="height:28px; width:260px;">
                  {#if analyticsLogFile && !logFiles.includes(analyticsLogFile)}
                    <option value={analyticsLogFile}>{analyticsLogFile.split('/').pop()} • Custom</option>
                  {/if}
                  {#if logFiles.length > 0}
                    {#each logFiles as lf}
                      {@const name = lf.split('/').pop() || lf}
                      {@const isRotated = name.includes('-') || name.endsWith('.gz') || name.includes('.1')}
                      <option value={lf}>{name} {isRotated ? '• Archive' : '• Active'}</option>
                    {/each}
                  {:else}
                    <option value="/var/log/nginx/access.log">access.log • Active</option>
                    <option value="/var/log/nginx/error.log">error.log • Active</option>
                  {/if}
                </Select>
              </div>

              <span class="analytics-sample-badge">
                <Radio size={11} class="text-accent pulsing" />
                <span>{analyticsLogFile.includes('error') ? 'Error Event Diagnostics' : 'Sample: 15,000 requests'}</span>
              </span>

              <Button variant="outline" class="btn-sm" onclick={() => loadAnalytics(true)} disabled={analyticsLoading} title="Refresh analytics data">
                <RefreshCw size={12} class={analyticsLoading ? 'animate-spin' : ''} />
                <span>Refresh</span>
              </Button>
            </div>
          </div>

          {#if activeSiteContext}
            <div class="site-context-banner">
              <div class="context-banner-left">
                <Globe size={14} class="text-accent" />
                <span class="context-banner-text">
                  Site Context: <strong>{activeSiteContext.name}</strong>
                </span>
                {#if activeSiteContext.access_log}
                  <span class="site-context-pill dedicated">
                    <FileText size={12} class="context-success-icon" />
                    <span>Dedicated Log: <code>{activeSiteContext.access_log}</code></span>
                  </span>
                {:else}
                  <span class="site-context-pill shared" title="No custom access_log directive defined inside {activeSiteContext.name}; Nginx directs all traffic to the default global access.log">
                    <AlertTriangle size={12} class="context-warn-icon" />
                    <span>Shared Global Log (No custom access_log in config)</span>
                  </span>
                {/if}
              </div>
              <button type="button" class="btn-clear-context" onclick={() => activeSiteContext = null} title="Clear site context">
                <span>Clear Site Context</span>
                <XCircle size={13} />
              </button>
            </div>
          {/if}

          {#if analyticsLoading}
            <div class="center-state" style="padding:60px 0;"><div class="spinner"></div></div>
          {:else if analyticsData}
            <!-- ─── 5 Large Hero KPI Cards ──────────────────────────────── -->
            <div class="analytics-kpi-grid">
              <!-- KPI 1: Total Requests / Error Events -->
              <div class="analytics-kpi-card card">
                <div class="analytics-kpi-header">
                  <span class="analytics-kpi-label">{analyticsLogFile.includes('error') ? 'Total Error Events' : 'Total Requests'}</span>
                  <div class="analytics-kpi-icon-wrap cyan">
                    <Activity size={18} />
                  </div>
                </div>
                <div class="analytics-kpi-value cyan">{analyticsData.total_requests.toLocaleString()}</div>
                <div class="analytics-kpi-footer">
                  <span class="analytics-kpi-subtext">{analyticsLogFile.includes('error') ? 'Error entries in log stream' : 'Requests in analyzed log stream'}</span>
                  <span class="analytics-kpi-tag cyan">{analyticsLogFile.includes('error') ? 'Errors' : 'Live'}</span>
                </div>
              </div>

              <!-- KPI 2: Unique Visitors / Affected IPs -->
              <div class="analytics-kpi-card card">
                <div class="analytics-kpi-header">
                  <span class="analytics-kpi-label">{analyticsLogFile.includes('error') ? 'Affected Client IPs' : 'Unique Client IPs'}</span>
                  <div class="analytics-kpi-icon-wrap purple">
                    <Globe size={18} />
                  </div>
                </div>
                <div class="analytics-kpi-value purple">{analyticsData.unique_ips.toLocaleString()}</div>
                <div class="analytics-kpi-footer">
                  <span class="analytics-kpi-subtext">
                    {((analyticsData.unique_ips / (analyticsData.total_requests || 1)) * 100).toFixed(1)}% unique visitor ratio
                  </span>
                  <span class="analytics-kpi-tag purple">Clients</span>
                </div>
              </div>

              <!-- KPI 3: Bandwidth Sent -->
              <div class="analytics-kpi-card card">
                <div class="analytics-kpi-header">
                  <span class="analytics-kpi-label">Data Transferred</span>
                  <div class="analytics-kpi-icon-wrap blue">
                    <Download size={18} />
                  </div>
                </div>
                <div class="analytics-kpi-value blue">{analyticsLogFile.includes('error') ? 'Diagnostic Log' : formatBytes(analyticsData.total_bytes_sent)}</div>
                <div class="analytics-kpi-footer">
                  <span class="analytics-kpi-subtext">{analyticsLogFile.includes('error') ? 'Error diagnostic stream' : 'Total outbound response payload'}</span>
                  <span class="analytics-kpi-tag blue">{analyticsLogFile.includes('error') ? 'Diagnostics' : 'Bandwidth'}</span>
                </div>
              </div>

              <!-- KPI 4: Success Rate (2xx/3xx) -->
              <div class="analytics-kpi-card card">
                <div class="analytics-kpi-header">
                  <span class="analytics-kpi-label">Success Rate</span>
                  <div class="analytics-kpi-icon-wrap green">
                    <CheckCircle size={18} />
                  </div>
                </div>
                <div class="analytics-kpi-value green">{analyticsData.success_rate.toFixed(1)}%</div>
                <div class="analytics-kpi-footer">
                  <span class="analytics-kpi-subtext">{(analyticsData.status_2xx + analyticsData.status_3xx).toLocaleString()} successful hits</span>
                  <span class="analytics-kpi-tag green">Healthy</span>
                </div>
              </div>

              <!-- KPI 5: Error Rate (4xx/5xx) -->
              <div class="analytics-kpi-card card">
                <div class="analytics-kpi-header">
                  <span class="analytics-kpi-label">Error Rate</span>
                  <div class="analytics-kpi-icon-wrap amber">
                    <AlertTriangle size={18} />
                  </div>
                </div>
                <div class="analytics-kpi-value {analyticsData.error_rate > 5 ? 'red' : 'amber'}">{analyticsData.error_rate.toFixed(1)}%</div>
                <div class="analytics-kpi-footer">
                  <span class="analytics-kpi-subtext">{(analyticsData.status_4xx + analyticsData.status_5xx).toLocaleString()} 4xx/5xx responses</span>
                  <span class="analytics-kpi-tag {analyticsData.error_rate > 5 ? 'red' : 'amber'}">
                    {analyticsData.error_rate > 5 ? 'Elevated' : 'Normal'}
                  </span>
                </div>
              </div>
            </div>

            <!-- ─── HTTP Status Code Breakdown ──────────────────────────── -->
            <div class="card analytics-status-card">
              <div class="analytics-card-header">
                <div>
                  <h3 class="analytics-card-title">HTTP Response Code Health</h3>
                  <p class="analytics-card-subtitle">Distribution of HTTP status codes across all processed server transactions</p>
                </div>
                <div class="analytics-status-counts-summary">
                  <span class="status-summary-item green">● 2xx: {analyticsData.status_2xx.toLocaleString()}</span>
                  <span class="status-summary-item blue">● 3xx: {analyticsData.status_3xx.toLocaleString()}</span>
                  <span class="status-summary-item amber">● 4xx: {analyticsData.status_4xx.toLocaleString()}</span>
                  <span class="status-summary-item red">● 5xx: {analyticsData.status_5xx.toLocaleString()}</span>
                </div>
              </div>

              <!-- Multi-color Segmented Status Bar -->
              <div class="analytics-status-bar">
                {#if analyticsData.total_requests > 0}
                  <div class="status-seg seg-2xx" style="width:{(analyticsData.status_2xx / analyticsData.total_requests) * 100}%;" title="2xx Success: {analyticsData.status_2xx}"></div>
                  <div class="status-seg seg-3xx" style="width:{(analyticsData.status_3xx / analyticsData.total_requests) * 100}%;" title="3xx Redirect: {analyticsData.status_3xx}"></div>
                  <div class="status-seg seg-4xx" style="width:{(analyticsData.status_4xx / analyticsData.total_requests) * 100}%;" title="4xx Client Error: {analyticsData.status_4xx}"></div>
                  <div class="status-seg seg-5xx" style="width:{(analyticsData.status_5xx / analyticsData.total_requests) * 100}%;" title="5xx Server Error: {analyticsData.status_5xx}"></div>
                {:else}
                  <div class="status-seg-empty">No status data recorded</div>
                {/if}
              </div>

              <!-- Status Pills Grid -->
              <div class="analytics-status-pills-grid">
                <div class="status-pill-card green">
                  <div class="status-pill-top">
                    <span class="status-pill-dot green"></span>
                    <span class="status-pill-name">2xx Success</span>
                    <span class="status-pill-pct">{(analyticsData.status_2xx / (analyticsData.total_requests || 1) * 100).toFixed(1)}%</span>
                  </div>
                  <div class="status-pill-count">{analyticsData.status_2xx.toLocaleString()} requests</div>
                </div>

                <div class="status-pill-card blue">
                  <div class="status-pill-top">
                    <span class="status-pill-dot blue"></span>
                    <span class="status-pill-name">3xx Redirection</span>
                    <span class="status-pill-pct">{(analyticsData.status_3xx / (analyticsData.total_requests || 1) * 100).toFixed(1)}%</span>
                  </div>
                  <div class="status-pill-count">{analyticsData.status_3xx.toLocaleString()} requests</div>
                </div>

                <div class="status-pill-card amber">
                  <div class="status-pill-top">
                    <span class="status-pill-dot amber"></span>
                    <span class="status-pill-name">4xx Client Error</span>
                    <span class="status-pill-pct">{(analyticsData.status_4xx / (analyticsData.total_requests || 1) * 100).toFixed(1)}%</span>
                  </div>
                  <div class="status-pill-count">{analyticsData.status_4xx.toLocaleString()} requests</div>
                </div>

                <div class="status-pill-card red">
                  <div class="status-pill-top">
                    <span class="status-pill-dot red"></span>
                    <span class="status-pill-name">5xx Server Error</span>
                    <span class="status-pill-pct">{(analyticsData.status_5xx / (analyticsData.total_requests || 1) * 100).toFixed(1)}%</span>
                  </div>
                  <div class="status-pill-count">{analyticsData.status_5xx.toLocaleString()} requests</div>
                </div>
              </div>
            </div>

            <!-- ─── 24-Hour Traffic Activity Histogram ───────────────────── -->
            <div class="card analytics-hourly-card">
              <div class="analytics-card-header">
                <div>
                  <h3 class="analytics-card-title">24-Hour Request Activity Timeline</h3>
                  <p class="analytics-card-subtitle">Hourly traffic volume distribution across the parsed sample window</p>
                </div>
                <div class="analytics-hourly-peak-badge">
                  <TrendingUp size={13} class="text-accent" />
                  <span>24-Hour Profile</span>
                </div>
              </div>

              {#if analyticsData.hourly_traffic && analyticsData.hourly_traffic.length > 0}
                {@const maxHourly = Math.max(...analyticsData.hourly_traffic.map(h => h[1]), 1)}
                <div class="analytics-hourly-chart">
                  {#each analyticsData.hourly_traffic as [hour, count]}
                    {@const heightPct = Math.max((count / maxHourly) * 100, 4)}
                    {@const pctShare = ((count / (analyticsData.total_requests || 1)) * 100).toFixed(1)}
                    <div class="analytics-hourly-col">
                      <div class="analytics-hourly-bar-wrap">
                        <div class="analytics-hourly-bar" style="height: {heightPct}%;">
                          <div class="hourly-bar-tooltip">
                            <span class="tooltip-time">{hour}</span>
                            <span class="tooltip-val">{count.toLocaleString()} reqs</span>
                            <span class="tooltip-pct">({pctShare}%)</span>
                          </div>
                        </div>
                      </div>
                      <span class="analytics-hourly-label">{hour.substring(0, 2)}h</span>
                    </div>
                  {/each}
                </div>
              {:else}
                <div class="empty-state">No hourly traffic data available</div>
              {/if}
            </div>

            <!-- ─── 2x2 Rich Sysadmin Telemetry Grid ─────────────────────── -->
            <div class="analytics-details-grid">
              <!-- Table 1: Top Client IPs -->
              <div class="card analytics-table-card">
                <div class="analytics-card-header">
                  <div>
                    <h3 class="analytics-card-title">Top Client IP Addresses</h3>
                    <p class="analytics-card-subtitle">Highest volume origin addresses and client endpoints</p>
                  </div>
                  <span class="badge badge-outline">{analyticsData.top_ips.length} IPs</span>
                </div>

                <div class="analytics-table-wrap">
                  <table class="table analytics-table">
                    <thead>
                      <tr>
                        <th style="width:36px;">#</th>
                        <th>Client IP</th>
                        <th style="width:130px;">Traffic Share</th>
                        <th style="text-align:right; width:70px;">Hits</th>
                        <th style="width:36px;"></th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each analyticsData.top_ips as [ip, count], idx}
                        {@const pct = (count / (analyticsData.total_requests || 1)) * 100}
                        <tr>
                          <td class="analytics-rank">{idx + 1}</td>
                          <td>
                            <div class="analytics-ip-cell">
                              <span class="analytics-ip-addr">{ip}</span>
                              {#if ip.startsWith('192.168.') || ip.startsWith('10.') || ip.startsWith('172.16.') || ip === '127.0.0.1'}
                                <span class="analytics-sub-badge">LAN</span>
                              {:else}
                                <span class="analytics-sub-badge public">WAN</span>
                              {/if}
                            </div>
                          </td>
                          <td>
                            <div class="analytics-prog-wrap">
                              <div class="analytics-prog-bar cyan" style="width: {pct}%;"></div>
                              <span class="analytics-prog-pct">{pct.toFixed(1)}%</span>
                            </div>
                          </td>
                          <td style="text-align:right; font-family:var(--font-mono); font-weight:600;">{count.toLocaleString()}</td>
                          <td>
                            <button 
                              class="analytics-copy-btn" 
                              onclick={() => { navigator.clipboard.writeText(ip); uiStore.addToast(`Copied ${ip}`, 'info'); }} 
                              title="Copy IP Address"
                            >
                              <Copy size={11} />
                            </button>
                          </td>
                        </tr>
                      {:else}
                        <tr><td colspan="5" class="empty-state">No IP data recorded</td></tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              </div>

              <!-- Table 2: Top Requested URL Endpoints -->
              <div class="card analytics-table-card">
                <div class="analytics-card-header">
                  <div>
                    <h3 class="analytics-card-title">Top Requested Endpoints</h3>
                    <p class="analytics-card-subtitle">Most frequently hit URIs and application routes</p>
                  </div>
                  <span class="badge badge-outline">{analyticsData.top_paths.length} Routes</span>
                </div>

                <div class="analytics-table-wrap">
                  <table class="table analytics-table">
                    <thead>
                      <tr>
                        <th style="width:36px;">#</th>
                        <th>Path URI</th>
                        <th style="width:130px;">Traffic Share</th>
                        <th style="text-align:right; width:70px;">Hits</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each analyticsData.top_paths as [path, count], idx}
                        {@const pct = (count / (analyticsData.total_requests || 1)) * 100}
                        <tr>
                          <td class="analytics-rank">{idx + 1}</td>
                          <td>
                            <span class="analytics-path-text" title={path}>{path}</span>
                          </td>
                          <td>
                            <div class="analytics-prog-wrap">
                              <div class="analytics-prog-bar purple" style="width: {pct}%;"></div>
                              <span class="analytics-prog-pct">{pct.toFixed(1)}%</span>
                            </div>
                          </td>
                          <td style="text-align:right; font-family:var(--font-mono); font-weight:600;">{count.toLocaleString()}</td>
                        </tr>
                      {:else}
                        <tr><td colspan="4" class="empty-state">No path data recorded</td></tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              </div>

              <!-- Card 3: HTTP Methods & User Agents -->
              <div class="card analytics-table-card">
                <div class="analytics-card-header">
                  <div>
                    <h3 class="analytics-card-title">HTTP Methods &amp; Protocol</h3>
                    <p class="analytics-card-subtitle">Distribution of HTTP verbs in client requests</p>
                  </div>
                </div>

                <div class="analytics-methods-container">
                  {#each analyticsData.top_methods as [method, count]}
                    {@const pct = (count / (analyticsData.total_requests || 1)) * 100}
                    <div class="analytics-method-row">
                      <div class="analytics-method-badge-wrap">
                        <span class="analytics-method-tag method-{method.toLowerCase()}">{method}</span>
                        <span class="analytics-method-count">{count.toLocaleString()} requests</span>
                      </div>
                      <div class="analytics-prog-wrap" style="flex:1;">
                        <div class="analytics-prog-bar green" style="width: {pct}%;"></div>
                        <span class="analytics-prog-pct">{pct.toFixed(1)}%</span>
                      </div>
                    </div>
                  {/each}
                </div>

                <div class="analytics-card-header" style="margin-top:20px; padding-top:16px; border-top:1px solid var(--color-border);">
                  <div>
                    <h3 class="analytics-card-title">Traffic Sources / Referrers</h3>
                    <p class="analytics-card-subtitle">Incoming referrer domains</p>
                  </div>
                </div>
                <div class="analytics-referrers-list">
                  {#each analyticsData.top_referrers as [ref, count]}
                    <div class="analytics-ref-row">
                      <span class="analytics-ref-name" title={ref}>{ref}</span>
                      <span class="analytics-ref-count">{count.toLocaleString()} hits</span>
                    </div>
                  {/each}
                </div>
              </div>

              <!-- Card 4: User Agents & Clients / Bots -->
              <div class="card analytics-table-card">
                <div class="analytics-card-header">
                  <div>
                    <h3 class="analytics-card-title">User Agents &amp; Client Software</h3>
                    <p class="analytics-card-subtitle">Browsers, automated scripts, CLI tools, and web crawlers</p>
                  </div>
                </div>

                <div class="analytics-ua-list">
                  {#each analyticsData.top_user_agents as [ua, count]}
                    {@const pct = (count / (analyticsData.total_requests || 1)) * 100}
                    <div class="analytics-ua-row">
                      <div class="analytics-ua-info">
                        <span class="analytics-ua-name">{ua}</span>
                        <span class="analytics-ua-count">{count.toLocaleString()} hits</span>
                      </div>
                      <div class="analytics-prog-wrap">
                        <div class="analytics-prog-bar blue" style="width: {pct}%;"></div>
                        <span class="analytics-prog-pct">{pct.toFixed(1)}%</span>
                      </div>
                    </div>
                  {:else}
                    <div class="empty-state">No User Agent data available</div>
                  {/each}
                </div>
              </div>
            </div>
          {/if}
        </div>

      <!-- ══ SSL ════════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'ssl'}
        <div class="tab-section">
          {#if !hasCertbot}
            <div class="card" style="display:flex; flex-direction:column; gap:14px; padding:32px 24px; align-items:center; text-align:center; max-width:620px; margin:32px auto;">
              <div style="width:52px; height:52px; border-radius:14px; background:rgba(0,218,243,0.1); border:1px solid rgba(0,218,243,0.25); display:flex; align-items:center; justify-content:center; color:var(--color-accent);">
                <Lock size={26} />
              </div>
              <div>
                <h3 style="margin:0 0 6px; font-size:16px; font-weight:600; color:var(--color-text-primary);">Certbot Not Installed</h3>
                <p style="margin:0; font-size:13px; color:var(--color-text-secondary); line-height:1.5;">
                  Certbot automates obtaining and renewing Let's Encrypt SSL/TLS certificates and configuring Nginx HTTPS.
                </p>
              </div>
              <div style="display:flex; align-items:center; gap:10px; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:8px; padding:10px 16px; width:100%; justify-content:space-between;">
                <code style="font-family:var(--font-mono); font-size:12.5px; color:var(--color-accent);">sudo dnf install certbot python3-certbot-nginx</code>
                <button 
                  type="button" 
                  class="inspect-copy-icon-btn" 
                  onclick={() => { navigator.clipboard.writeText('sudo dnf install certbot python3-certbot-nginx'); uiStore.addToast('Copied installation command', 'info'); }}
                  title="Copy installation command"
                >
                  <Copy size={14} />
                </button>
              </div>
              <p style="margin:0; font-size:11.5px; color:var(--color-text-muted);">
                For Debian / Ubuntu systems, use <code>sudo apt install certbot python3-certbot-nginx</code>
              </p>
            </div>
          {:else}
            <div class="ssl-notice">
              <AlertTriangle size={14} />
              To issue a <strong>new</strong> certificate, click the <strong>Get SSL</strong> button in the Sites tab or run:
              <code>sudo certbot --nginx -d yourdomain.com</code>
            </div>
            {#if sslLoading}
              <div class="center-state"><div class="spinner"></div></div>
            {:else if sslCerts.length === 0}
              <div class="empty-state">No certificates found in /etc/letsencrypt/live/</div>
            {:else}
            <div class="ssl-grid">
              {#each sslCerts as cert}
                <div class="card ssl-card">
                  <div class="ssl-card-header">
                    <span class="ssl-domain"><Lock size={14} />{cert.domain}</span>
                    <span class="badge {cert.status === 'valid' ? 'badge-success' : cert.status === 'expiring' ? 'badge-warning' : 'badge-error'}">
                      {cert.status === 'valid' ? '✓ Valid' : cert.status === 'expiring' ? '⚠ Expiring' : '✗ Expired'}
                    </span>
                  </div>
                  <div class="ssl-meta">
                    <div>Expires: <strong>{cert.expiry}</strong></div>
                    <div class="ssl-days {cert.status}">
                      {cert.days_until_expiry >= 0 ? `${cert.days_until_expiry} days left` : `Expired ${Math.abs(cert.days_until_expiry)} days ago`}
                    </div>
                    <div class="ssl-expiry-bar">
                      <div class="ssl-expiry-fill {cert.status}" style="width: {Math.min(100, Math.max(0, (cert.days_until_expiry / 90) * 100))}%"></div>
                    </div>
                  </div>
                  <div class="ssl-actions">
                    <Button
                      variant="outline" class=" btn-sm"
                      onclick={() => renewCert(cert.domain)}
                      disabled={renewingCert === cert.domain}
                      id={`nginx-renew-${cert.domain}`}
                    >
                      {#if renewingCert === cert.domain}<div class="spinner-sm"></div>{:else}<RefreshCw size={12} />{/if}
                      Renew
                    </Button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- ─── WWW Tree Node Snippet ─────────────────────────────────────────────── -->
{#snippet wwwTreeNode(entry: WwwEntry, depth: number)}
  <div class="tree-node" style="padding-left: {depth * 14 + 6}px">
    <button
      class="tree-item {selectedWwwEntry?.path === entry.path ? 'tree-selected' : ''}"
      onclick={() => viewWwwFile(entry)}
      oncontextmenu={(e) => handleWwwContextMenu(e, entry)}
      id={`nginx-www-${entry.name}-${depth}`}
      title="Click to view • Right-click for options"
    >
      {#if entry.is_dir}
        <span class="tree-arrow">
          {#if expandedPaths.has(entry.path)}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
        </span>
        <FolderOpen size={14} class="tree-icon folder" />
      {:else}
        <span class="tree-arrow-spacer"></span>
        <FileText size={14} class="tree-icon file" />
      {/if}

      {#if renamingEntry?.path === entry.path}
        <input
          class="rename-input"
          bind:value={renameValue}
          onkeydown={(e) => { if (e.key === 'Enter') doRename(); if (e.key === 'Escape') renamingEntry = null; }}
          onclick={(e) => e.stopPropagation()}
          id={`nginx-rename-${entry.name}`}
          autofocus
        />
      {:else}
        <span class="tree-name">{entry.name}</span>
      {/if}

      {#if !entry.is_dir}
        <span class="tree-size">{formatSize(entry.size)}</span>
      {/if}
    </button>
  </div>
  {#if entry.is_dir && expandedPaths.has(entry.path)}
    {#each entry.children as child}
      {@render wwwTreeNode(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

<!-- ─── Sites Right-Click Context Menu ────────────────────────────────────── -->
{#if siteContextMenu.site}
  <ContextMenu
    bind:isOpen={siteContextMenu.visible}
    x={siteContextMenu.x}
    y={siteContextMenu.y}
    title={siteContextMenu.site.name}
    subtitle={siteContextMenu.site.path}
    badge={{
      text: siteContextMenu.site.enabled ? 'ACTIVE SITE' : 'DISABLED',
      variant: siteContextMenu.site.enabled ? 'success' : 'muted'
    }}
    icon={Globe}
    items={[
      {
        label: 'Quick Inspect Site',
        icon: Eye,
        action: () => openSiteInspector(siteContextMenu.site!)
      },
      {
        label: 'Edit in Configuration Editor',
        icon: FileCode,
        action: () => openSiteInEditor(siteContextMenu.site!)
      },
      {
        label: 'Test Config Syntax (nginx -t)',
        icon: CheckCircle,
        action: () => runTest()
      },
      {
        label: 'Safe Reload Nginx',
        icon: RefreshCw,
        action: () => testAndReload()
      },
      { divider: true, label: '' },
      {
        label: 'View Logs & Analytics',
        icon: BarChart2,
        action: () => jumpToSiteLogs(siteContextMenu.site!, 'analytics')
      },
      {
        label: 'Clone Virtual Host Configuration',
        icon: Copy,
        action: () => openCloneModal(siteContextMenu.site!)
      },
      ...(!siteContextMenu.site.has_ssl ? [{
        label: "Issue Let's Encrypt SSL",
        icon: Sparkles,
        color: 'var(--color-accent)',
        action: () => openQuickSsl(siteContextMenu.site!)
      }] : []),
      ...(siteContextMenu.site.source === 'sites-available' ? [{
        label: siteContextMenu.site.enabled ? 'Disable Virtual Host' : 'Enable Virtual Host',
        icon: siteContextMenu.site.enabled ? EyeOff : Eye,
        disabled: toggleLoadingFor === siteContextMenu.site.name,
        action: () => toggleSite(siteContextMenu.site!)
      }] : []),
      { divider: true, label: '' },
      {
        label: 'Copy Config Path',
        icon: Copy,
        action: () => {
          navigator.clipboard.writeText(siteContextMenu.site!.path);
          uiStore.addToast('Copied site path to clipboard', 'info');
        }
      },
      {
        label: 'Delete Virtual Host',
        icon: Trash2,
        danger: true,
        action: () => confirmDeleteSite(siteContextMenu.site!)
      }
    ]}
  />
{/if}

<!-- ─── WWW Right-Click Context Menu ──────────────────────────────────────── -->
{#if wwwContextMenu.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div 
    class="www-context-menu" 
    style="top: {wwwContextMenu.y}px; left: {wwwContextMenu.x}px;"
    onclick={(e) => e.stopPropagation()}
  >
    {#if wwwContextMenu.entry}
      <div class="context-menu-header">
        {#if wwwContextMenu.entry.is_dir}
          <FolderOpen size={13} class="text-accent" />
        {:else}
          <FileText size={13} class="text-accent" />
        {/if}
        <span class="context-menu-title" title={wwwContextMenu.entry.name}>{wwwContextMenu.entry.name}</span>
      </div>
      <div class="context-menu-divider"></div>

      {#if !wwwContextMenu.entry.is_dir}
        <!-- File Actions -->
        <button class="context-menu-item" onclick={() => { viewWwwFile(wwwContextMenu.entry!); closeWwwContextMenu(); }}>
          <Eye size={13} />
          <span>View / Preview</span>
        </button>
        <button class="context-menu-item" onclick={() => { renamingEntry = wwwContextMenu.entry; renameValue = wwwContextMenu.entry!.name; closeWwwContextMenu(); }}>
          <Edit3 size={13} />
          <span>Rename File</span>
        </button>
        <button class="context-menu-item" onclick={() => { navigator.clipboard.writeText(wwwContextMenu.entry!.path); uiStore.addToast('Copied full path to clipboard', 'info'); closeWwwContextMenu(); }}>
          <Copy size={13} />
          <span>Copy Full Path</span>
        </button>
        <div class="context-menu-divider"></div>
        <button class="context-menu-item danger" onclick={() => { confirmDeleteWww(wwwContextMenu.entry!); closeWwwContextMenu(); }}>
          <Trash2 size={13} />
          <span>Delete File</span>
        </button>
      {:else}
        <!-- Directory Actions -->
        <button class="context-menu-item" onclick={() => { showNewDirForm = true; newDirParent = wwwContextMenu.entry!.path; closeWwwContextMenu(); }}>
          <FolderPlus size={13} />
          <span>New Subfolder</span>
        </button>
        <button class="context-menu-item" onclick={() => { uploadFile(wwwContextMenu.entry!.path); closeWwwContextMenu(); }}>
          <Upload size={13} />
          <span>Upload File Here</span>
        </button>
        <button class="context-menu-item" onclick={() => { renamingEntry = wwwContextMenu.entry; renameValue = wwwContextMenu.entry!.name; closeWwwContextMenu(); }}>
          <Edit3 size={13} />
          <span>Rename Folder</span>
        </button>
        <button class="context-menu-item" onclick={() => { navigator.clipboard.writeText(wwwContextMenu.entry!.path); uiStore.addToast('Copied folder path to clipboard', 'info'); closeWwwContextMenu(); }}>
          <Copy size={13} />
          <span>Copy Folder Path</span>
        </button>
        <div class="context-menu-divider"></div>
        <button class="context-menu-item danger" onclick={() => { confirmDeleteWww(wwwContextMenu.entry!); closeWwwContextMenu(); }}>
          <Trash2 size={13} />
          <span>Delete Folder</span>
        </button>
      {/if}
    {:else}
      <!-- Empty Tree Area Actions -->
      <div class="context-menu-header">
        <FolderOpen size={13} class="text-accent" />
        <span class="context-menu-title">/var/www Root</span>
      </div>
      <div class="context-menu-divider"></div>
      <button class="context-menu-item" onclick={() => { showNewDirForm = true; newDirParent = '/var/www'; closeWwwContextMenu(); }}>
        <FolderPlus size={13} />
        <span>New Folder at Root</span>
      </button>
      <button class="context-menu-item" onclick={() => { uploadFile('/var/www'); closeWwwContextMenu(); }}>
        <Upload size={13} />
        <span>Upload File to /var/www</span>
      </button>
      <button class="context-menu-item" onclick={() => { loadWww(); closeWwwContextMenu(); }}>
        <RefreshCw size={13} />
        <span>Refresh File Tree</span>
      </button>
    {/if}
  </div>
{/if}

<!-- ─── Modals ─────────────────────────────────────────────────────────────── -->

<!-- nginx -t result modal -->
{#if showTestModal && modalTestResult}
  <div use:portal class="modal-backdrop"
    onclick={() => (showTestModal = false)}
    onkeydown={(e) => e.key === 'Escape' && (showTestModal = false)}
    role="dialog" aria-modal="true" tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="document"
    >
      <div class="modal-header">
        <div class="modal-title {modalTestResult.passed ? 'modal-pass' : 'modal-fail'}">
          {#if modalTestResult.passed}
            <CheckCircle size={20} /> nginx -t Passed
          {:else}
            <XCircle size={20} /> nginx -t Failed
          {/if}
        </div>
        <span class="modal-ts">{modalTestResult.timestamp}</span>
      </div>
      <pre class="modal-output">{modalTestResult.output}</pre>
      <div class="modal-footer">
        <Button variant="primary" class="" onclick={() => (showTestModal = false)} id="nginx-close-test-modal">
          Close
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- General output modal -->
{#if showOutputModal}
  <div use:portal class="modal-backdrop"
    onclick={() => (showOutputModal = false)}
    onkeydown={(e) => e.key === 'Escape' && (showOutputModal = false)}
    role="dialog" aria-modal="true" tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal modal-wide"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="document"
    >
      <div class="modal-header">
        <div class="modal-title">{outputModalTitle}</div>
      </div>
      <pre class="modal-output">{outputModalContent}</pre>
      <div class="modal-footer">
        <Button variant="primary" class="" onclick={() => (showOutputModal = false)} id="nginx-close-output-modal">
          Close
        </Button>
      </div>
    </div>
  </div>
{/if}

<SideDrawer bind:isOpen={showProxyWizard} title="Reverse Proxy Wizard" width="460px">
  <div style="display:flex; flex-direction:column; gap:16px; padding-top:8px;">
    <div style="font-size:13px; color:var(--color-text-secondary); line-height:1.5;">
      Quickly configure Nginx as a reverse proxy for another local or remote service. This generator creates a new site configuration routing all HTTP traffic for a specific domain to your target server.
    </div>

    <!-- Active PM2 Apps Auto-Linker -->
    {#if pm2ProcessesForProxy.length > 0}
      <div style="background:var(--color-bg-raised); border:1px solid var(--color-border); border-radius:8px; padding:12px; display:flex; flex-direction:column; gap:8px;">
        <div style="display:flex; align-items:center; justify-content:space-between;">
          <div style="display:flex; align-items:center; gap:6px; font-size:12px; font-weight:600; color:var(--color-text-primary);">
            <Zap size={13} style="color:var(--color-accent);" />
            <span>Auto-Link Running PM2 Application</span>
          </div>
          <span style="font-size:10.5px; color:var(--color-text-muted);">{pm2ProcessesForProxy.filter(p => p.status === 'online').length} online</span>
        </div>
        <div style="display:flex; gap:6px; flex-wrap:wrap;">
          {#each pm2ProcessesForProxy as p}
            <button
              type="button"
              onclick={() => linkPm2AppToProxy(p)}
              style="display:inline-flex; align-items:center; gap:6px; padding:5px 10px; border-radius:6px; font-size:11.5px; font-family:inherit; cursor:pointer; background:var(--color-bg-surface); border:1px solid var(--color-border-subtle); color:var(--color-text-primary); transition:all 0.15s ease;"
              title="Click to auto-fill reverse proxy for {p.name}"
            >
              <span style="width:7px; height:7px; border-radius:50%; background:{p.status === 'online' ? 'var(--color-success, #10b981)' : 'var(--color-text-muted)'}"></span>
              <span style="font-weight:600;">{p.name}</span>
              <span style="font-family:var(--font-mono); font-size:10.5px; color:var(--color-text-muted);">#{p.pm_id}</span>
              <span style="font-family:var(--font-mono); font-size:10.5px; color:var(--color-accent); font-weight:600; padding:1px 4px; background:var(--color-accent-muted, rgba(0,218,243,0.1)); border-radius:4px;">:{extractPortFromPm2(p)}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="form-group" style="display:flex; flex-direction:column; gap:6px;">
      <label for="proxy-domain" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Domain Name <span style="color:var(--color-error, #f43f5e)">*</span></label>
      <input
        id="proxy-domain"
        type="text"
        class="input"
        class:input-error={proxyConfigTouched.domain && proxyConfigErrors.domain}
        bind:value={proxyConfig.domain}
        oninput={() => proxyConfigTouched.domain = true}
        placeholder="e.g. app.example.com or myapp.local"
      />
      {#if proxyConfigTouched.domain && proxyConfigErrors.domain}
        <small style="color:var(--color-error, #f43f5e); font-size:11px; font-weight:500;">{proxyConfigErrors.domain}</small>
      {/if}
    </div>

    <div style="display:flex; gap:16px;">
      <div class="form-group" style="display:flex; flex-direction:column; gap:6px; flex:2;">
        <label for="proxy-target" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Target IP / Host <span style="color:var(--color-error, #f43f5e)">*</span></label>
        <input
          id="proxy-target"
          type="text"
          class="input"
          class:input-error={proxyConfigTouched.target_ip && proxyConfigErrors.target_ip}
          bind:value={proxyConfig.target_ip}
          oninput={() => proxyConfigTouched.target_ip = true}
          placeholder="127.0.0.1"
        />
        {#if proxyConfigTouched.target_ip && proxyConfigErrors.target_ip}
          <small style="color:var(--color-error, #f43f5e); font-size:11px; font-weight:500;">{proxyConfigErrors.target_ip}</small>
        {/if}
      </div>
      <div class="form-group" style="display:flex; flex-direction:column; gap:6px; flex:1;">
        <label for="proxy-port" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Target Port <span style="color:var(--color-error, #f43f5e)">*</span></label>
        <input
          id="proxy-port"
          type="text"
          class="input"
          class:input-error={proxyConfigTouched.target_port && proxyConfigErrors.target_port}
          bind:value={proxyConfig.target_port}
          oninput={() => proxyConfigTouched.target_port = true}
          placeholder="8080"
        />
        {#if proxyConfigTouched.target_port && proxyConfigErrors.target_port}
          <small style="color:var(--color-error, #f43f5e); font-size:11px; font-weight:500;">{proxyConfigErrors.target_port}</small>
        {/if}
      </div>
    </div>

    <div class="form-group" style="display:flex; align-items:center; justify-content:space-between; padding:12px; background:var(--color-bg-raised); border-radius:8px; border:1px solid var(--color-border);">
      <div style="display:flex; flex-direction:column; gap:4px;">
        <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">Enable WebSockets</span>
        <span style="font-size:11.5px; color:var(--color-text-secondary);">Add Upgrade and Connection headers</span>
      </div>
      <Toggle checked={proxyConfig.enable_websockets} onToggle={(val) => proxyConfig.enable_websockets = val} />
    </div>

    <div style="padding:12px; background:var(--color-bg-raised); border-left:3px solid var(--color-accent); border-radius:4px; margin-top:4px;">
      <span style="font-size:12px; font-weight:600; color:var(--color-text-primary); display:block; margin-bottom:4px;">Note on SSL/HTTPS</span>
      <span style="font-size:12px; color:var(--color-text-secondary); line-height:1.4; display:block;">
        This wizard generates a standard HTTP (Port 80) configuration. To secure this proxy with HTTPS, use the <strong>SSL / Certbot</strong> tab to request a certificate after the site is created.
      </span>
    </div>

    <div style="display:flex; justify-content:flex-end; gap:12px; margin-top:8px;">
      <Button variant="outline" onclick={() => showProxyWizard = false}>Cancel</Button>
      <Button variant="primary" disabled={proxyLoading} onclick={createProxy}>
        {proxyLoading ? 'Generating...' : 'Generate Proxy'}
      </Button>
    </div>
  </div>
</SideDrawer>

<!-- ══ SITE INSPECTOR DRAWER ═════════════════════════════════════════════════ -->
<SideDrawer bind:isOpen={showInspectDrawer} title="Site Inspector — {inspectingSite?.name || 'Config'}" width="680px" dockable={true}>
  {#if inspectingSite}
    <div style="display:flex; flex-direction:column; gap:16px; padding-top:4px;">
      <!-- Metadata Bar -->
      <div class="inspect-meta-card">
        <div class="inspect-meta-row">
          <span class="inspect-label">File Path:</span>
          <code class="inspect-path-code">{inspectingSite.path}</code>
          <button 
            type="button" 
            class="inspect-copy-icon-btn" 
            title="Copy path"
            onclick={() => { navigator.clipboard.writeText(inspectingSite!.path); uiStore.addToast('Copied path', 'info'); }}
          >
            <Copy size={12} />
          </button>
        </div>

        {#if inspectingSite.domains && inspectingSite.domains.length > 0}
          <div class="inspect-meta-row">
            <span class="inspect-label">Domains:</span>
            <div style="display:flex; gap:6px; flex-wrap:wrap;">
              {#each inspectingSite.domains as dom}
                <span class="site-meta-pill domain"><Globe size={11} /> {dom}</span>
              {/each}
            </div>
          </div>
        {/if}

        {#if inspectingSite.proxies && inspectingSite.proxies.length > 0}
          <div class="inspect-meta-row">
            <span class="inspect-label">Proxy Pass:</span>
            <div style="display:flex; gap:6px; flex-wrap:wrap;">
              {#each inspectingSite.proxies as prx}
                <span class="site-meta-pill proxy"><ArrowUpRight size={11} /> {prx}</span>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Inspector Action Toolbar -->
      <div class="inspect-toolbar">
        <div style="display:flex; gap:8px; flex-wrap:wrap;">
          <Button variant="primary" class="btn-sm" onclick={() => { showInspectDrawer = false; openSiteInEditor(inspectingSite!); }}>
            <FileCode size={12} />
            <span>Open in Full Editor</span>
          </Button>
          <Button variant="outline" class="btn-sm" onclick={() => { showInspectDrawer = false; openCloneModal(inspectingSite!); }}>
            <Copy size={12} />
            <span>Clone Site</span>
          </Button>
          <Button variant="outline" class="btn-sm" onclick={() => { showInspectDrawer = false; jumpToSiteLogs(inspectingSite!, 'analytics'); }}>
            <BarChart2 size={12} />
            <span>View Logs</span>
          </Button>
        </div>

        <div style="display:flex; gap:6px;">
          <button 
            type="button" 
            class="reader-tool-btn" 
            class:active={inspectWrapLines} 
            onclick={() => inspectWrapLines = !inspectWrapLines}
            title="Toggle word wrap"
          >
            <WrapText size={12} />
            <span>Wrap</span>
          </button>
          <button 
            type="button" 
            class="reader-tool-btn" 
            onclick={() => { navigator.clipboard.writeText(inspectingContent); uiStore.addToast('Copied config to clipboard', 'info'); }}
            title="Copy full configuration"
          >
            <Copy size={12} />
            <span>Copy</span>
          </button>
        </div>
      </div>

      <!-- Config Code Viewport with Gutter -->
      {#if inspectingLoading}
        <div class="center-state" style="padding:60px 0;"><div class="spinner"></div></div>
      {:else}
        {@const lines = (inspectingContent || '').split('\n')}
        <div class="file-code-viewport" class:wrap-lines={inspectWrapLines} style="max-height: calc(100vh - 310px); border-radius: 8px; border: 1px solid var(--color-border);">
          <div class="code-rows-container">
            {#each lines as line, i}
              <div class="code-row">
                <div class="gutter-num" aria-hidden="true">{i + 1}</div>
                <div class="code-line">{line || '\u00A0'}</div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</SideDrawer>

<!-- ══ CLONE SITE DRAWER ═════════════════════════════════════════════════════ -->
<SideDrawer bind:isOpen={showCloneDrawer} title="Clone Site Configuration" width="460px">
  {#if cloningSite}
    <div style="display:flex; flex-direction:column; gap:16px; padding-top:4px;">
      <div class="drawer-banner">
        <Copy size={16} class="text-accent" />
        <span style="font-size:12.5px; color:var(--color-text-secondary); line-height:1.4;">
          Duplicate <strong>{cloningSite.name}</strong> from <code>{cloningSite.source}</code> into a new isolated virtual host configuration.
        </span>
      </div>

      <div class="form-group" style="display:flex; flex-direction:column; gap:6px;">
        <label for="clone-new-name" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">New Config File Name</label>
        <input 
          id="clone-new-name"
          type="text" 
          class="input" 
          bind:value={cloneNewName} 
          placeholder="my-new-site.conf" 
          style="font-family:var(--font-mono); font-size:12px;"
        />
        <span style="font-size:11px; color:var(--color-text-muted);">Will be saved in {cloningSite.source === 'sites-available' ? '/etc/nginx/sites-available/' : '/etc/nginx/conf.d/'}</span>
      </div>

      <div class="form-group" style="display:flex; flex-direction:column; gap:6px;">
        <label for="clone-new-domain" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">New Server Name / Domain (Optional)</label>
        <input 
          id="clone-new-domain"
          type="text" 
          class="input" 
          bind:value={cloneNewDomain} 
          placeholder="newdomain.com" 
          style="font-family:var(--font-mono); font-size:12px;"
        />
        <span style="font-size:11px; color:var(--color-text-muted);">If provided, replaces the <code>server_name</code> line automatically</span>
      </div>

      <div style="display:flex; justify-content:flex-end; gap:12px; margin-top:8px;">
        <Button variant="outline" onclick={() => showCloneDrawer = false}>Cancel</Button>
        <Button variant="primary" disabled={cloneLoading} onclick={executeCloneSite}>
          {#if cloneLoading}<div class="spinner-sm"></div>{/if}
          <span>Clone Site</span>
        </Button>
      </div>
    </div>
  {/if}
</SideDrawer>

<!-- ══ QUICK SSL ISSUE DRAWER ════════════════════════════════════════════════ -->
<SideDrawer bind:isOpen={showSslIssueDrawer} title="Issue Let's Encrypt SSL" width="480px">
  {#if sslIssueSite}
    <div style="display:flex; flex-direction:column; gap:16px; padding-top:4px;">
      {#if !hasCertbot}
        <div class="drawer-banner" style="background:rgba(239,68,68,0.08); border:1px solid rgba(239,68,68,0.25); flex-direction:column; gap:8px;">
          <div style="display:flex; align-items:center; gap:8px;">
            <AlertTriangle size={16} class="text-error" />
            <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">
              Certbot is not installed
            </span>
          </div>
          <span style="font-size:12px; color:var(--color-text-secondary); line-height:1.4;">
            Install Certbot and the Nginx plugin on your system to issue automated SSL certificates:
          </span>
          <div style="display:flex; align-items:center; justify-content:space-between; background:var(--color-bg-base); border:1px solid var(--color-border); border-radius:6px; padding:6px 10px;">
            <code style="font-family:var(--font-mono); font-size:11.5px; color:var(--color-accent);">sudo dnf install certbot python3-certbot-nginx</code>
            <button 
              type="button" 
              class="inspect-copy-icon-btn" 
              onclick={() => { navigator.clipboard.writeText('sudo dnf install certbot python3-certbot-nginx'); uiStore.addToast('Copied installation command', 'info'); }}
              title="Copy installation command"
            >
              <Copy size={13} />
            </button>
          </div>
        </div>
      {:else}
        <div class="drawer-banner ssl-banner">
          <Lock size={16} class="text-success" />
          <span style="font-size:12.5px; color:var(--color-text-secondary); line-height:1.4;">
            Automated SSL certificate request via Certbot for <strong>{sslIssueSite.name}</strong>.
          </span>
        </div>
      {/if}

      <div class="form-group" style="display:flex; flex-direction:column; gap:6px;">
        <label for="ssl-issue-domain" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Domain Name</label>
        <input 
          id="ssl-issue-domain"
          type="text" 
          class="input" 
          bind:value={sslIssueDomain} 
          placeholder="e.g. app.example.com" 
          style="font-family:var(--font-mono); font-size:12px;"
        />
        <span style="font-size:11px; color:var(--color-text-muted);">Ensure your public DNS A/AAAA records point to this server</span>
      </div>

      <div class="form-group" style="display:flex; flex-direction:column; gap:6px;">
        <label for="ssl-issue-email" style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Admin Email (Optional for Expiry Alerts)</label>
        <input 
          id="ssl-issue-email"
          type="email" 
          class="input" 
          bind:value={sslIssueEmail} 
          placeholder="admin@example.com" 
          style="font-size:12px;"
        />
      </div>

      <div style="display:flex; justify-content:flex-end; gap:12px; margin-top:8px;">
        <Button variant="outline" onclick={() => showSslIssueDrawer = false}>Cancel</Button>
        <Button variant="primary" disabled={sslIssueLoading || !hasCertbot} onclick={executeIssueSsl}>
          {#if sslIssueLoading}<div class="spinner-sm"></div>{/if}
          <Sparkles size={13} />
          <span>Issue &amp; Install Certificate</span>
        </Button>
      </div>
    </div>
  {/if}
</SideDrawer>

<!-- Universal Config Diff Modal for Nginx -->
{#if selectedConfig}
  <ConfigDiffModal
    bind:show={showConfigDiffModal}
    filePath={selectedConfig.path}
    title="Review Nginx Configuration Changes"
    oldContent={savedContent}
    newContent={editorContent}
    warningMessage={selectedConfig.path === '/etc/nginx/nginx.conf' ? 'Notice: You are modifying the primary server configuration file (/etc/nginx/nginx.conf). An automatic safety backup will be created, and syntax will be tested with nginx -t.' : ''}
    isSaving={configSaving}
    onconfirm={async () => {
      await executeSaveConfig();
      showConfigDiffModal = false;
    }}
    oncancel={() => showConfigDiffModal = false}
  />
{/if}

<style>
  /* ─── Layout ─────────────────────────────────────────────────────────── */
  .module-page { overflow: hidden; }
  .tab-content { flex: 1; overflow-y: auto; padding: 0; }
  .tab-section { padding: 0; display: flex; flex-direction: column; gap: 16px; }

  /* ─── Not Installed ──────────────────────────────────────────────────── */
  .not-installed {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    flex: 1;
    padding: 48px;
    text-align: center;
  }
  .ni-icon { color: var(--color-text-muted); }
  .not-installed h2 { font-size: 22px; color: var(--color-text-primary); margin: 0; }
  .not-installed p { color: var(--color-text-secondary); margin: 0; }
  .ni-cmds { display: flex; flex-direction: column; gap: 10px; margin: 8px 0; width: 100%; max-width: 500px; }
  .ni-cmd {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 16px;
  }
  .ni-cmd-label { font-size: 12px; color: var(--color-text-muted); white-space: nowrap; }
  .ni-cmd code { font-family: var(--font-mono); font-size: 13px; color: var(--color-accent-soft); }

  /* ─── Tab Bar ────────────────────────────────────────────────────────── */
  
  
  
  

  /* ─── Spinners ───────────────────────────────────────────────────────── */
  .spinner {
    width: 24px; height: 24px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner-sm {
    width: 14px; height: 14px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }
  .center-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px;
    color: var(--color-text-muted);
  }
  .empty-state {
    padding: 32px;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  /* ─── Overview ───────────────────────────────────────────────────────── */
  .ov-action-ribbon {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 18px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
    flex-wrap: wrap;
  }
  .ov-ribbon-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 260px;
  }
  .ov-service-pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-radius: 20px;
    font-size: 12px;
  }
  .ov-service-pill.active {
    background: rgba(16, 185, 129, 0.1);
    color: var(--color-success);
    border: 1px solid rgba(16, 185, 129, 0.25);
  }
  .ov-service-pill.inactive {
    background: rgba(244, 63, 94, 0.1);
    color: var(--color-error);
    border: 1px solid rgba(244, 63, 94, 0.25);
  }
  .ov-uptime-tag {
    font-size: 11px;
    color: var(--color-text-muted);
    font-weight: 500;
    margin-left: 4px;
  }
  .ov-ribbon-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .overview-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    padding: 0;
  }
  .overview-tri-grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 16px;
    padding: 0;
  }
  @media (max-width: 900px) {
    .overview-grid, .overview-tri-grid {
      grid-template-columns: 1fr;
    }
  }

  .ov-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 16px;
  }
  .ov-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .ov-card-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-primary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .ov-link-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-accent);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
    transition: all 0.15s ease;
  }
  .ov-link-btn:hover {
    background: rgba(99, 102, 241, 0.1);
    color: var(--color-accent-soft);
  }

  .status-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    box-shadow: 0 0 8px currentColor;
    flex-shrink: 0;
  }
  .dot-active {
    background: var(--color-success);
    color: var(--color-success);
    animation: pulse 2s infinite;
  }
  .dot-inactive {
    background: var(--color-error);
    color: var(--color-error);
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.5; transform: scale(0.9); }
  }

  .service-control-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .service-btns {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }
  .ov-since {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    margin: 0;
  }

  .test-result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 8px;
    font-weight: 600;
    font-size: 12.5px;
  }
  .test-pass {
    background: rgba(16, 185, 129, 0.1);
    color: var(--color-success);
    border: 1px solid rgba(16, 185, 129, 0.25);
  }
  .test-fail {
    background: rgba(244, 63, 94, 0.1);
    color: var(--color-error);
    border: 1px solid rgba(244, 63, 94, 0.25);
  }
  .test-output-wrap {
    position: relative;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }
  .test-output {
    max-height: 85px;
    overflow: auto;
    font-size: 11px;
    font-family: var(--font-mono);
    line-height: 1.4;
    padding: 8px 12px;
    margin: 0;
    color: var(--color-text-secondary);
  }

  .stats-grid {
    display: flex;
    gap: 10px;
    align-items: stretch;
    flex-wrap: wrap;
  }
  .stat-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 10px 12px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 8px;
  }
  .stat-value {
    font-size: 17px;
    font-weight: 700;
    color: var(--color-text-primary);
    line-height: 1;
  }
  .stat-label {
    font-size: 10.5px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    margin: 0;
  }
  .stat-enabled .stat-value { color: var(--color-success); }
  .stat-disabled .stat-value { color: var(--color-error); }

  .ov-ssl-ok-note {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--color-text-secondary);
    padding: 6px 10px;
    background: rgba(16, 185, 129, 0.06);
    border: 1px solid rgba(16, 185, 129, 0.2);
    border-radius: 6px;
    margin-top: 4px;
  }

  .ov-env-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ov-env-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .ov-env-label {
    font-size: 11.5px;
    color: var(--color-text-muted);
    font-weight: 500;
  }
  .ov-env-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    padding: 3px 8px;
    border-radius: 6px;
    cursor: pointer;
    color: var(--color-accent);
    font-size: 11px;
    transition: all 0.15s ease;
  }
  .ov-env-btn:hover {
    border-color: var(--color-accent);
    background: rgba(99, 102, 241, 0.08);
  }
  .ov-env-btn code {
    font-family: var(--font-mono);
    color: var(--color-text-secondary);
  }

  .ov-sites-mini-table-wrap {
    overflow-x: auto;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-base);
  }
  .ov-sites-mini-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .ov-sites-mini-table th {
    text-align: left;
    padding: 8px 12px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    background: rgba(255, 255, 255, 0.02);
  }
  .ov-sites-mini-table td {
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-primary);
  }
  .ov-sites-mini-table tr:last-child td {
    border-bottom: none;
  }
  .ov-sites-mini-table tr:hover {
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.04));
  }

  .version-display {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-accent-soft);
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    padding: 6px 10px;
    border-radius: 6px;
  }

  /* ─── Section Headers ────────────────────────────────────────────────── */
  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .section-header h3 { margin: 0; font-size: 15px; font-weight: 600; }
  .header-actions { display: flex; gap: 8px; }
  .row-actions { display: flex; gap: 6px; }

  /* ─── Sites ──────────────────────────────────────────────────────────── */
  .sites-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .sites-search-wrap {
    flex: 1;
    min-width: 260px;
    max-width: 480px;
  }
  .sites-filter-pills {
    display: flex;
    gap: 4px;
    background: var(--color-bg-base);
    padding: 3px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
  }
  .filter-pill {
    background: transparent;
    border: none;
    padding: 4px 10px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--color-text-secondary);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .filter-pill:hover {
    color: var(--color-text-primary);
  }
  .filter-pill.active {
    background: var(--color-bg-card);
    color: var(--color-accent);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    font-weight: 600;
  }

  .site-row {
    transition: background 0.15s ease, transform 0.1s ease;
  }
  .site-row:hover {
    background: var(--color-bg-hover);
  }
  .site-name-cell-wrapper {
    min-width: 180px;
  }
  .site-name-btn {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    background: transparent;
    border: none;
    padding: 2px 4px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    color: var(--color-text-primary);
    transition: all 0.15s ease;
  }
  .site-name-icon-box {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.18);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
    transition: all 0.15s ease;
  }
  .site-name-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text-primary);
    transition: color 0.15s ease;
  }
  .site-row:hover .site-name-title {
    color: var(--color-accent);
  }
  .site-path-wrap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: 5px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .site-path-wrap:hover {
    background: rgba(0, 218, 243, 0.08);
  }
  .site-path-wrap:hover .path-code {
    color: var(--color-accent);
  }
  .path-copy-icon {
    color: var(--color-text-muted);
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .site-row:hover .path-copy-icon {
    opacity: 0.7;
  }
  .site-path-wrap:hover .path-copy-icon {
    opacity: 1;
    color: var(--color-accent);
  }
  .site-routing-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .site-meta-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1.5px 6px;
    border-radius: 4px;
    font-size: 10.5px;
    font-family: var(--font-mono);
    line-height: 1.3;
    font-weight: 500;
  }
  .site-meta-pill.domain {
    background: rgba(0, 218, 243, 0.08);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.2);
  }
  .site-meta-pill.proxy {
    background: rgba(168, 85, 247, 0.1);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }
  .site-ports-cell {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .port-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border-radius: 5px;
    font-size: 11px;
    font-family: var(--font-mono);
    font-weight: 600;
  }
  .port-badge.plain {
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }
  .port-badge.ssl {
    background: rgba(16, 185, 129, 0.1);
    color: var(--color-success);
    border: 1px solid rgba(16, 185, 129, 0.25);
  }
  .btn-quick-ssl-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10.5px;
    font-weight: 600;
    background: linear-gradient(135deg, rgba(236, 72, 153, 0.15), rgba(168, 85, 247, 0.15));
    color: #f472b6;
    border: 1px dashed rgba(236, 72, 153, 0.4);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .btn-quick-ssl-badge:hover {
    background: linear-gradient(135deg, rgba(236, 72, 153, 0.35), rgba(168, 85, 247, 0.35));
    color: #ffffff;
    border-color: #f472b6;
    transform: translateY(-1px);
    box-shadow: 0 2px 8px rgba(236, 72, 153, 0.25);
  }

  /* ─── Inspector & Drawer Utilities ───────────────────────────────────── */
  .inspect-meta-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 8px;
  }
  .inspect-meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .inspect-label {
    font-weight: 600;
    color: var(--color-text-muted);
    min-width: 80px;
  }
  .inspect-path-code {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-accent-soft);
  }
  .inspect-copy-icon-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 2px;
    border-radius: 4px;
    transition: color 0.15s;
  }
  .inspect-copy-icon-btn:hover {
    color: var(--color-accent);
  }
  .inspect-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }
  .drawer-banner {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    background: rgba(0, 218, 243, 0.06);
    border: 1px solid rgba(0, 218, 243, 0.18);
    border-radius: 8px;
  }
  .drawer-banner.ssl-banner {
    background: rgba(16, 185, 129, 0.06);
    border-color: rgba(16, 185, 129, 0.2);
  }

  /* ─── Site Context Banner ────────────────────────────────────────────── */
  .site-context-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 16px;
    background: rgba(0, 218, 243, 0.05);
    border: 1px solid rgba(0, 218, 243, 0.2);
    border-radius: 8px;
    flex-wrap: wrap;
  }
  .context-banner-left {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .context-banner-text {
    font-size: 12.5px;
    color: var(--color-text-primary);
  }
  .site-context-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-radius: 6px;
    font-size: 11.5px;
    font-weight: 500;
  }
  .site-context-pill.dedicated {
    background: rgba(16, 185, 129, 0.12);
    color: var(--color-text-primary);
    border: 1px solid rgba(16, 185, 129, 0.35);
  }
  .site-context-pill.shared {
    background: rgba(245, 158, 11, 0.12);
    color: var(--color-text-primary);
    border: 1px solid rgba(245, 158, 11, 0.38);
  }
  .site-context-pill code {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-accent-soft);
    background: rgba(0, 0, 0, 0.2);
    padding: 1px 4px;
    border-radius: 3px;
  }
  .context-success-icon {
    color: #10b981;
    flex-shrink: 0;
  }
  .context-warn-icon {
    color: #f59e0b;
    flex-shrink: 0;
  }
  .btn-clear-context {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-size: 11.5px;
    padding: 3px 8px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .btn-clear-context:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
    background: rgba(0, 218, 243, 0.08);
  }

  .btn-open-site {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .btn-delete-site {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--color-error);
    border-color: rgba(239, 68, 68, 0.28);
    transition: all 0.15s ease;
  }
  .btn-delete-site:hover {
    background: var(--color-error) !important;
    color: #ffffff !important;
    border-color: var(--color-error) !important;
    transform: translateY(-1px);
    box-shadow: 0 3px 10px rgba(239, 68, 68, 0.35);
  }
  .path-code { font-size: 11px; color: var(--color-text-muted); font-family: var(--font-mono); }

  .new-site-form { margin-bottom: 8px; }
  .form-title { margin: 0 0 16px; font-size: 14px; font-weight: 600; }
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .form-field { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--color-text-secondary); }
  .form-field.form-full { grid-column: 1 / -1; }
  .form-field.form-check { flex-direction: row; align-items: center; gap: 8px; }
  .form-field.form-toggle { flex-direction: row; align-items: center; justify-content: space-between; }
  .form-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }

  /* ─── Config Editor ──────────────────────────────────────────────────── */
  .editor-layout {
    display: flex;
    flex: 1;
    height: 100%;
    overflow: hidden;
  }
  .editor-sidebar {
    width: 220px;
    min-width: 220px;
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 8px 0;
    gap: 2px;
  }
  .editor-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 12px 8px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text-muted);
  }
  .editor-sidebar-sep { border-top: 1px solid var(--color-border); margin: 8px 12px; }
  .sidebar-backup-btn { margin: 0 12px 8px; }
  .file-group-label {
    padding: 4px 12px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.08em;
    margin-top: 6px;
  }
  .file-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-mono);
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file-item:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
  .file-item.selected { background: var(--color-active-bg); color: var(--color-accent-soft); }
  .editor-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 12px;
  }
  .editor-tools { display: flex; gap: 8px; align-items: center; }
  .editor-filename {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-text-primary);
    font-weight: 600;
  }
  .unsaved-warning {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    background: var(--color-warning-muted);
    color: var(--color-warning);
    font-size: 12px;
    border-bottom: 1px solid rgba(245,158,11,0.2);
  }
  .code-editor {
    flex: 1;
    resize: none;
    border: none;
    outline: none;
    background: rgba(0,0,0,0.25);
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.6;
    padding: 16px;
    overflow-y: auto;
  }
  .code-editor.wrap { white-space: pre-wrap; word-break: break-all; }
  .editor-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
  }
  .diff-view {
    flex: 1;
    overflow-y: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    background: rgba(0,0,0,0.2);
    padding: 12px 0;
  }
  .diff-line { display: flex; gap: 12px; padding: 1px 16px; }
  .diff-add { background: rgba(16,185,129,0.1); color: var(--color-success); }
  .diff-remove { background: rgba(244,63,94,0.1); color: var(--color-error); }
  .diff-same { color: var(--color-text-muted); }
  .diff-marker { width: 14px; flex-shrink: 0; font-weight: 700; }
  .diff-text { white-space: pre-wrap; word-break: break-all; }

  /* Backups panel */
  .backups-panel {
    width: 250px;
    min-width: 250px;
    border-left: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 0;
  }
  .backup-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }
  .backup-name { font-family: var(--font-mono); color: var(--color-text-primary); font-size: 11px; word-break: break-all; }
  .backup-ts { color: var(--color-text-muted); font-size: 10px; }

  /* ─── WWW Files ──────────────────────────────────────────────────────── */
  .www-layout { display: flex; height: 100%; overflow: hidden; background: var(--color-bg-base); }
  .www-tree {
    width: 290px;
    min-width: 290px;
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--color-bg-card);
    padding: 0;
    user-select: none;
  }
  .www-tree-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-border);
    background: rgba(255, 255, 255, 0.02);
    flex-shrink: 0;
  }
  .www-tree-title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .www-tree-header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .new-dir-form {
    display: flex;
    gap: 6px;
    padding: 8px 12px;
    align-items: center;
    border-bottom: 1px solid var(--color-border);
    background: rgba(0, 218, 243, 0.05);
  }
  .new-dir-form input { flex: 1; padding: 4px 8px; font-size: 11.5px; border-radius: 4px; }
  .tree-list { display: flex; flex-direction: column; padding: 6px 0; }
  .tree-node {
    display: flex;
    align-items: center;
    position: relative;
  }
  .tree-item {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px 5px 0;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    min-width: 0;
    border-radius: 5px;
    margin: 1px 4px;
    transition: all 0.12s ease;
  }
  .tree-item:hover { 
    background: rgba(255, 255, 255, 0.05); 
    color: var(--color-text-primary); 
  }
  .tree-item.tree-selected { 
    background: rgba(0, 218, 243, 0.12); 
    color: var(--color-accent); 
    font-weight: 500;
  }
  .tree-arrow { width: 14px; display: flex; align-items: center; justify-content: center; opacity: 0.7; }
  .tree-arrow-spacer { width: 14px; flex-shrink: 0; }
  .tree-icon { flex-shrink: 0; }
  .tree-icon.folder { color: #f59e0b; }
  .tree-icon.file { color: var(--color-text-muted); }
  .tree-item.tree-selected .tree-icon.file { color: var(--color-accent); }
  .tree-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tree-size { font-size: 10px; color: var(--color-text-muted); font-family: var(--font-mono); flex-shrink: 0; }
  .rename-input { font-size: 11px; padding: 2px 6px; width: 120px; border-radius: 4px; }

  /* Right-Click Context Menu */
  .www-context-menu {
    position: fixed;
    z-index: 1000;
    min-width: 200px;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    padding: 6px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menu-fade 0.12s ease;
  }
  .context-menu-header {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px 6px;
    font-size: 11px;
    font-weight: 700;
    color: var(--color-text-muted);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    margin-bottom: 2px;
  }
  .context-menu-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }
  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--color-text-secondary);
    font-size: 11.5px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
    white-space: nowrap;
  }
  .context-menu-item:hover {
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-text-primary);
  }
  .context-menu-item.danger { color: var(--color-error); }
  .context-menu-item.danger:hover {
    background: rgba(239, 68, 68, 0.15);
    color: #fca5a5;
  }
  .context-menu-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 3px 0;
  }

  /* WWW Viewer */
  .www-viewer {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    background: var(--color-bg-base);
  }
  .viewer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 12px;
    background: var(--color-bg-card);
  }
  .viewer-header-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }
  .viewer-path-tag {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    background: rgba(255, 255, 255, 0.04);
    padding: 2px 7px;
    border-radius: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .viewer-path-tag:hover { color: var(--color-accent); background: rgba(0, 218, 243, 0.08); }
  .folder-overview-body {
    padding: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
  .folder-quick-shortcuts {
    display: flex;
    gap: 16px;
  }
  .shortcut-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 20px 24px;
    border-radius: 10px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    cursor: pointer;
    transition: all 0.15s ease;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .shortcut-card:hover {
    border-color: var(--color-accent);
    transform: translateY(-2px);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.3);
  }

  /* ─── Modern WWW Code / File Viewer ─────────────────────────────────── */
  .file-reader-frame {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-card);
    min-height: 0;
  }
  .file-reader-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 14px;
    background: rgba(255, 255, 255, 0.02);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 10px;
    flex-wrap: wrap;
  }
  .file-reader-meta-info {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .file-stat-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    background: var(--color-bg-base);
    padding: 2px 7px;
    border-radius: 4px;
    border: 1px solid var(--color-border);
  }
  .file-reader-tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .reader-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    font-size: 11px;
    font-family: var(--font-sans);
    color: var(--color-text-secondary);
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .reader-tool-btn:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
    background: var(--color-active-bg);
  }
  .reader-tool-btn.active {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-active-bg);
    font-weight: 600;
  }
  .file-code-viewport {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: auto;
    background: var(--color-bg-card);
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.65;
  }
  .code-rows-container {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: fit-content;
    padding: 8px 0;
  }
  .code-row {
    display: flex;
    align-items: flex-start;
    width: 100%;
    transition: background 0.1s ease;
  }
  .code-row:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .code-row .gutter-num {
    padding: 0 12px 0 10px;
    min-width: 48px;
    text-align: right;
    user-select: none;
    color: var(--color-text-muted);
    font-size: 11px;
    opacity: 0.6;
    border-right: 1px solid var(--color-border);
    background: var(--color-bg-base);
    flex-shrink: 0;
    line-height: 1.65;
  }
  .code-row .code-line {
    flex: 1;
    padding: 0 16px 0 12px;
    color: var(--color-text-primary);
    white-space: pre;
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.65;
  }
  .file-code-viewport.wrap-lines .code-row .code-line {
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* ─── Advanced Analytics ─────────────────────────────────────────────────── */
  .analytics-section {
    padding: 4px 0 24px 0;
  }
  .analytics-header-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    gap: 16px;
    flex-wrap: wrap;
  }
  .analytics-header-left {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .analytics-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .analytics-title {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .analytics-log-source-tag {
    display: flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    background: rgba(255, 255, 255, 0.05);
    padding: 3px 8px;
    border-radius: 5px;
  }
  .analytics-header-right {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .analytics-sample-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
    padding: 4px 10px;
    border-radius: 6px;
  }

  /* KPI Grid */
  .analytics-kpi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 14px;
  }
  .analytics-kpi-card {
    display: flex;
    flex-direction: column;
    padding: 16px 18px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    transition: transform 0.15s ease, border-color 0.15s ease;
  }
  .analytics-kpi-card:hover {
    transform: translateY(-2px);
    border-color: rgba(0, 218, 243, 0.35);
  }
  .analytics-kpi-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .analytics-kpi-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .analytics-kpi-icon-wrap {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .analytics-kpi-icon-wrap.cyan { background: rgba(0, 218, 243, 0.12); color: #00daf3; }
  .analytics-kpi-icon-wrap.purple { background: rgba(168, 85, 247, 0.12); color: #a855f7; }
  .analytics-kpi-icon-wrap.blue { background: rgba(59, 130, 246, 0.12); color: #3b82f6; }
  .analytics-kpi-icon-wrap.green { background: rgba(16, 185, 129, 0.12); color: #10b981; }
  .analytics-kpi-icon-wrap.amber { background: rgba(245, 158, 11, 0.12); color: #f59e0b; }

  .analytics-kpi-value {
    font-size: 26px;
    font-weight: 800;
    line-height: 1.1;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
    margin-bottom: 8px;
  }
  .analytics-kpi-value.cyan { color: #00daf3; }
  .analytics-kpi-value.purple { color: #c084fc; }
  .analytics-kpi-value.blue { color: #60a5fa; }
  .analytics-kpi-value.green { color: #34d399; }
  .analytics-kpi-value.amber { color: #fbbf24; }
  .analytics-kpi-value.red { color: #f87171; }

  .analytics-kpi-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .analytics-kpi-subtext {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .analytics-kpi-tag {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    text-transform: uppercase;
  }
  .analytics-kpi-tag.cyan { background: rgba(0, 218, 243, 0.15); color: #00daf3; }
  .analytics-kpi-tag.purple { background: rgba(168, 85, 247, 0.15); color: #c084fc; }
  .analytics-kpi-tag.blue { background: rgba(59, 130, 246, 0.15); color: #60a5fa; }
  .analytics-kpi-tag.green { background: rgba(16, 185, 129, 0.15); color: #34d399; }
  .analytics-kpi-tag.amber { background: rgba(245, 158, 11, 0.15); color: #fbbf24; }
  .analytics-kpi-tag.red { background: rgba(239, 68, 68, 0.15); color: #f87171; }

  /* HTTP Status Code Card */
  .analytics-status-card {
    padding: 18px 20px;
    border-radius: 10px;
  }
  .analytics-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .analytics-card-title {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .analytics-card-subtitle {
    margin: 3px 0 0;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }
  .analytics-status-counts-summary {
    display: flex;
    align-items: center;
    gap: 14px;
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-mono);
  }
  .status-summary-item.green { color: var(--color-success); }
  .status-summary-item.blue { color: #38bdf8; }
  .status-summary-item.amber { color: var(--color-warning); }
  .status-summary-item.red { color: var(--color-error); }

  .analytics-status-bar {
    display: flex;
    height: 18px;
    border-radius: 6px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.05);
    margin-bottom: 14px;
    box-shadow: inset 0 2px 4px rgba(0,0,0,0.3);
  }
  .status-seg { height: 100%; transition: width 0.4s ease; }
  .status-seg.seg-2xx { background: linear-gradient(90deg, #059669, #10b981); }
  .status-seg.seg-3xx { background: linear-gradient(90deg, #0284c7, #38bdf8); }
  .status-seg.seg-4xx { background: linear-gradient(90deg, #d97706, #f59e0b); }
  .status-seg.seg-5xx { background: linear-gradient(90deg, #dc2626, #f87171); }
  .status-seg-empty {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .analytics-status-pills-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
  }
  .status-pill-card {
    padding: 10px 14px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .status-pill-card.green { border-left: 3px solid var(--color-success); }
  .status-pill-card.blue { border-left: 3px solid #38bdf8; }
  .status-pill-card.amber { border-left: 3px solid var(--color-warning); }
  .status-pill-card.red { border-left: 3px solid var(--color-error); }

  .status-pill-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
    font-weight: 600;
  }
  .status-pill-dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; margin-right: 5px; }
  .status-pill-dot.green { background: var(--color-success); }
  .status-pill-dot.blue { background: #38bdf8; }
  .status-pill-dot.amber { background: var(--color-warning); }
  .status-pill-dot.red { background: var(--color-error); }
  .status-pill-name { color: var(--color-text-secondary); }
  .status-pill-pct { font-family: var(--font-mono); color: var(--color-text-primary); }
  .status-pill-count { font-size: 11px; color: var(--color-text-muted); font-family: var(--font-mono); }

  /* Hourly Activity Chart */
  .analytics-hourly-card {
    padding: 18px 20px;
    border-radius: 10px;
    position: relative;
    overflow: visible;
  }
  .analytics-hourly-peak-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .analytics-hourly-chart {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    height: 145px;
    padding-top: 42px;
    position: relative;
    overflow: visible;
  }
  .analytics-hourly-col {
    flex: 1;
    min-width: 18px;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    cursor: pointer;
    position: relative;
  }
  .analytics-hourly-bar-wrap {
    width: 100%;
    height: 90px;
    display: flex;
    align-items: flex-end;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 4px;
    overflow: visible;
    position: relative;
  }
  .analytics-hourly-bar {
    width: 100%;
    background: linear-gradient(180deg, #00daf3, #0284c7);
    border-radius: 3px;
    transition: height 0.3s ease, background 0.15s ease;
    position: relative;
  }
  .analytics-hourly-col:hover .analytics-hourly-bar {
    background: linear-gradient(180deg, #38bdf8, #00daf3);
    filter: brightness(1.2);
    box-shadow: 0 0 12px rgba(0, 218, 243, 0.4);
  }
  .hourly-bar-tooltip {
    display: none;
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    background: var(--color-bg-card, #0f172a);
    color: var(--color-text-primary, #fff);
    font-size: 10.5px;
    font-family: var(--font-mono);
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.18));
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
    white-space: nowrap;
    z-index: 9999;
    pointer-events: none;
    align-items: center;
    gap: 5px;
  }
  .hourly-bar-tooltip::after {
    content: '';
    position: absolute;
    top: 100%;
    left: 50%;
    transform: translateX(-50%);
    border-width: 4px;
    border-style: solid;
    border-color: var(--color-border, rgba(255, 255, 255, 0.18)) transparent transparent transparent;
  }
  .hourly-bar-tooltip .tooltip-time {
    color: var(--color-accent, #00daf3);
    font-weight: 700;
  }
  .hourly-bar-tooltip .tooltip-val {
    font-weight: 600;
    color: #fff;
  }
  .hourly-bar-tooltip .tooltip-pct {
    color: var(--color-text-muted, #94a3b8);
    font-size: 9.5px;
  }
  .analytics-hourly-col:hover .hourly-bar-tooltip {
    display: flex;
  }
  .analytics-hourly-label {
    font-size: 9.5px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  /* 2x2 Details Grid */
  .analytics-details-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
    gap: 16px;
  }
  .analytics-table-card {
    padding: 16px 18px;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
  }
  .analytics-table-wrap {
    overflow-x: auto;
    margin-top: 8px;
  }
  .analytics-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .analytics-table th {
    padding: 8px 10px;
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    text-align: left;
  }
  .analytics-table td {
    padding: 7px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
    vertical-align: middle;
  }
  .analytics-rank {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }
  .analytics-ip-cell {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .analytics-ip-addr {
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .analytics-sub-badge {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-muted);
  }
  .analytics-sub-badge.public {
    background: rgba(168, 85, 247, 0.15);
    color: #c084fc;
  }
  .analytics-path-text {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-primary);
    word-break: break-all;
  }
  .analytics-prog-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 100px;
  }
  .analytics-prog-bar {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .analytics-prog-bar.cyan { background: #00daf3; }
  .analytics-prog-bar.purple { background: #a855f7; }
  .analytics-prog-bar.green { background: #10b981; }
  .analytics-prog-bar.blue { background: #3b82f6; }
  .analytics-prog-pct {
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--color-text-muted);
    width: 38px;
    text-align: right;
    flex-shrink: 0;
  }
  .analytics-copy-btn {
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: all 0.12s;
  }
  .analytics-copy-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
  }

  /* Methods & Referrers */
  .analytics-methods-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 6px;
  }
  .analytics-method-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .analytics-method-badge-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 140px;
    flex-shrink: 0;
  }
  .analytics-method-tag {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
  }
  .analytics-method-tag.method-get { background: rgba(16, 185, 129, 0.18); color: #34d399; }
  .analytics-method-tag.method-post { background: rgba(59, 130, 246, 0.18); color: #60a5fa; }
  .analytics-method-tag.method-put { background: rgba(245, 158, 11, 0.18); color: #fbbf24; }
  .analytics-method-tag.method-delete { background: rgba(239, 68, 68, 0.18); color: #f87171; }
  .analytics-method-tag.method-head { background: rgba(168, 85, 247, 0.18); color: #c084fc; }
  .analytics-method-tag.method-options { background: rgba(255, 255, 255, 0.12); color: var(--color-text-secondary); }
  .analytics-method-count {
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--color-text-muted);
  }

  .analytics-referrers-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .analytics-ref-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px;
    background: rgba(255, 255, 255, 0.02);
    border-radius: 5px;
    font-size: 11.5px;
  }
  .analytics-ref-name {
    font-family: var(--font-mono);
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .analytics-ref-count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  /* User Agents */
  .analytics-ua-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 8px;
  }
  .analytics-ua-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    background: rgba(255, 255, 255, 0.02);
    border-radius: 6px;
  }
  .analytics-ua-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
  }
  .analytics-ua-name {
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .analytics-ua-count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  /* ─── Logs ───────────────────────────────────────────────────────────── */
  .logs-toolbar {
    display: flex;
    gap: 16px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 16px;
  }
  .log-select {
    padding: 8px 14px;
    background: rgba(1, 15, 31, 0.7);
    border: 1px solid #3b494c;
    border-radius: 4px;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-mono);
    outline: none;
    cursor: pointer;
    flex: 1;
    min-width: 200px;
    appearance: auto;
    -webkit-appearance: auto;
    color-scheme: dark;
  }
  .log-select:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(0, 218, 243, 0.10), 0 0 8px rgba(0, 218, 243, 0.12);
  }
  .log-select option {
    background: #0d1c2d;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 8px 12px;
  }
  .log-select option:hover,
  .log-select option:checked {
    background: var(--color-accent-muted);
    color: var(--color-accent-soft);
  }
  .log-filter {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 6px 12px;
    color: var(--color-text-muted);
  }


  .log-view {
    flex: 1;
    font-size: 11.5px;
    line-height: 1.5;
    background: rgba(0,0,0,0.3);
    border-radius: 8px;
    border: 1px solid var(--color-border);
    padding: 12px 16px;
    overflow: auto;
    max-height: calc(100vh - 280px);
    color: var(--color-text-secondary);
  }

  /* ─── Modern Human-Readable Log Viewer ────────────────────────────── */
  .log-tab-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
  }

  .log-control-ribbon {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding-bottom: 2px;
  }

  .log-status-pills {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .log-pill-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .log-pill-btn:hover {
    border-color: var(--color-border-hover);
    color: var(--color-text-primary);
  }

  .log-pill-btn.active {
    background: var(--color-bg-card);
    border-color: var(--color-accent);
    color: var(--color-accent);
    box-shadow: 0 0 10px rgba(0, 218, 243, 0.15);
  }

  /* Custom Range Popover (Journal Logs Style) */
  .custom-range-container {
    position: relative;
    display: inline-block;
  }

  .custom-range-popover {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 100;
    width: 310px;
    background: var(--color-bg-popover, var(--color-bg-card, #0d1c2d));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 14px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.45);
  }

  .popover-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .popover-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }

  .popover-row .log-dt {
    width: 100%;
    box-sizing: border-box;
    padding: 6px 10px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-mono);
    outline: none;
    color-scheme: dark;
  }

  .popover-row .log-dt:focus {
    border-color: var(--color-accent);
  }

  .popover-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    padding-top: 10px;
  }

  .popover-btn {
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 6px;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }

  .apply-btn {
    background: var(--color-accent);
    color: #0f172a;
  }

  .apply-btn:hover {
    background: #00b9cf;
  }

  .cancel-btn {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-secondary);
  }

  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: var(--color-text-primary);
  }

  .pill-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .pill-dot.green { background: var(--color-success); }
  .pill-dot.blue { background: var(--color-info); }
  .pill-dot.amber { background: var(--color-warning); }
  .pill-dot.red { background: var(--color-error); }

  .pill-badge {
    background: rgba(255, 255, 255, 0.08);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
  }

  .log-view-toggle {
    display: flex;
    align-items: center;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }

  .view-mode-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .view-mode-btn:hover {
    color: var(--color-text-primary);
  }

  .view-mode-btn.active {
    background: var(--color-bg-card);
    color: var(--color-text-primary);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  }

  .structured-log-stream {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    max-height: calc(100vh - 280px);
    padding-right: 4px;
    padding-bottom: 24px;
  }

  .log-row-card {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 40px;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, #1e293b);
    border-radius: 8px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
    overflow: hidden;
  }

  .log-row-card:hover {
    border-color: var(--color-border-hover, #334155);
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.03));
  }

  .log-row-card.expanded {
    border-color: var(--color-accent, #00daf3);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .log-row-main {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 14px;
    cursor: pointer;
    font-size: 12px;
    min-height: 40px;
    box-sizing: border-box;
    flex-shrink: 0;
  }

  .status-code-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px 8px;
    border-radius: 5px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.02em;
    min-width: 68px;
    flex-shrink: 0;
  }

  .status-code-badge.status-2xx {
    background: rgba(34, 197, 94, 0.14);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.35);
  }

  .status-code-badge.status-3xx {
    background: rgba(14, 165, 233, 0.14);
    color: #38bdf8;
    border: 1px solid rgba(14, 165, 233, 0.35);
  }

  .status-code-badge.status-4xx {
    background: rgba(245, 158, 11, 0.14);
    color: #fbbf24;
    border: 1px solid rgba(245, 158, 11, 0.35);
  }

  .status-code-badge.status-5xx {
    background: rgba(239, 68, 68, 0.14);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.35);
  }

  .status-code-badge.status-other {
    background: rgba(148, 163, 184, 0.14);
    color: #cbd5e1;
    border: 1px solid rgba(148, 163, 184, 0.35);
  }

  .method-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 1px 7px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    min-width: 46px;
    flex-shrink: 0;
    background: var(--color-bg-raised, #1e293b);
    color: var(--color-text-secondary, #94a3b8);
    border: 1px solid var(--color-border, #334155);
  }

  .method-badge.method-get { color: #00daf3; border-color: rgba(0, 218, 243, 0.35); background: rgba(0, 218, 243, 0.08); }
  .method-badge.method-post { color: #60a5fa; border-color: rgba(96, 165, 250, 0.35); background: rgba(96, 165, 250, 0.08); }
  .method-badge.method-put, .method-badge.method-patch { color: #fbbf24; border-color: rgba(251, 191, 36, 0.35); background: rgba(251, 191, 36, 0.08); }
  .method-badge.method-delete { color: #f87171; border-color: rgba(248, 113, 113, 0.35); background: rgba(248, 113, 113, 0.08); }

  .log-path {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary, #f8fafc);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .log-error-msg {
    flex: 1;
    color: var(--color-text-secondary, #cbd5e1);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .log-client-group {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .log-ip {
    font-family: var(--font-mono);
    font-size: 11.5px;
    font-weight: 500;
    color: var(--color-text-secondary, #94a3b8);
    background: var(--color-bg-raised, #1e293b);
    border: 1px solid var(--color-border, #334155);
    padding: 2px 7px;
    border-radius: 4px;
  }

  .client-badge {
    font-size: 10.5px;
    font-weight: 500;
    color: var(--color-text-muted, #64748b);
    background: var(--color-bg-raised, #1e293b);
    border: 1px solid var(--color-border, #334155);
    padding: 1px 6px;
    border-radius: 4px;
  }

  .log-size {
    font-family: var(--font-mono);
    font-size: 11.5px;
    font-weight: 500;
    color: var(--color-text-muted, #94a3b8);
    min-width: 55px;
    text-align: right;
    flex-shrink: 0;
  }

  .log-time {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-muted, #94a3b8);
    min-width: 60px;
    text-align: right;
    flex-shrink: 0;
  }

  .log-expand-icon {
    color: var(--color-text-muted, #64748b);
    transition: transform 0.18s ease;
    flex-shrink: 0;
  }

  .log-row-expanded {
    padding: 12px 14px;
    background: rgba(0, 0, 0, 0.25);
    border-top: 1px solid var(--color-border, #1e293b);
    animation: fadeIn 0.15s ease both;
  }

  .expanded-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    font-size: 12px;
  }

  .exp-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .exp-item.full-width {
    grid-column: 1 / -1;
  }

  .exp-label {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-text-muted, #64748b);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .exp-value {
    color: var(--color-text-secondary, #cbd5e1);
    word-break: break-all;
  }

  .raw-copy-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    background: rgba(0, 0, 0, 0.35);
    padding: 8px 12px;
    border-radius: 6px;
    border: 1px solid var(--color-border, #334155);
    margin-top: 4px;
  }

  .raw-copy-row code {
    font-family: var(--font-mono);
    font-size: 11px;
    color: #94a3b8;
    word-break: break-all;
    white-space: pre-wrap;
    flex: 1;
  }

  .raw-copy-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    border: 1px solid var(--color-border, #334155);
    background: var(--color-bg-raised, #1e293b);
    color: var(--color-text-muted, #94a3b8);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .raw-copy-btn:hover {
    color: var(--color-text-primary, #ffffff);
    border-color: var(--color-accent, #00daf3);
  }

  .raw-terminal-view {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
  }

  .raw-terminal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .raw-code-body {
    margin: 0;
    height: 100%;
    min-height: 350px;
  }

  /* ── Explicit Light Mode High-Contrast Overrides ── */
  :global(html.light-mode) .log-row-card,
  :global([data-theme="light"]) .log-row-card {
    background: #FFFFFF !important;
    border: 1px solid #CBD5E1 !important;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05) !important;
  }

  :global(html.light-mode) .log-row-card:hover,
  :global([data-theme="light"]) .log-row-card:hover {
    background: #F8FAFC !important;
    border-color: #94A3B8 !important;
  }

  :global(html.light-mode) .log-path,
  :global([data-theme="light"]) .log-path {
    color: #0F172A !important;
  }

  :global(html.light-mode) .log-error-msg,
  :global([data-theme="light"]) .log-error-msg {
    color: #991B1B !important;
  }

  :global(html.light-mode) .log-ip,
  :global([data-theme="light"]) .log-ip {
    background: #F1F5F9 !important;
    color: #0F172A !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .client-badge,
  :global([data-theme="light"]) .client-badge {
    background: #F1F5F9 !important;
    color: #334155 !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .log-size,
  :global([data-theme="light"]) .log-size {
    color: #334155 !important;
  }

  :global(html.light-mode) .log-time,
  :global([data-theme="light"]) .log-time {
    color: #475569 !important;
  }

  :global(html.light-mode) .log-expand-icon,
  :global([data-theme="light"]) .log-expand-icon {
    color: #64748B !important;
  }

  :global(html.light-mode) .status-code-badge.status-2xx,
  :global([data-theme="light"]) .status-code-badge.status-2xx {
    background: #DCFCE7 !important;
    color: #15803D !important;
    border: 1px solid #86EFAC !important;
  }

  :global(html.light-mode) .status-code-badge.status-3xx,
  :global([data-theme="light"]) .status-code-badge.status-3xx {
    background: #E0F2FE !important;
    color: #0369A1 !important;
    border: 1px solid #7DD3FC !important;
  }

  :global(html.light-mode) .status-code-badge.status-4xx,
  :global([data-theme="light"]) .status-code-badge.status-4xx {
    background: #FEF3C7 !important;
    color: #B45309 !important;
    border: 1px solid #FCD34D !important;
  }

  :global(html.light-mode) .status-code-badge.status-5xx,
  :global([data-theme="light"]) .status-code-badge.status-5xx {
    background: #FEE2E2 !important;
    color: #B91C1C !important;
    border: 1px solid #FCA5A5 !important;
  }

  :global(html.light-mode) .status-code-badge.status-other,
  :global([data-theme="light"]) .status-code-badge.status-other {
    background: #F1F5F9 !important;
    color: #334155 !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .method-badge,
  :global([data-theme="light"]) .method-badge {
    background: #F8FAFC !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .method-badge.method-get,
  :global([data-theme="light"]) .method-badge.method-get { color: #0284C7 !important; border-color: #7DD3FC !important; background: #F0F9FF !important; }
  :global(html.light-mode) .method-badge.method-post,
  :global([data-theme="light"]) .method-badge.method-post { color: #2563EB !important; border-color: #93C5FD !important; background: #EFF6FF !important; }
  :global(html.light-mode) .method-badge.method-put,
  :global(html.light-mode) .method-badge.method-patch,
  :global([data-theme="light"]) .method-badge.method-put,
  :global([data-theme="light"]) .method-badge.method-patch { color: #D97706 !important; border-color: #FCD34D !important; background: #FFFBEB !important; }
  :global(html.light-mode) .method-badge.method-delete,
  :global([data-theme="light"]) .method-badge.method-delete { color: #DC2626 !important; border-color: #FCA5A5 !important; background: #FEF2F2 !important; }

  :global(html.light-mode) .log-pill-btn,
  :global([data-theme="light"]) .log-pill-btn {
    background: #FFFFFF !important;
    border: 1px solid #CBD5E1 !important;
    color: #334155 !important;
  }

  :global(html.light-mode) .log-pill-btn:hover,
  :global([data-theme="light"]) .log-pill-btn:hover {
    background: #F8FAFC !important;
    color: #0F172A !important;
    border-color: #94A3B8 !important;
  }

  :global(html.light-mode) .log-pill-btn.active,
  :global([data-theme="light"]) .log-pill-btn.active {
    background: #EFF6FF !important;
    border-color: #2563EB !important;
    color: #1D4ED8 !important;
  }

  :global(html.light-mode) .pill-badge,
  :global([data-theme="light"]) .pill-badge {
    background: #F1F5F9 !important;
    color: #0F172A !important;
  }

  :global(html.light-mode) .log-view-toggle,
  :global([data-theme="light"]) .log-view-toggle {
    background: #F1F5F9 !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .view-mode-btn,
  :global([data-theme="light"]) .view-mode-btn {
    color: #475569 !important;
  }

  :global(html.light-mode) .view-mode-btn.active,
  :global([data-theme="light"]) .view-mode-btn.active {
    background: #FFFFFF !important;
    color: #0F172A !important;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1) !important;
  }

  :global(html.light-mode) .log-row-expanded,
  :global([data-theme="light"]) .log-row-expanded {
    background: #F8FAFC !important;
    border-top: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .exp-label,
  :global([data-theme="light"]) .exp-label {
    color: #64748B !important;
  }

  :global(html.light-mode) .exp-value,
  :global([data-theme="light"]) .exp-value {
    color: #0F172A !important;
  }

  :global(html.light-mode) .raw-copy-row,
  :global([data-theme="light"]) .raw-copy-row {
    background: #F1F5F9 !important;
    border: 1px solid #CBD5E1 !important;
  }

  :global(html.light-mode) .raw-copy-row code,
  :global([data-theme="light"]) .raw-copy-row code {
    color: #0F172A !important;
  }

  :global(html.light-mode) .raw-copy-btn,
  :global([data-theme="light"]) .raw-copy-btn {
    background: #FFFFFF !important;
    border: 1px solid #CBD5E1 !important;
    color: #475569 !important;
  }

  :global(html.light-mode) .raw-copy-btn:hover,
  :global([data-theme="light"]) .raw-copy-btn:hover {
    border-color: #2563EB !important;
    color: #1D4ED8 !important;
  }

  /* ─── SSL ────────────────────────────────────────────────────────────── */
  .ssl-notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: var(--color-info-muted);
    color: var(--color-info);
    border: 1px solid rgba(14,165,233,0.2);
    border-radius: 10px;
    font-size: 13px;
    flex-wrap: wrap;
  }
  .ssl-notice code { font-family: var(--font-mono); font-size: 12px; }
  .ssl-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; }
  .ssl-card { display: flex; flex-direction: column; gap: 12px; }
  .ssl-card-header { display: flex; align-items: center; justify-content: space-between; }
  .ssl-domain { display: flex; align-items: center; gap: 6px; font-weight: 600; font-size: 14px; }
  .ssl-meta { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--color-text-secondary); }
  .ssl-days { font-weight: 600; }
  .ssl-days.valid { color: var(--color-success); }
  .ssl-days.expiring { color: var(--color-warning); }
  .ssl-days.expired { color: var(--color-error); }
  .ssl-expiry-bar {
    height: 4px;
    background: rgba(255,255,255,0.08);
    border-radius: 2px;
    overflow: hidden;
  }
  .ssl-expiry-fill { height: 100%; border-radius: 2px; transition: width 0.5s ease; }
  .ssl-expiry-fill.valid { background: var(--color-success); }
  .ssl-expiry-fill.expiring { background: var(--color-warning); }
  .ssl-expiry-fill.expired { background: var(--color-error); }
  .ssl-actions { display: flex; justify-content: flex-end; }

  /* ─── Modals ─────────────────────────────────────────────────────────── */
  .modal-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
  .modal-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .modal-title.modal-pass { color: var(--color-success); }
  .modal-title.modal-fail { color: var(--color-error); }
  .modal-ts { font-size: 11px; color: var(--color-text-muted); }
  .modal-output {
    background: rgba(0,0,0,0.3);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 16px;
    font-size: 12px;
    line-height: 1.5;
    overflow: auto;
    max-height: 300px;
    max-width: 600px;
    min-width: 400px;
    color: var(--color-text-secondary);
  }
  .modal-footer { display: flex; justify-content: flex-end; margin-top: 20px; }
  .modal-wide .modal-output { max-width: 700px; min-width: 500px; }
</style>
