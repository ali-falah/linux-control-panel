<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { Activity, Cpu, HardDrive, TerminalSquare, Copy, Check, ChevronRight, ChevronDown, CornerDownRight, Network } from '@lucide/svelte';
  import { RefreshCw, Skull, Loader, Wifi, Play, Pause, UnfoldVertical, FoldVertical, MemoryStick, ChartColumn } from '@lucide/svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Table from '../components/ui/Table.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import FilterChips, { type FilterOption } from '../components/ui/FilterChips.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import {
    PROCESS_PRIORITY_MAX,
    PROCESS_PRIORITY_MIN,
    canReniceProcess,
    isValidProcessPriority,
    processPriorityLabel,
  } from '../utils/processPriority';
  import {
    smartHealthGate,
    smartHealthIssueLabel,
    smartHealthTone,
    type SmartHealthReport,
  } from '../utils/smartHealth';

  type MonitorTab = 'resources' | 'network' | 'processes';

  function normalizeTab(value: string | null): MonitorTab {
    if (value === 'processes' || value === 'network') return value;
    return 'resources';
  }

  let currentTab = $state<MonitorTab>(normalizeTab(uiStore.targetSubTab));
  let currentUser = $state('unknown');

  // Overview Stats
  let stats = $state<any>(null);
  let smartReport = $state<SmartHealthReport | null>(null);
  let smartHealthLoaded = $state(false);
  let smartHealthLoading = $state(false);
  let smartFetchGeneration = 0;
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

  // Processes & Tree State
  let processes = $state<any[]>([]);
  let processSearch = $state(uiStore.processSearchQuery || '');

  $effect(() => {
    const target = uiStore.targetSubTab;
    if (
      target === 'processes' ||
      target === 'network' ||
      target === 'resources' ||
      target === 'overview'
    ) {
      currentTab = target === 'overview' ? 'resources' : target;
      // The tab request is one-shot; a later visit starts on the default tab.
      uiStore.targetSubTab = null;
    }
    if (uiStore.processSearchQuery !== undefined && uiStore.processSearchQuery !== null) {
      processSearch = uiStore.processSearchQuery;
      if (uiStore.processSearchQuery !== '') {
        currentTab = 'processes';
      }
    }
  });

  let isRefreshing = $state(false);
  let isPaused = $state(false);
  let lastUpdated = $state<Date | null>(null);
  let networkError = $state<string | null>(null);
  let processFilter = $state<'all' | 'mine' | 'system' | 'high-cpu'>('all');
  let connSortKey = $state<'protocol' | 'local' | 'remote' | 'state' | 'pid' | 'process' | null>(null);
  let connSortAsc = $state<boolean>(true);
  let expandedTreePids = $state<Set<number>>(new Set([1]));

  // Polls can overlap when a scan is slow; one in flight per domain is enough.
  let statsInFlight = false;
  let processInFlight = false;
  let networkInFlight = false;

  // Process Inspector Drawer State
  let isProcessDrawerOpen = $state(false);
  let inspectedProcess = $state<any | null>(null);
  let copiedCmd = $state(false);

  let inspectedParentProcess = $derived.by(() => {
    if (!inspectedProcess || !inspectedProcess.ppid) return null;
    return processes.find(p => p.pid === inspectedProcess.ppid) || null;
  });

  let inspectedChildProcesses = $derived.by(() => {
    if (!inspectedProcess) return [];
    return processes.filter(p => p.ppid === inspectedProcess.pid && p.pid !== inspectedProcess.pid);
  });

  function toggleTreeExpand(pid: number, e?: MouseEvent) {
    if (e) e.stopPropagation();
    const next = new Set(expandedTreePids);
    if (next.has(pid)) {
      next.delete(pid);
    } else {
      next.add(pid);
    }
    expandedTreePids = next;
  }

  function expandAllTree() {
    const allPids = new Set<number>();
    for (const p of processes) {
      allPids.add(p.pid);
    }
    expandedTreePids = allPids;
  }

  function collapseAllTree() {
    expandedTreePids = new Set();
  }

  let inspectedProcessPriority = $state(0);
  let renicing = $state(false);

  function openProcessInspector(proc: any, e?: MouseEvent) {
    if (e) e.stopPropagation();
    inspectedProcess = proc;
    inspectedProcessPriority = 0;
    isProcessDrawerOpen = true;
  }

  function canManageProcess(proc: any) {
    // `owned` comes from the backend; older rows without it keep the old
    // behaviour and let the backend refuse.
    return proc?.owned !== false;
  }

  function canSignalPid(pid?: number | null) {
    if (!pid || pid <= 100) return false;
    const target = processes.find((process) => process.pid === pid);
    return !target || canManageProcess(target);
  }

  async function applyRenice() {
    if (!inspectedProcess) return;
    if (!canManageProcess(inspectedProcess) || !canReniceProcess(inspectedProcess.user ?? '', currentUser)) {
      uiStore.addToast('You can only change the priority of your own processes.', 'error');
      return;
    }
    const priority = Number(inspectedProcessPriority);
    if (!isValidProcessPriority(priority)) {
      uiStore.addToast(
        `Priority must be an integer between ${PROCESS_PRIORITY_MIN} and ${PROCESS_PRIORITY_MAX}.`,
        'error'
      );
      return;
    }
    renicing = true;
    try {
      await invoke('renice_process', { pid: inspectedProcess.pid, priority });
      uiStore.addToast(`Adjusted priority for ${inspectedProcess.name} (PID ${inspectedProcess.pid}) to ${inspectedProcessPriority}`, 'success');
      statusStore.setLastCommand(`renice -n ${inspectedProcessPriority} -p ${inspectedProcess.pid}`, 0, true);
    } catch (e: any) {
      uiStore.addToast(e.toString(), 'error');
      statusStore.setLastCommand(`renice -n ${inspectedProcessPriority} -p ${inspectedProcess.pid}`, 1, false);
    } finally {
      renicing = false;
    }
  }

  let copiedPid = $state<number | null>(null);

  async function copyPid(pid: number) {
    try {
      await navigator.clipboard.writeText(pid.toString());
      copiedPid = pid;
      uiStore.addToast(`Copied PID ${pid} to clipboard`, 'info', 1500);
      setTimeout(() => {
        if (copiedPid === pid) copiedPid = null;
      }, 1800);
    } catch (err) {
      uiStore.handleError(err, 'Failed to copy PID');
    }
  }

  async function copyProcessCmdline(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedCmd = true;
      setTimeout(() => { copiedCmd = false; }, 2000);
      uiStore.addToast('Full command line copied to clipboard', 'info', 2000);
    } catch (err) {
      uiStore.handleError(err, 'Failed to copy command');
    }
  }

  async function sendProcessSignal(pid: number, signal: number, name: string) {
    if (pid <= 100) {
      uiStore.addToast(`PID ${pid} is a critical system process and cannot be signaled.`, 'warning');
      return;
    }
    const target = processes.find((process) => process.pid === pid);
    if (target && !canManageProcess(target)) {
      uiStore.addToast(`PID ${pid} belongs to another user; administrator rights are required.`, 'warning');
      return;
    }
    try {
      const res: string = await invoke('kill_process', { pid, signal });
      uiStore.addToast(res, 'info');
      if (inspectedProcess && inspectedProcess.pid === pid && (signal === 15 || signal === 9)) {
        isProcessDrawerOpen = false;
      }
      forceRefresh();
    } catch (err) {
      uiStore.handleError(err, `Failed to send signal to PID ${pid}`);
    }
  }

  function killProcessTree(parentPid: number, parentName: string) {
    const parent = processes.find((process) => process.pid === parentPid);
    if (parent && !canManageProcess(parent)) {
      uiStore.addToast('This process belongs to another user; administrator rights are required.', 'warning');
      return;
    }
    const children = processes.filter(
      (process) => process.ppid === parentPid && process.pid !== parentPid && canManageProcess(process)
    );
    const totalCount = children.length + 1;
    uiStore.confirm(
      `Terminate Process Tree`,
      `Kill ${parentName} (PID ${parentPid}) and all ${children.length} child processes (${totalCount} processes total)?`,
      async () => {
        for (const child of children) {
          if (child.pid > 100) {
            try {
              await invoke('kill_process', { pid: child.pid, signal: 15 });
            } catch (_) {}
          }
        }
        if (parentPid > 100) {
          try {
            await invoke('kill_process', { pid: parentPid, signal: 15 });
            uiStore.addToast(`Terminated process tree (${totalCount} processes)`, 'success');
          } catch (e: any) {
            uiStore.handleError(e, `Failed to terminate PID ${parentPid}`);
          }
        }
        isProcessDrawerOpen = false;
        forceRefresh();
      },
      true
    );
  }

  // Context Menu State for Active Connections
  let contextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    conn: any | null;
  }>({ x: 0, y: 0, show: false, conn: null });

  // Context Menu State for Process Tree
  let procContextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    proc: any | null;
  }>({ x: 0, y: 0, show: false, proc: null });

  // Side Drawer details State
  let isDrawerOpen = $state(false);
  let selectedConnection = $state<any | null>(null);
  let detailedProcess = $state<any | null>(null);
  let loadingProcessDetails = $state(false);

  function closeContextMenu() {
    contextMenu.show = false;
    procContextMenu.show = false;
  }

  function handleConnectionContextMenu(e: MouseEvent, conn: any) {
    e.preventDefault();
    e.stopPropagation();
    procContextMenu.show = false;
    contextMenu = {
      x: Math.min(e.clientX, window.innerWidth - 220),
      y: Math.min(e.clientY, window.innerHeight - 180),
      show: true,
      conn
    };
  }

  function handleProcContextMenu(e: MouseEvent, proc: any) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu.show = false;
    procContextMenu = {
      x: Math.min(e.clientX, window.innerWidth - 240),
      y: Math.min(e.clientY, window.innerHeight - 300),
      show: true,
      proc
    };
  }

  async function openConnectionDetails(conn: any) {
    selectedConnection = conn;
    isDrawerOpen = true;
    detailedProcess = null;
    if (conn.pid) {
      loadingProcessDetails = true;
      try {
        detailedProcess = await invoke<any | null>('get_process_details', { pid: conn.pid });
      } catch (err) {
        console.error("Error loading process details:", err);
      } finally {
        loadingProcessDetails = false;
      }
    }
  }

  // Timers
  let statsTimer: any;
  let topTimer: any;
  let networkTimer: any;
  let interfacesTimer: any;
  let processesTimer: any;

  function clearTimers() {
    clearInterval(statsTimer);
    clearInterval(topTimer);
    clearInterval(networkTimer);
    clearInterval(interfacesTimer);
    clearInterval(processesTimer);
  }

  // Enhanced monitoring state
  let consumerMetric = $state<'cpu' | 'mem'>('cpu');
  let connFilter = $state<'all' | 'listen' | 'estab' | 'external'>('all');
  let connSearch = $state('');

  // Top Consumers Derived
  let topConsumers = $derived.by(() => {
    if (!processes || processes.length === 0) return [];
    const sorted = [...processes].sort((a, b) =>
      consumerMetric === 'cpu'
        ? (b.cpu_percent || 0) - (a.cpu_percent || 0)
        : (b.mem_rss_mb || 0) - (a.mem_rss_mb || 0)
    );
    return sorted.slice(0, 5);
  });

  function openProcessByPid(pid: number) {
    processFilter = 'all';
    processSearch = String(pid);
    currentTab = 'processes';
  }

  function openAllProcesses() {
    processFilter = 'all';
    processSearch = '';
    currentTab = 'processes';
  }

  // Filtered Connections Derived
  let filteredConnections = $derived.by(() => {
    if (!activeConnections) return [];
    let list = activeConnections;

    if (connFilter === 'listen') {
      list = list.filter(c => c.state && c.state.toLowerCase().includes('listen'));
    } else if (connFilter === 'estab') {
      list = list.filter(c => c.state && (c.state.toLowerCase().includes('estab') || c.state.toLowerCase().includes('connected')));
    } else if (connFilter === 'external') {
      // The backend classifies the parsed remote host, including IPv6.
      list = list.filter(c => c.remote_is_external);
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

    if (connSortKey) {
      const direction = connSortAsc ? 1 : -1;
      list = list.slice().sort((a, b) => {
        let result = 0;
        switch (connSortKey) {
          case 'protocol':
            result = (a.protocol || '').localeCompare(b.protocol || '');
            break;
          case 'local':
            result =
              (a.local_host || '').localeCompare(b.local_host || '') ||
              (a.local_port ?? 0) - (b.local_port ?? 0);
            break;
          case 'remote':
            result =
              (a.remote_host || '').localeCompare(b.remote_host || '') ||
              (a.remote_port ?? 0) - (b.remote_port ?? 0);
            break;
          case 'state':
            result = (a.state || '').localeCompare(b.state || '');
            break;
          case 'pid':
            result = (a.pid ?? 0) - (b.pid ?? 0);
            break;
          case 'process':
            result = (a.process_name || '').localeCompare(b.process_name || '');
            break;
        }
        if (result === 0) result = (a.local_address || '').localeCompare(b.local_address || '');
        return result * direction;
      });
    }
    return list;
  });

  let connListenCount = $derived(activeConnections.filter(c => c.state && c.state.toLowerCase().includes('listen')).length);
  let connEstabCount = $derived(activeConnections.filter(c => c.state && (c.state.toLowerCase().includes('estab') || c.state.toLowerCase().includes('connected'))).length);
  let connExternalCount = $derived(activeConnections.filter(c => c.remote_is_external).length);

  let connFilterOptions = $derived.by(
    (): FilterOption<'all' | 'listen' | 'estab' | 'external'>[] => [
      { id: 'all', label: 'All', count: activeConnections.length },
      { id: 'listen', label: 'Listen', count: connListenCount || undefined },
      { id: 'estab', label: 'Estab', count: connEstabCount || undefined },
      { id: 'external', label: 'External', count: connExternalCount || undefined }
    ]
  );

  function toggleConnSort(key: 'protocol' | 'local' | 'remote' | 'state' | 'pid' | 'process') {
    if (connSortKey === key) {
      connSortAsc = !connSortAsc;
    } else {
      connSortKey = key;
      connSortAsc = true;
    }
  }

  function connectionStateVariant(state: string): 'success' | 'warning' | 'info' | 'muted' {
    const value = (state || '').toUpperCase();
    if (value.includes('LISTEN')) return 'warning';
    if (value.includes('ESTAB')) return 'success';
    if (value.includes('CLOSE') || value.includes('TIME') || value.includes('FIN')) return 'muted';
    return 'info';
  }

  function interfaceMeta(name: string) {
    return networkInterfaces.find((iface) => iface.name === name);
  }

  function formatEndpoint(host?: string | null, port?: number | null) {
    if (!host) return '—';
    const formatted = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
    return port != null ? `${formatted}:${port}` : formatted;
  }

  // Processes matching the filter; the tree builder runs on this subset.
  let filteredProcesses = $derived.by(() => {
    switch (processFilter) {
      case 'mine':
        return processes.filter((process) => process.owned !== false);
      case 'system':
        return processes.filter((process) => process.owned === false);
      case 'high-cpu':
        return processes.filter((process) => process.cpu_percent >= 10);
      default:
        return processes;
    }
  });

  let processFilterOptions = $derived.by(
    (): FilterOption<'all' | 'mine' | 'system' | 'high-cpu'>[] => {
      const mine = processes.filter((p) => p.owned !== false).length;
      const system = processes.filter((p) => p.owned === false).length;
      const highCpu = processes.filter((p) => p.cpu_percent >= 10).length;
      return [
        { id: 'all', label: 'All', count: processes.length },
        { id: 'mine', label: 'Mine', count: mine || undefined },
        { id: 'system', label: 'System', count: system || undefined },
        { id: 'high-cpu', label: 'High CPU', count: highCpu || undefined }
      ];
    }
  );

  let statsError = $state<string | null>(null);

  async function pollStats() {
    if (statsInFlight) return;
    statsInFlight = true;
    try {
      const [sysStats, temp] = await Promise.all([
        invoke('get_system_stats'),
        invoke('get_cpu_temperature')
      ]);
      const next = sysStats as any;
      stats = next;
      cpuTemp = temp as number | null;
      statsError = null;
      lastUpdated = new Date();
      cpuHistory = [...cpuHistory.slice(1), next.cpu_percent];
      ramHistory = [...ramHistory.slice(1), next.ram_percent];
      swapHistory = [...swapHistory.slice(1), next.swap_percent];
    } catch (e) {
      statsError = e instanceof Error ? e.message : String(e);
    } finally {
      statsInFlight = false;
    }
    await fetchDiskIo();
  }

  async function pollInterfaces() {
    try {
      networkInterfaces = await invoke('get_network_interfaces');
    } catch (e) {
      // Interfaces change rarely; keep the last known list.
      console.warn('Could not refresh network interfaces', e);
    }
  }

  async function pollNetwork() {
    if (networkInFlight) return;
    networkInFlight = true;
    try {
      const results = await Promise.allSettled([
        fetchNetworkTraffic(),
        fetchActiveConnections()
      ]);
      const failed = results.find((result) => result.status === 'rejected');
      if (failed && failed.status === 'rejected') {
        networkError = failed.reason instanceof Error
          ? failed.reason.message
          : String(failed.reason);
      } else {
        networkError = null;
      }
      lastUpdated = new Date();
    } finally {
      networkInFlight = false;
    }
  }

  async function pollProcesses() {
    if (processInFlight) return;
    processInFlight = true;
    try {
      processes = await invoke('get_process_list');
      lastUpdated = new Date();
    } catch (e) {
      // Keep the last list; a transient scan failure should not blank the table.
      console.warn('Could not refresh the process list', e);
    } finally {
      processInFlight = false;
    }
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
  }

  async function fetchActiveConnections() {
    activeConnections = await invoke('get_active_connections');
  }

  function restartTimers() {
    clearTimers();
    if (isPaused || uiStore.isThrottled) return;
    if (currentTab === 'resources') {
      // Stats are cheap; the process list is only needed for Top Consumers and
      // is scanned less often.
      pollStats();
      pollProcesses();
      statsTimer = setInterval(pollStats, 2000);
      topTimer = setInterval(pollProcesses, 6000);
    } else if (currentTab === 'network') {
      pollInterfaces();
      pollNetwork();
      interfacesTimer = setInterval(pollInterfaces, 15000);
      networkTimer = setInterval(pollNetwork, 3000);
    } else if (currentTab === 'processes') {
      pollProcesses();
      processesTimer = setInterval(pollProcesses, 3000);
    }
  }

  function handleVisibilityChange() {
    if (uiStore.isThrottled) {
      clearTimers();
    } else {
      restartTimers();
    }
  }

  $effect(() => {
    // React to tab switch, pause toggle, or window focus/visibility changes
    const _throttled = uiStore.isThrottled;
    const _tab = currentTab;
    const _paused = isPaused;
    restartTimers();
  });

  $effect(() => {
    const adminOn = statusStore.adminMode.on;
    const decision = smartHealthGate(adminOn, smartHealthLoaded);
    smartHealthLoaded = decision.loaded;
    if (!adminOn) {
      // Invalidate any in-flight request and hide stale results.
      smartFetchGeneration += 1;
      smartHealthLoading = false;
      smartReport = null;
      return;
    }
    if (decision.fetch) {
      void loadSmartHealth();
    }
  });

  async function loadSmartHealth() {
    const generation = ++smartFetchGeneration;
    smartHealthLoading = true;
    try {
      const report = await invoke<SmartHealthReport>('get_smart_health');
      if (generation !== smartFetchGeneration || !statusStore.adminMode.on) return;
      smartReport = report;
    } catch {
      if (generation !== smartFetchGeneration || !statusStore.adminMode.on) return;
      smartReport = {
        disks: [],
        errors: [{ disk_path: 'SMART scan', error: 'SMART health could not be loaded' }],
      };
    } finally {
      if (generation === smartFetchGeneration) smartHealthLoading = false;
    }
  }

  onMount(async () => {
    document.addEventListener('visibilitychange', handleVisibilityChange);
    try {
      currentUser = await invoke('get_current_user');
      const history: any[] = await invoke('get_system_stats_history');
      if (history && history.length > 0) {
        const padHistory = (values: number[]) =>
          values.length < 40
            ? [...Array(40 - values.length).fill(0), ...values]
            : values.slice(-40);
        cpuHistory = padHistory(history.map((sample) => sample.cpu_percent));
        ramHistory = padHistory(history.map((sample) => sample.ram_percent));
        swapHistory = padHistory(history.map((sample) => sample.swap_percent));
      }
    } catch(e) {
      console.error(e);
    }
  });

  onDestroy(() => {
    clearTimers();
    document.removeEventListener('visibilitychange', handleVisibilityChange);
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

  // Idle interfaces (virtual NICs, bridges) are dropped so the card row stays
  // short; when nothing is moving we fall back to the full list rather than
  // showing an empty strip.
  const busyInterfaces = $derived(
    activeInterfaces.filter((iface) => {
      const speeds = networkSpeeds[iface.interface];
      return !!speeds && (speeds.rxSpeed > 0 || speeds.txSpeed > 0);
    })
  );
  const shownInterfaces = $derived(busyInterfaces.length > 0 ? busyInterfaces : activeInterfaces);

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

  function killProcess(pid: number, name: string, signal: number = 15) {
    const signalLabel = signal === 9 ? 'SIGKILL (Force Kill)' : (signal === 1 ? 'SIGHUP (Reload)' : 'SIGTERM (Terminate)');
    uiStore.confirm(
      `Confirm Signal ${signalLabel}`,
      `Send ${signalLabel} to process ${name} (PID ${pid})?`,
      async () => {
        try {
          const res = await invoke('kill_process', { pid, signal });
          statusStore.setLastCommand(`kill -${signal} ${pid}`, 0, true);
          uiStore.addToast(res as string, 'success');
          pollProcesses();
        } catch (e: any) {
          statusStore.setLastCommand(`kill -${signal} ${pid}`, 1, false);
          uiStore.addToast(e.toString(), 'error');
        }
      },
      signal === 9
    );
  }

  // Process Sorting State
  let procSortKey = $state<'pid' | 'name' | 'user' | 'cpu' | 'mem' | 'rss'>('pid');
  let procSortAsc = $state<boolean>(true);

  function toggleProcSort(key: 'pid' | 'name' | 'user' | 'cpu' | 'mem' | 'rss') {
    if (procSortKey === key) {
      procSortAsc = !procSortAsc;
    } else {
      procSortKey = key;
      procSortAsc = key === 'pid' || key === 'name' || key === 'user';
    }
  }

  // Build parent-child process hierarchy for Tree View
  let treeVisibleProcesses = $derived.by(() => {
    const search = processSearch.toLowerCase().trim();
    // Filtering happens before the tree is built: a hidden parent simply makes
    // its visible children roots instead of leaving them unreachable.
    const source = filteredProcesses;

    // Map of all processes
    const map = new Map<number, any>();
    for (const p of source) {
      map.set(p.pid, p);
    }

    // Tree-aware sorting function
    function sortProcs(arr: any[]) {
      return arr.slice().sort((a, b) => {
        let cmp = 0;
        switch (procSortKey) {
          case 'pid': cmp = a.pid - b.pid; break;
          case 'name': cmp = a.name.localeCompare(b.name); break;
          case 'user': cmp = (a.user || '').localeCompare(b.user || ''); break;
          case 'cpu': cmp = a.cpu_percent - b.cpu_percent; break;
          case 'mem': cmp = a.mem_percent - b.mem_percent; break;
          case 'rss': cmp = a.mem_rss_mb - b.mem_rss_mb; break;
        }
        return procSortAsc ? cmp : -cmp;
      });
    }

    // Map parent PID -> list of child processes
    const childrenMap = new Map<number, any[]>();
    let roots: any[] = [];

    for (const p of source) {
      const parentId = p.ppid;
      if (parentId && map.has(parentId) && parentId !== p.pid) {
        if (!childrenMap.has(parentId)) childrenMap.set(parentId, []);
        childrenMap.get(parentId)!.push(p);
      } else {
        roots.push(p);
      }
    }

    roots = sortProcs(roots);

    // If searching, identify matching PIDs and their ancestor chain
    let matchingPids = new Set<number>();
    let matchingAncestorPids = new Set<number>();

    if (search) {
      for (const p of source) {
        const matches = p.name.toLowerCase().includes(search) ||
          p.pid.toString().includes(search) ||
          (p.cmdline && p.cmdline.toLowerCase().includes(search)) ||
          (p.user && p.user.toLowerCase().includes(search));
        
        if (matches) {
          matchingPids.add(p.pid);
          let curr = p;
          while (curr.ppid && map.has(curr.ppid) && curr.ppid !== curr.pid) {
            matchingAncestorPids.add(curr.ppid);
            curr = map.get(curr.ppid)!;
          }
        }
      }
    }

    const result: Array<{ 
      process: any; 
      depth: number; 
      hasChildren: boolean; 
      childrenCount: number; 
      isExpanded: boolean;
    }> = [];

    function walk(proc: any, depth: number) {
      if (search && !matchingPids.has(proc.pid) && !matchingAncestorPids.has(proc.pid)) {
        return;
      }

      const rawChildren = childrenMap.get(proc.pid) || [];
      const children = sortProcs(rawChildren);
      const hasChildren = children.length > 0;
      const isExpanded = search ? (matchingAncestorPids.has(proc.pid) || expandedTreePids.has(proc.pid)) : expandedTreePids.has(proc.pid);

      result.push({ 
        process: proc, 
        depth, 
        hasChildren, 
        childrenCount: children.length, 
        isExpanded 
      });

      if (hasChildren && isExpanded) {
        for (const child of children) {
          walk(child, depth + 1);
        }
      }
    }

    for (const root of roots) {
      walk(root, 0);
    }

    return result;
  });

  // Virtual Scrolling State for Process Table
  let procScrollTop = $state(0);
  let procContainerHeight = $state(600);
  const PROC_OVERSCAN = 15;

  // Rows contain two lines (name + command). These heights match the table
  // density CSS; the row sets its own height so the virtualizer stays in sync.
  let procRowHeight = $derived(
    uiStore.tableDensity === 'compact' ? 34 : (uiStore.tableDensity === 'spacious' ? 57 : 44)
  );
  let totalProcCount = $derived(treeVisibleProcesses.length);

  let procStartIndex = $derived.by(() => {
    if (totalProcCount <= 40) return 0;
    const raw = Math.floor(procScrollTop / procRowHeight) - PROC_OVERSCAN;
    return Math.max(0, Math.min(raw, Math.max(0, totalProcCount - 1)));
  });

  let procVisibleCount = $derived.by(() => {
    if (totalProcCount <= 40) return totalProcCount;
    return Math.ceil(procContainerHeight / procRowHeight) + 2 * PROC_OVERSCAN;
  });

  let procEndIndex = $derived(
    totalProcCount <= 40
      ? totalProcCount
      : Math.min(totalProcCount, procStartIndex + procVisibleCount)
  );

  let visibleProcesses = $derived(
    totalProcCount <= 40
      ? treeVisibleProcesses
      : treeVisibleProcesses.slice(procStartIndex, procEndIndex)
  );

  let procTopPadding = $derived(totalProcCount <= 40 ? 0 : procStartIndex * procRowHeight);
  let procBottomPadding = $derived(totalProcCount <= 40 ? 0 : Math.max(0, (totalProcCount - procEndIndex) * procRowHeight));

  function handleProcessScroll(e: Event) {
    const target = e.currentTarget as HTMLElement;
    if (target) {
      procScrollTop = target.scrollTop;
      procContainerHeight = target.clientHeight || 600;
    }
  }

  async function forceRefresh() {
    // the header button spins/pulses off this flag — without it the refresh
    // looked like a dead click
    isRefreshing = true;
    try {
      if (currentTab === 'resources') {
        await Promise.all([pollStats(), pollProcesses()]);
        if (statusStore.adminMode.on) {
          void loadSmartHealth();
        }
      } else if (currentTab === 'network') {
        await Promise.all([pollInterfaces(), pollNetwork()]);
      } else {
        await pollProcesses();
      }
    } finally {
      isRefreshing = false;
    }
  }
</script>

<div class="module-page">
  <PageHeader title="Monitoring" subtitle="Real-time system health and process management." icon={Activity}>
    <TabGroup size="sm"
      tabs={[
        { id: 'resources', label: 'Resources' },
        { id: 'network', label: 'Network', count: activeConnections.length },
        { id: 'processes', label: 'Processes', count: processes.length }
      ]}
      bind:activeTab={currentTab}
      style="margin-right: 8px;"
    />
    <span class="live-status" title="Polling state and the last successful update">
      <span class="live-dot" class:paused={isPaused}></span>
      {isPaused ? 'Paused' : 'Live'}
      {#if lastUpdated}
        <span class="live-time">· {lastUpdated.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</span>
      {/if}
    </span>
    <div style="display:flex; gap: 8px;">
      <Button size="sm" onclick={() => isPaused = !isPaused} variant={isPaused ? 'primary' : 'ghost'}>
        {#if isPaused}
          <Play size={14} style="margin-right: 4px;" /> Resume
        {:else}
          <Pause size={14} style="margin-right: 4px;" /> Pause
        {/if}
      </Button>
      <button class="header-refresh-btn" onclick={forceRefresh} disabled={isRefreshing}>
        <RefreshCw size={14} class={isRefreshing ? 'animate-spin-slow' : ''} />
        Refresh
      </button>
    </div>
  </PageHeader>

  <div class="page-content">
    {#if currentTab === 'resources'}
      {#if stats}
        {#if statsError}
          <div class="monitor-load-error" style="flex:0 0 auto;" role="alert">
            <span>Showing the last reading. Refresh failed: {statsError}</span>
            <Button variant="outline" size="sm" onclick={() => pollStats()}>Retry</Button>
          </div>
        {/if}
        <div class="resources-columns">
          <div class="resources-main">
            <Card title="CPU" icon={Cpu} class="monitor-panel">
              <div class="panel-scroll" style="gap:10px;">
                <div class="cpu-summary">
                  <span class="metric-label">Usage</span>
                  <span class="cpu-summary-values">
                    {#if cpuTemp !== null}
                      <span class="temp-badge" class:hot={cpuTemp >= 85} class:warm={cpuTemp >= 70}>
                        {cpuTemp.toFixed(0)}&deg;C
                      </span>
                    {/if}
                    <strong>{stats.cpu_percent.toFixed(1)}%</strong>
                  </span>
                </div>
                <div class="cpu-chart">
                  <svg viewBox="0 0 200 70" preserveAspectRatio="none">
                    <path d={generateSparkline(cpuHistory, 70, 200)} class="spark-fill cpu-fill" />
                    <polyline
                      points={generateSparklineStroke(cpuHistory, 70, 200)}
                      class="spark-stroke cpu-stroke"
                      fill="none"
                    />
                  </svg>
                </div>
                <div class="load-row">
                  <span>{stats.cpu_cores} Cores</span>
                  <span>Load {stats.load_1.toFixed(2)} / {stats.load_5.toFixed(2)} / {stats.load_15.toFixed(2)}</span>
                </div>
                {#if stats.cpu_per_core && stats.cpu_per_core.length > 0}
                  <div class="core-grid" aria-label="Per-core CPU usage">
                    {#each stats.cpu_per_core as corePct, idx}
                      <div class="core-tile" class:hot={corePct >= 70} class:warm={corePct >= 40}>
                        <div class="core-head">
                          <span>C{idx}</span>
                          <strong>{corePct.toFixed(0)}%</strong>
                        </div>
                        <div class="core-bar"><span style="width: {corePct}%"></span></div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            </Card>

            <Card title="Top Consumers" icon={ChartColumn} class="monitor-panel">
              {#snippet headerActions()}
                <button type="button" class="link-btn" onclick={openAllProcesses}>
                  View all &rarr;
                </button>
              {/snippet}
              <div class="consumer-switch" role="group" aria-label="Rank processes by">
                <button
                  type="button"
                  class:active={consumerMetric === 'cpu'}
                  onclick={() => consumerMetric = 'cpu'}
                >
                  CPU
                </button>
                <button
                  type="button"
                  class:active={consumerMetric === 'mem'}
                  onclick={() => consumerMetric = 'mem'}
                >
                  Memory
                </button>
              </div>
              {#if topConsumers.length > 0}
                <div class="consumer-list">
                  {#each topConsumers as proc, index (proc.pid)}
                    <button
                      type="button"
                      class="consumer-row"
                      onclick={() => openProcessByPid(proc.pid)}
                      title={`${proc.cmdline || proc.name} — open PID ${proc.pid} in Processes`}
                    >
                      <span class="consumer-rank">{index + 1}</span>
                      <span class="consumer-name">{proc.name}</span>
                      <span class="consumer-pid">PID {proc.pid}</span>
                      <span class="consumer-value" class:mem={consumerMetric === 'mem'}>
                        {consumerMetric === 'cpu'
                          ? `${proc.cpu_percent.toFixed(1)}%`
                          : formatBytes(proc.mem_rss_mb)}
                      </span>
                      <span class="consumer-arrow"><ChevronRight size={13} /></span>
                    </button>
                  {/each}
                </div>
              {:else}
                <span class="text-muted" style="font-size:11px;">Collecting process samples…</span>
              {/if}
            </Card>
          </div>

          <div class="resources-side">
            <Card title="Memory & Swap" icon={MemoryStick} class="monitor-panel">
              <div class="panel-scroll" style="gap:14px;">
                <div class="metric-block">
                  <div class="metric-row">
                    <span>Memory</span>
                    <strong>{stats.ram_percent.toFixed(1)}%</strong>
                  </div>
                  <div class="progress-bg">
                    <div class="progress-fill ram-fill" style="width: {stats.ram_percent}%"></div>
                  </div>
                  <div class="mini-chart">
                    <svg viewBox="0 0 200 30" preserveAspectRatio="none">
                      <polyline points={generateSparklineStroke(ramHistory, 30, 200)} class="spark-stroke ram-stroke" />
                    </svg>
                  </div>
                  <div class="metric-foot">
                    <span>Used: {formatBytes(stats.ram_used_mb)}</span>
                    <span>Total: {formatBytes(stats.ram_total_mb)}</span>
                  </div>
                </div>

                {#if stats.swap_total_mb > 0}
                  <div class="metric-block">
                    <div class="metric-row">
                      <span>Swap</span>
                      <strong>{stats.swap_percent.toFixed(1)}%</strong>
                    </div>
                    <div class="progress-bg">
                      <div class="progress-fill swap-fill" style="width: {stats.swap_percent}%"></div>
                    </div>
                    <div class="mini-chart">
                      <svg viewBox="0 0 200 30" preserveAspectRatio="none">
                        <polyline points={generateSparklineStroke(swapHistory, 30, 200)} class="spark-stroke swap-stroke" />
                      </svg>
                    </div>
                    <div class="metric-foot">
                      <span>Used: {formatBytes(stats.swap_used_mb)}</span>
                      <span>Total: {formatBytes(stats.swap_total_mb)}</span>
                    </div>
                  </div>
                {:else}
                  <div class="metric-row">
                    <span>Swap</span>
                    <span class="text-muted" style="font-size:11px;">Not configured</span>
                  </div>
                {/if}
              </div>
            </Card>

            <Card title="Disk I/O" icon={HardDrive} class="monitor-panel">
              <div class="panel-scroll" style="gap:10px;">
                {#if diskIoStats.length > 0}
                  {#each diskIoStats as disk}
                    <div class="disk-card">
                      <div class="disk-head">
                        <div class="disk-identity">
                          <div class="disk-icon"><HardDrive size={16} /></div>
                          <span class="disk-name">{disk.device}</span>
                        </div>
                        <div class="disk-speeds">
                          <div class="disk-speed">
                            <span>READ</span>
                            <strong>{formatSpeed(diskIoSpeeds[disk.device]?.readSpeed || 0)}</strong>
                          </div>
                          <div class="disk-speed write">
                            <span>WRITE</span>
                            <strong>{formatSpeed(diskIoSpeeds[disk.device]?.writeSpeed || 0)}</strong>
                          </div>
                        </div>
                      </div>
                      <div class="disk-chart">
                        <svg viewBox="0 0 300 36" preserveAspectRatio="none">
                          <polyline
                            points={generateDynamicSparklineStroke(diskIoHistory[disk.device] || [], 36, 300)}
                            class="spark-stroke"
                          />
                        </svg>
                      </div>
                    </div>
                  {/each}
                {:else}
                  <span class="text-muted">Loading disk I/O metrics…</span>
                {/if}
              </div>
            </Card>

            <Card title="SMART Health" icon={HardDrive} class="monitor-panel">
              <div class="panel-scroll" style="gap:8px;">
                {#if !statusStore.adminMode.on}
                  <span class="text-muted" style="font-size:11px;">
                    Admin mode is required to scan SMART health. Enable it in the status bar.
                  </span>
                {:else if smartHealthLoading}
                  <span class="text-muted">Scanning SMART health…</span>
                {:else if smartReport && (smartReport.disks.length > 0 || smartReport.errors.length > 0)}
                  {#each smartReport.disks as disk}
                    {@const tone = smartHealthTone(disk.health_status)}
                    <div class="smart-card">
                      <div class="smart-head">
                        <span class="smart-device">{disk.disk_path}</span>
                        <span
                          class="badge"
                          style="font-size:10px; color:{tone === 'success' ? 'var(--color-success)' : tone === 'error' ? 'var(--color-error)' : 'var(--color-warning)'};"
                        >
                          {disk.health_status}
                        </span>
                      </div>
                      <span class="text-muted" style="font-size:11px;">{disk.model}</span>
                    </div>
                  {/each}
                  {#each smartReport.errors as issue}
                    <div role="alert" class="smart-error">
                      {smartHealthIssueLabel(issue)}
                    </div>
                  {/each}
                {:else}
                  <span class="text-muted">No SMART devices reported.</span>
                {/if}
              </div>
            </Card>
          </div>
        </div>
      {:else if statsError}
        <div class="monitor-load-error" role="alert">
          <span>Could not load system metrics: {statsError}</span>
          <Button variant="outline" size="sm" onclick={() => pollStats()}>Retry</Button>
        </div>
      {:else}
        <div class="loading-state">
          <Loader size={24} class="animate-spin-slow" /> Loading system metrics...
        </div>
      {/if}
    {/if}

    {#if currentTab === 'network'}
      {#if networkError}
        <div class="monitor-load-error" style="flex:0 0 auto;" role="alert">
          <span>Showing the last connections. Refresh failed: {networkError}</span>
          <Button variant="outline" size="sm" onclick={() => pollNetwork()}>Retry</Button>
        </div>
      {/if}
      <div class="network-grid">
        <!-- Network & Connections -->
        <Card title="Network & Connections" icon={Wifi} class="monitor-panel" style="display: flex; flex-direction: column; height: 100%; min-height: 0;">
          {#snippet headerActions()}
            <div class="iface-strip">
              {#each shownInterfaces as iface}
                {@const meta = interfaceMeta(iface.interface)}
                <span
                  class="iface-chip"
                  title={`${iface.interface}${meta?.ip4 ? ` · ${meta.ip4}` : meta?.ip6 ? ` · ${meta.ip6}` : ''}`}
                >
                  <span class="iface-dot"></span>
                  <span class="iface-name">{iface.interface}</span>
                  {#if meta?.ip4 || meta?.ip6}
                    <span class="iface-ip">{meta.ip4 || meta.ip6}</span>
                  {/if}
                  <span class="iface-rates">
                    <span title="Download">↓ {formatSpeed(networkSpeeds[iface.interface]?.rxSpeed || 0)}</span>
                    <span title="Upload">↑ {formatSpeed(networkSpeeds[iface.interface]?.txSpeed || 0)}</span>
                  </span>
                </span>
              {/each}
              {#if shownInterfaces.length === 0}
                <span class="iface-empty">No active interfaces</span>
              {/if}
            </div>
          {/snippet}
          <div style="display:flex; flex-direction:column; flex:1; min-height:0; gap:8px;">
                <div class="network-toolbar">
                  <h4 style="margin: 0; font-size: 12px; color: var(--color-text-secondary); font-weight: 600; white-space: nowrap; flex-shrink: 0;">
                    Connections
                  </h4>

                  <div class="conn-search">
                    <input
                      type="text"
                      placeholder="Search IP, port, PID, process…"
                      aria-label="Search connections by IP, port, PID, or process"
                      bind:value={connSearch}
                    />
                  </div>

                  <FilterChips
                    options={connFilterOptions}
                    bind:value={connFilter}
                    label="Filter connections"
                    style="flex-shrink:0;"
                  />
                  <span class="network-count">
                    {filteredConnections.length} shown
                  </span>
                </div>

                <div style="flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--color-border); border-radius: 8px;">
                  <Table
                    tableAction={tableFeatures}
                    actionParams={{ sortable: false }}
                    class="active-conn-table"
                    style="flex: 1; min-height: 0; overflow-y: auto;"
                  >
                    <thead>
                      <tr>
                        <th class="sortable-th" onclick={() => toggleConnSort('protocol')}>
                          <div class="th-content">Proto <span class="th-sort-icon">{connSortKey === 'protocol' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="sortable-th" onclick={() => toggleConnSort('local')}>
                          <div class="th-content">Local <span class="th-sort-icon">{connSortKey === 'local' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="sortable-th" onclick={() => toggleConnSort('remote')}>
                          <div class="th-content">Remote <span class="th-sort-icon">{connSortKey === 'remote' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="sortable-th" onclick={() => toggleConnSort('state')}>
                          <div class="th-content">State <span class="th-sort-icon">{connSortKey === 'state' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="sortable-th" onclick={() => toggleConnSort('pid')}>
                          <div class="th-content">PID <span class="th-sort-icon">{connSortKey === 'pid' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="sortable-th" onclick={() => toggleConnSort('process')}>
                          <div class="th-content">Process <span class="th-sort-icon">{connSortKey === 'process' ? (connSortAsc ? '↑' : '↓') : '↕'}</span></div>
                        </th>
                        <th class="col-actions" aria-label="Actions"></th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each filteredConnections as conn (conn.protocol + conn.local_address + conn.remote_address + (conn.pid ?? ''))}
                        {@const isListen = conn.state && conn.state.toLowerCase().includes('listen')}
                        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                        <tr
                          tabindex="0"
                          style="border-bottom:1px solid var(--color-border-subtle); cursor: pointer; background: {isListen ? 'rgba(245, 158, 11, 0.04)' : 'transparent'};"
                          onclick={() => openConnectionDetails(conn)}
                          oncontextmenu={(e) => handleConnectionContextMenu(e, conn)}
                          onkeydown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault();
                              openConnectionDetails(conn);
                            }
                          }}
                        >
                          <td style="padding:6px; color:var(--color-text-primary); font-family:var(--font-mono); font-size:11px;">{conn.protocol}</td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all; font-family:var(--font-mono); font-size:11px;" title={conn.local_address}>
                            {formatEndpoint(conn.local_host, conn.local_port)}
                          </td>
                          <td style="padding:6px; color:var(--color-text-primary); word-break:break-all; font-family:var(--font-mono); font-size:11px;" title={conn.remote_address}>
                            {formatEndpoint(conn.remote_host, conn.remote_port)}
                          </td>
                          <td style="padding:6px;">
                            <Badge variant={connectionStateVariant(conn.state)}>{conn.state}</Badge>
                          </td>
                          <td style="padding:6px; color:var(--color-text-muted); font-family:var(--font-mono); font-size:11px;">{conn.pid || '-'}</td>
                          <td style="padding:6px; color:var(--color-text-secondary); font-size:11px;">
                            {#if conn.process_name && conn.process_name !== 'N/A'}
                              {conn.process_name}
                            {:else}
                              <span style="color:var(--color-text-muted);" title="The socket owner is only visible with administrator access">—</span>
                            {/if}
                          </td>
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
                                  disabled={!canSignalPid(conn.pid)}
                                  title={canSignalPid(conn.pid)
                                    ? 'Terminate the owning process'
                                    : 'This process belongs to another user; administrator rights are required.'}
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
          </Card>
      </div>
    {/if}

    {#if currentTab === 'processes'}
      <!-- Processes Tab (Pure Tree View) -->
      <div class="table-container" style="flex:1; min-height:0; display:flex; flex-direction:column;">
        <div class="table-toolbar process-toolbar">
          <div class="search-box process-search">
            <input
              type="text"
              placeholder="Search name, PID, or user…"
              aria-label="Search processes by name, PID, or user"
              bind:value={processSearch}
            />
          </div>

          <FilterChips
            options={processFilterOptions}
            bind:value={processFilter}
            label="Filter processes"
            style="flex-shrink:0;"
          />

          <div class="process-tree-actions">
            <button
              type="button"
              class="toolbar-icon-btn"
              onclick={expandAllTree}
              title="Expand all branches"
              aria-label="Expand all branches"
            >
              <UnfoldVertical size={14} />
            </button>
            <button
              type="button"
              class="toolbar-icon-btn"
              onclick={collapseAllTree}
              title="Collapse all branches"
              aria-label="Collapse all branches"
            >
              <FoldVertical size={14} />
            </button>
          </div>

          <div
            class="toolbar-stats"
            title={`${treeVisibleProcesses.length} visible of ${filteredProcesses.length} matching (${processes.length} loaded)`}
          >
            {treeVisibleProcesses.length} / {filteredProcesses.length}
          </div>
        </div>

        <Table class="process-table-wrap" onscroll={handleProcessScroll} style="flex:1; min-height:0; overflow-y:auto; border:none; border-radius:0;">
          <thead>
            <tr>
              <th class="col-pid sortable-th" onclick={() => toggleProcSort('pid')}>
                <div class="th-content">PID <span class="th-sort-icon">{procSortKey === 'pid' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-name sortable-th" onclick={() => toggleProcSort('name')}>
                <div class="th-content">Name <span class="th-sort-icon">{procSortKey === 'name' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-user sortable-th" onclick={() => toggleProcSort('user')}>
                <div class="th-content">User <span class="th-sort-icon">{procSortKey === 'user' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-cpu sortable-th" onclick={() => toggleProcSort('cpu')}>
                <div class="th-content">CPU % <span class="th-sort-icon">{procSortKey === 'cpu' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-mem sortable-th" onclick={() => toggleProcSort('mem')}>
                <div class="th-content">Mem % <span class="th-sort-icon">{procSortKey === 'mem' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-rss sortable-th" onclick={() => toggleProcSort('rss')}>
                <div class="th-content">RSS <span class="th-sort-icon">{procSortKey === 'rss' ? (procSortAsc ? '↑' : '↓') : '↕'}</span></div>
              </th>
              <th class="col-actions"></th>
            </tr>
          </thead>
          <tbody>
            {#if procTopPadding > 0}
              <tr style="height: {procTopPadding}px; border: none !important; pointer-events: none;"><td colspan="7" style="padding: 0; border: none !important; height: {procTopPadding}px;"></td></tr>
            {/if}
            {#each visibleProcesses as item (item.process.pid)}
              {@const p = item.process}
              <tr
                class="proc-row tree-row"
                class:is-root={p.user === 'root'}
                class:is-kernel={p.pid <= 100}
                class:is-parent={item.hasChildren}
                class:is-expanded-parent={item.hasChildren && item.isExpanded}
                class:is-child={item.depth > 0}
                class:depth-1={item.depth === 1}
                class:depth-2={item.depth === 2}
                class:depth-3={item.depth >= 3}
                onclick={() => item.hasChildren ? toggleTreeExpand(p.pid) : undefined}
                oncontextmenu={(e) => handleProcContextMenu(e, p)}
                style="cursor: {item.hasChildren ? 'pointer' : 'default'}; height: {procRowHeight}px;"
              >
                <td class="col-pid" onclick={(e) => e.stopPropagation()}>
                  <div class="pid-badge-container">
                    <span class="pid-text" class:parent-pid={item.hasChildren && item.isExpanded} class:child-pid={item.depth > 0}>
                      {p.pid}
                    </span>
                    <button
                      type="button"
                      class="pid-copy-btn"
                      onclick={(e) => { e.stopPropagation(); copyPid(p.pid); }}
                      title="Copy PID {p.pid}"
                    >
                      {#if copiedPid === p.pid}
                        <Check size={11} class="pid-copied-icon" />
                      {:else}
                        <Copy size={11} class="pid-copy-icon" />
                      {/if}
                    </button>
                  </div>
                </td>
                <td class="col-name">
                  <div class="tree-cell-wrapper" style="margin-left: {item.depth > 0 ? (item.depth * 44) : 0}px;">
                    <div style="display:flex; align-items:center; gap:6px;">
                      {#if item.depth > 0}
                        <!-- Multi-depth visual branch guide tracks -->
                        <div class="tree-indent-tracks">
                          {#each Array(item.depth) as _, d}
                            {#if d === item.depth - 1}
                              <span class="tree-branch-guide active-elbow" title="Sub-process of parent">
                                <CornerDownRight size={13} class="branch-elbow-icon" />
                              </span>
                            {:else}
                              <span class="tree-guide-vertical-bar"></span>
                            {/if}
                          {/each}
                        </div>
                      {/if}

                      {#if item.hasChildren}
                        <!-- Clickable on Arrow + Process Name + Count Badge to expand/collapse -->
                        <button
                          type="button"
                          class="tree-node-trigger"
                          class:is-expanded={item.isExpanded}
                          onclick={(e) => toggleTreeExpand(p.pid, e)}
                          title={item.isExpanded ? 'Click to collapse sub-processes' : `Click to expand (${item.childrenCount} sub-processes)`}
                        >
                          <span class="tree-chevron-icon" class:is-expanded={item.isExpanded}>
                            <ChevronRight size={13} />
                          </span>
                          <span class="proc-name parent-proc-name" class:expanded-name={item.isExpanded}>{p.name}</span>
                          <span class="tree-child-badge" class:expanded-badge={item.isExpanded}>
                            {item.childrenCount}
                          </span>
                        </button>
                      {:else}
                        <span class="proc-name" class:child-proc-name={item.depth > 0}>{p.name}</span>
                      {/if}
                    </div>
                    <div class="proc-cmd" style="padding-left: {item.hasChildren ? 22 : (item.depth > 0 ? 24 : 0)}px;">
                      {p.cmdline || p.name}
                    </div>
                  </div>
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
                <td class="col-actions" onclick={(e) => e.stopPropagation()}>
                  <KebabMenu align="right">
                    <button class="menu-item" onclick={() => openProcessInspector(p)}>
                      <Activity size={14} style="color: var(--color-info);" />
                      Inspect Details
                    </button>
                    {#if p.pid > 100}
                      {#if canManageProcess(p)}
                        <button class="menu-item danger" onclick={() => sendProcessSignal(p.pid, 15, p.name)}>
                          <Skull size={14} style="color: var(--color-error);" />
                          Soft Kill (SIGTERM)
                        </button>
                        <button class="menu-item danger" onclick={() => sendProcessSignal(p.pid, 9, p.name)}>
                          <Skull size={14} style="color: var(--color-error);" />
                          Force Kill (SIGKILL)
                        </button>
                      {:else}
                        <button
                          class="menu-item"
                          disabled
                          title="This process belongs to another user; administrator rights are required."
                        >
                          <Skull size={14} style="color: var(--color-text-muted);" />
                          Signals need administrator
                        </button>
                      {/if}
                    {/if}
                  </KebabMenu>
                </td>
              </tr>
            {/each}
            {#if procBottomPadding > 0}
              <tr style="height: {procBottomPadding}px; border: none !important; pointer-events: none;"><td colspan="7" style="padding: 0; border: none !important; height: {procBottomPadding}px;"></td></tr>
            {/if}
            {#if treeVisibleProcesses.length === 0}
              <tr>
                <td colspan="7" style="padding:32px 16px; text-align:center; color:var(--color-text-muted); font-size:13px;">
                  No processes match the search criteria.
                </td>
              </tr>
            {/if}
          </tbody>
        </Table>
      </div>
      {/if}
  </div>
</div>

<svelte:window onclick={closeContextMenu} oncontextmenu={closeContextMenu} />

{#if contextMenu.show && contextMenu.conn}
  <div 
    class="custom-context-menu" 
    style="position: fixed; left: {contextMenu.x}px; top: {contextMenu.y}px; z-index: 10000;"
    role="presentation"
    onclick={(e) => e.stopPropagation()}
  >
    <button 
      onclick={() => {
        if (contextMenu.conn && contextMenu.conn.pid) {
          killProcess(contextMenu.conn.pid, contextMenu.conn.process_name);
        }
        closeContextMenu();
      }}
      disabled={!canSignalPid(contextMenu.conn.pid)}
      title={canSignalPid(contextMenu.conn.pid)
        ? 'Terminate the owning process'
        : 'This process belongs to another user; administrator rights are required.'}
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

{#if procContextMenu.show && procContextMenu.proc}
  <div 
    class="custom-context-menu" 
    style="position: fixed; left: {procContextMenu.x}px; top: {procContextMenu.y}px; z-index: 10000; min-width: 230px;"
    role="presentation"
    onclick={(e) => e.stopPropagation()}
  >
    <div style="display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; gap: 8px;">
      <span style="font-size: 12px; font-weight: 700; color: var(--color-text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 140px;" title={procContextMenu.proc.name}>
        {procContextMenu.proc.name}
      </span>
      <span style="font-size: 9.5px; font-weight: 600; font-family: var(--font-mono); padding: 1px 5px; border-radius: 3px; background: rgba(0, 218, 243, 0.12); color: var(--color-accent);">
        PID {procContextMenu.proc.pid}
      </span>
    </div>
    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button 
      type="button"
      onclick={() => {
        const p = procContextMenu.proc!;
        closeContextMenu();
        openProcessInspector(p);
      }}
    >
      <TerminalSquare size={13} style="margin-right: 8px; color: var(--color-accent);" />
      Inspect Details
    </button>

    <button 
      type="button"
      onclick={() => {
        copyPid(procContextMenu.proc!.pid);
        closeContextMenu();
      }}
    >
      <Copy size={13} style="margin-right: 8px;" />
      Copy PID ({procContextMenu.proc.pid})
    </button>

    <button 
      type="button"
      onclick={() => {
        navigator.clipboard.writeText(procContextMenu.proc!.name);
        uiStore.addToast(`Copied process name: ${procContextMenu.proc!.name}`, 'info');
        closeContextMenu();
      }}
    >
      <Copy size={13} style="margin-right: 8px;" />
      Copy Name
    </button>

    <button 
      type="button"
      onclick={() => {
        const p = procContextMenu.proc!;
        closeContextMenu();
        uiStore.jumpToJournalService(p.name);
        uiStore.setActiveTab('journal-logs');
      }}
    >
      <Activity size={13} style="margin-right: 8px; color: var(--color-info);" />
      View Unit Logs in Journalctl
    </button>

    <div style="height: 1px; background: var(--color-border); margin: 4px 0;"></div>

    <button 
      type="button"
      onclick={() => {
        const p = procContextMenu.proc!;
        closeContextMenu();
        killProcess(p.pid, p.name, 15);
      }}
      style="color: var(--color-warning);"
    >
      <Skull size={13} style="margin-right: 8px; color: var(--color-warning);" />
      Send SIGTERM (Terminate)
    </button>

    <button 
      type="button"
      onclick={() => {
        const p = procContextMenu.proc!;
        closeContextMenu();
        killProcess(p.pid, p.name, 9);
      }}
      style="color: var(--color-error);"
    >
      <Skull size={13} style="margin-right: 8px; color: var(--color-error);" />
      Send SIGKILL (Force Kill)
    </button>
  </div>
{/if}

<SideDrawer bind:isOpen={isDrawerOpen} title="Connection Details" width="450px">
  {#if selectedConnection}
    <div style="display:flex; flex-direction:column; gap: 20px;">
      <!-- Port/Connection Section -->
      <div style="background: var(--color-bg-hover); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px;">
        <h4 style="margin: 0 0 12px; font-size: 13px; font-weight: 600; color: var(--color-info); display: flex; align-items: center; gap: 6px;">
          <Wifi size={14} /> Network Port Details
        </h4>
        <div style="display:flex; flex-direction:column; gap: 8px; font-size: 12px;">
          <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Protocol</span> <span style="font-weight:600; font-family:var(--font-mono);">{selectedConnection.protocol}</span></div>
          <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">State</span> <span style="color:var(--color-success); font-weight:600;">{selectedConnection.state}</span></div>
          <div style="display:flex; justify-content:space-between; flex-direction:column; gap:2px;">
            <span style="color:var(--color-text-muted);">Local Address & Port</span>
            <span style="font-family:var(--font-mono); background:var(--color-bg-surface); padding: 4px 8px; border-radius:4px; word-break:break-all;">{selectedConnection.local_address}</span>
          </div>
          <div style="display:flex; justify-content:space-between; flex-direction:column; gap:2px;">
            <span style="color:var(--color-text-muted);">Remote Address & Port</span>
            <span style="font-family:var(--font-mono); background:var(--color-bg-surface); padding: 4px 8px; border-radius:4px; word-break:break-all;">{selectedConnection.remote_address}</span>
          </div>
        </div>
      </div>

      <!-- Process Section -->
      <div style="background: var(--color-bg-hover); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px;">
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
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">User</span> <span style="background:var(--color-bg-hover); padding:1px 6px; border-radius:4px;">{detailedProcess.user}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">CPU Usage</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.cpu_percent.toFixed(1)}%</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Memory Usage</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.mem_percent.toFixed(1)}% ({detailedProcess.mem_rss_mb.toFixed(1)} MB)</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">State</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.state}</span></div>
                <div style="display:flex; justify-content:space-between;"><span style="color:var(--color-text-muted);">Threads</span> <span style="font-weight:600; font-family:var(--font-mono);">{detailedProcess.threads}</span></div>
                <div style="display:flex; flex-direction:column; gap:4px; margin-top:4px;">
                  <span style="color:var(--color-text-muted);">Full Command Line</span>
                  <div style="font-family:var(--font-mono); background:var(--color-bg-surface); padding: 8px; border-radius:4px; font-size:10px; max-height:100px; overflow-y:auto; word-break:break-all; border: 1px solid var(--color-border-subtle);">
                    {detailedProcess.cmdline || detailedProcess.name}
                  </div>
                </div>
                
                <div style="margin-top: 12px; display:flex; justify-content:flex-end; align-items:center; gap:8px;">
                  {#if detailedProcess.owned === false}
                    <span style="font-size:11px; color:var(--color-warning);">
                      Another user owns this process; killing it needs administrator rights.
                    </span>
                  {:else}
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
                  {/if}
                </div>
              </div>
            {:else}
              <div style="color:var(--color-text-muted); font-size:12px; display:flex; flex-direction:column; gap:10px; padding:8px 0;">
                <p style="margin:0; line-height:1.4;">Detailed process metrics are unavailable (the process may have terminated, or requires higher privileges to inspect).</p>
                <div style="background: var(--color-bg-surface); border: 1px solid var(--color-border); border-radius: 6px; padding: 12px; display:flex; flex-direction:column; gap:8px;">
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

<!-- Process Inspector Side Drawer -->
<SideDrawer bind:isOpen={isProcessDrawerOpen} title="Process Inspector — {inspectedProcess?.name || 'Process'}" width="580px">
  {#if inspectedProcess}
    <div style="display:flex; flex-direction:column; gap:16px; padding:4px;">

      <!-- Header Overview Card -->
      <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:10px; padding:14px; display:flex; align-items:center; justify-content:space-between;">
        <div>
          <div style="font-size:15px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
            <Cpu size={18} style="color:var(--color-accent);" />
            {inspectedProcess.name}
          </div>
          <div style="font-size:12px; color:var(--color-text-muted); font-family:var(--font-mono); margin-top:2px;">
            PID: <strong style="color:var(--color-text-primary);">{inspectedProcess.pid}</strong> &nbsp;|&nbsp; PPID: <strong style="color:var(--color-text-primary);">{inspectedProcess.ppid || '1'}</strong> &nbsp;|&nbsp; User: <strong style="color:var(--color-success);">{inspectedProcess.user}</strong>
          </div>
        </div>
        <span style="font-size:11px; font-weight:700; padding:2px 8px; border-radius:6px; font-family:var(--font-mono); background:rgba(34,197,94,0.15); color:var(--color-success);">
          {inspectedProcess.state || 'Running'}
        </span>
      </div>

      <!-- Process Hierarchy & Parent/Child Tree -->
      <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:10px; padding:14px; display:flex; flex-direction:column; gap:12px;">
        <div style="display:flex; justify-content:space-between; align-items:center;">
          <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted);">
            Process Hierarchy
          </div>
          {#if inspectedChildProcesses.length > 0}
            <button
              type="button"
              class="action-btn danger"
              style="font-size:11px; display:flex; align-items:center; gap:4px; padding:4px 9px; border-radius:6px; background:rgba(239,68,68,0.1); border:1px solid rgba(239,68,68,0.3); color:var(--color-error); cursor:{inspectedProcess.owned === false ? 'not-allowed' : 'pointer'}; font-weight:600; opacity:{inspectedProcess.owned === false ? 0.45 : 1};"
              onclick={() => killProcessTree(inspectedProcess.pid, inspectedProcess.name)}
              disabled={inspectedProcess.owned === false}
              title={inspectedProcess.owned === false
                ? 'This process belongs to another user; administrator rights are required.'
                : `Terminate parent process and all ${inspectedChildProcesses.length} sub-processes`}
            >
              <Skull size={12} /> Kill Tree ({inspectedChildProcesses.length + 1})
            </button>
          {/if}
        </div>

        <!-- Parent Process Row -->
        <div style="display:flex; align-items:center; justify-content:space-between; background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:6px; padding:8px 12px;">
          <div style="display:flex; align-items:center; gap:8px;">
            <CornerDownRight size={14} style="color:var(--color-text-muted);" />
            <div>
              <div style="font-size:10px; color:var(--color-text-muted); text-transform:uppercase;">Parent Process (PPID)</div>
              <div style="font-size:12px; font-weight:600; color:var(--color-text-primary); font-family:var(--font-mono);">
                {#if inspectedParentProcess}
                  {inspectedParentProcess.name} (PID: {inspectedParentProcess.pid})
                {:else if inspectedProcess.ppid === 0 || inspectedProcess.pid === 1}
                  None (System Root)
                {:else}
                  PID: {inspectedProcess.ppid}
                {/if}
              </div>
            </div>
          </div>
          {#if inspectedParentProcess}
            <button
              type="button"
              style="font-size:11px; padding:4px 8px; border-radius:4px; background:rgba(59,130,246,0.1); border:1px solid rgba(59,130,246,0.3); color:var(--color-accent); cursor:pointer;"
              onclick={() => openProcessInspector(inspectedParentProcess)}
              title="Inspect parent process"
            >
              Inspect Parent
            </button>
          {/if}
        </div>

        <!-- Child Processes List -->
        <div>
          <div style="font-size:11px; font-weight:600; color:var(--color-text-secondary); margin-bottom:6px; display:flex; align-items:center; gap:6px;">
            <Network size={13} style="color:var(--color-accent);" />
            Direct Child Processes ({inspectedChildProcesses.length})
          </div>

          {#if inspectedChildProcesses.length === 0}
            <div style="font-size:12px; color:var(--color-text-muted); font-style:italic; padding:6px 0;">
              No active sub-processes under this process.
            </div>
          {:else}
            <div class="drawer-children-list" style="max-height:160px; overflow-y:auto; display:flex; flex-direction:column; gap:4px; background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:6px; padding:6px;">
              {#each inspectedChildProcesses as child (child.pid)}
                <div style="display:flex; align-items:center; justify-content:space-between; padding:6px 8px; border-radius:4px; background:var(--color-bg-hover);">
                  <div style="display:flex; align-items:center; gap:6px; min-width:0;">
                    <span style="font-family:var(--font-mono); font-size:11px; color:var(--color-text-muted); min-width:36px;">{child.pid}</span>
                    <span style="font-size:12px; font-weight:500; color:var(--color-text-primary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">{child.name}</span>
                    <span style="font-size:10px; color:var(--color-text-muted);">({child.cpu_percent.toFixed(1)}% CPU, {child.mem_rss_mb.toFixed(0)}MB)</span>
                  </div>
                  <div style="display:flex; align-items:center; gap:4px; flex-shrink:0;">
                    <button
                      type="button"
                      style="font-size:10px; padding:2px 6px; border-radius:4px; background:transparent; border:1px solid var(--color-border); color:var(--color-text-secondary); cursor:pointer;"
                      onclick={() => openProcessInspector(child)}
                      title="Inspect child details"
                    >
                      Inspect
                    </button>
                    {#if child.pid > 100}
                      <button
                        type="button"
                        style="font-size:10px; padding:2px 6px; border-radius:4px; background:rgba(239,68,68,0.1); border:1px solid rgba(239,68,68,0.2); color:var(--color-error); cursor:pointer;"
                        onclick={() => sendProcessSignal(child.pid, 15, child.name)}
                        title="Soft Kill child (SIGTERM)"
                      >
                        Kill
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- Process Signal Controls -->
      <div style="background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:10px; padding:14px;">
        <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted); margin-bottom:10px;">
          Process Signals
        </div>
        {#if inspectedProcess.owned === false}
          <p style="margin:0 0 10px; font-size:11px; color:var(--color-warning);">
            This process belongs to another user. Signals and priority changes need administrator rights.
          </p>
        {/if}
        <fieldset
          disabled={inspectedProcess.owned === false}
          style="display:grid; grid-template-columns:1fr 1fr; gap:8px; border:none; margin:0; padding:0; min-width:0;"
        >
          <button
            type="button"
            class="sig-btn"
            style="background:rgba(59,130,246,0.12); border:1px solid rgba(59,130,246,0.3); color:var(--color-accent); padding:8px 10px; border-radius:6px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px; font-size:11px; font-weight:600;"
            onclick={() => sendProcessSignal(inspectedProcess.pid, 1, inspectedProcess.name)}
            title="Reload process configuration (SIGHUP)"
          >
            <RefreshCw size={14} /> Reload (SIGHUP)
          </button>
          <button
            type="button"
            class="sig-btn"
            style="background:rgba(245,158,11,0.12); border:1px solid rgba(245,158,11,0.3); color:var(--color-warning); padding:8px 10px; border-radius:6px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px; font-size:11px; font-weight:600;"
            onclick={() => sendProcessSignal(inspectedProcess.pid, 19, inspectedProcess.name)}
            title="Pause process execution (SIGSTOP)"
          >
            <Pause size={14} /> Pause (SIGSTOP)
          </button>
          <button
            type="button"
            class="sig-btn"
            style="background:rgba(34,197,94,0.12); border:1px solid rgba(34,197,94,0.3); color:var(--color-success); padding:8px 10px; border-radius:6px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px; font-size:11px; font-weight:600;"
            onclick={() => sendProcessSignal(inspectedProcess.pid, 18, inspectedProcess.name)}
            title="Resume paused process (SIGCONT)"
          >
            <Play size={14} /> Resume (SIGCONT)
          </button>
          <button
            type="button"
            class="sig-btn"
            style="background:rgba(239,68,68,0.12); border:1px solid rgba(239,68,68,0.3); color:var(--color-error); padding:8px 10px; border-radius:6px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px; font-size:11px; font-weight:600;"
            onclick={() => sendProcessSignal(inspectedProcess.pid, 15, inspectedProcess.name)}
            title="Soft terminate process (SIGTERM)"
          >
            <Skull size={14} /> Soft Kill (SIGTERM)
          </button>
          <button
            type="button"
            class="sig-btn"
            style="background:rgba(220,38,38,0.25); border:1px solid var(--color-error); color:#FFFFFF; padding:8px 10px; border-radius:6px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px; font-size:11px; font-weight:700; grid-column: span 2;"
            onclick={() => sendProcessSignal(inspectedProcess.pid, 9, inspectedProcess.name)}
            title="Force terminate process immediately (SIGKILL)"
          >
            <Skull size={14} /> Force Kill (SIGKILL)
          </button>
        </fieldset>
      </div>

      <!-- Priority / Niceness Adjustment -->
      <div style="background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:10px; padding:14px;">
        <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted); margin-bottom:8px; display:flex; justify-content:space-between; align-items:center;">
          <span>Process Scheduling Priority (Niceness)</span>
          <span class="badge" style="font-family:var(--font-mono); font-size:11px; color:var(--color-text-primary);">{inspectedProcessPriority} ({processPriorityLabel(Number(inspectedProcessPriority))})</span>
        </div>
        <div style="display:flex; align-items:center; gap:12px;">
          <span style="font-size:11px; color:var(--color-text-muted);">-20 (Max)</span>
          <input 
            type="range" 
            min={PROCESS_PRIORITY_MIN}
            max={PROCESS_PRIORITY_MAX}
            bind:value={inspectedProcessPriority}
            disabled={inspectedProcess.owned === false}
            style="flex:1; accent-color:var(--color-accent); cursor:{inspectedProcess.owned === false ? 'not-allowed' : 'pointer'};"
          />
          <span style="font-size:11px; color:var(--color-text-muted);">+19 (Idle)</span>
          <Button
            variant="primary"
            style="padding: 4px 10px; font-size:11px; flex-shrink:0;"
            onclick={applyRenice}
            disabled={renicing || inspectedProcess.owned === false}
          >
            {renicing ? 'Applying…' : 'Set Priority'}
          </Button>
        </div>
      </div>

      <!-- Full Command Line -->
      <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:10px; padding:14px;">
        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
          <span style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted);">
            Full Execution Command
          </span>
          <button
            type="button"
            class="action-btn"
            onclick={() => copyProcessCmdline(inspectedProcess.cmdline)}
            title="Copy command"
            style="font-size:11px; display:flex; align-items:center; gap:4px; color:var(--color-accent); background:transparent; border:none; cursor:pointer;"
          >
            {#if copiedCmd}
              <Check size={13} /> Copied!
            {:else}
              <Copy size={13} /> Copy
            {/if}
          </button>
        </div>
        <div style="font-family:var(--font-mono); font-size:11px; color:var(--color-text-primary); background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:6px; padding:10px; word-break:break-all; max-height:120px; overflow-y:auto; line-height:1.4;">
          {inspectedProcess.cmdline || inspectedProcess.name}
        </div>
      </div>

      <!-- Metrics Grid -->
      <div style="display:grid; grid-template-columns:1fr 1fr; gap:10px;">
        <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
          <div style="font-size:10px; color:var(--color-text-muted); text-transform:uppercase;">CPU Load</div>
          <div style="font-size:18px; font-weight:700; color:var(--color-accent); margin-top:2px;">{inspectedProcess.cpu_percent}%</div>
        </div>
        <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
          <div style="font-size:10px; color:var(--color-text-muted); text-transform:uppercase;">Memory (RSS)</div>
          <div style="font-size:18px; font-weight:700; color:var(--color-warning); margin-top:2px;">{inspectedProcess.mem_rss_mb} MB ({inspectedProcess.mem_percent}%)</div>
        </div>
        <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
          <div style="font-size:10px; color:var(--color-text-muted); text-transform:uppercase;">Active Threads</div>
          <div style="font-size:16px; font-weight:600; color:var(--color-text-primary); margin-top:2px;">{inspectedProcess.threads || 1}</div>
        </div>
        <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
          <div style="font-size:10px; color:var(--color-text-muted); text-transform:uppercase;">Owner User</div>
          <div style="font-size:16px; font-weight:600; color:var(--color-success); margin-top:2px;">{inspectedProcess.user}</div>
        </div>
      </div>

    </div>
  {/if}
</SideDrawer>

<style>/* Base Layout */ .module-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
  }.page-content {
    flex: 1;
    overflow: hidden;
    padding: 8px 12px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }/* Loading */ .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 200px;
    color: var(--color-text-muted);
    font-size: 14px;
  }/* Interface speeds sit in the card header as compact chips. */
  .iface-strip {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .iface-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    max-width: 100%;
  }
  .iface-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-success);
    flex-shrink: 0;
  }
  .iface-name {
    font-weight: 600;
    font-size: 11px;
    color: var(--color-text-primary);
  }
  .iface-ip {
    color: var(--color-text-muted);
    font-size: 9.5px;
  }
  .iface-rates {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-accent);
    font-size: 9.5px;
  }
  .iface-rates span {
    white-space: nowrap;
  }
  .iface-empty {
    font-size: 11px;
    color: var(--color-text-muted);
    font-style: italic;
  }
  .network-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    row-gap: 6px;
    flex-shrink: 0;
  }
  .conn-search {
    flex: 1 1 220px;
    min-width: 160px;
    max-width: 340px;
  }
  .conn-search input {
    width: 100%;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    padding: 5px 10px;
    border-radius: 6px;
    font-size: 11px;
    color: var(--color-text-primary);
    outline: none;
    transition: border-color 0.2s;
  }
  .conn-search input:focus {
    border-color: var(--color-accent);
  }
  .network-count {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .page-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 8px 6px 8px;
    overflow: hidden;
  }
  .live-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 10px;
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-success);
    box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.12);
  }
  .live-dot.paused {
    background: var(--color-warning);
    box-shadow: 0 0 0 3px rgba(245, 158, 11, 0.12);
  }
  .live-time {
    font-family: var(--font-mono);
  }
  .resources-columns {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    gap: 10px;
    align-items: start;
    overflow-y: auto;
    padding-right: 2px;
  }
  .resources-main,
  .resources-side {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .resources-columns :global(.monitor-panel) {
    height: auto;
    min-height: 0;
  }
  .network-grid {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .network-grid :global(.monitor-panel) {
    flex: 1;
    min-height: 0;
  }
  @media (max-width: 1100px) {
    .resources-columns {
      grid-template-columns: minmax(0, 1fr);
    }
    .live-status {
      display: none;
    }
  }
  .cpu-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .metric-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .cpu-summary-values {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
  }
  .cpu-summary-values strong {
    font-size: 16px;
    color: var(--color-text-primary);
  }
  .temp-badge {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
    font-size: 11px;
    font-weight: 500;
  }
  .temp-badge.warm {
    background: rgba(245, 158, 11, 0.15);
    color: var(--color-warning);
  }
  .temp-badge.hot {
    background: rgba(239, 68, 68, 0.15);
    color: var(--color-error);
  }
  .cpu-chart {
    height: 70px;
    padding: 4px;
    background: var(--color-bg-surface);
    border-radius: 8px;
    overflow: hidden;
  }
  .cpu-chart svg {
    width: 100%;
    height: 100%;
  }
  .load-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .core-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(58px, 1fr));
    gap: 6px;
  }
  .core-tile {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 5px 7px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
  }
  .core-head {
    display: flex;
    justify-content: space-between;
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--color-text-muted);
  }
  .core-head strong {
    color: var(--color-text-secondary);
    font-weight: 600;
  }
  .core-bar {
    height: 4px;
    background: var(--color-bg-hover);
    border-radius: 999px;
    overflow: hidden;
  }
  .core-bar span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-accent);
    transition: width 0.3s ease;
  }
  .core-tile.warm .core-bar span {
    background: var(--color-warning);
  }
  .core-tile.hot .core-bar span {
    background: var(--color-error);
  }
  .core-tile.warm .core-head strong {
    color: var(--color-warning);
  }
  .core-tile.hot .core-head strong {
    color: var(--color-error);
  }
  .metric-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .metric-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
  }
  .metric-row strong {
    font-family: var(--font-mono);
  }
  .metric-foot {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
    color: var(--color-text-muted);
  }
  .mini-chart {
    height: 30px;
    padding: 2px;
    background: var(--color-bg-surface);
    border-radius: 6px;
    overflow: hidden;
  }
  .mini-chart svg {
    width: 100%;
    height: 100%;
  }
  .ram-stroke {
    stroke: var(--color-accent);
    fill: none;
  }
  .swap-stroke {
    stroke: var(--color-warning, #f59e0b);
    fill: none;
  }
  .consumer-switch {
    display: inline-flex;
    align-self: flex-start;
    gap: 2px;
    padding: 2px;
    margin-bottom: 8px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 6px;
  }
  .consumer-switch button {
    padding: 3px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
  }
  .consumer-switch button.active {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }
  .consumer-list {
    display: flex;
    flex-direction: column;
  }
  .consumer-row {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) auto auto 14px;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 4px;
    border: none;
    border-bottom: 1px solid var(--color-border-subtle);
    border-radius: 4px;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .consumer-row:last-child {
    border-bottom: none;
  }
  .consumer-row:hover {
    background: var(--color-bg-hover);
  }
  .consumer-rank {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--color-text-muted);
    text-align: right;
  }
  .consumer-name {
    font-size: 12px;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .consumer-pid {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .consumer-value {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-info);
    white-space: nowrap;
  }
  .consumer-value.mem {
    color: var(--color-accent);
  }
  .consumer-arrow {
    color: var(--color-text-muted);
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .consumer-row:hover .consumer-arrow {
    opacity: 1;
  }
  .link-btn {
    padding: 0;
    border: none;
    background: transparent;
    color: var(--color-accent);
    font-size: 10.5px;
    cursor: pointer;
  }
  .link-btn:hover {
    text-decoration: underline;
  }
  .disk-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-radius: 8px;
  }
  .disk-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .disk-identity {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .disk-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    background: var(--color-module-icon-bg, var(--color-bg-raised));
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-accent);
  }
  .disk-name {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
  }
  .disk-speeds {
    display: flex;
    gap: 16px;
    flex-shrink: 0;
  }
  .disk-speed {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .disk-speed > span {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }
  .disk-speed strong {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .disk-speed.write strong {
    color: var(--color-accent);
  }
  .disk-chart {
    height: 36px;
    overflow: hidden;
  }
  .disk-chart svg {
    width: 100%;
    height: 100%;
  }
  .disk-chart polyline {
    fill: none;
    stroke: var(--color-accent);
  }
  .smart-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-radius: 8px;
  }
  .smart-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .smart-device {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
  }
  .smart-error {
    padding: 10px 12px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-left: 3px solid var(--color-warning);
    border-radius: 8px;
    font-size: 11px;
    color: var(--color-warning);
  }
  :global(.active-conn-table table thead th) {
    position: sticky;
    top: 0;
    z-index: 10;
    background: var(--color-bg-card);
  }
  /* Leave the floating density pills room over the empty Actions header. */
  :global(.active-conn-table th:last-child),
  :global(.active-conn-table td:last-child) {
    width: 84px;
  }:global(.monitor-panel) {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }.panel-scroll {
    flex: 1;
    /* No inner scrollbar: the column above owns scrolling, and an overflow box
       has a min-content size of 0, which would collapse the grid row. */
    overflow: visible;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }.spark-stroke { stroke-width: 2px; stroke-linejoin: round; stroke-linecap: round; }.cpu-stroke { stroke: var(--color-info); }.cpu-fill { fill: var(--color-info); opacity: 0.15; }.progress-bg {
    height: 6px;
    background: var(--color-bg-surface);
    border-radius: 4px;
    overflow: hidden;
  }.progress-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.3s ease;
  }.ram-fill { background: linear-gradient(90deg, var(--color-accent), var(--color-accent-soft)); }.swap-fill { background: linear-gradient(90deg, var(--color-text-secondary), var(--color-border)); }/* PROCESSES TAB */ .table-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    min-height: 0;
  }.table-toolbar {
    padding: 6px 10px;
    border-bottom: 1px solid var(--color-border);
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    background: var(--color-surface-elevated, rgba(0,0,0,0.1));
  }.process-search {
    flex: 1 1 220px;
    min-width: 180px;
    max-width: 340px;
    position: relative;
  }.search-box input {
    background: var(--color-surface, rgba(255, 255, 255, 0.05));
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    padding: 5px 10px;
    border-radius: 6px;
    font-size: 12px;
    width: 100%;
    outline: none;
    transition: all 0.2s;
  }.search-box input:focus { border-color: var(--color-accent); }.process-tree-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }.toolbar-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }.toolbar-icon-btn:hover {
    background: rgba(59, 130, 246, 0.12);
    color: var(--color-accent);
    border-color: rgba(59, 130, 246, 0.4);
  }:global(html.light-mode) .toolbar-icon-btn {
    background: #FFFFFF;
    border-color: #CBD5E1;
    color: #475569;
  }:global(html.light-mode) .toolbar-icon-btn:hover {
    background: #EFF6FF;
    border-color: #93C5FD;
    color: #2563EB;
  }.toolbar-stats {
    margin-left: auto;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }/* ── Process Tree Hierarchy Styles ─────────────────────────────────── */ .tree-row {
    transition: background 0.15s ease, border-color 0.15s ease;
    position: relative;
  }/* Expanded Parent Process Row */ .tree-row.is-expanded-parent {
    background: rgba(0, 218, 243, 0.05) !important;
    border-left: 3px solid var(--color-accent) !important;
  }:global(html.light-mode) .tree-row.is-expanded-parent {
    background: rgba(37, 99, 235, 0.06) !important;
    border-left: 3px solid #2563EB !important;
  }.tree-row.is-expanded-parent .parent-proc-name.expanded-name {
    color: var(--color-accent);
    font-weight: 700;
  }:global(html.light-mode) .tree-row.is-expanded-parent .parent-proc-name.expanded-name {
    color: #2563EB;
  }.tree-child-badge.expanded-badge {
    background: rgba(0, 218, 243, 0.15);
    border-color: rgba(0, 218, 243, 0.4);
    color: var(--color-accent);
    font-weight: 700;
  }:global(html.light-mode) .tree-child-badge.expanded-badge {
    background: rgba(37, 99, 235, 0.12);
    border-color: rgba(37, 99, 235, 0.35);
    color: #1D4ED8;
  }/* Child Process Rows with Rich Gradient and Left Border Accent */ .tree-row.is-child {
    position: relative;
    border-left: 2px dashed rgba(255, 255, 255, 0.12);
  }:global(html.light-mode) .tree-row.is-child {
    border-left: 2px dashed rgba(0, 0, 0, 0.15);
  }/* Depth 1 Gradient (Direct Sub-processes) */ .tree-row.is-child.depth-1 {
    background: linear-gradient(90deg, rgba(0, 218, 243, 0.06) 0%, rgba(0, 218, 243, 0.015) 30%, transparent 80%) !important;
    border-left-color: rgba(0, 218, 243, 0.35);
  }:global(html.light-mode) .tree-row.is-child.depth-1 {
    background: linear-gradient(90deg, rgba(37, 99, 235, 0.06) 0%, rgba(37, 99, 235, 0.015) 35%, transparent 85%) !important;
    border-left-color: rgba(37, 99, 235, 0.4);
  }/* Depth 2 Gradient (Sub-sub-processes) */ .tree-row.is-child.depth-2 {
    background: linear-gradient(90deg, rgba(168, 85, 247, 0.06) 0%, rgba(168, 85, 247, 0.015) 35%, transparent 80%) !important;
    border-left-color: rgba(168, 85, 247, 0.35);
  }:global(html.light-mode) .tree-row.is-child.depth-2 {
    background: linear-gradient(90deg, rgba(147, 51, 234, 0.06) 0%, rgba(147, 51, 234, 0.015) 40%, transparent 85%) !important;
    border-left-color: rgba(147, 51, 234, 0.4);
  }/* Depth 3+ Gradient */ .tree-row.is-child.depth-3 {
    background: linear-gradient(90deg, rgba(236, 72, 153, 0.06) 0%, rgba(236, 72, 153, 0.015) 40%, transparent 80%) !important;
    border-left-color: rgba(236, 72, 153, 0.35);
  }:global(html.light-mode) .tree-row.is-child.depth-3 {
    background: linear-gradient(90deg, rgba(219, 39, 119, 0.06) 0%, rgba(219, 39, 119, 0.015) 45%, transparent 85%) !important;
    border-left-color: rgba(219, 39, 119, 0.4);
  }/* Hover states for tree rows */ .tree-row:hover {
    background: rgba(255, 255, 255, 0.04) !important;
  }:global(html.light-mode) .tree-row:hover {
    background: #F1F5F9 !important;
  }.tree-row.is-child:hover {
    background: linear-gradient(90deg, rgba(0, 218, 243, 0.1) 0%, rgba(255, 255, 255, 0.04) 40%, transparent 95%) !important;
  }:global(html.light-mode) .tree-row.is-child:hover {
    background: linear-gradient(90deg, rgba(37, 99, 235, 0.1) 0%, rgba(241, 245, 249, 0.8) 45%, transparent 95%) !important;
  }/* Tree Branch Elbow Connector & Guide Tracks */ .tree-cell-wrapper {
    display: flex;
    flex-direction: column;
    transition: margin-left 0.15s ease;
  }.tree-indent-tracks {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 2px;
    flex-shrink: 0;
  }.tree-guide-vertical-bar {
    width: 2px;
    height: 16px;
    background: rgba(255, 255, 255, 0.18);
    border-radius: 1px;
    display: inline-block;
  }:global(html.light-mode) .tree-guide-vertical-bar {
    background: rgba(0, 0, 0, 0.2);
  }.tree-branch-guide {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-accent);
    flex-shrink: 0;
  }.tree-branch-guide.active-elbow {
    background: rgba(0, 218, 243, 0.14);
    border-radius: 4px;
    padding: 2px 4px;
    border: 1px solid rgba(0, 218, 243, 0.3);
  }.depth-2 .tree-branch-guide.active-elbow {
    background: rgba(168, 85, 247, 0.14);
    border-color: rgba(168, 85, 247, 0.3);
    color: #C084FC;
  }.depth-3 .tree-branch-guide.active-elbow {
    background: rgba(236, 72, 153, 0.14);
    border-color: rgba(236, 72, 153, 0.3);
    color: #F472B6;
  }:global(html.light-mode) .tree-branch-guide.active-elbow {
    background: rgba(37, 99, 235, 0.1);
    border-color: rgba(37, 99, 235, 0.25);
    color: #2563EB;
  }:global(html.light-mode) .depth-2 .tree-branch-guide.active-elbow {
    background: rgba(147, 51, 234, 0.1);
    border-color: rgba(147, 51, 234, 0.25);
    color: #9333EA;
  }:global(html.light-mode) .depth-3 .tree-branch-guide.active-elbow {
    background: rgba(219, 39, 119, 0.1);
    border-color: rgba(219, 39, 119, 0.25);
    color: #DB2777;
  }:global(.branch-elbow-icon) {
    transform: translateY(-1px);
  }.child-proc-name {
    font-weight: 500;
    color: var(--color-text-primary);
  }.pid-text.parent-pid {
    font-weight: 700;
    color: var(--color-accent);
  }:global(html.light-mode) .pid-text.parent-pid {
    color: #2563EB;
  }.pid-text.child-pid {
    opacity: 0.75;
  }.tree-node-trigger {
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 2px 6px 2px 2px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: all 0.15s ease;
    text-align: left;
    outline: none;
  }.tree-node-trigger:hover {
    background: rgba(59, 130, 246, 0.12);
  }:global(html.light-mode) .tree-node-trigger:hover {
    background: rgba(37, 99, 235, 0.08);
  }.tree-node-trigger:hover .parent-proc-name {
    color: var(--color-accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }.tree-chevron-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1), color 0.15s ease;
  }.tree-node-trigger:hover .tree-chevron-icon {
    color: var(--color-accent);
  }.tree-chevron-icon.is-expanded {
    transform: rotate(90deg);
  }.tree-child-badge {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }.tree-node-trigger:hover .tree-child-badge {
    background: rgba(59, 130, 246, 0.2);
    border-color: rgba(59, 130, 246, 0.4);
    color: var(--color-accent);
  }:global(html.light-mode) .tree-child-badge {
    background: #E2E8F0;
    color: #475569;
    border-color: #CBD5E1;
  }.proc-row:hover {
    background: rgba(255, 255, 255, 0.04) !important;
  }:global(html.light-mode) .proc-row:hover {
    background: #F1F5F9 !important;
  }:global(html.light-mode) .table-toolbar {
    background: #F1F5F9;
    border-bottom: 1px solid #E2E8F0;
  }:global(html.light-mode) .search-box input {
    background: #FFFFFF;
    border: 1px solid #CBD5E1;
    color: #1E293B;
  }:global(html.light-mode) .search-box input::placeholder {
    color: #94A3B8;
  }:global(.process-table-wrap) {
    flex: 1;
    min-height: 0;
    overflow-y: auto !important;
  }:global(.process-table-wrap table thead) {
    position: sticky;
    top: 0;
    z-index: 20;
  }:global(.process-table-wrap table thead th) {
    position: sticky !important;
    top: 0 !important;
    z-index: 20 !important;
    background: var(--color-bg-card, #0B1726) !important;
  }:global(html.light-mode .process-table-wrap table thead th) {
    background: #EEF0F4 !important;
    color: #334155 !important;
    border-bottom: 1px solid #CBD5E1 !important;
  }.sortable-th {
    cursor: pointer;
    user-select: none;
    transition: background 0.15s ease, color 0.15s ease;
  }.sortable-th:hover {
    color: var(--color-accent) !important;
  }:global(html.light-mode) .sortable-th:hover {
    color: #2563EB !important;
  }.th-content {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }.th-sort-icon {
    font-size: 11px;
    opacity: 0.6;
    color: var(--color-accent);
  }:global(html.light-mode) .th-sort-icon {
    color: #2563EB;
  }.is-root td { opacity: 0.8; }.is-kernel td { opacity: 0.5; }.col-pid {
    width: 95px;
    font-family: var(--font-mono);
    color: var(--color-text-muted);
    user-select: text;
  }.pid-badge-container {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }.pid-copy-btn {
    background: transparent;
    border: none;
    padding: 2px 4px;
    border-radius: 4px;
    cursor: pointer;
    color: var(--color-text-muted);
    opacity: 0;
    transition: opacity 0.15s ease, background 0.15s ease, color 0.15s ease;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }.proc-row:hover .pid-copy-btn, .pid-copy-btn:focus-visible {
    opacity: 0.7;
  }.pid-copy-btn:hover {
    opacity: 1 !important;
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
  }:global(html.light-mode) .pid-copy-btn:hover {
    background: #EFF6FF;
    color: #2563EB;
  }:global(.pid-copied-icon) {
    color: var(--color-success) !important;
    opacity: 1 !important;
  }.col-name { min-width: 280px; max-width: 480px; }.col-user { width: 120px; }.col-cpu, .col-mem, .col-rss { width: 90px; font-family: var(--font-mono); }.col-actions { width: 60px; text-align: right; }.proc-name { font-weight: 600; color: var(--color-text-primary); margin-bottom: 2px; }.proc-cmd { 
    font-size: 10px; color: var(--color-text-muted); font-family: var(--font-mono);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }.user-badge {
    background: var(--color-bg-hover);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
  }.text-warn { color: var(--color-warning) !important; font-weight: 600; }/* Context Menu Styles */ .custom-context-menu {
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
  }.custom-context-menu button {
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
  }.custom-context-menu button:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }.custom-context-menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }</style>
