<script lang="ts">
  import { onMount } from 'svelte';
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { LayoutDashboard, HardDrive, Wifi, Server, Heart, Activity, RefreshCw, Shield } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import Card from '../components/ui/Card.svelte';
  
  let osInfo = $state<any>(null);
  let systemStats = $state<any>(null);
  let diskUsage = $state<any[]>([]);
  let smartHealth = $state<any[]>([]);
  let networkInterfaces = $state<any[]>([]);
  
  // Network details (gateway, dns)
  let networkDetails = $state<any>(null);
  let gatewayPing = $state<string>('');
  // System Events State (Boot time, Err/Warn counts)
  let systemEvents = $state<any>(null);
  
  // New features state
  let lastSystemUpdate = $state<string>('');
  let failedServicesCount = $state<number>(0);

  let securityReport = $state<any>(null);
  let loadingSecurity = $state(false);
  let securityCriticalCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Critical' && !f.is_resolved).length : 0);
  let securityWarningCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.severity === 'Warning' && !f.is_resolved).length : 0);
  let securityPassedCount = $derived(securityReport ? securityReport.findings.filter((f: any) => f.is_resolved).length : 0);

  function getScoreColor(score: number) {
    if (score >= 80) return 'var(--color-success)';
    if (score >= 50) return 'var(--color-warning)';
    return 'var(--color-error)';
  }

  function getScoreLabel(score: number) {
    if (score >= 90) return 'EXCELLENT';
    if (score >= 80) return 'GOOD';
    if (score >= 60) return 'FAIR';
    if (score >= 40) return 'POOR';
    return 'CRITICAL RISK';
  }

  let pollInterval: any;
  let pingInterval: any;

  let isRefreshing = $state(false);

  async function handleManualRefresh() {
    isRefreshing = true;
    try {
      await Promise.all([fetchData(), fetchSecurityReport()]);
    } catch (e) {
      console.error(e);
    } finally {
      isRefreshing = false;
    }
  }

  async function fetchSecurityReport() {
    loadingSecurity = true;
    try {
      securityReport = await invoke('security_run_audit');
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

  let storageDist = $state<{ rpm_gb: number; flatpak_gb: number; system_gb: number } | null>(null);
  let storageDistTotal = $derived(storageDist ? storageDist.rpm_gb + storageDist.flatpak_gb + storageDist.system_gb : 0);
  let storageDistSubTotal = $derived(storageDist ? storageDist.rpm_gb + storageDist.flatpak_gb : 0);

  async function fetchStorageDistribution() {
    try {
      storageDist = await invoke('get_storage_distribution');
    } catch (e) {
      console.error("Error fetching storage distribution:", e);
    }
  }

  async function fetchData() {
    fetchStorageDistribution();
    try {
      const [os, stats, disks, smart, ifaces, lastUpdate, failedSvc] = await Promise.all([
        invoke('get_os_info'),
        invoke('get_system_stats'),
        invoke('get_disk_usage'),
        invoke('get_smart_health'),
        invoke('get_network_interfaces'),
        invoke('get_last_system_update'),
        invoke('get_failed_services_count')
      ]);
      osInfo = os;
      systemStats = stats;
      diskUsage = disks as any[];
      smartHealth = smart as any[];
      networkInterfaces = ifaces as any[];
      lastSystemUpdate = lastUpdate as string;
      failedServicesCount = failedSvc as number;
      
      // Secondary updates
      fetchNetworkDetails();
      fetchSystemEvents();
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    fetchData();
    fetchNetworkDetails();
    fetchSecurityReport();
    pollInterval = setInterval(fetchData, 30000);
    pingInterval = setInterval(updateGatewayPing, 10000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (pingInterval) clearInterval(pingInterval);
  });
</script>

<div class="module-page">
  <PageHeader title="Dashboard" subtitle="System Analytics Overview" icon={LayoutDashboard}>
    <Button variant="ghost" onclick={handleManualRefresh} disabled={isRefreshing}>
      <RefreshCw size={14} class={isRefreshing ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  <div class="dashboard-grid">
    <!-- Column 1: System Overview & Resources -->
    <div class="dashboard-column">
      <Card title="System Overview" icon={Server} class="panel-overview" style="display:flex; flex-direction:column; gap:12px;">
        {#if osInfo}
          <div class="info-row"><span>Hostname:</span> <strong>{osInfo.hostname}</strong></div>
          <div class="info-row"><span>OS Name:</span> <strong>{osInfo.name} {osInfo.os_version}</strong></div>
          <div class="info-row"><span>Kernel:</span> <strong>{osInfo.kernel_version}</strong></div>
          <div class="info-row"><span>Uptime:</span> <strong>{systemStats ? (systemStats.uptime_seconds / 3600).toFixed(1) + ' hours' : '...'}</strong></div>
          <div class="info-row"><span>Last Updated:</span> <strong>{lastSystemUpdate || '...'}</strong></div>
        {:else}
          <span class="text-muted">Loading OS information...</span>
        {/if}
      </Card>



      <!-- System Events -->
      <Card title="System Events" icon={Activity} class="panel-events" style="display:flex; flex-direction:column; gap:12px;">
        {#if systemEvents}
          <div class="info-row" style="display:flex; justify-content:space-between; margin-bottom:4px; font-size: 13px; border-bottom: 1px dashed rgba(255,255,255,0.05); padding-bottom: 6px;">
            <span>Last Boot Time:</span> 
            <strong>{systemEvents.last_boot_time}</strong>
          </div>
          
          <div style="display: flex; gap: 12px; margin-top: 4px;">
            <!-- Errors Button -->
            <button 
              onclick={() => {
                uiStore.preAppliedJournalPriority = '3'; // Error & Above
                uiStore.setActiveTab('journal-logs');
              }}
              style="flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px; background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); padding: 10px; border-radius: 8px; cursor: pointer; transition: all 0.2s;"
              class="hover-bg-error-light"
            >
              <span style="font-size: 10px; color: var(--color-text-secondary); font-weight:600; text-transform: uppercase;">Errors</span>
              <Badge variant={systemEvents.error_count > 0 ? 'error' : 'success'} style="font-size: 12px; font-weight: bold; padding: 2px 8px; border-radius: 20px;">
                {systemEvents.error_count}
              </Badge>
            </button>
            
            <!-- Warnings Button -->
            <button 
              onclick={() => {
                uiStore.preAppliedJournalPriority = '4'; // Warning & Above
                uiStore.setActiveTab('journal-logs');
              }}
              style="flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px; background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); padding: 10px; border-radius: 8px; cursor: pointer; transition: all 0.2s;"
              class="hover-bg-warn-light"
            >
              <span style="font-size: 10px; color: var(--color-text-secondary); font-weight:600; text-transform: uppercase;">Warnings</span>
              <Badge variant={systemEvents.warning_count > 0 ? 'warning' : 'muted'} style="font-size: 12px; font-weight: bold; padding: 2px 8px; border-radius: 20px;">
                {systemEvents.warning_count}
              </Badge>
            </button>

            <!-- Failed Services Button -->
            <button 
              onclick={() => {
                uiStore.serviceFilter = 'failed';
                uiStore.setActiveTab('service-manager');
              }}
              style="flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px; background: rgba(0,0,0,0.15); border: 1px solid var(--color-border); padding: 10px; border-radius: 8px; cursor: pointer; transition: all 0.2s;"
              class="hover-bg-error-light"
            >
              <span style="font-size: 10px; color: var(--color-text-secondary); font-weight:600; text-transform: uppercase; text-align: center; line-height: 1;">Failed Services</span>
              <Badge variant={failedServicesCount > 0 ? 'error' : 'success'} style="font-size: 12px; font-weight: bold; padding: 2px 8px; border-radius: 20px;">
                {failedServicesCount}
              </Badge>
            </button>
          </div>
        {:else}
          <span class="text-muted" style="font-size: 11px;">Loading system events...</span>
        {/if}
      </Card>
    </div>

    <!-- Column 2: Network Interfaces -->
    <div class="dashboard-column">
      <Card title="Network Interfaces" icon={Wifi} class="panel-network" style="display:flex; flex-direction:column; gap:12px;">
        {#if networkInterfaces.length > 0}
          <div class="network-list">
            {#each networkInterfaces as iface}
              {#if iface.name !== 'lo'}
                <div class="network-item" style="display:flex; justify-content:space-between; align-items:center; background: rgba(0,0,0,0.2); padding: 10px 12px; border-radius: 8px; margin-bottom: 8px;">
                  <div style="display:flex; align-items:center; gap:8px;">
                    <span class="iface-name" style="font-weight:600; font-size:13px; font-family:var(--font-mono); color: var(--color-text-primary);">{iface.name}</span>
                    <Badge variant={iface.is_up ? 'success' : 'muted'} style="font-size: 9px; padding: 2px 6px;">{iface.is_up ? 'UP' : 'DOWN'}</Badge>
                  </div>
                  <div style="font-family:var(--font-mono); font-size:12px; color:var(--color-text-secondary); display: flex; align-items: center; gap: 4px;">
                    {iface.ip4 || 'No IP'}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <span class="text-muted">Loading network interfaces...</span>
        {/if}

        <!-- Divider -->
        <div style="height: 1px; background: rgba(255, 255, 255, 0.05); margin: 12px 0;"></div>

        <!-- Gateway & DNS details -->
        {#if networkDetails}
          <div style="display: flex; flex-direction: column; gap: 8px; font-size: 12px; color: var(--color-text-secondary); padding: 0 4px;">
            {#if networkDetails.gateway}
              <div style="display: flex; justify-content: space-between; align-items: center;">
                <span>Gateway IP:</span>
                <strong style="font-family: var(--font-mono); color: var(--color-text-primary);">
                  {networkDetails.gateway}
                  {#if gatewayPing}
                    <span style="color: var(--color-text-muted); margin: 0 2px;">·</span>
                    {#if gatewayPing === 'timeout'}
                      <span style="color: var(--color-error); font-weight: 600;">timeout</span>
                    {:else}
                      <span style="color: var(--color-success); font-weight: 600;">{gatewayPing}</span>
                    {/if}
                  {:else}
                    <span style="color: var(--color-text-muted); font-size: 10px; margin-left: 4px;" class="animate-pulse">pinging...</span>
                  {/if}
                </strong>
              </div>
            {/if}
            
            {#if networkDetails.dns && networkDetails.dns.length > 0}
              <div style="display: flex; justify-content: space-between; align-items: flex-start;">
                <span>DNS Servers:</span>
                <div style="display: flex; flex-direction: column; align-items: flex-end; gap: 2px;">
                  {#each networkDetails.dns as dnsIp}
                    <strong style="font-family: var(--font-mono); color: var(--color-text-primary);">{dnsIp}</strong>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {:else}
          <span class="text-muted" style="font-size: 11px;">Loading gateway & DNS details...</span>
        {/if}
      </Card>

      <!-- Security Auditor Card -->
      <Card title="Security Auditor" icon={Shield} class="panel-security" style="display:flex; flex-direction:column; gap:12px;">
        {#if loadingSecurity && !securityReport}
          <!-- Loading skeleton -->
          <div style="display:flex; flex-direction:column; gap:12px; align-items:center; padding: 8px 0;">
            <div style="width: 64px; height: 64px; border-radius: 50%; background: rgba(255,255,255,0.03); border: 4px solid rgba(255,255,255,0.05);" class="animate-pulse"></div>
            <div style="height: 14px; background: rgba(255,255,255,0.03); border-radius: 4px; width: 100px;" class="animate-pulse"></div>
            <div style="display:flex; gap:8px; width: 100%; justify-content:center;">
              <div style="height: 24px; background: rgba(255,255,255,0.03); border-radius: 6px; width: 70px;" class="animate-pulse"></div>
              <div style="height: 24px; background: rgba(255,255,255,0.03); border-radius: 6px; width: 70px;" class="animate-pulse"></div>
              <div style="height: 24px; background: rgba(255,255,255,0.03); border-radius: 6px; width: 70px;" class="animate-pulse"></div>
            </div>
          </div>
        {:else if securityReport}
          <div style="display:flex; flex-direction:column; gap:16px; align-items:center;">
            <!-- Circular score indicator -->
            <div style="display: flex; flex-direction: column; align-items: center; gap: 8px;">
              <div class="dashboard-score-gauge" style="--score-color: {getScoreColor(securityReport.score)}; position: relative; width: 72px; height: 72px; display: flex; align-items: center; justify-content: center;">
                <svg viewBox="0 0 100 100" style="transform: rotate(-90deg); width: 100%; height: 100%;">
                  <circle cx="50" cy="50" r="40" fill="none" stroke="rgba(255,255,255,0.03)" stroke-width="8"></circle>
                  <circle cx="50" cy="50" r="40" fill="none" stroke="var(--score-color)" stroke-width="8"
                    style="stroke-dasharray: {securityReport.score * 2.513} 251.3; transition: stroke-dasharray 0.8s ease; stroke-linecap: round;"></circle>
                </svg>
                <div style="position: absolute; font-size: 16px; font-weight: 800; font-family: var(--font-mono); color: var(--color-text-primary);">{securityReport.score}%</div>
              </div>
              <span style="font-size: 11px; font-weight: 700; letter-spacing: 0.05em; color: {getScoreColor(securityReport.score)}">{getScoreLabel(securityReport.score)}</span>
            </div>

            <!-- Clickable stats pills -->
            <div style="display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; width: 100%;">
              <button
                onclick={() => {
                  uiStore.securitySeverityFilter = 'Critical';
                  uiStore.setActiveTab('security-auditor');
                }}
                style="display: flex; align-items: center; gap: 4px; font-size: 11px; font-weight: 600; padding: 4px 10px; border-radius: 6px; border: none; cursor: pointer; transition: all 0.2s ease; background: rgba(239,68,68,.12); color: var(--color-error); font-family: inherit;"
                class="hover-scale-pill"
                title="Filter by Critical Issues"
              >
                <span style="width: 6px; height: 6px; border-radius: 50%; background: var(--color-error);"></span>
                {securityCriticalCount} Critical
              </button>
              <button
                onclick={() => {
                  uiStore.securitySeverityFilter = 'Warning';
                  uiStore.setActiveTab('security-auditor');
                }}
                style="display: flex; align-items: center; gap: 4px; font-size: 11px; font-weight: 600; padding: 4px 10px; border-radius: 6px; border: none; cursor: pointer; transition: all 0.2s ease; background: rgba(251,191,36,.12); color: var(--color-warning); font-family: inherit;"
                class="hover-scale-pill"
                title="Filter by Warning Issues"
              >
                <span style="width: 6px; height: 6px; border-radius: 50%; background: var(--color-warning);"></span>
                {securityWarningCount} Warnings
              </button>
              <button
                onclick={() => {
                  uiStore.securitySeverityFilter = 'Good';
                  uiStore.setActiveTab('security-auditor');
                }}
                style="display: flex; align-items: center; gap: 4px; font-size: 11px; font-weight: 600; padding: 4px 10px; border-radius: 6px; border: none; cursor: pointer; transition: all 0.2s ease; background: rgba(34,197,94,.12); color: var(--color-success); font-family: inherit;"
                class="hover-scale-pill"
                title="Filter by Passed Checks"
              >
                <span style="width: 6px; height: 6px; border-radius: 50%; background: var(--color-success);"></span>
                {securityPassedCount} Passed
              </button>
            </div>
          </div>
        {:else}
          <div style="display:flex; justify-content:center; padding: 12px 0;">
            <span class="text-muted" style="font-size:12px;">No security report loaded.</span>
          </div>
        {/if}
      </Card>

    </div>

    <!-- Column 3: Storage & SMART Health & System Events -->
    <div class="dashboard-column">
      <Card title="Storage & SMART Health" icon={HardDrive} class="panel-storage" style="display:flex; flex-direction:column; gap:12px;">
        <div class="storage-scroll" style="display:flex; flex-direction:column; gap:12px;">
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
          
          <div class="storage-dist-section mt-4" style="border-top: 1px dashed rgba(255,255,255,0.08); padding-top: 12px; margin-top: 12px;">
            <h4 class="text-sm font-semibold mb-2 flex items-center gap-2" style="font-size: 13px; font-weight: 600; margin-bottom: 8px;">
              <HardDrive size={14} color="var(--color-accent)" /> Storage Distribution
            </h4>
            {#if storageDist}
              <div style="display:flex; flex-direction:column; gap:10px;">
                <!-- Stacked bar chart (RPM, Flatpak, System) -->
                {#if storageDistTotal > 0}
                  <div style="height: 10px; background: rgba(0, 0, 0, 0.2); border-radius: 5px; overflow: hidden; display: flex; width: 100%;">
                    <div style="width: {(storageDist.rpm_gb / storageDistTotal * 100).toFixed(1)}%; background: var(--color-accent);" title="RPM"></div>
                    <div style="width: {(storageDist.flatpak_gb / storageDistTotal * 100).toFixed(1)}%; background: var(--color-text-secondary);" title="Flatpak"></div>
                    <div style="width: {(storageDist.system_gb / storageDistTotal * 100).toFixed(1)}%; background: var(--color-border);" title="System"></div>
                  </div>
                {/if}
                <!-- Legend and metrics (RPM and Flatpak clickable, System static) -->
                <div style="display: flex; gap: 16px; font-size: 11px; font-family: var(--font-mono); color: var(--color-text-secondary); flex-wrap: wrap;">
                  <button 
                    onclick={() => {
                      uiStore.appSourceFilter = 'RPM';
                      uiStore.setActiveTab('app-manager');
                    }}
                    style="display: flex; align-items: center; gap: 6px; background: transparent; border: none; padding: 0; cursor: pointer; color: inherit; font-family: inherit;"
                    class="legend-btn"
                    title="Filter App Manager by RPM packages"
                  >
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: var(--color-accent);"></span>
                    <span class="hover-underline">RPM: {storageDist.rpm_gb.toFixed(1)} GB</span>
                  </button>
                  <button 
                    onclick={() => {
                      uiStore.appSourceFilter = 'Flatpak';
                      uiStore.setActiveTab('app-manager');
                    }}
                    style="display: flex; align-items: center; gap: 6px; background: transparent; border: none; padding: 0; cursor: pointer; color: inherit; font-family: inherit;"
                    class="legend-btn"
                    title="Filter App Manager by Flatpak packages"
                  >
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: var(--color-text-secondary);"></span>
                    <span class="hover-underline">Flatpak: {storageDist.flatpak_gb.toFixed(1)} GB</span>
                  </button>
                  <div style="display: flex; align-items: center; gap: 6px; color: var(--color-text-muted);">
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: var(--color-border);"></span>
                    <span>System: {storageDist.system_gb.toFixed(1)} GB</span>
                  </div>
                </div>
              </div>
            {:else}
              <!-- Storage loader skeleton placeholder -->
              <div style="display:flex; flex-direction:column; gap:8px;">
                <div style="height: 10px; background: rgba(255,255,255,0.03); border-radius: 5px; width: 100%;" class="animate-pulse"></div>
                <div style="display:flex; gap:16px;">
                  <div style="height: 12px; background: rgba(255,255,255,0.03); border-radius: 4px; width: 70px;" class="animate-pulse"></div>
                  <div style="height: 12px; background: rgba(255,255,255,0.03); border-radius: 4px; width: 85px;" class="animate-pulse"></div>
                  <div style="height: 12px; background: rgba(255,255,255,0.03); border-radius: 4px; width: 80px;" class="animate-pulse"></div>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </Card>
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
    align-items: start;
  }

  .dashboard-column {
    display: flex;
    flex-direction: column;
    gap: 16px;
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
  .storage-fill { background: linear-gradient(90deg, var(--color-accent), var(--color-bg-raised)); }

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
  
  .disks-list { display: flex; flex-direction: column; gap: 12px; }
  .disk-item { display: flex; flex-direction: column; gap: 6px; }
  .disk-header { display: flex; justify-content: space-between; font-size: 13px; }
  .disk-stats { display: flex; align-items: center; gap: 8px; font-size: 11px; }
  .disk-pct { min-width: 120px; text-align: right; }
  


  .hover-bg-error-light:hover {
    background: rgba(239, 68, 68, 0.15) !important;
    border-color: rgba(239, 68, 68, 0.3) !important;
  }
  .hover-bg-warn-light:hover {
    background: rgba(245, 158, 11, 0.15) !important;
    border-color: rgba(245, 158, 11, 0.3) !important;
  }

  .text-primary { color: #3b82f6; }
  .text-success { color: #10b981; }
  .text-warning { color: #f59e0b; }
  .text-error { color: #ef4444; }
  .text-info { color: #06b6d4; }
  .text-purple { color: #a855f7; }

  .legend-btn:hover .hover-underline {
    text-decoration: underline;
    color: var(--color-text-primary);
  }

  .hover-scale-pill {
    transition: all 0.2s ease;
  }
  .hover-scale-pill:hover {
    filter: brightness(1.25);
    transform: translateY(-1px);
  }
</style>
