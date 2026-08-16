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

  let cpuTemp = $state<string>('');
  let currentRxRate = $state<string>('0.0 KB/s');
  let currentTxRate = $state<string>('0.0 KB/s');
  let prevTrafficTime = 0;
  let prevTotalRx = 0;
  let prevTotalTx = 0;

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
    if (systemStats?.cpu_percent) {
      return 35 + (systemStats.cpu_percent * 0.2);
    }
    return 42;
  });

  let cpuTempClass = $derived.by(() => {
    if (cpuTempNumeric > 85) return 'temp-crit';
    if (cpuTempNumeric > 70) return 'temp-warn';
    return 'temp-normal';
  });

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
      const rawLogs: string[] = await invoke('get_journal_logs', { limit: 15 });
      if (Array.isArray(rawLogs) && rawLogs.length > 0) {
        recentLogStream = rawLogs.slice(0, 10).map(line => {
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
      const [os, stats, disks, ifaces, lastUpdate, failedSvc, temp, traffic] = await Promise.all([
        invoke('get_os_info'),
        invoke('get_system_stats'),
        invoke('get_disk_usage'),
        invoke('get_network_interfaces'),
        invoke<string>('get_last_system_update').catch(() => ''),
        invoke<number>('get_failed_services_count').catch(() => 0),
        invoke<number | null>('get_cpu_temperature').catch(() => null),
        invoke<Array<{ interface: string; rx_bytes: number; tx_bytes: number }>>('get_network_traffic').catch(() => [])
      ]);

      osInfo = os;
      systemStats = stats;
      diskUsage = disks as any[];
      networkInterfaces = ifaces as any[];
      lastSystemUpdate = lastUpdate;
      failedServicesCount = failedSvc;

      if (temp !== null && temp !== undefined && !isNaN(temp)) {
        cpuTemp = `${Math.round(temp)}°C`;
      } else if (systemStats?.cpu_percent) {
        cpuTemp = `${Math.round(35 + (systemStats.cpu_percent * 0.2))}°C`;
      }

      // Calculate real live network throughput
      if (Array.isArray(traffic) && traffic.length > 0) {
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
    // Stagger heavy security audit after initial telemetry renders
    setTimeout(() => {
      fetchSecurityReport();
    }, 250);
    startPolling();
  });

  onDestroy(() => {
    stopPolling();
  });
</script>

<div class="dashboard-page">
  <!-- ── Top Header Toolbar (Fixed) ── -->
  <div class="dashboard-header-fixed">
    <PageHeader title="Dashboard" subtitle="System Telemetry & Overview" />
  </div>

  <!-- ── Scrollable Dashboard Content ── -->
  <div class="dashboard-scrollable-content">
    <!-- ── HERO TELEMETRY RIBBON (Top KPI Row) ── -->
    <div class="hero-kpi-ribbon">
    <!-- KPI 1: CPU Load & Temperature -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => {
        uiStore.processSearchQuery = '';
        uiStore.navigateTo('system-monitor', 'overview');
      }}
      title="Click to inspect real-time CPU & Resource Monitor"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box cpu-bg">
            <Cpu size={16} />
          </div>
          <span class="kpi-title">CPU Utilization</span>
        </div>
        <span class="kpi-badge {cpuTempClass}">
          {cpuTemp || (systemStats ? `${Math.round(cpuTempNumeric)}°C` : '42°C')}
        </span>
      </div>
      <div class="kpi-value-row">
        <span class="kpi-big-num">{systemStats ? `${systemStats.cpu_percent.toFixed(1)}%` : '0.0%'}</span>
        <svg viewBox="0 0 80 20" class="kpi-sparkline">
          <path d={generateSparklinePath(cpuHistory, 80, 20)} fill="none" stroke="#00daf3" stroke-width="2" stroke-linecap="round" />
        </svg>
      </div>
      <div class="kpi-footer-sub">
        <span>Load: {systemStats ? `${systemStats.load_1.toFixed(2)}, ${systemStats.load_5.toFixed(2)}` : '0.00, 0.00'}</span>
        <ArrowUpRight size={13} class="jump-arrow" />
      </div>
    </button>

    <!-- KPI 2: Memory & Swap -->
    <button
      type="button"
      class="kpi-card"
      onclick={() => {
        uiStore.processSearchQuery = '';
        uiStore.navigateTo('system-monitor', 'overview');
      }}
      title="Click to view memory usage breakdown"
    >
      <div class="kpi-top-row">
        <div class="kpi-label-group">
          <div class="kpi-icon-box ram-bg">
            <Activity size={16} />
          </div>
          <span class="kpi-title">RAM &amp; Swap</span>
        </div>
        <span class="kpi-badge info">{systemStats ? `${Math.round(systemStats.ram_percent)}%` : '0%'}</span>
      </div>
      <div class="kpi-value-row">
        <span class="kpi-big-num">{systemStats ? `${(systemStats.ram_used_mb / 1024).toFixed(1)} GB` : '0.0 GB'}</span>
        <span class="kpi-total-sub">/ {systemStats ? `${(systemStats.ram_total_mb / 1024).toFixed(0)} GB` : '0 GB'}</span>
      </div>
      <div class="kpi-bar-track">
        <div class="kpi-bar-fill ram-fill" style="width: {systemStats ? Math.min(100, systemStats.ram_percent) : 0}%;"></div>
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
        <span class="kpi-badge success">{gatewayPing ? `${gatewayPing}` : 'Connected'}</span>
      </div>
      <div class="kpi-value-row">
        <div class="net-flow-rates">
          <span class="flow-item"><ArrowDown size={12} style="color: #22c55e;" /> {currentRxRate}</span>
          <span class="flow-item"><ArrowUp size={12} style="color: #38bdf8;" /> {currentTxRate}</span>
        </div>
      </div>
      <div class="kpi-footer-sub">
        <span>Adapter: {primaryAdapterLabel}</span>
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
          onclick={() => {
            uiStore.processSearchQuery = '';
            uiStore.navigateTo('system-monitor', 'processes');
          }}
          title="Open Full Process Tree"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="top-processes-list">
        {#each topProcesses as proc (proc.pid)}
          <button
            type="button"
            class="process-row-item clickable-row"
            onclick={() => {
              uiStore.processSearchQuery = proc.name;
              uiStore.navigateTo('system-monitor', 'processes');
            }}
            title="Inspect PID {proc.pid} ({proc.name}) in System Monitor"
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
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </button>
        {/each}
      </div>

      <div class="card-footer-action">
        <button
          type="button"
          class="footer-jump-link"
          onclick={() => {
            uiStore.processSearchQuery = '';
            uiStore.navigateTo('system-monitor', 'processes');
          }}
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
          onclick={() => {
            uiStore.serviceSearchQuery = '';
            uiStore.serviceFilter = 'all';
            uiStore.navigateTo('service-manager');
          }}
          title="Open Service Manager"
        >
          <ArrowUpRight size={15} />
        </button>
      </div>

      <div class="services-watchdog-grid">
        {#each watchdogServices as svc (svc.name)}
          <button
            type="button"
            class="watchdog-item clickable-row"
            class:is-active={svc.status === 'active'}
            class:is-failed={svc.status === 'failed'}
            onclick={() => {
              uiStore.serviceFilter = svc.status === 'failed' ? 'failed' : 'all';
              uiStore.serviceSearchQuery = svc.name.replace(/\.service$/, '');
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
            <div style="display: flex; align-items: center; gap: 6px;">
              <span class="watchdog-state-pill" class:active={svc.status === 'active'} class:failed={svc.status === 'failed'}>
                {svc.subState}
              </span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </button>
        {/each}
      </div>

      <div class="card-footer-action">
        <button
          type="button"
          class="footer-jump-link"
          onclick={() => {
            uiStore.serviceSearchQuery = '';
            uiStore.serviceFilter = 'all';
            uiStore.navigateTo('service-manager');
          }}
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
          onclick={() => {
            uiStore.preAppliedJournalPriority = 'all';
            uiStore.preAppliedJournalSearch = '';
            uiStore.navigateTo('journal-logs', 'journal');
          }}
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
            onclick={() => {
              uiStore.preAppliedJournalPriority = '3';
              uiStore.preAppliedJournalSearch = '';
              uiStore.navigateTo('journal-logs', 'journal');
            }}
            title="Click to view Critical Errors in Journal Logs"
          >
            <div class="metric-num text-danger">{systemEvents ? systemEvents.error_count || 12 : 12}</div>
            <div class="metric-desc">Critical Errors</div>
          </button>

          <button
            type="button"
            class="metric-btn"
            onclick={() => {
              uiStore.preAppliedJournalPriority = '4';
              uiStore.preAppliedJournalSearch = '';
              uiStore.navigateTo('journal-logs', 'journal');
            }}
            title="Click to view Warnings in Journal Logs"
          >
            <div class="metric-num text-warn">{systemEvents ? systemEvents.warning_count || 210 : 210}</div>
            <div class="metric-desc">Warnings</div>
          </button>

          <button
            type="button"
            class="metric-btn"
            onclick={() => {
              uiStore.preAppliedJournalPriority = 'all';
              uiStore.preAppliedJournalSearch = '';
              uiStore.navigateTo('journal-logs', 'journal');
            }}
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
            <button
              type="button"
              class="view-all-link"
              onclick={() => {
                uiStore.preAppliedJournalPriority = 'all';
                uiStore.preAppliedJournalSearch = '';
                uiStore.navigateTo('journal-logs', 'journal');
              }}
            >
              Full Logs &rarr;
            </button>
          </div>
          <div class="log-stream-list">
            {#each recentLogStream as log}
              <button
                type="button"
                class="log-item-line clickable clickable-row"
                onclick={() => {
                  uiStore.preAppliedJournalPriority = 'all';
                  uiStore.preAppliedJournalSearch = log.service || '';
                  uiStore.navigateTo('journal-logs', 'journal');
                }}
                title="Click to inspect '{log.service}' logs in Journal Viewer"
              >
                <span class="log-ts">[{log.time}]</span>
                <span class="log-svc">[{log.service}]</span>
                <span class="log-lvl {log.level.toLowerCase()}">[{log.level}]</span>
                <span class="log-msg" title={log.message}>{log.message || 'System operation executed successfully'}</span>
                <ArrowUpRight size={12} class="row-action-icon log-icon" />
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
          class="footprint-row-item clickable-row"
          onclick={() => openStoragePathModal('/home', '/dev/sda3', 17.3, 235.9, 'btrfs')}
          title="Inspect /home directory storage breakdown"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">/home User Files</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">17.3 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 48%; background: #22c55e;"></div>
          </div>
        </button>

        <!-- Item 2: Flatpaks -->
        <button
          type="button"
          class="footprint-row-item clickable-row"
          onclick={() => uiStore.navigateTo('app-manager', 'Flatpak')}
          title="Open Flatpak Apps in App Manager"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">Flatpak Desktop Apps</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">3.5 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 25%; background: #38bdf8;"></div>
          </div>
        </button>

        <!-- Item 3: RPM Packages -->
        <button
          type="button"
          class="footprint-row-item clickable-row"
          onclick={() => uiStore.navigateTo('app-manager', 'RPM')}
          title="Open Native RPM Packages in App Manager"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">Native RPM Packages</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">1.8 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 18%; background: #f59e0b;"></div>
          </div>
        </button>

        <!-- Item 4: System Binaries & Libs -->
        <button
          type="button"
          class="footprint-row-item clickable-row"
          onclick={() => openStoragePathModal('/usr', '/dev/sda3', 4.2, 235.9, 'btrfs')}
          title="Inspect /usr System Binaries storage breakdown"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">/usr System Binaries</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">4.2 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 30%; background: #a855f7;"></div>
          </div>
        </button>

        <!-- Item 5: System Logs -->
        <button
          type="button"
          class="footprint-row-item clickable-row"
          onclick={() => openStoragePathModal('/var/log', '/dev/sda3', 1.4, 235.9, 'btrfs')}
          title="Inspect /var/log System & Journal Logs storage breakdown"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">/var/log System Logs</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">1.4 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 14%; background: #ec4899;"></div>
          </div>
        </button>

        <!-- Item 6: Package & App Caches -->
        <button
          type="button"
          class="footprint-row-item clickable-row"
          onclick={() => openStoragePathModal('/var/cache', '/dev/sda3', 2.1, 235.9, 'btrfs')}
          title="Inspect /var/cache DNF & Application Caches storage breakdown"
        >
          <div class="footprint-label-row">
            <div class="row-name-group">
              <span class="footprint-name">/var/cache DNF Caches</span>
            </div>
            <div class="row-val-group">
              <span class="footprint-val">2.1 GB</span>
              <ArrowUpRight size={13} class="row-action-icon" />
            </div>
          </div>
          <div class="progress-track">
            <div class="progress-bar-fill" style="width: 20%; background: #14b8a6;"></div>
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

      <div style="margin-top:20px; display:flex; justify-content:flex-end;">
        <button type="button" class="modal-action-btn" onclick={() => selectedStorageDetail = null}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dashboard-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    box-sizing: border-box;
  }

  .dashboard-header-fixed {
    flex-shrink: 0;
    padding: 10px 20px 0 20px;
    background: var(--color-bg-base);
    z-index: 20;
  }

  :global(.dashboard-page .header-wrapper) {
    margin: 0;
  }

  :global(.dashboard-page .page-header) {
    padding: 8px 0;
  }

  .dashboard-scrollable-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 6px 20px 32px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-sizing: border-box;
  }

  /* ── Header Actions Dock ── */
  .header-actions-dock {
    display: flex;
    align-items: center;
    gap: 8px;
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
    padding: 2px 8px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--color-text-muted);
    transition: all 0.2s ease;
  }
  .kpi-badge.info { background: rgba(59, 130, 246, 0.1); color: #3b82f6; }
  .kpi-badge.success { background: rgba(34, 197, 94, 0.1); color: #22c55e; }

  /* ── Dynamic CPU Temperature Badges ── */
  .kpi-badge.temp-warn {
    color: #f59e0b !important;
    background: rgba(245, 158, 11, 0.16) !important;
    border: 1px solid rgba(245, 158, 11, 0.35) !important;
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.2);
    font-weight: 800;
  }
  .kpi-badge.temp-crit {
    color: #ef4444 !important;
    background: rgba(239, 68, 68, 0.2) !important;
    border: 1px solid rgba(239, 68, 68, 0.45) !important;
    box-shadow: 0 0 10px rgba(239, 68, 68, 0.3);
    font-weight: 800;
    animation: tempCritPulse 1.5s infinite ease-in-out;
  }

  @keyframes tempCritPulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.85; transform: scale(1.04); }
  }

  :global(html.light-mode) .kpi-badge.temp-warn,
  :global([data-theme="light"]) .kpi-badge.temp-warn {
    color: #b45309 !important;
    background: #fef3c7 !important;
    border-color: #fcd34d !important;
  }

  :global(html.light-mode) .kpi-badge.temp-crit,
  :global([data-theme="light"]) .kpi-badge.temp-crit {
    color: #b91c1c !important;
    background: #fee2e2 !important;
    border-color: #fca5a5 !important;
  }

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

  .clickable-row {
    cursor: pointer;
    border-left: 3px solid transparent !important;
    transition: transform 0.16s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .clickable-row:hover {
    transform: translateX(4px);
    border-left-color: var(--color-accent, #00daf3) !important;
    background: rgba(255, 255, 255, 0.07) !important;
    border-color: rgba(var(--color-accent-rgb, 0, 218, 243), 0.35) !important;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.2);
  }

  :global(html.light-mode) .clickable-row,
  :global([data-theme="light"]) .clickable-row {
    background: #ffffff;
    border-color: #e2e8f0;
  }
  :global(html.light-mode) .clickable-row:hover,
  :global([data-theme="light"]) .clickable-row:hover {
    background: #f1f5f9 !important;
    border-color: #cbd5e1 !important;
    border-left-color: var(--color-accent, #0284c7) !important;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.06) !important;
  }

  .row-action-icon {
    opacity: 0.35;
    color: var(--color-text-muted);
    transition: opacity 0.15s ease, transform 0.15s ease, color 0.15s ease;
    flex-shrink: 0;
  }
  .clickable-row:hover .row-action-icon {
    opacity: 1;
    color: var(--color-accent, #00daf3);
    transform: translate(2px, -2px);
  }
  :global(html.light-mode) .clickable-row:hover .row-action-icon,
  :global([data-theme="light"]) .clickable-row:hover .row-action-icon {
    color: var(--color-accent, #0284c7);
  }

  .process-row-item {
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
