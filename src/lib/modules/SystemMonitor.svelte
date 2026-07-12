<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import {
    Activity, Cpu, Database, HardDrive, TerminalSquare, AlertCircle,
    RefreshCw, Play, Trash2, PowerOff, Loader, CheckCircle2, ChevronRight
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';

  let currentTab = $state<'overview' | 'processes'>('overview');

  // Overview Stats
  let stats = $state<any>(null);
  let disks = $state<any[]>([]);
  let cpuHistory = $state<number[]>(Array(40).fill(0));
  let ramHistory = $state<number[]>(Array(40).fill(0));

  // Processes
  let processes = $state<any[]>([]);
  let processSearch = $state('');
  let isRefreshing = $state(false);

  // Auto-refresh timer
  let refreshTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    await refreshAll();
    refreshTimer = setInterval(() => {
      if (currentTab === 'overview') {
        refreshAll();
      }
    }, 2000);
  });

  onDestroy(() => {
    clearInterval(refreshTimer);
  });

  async function refreshAll() {
    isRefreshing = true;
    try {
      if (currentTab === 'overview') {
        const [sysStats, sysDisks] = await Promise.all([
          invoke('get_system_stats'),
          invoke('get_disk_usage')
        ]);
        stats = sysStats;
        disks = sysDisks as any[];

        cpuHistory = [...cpuHistory.slice(1), stats.cpu_percent];
        ramHistory = [...ramHistory.slice(1), stats.ram_percent];
      } else {
        processes = await invoke('get_process_list');
      }
    } catch (e) {
      console.error(e);
    } finally {
      isRefreshing = false;
    }
  }

  $effect(() => {
    // Immediate refresh on tab change
    refreshAll();
  });

  // Sparkline generator helper
  function generateSparkline(data: number[], height: number, width: number): string {
    if (data.length === 0) return '';
    const max = 100;
    const points = data.map((val, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - (val / max) * height;
      return `${x},${y}`;
    }).join(' ');
    
    // Fill path
    return `M 0,${height} L ${points} L ${width},${height} Z`;
  }
  function generateSparklineStroke(data: number[], height: number, width: number): string {
    if (data.length === 0) return '';
    const max = 100;
    return data.map((val, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - (val / max) * height;
      return `${x},${y}`;
    }).join(' ');
  }

  // Formatting helpers
  const formatBytes = (mb: number) => {
    if (mb < 1024) return `${mb.toFixed(0)} MB`;
    return `${(mb / 1024).toFixed(1)} GB`;
  };
  const formatTime = (secs: number) => {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  };

  async function killProcess(pid: number, name: string) {
    if (!statusStore.rootEnabled) {
      uiStore.addToast('Root required to kill processes', 'error');
      return;
    }
    const confirmed = await uiStore.confirm(
      `Kill ${name}?`,
      `Are you sure you want to terminate process ${pid}? Data may be lost.`,
      true
    );
    if (!confirmed) return;

    try {
      const res = await invoke('kill_process', { pid, signal: 15 });
      statusStore.setLastCommand(`kill -15 ${pid}`, 0, true);
      uiStore.addToast(res as string, 'success');
      refreshAll();
    } catch (e: any) {
      statusStore.setLastCommand(`kill -15 ${pid}`, 1, false);
      uiStore.addToast(e, 'error');
    }
  }

  let filteredProcesses = $derived(
    processSearch
      ? processes.filter(p => p.name.toLowerCase().includes(processSearch.toLowerCase()) || p.pid.toString().includes(processSearch))
      : processes
  );
</script>

