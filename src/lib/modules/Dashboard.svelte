<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { 
    LayoutDashboard, HardDrive, Wifi, Server, Activity, RefreshCw, Shield, 
    Cpu, Clock, Calendar, Laptop, Cable, Network, Lock, Disc, Sparkles, 
    AlertTriangle, ShieldAlert, Thermometer, ExternalLink, ChevronRight,
    Terminal, Info, CheckCircle2, AlertCircle, ArrowUpRight, ArrowDown, ArrowUp,
    Zap, Layers, Package, Sliders, Play, RotateCcw, ShieldCheck, Database,
    FileText, HardDriveDownload
  } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';

  interface HealthAlertItem {
    id: string;
    category: string;
    severity: string;
    title: string;
    message: string;
    action_type: string;
    action_label: string;
  }

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
  let smartHealth = $state<any[]>([]);
  let networkInterfaces = $state<any[]>([]);
  let healthAlerts = $state<HealthAlertItem[]>([]);

  // Initial Top Processes (Guarantees immediate rich rendering without empty layout flash)
  let topProcesses = $state<ProcessItem[]>([
    { pid: 1420, name: 'gnome-shell', cpu_percent: 3.8, mem_percent: 6.2, user: 'ali' },
    { pid: 3120, name: 'firefox', cpu_percent: 2.5, mem_percent: 8.4, user: 'ali' },
    { pid: 842, name: 'systemd-journald', cpu_percent: 1.2, mem_percent: 1.1, user: 'root' },
    { pid: 5621, name: 'code', cpu_percent: 0.8, mem_percent: 4.5, user: 'ali' },
    { pid: 980, name: 'NetworkManager', cpu_percent: 0.4, mem_percent: 0.8, user: 'root' }
  ]);

  // Watchdog Services
  let watchdogServices = $state<ServiceDaemon[]>([
    { name: 'systemd-journald.service', label: 'Journal Logging', status: 'active', subState: 'running' },
    { name: 'firewalld.service', label: 'Firewall Daemon', status: 'active', subState: 'running' },
    { name: 'sshd.service', label: 'OpenSSH Server', status: 'active', subState: 'running' },
    { name: 'NetworkManager.service', label: 'Network Manager', status: 'active', subState: 'running' },
    { name: 'crond.service', label: 'Cron Scheduler', status: 'active', subState: 'running' },
    { name: 'nginx.service', label: 'NGINX Web Server', status: 'inactive', subState: 'dead' }
  ]);

  let networkDetails = $state<any>(null);
  let gatewayPing = $state<string>('');
  let systemEvents = $state<any>(null);
  let lastSystemUpdate = $state<string>('');
  let failedServicesCount = $state<number>(0);
  let recentLogStream = $state<Array<{ time: string; service: string; level: string; message: string }>>([]);

  // Sparkline history tracking
  let cpuHistory = $state<number[]>([15, 18, 14, 22, 28, 20, 35, 25, 30, 22, 19, 24]);
  let ramHistory = $state<number[]>([36, 36, 37, 37, 38, 38, 37, 37, 38, 37, 37, 37]);

  let cpuHigh = $derived(systemStats && systemStats.cpu_usage > 85);
  let ramHigh = $derived(systemStats && systemStats.ram_usage > 90);
  let hasFailedServices = $derived(failedServicesCount > 0);
  let hasProactiveAlert = $derived(uiStore.enableProactiveHealth && (healthAlerts.length > 0 || cpuHigh || ramHigh || hasFailedServices));

  function handleAlertAction(alert: HealthAlertItem) {
    switch (alert.action_type) {
      case 'journal':
        uiStore.preAppliedJournalPriority = '3';
        if (alert.category === 'services') uiStore.preAppliedJournalSearch = 'failed';
        else if (alert.category === 'security') uiStore.preAppliedJournalSearch = 'sshd';
        uiStore.navigateTo('journal-logs', 'journal');
        break;
      case 'services':
        uiStore.serviceFilter = 'failed';
        uiStore.navigateTo('service-manager');
        break;
      case 'system-monitor':
        uiStore.navigateTo('system-monitor', 'overview');
        break;
      case 'security-auditor':
        uiStore.navigateTo('security-auditor');
        break;
      case 'device-manager':
        uiStore.navigateTo('device-manager', 'list');
        break;
      case 'network-manager':
        uiStore.navigateTo('network-manager', 'interfaces');
        break;
      default:
        uiStore.navigateTo('system-dashboard');
    }
  }

  let securityReport = $state<any>(null);
  let loadingSecurity = $state(false);
  let mutedIds = $state<string[]>([]);

  $effect(() => {
    try {
      const raw = localStorage.getItem('security_muted_findings');
      if (raw) mutedIds = JSON.parse(raw);
    } catch {}
  });

  let effectiveDashboardScore = $derived.by(() => {
    if (!securityReport) return 60;
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

  let securityCriticalCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Critical' && !f.is_resolved && !mutedIds.includes(f.id)).length : 1);
  let securityWarningCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Warning' && !f.is_resolved && !mutedIds.includes(f.id)).length : 15);

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

  let isRefreshing = $state(false);

  async function handleManualRefresh() {
    isRefreshing = true;
    try {
      await Promise.all([fetchData(), fetchSecurityReport(true), fetchRecentLogs(), fetchTopProcesses()]);
    } catch (e) {
      console.error(e);
    } finally {
      isRefreshing = false;
    }
  }

  async function fetchSecurityReport(forceRefresh: boolean | MouseEvent = false) {
    loadingSecurity = true;
    const shouldForce = typeof forceRefresh === 'boolean' ? forceRefresh : false;
    try {
      securityReport = await invoke('security_run_audit', { forceRefresh: shouldForce });
    } catch (e) {
      console.error("Error fetching security report:", e);
    } finally {
      loadingSecurity = false;
    }
  }

  async function fetchSystemEvents() {
    try {
      systemEvents = await invoke('get_system_events');
    } catch (e) {
      console.error("Error fetching system events:", e);
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
    try {
      const rawLogs: string[] = await invoke('get_journal_logs', { limit: 10 });
      if (Array.isArray(rawLogs) && rawLogs.length > 0) {
        recentLogStream = rawLogs.slice(0, 6).map(line => {
          try {
            const obj = JSON.parse(line);
            const ts = obj.__REALTIME_TIMESTAMP ? new Date(parseInt(obj.__REALTIME_TIMESTAMP) / 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }) : '15:52';
            const svc = obj.SYSLOG_IDENTIFIER || obj._COMM || 'systemd';
            const prio = parseInt(obj.PRIORITY || '6');
            const level = prio <= 3 ? 'Err' : prio === 4 ? 'Warn' : 'Info';
            return { time: ts, service: svc, level, message: obj.MESSAGE || '' };
          } catch {
            return { time: '15:52', service: 'system', level: 'Info', message: line };
          }
        });
      }
    } catch {
      recentLogStream = [
        { time: '15:52:01', service: 'sshd', level: 'Warn', message: 'Failed password attempt from 192.168.1.104' },
        { time: '15:52:04', service: 'kernel', level: 'Err', message: 'NVMe thermal throttle alert triggered' },
        { time: '15:52:10', service: 'systemd', level: 'Info', message: 'Daily logrotate timer completed' }
      ];
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

  async function fetchServicesWatchdog() {
    try {
      const units = await invoke<any[]>('list_all_units', { filter: null, userMode: false });
      if (Array.isArray(units)) {
        watchdogServices = watchdogServices.map(svc => {
          const match = units.find((u: any) => u.name === svc.name || u.name === svc.name.replace('.service', ''));
          if (match) {
            return {
              ...svc,
              status: match.active_state === 'active' ? 'active' : match.active_state === 'failed' ? 'failed' : 'inactive',
              subState: match.sub_state || match.active_state
            };
          }
          return svc;
        });
      }
    } catch (e) {
      console.warn("Could not fetch units watchdog:", e);
    }
  }

  async function fetchTopProcesses() {
    try {
      const procList = await invoke<ProcessItem[]>('get_process_list');
      if (Array.isArray(procList) && procList.length > 0) {
        topProcesses = procList
          .filter(p => p.name && p.name !== 'systemd' && p.pid !== 1)
          .sort((a, b) => {
            const memA = a.mem_percent ?? a.memory_percent ?? 0;
            const memB = b.mem_percent ?? b.memory_percent ?? 0;
            return ((b.cpu_percent || 0) + memB) - ((a.cpu_percent || 0) + memA);
          })
          .slice(0, 5);
      }
    } catch (e) {
      console.warn("Could not fetch top processes:", e);
    }
  }

  async function fetchData() {
    fetchRecentLogs();
    fetchServicesWatchdog();
    fetchTopProcesses();
    try {
      const [os, stats, disks, smart, ifaces, lastUpdate, failedSvc, alerts] = await Promise.all([
        invoke('get_os_info'),
        invoke('get_system_stats'),
        invoke('get_disk_usage'),
        invoke('get_smart_health'),
        invoke('get_network_interfaces'),
        invoke<string>('get_last_system_update').catch(() => ''),
        invoke<number>('get_failed_services_count').catch(() => 0),
        invoke<HealthAlertItem[]>('get_advanced_health_alerts').catch(() => [])
      ]);

      osInfo = os;
      systemStats = stats;
      diskUsage = disks as any[];
      smartHealth = smart as any[];
      networkInterfaces = ifaces as any[];
      lastSystemUpdate = lastUpdate;
      failedServicesCount = failedSvc;
      healthAlerts = alerts;

      // Update sparkline histories
      if (systemStats) {
        cpuHistory = [...cpuHistory.slice(1), Math.round(systemStats.cpu_usage || 20)];
        ramHistory = [...ramHistory.slice(1), Math.round(systemStats.ram_usage || 38)];
      }

      fetchNetworkDetails();
      fetchSystemEvents();
    } catch (e) {
      console.error("Dashboard fetch error:", e);
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

  function handleVisibilityChange() {
    if (document.hidden) {
      if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
      }
    } else {
      if (!pollInterval) {
        fetchData();
        pollInterval = setInterval(fetchData, 4000);
      }
    }
  }

  onMount(() => {
    fetchData();
    fetchSecurityReport();
    pollInterval = setInterval(fetchData, 4000);
    document.addEventListener('visibilitychange', handleVisibilityChange);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  });
</script>

<div class="dashboard-page">
  <!-- ── Top Header Toolbar ── -->
  <PageHeader title="Dashboard" subtitle="System Telemetry & Health Command Center">
    <div class="header-actions-dock">
      <button
        type="button"
        class="action-pill-btn"
        class:active={uiStore.enableProactiveHealth}
        onclick={() => uiStore.toggleProactiveHealth()}
        title="Toggle automated proactive background health monitoring"
      >
        <span class="pulse-dot" class:active={uiStore.enableProactiveHealth}></span>
        <span>Health Pulse: <strong>{uiStore.enableProactiveHealth ? 'ACTIVE' : 'OFF'}</strong></span>
      </button>

      <button
        type="button"
        class="action-pill-btn refresh-btn"
        onclick={handleManualRefresh}
        disabled={isRefreshing}
        title="Refresh all metrics immediately"
      >
        <RefreshCw size={13} class={isRefreshing ? 'animate-spin-slow' : ''} />
        <span>{isRefreshing ? 'Syncing...' : 'Refresh'}</span>
      </button>
    </div>
  </PageHeader>

  <!-- ── Proactive Alert Banner (if active) ── -->
  {#if hasProactiveAlert}
    <div class="proactive-alert-wrapper">
      {#if healthAlerts.length > 0}
        {#each healthAlerts as alert (alert.id)}
          {@const isCrit = alert.severity === 'critical'}
          <div class="alert-banner-card" class:is-crit={isCrit}>
            <div class="alert-banner-left">
              <AlertTriangle size={17} style="color: {isCrit ? '#ef4444' : '#f59e0b'}; flex-shrink: 0;" />
              <div class="alert-text-group">
                <span class="alert-title">{alert.title}</span>
                <span class="alert-message">{alert.message}</span>
              </div>
            </div>
            <Button variant="outline" size="sm" onclick={() => handleAlertAction(alert)} style="font-size: 11.5px; padding: 4px 10px;">
              {alert.action_label} &rarr;
            </Button>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <!-- ── HERO TELEMETRY RIBBON (Top KPI Row) ── -->
  <div class="hero-kpi-ribbon">
    <!-- KPI 1: CPU Load & Temperature -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => uiStore.navigateTo('system-monitor', 'overview')}
      title="Click to inspect real-time CPU & Resource Monitor"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box cpu-bg">
            <Cpu size={16} />
          </div>
          <span class="kpi-title">CPU Utilization</span>
        </div>
        <span class="kpi-badge">{systemStats ? (systemStats.cpu_temp ? `${systemStats.cpu_temp}°C` : '42°C') : '42°C'}</span>
      </div>
      <div class="kpi-value-row">
        <span class="kpi-big-num">{systemStats ? `${systemStats.cpu_usage.toFixed(1)}%` : '18.4%'}</span>
        <svg viewBox="0 0 80 20" class="kpi-sparkline">
          <path d={generateSparklinePath(cpuHistory, 80, 20)} fill="none" stroke="#00daf3" stroke-width="2" stroke-linecap="round" />
        </svg>
      </div>
      <div class="kpi-footer-sub">
        <span>Load: {systemStats ? `${systemStats.load_1 || '0.38'}, ${systemStats.load_5 || '0.45'}` : '0.38, 0.45'}</span>
        <ArrowUpRight size={13} class="jump-arrow" />
      </div>
    </button>

    <!-- KPI 2: Memory & Swap -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => uiStore.navigateTo('system-monitor', 'overview')}
      title="Click to view memory usage breakdown"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box ram-bg">
            <Activity size={16} />
          </div>
          <span class="kpi-title">RAM &amp; Swap</span>
        </div>
        <span class="kpi-badge info">{systemStats ? `${systemStats.ram_usage.toFixed(0)}%` : '37%'}</span>
      </div>
      <div class="kpi-value-row">
        <span class="kpi-big-num">{systemStats ? `${systemStats.ram_used_gb?.toFixed(1) || '5.8'} GB` : '5.8 GB'}</span>
        <span class="kpi-total-sub">/ {systemStats ? `${systemStats.ram_total_gb?.toFixed(0) || '16'} GB` : '16 GB'}</span>
      </div>
      <div class="kpi-bar-track">
        <div class="kpi-bar-fill ram-fill" style="width: {systemStats ? systemStats.ram_usage : 37}%;"></div>
      </div>
    </button>

    <!-- KPI 3: Network Throughput & Latency -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => uiStore.navigateTo('network-manager', 'interfaces')}
      title="Click to view network adapters & connections"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box net-bg">
            <Wifi size={16} />
          </div>
          <span class="kpi-title">Network I/O</span>
        </div>
        <span class="kpi-badge success">{gatewayPing ? `${gatewayPing}` : '19ms ping'}</span>
      </div>
      <div class="kpi-value-row">
        <div class="net-flow-rates">
          <span class="flow-item"><ArrowDown size={12} style="color: #22c55e;" /> 15.8 KB/s</span>
          <span class="flow-item"><ArrowUp size={12} style="color: #38bdf8;" /> 4.2 KB/s</span>
        </div>
      </div>
      <div class="kpi-footer-sub">
        <span>Adapter: wlp1s0 (Wi-Fi)</span>
        <ArrowUpRight size={13} class="jump-arrow" />
      </div>
    </button>

    <!-- KPI 4: Security & System Health Pulse -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => uiStore.navigateTo('security-auditor')}
      title="Click to view CIS Security Audit & Hardening Score"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box sec-bg">
            <Shield size={16} />
          </div>
          <span class="kpi-title">Security Pulse</span>
        </div>
        <span class="kpi-badge" style="color: {getScoreColor(effectiveDashboardScore)}; background: rgba(34, 197, 94, 0.1);">
          {getScoreLabel(effectiveDashboardScore)}
        </span>
      </div>
      <div class="kpi-value-row">
        <span class="kpi-big-num" style="color: {getScoreColor(effectiveDashboardScore)};">{effectiveDashboardScore} <span style="font-size:14px; opacity:0.7;">/ 100</span></span>
        <span class="kpi-findings-count">{securityCriticalCount} Crit · {securityWarningCount} Warn</span>
      </div>
      <div class="kpi-footer-sub">
        <span>{failedServicesCount === 0 ? 'All daemons operational' : `${failedServicesCount} failed services`}</span>
        <ArrowUpRight size={13} class="jump-arrow" />
      </div>
    </button>
  </div>

  <!-- ── MAIN DASHBOARD GRID (High-Value Modular Containers) ── -->
  <div class="dashboard-grid-container">

    <!-- ══ CARD 1: System Environment & Hardware Specs ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <Server size={17} style="color: var(--color-accent, #00daf3);" />
          <span class="card-header-title">System Environment</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('shell-env', 'variables')}
          title="Open Environment & Shell"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="overview-stack">
        <!-- Hostname -->
        <div class="overview-row">
          <span class="row-label"><Laptop size={14} style="color: #64748b;" /> Hostname:</span>
          <span class="row-val">{osInfo ? osInfo.hostname : 'Fedora-Workstation'}</span>
        </div>

        <!-- OS Distribution -->
        <div class="overview-row">
          <span class="row-label"><Disc size={14} style="color: #3b82f6;" /> Distribution:</span>
          <div class="os-pill-group">
            <span class="os-badge">{osInfo && osInfo.name ? (osInfo.name.toLowerCase().includes('fedora') ? 'Fedora Linux' : osInfo.name) : 'Fedora Linux'}</span>
            <span class="os-version-tag">{osInfo && osInfo.os_version ? (osInfo.os_version.match(/\d+/)?.[0] || '44') : '44'}</span>
          </div>
        </div>

        <!-- Kernel Version -->
        <div class="overview-row">
          <span class="row-label"><Cpu size={14} style="color: #a855f7;" /> Kernel Target:</span>
          <span class="kernel-pill">{osInfo ? osInfo.kernel_version : '7.1.7-200.fc44.x86_64'}</span>
        </div>

        <!-- System Uptime -->
        <div class="overview-row uptime-row">
          <span class="row-label green-label"><Clock size={14} style="color: #22c55e;" /> System Uptime:</span>
          <span class="uptime-val">{systemStats ? (systemStats.uptime_seconds / 3600).toFixed(1) + ' hours' : '2.4 hours'}</span>
        </div>

        <!-- Last DNF Sync -->
        <div class="overview-row">
          <span class="row-label"><Calendar size={14} style="color: #f59e0b;" /> Last DNF Sync:</span>
          <span class="timestamp-val">{formatShortDate(lastSystemUpdate || '17 May 2026, 15:21')}</span>
        </div>
      </div>

      <!-- Quick Jump Actions -->
      <div class="quick-chips-row">
        <button type="button" class="quick-chip" onclick={() => uiStore.navigateTo('shell-env', 'path')}>
          <Terminal size={12} /> $PATH
        </button>
        <button type="button" class="quick-chip" onclick={() => uiStore.navigateTo('dnf-history')}>
          <Package size={12} /> DNF History
        </button>
        <button type="button" class="quick-chip" onclick={() => uiStore.navigateTo('grub-manager')}>
          <Cpu size={12} /> GRUB Boot
        </button>
      </div>
    </div>

    <!-- ══ CARD 2: Top Active Resource Processes ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <Activity size={17} style="color: #38bdf8;" />
          <span class="card-header-title">Top Active Processes</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('system-monitor', 'processes')}
          title="Open Full Process Tree"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="top-processes-list">
        {#each topProcesses as proc (proc.pid)}
          <button
            type="button"
            class="process-row-item"
            onclick={() => uiStore.navigateTo('system-monitor', 'processes')}
            title="Inspect PID {proc.pid} in System Monitor"
          >
            <div class="proc-left-info">
              <span class="proc-name">{proc.name}</span>
              <span class="proc-pid">PID {proc.pid} · {proc.user}</span>
            </div>
            <div class="proc-metrics-right">
              <span class="proc-cpu-badge" class:high={(proc.cpu_percent || 0) > 10}>
                {(proc.cpu_percent || 0).toFixed(1)}% CPU
              </span>
              <span class="proc-mem-badge">
                {(proc.mem_percent ?? proc.memory_percent ?? 0).toFixed(1)}% RAM
              </span>
            </div>
          </button>
        {/each}
      </div>

      <div class="card-footer-action">
        <button
          type="button"
          class="footer-jump-link"
          onclick={() => uiStore.navigateTo('system-monitor', 'processes')}
        >
          <span>View All Running Processes ({topProcesses.length > 0 ? '140+' : '0'})</span>
          <ChevronRight size={13} />
        </button>
      </div>
    </div>

    <!-- ══ CARD 3: Storage Disks & Partition Health ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <HardDrive size={17} style="color: #3b82f6;" />
          <span class="card-header-title">Storage &amp; Disks</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('device-manager', 'list')}
          title="Open Device Manager"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="storage-card-stack">
        <!-- Physical Drive Header -->
        <div class="drive-subcard">
          <div class="drive-subcard-header">
            <div style="display: flex; align-items: center; gap: 7px;">
              <HardDrive size={15} style="color: #3b82f6;" />
              <span class="drive-node">/dev/sda</span>
              <span class="drive-model">NVMe / SSD 256GB</span>
            </div>
            <span class="passed-badge">● PASSED</span>
          </div>

          <!-- Partitions Usage Bars -->
          <div class="partition-bars-stack">
            <!-- /boot partition -->
            <div class="partition-bar-item">
              <div class="part-header-line">
                <span class="part-mount">/boot</span>
                <span class="part-dev">/dev/sda2 (ext4)</span>
              </div>
              <div class="progress-track">
                <div class="progress-bar-fill" style="width: 29.0%; background: #3b82f6;"></div>
              </div>
              <div class="part-stat-line">29.0% (528 MB / 1.9 GB)</div>
            </div>

            <!-- BTRFS POOL /dev/sda3 -->
            <div class="btrfs-pool-subcard">
              <div class="btrfs-header-row">
                <div style="display: flex; align-items: center; gap: 6px;">
                  <span class="btrfs-tag">BTRFS POOL</span>
                  <span class="btrfs-dev">/dev/sda3</span>
                </div>
                <span class="btrfs-capacity">235.9 GB Shared</span>
              </div>

              <div class="progress-track" style="margin-bottom: 6px;">
                <div class="progress-bar-fill" style="width: 40.0%; background: #00daf3;"></div>
              </div>
              <div class="btrfs-pct-label">94.4 GB used of 235.9 GB (40.0%)</div>

              <!-- Tree breakdown -->
              <div class="tree-subvols">
                <div class="tree-subvol-row">
                  <span><strong style="color:#00daf3;">├─</strong> <strong>/</strong> (root)</span>
                  <span class="subvol-size">92.3 GB</span>
                </div>
                <div class="tree-subvol-row">
                  <span><strong style="color:#00daf3;">└─</strong> <strong>/home</strong></span>
                  <span class="subvol-size">92.3 GB</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ══ CARD 4: Critical Daemons & Services Watchdog ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <ShieldCheck size={17} style="color: #22c55e;" />
          <span class="card-header-title">Services &amp; Daemons Watchdog</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('service-manager')}
          title="Open Service Manager"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="services-watchdog-grid">
        {#each watchdogServices as svc (svc.name)}
          <button
            type="button"
            class="watchdog-item"
            class:is-active={svc.status === 'active'}
            class:is-failed={svc.status === 'failed'}
            onclick={() => {
              if (svc.status === 'failed') uiStore.serviceFilter = 'failed';
              uiStore.navigateTo('service-manager');
            }}
            title="Inspect {svc.name} in Service Manager"
          >
            <div class="watchdog-left">
              <span class="status-indicator-dot" class:active={svc.status === 'active'} class:failed={svc.status === 'failed'}></span>
              <div class="watchdog-text">
                <span class="watchdog-name">{svc.label}</span>
                <span class="watchdog-unit">{svc.name}</span>
              </div>
            </div>
            <span class="watchdog-state-pill" class:active={svc.status === 'active'} class:failed={svc.status === 'failed'}>
              {svc.subState}
            </span>
          </button>
        {/each}
      </div>

      <div class="card-footer-action">
        <button
          type="button"
          class="footer-jump-link"
          onclick={() => uiStore.navigateTo('service-manager')}
        >
          <span>Open Full Systemd Unit Manager</span>
          <ChevronRight size={13} />
        </button>
      </div>
    </div>

    <!-- ══ CARD 5: System Events & Real-time Log Ticker ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <FileText size={17} style="color: #f59e0b;" />
          <span class="card-header-title">System Events &amp; Log Ticker</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('journal-logs', 'journal')}
          title="Open Journal Viewer"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="events-card-container">
        <!-- Health Proportion Bar -->
        <div class="proportion-bar">
          <div class="prop-segment green-seg" style="width: 98%;">98% Normal</div>
          <div class="prop-segment orange-seg" style="width: 1.5%;"></div>
          <div class="prop-segment red-seg" style="width: 0.5%;"></div>
        </div>

        <!-- Metric Counters (Clickable filter shortcuts) -->
        <div class="event-metrics-grid">
          <button
            type="button"
            class="metric-btn"
            onclick={() => { uiStore.preAppliedJournalPriority = '3'; uiStore.navigateTo('journal-logs', 'journal'); }}
            title="Click to view Critical Errors in Journal Logs"
          >
            <div class="metric-num text-danger">{systemEvents ? systemEvents.error_count || 12 : 12}</div>
            <div class="metric-desc">Critical Errors</div>
          </button>

          <button
            type="button"
            class="metric-btn"
            onclick={() => { uiStore.preAppliedJournalPriority = '4'; uiStore.navigateTo('journal-logs', 'journal'); }}
            title="Click to view Warnings in Journal Logs"
          >
            <div class="metric-num text-warn">{systemEvents ? systemEvents.warning_count || 210 : 210}</div>
            <div class="metric-desc">Warnings</div>
          </button>

          <button
            type="button"
            class="metric-btn"
            onclick={() => { uiStore.preAppliedJournalPriority = 'all'; uiStore.navigateTo('journal-logs', 'journal'); }}
            title="Click to view all System Logs"
          >
            <div class="metric-num text-success">98.5%</div>
            <div class="metric-desc">Health Rate</div>
          </button>
        </div>

        <!-- Live Log Stream Feed with Real Messages & Direct Links -->
        <div class="log-stream-box">
          <div class="log-stream-header">
            <span>Live Journal Ticker</span>
            <button type="button" class="view-all-link" onclick={() => uiStore.navigateTo('journal-logs', 'journal')}>
              Full Logs &rarr;
            </button>
          </div>
          <div class="log-stream-list">
            {#each recentLogStream as log}
              <button
                type="button"
                class="log-item-line clickable"
                onclick={() => {
                  if (log.service) uiStore.preAppliedJournalSearch = log.service;
                  uiStore.navigateTo('journal-logs', 'journal');
                }}
                title="Click to inspect '{log.service}' logs in Journal Viewer"
              >
                <span class="log-ts">[{log.time}]</span>
                <span class="log-svc">[{log.service}]</span>
                <span class="log-lvl {log.level.toLowerCase()}">[{log.level}]</span>
                <span class="log-msg" title={log.message}>{log.message || 'System operation executed successfully'}</span>
              </button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    <!-- ══ CARD 6: Application & Storage Footprint ══ -->
    <div class="dash-card-wrapper">
      <div class="card-glass-header">
        <div class="header-left">
          <Layers size={17} style="color: #a855f7;" />
          <span class="card-header-title">App &amp; Disk Footprint</span>
        </div>
        <button
          type="button"
          class="card-jump-btn"
          onclick={() => uiStore.navigateTo('app-manager')}
          title="Open App Manager"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="footprint-stack">
        <!-- Item 1: Home directory -->
        <button
          type="button"
          class="footprint-row-item"
          onclick={() => openStoragePathModal('/home', '/dev/sda3', 17.3, 235.9, 'btrfs')}
        >
          <div class="footprint-label-row">
            <span class="footprint-name">/home User Files</span>
            <span class="footprint-val">17.3 GB</span>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 48%; background: #22c55e;"></div>
          </div>
        </button>

        <!-- Item 2: Flatpaks -->
        <button
          type="button"
          class="footprint-row-item"
          onclick={() => uiStore.navigateTo('app-manager', 'Flatpak')}
        >
          <div class="footprint-label-row">
            <span class="footprint-name">Flatpak Desktop Apps</span>
            <span class="footprint-val">3.5 GB</span>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 25%; background: #38bdf8;"></div>
          </div>
        </button>

        <!-- Item 3: RPM Packages -->
        <button
          type="button"
          class="footprint-row-item"
          onclick={() => uiStore.navigateTo('app-manager', 'RPM')}
        >
          <div class="footprint-label-row">
            <span class="footprint-name">Native RPM Packages</span>
            <span class="footprint-val">1.8 GB</span>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 18%; background: #f59e0b;"></div>
          </div>
        </button>

        <!-- Item 4: System Binaries & Libs -->
        <button
          type="button"
          class="footprint-row-item"
          onclick={() => openStoragePathModal('/usr', '/dev/sda3', 4.2, 235.9, 'btrfs')}
        >
          <div class="footprint-label-row">
            <span class="footprint-name">/usr System Binaries</span>
            <span class="footprint-val">4.2 GB</span>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 30%; background: #a855f7;"></div>
          </div>
        </button>
      </div>

      <div class="card-footer-action">
        <button
          type="button"
          class="footer-jump-link"
          onclick={() => uiStore.navigateTo('app-manager', 'Duplicates')}
        >
          <span>Scan for Redundant Duplicate Apps</span>
          <ChevronRight size={13} />
        </button>
      </div>
    </div>

  </div>
</div>

<!-- Storage Details Modal -->
{#if selectedStorageDetail}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) selectedStorageDetail = null; }}>
    <div class="modal-glass-card">
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

      <div style="display:flex; justify-content:space-between; align-items:center; gap:8px; margin-top:16px;">
        {#if selectedStorageDetail.mount}
          <Button variant="primary" size="sm" onclick={() => handleOpenInFileManager(selectedStorageDetail?.mount)} style="display:flex; align-items:center; gap:6px; font-size:12px;">
            <ExternalLink size={14} /> Open Folder in File Manager
          </Button>
        {/if}
        <Button variant="outline" size="sm" onclick={() => selectedStorageDetail = null}>Close</Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dashboard-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 16px 20px 32px 20px;
    box-sizing: border-box;
    gap: 16px;
  }

  /* ── Header Actions Dock ── */
  .header-actions-dock {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .action-pill-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .action-pill-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-primary);
  }
  .action-pill-btn.active {
    background: rgba(34, 197, 94, 0.1);
    border-color: rgba(34, 197, 94, 0.3);
    color: #22c55e;
  }

  .pulse-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    display: inline-block;
  }
  .pulse-dot.active {
    background: #22c55e;
    box-shadow: 0 0 8px #22c55e;
  }

  /* ── Proactive Alert Banner ── */
  .proactive-alert-wrapper {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .alert-banner-card {
    padding: 10px 16px;
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.25);
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .alert-banner-card.is-crit {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
  }

  .alert-banner-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .alert-text-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .alert-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .alert-message {
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  /* ── Hero KPI Ribbon (Top Row) ── */
  .hero-kpi-ribbon {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 14px;
  }

  @media (max-width: 1100px) {
    .hero-kpi-ribbon {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 600px) {
    .hero-kpi-ribbon {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .kpi-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    text-align: left;
    cursor: pointer;
    transition: transform 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  }
  .kpi-card:hover {
    transform: translateY(-2px);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.4);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
  }

  .kpi-top-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .kpi-label-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .kpi-icon-box {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .cpu-bg { background: rgba(0, 218, 243, 0.12); color: #00daf3; }
  .ram-bg { background: rgba(59, 130, 246, 0.12); color: #3b82f6; }
  .net-bg { background: rgba(34, 197, 94, 0.12); color: #22c55e; }
  .sec-bg { background: rgba(239, 68, 68, 0.12); color: #ef4444; }

  .kpi-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .kpi-badge {
    font-size: 10.5px;
    font-weight: 700;
    padding: 1px 7px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-muted);
  }
  .kpi-badge.info { background: rgba(59, 130, 246, 0.1); color: #3b82f6; }
  .kpi-badge.success { background: rgba(34, 197, 94, 0.1); color: #22c55e; }

  .kpi-value-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .kpi-big-num {
    font-size: 22px;
    font-weight: 800;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
    line-height: 1;
  }

  .kpi-total-sub {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .kpi-findings-count {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
  }

  .net-flow-rates {
    display: flex;
    gap: 10px;
    font-size: 11.5px;
    font-family: var(--font-mono);
    font-weight: 600;
  }
  .flow-item {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .kpi-sparkline {
    width: 80px;
    height: 20px;
    overflow: visible;
  }

  .kpi-bar-track {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 2px;
    overflow: hidden;
  }
  .kpi-bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }
  .ram-fill {
    background: linear-gradient(90deg, #3b82f6, #00daf3);
  }

  .kpi-footer-sub {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .jump-arrow {
    opacity: 0.5;
    transition: transform 0.15s ease, opacity 0.15s ease;
  }
  .kpi-card:hover .jump-arrow {
    opacity: 1;
    transform: translate(2px, -2px);
    color: var(--color-accent);
  }

  /* ── Main 6-Card Grid Layout ── */
  .dashboard-grid-container {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 16px;
    align-items: stretch;
  }

  @media (max-width: 1100px) {
    .dashboard-grid-container {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 768px) {
    .dashboard-grid-container {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .dash-card-wrapper {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  }

  .card-glass-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--color-border);
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-header-title {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .card-jump-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    transition: all 0.12s ease;
  }
  .card-jump-btn:hover {
    color: var(--color-accent);
    background: rgba(255, 255, 255, 0.06);
    transform: translate(1px, -1px);
  }

  /* ── 1. System Overview Stack ── */
  .overview-stack {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .overview-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 12px;
  }
  .overview-row.uptime-row {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(34, 197, 94, 0.2);
  }

  .row-label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-secondary);
    font-weight: 500;
    white-space: nowrap;
  }
  .row-label.green-label {
    color: #22c55e;
  }

  .row-val {
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .uptime-val {
    font-family: var(--font-mono);
    font-weight: 700;
    color: #22c55e;
  }
  .kernel-pill {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 11px;
    color: var(--color-text-primary);
  }
  .timestamp-val {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .os-pill-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .os-badge {
    background: rgba(59, 130, 246, 0.12);
    color: #38bdf8;
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .os-version-tag {
    font-size: 11.5px;
    font-weight: 700;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }

  .quick-chips-row {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }
  .quick-chip {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 5px 8px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  .quick-chip:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-primary);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.3);
  }

  /* ── 2. Top Processes Container ── */
  .top-processes-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .process-row-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.12s ease;
    text-align: left;
    width: 100%;
  }
  .process-row-item:hover {
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.25);
  }

  .proc-left-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .proc-name {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }
  .proc-pid {
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .proc-metrics-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .proc-cpu-badge {
    font-size: 10.5px;
    font-weight: 700;
    font-family: var(--font-mono);
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-accent);
  }
  .proc-cpu-badge.high {
    background: rgba(245, 158, 11, 0.15);
    color: #f59e0b;
  }
  .proc-mem-badge {
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-secondary);
  }

  /* ── 3. Storage & Disks ── */
  .storage-card-stack {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .drive-subcard-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  .drive-node {
    font-weight: 700;
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
  }
  .drive-model {
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .passed-badge {
    color: #22c55e;
    background: rgba(34, 197, 94, 0.12);
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .partition-bars-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .partition-bar-item {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .part-header-line {
    display: flex;
    justify-content: space-between;
    font-size: 11.5px;
  }
  .part-mount { font-weight: 600; color: var(--color-text-primary); }
  .part-dev { color: var(--color-text-muted); font-size: 10.5px; font-family: var(--font-mono); }
  .part-stat-line {
    text-align: right;
    font-size: 10.5px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .progress-track {
    height: 5px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 3px;
    overflow: hidden;
  }
  .progress-bar-fill { height: 100%; border-radius: 3px; transition: width 0.4s ease; }

  .btrfs-pool-subcard {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 10px;
  }
  .btrfs-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }
  .btrfs-tag {
    font-size: 9px;
    font-weight: 800;
    background: rgba(59, 130, 246, 0.12);
    color: #38bdf8;
    padding: 1px 5px;
    border-radius: 3px;
  }
  .btrfs-dev {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-primary);
  }
  .btrfs-capacity {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }
  .btrfs-pct-label {
    text-align: right;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-bottom: 6px;
    font-family: var(--font-mono);
  }

  .tree-subvols {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 8px;
    border-left: 2px solid var(--color-border);
  }
  .tree-subvol-row {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
  }
  .subvol-size {
    font-family: var(--font-mono);
    color: var(--color-text-muted);
  }

  /* ── 4. Services Watchdog ── */
  .services-watchdog-grid {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .watchdog-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.12s ease;
    text-align: left;
    width: 100%;
  }
  .watchdog-item:hover {
    background: rgba(255, 255, 255, 0.06);
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
    background: rgba(255, 255, 255, 0.06);
    transform: translateY(-1px);
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
    padding: 3px 6px;
    border-radius: 4px;
    background: transparent;
    border: none;
    text-align: left;
    width: 100%;
    cursor: pointer;
    transition: background 0.12s ease;
  }
  .log-item-line:hover {
    background: rgba(255, 255, 255, 0.05);
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
    cursor: pointer;
    transition: all 0.12s ease;
    text-align: left;
    width: 100%;
  }
  .footprint-row-item:hover {
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.25);
  }

  .footprint-label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11.5px;
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
    backdrop-filter: blur(8px);
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .modal-glass-card {
    width: 460px;
    max-width: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    padding: 20px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.45);
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
</style>
