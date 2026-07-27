<script lang="ts">
  import { onMount } from 'svelte';
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { LayoutDashboard, HardDrive, Wifi, Server, Heart, Activity, RefreshCw, Shield, Cpu, Clock, Calendar, Laptop, Cable, Network, Lock, Disc, Layers } from '@lucide/svelte';
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

  function getIfaceMeta(name: string) {
    if (name.startsWith('wl')) return { label: 'Wi-Fi Interface', icon: Wifi, color: 'var(--color-accent)' };
    if (name.startsWith('en') || name.startsWith('eth')) return { label: 'Ethernet Adapter', icon: Cable, color: '#3b82f6' };
    if (name.startsWith('virbr') || name.startsWith('docker') || name.startsWith('veth') || name.startsWith('br-')) return { label: 'Virtual Bridge', icon: Network, color: '#a855f7' };
    if (name.startsWith('tun') || name.startsWith('wg') || name.startsWith('vpn')) return { label: 'VPN / Tunnel', icon: Lock, color: '#f59e0b' };
    return { label: 'Network Adapter', icon: Network, color: 'var(--color-text-secondary)' };
  }
  
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

  // Selected Storage Details Modal (Read-Only)
  let selectedStorageDetail = $state<{
    title: string;
    device: string;
    mount?: string;
    fs_type?: string;
    total_gb: number;
    used_gb: number;
    free_gb: number;
    percent: number;
    health_status?: string;
    model?: string;
    subvolumes?: any[];
  } | null>(null);

  function formatStorageBytes(gb: number) {
    if (!gb || gb <= 0) return '0 B';
    if (gb < 1.0) {
      const mb = gb * 1024;
      return `${mb.toFixed(0)} MB`;
    }
    return `${gb.toFixed(1)} GB`;
  }

  // Hierarchical storage derived computation
  let storageHierarchy = $derived.by(() => {
    if (!diskUsage || diskUsage.length === 0) return [];

    const deviceMap = new Map<string, any[]>();
    for (const d of diskUsage) {
      if (!d.device || !d.device.startsWith('/dev/')) continue;
      if (!deviceMap.has(d.device)) deviceMap.set(d.device, []);
      deviceMap.get(d.device)!.push(d);
    }

    const physicalDrives = new Map<string, {
      disk_path: string;
      model: string;
      health_status: string;
      partitions: any[];
      btrfsPools: any[];
    }>();

    for (const s of smartHealth) {
      physicalDrives.set(s.disk_path, {
        disk_path: s.disk_path,
        model: s.model,
        health_status: s.health_status,
        partitions: [],
        btrfsPools: []
      });
    }

    for (const [device, mounts] of deviceMap.entries()) {
      let parentDiskPath = Array.from(physicalDrives.keys()).find(p => device.startsWith(p));
      if (!parentDiskPath) {
        parentDiskPath = device.replace(/p?\d+$/, '');
        if (!physicalDrives.has(parentDiskPath)) {
          physicalDrives.set(parentDiskPath, {
            disk_path: parentDiskPath,
            model: 'Disk Drive',
            health_status: 'OK',
            partitions: [],
            btrfsPools: []
          });
        }
      }

      const drive = physicalDrives.get(parentDiskPath)!;

      if (mounts.length > 1 && mounts[0].fs_type === 'btrfs') {
        const primary = mounts[0];
        drive.btrfsPools.push({
          device,
          fs_type: primary.fs_type,
          total_gb: primary.total_gb,
          used_gb: primary.used_gb,
          free_gb: primary.free_gb,
          percent: primary.percent,
          subvolumes: mounts
        });
      } else {
        for (const m of mounts) {
          drive.partitions.push(m);
        }
      }
    }

    return Array.from(physicalDrives.values()).filter(d => d.partitions.length > 0 || d.btrfsPools.length > 0);
  });

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
          <div class="info-row" style="display:flex; justify-content:space-between; align-items:center;">
            <span style="display:flex; align-items:center; gap:6px;"><Laptop size={14} style="color:var(--color-accent);" /> Hostname:</span>
            <strong style="font-family:var(--font-mono);">{osInfo.hostname}</strong>
          </div>
          <div class="info-row" style="display:flex; justify-content:space-between; align-items:center;">
            <span style="display:flex; align-items:center; gap:6px;"><Disc size={14} style="color:#3b82f6;" /> OS Name:</span>
            <div style="display:flex; align-items:center; gap:6px;">
              <Badge variant="primary" style="font-size:10px; padding:1px 6px;">{osInfo.name}</Badge>
              <strong style="font-size:12px;">{osInfo.os_version}</strong>
            </div>
          </div>
          <div class="info-row" style="display:flex; justify-content:space-between; align-items:center;">
            <span style="display:flex; align-items:center; gap:6px;"><Cpu size={14} style="color:#a855f7;" /> Kernel:</span>
            <strong style="font-family:var(--font-mono); font-size:12px;">{osInfo.kernel_version}</strong>
          </div>
          <div class="info-row" style="display:flex; justify-content:space-between; align-items:center;">
            <span style="display:flex; align-items:center; gap:6px;"><Clock size={14} style="color:#10b981;" /> Uptime:</span>
            <strong style="color:var(--color-success); font-family:var(--font-mono);">{systemStats ? (systemStats.uptime_seconds / 3600).toFixed(1) + ' hours' : '...'}</strong>
          </div>
          <div class="info-row" style="display:flex; justify-content:space-between; align-items:center;">
            <span style="display:flex; align-items:center; gap:6px;"><Calendar size={14} style="color:#f59e0b;" /> Last Updated:</span>
            <strong style="font-size:11px; color:var(--color-text-secondary);">{lastSystemUpdate || '...'}</strong>
          </div>
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
              title="Click to view Error log entries in Journal Viewer"
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
              title="Click to view Warning log entries in Journal Viewer"
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
              title="Click to inspect failed systemd units in Service Manager"
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
                {@const meta = getIfaceMeta(iface.name)}
                <div class="network-item" style="display:flex; justify-content:space-between; align-items:center; background: rgba(0,0,0,0.2); padding: 10px 12px; border-radius: 8px; margin-bottom: 8px; border: 1px solid rgba(255,255,255,0.04);">
                  <div style="display:flex; align-items:center; gap:10px;">
                    <div style="width: 22px; height: 22px; border-radius: 5px; background: rgba(255,255,255,0.04); display: flex; align-items: center; justify-content: center; flex-shrink: 0;">
                      <meta.icon size={12} style="color: {meta.color};" />
                    </div>
                    <div style="display:flex; flex-direction:column; gap:2px;">
                      <div style="display:flex; align-items:center; gap:6px;">
                        <span class="iface-name" style="font-weight:600; font-size:13px; font-family:var(--font-mono); color: var(--color-text-primary);">{iface.name}</span>
                        <Badge variant={iface.is_up ? 'success' : 'muted'} style="font-size: 9px; padding: 1px 6px;">{iface.is_up ? 'UP' : 'DOWN'}</Badge>
                      </div>
                      <span style="font-size: 10px; color: var(--color-text-muted);">{meta.label}</span>
                    </div>
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
              <div style="display:flex; align-items:center; gap:6px;">
                <span style="font-size: 11px; font-weight: 700; letter-spacing: 0.05em; color: {getScoreColor(securityReport.score)}">{getScoreLabel(securityReport.score)}</span>
                <button
                  onclick={fetchSecurityReport}
                  disabled={loadingSecurity}
                  style="background: transparent; border: none; padding: 2px 4px; cursor: pointer; color: var(--color-text-muted); display: flex; align-items: center;"
                  title="Re-run Security Audit"
                >
                  <RefreshCw size={11} class={loadingSecurity ? 'animate-spin-slow' : ''} />
                </button>
              </div>
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
          {#if storageHierarchy.length > 0}
            <div class="disks-hierarchy" style="display:flex; flex-direction:column; gap:12px;">
              {#each storageHierarchy as drive}
                <!-- Drive Group Container -->
                <div class="drive-group" style="background: rgba(0,0,0,0.25); border: 1px solid rgba(255,255,255,0.06); border-radius: 8px; padding: 10px; display:flex; flex-direction:column; gap:8px;">
                  <!-- Physical Drive Header -->
                  <div 
                    class="drive-header" 
                    style="display:flex; justify-content:space-between; align-items:center; cursor:pointer;"
                    onclick={() => {
                      selectedStorageDetail = {
                        title: `Physical Drive (${drive.disk_path})`,
                        device: drive.disk_path,
                        model: drive.model,
                        health_status: drive.health_status,
                        total_gb: drive.partitions.reduce((acc, p) => acc + p.total_gb, 0) + drive.btrfsPools.reduce((acc, b) => acc + b.total_gb, 0),
                        used_gb: drive.partitions.reduce((acc, p) => acc + p.used_gb, 0) + drive.btrfsPools.reduce((acc, b) => acc + b.used_gb, 0),
                        free_gb: drive.partitions.reduce((acc, p) => acc + p.free_gb, 0) + drive.btrfsPools.reduce((acc, b) => acc + b.free_gb, 0),
                        percent: 0
                      };
                    }}
                    title="Click to view detailed drive properties"
                  >
                    <div style="display:flex; align-items:center; gap:8px;">
                      <HardDrive size={15} style="color:var(--color-accent);" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">{drive.disk_path}</div>
                        <div style="font-size:10px; color:var(--color-text-muted);">{drive.model}</div>
                      </div>
                    </div>
                    <Badge variant={drive.health_status === 'PASSED' || drive.health_status === 'OK' ? 'success' : (drive.health_status === 'UNKNOWN' ? 'warning' : 'error')}>
                      {drive.health_status}
                    </Badge>
                  </div>

                  <!-- Child Partitions & BTRFS Pools -->
                  <div class="drive-children" style="display:flex; flex-direction:column; gap:8px; padding-left: 8px; border-left: 2px solid rgba(0, 218, 243, 0.15); margin-left: 4px;">
                    <!-- Standalone Partitions -->
                    {#each drive.partitions as partition}
                      <div 
                        class="partition-item"
                        style="display:flex; flex-direction:column; gap:4px; padding: 6px 8px; background: rgba(255,255,255,0.02); border-radius: 6px; cursor:pointer;"
                        onclick={() => {
                          selectedStorageDetail = {
                            title: `Partition (${partition.mount})`,
                            device: partition.device,
                            mount: partition.mount,
                            fs_type: partition.fs_type,
                            total_gb: partition.total_gb,
                            used_gb: partition.used_gb,
                            free_gb: partition.free_gb,
                            percent: partition.percent
                          };
                        }}
                        title="Click to view partition details"
                      >
                        <div class="disk-header">
                          <strong style="color:var(--color-text-primary); font-size:12px;">{partition.mount}</strong>
                          <span class="text-muted" style="font-size: 11px;">{partition.device} ({partition.fs_type})</span>
                        </div>
                        <div class="disk-stats">
                          <div class="progress-bg" style="flex:1;">
                            <div class="progress-fill storage-fill" style="width: {partition.percent}%"></div>
                          </div>
                          <span class="disk-pct" style="font-size:11px;">{partition.percent.toFixed(1)}% ({formatStorageBytes(partition.used_gb)} / {formatStorageBytes(partition.total_gb)})</span>
                        </div>
                      </div>
                    {/each}

                    <!-- BTRFS Pools (Shared Subvolumes) -->
                    {#each drive.btrfsPools as pool}
                      <div 
                        class="btrfs-pool-item"
                        style="display:flex; flex-direction:column; gap:6px; padding: 8px; background: rgba(0, 218, 243, 0.03); border: 1px solid rgba(0, 218, 243, 0.12); border-radius: 6px;"
                      >
                        <div class="disk-header" style="align-items:center;">
                          <div style="display:flex; align-items:center; gap:6px;">
                            <span style="font-size:10px; font-weight:700; background:rgba(0,218,243,0.15); color:var(--color-accent); padding:2px 5px; border-radius:4px; font-family:var(--font-mono);">BTRFS POOL</span>
                            <span style="font-size:11px; color:var(--color-text-secondary); font-family:var(--font-mono);">{pool.device}</span>
                          </div>
                          <span class="text-muted" style="font-size: 11px;">Shared Pool ({formatStorageBytes(pool.total_gb)})</span>
                        </div>

                        <div class="disk-stats">
                          <div class="progress-bg" style="flex:1;">
                            <div class="progress-fill storage-fill" style="width: {pool.percent}%"></div>
                          </div>
                          <span class="disk-pct" style="font-size:11px;">{pool.percent.toFixed(1)}% Used</span>
                        </div>

                        <!-- Nested Subvolumes with visual tree connectors -->
                        <div class="subvolumes-list" style="display:flex; flex-direction:column; gap:4px; margin-top:4px; padding-left:6px; border-left: 2px solid rgba(0,218,243,0.25);">
                          {#each pool.subvolumes as subvol, i}
                            <div 
                              class="subvol-row"
                              style="display:flex; justify-content:space-between; align-items:center; font-size:11px; padding: 4px 8px; border-radius:4px; background:rgba(0,0,0,0.2); cursor:pointer;"
                              onclick={() => {
                                selectedStorageDetail = {
                                  title: `BTRFS Subvolume (${subvol.mount})`,
                                  device: subvol.device,
                                  mount: subvol.mount,
                                  fs_type: subvol.fs_type,
                                  total_gb: subvol.total_gb,
                                  used_gb: subvol.used_gb,
                                  free_gb: subvol.free_gb,
                                  percent: subvol.percent,
                                  subvolumes: pool.subvolumes.map(s => s.mount)
                                };
                              }}
                              title="Subvolume dynamically shares pool space. Click for info."
                            >
                              <div style="display:flex; align-items:center; gap:6px;">
                                <span style="color:var(--color-accent); opacity:0.7; font-family:var(--font-mono); font-weight:bold;">
                                  {i === pool.subvolumes.length - 1 ? '└─' : '├─'}
                                </span>
                                <span style="font-weight:600; color:var(--color-text-primary);">{subvol.mount}</span>
                                <span style="font-size:9px; color:var(--color-text-muted); background:rgba(255,255,255,0.05); padding:1px 4px; border-radius:3px;">subvol</span>
                              </div>
                              <span style="color:var(--color-text-secondary); font-family:var(--font-mono);">{formatStorageBytes(subvol.used_gb)}</span>
                            </div>
                          {/each}
                        </div>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {:else}
            <span class="text-muted">Loading storage data...</span>
          {/if}
          
          <div class="storage-dist-section mt-4" style="border-top: 1px dashed rgba(255,255,255,0.08); padding-top: 12px; margin-top: 12px;">
            <h4 class="text-sm font-semibold mb-2 flex items-center gap-2" style="font-size: 13px; font-weight: 600; margin-bottom: 8px;">
              <HardDrive size={14} color="var(--color-accent)" /> Storage Distribution
            </h4>
            {#if storageDist}
              <div style="display:flex; flex-direction:column; gap:10px;">
                <!-- Stacked bar chart (RPM cyan, Flatpak purple, System blue) -->
                {#if storageDistTotal > 0}
                  <div style="height: 10px; background: rgba(0, 0, 0, 0.3); border-radius: 5px; overflow: hidden; display: flex; width: 100%;">
                    <div style="width: {(storageDist.rpm_gb / storageDistTotal * 100).toFixed(1)}%; background: #00daf3;" title="RPM: {storageDist.rpm_gb.toFixed(1)} GB"></div>
                    <div style="width: {(storageDist.flatpak_gb / storageDistTotal * 100).toFixed(1)}%; background: #a855f7;" title="Flatpak: {storageDist.flatpak_gb.toFixed(1)} GB"></div>
                    <div style="width: {(storageDist.system_gb / storageDistTotal * 100).toFixed(1)}%; background: #3b82f6;" title="System: {storageDist.system_gb.toFixed(1)} GB"></div>
                  </div>
                {/if}
                <!-- Legend and metrics -->
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
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: #00daf3;"></span>
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
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: #a855f7;"></span>
                    <span class="hover-underline">Flatpak: {storageDist.flatpak_gb.toFixed(1)} GB</span>
                  </button>
                  <div style="display: flex; align-items: center; gap: 6px; color: var(--color-text-muted);">
                    <span style="width: 8px; height: 8px; border-radius: 50%; background: #3b82f6;"></span>
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

<!-- Read-Only Storage Details Modal -->
{#if selectedStorageDetail}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-backdrop" onclick={(e) => { if(e.target === e.currentTarget) selectedStorageDetail = null; }}>
    <div class="modal" style="width: 440px; max-width: calc(100vw - 32px); position: relative; z-index: 101;">
      <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:16px;">
        <h3 style="margin:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px; font-size:15px;">
          <HardDrive size={18} style="color:var(--color-accent)"/>
          {selectedStorageDetail.title}
        </h3>
        <button type="button" class="btn btn-outline" style="padding: 2px 8px;" onclick={() => selectedStorageDetail = null}>&times;</button>
      </div>

      <div style="display:flex; flex-direction:column; gap:10px; font-size:12px; margin-bottom:18px;">
        <div class="info-row">
          <span>Device Node</span>
          <strong style="color:var(--color-accent); font-family:var(--font-mono);">{selectedStorageDetail.device}</strong>
        </div>

        {#if selectedStorageDetail.mount}
          <div class="info-row">
            <span>Mount Target</span>
            <strong style="color:var(--color-text-primary); font-family:var(--font-mono);">{selectedStorageDetail.mount}</strong>
          </div>
        {/if}

        {#if selectedStorageDetail.fs_type}
          <div class="info-row">
            <span>File System</span>
            <span style="font-family:var(--font-mono); text-transform:uppercase;">{selectedStorageDetail.fs_type}</span>
          </div>
        {/if}

        {#if selectedStorageDetail.model}
          <div class="info-row">
            <span>Model / Hardware</span>
            <span style="color:var(--color-text-secondary);">{selectedStorageDetail.model}</span>
          </div>
        {/if}

        {#if selectedStorageDetail.health_status}
          <div class="info-row">
            <span>SMART Health</span>
            <Badge variant={selectedStorageDetail.health_status === 'PASSED' || selectedStorageDetail.health_status === 'OK' ? 'success' : 'warning'}>
              {selectedStorageDetail.health_status}
            </Badge>
          </div>
        {/if}

        <div class="info-row">
          <span>Total Space</span>
          <strong style="font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.total_gb)}</strong>
        </div>

        <div class="info-row">
          <span>Used Space</span>
          <strong style="color:var(--color-text-primary); font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.used_gb)} ({selectedStorageDetail.percent.toFixed(1)}%)</strong>
        </div>

        <div class="info-row">
          <span>Available Free</span>
          <strong style="color:var(--color-success); font-family:var(--font-mono);">{formatStorageBytes(selectedStorageDetail.free_gb)}</strong>
        </div>

        {#if selectedStorageDetail.subvolumes && selectedStorageDetail.subvolumes.length > 0}
          <div style="margin-top:8px; background:rgba(0,218,243,0.05); border:1px solid rgba(0,218,243,0.15); border-radius:6px; padding:10px;">
            <div style="font-size:11px; font-weight:700; color:var(--color-accent); margin-bottom:6px; text-transform:uppercase;">Shared BTRFS Subvolumes:</div>
            <div style="display:flex; gap:6px; flex-wrap:wrap;">
              {#each selectedStorageDetail.subvolumes as sub}
                <span style="background:rgba(255,255,255,0.08); padding:2px 8px; border-radius:4px; font-family:var(--font-mono); font-size:11px;">{sub}</span>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div style="display:flex; justify-content:space-between; align-items:center; background:rgba(255,255,255,0.03); padding:8px 12px; border-radius:6px;">
        <span style="font-size:11px; color:var(--color-text-muted);">🛡️ Read-Only System Information</span>
        <button type="button" class="btn btn-primary" onclick={() => selectedStorageDetail = null}>Close</button>
      </div>
    </div>
  </div>
{/if}

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
  .disks-hierarchy {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 420px; /* Increased container height by ~15% */
    overflow-y: auto;
    padding-right: 6px;
  }
  .disks-hierarchy::-webkit-scrollbar {
    width: 5px;
  }
  .disks-hierarchy::-webkit-scrollbar-track {
    background: rgba(0, 0, 0, 0.2);
    border-radius: 4px;
  }
  .disks-hierarchy::-webkit-scrollbar-thumb {
    background: rgba(0, 218, 243, 0.25);
    border-radius: 4px;
  }
  .disks-hierarchy::-webkit-scrollbar-thumb:hover {
    background: var(--color-accent);
  }

  .drive-header {
    transition: background 0.15s ease;
    padding: 4px 6px;
    border-radius: 6px;
  }
  .drive-header:hover {
    background: rgba(0, 218, 243, 0.05);
  }

  .partition-item {
    transition: all 0.15s ease;
  }
  .partition-item:hover {
    background: rgba(0, 218, 243, 0.08) !important;
    transform: translateX(2px);
  }

  .subvol-row {
    transition: all 0.15s ease;
  }
  .subvol-row:hover {
    background: rgba(0, 218, 243, 0.12) !important;
    color: var(--color-accent);
  }

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
