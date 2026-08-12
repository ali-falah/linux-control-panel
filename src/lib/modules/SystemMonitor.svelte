<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { Activity, Cpu, Database, HardDrive, TerminalSquare } from '@lucide/svelte';
  import { RefreshCw, Skull, Loader, Wifi, Play, Pause } from '@lucide/svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Table from '../components/ui/Table.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import { tableFeatures } from '../actions/tableFeatures';

  let currentTab = $state<'overview' | 'processes'>('overview');
  let currentUser = $state('unknown');

  // Overview Stats
  let stats = $state<any>(null);
  let cpuTemp = $state<number | null>(null);
  let cpuHistory = $state<number[]>(Array(40).fill(0));
  let ramHistory = $state<number[]>(Array(40).fill(0));
  let swapHistory = $state<number[]>(Array(40).fill(0));

  // Disk I/O
  let diskIoStats = $state<any[]>([]);
  let lastDiskIoTime = $state<number>(0);
  let lastDiskIoData = $state<any[]>([]);
  let diskIoSpeeds = $state<Record<string, { readSpeed: number, writeSpeed: number }>>({});
  let diskIoHistory = $state<Record<string, number[]>>({});

  // Network Live Traffic
  let networkTraffic = $state<any[]>([]);
  let lastNetworkTime = $state<number>(0);
  let lastNetworkData = $state<any[]>([]);
  let networkSpeeds = $state<Record<string, { rxSpeed: number, txSpeed: number }>>({});
  let networkInterfaces = $state<any[]>([]);

  // Active Connections
  let activeConnections = $state<any[]>([]);

  // Processes
  let processes = $state<any[]>([]);
  let processSearch = $state('');
  let isRefreshing = $state(false);
  let isPaused = $state(false);

  // Context Menu State for Active Connections
  let contextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    conn: any | null;
  }>({ x: 0, y: 0, show: false, conn: null });

  // Side Drawer details State
  let isDrawerOpen = $state(false);
  let selectedConnection = $state<any | null>(null);
  let detailedProcess = $state<any | null>(null);
  let loadingProcessDetails = $state(false);

  function closeContextMenu() {
    contextMenu.show = false;
  }

  function handleConnectionContextMenu(e: MouseEvent, conn: any) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      show: true,
      conn
    };
  }

  async function openConnectionDetails(conn: any) {
    selectedConnection = conn;
    isDrawerOpen = true;
    detailedProcess = null;
    if (conn.pid) {
      loadingProcessDetails = true;
      try {
        const list: any[] = await invoke('get_process_list');
        detailedProcess = list.find(p => p.pid === conn.pid) || null;
      } catch (err) {
        console.error("Error loading process details:", err);
      } finally {
        loadingProcessDetails = false;
      }
    }
  }

  // Timers
  let leftTimer: any;
  let rightTimer: any;
  let processesTimer: any;

  function clearTimers() {
    clearInterval(leftTimer);
    clearInterval(rightTimer);
    clearInterval(processesTimer);
  }

  // Enhanced monitoring state
  let showCores = $state(false);
  let showTopConsumers = $state(false);
  let connFilter = $state<'all' | 'listen' | 'estab' | 'external'>('all');
  let connSearch = $state('');

  // Top Consumers Derived
  let topCpuProcesses = $derived.by(() => {
    if (!processes || processes.length === 0) return [];
    return [...processes].sort((a, b) => b.cpu_percent - a.cpu_percent).slice(0, 3);
  });

  let topMemProcesses = $derived.by(() => {
    if (!processes || processes.length === 0) return [];
    return [...processes].sort((a, b) => b.mem_rss_mb - a.mem_rss_mb).slice(0, 3);
  });

  // Filtered Connections Derived
  let filteredConnections = $derived.by(() => {
    if (!activeConnections) return [];
    let list = activeConnections;

    if (connFilter === 'listen') {
      list = list.filter(c => c.state && c.state.toLowerCase().includes('listen'));
    } else if (connFilter === 'estab') {
      list = list.filter(c => c.state && (c.state.toLowerCase().includes('estab') || c.state.toLowerCase().includes('connected')));
    } else if (connFilter === 'external') {
      list = list.filter(c => {
        const remote = c.remote_address || '';
        return remote && !remote.startsWith('127.0.0.1') && !remote.startsWith('::1') && !remote.startsWith('0.0.0.0') && !remote.startsWith('*');
      });
    }

    if (connSearch.trim()) {
      const q = connSearch.toLowerCase();
      list = list.filter(c => 
        (c.local_address && c.local_address.toLowerCase().includes(q)) ||
        (c.remote_address && c.remote_address.toLowerCase().includes(q)) ||
        (c.process_name && c.process_name.toLowerCase().includes(q)) ||
        (c.pid && c.pid.toString().includes(q)) ||
        (c.protocol && c.protocol.toLowerCase().includes(q))
      );
    }
    return list;
  });

  let connListenCount = $derived(activeConnections.filter(c => c.state && c.state.toLowerCase().includes('listen')).length);
  let connEstabCount = $derived(activeConnections.filter(c => c.state && (c.state.toLowerCase().includes('estab') || c.state.toLowerCase().includes('connected'))).length);

  async function pollLeftAndCenter() {
    try {
      const [sysStats, temp, procList] = await Promise.all([
        invoke('get_system_stats'),
        invoke('get_cpu_temperature'),
        invoke('get_process_list')
      ]);
      stats = sysStats;
      cpuTemp = temp as number | null;
      processes = procList as any[];
      cpuHistory = [...cpuHistory.slice(1), stats.cpu_percent];
      ramHistory = [...ramHistory.slice(1), stats.ram_percent];
      swapHistory = [...swapHistory.slice(1), stats.swap_percent];
    } catch(e) {}
    await fetchDiskIo();
  }

  async function pollRight() {
    try {
      networkInterfaces = await invoke('get_network_interfaces');
    } catch(e) {}
    await Promise.all([
      fetchNetworkTraffic(),
      fetchActiveConnections()
    ]);
  }

  async function pollProcesses() {
    isRefreshing = true;
    try {
      processes = await invoke('get_process_list');
    } catch(e) {}
    isRefreshing = false;
  }

  async function fetchDiskIo() {
    try {
      const newDiskIo: any[] = await invoke('get_disk_io_stats');
      const now = performance.now();
      if (lastDiskIoTime > 0 && lastDiskIoData.length > 0) {
        const deltaSec = (now - lastDiskIoTime) / 1000;
        let speeds: Record<string, { readSpeed: number, writeSpeed: number }> = {};
        for (const current of newDiskIo) {
          const prev = lastDiskIoData.find(d => d.device === current.device);
          if (prev) {
            const readSpeed = Math.max(0, (current.read_bytes - prev.read_bytes) / deltaSec);
            const writeSpeed = Math.max(0, (current.write_bytes - prev.write_bytes) / deltaSec);
            speeds[current.device] = { readSpeed, writeSpeed };
            
            const combinedSpeed = readSpeed + writeSpeed;
            if (!diskIoHistory[current.device]) {
              diskIoHistory[current.device] = Array(40).fill(0);
            }
            diskIoHistory[current.device] = [...diskIoHistory[current.device].slice(1), combinedSpeed];
          }
        }
        diskIoSpeeds = speeds;
      }
      lastDiskIoData = newDiskIo;
      lastDiskIoTime = now;
      diskIoStats = newDiskIo;
    } catch (e) {
      console.error(e);
    }
  }

  async function fetchNetworkTraffic() {
    try {
      const newTraffic: any[] = await invoke('get_network_traffic');
      const now = performance.now();
      if (lastNetworkTime > 0 && lastNetworkData.length > 0) {
        const deltaSec = (now - lastNetworkTime) / 1000;
        let speeds: Record<string, { rxSpeed: number, txSpeed: number }> = {};
        for (const current of newTraffic) {
          const prev = lastNetworkData.find(t => t.interface === current.interface);
          if (prev) {
            const rxDiff = current.rx_bytes - prev.rx_bytes;
            const txDiff = current.tx_bytes - prev.tx_bytes;
            speeds[current.interface] = {
              rxSpeed: Math.max(0, rxDiff / deltaSec),
              txSpeed: Math.max(0, txDiff / deltaSec)
            };
          }
        }
        networkSpeeds = speeds;
      }
      lastNetworkData = newTraffic;
      lastNetworkTime = now;
      networkTraffic = newTraffic;
    } catch (e) {
      console.error(e);
    }
  }

  async function fetchActiveConnections() {
    try {
      activeConnections = await invoke('get_active_connections');
    } catch(e) {
      console.error(e);
    }
  }

  $effect(() => {
    clearTimers();
    if (isPaused) return;
    if (currentTab === 'overview') {
      pollLeftAndCenter();
      pollRight();
      leftTimer = setInterval(pollLeftAndCenter, 2000);
      rightTimer = setInterval(pollRight, 3000);
    } else if (currentTab === 'processes') {
      pollProcesses();
      processesTimer = setInterval(pollProcesses, 3000);
    }
  });

  onMount(async () => {
    try {
      currentUser = await invoke('get_current_user');
      const history: any[] = await invoke('get_system_stats_history');
      if (history && history.length > 0) {
        const cPcts = history.map(h => h.cpu_percent);
        const rPcts = history.map(h => h.ram_percent);
        if (cPcts.length < 40) {
          cpuHistory = [...Array(40 - cPcts.length).fill(0), ...cPcts];
          ramHistory = [...Array(40 - rPcts.length).fill(0), ...rPcts];
        } else {
          cpuHistory = cPcts.slice(-40);
          ramHistory = rPcts.slice(-40);
        }
      }
    } catch(e) {
      console.error(e);
    }
  });

  onDestroy(() => {
    clearTimers();
  });

  // Dynamic Sparkline generator helper for rates (e.g. disk speed)
  function generateDynamicSparkline(data: number[], height: number, width: number): string {
    if (data.length === 0) return '';
    const maxVal = Math.max(...data, 1024); // scale to max rate, at least 1KB/s
    const points = data.map((val, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - (val / maxVal) * height;
      return `${x},${y}`;
    }).join(' ');
    return `M 0,${height} L ${points} L ${width},${height} Z`;
  }
  
  function generateDynamicSparklineStroke(data: number[], height: number, width: number): string {
    if (data.length === 0) return '';
    const maxVal = Math.max(...data, 1024);
    return data.map((val, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - (val / maxVal) * height;
      return `${x},${y}`;
    }).join(' ');
  }

  // Filter out unused network interfaces
  const activeInterfaces = $derived(
    networkTraffic.filter(iface => {
      if (iface.interface === 'lo') return false;
      const meta = networkInterfaces.find(ni => ni.name === iface.interface);
      if (!meta) {
        const speeds = networkSpeeds[iface.interface];
        return speeds && (speeds.rxSpeed > 0 || speeds.txSpeed > 0);
      }
      return meta.is_up && (meta.ip4 || meta.ip6 || (networkSpeeds[iface.interface] && (networkSpeeds[iface.interface].rxSpeed > 0 || networkSpeeds[iface.interface].txSpeed > 0)));
    })
  );

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

  function formatSpeed(bytesPerSec: number) {
    if (bytesPerSec < 1024) return Math.round(bytesPerSec) + " B/s";
    if (bytesPerSec < 1024 * 1024) return (bytesPerSec / 1024).toFixed(1) + " KB/s";
    if (bytesPerSec < 1024 * 1024 * 1024) return (bytesPerSec / (1024 * 1024)).toFixed(1) + " MB/s";
    return (bytesPerSec / (1024 * 1024 * 1024)).toFixed(1) + " GB/s";
  }

  function killProcess(pid: number, name: string) {
    uiStore.confirm(
      `Confirm Kill Process`,
      `Kill process ${name} (PID ${pid})?`,
      async () => {
        try {
          const res = await invoke('kill_process', { pid, signal: 15 });
          statusStore.setLastCommand(`kill -15 ${pid}`, 0, true);
          uiStore.addToast(res as string, 'success');
          pollProcesses();
        } catch (e: any) {
          statusStore.setLastCommand(`kill -15 ${pid}`, 1, false);
          uiStore.addToast(e.toString(), 'error');
        }
      },
      true
    );
  }

  interface GroupedProcess {
    pid: number;
    name: string;
    cmdline: string;
    cpu_percent: number;
    mem_percent: number;
    mem_rss_mb: number;
    user: string;
    count: number;
    pids: number[];
  }

  // Deduplicate and collapse processes by identical name
  let groupedProcesses = $derived.by(() => {
    const search = processSearch.toLowerCase();
    const list = processes.filter(p => 
      !search || 
      p.name.toLowerCase().includes(search) || 
      p.pid.toString().includes(search) ||
      (p.user && p.user.toLowerCase().includes(search))
    );

    const groups = new Map<string, GroupedProcess>();
    for (const p of list) {
      const existing = groups.get(p.name);
      if (existing) {
        existing.cpu_percent += p.cpu_percent;
        existing.mem_percent += p.mem_percent;
        existing.mem_rss_mb += p.mem_rss_mb;
        existing.count += 1;
        existing.pids.push(p.pid);
        if (p.pid < existing.pid) {
          existing.pid = p.pid;
          existing.cmdline = p.cmdline;
          existing.user = p.user;
        }
      } else {
        groups.set(p.name, {
          pid: p.pid,
          name: p.name,
          cmdline: p.cmdline,
          cpu_percent: p.cpu_percent,
          mem_percent: p.mem_percent,
          mem_rss_mb: p.mem_rss_mb,
          user: p.user,
          count: 1,
          pids: [p.pid],
        });
      }
    }
    return Array.from(groups.values());
  });

  async function forceRefresh() {
    if (currentTab === 'overview') {
      await Promise.all([pollLeftAndCenter(), pollRight()]);
    } else {
      await pollProcesses();
    }
  }
