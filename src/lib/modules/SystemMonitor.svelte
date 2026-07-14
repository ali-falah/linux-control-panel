<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { Activity, Cpu, Database, HardDrive, TerminalSquare } from '@lucide/svelte';
  import { RefreshCw, Skull, Loader, Wifi, Play, Pause } from '@lucide/svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';
  import { tableFeatures } from '../actions/tableFeatures';

  let currentTab = $state<'overview' | 'processes'>('overview');
  let currentUser = $state('unknown');

  // Overview Stats
  let stats = $state<any>(null);
  let cpuHistory = $state<number[]>(Array(40).fill(0));

  // Disk I/O
  let diskIoStats = $state<any[]>([]);
  let lastDiskIoTime = $state<number>(0);
  let lastDiskIoData = $state<any[]>([]);
  let diskIoSpeeds = $state<Record<string, { readSpeed: number, writeSpeed: number }>>({});

  // Network Live Traffic
  let networkTraffic = $state<any[]>([]);
  let lastNetworkTime = $state<number>(0);
  let lastNetworkData = $state<any[]>([]);
  let networkSpeeds = $state<Record<string, { rxSpeed: number, txSpeed: number }>>({});

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

  async function pollLeftAndCenter() {
    try {
      const sysStats = await invoke('get_system_stats');
      stats = sysStats;
      cpuHistory = [...cpuHistory.slice(1), stats.cpu_percent];
    } catch(e) {}
    await fetchDiskIo();
  }

  async function pollRight() {
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
            const readDiff = current.read_bytes - prev.read_bytes;
            const writeDiff = current.write_bytes - prev.write_bytes;
            speeds[current.device] = {
              readSpeed: Math.max(0, readDiff / deltaSec),
              writeSpeed: Math.max(0, writeDiff / deltaSec)
            };
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
    } catch(e) {
      console.error(e);
    }
  });

  onDestroy(() => {
    clearTimers();
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
    <div class="tab-switcher">
      <button class:active={currentTab === 'overview'} onclick={() => currentTab = 'overview'}>Overview</button>
      <button class:active={currentTab === 'processes'} onclick={() => currentTab = 'processes'}>Processes</button>
    </div>
    <div style="display:flex; gap: 8px;">
      <Button onclick={() => isPaused = !isPaused} variant={isPaused ? 'accent' : 'secondary'}>
        {#if isPaused}
          <Play size={14} style="margin-right: 4px;" /> Resume
        {:else}
          <Pause size={14} style="margin-right: 4px;" /> Pause
        {/if}
      </Button>
      <Button onclick={forceRefresh} variant="secondary" disabled={isRefreshing}>
        <RefreshCw size={14} class={isRefreshing ? 'animate-spin-slow' : ''} />
        Refresh
      </Button>
    </div>
  </PageHeader>

  <div class="page-content" style="flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 24px; overflow-y: auto;">
    {#if currentTab === 'overview'}
      {#if stats}
        <div class="monitor-layout">
          <!-- COLUMN 1: Resource Usage & Disk I/O -->
          <div class="monitor-column-left">
            <!-- Core Resource Usage -->
            <div class="monitor-panel">
              <h3 class="panel-title"><Cpu size={16} class="text-primary" /> Core Resource Usage</h3>
              <div class="panel-scroll">
                <!-- CPU -->
                <div class="metric-block" style="display:flex; flex-direction:column; gap:8px;">
                  <div style="display:flex; justify-content:space-between; font-size:12px; font-weight:600;">
                    <span>CPU Usage</span>
                    <span>{stats.cpu_percent.toFixed(1)}%</span>
                  </div>
                  <div style="height: 60px; background: rgba(0,0,0,0.2); border-radius: 8px; overflow:hidden; padding: 4px;">
                    <svg viewBox="0 0 200 60" preserveAspectRatio="none" style="width: 100%; height: 100%;">
                      <path d={generateSparkline(cpuHistory, 60, 200)} class="spark-fill cpu-fill" />
                      <polyline points={generateSparklineStroke(cpuHistory, 60, 200)} class="spark-stroke cpu-stroke" fill="none" />
                    </svg>
                  </div>
                  <div style="display:flex; justify-content:space-between; font-size:11px; color:var(--color-text-muted);">
                    <span>{stats.cpu_cores} Cores</span>
                    <span>Load: {stats.load_1.toFixed(2)}, {stats.load_5.toFixed(2)}</span>
                  </div>
                </div>
                
                <!-- Memory -->
                <div class="metric-block" style="display:flex; flex-direction:column; gap:6px;">
                  <div style="display:flex; justify-content:space-between; font-size:12px; font-weight:600;">
                    <span>Memory</span>
                    <span>{stats.ram_percent.toFixed(1)}%</span>
                  </div>
                  <div class="progress-bg" style="height: 8px; background: rgba(0,0,0,0.3); border-radius: 4px; overflow: hidden;">
                    <div class="progress-fill ram-fill" style="width: {stats.ram_percent}%; height: 100%; transition: width 0.3s ease;"></div>
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
                  <div style="display:flex; justify-content:space-between; font-size:11px; color:var(--color-text-muted);">
                    <span>Used: {formatBytes(stats.swap_used_mb)}</span>
                    <span>Total: {formatBytes(stats.swap_total_mb)}</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Disk I/O -->
            <div class="monitor-panel">
              <h3 class="panel-title"><HardDrive size={16} class="text-purple" /> Disk I/O</h3>
              <div class="panel-scroll" style="gap:10px;">
                {#if diskIoStats.length > 0}
                  {#each diskIoStats as disk}
                    <div style="background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); border-radius: 8px; padding: 12px; display:flex; flex-direction:column; gap:8px;">
                      <div style="font-weight: 600; font-family: var(--font-mono); font-size: 13px; color: var(--color-text-primary);">{disk.device}</div>
                      <div style="display:flex; justify-content:space-between; font-size: 12px; font-family: var(--font-mono);">
                        <div>
                          <span style="color: var(--color-text-muted); font-size: 10px; margin-right: 4px;">READ</span>
                          <strong class="text-info">{formatSpeed(diskIoSpeeds[disk.device]?.readSpeed || 0)}</strong>
                        </div>
                        <div>
                          <span style="color: var(--color-text-muted); font-size: 10px; margin-right: 4px;">WRITE</span>
                          <strong class="text-success">{formatSpeed(diskIoSpeeds[disk.device]?.writeSpeed || 0)}</strong>
                        </div>
                      </div>
                    </div>
                  {/each}
                {:else}
                  <span class="text-muted">Loading disk I/O metrics...</span>
                {/if}
              </div>
            </div>
          </div>

          <!-- COLUMN 2: Network & Connections -->
          <div class="monitor-panel">
            <h3 class="panel-title"><Wifi size={16} class="text-info" /> Network & Connections</h3>
            <div class="panel-scroll" style="gap:16px;">
              <div>
                <h4 style="margin: 0 0 8px; font-size: 12px; color: var(--color-text-secondary); font-weight:600;">Interface Speeds</h4>
                <div style="display:flex; flex-direction:column; gap: 8px;">
                  {#each networkTraffic as iface}
                    {#if iface.interface !== 'lo'}
                      <div style="background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); border-radius: 8px; padding: 10px; display:flex; flex-direction:column; gap:4px;">
                        <div style="font-weight:600; font-family:var(--font-mono); font-size:12px;">{iface.interface}</div>
                        <div style="display:flex; justify-content:space-between; font-size:11px; font-family:var(--font-mono);">
                          <div><span style="color:var(--color-text-muted); font-size:9px;">DL</span> <strong class="text-success">{formatSpeed(networkSpeeds[iface.interface]?.rxSpeed || 0)}</strong></div>
                          <div><span style="color:var(--color-text-muted); font-size:9px;">UL</span> <strong class="text-warning">{formatSpeed(networkSpeeds[iface.interface]?.txSpeed || 0)}</strong></div>
                        </div>
                      </div>
                    {/if}
                  {/each}
                </div>
              </div>
              <div style="display:flex; flex-direction:column; flex:1.8; min-height:380px;">
                <h4 style="margin: 0 0 8px; font-size: 12px; color: var(--color-text-secondary); font-weight:600;">Active Connections <span style="font-size:10px; color:var(--color-text-muted); font-weight:normal; margin-left:6px;">(Click for details, right-click for options)</span></h4>
                <div style="flex:1; overflow:auto; border: 1px solid var(--color-border); border-radius: 8px; background: rgba(0,0,0,0.2);">
                  <Table tableAction={tableFeatures}>
                    <thead>
                      <tr>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Proto</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Local</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Remote</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">State</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">PID</th>
                        <th style="padding:6px; text-align:left; color:var(--color-text-secondary); cursor:pointer;">Process</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each activeConnections as conn}
                        <tr 
                          style="border-bottom:1px solid rgba(255,255,255,0.02); cursor: pointer;"
                          onclick={() => openConnectionDetails(conn)}
                          oncontextmenu={(e) => handleConnectionContextMenu(e, conn)}
                        >
                          <td style="padding:6px; color:var(--color-text-primary);">{conn.protocol}</td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all;" title={conn.local_address}>{conn.local_address}</td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all;" title={conn.remote_address}>{conn.remote_address}</td>
                          <td style="padding:6px; color:var(--color-success);">{conn.state}</td>
                          <td style="padding:6px; color:var(--color-text-muted); font-family:var(--font-mono);">{conn.pid || '-'}</td>
                          <td style="padding:6px; color:var(--color-text-secondary);">{conn.process_name}</td>
                        </tr>
                      {/each}
                      {#if activeConnections.length === 0}
                        <tr>
                          <td colspan="6" style="padding:16px; text-align:center; color:var(--color-text-muted);">No active connections</td>
                        </tr>
                      {/if}
                    </tbody>
                  </Table>
                </div>
              </div>
            </div>
          </div>
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

  /* 2-column layout */
  .monitor-layout {
    display: grid;
    grid-template-columns: 4fr 6fr;
    gap: 20px;
    align-items: stretch;
    height: 100%;
    min-height: 0;
  }
  .monitor-column-left {
    display: flex;
    flex-direction: column;
    gap: 20px;
    height: 100%;
    min-height: 0;
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

  .monitor-panel {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.15);
    overflow: hidden;
    height: 100%;
    min-height: 0;
  }
  .panel-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 12px;
    margin: 0;
    flex-shrink: 0;
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
  .ram-stroke { stroke: var(--color-accent); }
  .ram-fill { fill: var(--color-accent); opacity: 0.15; }
  
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
  .ram-fill { background: linear-gradient(90deg, #f59e0b, #ef4444); }
  .swap-fill { background: linear-gradient(90deg, #ef4444, #b91c1c); }

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
    background: rgba(20, 20, 30, 0.95);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 8px 10px -6px rgba(0, 0, 0, 0.5);
    padding: 6px;
    min-width: 150px;
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
    color: var(--color-text-secondary);
    padding: 8px 12px;
    font-size: 12px;
    text-align: left;
    border-radius: 6px;
    cursor: pointer;
    width: 100%;
    transition: all 0.15s;
  }
  .custom-context-menu button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-primary);
  }
  .custom-context-menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
