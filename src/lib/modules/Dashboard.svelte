<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { LayoutDashboard, Cpu, MemoryStick, HardDrive, Wifi, Activity, Heart, Server, Layers } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Badge from '../components/ui/Badge.svelte';
  
  let osInfo = $state<any>(null);
  let systemStats = $state<any>(null);
  let diskUsage = $state<any[]>([]);
  let smartHealth = $state<any[]>([]);
  let networkTraffic = $state<any[]>([]);
  
  let trafficSpeeds = $state<Record<string, { rxSpeed: number, txSpeed: number }>>({});
  let lastTrafficData = $state<any[]>([]);
  let lastTrafficTime = $state<number>(0);

  let pollInterval: any;

  async function fetchStaticInfo() {
    try {
      osInfo = await invoke('get_os_info');
      smartHealth = await invoke('get_smart_health');
    } catch (e) {
      console.error(e);
    }
  }

  async function fetchLiveStats() {
    try {
      systemStats = await invoke('get_system_stats');
      diskUsage = await invoke('get_disk_usage');
      
      const newTraffic: any[] = await invoke('get_network_traffic');
      const now = performance.now();
      
      if (lastTrafficTime > 0 && lastTrafficData.length > 0) {
        const deltaSec = (now - lastTrafficTime) / 1000;
        let speeds: Record<string, { rxSpeed: number, txSpeed: number }> = {};
        
        for (const current of newTraffic) {
          const prev = lastTrafficData.find(t => t.interface === current.interface);
          if (prev) {
            const rxDiff = current.rx_bytes - prev.rx_bytes;
            const txDiff = current.tx_bytes - prev.tx_bytes;
            speeds[current.interface] = {
              rxSpeed: Math.max(0, rxDiff / deltaSec),
              txSpeed: Math.max(0, txDiff / deltaSec)
            };
          }
        }
        trafficSpeeds = speeds;
      }
      
      lastTrafficData = newTraffic;
      lastTrafficTime = now;
      networkTraffic = newTraffic;
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    fetchStaticInfo();
    fetchLiveStats();
    pollInterval = setInterval(fetchLiveStats, 2000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });
  
  function formatBytes(bytes: number) {
    if (bytes < 1024) return Math.round(bytes) + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + " MB";
    return (bytes / (1024 * 1024 * 1024)).toFixed(1) + " GB";
  }

  function formatSpeed(bytesPerSec: number) {
    return formatBytes(bytesPerSec) + "/s";
  }
</script>

<div class="module-page">
  <PageHeader title="Dashboard" subtitle="System Analytics Overview" icon={LayoutDashboard} />

  <div class="dashboard-grid">
    <!-- OS Overview -->
    <div class="card-glass panel-overview">
      <div class="panel-header">
        <Server size={18} class="text-primary" />
        <h3>System Overview</h3>
      </div>
      <div class="panel-body">
        {#if osInfo}
          <div class="info-row"><span>Hostname:</span> <strong>{osInfo.hostname}</strong></div>
          <div class="info-row"><span>OS Name:</span> <strong>{osInfo.name} {osInfo.os_version}</strong></div>
          <div class="info-row"><span>Kernel:</span> <strong>{osInfo.kernel_version}</strong></div>
          <div class="info-row"><span>Uptime:</span> <strong>{systemStats ? (systemStats.uptime_seconds / 3600).toFixed(1) + ' hours' : '...'}</strong></div>
        {:else}
          <span class="text-muted">Loading OS information...</span>
        {/if}
      </div>
    </div>

    <!-- Resource Utilization -->
    <div class="card-glass panel-resources">
      <div class="panel-header">
        <Activity size={18} class="text-success" />
        <h3>Resource Utilization</h3>
      </div>
      <div class="panel-body">
        {#if systemStats}
          <!-- CPU -->
          <div class="metric-block">
            <div class="metric-label">
              <span><Cpu size={14} class="inline-icon text-primary"/> CPU Usage</span>
              <span>{systemStats.cpu_percent.toFixed(1)}%</span>
            </div>
            <div class="progress-bg">
              <div class="progress-fill cpu-fill" style="width: {systemStats.cpu_percent}%"></div>
            </div>
          </div>
          <!-- RAM -->
          <div class="metric-block">
            <div class="metric-label">
              <span><MemoryStick size={14} class="inline-icon text-warning"/> Memory ({(systemStats.ram_used_mb / 1024).toFixed(1)}GB / {(systemStats.ram_total_mb / 1024).toFixed(1)}GB)</span>
              <span>{systemStats.ram_percent.toFixed(1)}%</span>
            </div>
            <div class="progress-bg">
              <div class="progress-fill ram-fill" style="width: {systemStats.ram_percent}%"></div>
            </div>
          </div>
          <!-- Swap -->
          <div class="metric-block">
            <div class="metric-label">
              <span><Layers size={14} class="inline-icon text-error"/> Swap Space</span>
              <span>{systemStats.swap_percent.toFixed(1)}%</span>
            </div>
            <div class="progress-bg">
              <div class="progress-fill swap-fill" style="width: {systemStats.swap_percent}%"></div>
            </div>
          </div>
        {:else}
           <span class="text-muted">Loading resource metrics...</span>
        {/if}
      </div>
    </div>

    <!-- Network Live Traffic -->
    <div class="card-glass panel-network">
      <div class="panel-header">
        <Wifi size={18} class="text-info" />
        <h3>Network Live Traffic</h3>
      </div>
      <div class="panel-body">
        {#if networkTraffic.length > 0}
          <div class="network-list">
            {#each networkTraffic as iface}
              {#if iface.interface !== 'lo'}
                <div class="network-item">
                  <div class="iface-name">{iface.interface}</div>
                  <div class="iface-speeds">
                    <div class="speed-down">
                      <span class="speed-lbl">DL</span>
                      <strong class="text-success">{formatSpeed(trafficSpeeds[iface.interface]?.rxSpeed || 0)}</strong>
                    </div>
                    <div class="speed-up">
                      <span class="speed-lbl">UL</span>
                      <strong class="text-warning">{formatSpeed(trafficSpeeds[iface.interface]?.txSpeed || 0)}</strong>
                    </div>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <span class="text-muted">Loading network traffic...</span>
        {/if}
      </div>
    </div>

    <!-- Storage & SMART -->
    <div class="card-glass panel-storage">
      <div class="panel-header">
        <HardDrive size={18} class="text-purple" />
        <h3>Storage & SMART Health</h3>
      </div>
      <div class="panel-body storage-scroll">
        {#if diskUsage.length > 0}
          <div class="disks-list">
            {#each diskUsage as disk}
              {#if disk.device.startsWith('/dev/')}
                <div class="disk-item">
                  <div class="disk-header">
                    <strong>{disk.mount}</strong>
                    <span class="text-muted" style="font-size: 11px;">{disk.device} ({disk.fs_type})</span>
                  </div>
                  <div class="disk-stats">
                    <div class="progress-bg" style="flex:1;">
                      <div class="progress-fill storage-fill" style="width: {disk.percent}%"></div>
                    </div>
                    <span class="disk-pct">{disk.percent.toFixed(1)}% ({disk.used_gb.toFixed(1)}GB / {disk.total_gb.toFixed(1)}GB)</span>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <span class="text-muted">Loading storage data...</span>
        {/if}
        
        {#if smartHealth.length > 0}
          <div class="smart-health-section mt-4">
            <h4 class="text-sm font-semibold mb-2 flex items-center gap-2" style="font-size: 13px; font-weight: 600;"><Heart size={14} class="text-error" /> SMART Status</h4>
            {#each smartHealth as smart}
              <div class="smart-item" style="display: flex; justify-content: space-between; align-items: center; background: rgba(0,0,0,0.2); padding: 8px; border-radius: 6px; margin-bottom: 8px;">
                <div>
                  <div style="font-size: 12px; font-weight: 600;">{smart.disk_path}</div>
                  <div style="font-size: 10px; color: var(--color-text-muted);">{smart.model}</div>
                </div>
                <Badge variant={smart.health_status === 'PASSED' || smart.health_status === 'OK' ? 'success' : (smart.health_status === 'UNKNOWN' ? 'warning' : 'error')}>
                  {smart.health_status}
                </Badge>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .dashboard-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
    padding: 16px;
    overflow-y: auto;
    height: 100%;
  }

  .card-glass {
    background: rgba(30, 30, 40, 0.6);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-header {
    padding: 14px 16px;
    background: rgba(0, 0, 0, 0.2);
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .panel-header h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .panel-body {
    padding: 16px;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
    padding: 6px 0;
    border-bottom: 1px dashed rgba(255,255,255,0.05);
  }
  .info-row:last-child { border-bottom: none; }
  .info-row span { color: var(--color-text-muted); }

  .metric-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .metric-label {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 500;
  }
  .inline-icon { vertical-align: -2px; margin-right: 4px; }

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
  .cpu-fill { background: linear-gradient(90deg, #3b82f6, #8b5cf6); }
  .ram-fill { background: linear-gradient(90deg, #f59e0b, #ef4444); }
  .swap-fill { background: linear-gradient(90deg, #ef4444, #b91c1c); }
  .storage-fill { background: linear-gradient(90deg, #8b5cf6, #d946ef); }

  .network-list { display: flex; flex-direction: column; gap: 8px; }
  .network-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(0,0,0,0.2);
    padding: 10px 12px;
    border-radius: 8px;
  }
  .iface-name { font-weight: 600; font-size: 13px; font-family: var(--font-mono); }
  .iface-speeds { display: flex; gap: 16px; font-size: 12px; font-family: var(--font-mono); }
  .speed-lbl { color: var(--color-text-muted); font-size: 10px; margin-right: 4px; }

  .disks-list { display: flex; flex-direction: column; gap: 12px; }
  .disk-item { display: flex; flex-direction: column; gap: 6px; }
  .disk-header { display: flex; justify-content: space-between; font-size: 13px; }
  .disk-stats { display: flex; align-items: center; gap: 8px; font-size: 11px; }
  .disk-pct { min-width: 120px; text-align: right; }
  
  .storage-scroll {
      overflow-y: auto;
      max-height: 400px;
  }

  .text-primary { color: #3b82f6; }
  .text-success { color: #10b981; }
  .text-warning { color: #f59e0b; }
  .text-error { color: #ef4444; }
  .text-info { color: #06b6d4; }
  .text-purple { color: #a855f7; }
</style>