<div class="module-page">
  <PageHeader title="System Monitor" subtitle="Real-time system health and process management." icon={Activity}>
    <div class="tab-switcher">
      <button class:active={currentTab === 'overview'} onclick={() => currentTab = 'overview'}>Overview</button>
      <button class:active={currentTab === 'processes'} onclick={() => currentTab = 'processes'}>Processes</button>
    </div>
    <Button onclick={refreshAll} variant="secondary">
      <RefreshCw size={14} class={isRefreshing ? 'animate-spin-slow' : ''} />
      Refresh
    </Button>
  </PageHeader>

  <div class="page-content">
    {#if currentTab === 'overview'}
      {#if stats}
        <!-- Core Stats Row -->
        <div class="stats-grid">
          <!-- CPU Card -->
          <div class="stat-card">
            <div class="card-header">
              <div class="card-title"><Cpu size={16} /> CPU Usage</div>
              <div class="card-val">{stats.cpu_percent.toFixed(1)}%</div>
            </div>
            <div class="sparkline-container">
              <svg viewBox="0 0 200 40" preserveAspectRatio="none" class="sparkline-svg">
                <path d={generateSparkline(cpuHistory, 40, 200)} class="spark-fill cpu-fill" />
                <polyline points={generateSparklineStroke(cpuHistory, 40, 200)} class="spark-stroke cpu-stroke" fill="none" />
              </svg>
            </div>
            <div class="card-meta">
              <span>{stats.cpu_cores} Cores</span>
              <span>Load: {stats.load_1.toFixed(2)}, {stats.load_5.toFixed(2)}</span>
            </div>
          </div>

          <!-- RAM Card -->
          <div class="stat-card">
            <div class="card-header">
              <div class="card-title"><Database size={16} /> Memory</div>
              <div class="card-val">{stats.ram_percent.toFixed(1)}%</div>
            </div>
            <div class="sparkline-container">
              <svg viewBox="0 0 200 40" preserveAspectRatio="none" class="sparkline-svg">
                <path d={generateSparkline(ramHistory, 40, 200)} class="spark-fill ram-fill" />
                <polyline points={generateSparklineStroke(ramHistory, 40, 200)} class="spark-stroke ram-stroke" fill="none" />
              </svg>
            </div>
            <div class="card-meta">
              <span>{formatBytes(stats.ram_used_mb)} / {formatBytes(stats.ram_total_mb)}</span>
              <span>Swap: {stats.swap_percent.toFixed(1)}%</span>
            </div>
          </div>

          <!-- Uptime Card -->
          <div class="stat-card uptime-card">
            <div class="card-header">
              <div class="card-title"><Activity size={16} /> Uptime</div>
              <div class="card-val uptime-val">{formatTime(stats.uptime_seconds)}</div>
            </div>
            <div class="uptime-visual">
              <div class="pulse-ring"></div>
              <span>System Online</span>
            </div>
          </div>
        </div>

        <!-- Disk Usage -->
        <h3 class="section-title"><HardDrive size={16} /> Mounted Disks</h3>
        <div class="disk-grid">
          {#each disks as disk}
            <div class="disk-card">
              <div class="disk-header">
                <span class="disk-mount">{disk.mount}</span>
                <span class="disk-pct {disk.percent > 90 ? 'pct-danger' : disk.percent > 75 ? 'pct-warn' : ''}">
                  {disk.percent.toFixed(1)}%
                </span>
              </div>
              <div class="progress-track">
                <div class="progress-bar {disk.percent > 90 ? 'bg-danger' : disk.percent > 75 ? 'bg-warn' : 'bg-primary'}" 
                     style="width: {Math.min(disk.percent, 100)}%"></div>
              </div>
              <div class="disk-meta">
                <span>{disk.device} • {disk.fs_type}</span>
                <span>{disk.free_gb.toFixed(1)} GB free of {disk.total_gb.toFixed(1)} GB</span>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="loading-state">
          <Loader size={24} class="animate-spin-slow" /> Loading system metrics...
        </div>
      {/if}

    {:else}
      <!-- Processes Tab -->
      <div class="table-container">
        <div class="table-toolbar">
          <div class="search-box">
            <input type="text" placeholder="Search processes by name or PID..." bind:value={processSearch} />
          </div>
          <div class="toolbar-stats">
            Showing top {filteredProcesses.length} processes
          </div>
        </div>

        <div class="table-scroll">
          <table class="process-table">
            <thead>
              <tr>
                <th class="col-pid">PID</th>
                <th class="col-name">Name</th>
                <th class="col-user">User</th>
                <th class="col-cpu">CPU %</th>
                <th class="col-mem">Mem %</th>
                <th class="col-rss">RSS (MB)</th>
                <th class="col-actions"></th>
              </tr>
            </thead>
            <tbody>
              {#each filteredProcesses as p (p.pid)}
                <tr class:is-root={p.user === 'root'} class:is-kernel={p.pid <= 100}>
                  <td class="col-pid">{p.pid}</td>
                  <td class="col-name">
                    <div class="proc-name">{p.name}</div>
                    <div class="proc-cmd">{p.cmdline}</div>
                  </td>
                  <td class="col-user"><span class="user-badge">{p.user}</span></td>
                  <td class="col-cpu {p.cpu_percent > 20 ? 'text-warn' : p.cpu_percent > 50 ? 'text-danger' : ''}">
                    {p.cpu_percent.toFixed(1)}
                  </td>
                  <td class="col-mem {p.mem_percent > 15 ? 'text-warn' : ''}">
                    {p.mem_percent.toFixed(1)}
                  </td>
                  <td class="col-rss">{p.mem_rss_mb.toFixed(1)}</td>
                  <td class="col-actions">
                    {#if p.pid > 100}
                      <button class="action-btn kill" onclick={() => killProcess(p.pid, p.name)} title="Kill Process (SIGTERM)">
                        <PowerOff size={14} />
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Base Layout */
  .module-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
  }
  .page-content {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* Tab Switcher */
  .tab-switcher {
    display: flex;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 3px;
    border: 1px solid rgba(255, 255, 255, 0.05);
    margin-right: 8px;
  }
  .tab-switcher button {
    background: transparent;
    border: none;
    padding: 6px 14px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-muted);
    border-radius: 5px;
    cursor: pointer;
    transition: all 0.2s;
  }
  .tab-switcher button:hover {
    color: var(--color-text-primary);
  }
  .tab-switcher button.active {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
  }

  /* Loading */
  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 200px;
    color: var(--color-text-muted);
    font-size: 14px;
  }

  /* ── OVERVIEW TAB ──────────────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 20px;
  }
  
  .stat-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.1);
  }
  
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }
  .card-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }
  .card-val {
    font-size: 24px;
    font-weight: 700;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
    line-height: 1;
  }
  
  .sparkline-container {
    height: 40px;
    width: 100%;
    margin-top: 4px;
  }
  .sparkline-svg {
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .spark-stroke { stroke-width: 2px; stroke-linejoin: round; stroke-linecap: round; }
  
  .cpu-stroke { stroke: var(--color-info); }
  .cpu-fill { fill: var(--color-info); opacity: 0.15; }
  .ram-stroke { stroke: var(--color-accent); }
  .ram-fill { fill: var(--color-accent); opacity: 0.15; }
  
  .card-meta {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--color-text-muted);
    border-top: 1px dashed rgba(255, 255, 255, 0.05);
    padding-top: 10px;
  }

  .uptime-card {
    justify-content: space-between;
  }
  .uptime-val { font-size: 20px; }
  .uptime-visual {
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(0, 210, 211, 0.05);
    border: 1px solid rgba(0, 210, 211, 0.1);
    padding: 12px 16px;
    border-radius: 8px;
    color: var(--color-success);
    font-weight: 600;
    font-size: 13px;
  }
  .pulse-ring {
    width: 10px;
    height: 10px;
    background: var(--color-success);
    border-radius: 50%;
    box-shadow: 0 0 0 rgba(0, 210, 211, 0.4);
    animation: pulse 2s infinite;
  }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 rgba(0, 210, 211, 0.4); }
    70% { box-shadow: 0 0 0 6px rgba(0, 210, 211, 0); }
    100% { box-shadow: 0 0 0 0 rgba(0, 210, 211, 0); }
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin: 8px 0 0;
  }

  .disk-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 16px;
  }
  .disk-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 16px;
  }
  .disk-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 12px;
  }
  .disk-mount { font-weight: 600; font-family: var(--font-mono); font-size: 13px; }
  .disk-pct { font-weight: 700; font-size: 13px; font-family: var(--font-mono); }
  .pct-danger { color: var(--color-error); }
  .pct-warn { color: var(--color-warning); }

  .progress-track {
    height: 6px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 3px;
    overflow: hidden;
    margin-bottom: 12px;
  }
  .progress-bar {
    height: 100%;
    border-radius: 3px;
    transition: width 0.5s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .bg-primary { background: var(--color-accent); }
  .bg-warn { background: var(--color-warning); }
  .bg-danger { background: var(--color-error); }

  .disk-meta {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  /* ── PROCESSES TAB ─────────────────────────────────────────── */
  .table-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    min-height: 0;
  }
  .table-toolbar {
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(0,0,0,0.1);
  }
  .search-box input {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    width: 300px;
    outline: none;
    transition: all 0.2s;
  }
  .search-box input:focus { border-color: var(--color-accent); }
  .toolbar-stats { font-size: 12px; color: var(--color-text-muted); }

  .table-scroll {
    flex: 1;
    overflow-y: auto;
  }
  .process-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .process-table th {
    text-align: left;
    padding: 10px 16px;
    font-weight: 600;
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
    position: sticky;
    top: 0;
    background: var(--color-bg-card);
    z-index: 10;
  }
  .process-table td {
    padding: 10px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.02);
    vertical-align: middle;
  }
  .process-table tr:hover td { background: rgba(255, 255, 255, 0.02); }
  
  .is-root td { opacity: 0.8; }
  .is-kernel td { opacity: 0.5; }

  .col-pid { width: 80px; font-family: var(--font-mono); color: var(--color-text-muted); }
  .col-name { max-width: 300px; }
  .col-user { width: 120px; }
  .col-cpu, .col-mem, .col-rss { width: 90px; font-family: var(--font-mono); }
  .col-actions { width: 60px; text-align: right; }

  .proc-name { font-weight: 600; color: var(--color-text-primary); margin-bottom: 2px; }
  .proc-cmd { 
    font-size: 10px; color: var(--color-text-muted); font-family: var(--font-mono);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .user-badge {
    background: rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
  }
  .is-root .user-badge {
    background: rgba(255, 118, 117, 0.1);
    color: var(--color-error);
  }

  .text-warn { color: var(--color-warning) !important; font-weight: 600; }
  .text-danger { color: var(--color-error) !important; font-weight: 700; }

  .action-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    display: inline-flex;
    transition: all 0.2s;
  }
  .action-btn.kill:hover {
    background: rgba(255, 118, 117, 0.15);
    color: var(--color-error);
  }
</style>