</script>

<div class="module-page">
  <PageHeader title="Monitoring" subtitle="Real-time system health and process management." icon={Activity}>
    <TabGroup
      tabs={[
        { id: 'overview', label: 'Overview' },
        { id: 'processes', label: 'Processes' }
      ]}
      bind:activeTab={currentTab}
      style="margin-right: 8px;"
    />
    <div style="display:flex; gap: 8px;">
      <Button onclick={() => isPaused = !isPaused} variant={isPaused ? 'primary' : 'ghost'}>
        {#if isPaused}
          <Play size={14} style="margin-right: 4px;" /> Resume
        {:else}
          <Pause size={14} style="margin-right: 4px;" /> Pause
        {/if}
      </Button>
      <Button onclick={forceRefresh} variant="primary" disabled={isRefreshing}>
        <RefreshCw size={14} class={isRefreshing ? 'animate-spin-slow' : ''} />
        Refresh
      </Button>
    </div>
  </PageHeader>

  <div class="page-content" style="flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 8px; padding: 2px 8px 6px 8px; overflow: hidden;">
    {#if currentTab === 'overview'}
      {#if stats}
        <div class="monitor-layout">
          <!-- COLUMN 1: Resource Usage & Disk I/O -->
          <div class="monitor-column-left">
            <!-- Core Resource Usage -->
            <Card title="Core Resource Usage" icon={Cpu} class="monitor-panel">
              <div class="panel-scroll">
                <!-- CPU -->
                <div class="metric-block" style="display:flex; flex-direction:column; gap:8px;">
                  <div style="display:flex; justify-content:space-between; font-size:12px; font-weight:600; align-items: center;">
                    <span>CPU Usage</span>
                    <span style="display: flex; gap: 8px; align-items: center;">
                      {#if cpuTemp !== null}
                        <span style="font-size: 11px; font-weight: 500; padding: 2px 6px; border-radius: 4px; background: {cpuTemp >= 85 ? 'rgba(239, 68, 68, 0.15)' : cpuTemp >= 70 ? 'rgba(245, 158, 11, 0.15)' : 'rgba(255, 255, 255, 0.05)'}; color: {cpuTemp >= 85 ? 'var(--color-error)' : cpuTemp >= 70 ? 'var(--color-warning)' : 'var(--color-text-primary)'}; font-family: var(--font-mono);">
                          {cpuTemp.toFixed(0)}&deg;C
                        </span>
                      {/if}
                      <span>{stats.cpu_percent.toFixed(1)}%</span>
                    </span>
                  </div>
                  <div style="height: 60px; background: rgba(0,0,0,0.2); border-radius: 8px; overflow:hidden; padding: 4px;">
                    <svg viewBox="0 0 200 60" preserveAspectRatio="none" style="width: 100%; height: 100%;">
                      <path d={generateSparkline(cpuHistory, 60, 200)} class="spark-fill cpu-fill" />
                      <polyline points={generateSparklineStroke(cpuHistory, 60, 200)} class="spark-stroke cpu-stroke" fill="none" />
                    </svg>
                  </div>
                  <div style="display:flex; justify-content:space-between; font-size:11px; color:var(--color-text-muted);">
                    <button
                      onclick={() => showCores = !showCores}
                      style="background: transparent; border: none; padding: 0; color: var(--color-accent); cursor: pointer; font-size: 11px; text-decoration: underline; font-family: inherit;"
                    >
                      {stats.cpu_cores} Cores {showCores ? '▲' : '▼'}
                    </button>
                    <span>Load: {stats.load_1.toFixed(2)}, {stats.load_5.toFixed(2)}</span>
                  </div>
                </div>

                <!-- CPU Cores Expandable Grid -->
                {#if showCores && stats.cpu_per_core && stats.cpu_per_core.length > 0}
                  <div style="display:grid; grid-template-columns: repeat(auto-fill, minmax(75px, 1fr)); gap: 6px; padding: 8px; background: rgba(0,0,0,0.25); border-radius: 6px; border: 1px solid rgba(255,255,255,0.04);">
                    {#each stats.cpu_per_core as corePct, idx}
                      <div style="display:flex; flex-direction:column; gap:2px; font-size:10px; font-family:var(--font-mono);">
                        <div style="display:flex; justify-content:space-between; color:var(--color-text-muted);">
                          <span>C{idx}</span>
                          <span>{corePct.toFixed(0)}%</span>
                        </div>
                        <div class="progress-bg" style="height: 4px; background: rgba(255,255,255,0.05);">
                          <div class="progress-fill" style="width: {corePct}%; height: 100%; background: {corePct > 70 ? 'var(--color-error)' : corePct > 40 ? 'var(--color-warning)' : 'var(--color-accent)'};"></div>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
                
                <!-- Memory -->
                <div class="metric-block" style="display:flex; flex-direction:column; gap:6px;">
                  <div style="display:flex; justify-content:space-between; font-size:12px; font-weight:600;">
                    <span>Memory</span>
                    <span>{stats.ram_percent.toFixed(1)}%</span>
                  </div>
                  <div class="progress-bg" style="height: 8px; background: rgba(0,0,0,0.3); border-radius: 4px; overflow: hidden;">
                    <div class="progress-fill ram-fill" style="width: {stats.ram_percent}%; height: 100%; transition: width 0.3s ease;"></div>
                  </div>
                  <div style="height: 36px; background: rgba(0,0,0,0.2); border-radius: 6px; overflow:hidden; padding: 2px;">
                    <svg viewBox="0 0 200 36" preserveAspectRatio="none" style="width: 100%; height: 100%;">
                      <polyline points={generateSparklineStroke(ramHistory, 36, 200)} style="stroke: var(--color-accent); stroke-width: 1.5px; fill: none; stroke-linejoin: round; stroke-linecap: round;" />
                    </svg>
                  </div>
                  <div style="display:flex; justify-content:space-between; font-size:11px; color:var(--color-text-muted);">
                    <span>Used: {formatBytes(stats.ram_used_mb)}</span>
                    <span>Total: {formatBytes(stats.ram_total_mb)}</span>
                  </div>
                </div>
                
                <!-- Swap -->
                <div class="metric-block" style="display:flex; flex-direction:column; gap:6px;">
                  <div style="display:flex; justify-content:space-between; font-size:12px; font-weight:600;">
                    <span>Swap Space</span>
                    <span>{stats.swap_percent.toFixed(1)}%</span>
                  </div>
                  <div class="progress-bg" style="height: 8px; background: rgba(0,0,0,0.3); border-radius: 4px; overflow: hidden;">
                    <div class="progress-fill swap-fill" style="width: {stats.swap_percent}%; height: 100%; transition: width 0.3s ease;"></div>
                  </div>
                  <div style="height: 36px; background: rgba(0,0,0,0.2); border-radius: 6px; overflow:hidden; padding: 2px;">
                    <svg viewBox="0 0 200 36" preserveAspectRatio="none" style="width: 100%; height: 100%;">
                      <polyline points={generateSparklineStroke(swapHistory, 36, 200)} style="stroke: var(--color-warning, #f59e0b); stroke-width: 1.5px; fill: none; stroke-linejoin: round; stroke-linecap: round;" />
                    </svg>
                  </div>
                  <div style="display:flex; justify-content:space-between; font-size:11px; color:var(--color-text-muted);">
                    <span>Used: {formatBytes(stats.swap_used_mb)}</span>
                    <span>Total: {formatBytes(stats.swap_total_mb)}</span>
                  </div>
                </div>

                <!-- Top Resource Consumers Section -->
                {#if topCpuProcesses.length > 0}
                  <div style="border-top: 1px dashed rgba(255,255,255,0.08); padding-top: 10px; margin-top: 4px; display:flex; flex-direction:column; gap:8px;">
                    <div style="font-size: 11px; font-weight: 700; color: var(--color-text-secondary); text-transform: uppercase; letter-spacing: 0.05em; display:flex; justify-content:space-between; align-items:center;">
                      <button
                        onclick={() => showTopConsumers = !showTopConsumers}
                        style="background: transparent; border: none; padding: 0; color: var(--color-accent); font-size: 11px; font-weight: 700; cursor: pointer; text-transform: uppercase; letter-spacing: 0.05em; font-family: inherit;"
                      >
                        Top Consumers {showTopConsumers ? '▲' : '▼'}
                      </button>
                      {#if showTopConsumers}
                        <button 
                          onclick={() => currentTab = 'processes'}
                          style="background: transparent; border: none; padding: 0; color: var(--color-accent); font-size: 10px; cursor: pointer;"
                        >
                          View All &rarr;
                        </button>
                      {/if}
                    </div>

                    {#if showTopConsumers}
                      <div style="display:grid; grid-template-columns: 1fr 1fr; gap: 8px;">
                        <!-- Top CPU -->
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 8px; border-radius: 6px; display:flex; flex-direction:column; gap:4px;">
                          <span style="font-size: 9px; font-weight:700; color: var(--color-info); text-transform: uppercase;">Top CPU</span>
                          {#each topCpuProcesses as p}
                            <div style="display:flex; justify-content:space-between; font-size: 10px; font-family: var(--font-mono);">
                              <span style="color:var(--color-text-primary); text-overflow:ellipsis; overflow:hidden; white-space:nowrap; max-width:85px;" title={p.name}>{p.name}</span>
                              <span style="color:var(--color-info); font-weight:600;">{p.cpu_percent.toFixed(1)}%</span>
                            </div>
                          {/each}
                        </div>

                        <!-- Top RAM -->
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 8px; border-radius: 6px; display:flex; flex-direction:column; gap:4px;">
                          <span style="font-size: 9px; font-weight:700; color: var(--color-accent); text-transform: uppercase;">Top RAM</span>
                          {#each topMemProcesses as p}
                            <div style="display:flex; justify-content:space-between; font-size: 10px; font-family: var(--font-mono);">
                              <span style="color:var(--color-text-primary); text-overflow:ellipsis; overflow:hidden; white-space:nowrap; max-width:85px;" title={p.name}>{p.name}</span>
                              <span style="color:var(--color-accent); font-weight:600;">{p.mem_rss_mb > 1024 ? (p.mem_rss_mb/1024).toFixed(1) + 'GB' : p.mem_rss_mb.toFixed(0) + 'MB'}</span>
                            </div>
                          {/each}
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            </Card>

            <!-- Disk I/O -->
            <Card title="Disk I/O" icon={HardDrive} class="monitor-panel">
              <div class="panel-scroll" style="gap:10px;">
                {#if diskIoStats.length > 0}
                  {#each diskIoStats as disk}
                    <div style="background: var(--color-bg-input); border: 1px solid var(--color-border); border-radius: 8px; padding: 12px; display:flex; flex-direction:column; gap:8px;">
                      <div style="display:flex; justify-content:space-between; align-items:center; width:100%;">
                        <div style="display:flex; align-items:center; gap:10px;">
                          <div style="width:32px; height:32px; background: var(--color-module-icon-bg, var(--color-bg-raised)); border: 1px solid var(--color-border); border-radius: 6px; display:flex; align-items:center; justify-content:center; color:var(--color-accent);">
                            <HardDrive size={16} />
                          </div>
                          <span style="font-weight:600; font-family:var(--font-mono); font-size:13px; color:var(--color-text-primary);">{disk.device}</span>
                        </div>
                        <div style="display:flex; gap:16px;">
                          <div style="display:flex; flex-direction:column; align-items:flex-end;">
                            <span style="color:var(--color-text-muted); font-size:9px; font-weight:700; letter-spacing:0.05em;">READ</span>
                            <strong style="color:var(--color-text-primary); font-size:12px; font-family:var(--font-mono);">{formatSpeed(diskIoSpeeds[disk.device]?.readSpeed || 0)}</strong>
                          </div>
                          <div style="display:flex; flex-direction:column; align-items:flex-end;">
                            <span style="color:var(--color-text-muted); font-size:9px; font-weight:700; letter-spacing:0.05em;">WRITE</span>
                            <strong style="color:var(--color-accent); font-size:12px; font-family:var(--font-mono);">{formatSpeed(diskIoSpeeds[disk.device]?.writeSpeed || 0)}</strong>
                          </div>
                        </div>
                      </div>
                      <div style="height:36px; width:100%; margin-top:4px; overflow:hidden;">
                        <svg viewBox="0 0 300 36" preserveAspectRatio="none" style="width:100%; height:100%;">
                          <polyline
                            points={generateDynamicSparklineStroke(diskIoHistory[disk.device] || [], 36, 300)}
                            style="stroke: var(--color-accent); stroke-width: 1.5px; fill: none; stroke-linejoin: round; stroke-linecap: round;"
                          />
                        </svg>
                      </div>
                    </div>
                  {/each}
                {:else}
                  <span class="text-muted">Loading disk I/O metrics...</span>
                {/if}
              </div>
            </Card>
          </div>

          <!-- COLUMN 2: Network & Connections -->
          <Card title="Network & Connections" icon={Wifi} class="monitor-panel" style="display: flex; flex-direction: column; height: 100%; min-height: 0;">
            <div style="display: flex; flex-direction: column; gap: 16px; flex: 1; min-height: 0;">
              <div>
                <h4 style="margin: 0 0 8px; font-size: 10px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; color: var(--color-text-muted);">Interface Speeds</h4>
                <div style="display:flex; gap: 8px; flex-wrap: wrap; width: 100%;">
                  {#each activeInterfaces as iface}
                    <div style="flex: 1; min-width: 180px; background: var(--color-bg-input); border: 1px solid var(--color-border); border-radius: 6px; padding: 6px 10px; display:flex; align-items:center; justify-content:space-between; gap:8px;">
                      <div style="display:flex; align-items:center; gap:6px;">
                        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="3" style="color:var(--color-accent); flex-shrink:0;">
                          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" stroke-linecap="round" stroke-linejoin="round"/>
                          <polyline points="22 4 12 14.01 9 11.01" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                        <span style="font-weight:600; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary);">{iface.interface}</span>
                      </div>
                      <div style="display:flex; align-items:center; gap:10px; font-size:10px; font-family:var(--font-mono);">
                        <div>
                          <span style="color:var(--color-text-muted); font-size:9px; font-weight:700; margin-right:2px;">DL</span>
                          <strong style="color:var(--color-accent);">{formatSpeed(networkSpeeds[iface.interface]?.rxSpeed || 0)}</strong>
                        </div>
                        <div>
                          <span style="color:var(--color-text-muted); font-size:9px; font-weight:700; margin-right:2px;">UL</span>
                          <strong style="color:var(--color-accent);">{formatSpeed(networkSpeeds[iface.interface]?.txSpeed || 0)}</strong>
                        </div>
                      </div>
                    </div>
                  {/each}
                  {#if activeInterfaces.length === 0}
                    <span style="font-size:11px; color:var(--color-text-muted); font-style:italic;">No active network interfaces.</span>
                  {/if}
                </div>
              </div>
              <div style="display:flex; flex-direction:column; flex:1; min-height:0; gap:8px;">
                <!-- Single Compact Toolbar Row: Title + Search Input + Filter Pills -->
                <div style="display: flex; align-items: center; gap: 8px; flex-wrap: nowrap; margin-bottom: 2px;">
                  <h4 style="margin: 0; font-size: 12px; color: var(--color-text-secondary); font-weight: 600; white-space: nowrap; flex-shrink: 0;">
                    Connections <span style="font-size: 10px; color: var(--color-text-muted); font-weight: normal; margin-left: 2px;">({filteredConnections.length})</span>
                  </h4>

                  <!-- Connection Search Box -->
                  <div style="flex: 1; min-width: 130px;">
                    <input
                      type="text"
                      placeholder="Search IP, port, PID, process..."
                      bind:value={connSearch}
                      style="width: 100%; background: rgba(0,0,0,0.2); border: 1px solid rgba(255,255,255,0.08); padding: 4px 8px; border-radius: 6px; font-size: 11px; color: var(--color-text-primary); outline: none;"
                    />
                  </div>

                  <!-- Connection Filter Pills -->
                  <div style="display: flex; gap: 3px; font-size: 10px; flex-shrink: 0;">
                    <button
                      onclick={() => connFilter = 'all'}
                      style="padding: 2px 7px; border-radius: 4px; border: none; cursor: pointer; background:{connFilter === 'all' ? 'var(--color-accent)' : 'var(--color-bg-hover)'}; color:{connFilter === 'all' ? 'var(--color-text-on-accent)' : 'var(--color-text-secondary)'}; font-weight: 600; transition: all 0.15s;"
                    >
                      All ({activeConnections.length})
                    </button>
                    <button
                      onclick={() => connFilter = 'listen'}
                      style="padding: 2px 7px; border-radius: 4px; border: none; cursor: pointer; background:{connFilter === 'listen' ? 'var(--color-warning)' : 'var(--color-bg-hover)'}; color:{connFilter === 'listen' ? 'var(--color-text-on-accent)' : 'var(--color-text-secondary)'}; font-weight: 600; transition: all 0.15s;"
                    >
                      Listen ({connListenCount})
                    </button>
                    <button
                      onclick={() => connFilter = 'estab'}
                      style="padding: 2px 7px; border-radius: 4px; border: none; cursor: pointer; background:{connFilter === 'estab' ? 'var(--color-success)' : 'var(--color-bg-hover)'}; color:{connFilter === 'estab' ? 'var(--color-text-on-accent)' : 'var(--color-text-secondary)'}; font-weight: 600; transition: all 0.15s;"
                    >
                      Estab ({connEstabCount})
                    </button>
                    <button
                      onclick={() => connFilter = 'external'}
                      style="padding: 2px 7px; border-radius: 4px; border: none; cursor: pointer; background:{connFilter === 'external' ? 'var(--color-accent)' : 'var(--color-bg-hover)'}; color:{connFilter === 'external' ? 'var(--color-text-on-accent)' : 'var(--color-text-secondary)'}; font-weight: 600; transition: all 0.15s;"
                    >
                      External
                    </button>
                  </div>
                </div>

                <div style="flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--color-border); border-radius: 8px;">
                  <Table tableAction={tableFeatures} class="active-conn-table" style="flex: 1; min-height: 0; overflow-y: auto;">
                    <thead>
                      <tr>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Proto</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Local</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Remote</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">State</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">PID</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Process</th>
                        <th style="padding:6px; text-align:right; color:var(--color-text-secondary);">Action</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each filteredConnections as conn}
                        {@const isListen = conn.state && conn.state.toLowerCase().includes('listen')}
                        <tr 
                          style="border-bottom:1px solid rgba(255,255,255,0.02); cursor: pointer; background: {isListen ? 'rgba(245, 158, 11, 0.04)' : 'transparent'};"
                          onclick={() => openConnectionDetails(conn)}
                          oncontextmenu={(e) => handleConnectionContextMenu(e, conn)}
                        >
                          <td style="padding:6px; color:var(--color-text-primary); font-family:var(--font-mono); font-size:11px;">{conn.protocol}</td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all; font-family:var(--font-mono); font-size:11px;" title={conn.local_address}>
                            {conn.local_address}
                          </td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all; font-family:var(--font-mono); font-size:11px;" title={conn.remote_address}>
                            {conn.remote_address}
                          </td>
                          <td style="padding:6px;">
                            <span style="font-size:10px; font-weight:700; padding:1px 5px; border-radius:4px; font-family:var(--font-mono); background:{isListen ? 'var(--color-warning-muted, rgba(245,158,11,0.15))' : 'var(--color-success-muted, rgba(34,197,94,0.15))'}; color:{isListen ? 'var(--color-warning)' : 'var(--color-success)'};">
                              {conn.state}
                            </span>
                          </td>
                          <td style="padding:6px; color:var(--color-text-muted); font-family:var(--font-mono); font-size:11px;">{conn.pid || '-'}</td>
                          <td style="padding:6px; color:var(--color-text-secondary); font-size:11px;">{conn.process_name}</td>
                          <td style="padding:6px; text-align:right;" onclick={(e) => e.stopPropagation()}>
                            <KebabMenu align="right">
                              <button
                                class="menu-item"
                                onclick={() => openConnectionDetails(conn)}
                              >
                                <TerminalSquare size={14} style="color: var(--color-info);" />
                                Show Details
                              </button>
                              {#if conn.pid}
                                <button
                                  class="menu-item danger"
                                  onclick={() => killProcess(conn.pid, conn.process_name)}
                                >
                                  <Skull size={14} style="color: var(--color-error);" />
                                  Kill Process
                                </button>
                              {/if}
                            </KebabMenu>
                          </td>
                        </tr>
                      {/each}
                      {#if filteredConnections.length === 0}
                        <tr>
                          <td colspan="7" style="padding:16px; text-align:center; color:var(--color-text-muted); font-size:12px;">
                            No connections match the selected filter.
                          </td>
                        </tr>
                      {/if}
                    </tbody>
                  </Table>
                </div>
              </div>
            </div>
          </Card>
        </div>
      {:else}
        <div class="loading-state">
          <Loader size={24} class="animate-spin-slow" /> Loading system metrics...
        </div>
      {/if}
    {:else}
      <!-- Processes Tab -->
      <div class="table-container" style="flex:1; min-height:0; display:flex; flex-direction:column;">
        <div class="table-toolbar">
          <div class="search-box">
            <input type="text" placeholder="Search processes by name, PID, or user..." bind:value={processSearch} />
          </div>
          <div class="toolbar-stats">
            Showing {groupedProcesses.length} process groups
          </div>
        </div>

        <div class="table-scroll" style="flex:1; overflow:auto;">
          <Table tableAction={tableFeatures}>
            <thead>
              <tr>
                <th class="col-pid">PID</th>
                <th class="col-name">Name</th>
                <th class="col-user">User</th>
                <th class="col-cpu">CPU %</th>
                <th class="col-mem">Mem %</th>
                <th class="col-rss">RSS</th>
                <th class="col-actions"></th>
              </tr>
            </thead>
            <tbody>
              {#each groupedProcesses as p (p.name)}
                <tr class:is-root={p.user === 'root'} class:is-kernel={p.pid <= 100}>
                  <td class="col-pid">{p.pid}</td>
                  <td class="col-name">
                    <div style="display:flex; align-items:center; gap:6px;">
                      <div class="proc-name">{p.name}</div>
                      {#if p.count > 1}
                        <span style="background:rgba(255,255,255,0.06); border:1px solid var(--color-border); color:var(--color-text-accent); font-size:10px; font-weight:bold; padding:1px 5px; border-radius:4px;">
                          {p.count}
                        </span>
                      {/if}
                    </div>
                    <div class="proc-cmd">{p.cmdline}</div>
                  </td>
                  <td class="col-user">
                    <span 
                      class="user-badge"
                      style="color: {p.user === 'root' ? 'var(--color-error)' : p.user === currentUser ? 'var(--color-success)' : 'var(--color-text-secondary)'}; font-weight: 600;"
                    >
                      {p.user}
                    </span>
                  </td>
                  <td class="col-cpu {p.cpu_percent > 20 ? 'text-warn' : p.cpu_percent > 50 ? 'text-danger' : ''}">
                    {p.cpu_percent.toFixed(1)}%
                  </td>
                  <td class="col-mem {p.mem_percent > 15 ? 'text-warn' : ''}">
                    {p.mem_percent.toFixed(1)}%
                  </td>
                  <td class="col-rss">{p.mem_rss_mb.toFixed(1)} MB</td>
                  <td class="col-actions">
                    {#if p.pid > 100}
                      <button class="action-btn kill" onclick={() => killProcess(p.pid, p.name)} title="Kill Process (SIGTERM)">
                        <Skull size={14} />
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </Table>
        </div>
      </div>
    {/if}
  </div>
</div>

<svelte:window onclick={closeContextMenu} oncontextmenu={closeContextMenu} />

{#if contextMenu.show && contextMenu.conn}
  <div 
    class="custom-context-menu" 
    style="position: fixed; left: {contextMenu.x}px; top: {contextMenu.y}px; z-index: 10000;"
    onclick={(e) => e.stopPropagation()}
  >
    <button 
      onclick={() => {
        if (contextMenu.conn && contextMenu.conn.pid) {
          killProcess(contextMenu.conn.pid, contextMenu.conn.process_name);
        }
        closeContextMenu();
      }}
      disabled={!contextMenu.conn.pid}
    >
      <Skull size={13} style="margin-right: 8px; color: var(--color-error);" />
      Kill Process
    </button>
    <button 
      onclick={() => {
        if (contextMenu.conn) {
          openConnectionDetails(contextMenu.conn);
        }
        closeContextMenu();
      }}
    >
      <TerminalSquare size={13} style="margin-right: 8px; color: var(--color-info);" />
      Show Details
    </button>
  </div>
{/if}

<SideDrawer bind:isOpen={isDrawerOpen} title="Connection Details" width="450px">
  {#if selectedConnection}
    <div style="display:flex; flex-direction:column; gap: 20px;">
      <!-- Port/Connection Section -->
      <div style="background: rgba(255,255,255,0.02); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px;">
        <h4 style="margin: 0 0 12px; font-size: 13px; font-weight: 600; color: var(--color-info); display: flex; align-items: center; gap: 6px;">
          <Wifi size={14} /> Network Port Details
        </h4>
        <div style="display:flex; flex-direction:column; gap: 8px; font-size: 12px;">
          <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Protocol</span> <span style="font-weight:600; font-family:var(--font-mono);">{selectedConnection.protocol}</span></div>
          <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">State</span> <span style="color:var(--color-success); font-weight:600;">{selectedConnection.state}</span></div>
          <div style="display:flex; justify-content:space-between; flex-direction:column; gap:2px;">
            <span style="color:var(--color-text-muted);">Local Address & Port</span>
            <span style="font-family:var(--font-mono); background:rgba(0,0,0,0.2); padding: 4px 8px; border-radius:4px; word-break:break-all;">{selectedConnection.local_address}</span>
          </div>
          <div style="display:flex; justify-content:space-between; flex-direction:column; gap:2px;">
            <span style="color:var(--color-text-muted);">Remote Address & Port</span>
            <span style="font-family:var(--font-mono); background:rgba(0,0,0,0.2); padding: 4px 8px; border-radius:4px; word-break:break-all;">{selectedConnection.remote_address}</span>
          </div>
        </div>
      </div>

      <!-- Process Section -->
      <div style="background: rgba(255,255,255,0.02); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px;">
        <h4 style="margin: 0 0 12px; font-size: 13px; font-weight: 600; color: var(--color-accent); display: flex; align-items: center; gap: 6px;">
          <Activity size={14} /> Process Details
        </h4>
        {#if selectedConnection.pid}
          {#if loadingProcessDetails}
            <div style="display:flex; align-items:center; justify-content:center; gap:8px; padding: 16px; color:var(--color-text-muted); font-size:12px;">
              <Loader size={16} class="animate-spin-slow" /> Fetching process info...
            </div>
          {:else}
            {#if detailedProcess}
              <div style="display:flex; flex-direction:column; gap: 8px; font-size: 12px;">
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Process Name</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.name}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">PID</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.pid}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">User</span> <span style="background:rgba(255,255,255,0.05); padding:1px 6px; border-radius:4px;">{detailedProcess.user}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">CPU Usage</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.cpu_percent.toFixed(1)}%</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Memory Usage</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.mem_percent.toFixed(1)}% ({detailedProcess.mem_rss_mb.toFixed(1)} MB)</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">State</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.state}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Threads</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.threads}</span></div>
                <div style="display:flex; flex-direction:column; gap:4px; margin-top:4px;">
                  <span style="color:var(--color-text-muted);">Full Command Line</span>
                  <div style="font-family:var(--font-mono); background:rgba(0,0,0,0.3); padding: 8px; border-radius:4px; font-size:10px; max-height:100px; overflow-y:auto; word-break:break-all; border: 1px solid rgba(255,255,255,0.03);">
                    {detailedProcess.cmdline || detailedProcess.name}
                  </div>
                </div>
                
                <div style="margin-top: 12px; display:flex; justify-content:flex-end;">
                  <Button 
                    variant="danger" 
                    size="sm"
                    onclick={() => {
                      isDrawerOpen = false;
                      killProcess(detailedProcess.pid, detailedProcess.name);
                    }}
                  >
                    <Skull size={13} style="margin-right: 4px;" /> Kill Process
                  </Button>
                </div>
              </div>
            {:else}
              <div style="color:var(--color-text-muted); font-size:12px; display:flex; flex-direction:column; gap:10px; padding:8px 0;">
                <p style="margin:0; line-height:1.4;">Detailed process metrics are unavailable (the process may have terminated, or requires higher privileges to inspect).</p>
                <div style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); border-radius: 6px; padding: 12px; display:flex; flex-direction:column; gap:8px;">
                  <div style="display:flex; justify-content:space-between; align-items:center; gap:8px;">
                    <span style="color:var(--color-text-secondary);">Application / Process:</span> 
                    <strong style="font-family:var(--font-mono); color:var(--color-text-primary); text-align:right; word-break:break-all;">{selectedConnection.process_name}</strong>
                  </div>
                  <div style="display:flex; justify-content:space-between; align-items:center; gap:8px;">
                    <span style="color:var(--color-text-secondary);">Process PID:</span> 
                    <strong style="font-family:var(--font-mono); color:var(--color-text-primary); text-align:right;">{selectedConnection.pid}</strong>
                  </div>
                  <div style="display:flex; justify-content:space-between; align-items:center; gap:8px;">
                    <span style="color:var(--color-text-secondary);">Local Port / Address:</span> 
                    <strong style="font-family:var(--font-mono); color:var(--color-text-primary); text-align:right; word-break:break-all;">{selectedConnection.local_address}</strong>
                  </div>
                </div>
              </div>
            {/if}
          {/if}
        {:else}
          <div style="color:var(--color-text-muted); font-size:12px; text-align:center; padding:16px;">
            No PID associated with this connection.
          </div>
        {/if}
      </div>
    </div>
  {/if}
</SideDrawer>

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
    overflow: hidden;
    padding: 8px 12px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  /* 2-column layout */
  .monitor-layout {
    display: grid;
    grid-template-columns: 3.5fr 10.5fr;
    gap: 10px;
    align-items: stretch;
    height: 100%;
    min-height: 0;
  }
  .monitor-column-left {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    min-height: 0;
    overflow-y: auto;
  }
  .monitor-column-left .monitor-panel {
    min-height: 0;
  }
  .monitor-column-left .monitor-panel:first-child {
    flex: 1.5;
  }
  .monitor-column-left .monitor-panel:last-child {
    flex: 0.8;
  }

  :global(.active-conn-table table thead th) {
    position: sticky;
    top: 0;
    z-index: 10;
    background: var(--color-bg-card);
  }

  .monitor-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .panel-scroll {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .sparkline-container {
    height: 60px;
    width: 100%;
  }
  .sparkline-svg {
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .spark-stroke { stroke-width: 2px; stroke-linejoin: round; stroke-linecap: round; }
  
  .cpu-stroke { stroke: var(--color-info); }
  .cpu-fill { fill: var(--color-info); opacity: 0.15; }
  
  
  .progress-bg {
    height: 6px;
    background: rgba(0,0,0,0.3);
    border-radius: 4px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.3s ease;
  }
  .ram-fill { background: linear-gradient(90deg, var(--color-accent), var(--color-accent-soft)); }
  .swap-fill { background: linear-gradient(90deg, var(--color-text-secondary), var(--color-border)); }

  /* PROCESSES TAB */
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
    background: var(--color-surface-elevated, rgba(0,0,0,0.1));
  }
  .search-box input {
    background: var(--color-surface, rgba(255, 255, 255, 0.05));
    border: 1px solid var(--color-border);
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

  :global(html.light-mode) .table-toolbar {
    background: #F1F5F9;
    border-bottom: 1px solid #E2E8F0;
  }
  :global(html.light-mode) .search-box input {
    background: #FFFFFF;
    border: 1px solid #CBD5E1;
    color: #1E293B;
  }
  :global(html.light-mode) .search-box input::placeholder {
    color: #94A3B8;
  }

  .table-scroll {
    flex: 1;
    overflow-y: auto;
  }
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

  .text-warn { color: var(--color-warning) !important; font-weight: 600; }
  .text-danger { color: var(--color-error) !important; font-weight: 700; }

  /* Context Menu Styles */
  .custom-context-menu {
    background: var(--color-bg-popover, var(--color-bg-card));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.15), 0 8px 10px -6px rgba(0, 0, 0, 0.08);
    padding: 6px;
    min-width: 160px;
    backdrop-filter: blur(12px);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .custom-context-menu button {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    padding: 8px 12px;
    font-size: 12px;
    text-align: left;
    border-radius: 6px;
    cursor: pointer;
    width: 100%;
    transition: all 0.15s;
  }
  .custom-context-menu button:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }
  .custom-context-menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .hover-text-error:hover {
    color: var(--color-error) !important;
  }
</style>
