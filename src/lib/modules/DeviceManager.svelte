<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { 
    HardDrive, Cpu, RefreshCw, Monitor, Network, MemoryStick, Usb, Settings,
    ShieldCheck, Thermometer, Clock, Database, ChevronDown, Activity, Play, ShieldAlert
  } from '@lucide/svelte';
  
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  type ActiveTab = 'list' | 'smart' | 'topology';
  let activeTab = $state<ActiveTab>('list');

  // lshw listing variables
  let rawData = $state<any>(null);
  let devicesByClass = $state<Record<string, any[]>>({});
  let loading = $state(true);
  let expandedClasses = $state<Record<string, boolean>>({});
  let searchQuery = $state('');

  // SMART variables
  interface SmartDrive {
    name: string;
    info_name: string;
    type: string;
    protocol: string;
  }
  let smartDrives = $state<SmartDrive[]>([]);
  let selectedDrive = $state<string>('');
  let loadingSmartDrives = $state(false);
  let smartData = $state<any>(null);
  let loadingSmartData = $state(false);
  let smartError = $state<string>('');
  let triggerMessage = $state<string>('');
  let testingDrive = $state(false);

  // Hardware topology variables
  interface PciDevice {
    slot: string;
    class: string;
    vendor_device: string;
    rev: string;
  }
  interface UsbDevice {
    bus: string;
    device: string;
    id: string;
    name: string;
  }
  let pciDevices = $state<PciDevice[]>([]);
  let usbDevices = $state<UsbDevice[]>([]);
  let loadingTopology = $state(false);

  let filteredDevices = $derived.by(() => {
    if (!searchQuery) return devicesByClass;
    const lowerQ = searchQuery.toLowerCase();
    const result: Record<string, any[]> = {};
    for (const [cls, devices] of Object.entries(devicesByClass)) {
      const matches = devices.filter((d: any) => 
        (d.product && d.product.toLowerCase().includes(lowerQ)) ||
        (d.vendor && d.vendor.toLowerCase().includes(lowerQ)) ||
        (d.description && d.description.toLowerCase().includes(lowerQ)) ||
        (cls.toLowerCase().includes(lowerQ))
      );
      if (matches.length > 0) {
        result[cls] = matches;
      }
    }
    return result;
  });

  function toggleAll(expand: boolean) {
    const nextState = { ...expandedClasses };
    for (const cls of Object.keys(devicesByClass)) {
      nextState[cls] = expand;
    }
    expandedClasses = nextState;
  }

  function flattenLshw(node: any, result: any[] = []) {
    if (node.class && node.id && node.class !== 'system' && node.class !== 'bus') {
      result.push(node);
    }
    if (node.children && Array.isArray(node.children)) {
      for (const child of node.children) {
        flattenLshw(child, result);
      }
    }
    return result;
  }

  async function loadData() {
    loading = true;
    statusStore.setBusy('Scanning system devices…');
    try {
      const jsonStr: string = await invoke('device_get_all');
      rawData = JSON.parse(jsonStr);
      
      const allDevices = flattenLshw(rawData);
      
      const grouped: Record<string, any[]> = {};
      for (const dev of allDevices) {
        const cls = dev.class || 'unknown';
        if (!grouped[cls]) {
          grouped[cls] = [];
        }
        grouped[cls].push(dev);
      }
      
      // Sort keys alphabetically
      const sortedKeys = Object.keys(grouped).sort();
      const finalGrouped: Record<string, any[]> = {};
      for (const k of sortedKeys) {
        finalGrouped[k] = grouped[k];
        expandedClasses[k] = true; // Expand all by default
      }
      
      devicesByClass = finalGrouped;
      statusStore.setLastCommand('lshw -json', 0, true);
    } catch (e) {
      console.error(e);
      statusStore.setLastCommand('lshw -json', 1, false);
      uiStore.addToast(`Failed to load system devices: ${e}`, 'error');
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function loadSmartDrives() {
    loadingSmartDrives = true;
    try {
      const scanStr: string = await invoke('device_get_smart_drives');
      const scan = JSON.parse(scanStr);
      smartDrives = scan.devices || [];
      if (smartDrives.length > 0) {
        if (!selectedDrive || !smartDrives.some(d => d.name === selectedDrive)) {
          selectedDrive = smartDrives[0].name;
        }
        loadSmartData(selectedDrive);
      }
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Failed to scan SMART drives: ${e}`, 'error');
    } finally {
      loadingSmartDrives = false;
    }
  }

  async function loadSmartData(dev: string) {
    if (!dev) return;
    loadingSmartData = true;
    smartError = '';
    smartData = null;
    try {
      const smartStr: string = await invoke('device_get_smart_data', { device: dev });
      smartData = JSON.parse(smartStr);
      
      // Check for messages/errors reported inside the JSON payload itself
      if (smartData.smartctl && smartData.smartctl.messages) {
        const errors = smartData.smartctl.messages.filter((m: any) => m.severity === 'error');
        if (errors.length > 0) {
          smartError = errors.map((e: any) => e.string).join('\n');
        }
      }
    } catch (e) {
      console.error(e);
      smartError = String(e);
      uiStore.addToast(`S.M.A.R.T requires Root Privileges to access raw disk telemetry.`, 'warning');
    } finally {
      loadingSmartData = false;
    }
  }

  async function triggerSelfTest(testType: 'short' | 'long') {
    if (!selectedDrive) return;
    testingDrive = true;
    triggerMessage = '';
    try {
      const output: string = await invoke('device_trigger_self_test', {
        device: selectedDrive,
        testType
      });
      triggerMessage = `Self-test started successfully!\n\n${output}`;
      uiStore.addToast(`S.M.A.R.T self-test initiated: ${testType}`, 'success');
      // Reload shortly
      setTimeout(() => loadSmartData(selectedDrive), 4000);
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Failed to trigger self-test: ${e}`, 'error');
    } finally {
      testingDrive = false;
    }
  }

  async function loadTopology() {
    loadingTopology = true;
    try {
      const res: any = await invoke('device_get_topology');
      pciDevices = res.pci_devices || [];
      usbDevices = res.usb_devices || [];
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Failed to load hardware topology: ${e}`, 'error');
    } finally {
      loadingTopology = false;
    }
  }

  function getIconForClass(cls: string) {
    switch (cls) {
      case 'processor': return Cpu;
      case 'memory': return MemoryStick;
      case 'storage':
      case 'disk':
      case 'volume': return HardDrive;
      case 'network': return Network;
      case 'display': return Monitor;
      case 'usb': return Usb;
      default: return Settings;
    }
  }

  function formatSize(bytes: number) {
    if (!bytes) return 'Unknown';
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    if (bytes === 0) return '0 Byte';
    const i = parseInt(Math.floor(Math.log(bytes) / Math.log(1024)).toString());
    return Math.round((bytes / Math.pow(1024, i)) * 100) / 100 + ' ' + sizes[i];
  }

  $effect(() => {
    if (activeTab === 'list') {
      loadData();
    } else if (activeTab === 'smart') {
      loadSmartDrives();
    } else if (activeTab === 'topology') {
      loadTopology();
    }
  });
</script>

<div class="module-page">
  <PageHeader title="Device Manager" icon={HardDrive} description="Hardware inventory, disk diagnostics & motherboard topology">
    <TabGroup
      tabs={[
        { id: 'list', label: 'Device Inventory' },
        { id: 'smart', label: 'Disk Diagnostics (S.M.A.R.T)' },
        { id: 'topology', label: 'Hardware Bus Topology' }
      ]}
      bind:activeTab={activeTab}
    />
  </PageHeader>

  {#if activeTab === 'list'}
    <!-- Original Device Inventory Tab -->
    <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom: 16px; flex-shrink: 0; flex-wrap: wrap; gap: 8px;">
      <div style="display:flex; align-items:center; gap:8px;">
        <input 
          type="text" 
          class="input" 
          placeholder="Search devices..." 
          bind:value={searchQuery}
          style="width: 250px; padding: 6px 12px;"
        />
        <Button variant="outline" class="" style="padding: 6px 10px; font-size:12px;" onclick={() => toggleAll(true)}>Expand All</Button>
        <Button variant="outline" class="" style="padding: 6px 10px; font-size:12px;" onclick={() => toggleAll(false)}>Collapse All</Button>
      </div>
      <Button variant="outline" class="" style="padding: 6px 12px; font-size:12px; display:flex; align-items:center; gap:6px" onclick={loadData}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Rescan Devices
      </Button>
    </div>

    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">
      {#if loading}
        <div class="card" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted)">
          <RefreshCw size={24} class="animate-spin-slow" />
        </div>
      {:else}
        {#each Object.entries(filteredDevices) as [cls, devices]}
          {@const Icon = getIconForClass(cls)}
          <div class="card" style="padding:0; overflow:hidden; flex-shrink:0;">
            <button 
              class="device-header" 
              onclick={() => expandedClasses[cls] = !expandedClasses[cls]}
            >
              <div style="display:flex; align-items:center; gap:8px;">
                <Icon size={18} style="color:var(--color-accent)" />
                <span style="font-weight:600; text-transform:capitalize; font-size:15px; color:var(--color-text-primary)">
                  {cls}
                </span>
                <span class="badge badge-muted">{devices.length}</span>
              </div>
              <div style="color:var(--color-text-muted); transition:transform 0.2s" style:transform={expandedClasses[cls] ? 'rotate(180deg)' : 'rotate(0deg)'}>
                ▼
              </div>
            </button>
            
            {#if expandedClasses[cls]}
              <div style="padding: 12px 16px; display:flex; flex-direction:column; gap:8px; background:rgba(0,0,0,0.1);">
                {#each devices as dev}
                  <div style="background:var(--color-bg-surface); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
                    <div style="display:flex; justify-content:space-between; margin-bottom:4px;">
                      <span style="font-weight:600; color:var(--color-text-primary); font-size:14px;">
                        {dev.product || dev.description || dev.id}
                      </span>
                      {#if dev.vendor}
                        <span style="font-size:12px; color:var(--color-text-muted);">{dev.vendor}</span>
                      {/if}
                    </div>
                    
                    <div style="display:grid; grid-template-columns: 1fr 1fr; gap:8px; font-size:12px; color:var(--color-text-secondary); margin-top:8px;">
                      {#if dev.logicalname}
                        <div style="display:flex; gap:6px;">
                          <span style="color:var(--color-text-muted)">Logical:</span>
                          <code style="color:var(--color-info)">{Array.isArray(dev.logicalname) ? dev.logicalname.join(', ') : dev.logicalname}</code>
                        </div>
                      {/if}
                      {#if dev.size || dev.capacity}
                        <div style="display:flex; gap:6px;">
                          <span style="color:var(--color-text-muted)">Size/Capacity:</span>
                          <span style="color:var(--color-text-primary)">{formatSize(dev.size || dev.capacity)}</span>
                        </div>
                      {/if}
                      {#if dev.clock}
                        <div style="display:flex; gap:6px;">
                          <span style="color:var(--color-text-muted)">Clock:</span>
                          <span style="color:var(--color-text-primary)">{formatSize(dev.clock).replace('Bytes', 'Hz').replace('B', 'Hz')}</span>
                        </div>
                      {/if}
                      {#if dev.width}
                        <div style="display:flex; gap:6px;">
                          <span style="color:var(--color-text-muted)">Width:</span>
                          <span style="color:var(--color-text-primary)">{dev.width} bits</span>
                        </div>
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

  {:else if activeTab === 'smart'}
    <!-- S.M.A.R.T Disk Diagnostics Tab -->
    <div style="display:flex; gap:16px; align-items:center; margin-bottom:16px; flex-shrink:0;">
      <div style="display:flex; align-items:center; gap:8px; flex:1;">
        <span style="font-size:13px; color:var(--color-text-secondary); font-weight:600; width:90px;">Select Drive:</span>
        <div style="width:200px;">
          <Select bind:value={selectedDrive} onchange={(e: any) => { triggerMessage = ''; loadSmartData(e.target.value); }} disabled={loadingSmartDrives}>
            {#each smartDrives as drv}
              <option value={drv.name}>{drv.name} [{drv.protocol}]</option>
            {/each}
          </Select>
        </div>
        <Button variant="outline" class="btn-sm" style="padding:6px 10px;" onclick={loadSmartDrives} disabled={loadingSmartDrives}>
          <RefreshCw size={13} class={loadingSmartDrives ? 'animate-spin-slow' : ''} /> Rescan
        </Button>
      </div>
      
      {#if selectedDrive}
        <div style="display:flex; gap:8px;">
          <Button variant="outline" class="btn-sm" style="border-color:rgba(0, 218, 243, 0.25);" onclick={() => triggerSelfTest('short')} disabled={testingDrive || loadingSmartData}>
            <Play size={12} style="margin-right:4px;" /> Short Self-Test
          </Button>
          <Button variant="outline" class="btn-sm" style="border-color:rgba(0, 218, 243, 0.25);" onclick={() => triggerSelfTest('long')} disabled={testingDrive || loadingSmartData}>
            <Activity size={12} style="margin-right:4px;" /> Long Self-Test
          </Button>
        </div>
      {/if}
    </div>

    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">
      {#if loadingSmartData}
        <div class="card" style="display:flex;align-items:center;justify-content:center;padding:60px;">
          <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-text-muted);" />
        </div>
      {:else if smartError}
        <div class="card" style="border-color:rgba(239, 68, 68, 0.3); background:rgba(239, 68, 68, 0.03); display:flex; flex-direction:column; gap:12px; padding:20px;">
          <div style="display:flex; align-items:center; gap:8px; color:var(--color-error)">
            <ShieldAlert size={20} />
            <span style="font-weight:700; font-size:14px;">Failed to Retrieve S.M.A.R.T Attributes</span>
          </div>
          <p style="margin:0; font-size:13px; color:var(--color-text-secondary); line-height:1.5;">
            {smartError}
          </p>
          <div style="font-size:12px; color:var(--color-text-muted);">
            Ensure your user has Root permission enabled on the side panel, or check if the drive supports S.M.A.R.T operations.
          </div>
        </div>
      {:else if smartData}
        <!-- SMART Telemetry Layout -->
        <div style="display:grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap:16px; flex-shrink:0;">
          
          <!-- Health status & Basic telemetry -->
          <Card title="Diagnostics Status" icon={ShieldCheck}>
            <div style="display:flex; flex-direction:column; gap:16px; padding:8px 0;">
              <div style="display:flex; justify-content:space-between; align-items:center;">
                <span style="font-size:13px; color:var(--color-text-secondary);">S.M.A.R.T Health:</span>
                {#if smartData.smart_status}
                  <span class="badge {smartData.smart_status.passed ? 'badge-success' : 'badge-danger'}" style="font-size:13px; padding: 4px 10px;">
                    {smartData.smart_status.passed ? 'PASSED' : 'FAILED / WARNING'}
                  </span>
                {:else}
                  <span class="badge badge-muted">Unknown</span>
                {/if}
              </div>

              <div style="height:1px; background:var(--color-border); opacity:0.5;"></div>

              <div style="display:grid; grid-template-columns: 1fr 1fr; gap:12px;">
                <div style="display:flex; align-items:center; gap:8px;">
                  <Thermometer size={16} style="color:var(--color-accent);" />
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:9px; text-transform:uppercase; color:var(--color-text-muted);">Temp</span>
                    <span style="font-size:14px; font-weight:700; color:var(--color-text-primary)">
                      {smartData.temperature?.current ? `${smartData.temperature.current} °C` : '—'}
                    </span>
                  </div>
                </div>

                <div style="display:flex; align-items:center; gap:8px;">
                  <Clock size={16} style="color:var(--color-accent);" />
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:9px; text-transform:uppercase; color:var(--color-text-muted);">Power Hours</span>
                    <span style="font-size:14px; font-weight:700; color:var(--color-text-primary)">
                      {smartData.power_on_time?.hours ? `${smartData.power_on_time.hours} hrs` : '—'}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </Card>

          <!-- Model & Specs -->
          <Card title="Drive Specifications" icon={Database}>
            <div style="display:flex; flex-direction:column; gap:8px; font-size:12.5px;">
              <div style="display:flex; justify-content:space-between;">
                <span style="color:var(--color-text-muted)">Model Family:</span>
                <span style="color:var(--color-text-primary); font-weight:600;">{smartData.model_family || '—'}</span>
              </div>
              <div style="display:flex; justify-content:space-between;">
                <span style="color:var(--color-text-muted)">Device Model:</span>
                <span style="color:var(--color-text-primary); font-weight:600;">{smartData.device?.model_name || smartData.model_name || '—'}</span>
              </div>
              <div style="display:flex; justify-content:space-between;">
                <span style="color:var(--color-text-muted)">Serial Number:</span>
                <code style="color:var(--color-text-accent)">{smartData.serial_number || '—'}</code>
              </div>
              <div style="display:flex; justify-content:space-between;">
                <span style="color:var(--color-text-muted)">Capacity:</span>
                <span style="color:var(--color-text-primary)">
                  {smartData.user_capacity?.bytes ? formatSize(smartData.user_capacity.bytes) : '—'}
                </span>
              </div>
              <div style="display:flex; justify-content:space-between;">
                <span style="color:var(--color-text-muted)">Interface / protocol:</span>
                <span style="color:var(--color-text-primary); text-transform:uppercase;">{smartData.device?.protocol || '—'}</span>
              </div>
            </div>
          </Card>

        </div>

        {#if triggerMessage}
          <div class="card" style="background:var(--color-bg-card); border-color:var(--color-border); font-family:var(--font-mono); font-size:12px; color:var(--color-text-secondary); white-space:pre-wrap; padding:16px; margin-top:8px;">
            {triggerMessage}
          </div>
        {/if}

        <!-- SMART Attributes Table -->
        {#if smartData.ata_smart_attributes && smartData.ata_smart_attributes.table}
          <div class="card" style="padding:0; overflow:hidden; flex-shrink:0;">
            <div style="padding:14px 16px; border-bottom:1px solid var(--color-border); background:var(--color-bg-raised)">
              <h4 style="margin:0; font-size:14px; font-weight:600; color:var(--color-text-primary);">S.M.A.R.T Raw Attributes</h4>
            </div>
            
            <div class="table-wrap" style="border:none; border-radius:0; max-height:400px; overflow-y:auto;">
              <table>
                <thead>
                  <tr>
                    <th style="width:60px;">ID</th>
                    <th>Attribute Name</th>
                    <th style="text-align:center;">Value</th>
                    <th style="text-align:center;">Worst</th>
                    <th style="text-align:center;">Threshold</th>
                    <th style="text-align:right;">Raw Value</th>
                    <th style="width:100px; text-align:center;">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {#each smartData.ata_smart_attributes.table as attr}
                    <tr>
                      <td><code style="font-size:11px;">{attr.id}</code></td>
                      <td style="font-weight:600; color:var(--color-text-primary);">{attr.name}</td>
                      <td style="text-align:center; font-family:var(--font-mono);">{attr.value}</td>
                      <td style="text-align:center; font-family:var(--font-mono);">{attr.worst}</td>
                      <td style="text-align:center; font-family:var(--font-mono);">{attr.thresh || '—'}</td>
                      <td style="text-align:right; font-family:var(--font-mono); color:var(--color-text-accent);">{attr.raw?.value ?? attr.raw_string}</td>
                      <td style="text-align:center;">
                        <span class="badge {attr.when_failed ? 'badge-danger' : 'badge-success'}" style="font-size:10px;">
                          {attr.when_failed ? 'FAILING' : 'OK'}
                        </span>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {:else if smartData.nvme_smart_health_information_log}
          <!-- NVMe Smart Attributes list -->
          {@const nvme = smartData.nvme_smart_health_information_log}
          <div class="card" style="padding:16px;">
            <h4 style="margin:0 0 12px; font-size:14px; font-weight:600; color:var(--color-text-primary);">NVMe SMART / Health Log</h4>
            <div style="display:grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap:12px; font-size:12.5px;">
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Critical Warning:</span>
                <span class="badge {nvme.critical_warning === 0 ? 'badge-success' : 'badge-danger'}">{nvme.critical_warning}</span>
              </div>
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Percentage Used (Wear):</span>
                <span style="color:var(--color-text-primary); font-weight:700;">{nvme.percentage_used}%</span>
              </div>
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Data Units Read:</span>
                <span style="color:var(--color-text-primary); font-family:var(--font-mono);">{nvme.data_units_read || '—'}</span>
              </div>
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Data Units Written:</span>
                <span style="color:var(--color-text-primary); font-family:var(--font-mono);">{nvme.data_units_written || '—'}</span>
              </div>
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Media Errors:</span>
                <span class="badge {nvme.media_errors === 0 ? 'badge-success' : 'badge-danger'}">{nvme.media_errors}</span>
              </div>
              <div class="stat-item" style="display:flex; justify-content:space-between; padding:8px; border-bottom:1px solid rgba(255,255,255,0.03);">
                <span style="color:var(--color-text-muted)">Unsafe Shutdowns:</span>
                <span style="color:var(--color-text-primary); font-family:var(--font-mono);">{nvme.unsafe_shutdowns || 0}</span>
              </div>
            </div>
          </div>
        {/if}
      {:else}
        <div style="text-align:center; padding:48px; color:var(--color-text-muted);">
          No SMART drive telemetry loaded.
        </div>
      {/if}
    </div>

  {:else if activeTab === 'topology'}
    <!-- Hardware Bus Topology Tab -->
    <div class="module-content-scroll" style="display:grid; grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); gap:16px;">
      
      {#if loadingTopology}
        <div class="card" style="grid-column: 1 / -1; display:flex; align-items:center; justify-content:center; padding:60px;">
          <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-text-muted);" />
        </div>
      {:else}
        <!-- PCI Bus Card -->
        <Card title="PCI Express Bus Topology (lspci)" icon={Cpu}>
          <div style="display:flex; flex-direction:column; gap:8px; font-size:12px;">
            {#each pciDevices as pci}
              <div style="background:rgba(0,218,243,0.01); border:1px solid var(--color-border); border-radius:6px; padding:10px; display:flex; flex-direction:column; gap:4px;">
                <div style="display:flex; justify-content:space-between; align-items:center;">
                  <code style="color:var(--color-accent); font-weight:600; font-size:11px;">{pci.slot}</code>
                  {#if pci.rev}
                    <span class="badge badge-muted" style="font-size:9px;">{pci.rev}</span>
                  {/if}
                </div>
                <div style="font-weight:600; color:var(--color-text-primary);">{pci.class}</div>
                <div style="color:var(--color-text-secondary); line-height:1.4;">{pci.vendor_device}</div>
              </div>
            {/each}
          </div>
        </Card>

        <!-- USB Bus Card -->
        <Card title="USB Bus Connections (lsusb)" icon={Usb}>
          <div style="display:flex; flex-direction:column; gap:12px; font-size:12px;">
            {#if usbDevices.length === 0}
              <div style="text-align:center; padding:24px; color:var(--color-text-muted);">
                No USB devices detected.
              </div>
            {:else}
              <!-- Group USB by Bus ID -->
              {@const buses = [...new Set(usbDevices.map(u => u.bus))].sort()}
              {#each buses as bus}
                <div style="background:rgba(0,0,0,0.15); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
                  <div style="font-weight:700; color:var(--color-text-primary); border-bottom:1px solid var(--color-border); padding-bottom:6px; margin-bottom:8px; display:flex; align-items:center; gap:6px;">
                    <Usb size={14} style="color:var(--color-accent);" /> USB Bus {bus}
                  </div>
                  
                  <div style="display:flex; flex-direction:column; gap:8px;">
                    {#each usbDevices.filter(u => u.bus === bus) as usb}
                      <div style="display:flex; align-items:flex-start; justify-content:space-between; gap:12px; background:var(--color-bg-surface); padding:8px 10px; border-radius:4px; border:1px solid rgba(255,255,255,0.02)">
                        <div style="display:flex; flex-direction:column; gap:2px;">
                          <span style="font-weight:600; color:var(--color-text-primary);">{usb.name}</span>
                          <span style="font-size:10px; color:var(--color-text-muted);">Device {usb.device}</span>
                        </div>
                        <code style="font-size:10.5px; color:var(--color-text-accent); flex-shrink:0;">{usb.id}</code>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </Card>
      {/if}

    </div>
  {/if}
</div>

<style>
  .device-header {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    font-family: var(--font-sans);
    transition: background 0.15s;
  }
  .device-header:hover {
    background: rgba(255,255,255,0.02);
  }
  .device-header:last-child {
    border-bottom: none;
  }
</style>
