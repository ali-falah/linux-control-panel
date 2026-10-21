<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Select from '../components/ui/Select.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { 
    HardDrive, Cpu, RefreshCw, Monitor, Network, MemoryStick, Usb, Settings,
    ShieldCheck, Thermometer, Clock, Database, ChevronDown, Activity, Play, ShieldAlert,
    CheckCircle, AlertTriangle, Search, MoreHorizontal, Copy, Unplug, RotateCcw, Info
  } from '@lucide/svelte';
  
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  type ActiveTab = 'list' | 'smart' | 'topology';
  let activeTab = $state<ActiveTab>(
    uiStore.targetSubTab && ['list', 'smart', 'topology'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'list'
  );
  if (uiStore.targetSubTab && ['list', 'smart', 'topology'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  // lshw listing variables
  let rawData = $state<any>(null);
  let devicesByClass = $state<Record<string, any[]>>({});
  let loading = $state(true);
  let expandedClasses = $state<Record<string, boolean>>({});
  let searchQuery = $state('');

  // Per-device action state: tracks which device's module info we've loaded
  // key = "cls:index" (unique per row), value = { module: string | null, loading: bool, actionBusy: bool }
  let deviceModuleState = $state<Record<string, { module: string | null; loading: boolean; actionBusy: boolean }>>({});
  // Which device row's kebab menu is open — keyed by "cls:index" to be unique
  let openMenuKey = $state<string | null>(null);

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

  let totalDeviceCount = $derived(
    Object.values(filteredDevices).reduce((acc, arr) => acc + arr.length, 0)
  );

  function toggleAll(expand: boolean) {
    const nextState = { ...expandedClasses };
    for (const cls of Object.keys(devicesByClass)) {
      nextState[cls] = expand;
    }
    expandedClasses = nextState;
  }

  // A class is expanded if the user explicitly opened/closed it, otherwise it
  // follows the default: collapsed — except while searching, where matching
  // classes auto-open so the hits are actually visible.
  function isClassExpanded(cls: string) {
    if (cls in expandedClasses) return expandedClasses[cls];
    return searchQuery.trim().length > 0;
  }

  function toggleClass(cls: string) {
    expandedClasses[cls] = !isClassExpanded(cls);
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
      
      const sortedKeys = Object.keys(grouped).sort();
      const finalGrouped: Record<string, any[]> = {};
      for (const k of sortedKeys) {
        finalGrouped[k] = grouped[k];
      }
      
      devicesByClass = finalGrouped;
      expandedClasses = {}; // all classes start collapsed
      deviceModuleState = {}; // reset on rescan
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

  // ── Device actions ────────────────────────────────────────────────────

  function getDeviceState(rowKey: string) {
    return deviceModuleState[rowKey] ?? { module: null, loading: false, actionBusy: false };
  }

  async function openDeviceMenu(dev: any, rowKey: string) {
    openMenuKey = rowKey;

    // Only fetch module info once per row and only for PCI devices (have businfo)
    const current = getDeviceState(rowKey);
    if (current.module === null && !current.loading && dev.businfo) {
      deviceModuleState[rowKey] = { ...current, loading: true };
      try {
        // businfo looks like "pci@0000:00:1f.6" — strip the "pci@" prefix
        const slot = dev.businfo.replace(/^[a-z]+@/, '');
        const mod: string = await invoke('device_get_pci_driver', { slot });
        deviceModuleState[rowKey] = { module: mod, loading: false, actionBusy: false };
      } catch {
        // no driver — that's fine
        deviceModuleState[rowKey] = { module: '', loading: false, actionBusy: false };
      }
    }
  }

  function closeDeviceMenu() {
    openMenuKey = null;
  }

  function copyDeviceInfo(dev: any) {
    closeDeviceMenu();
    const lines = [
      `Name: ${dev.product || dev.description || dev.id}`,
      dev.vendor ? `Vendor: ${dev.vendor}` : null,
      dev.businfo ? `Bus info: ${dev.businfo}` : null,
      dev.logicalname ? `Logical: ${Array.isArray(dev.logicalname) ? dev.logicalname.join(', ') : dev.logicalname}` : null,
      (dev.size || dev.capacity) ? `Size: ${formatSize(dev.size || dev.capacity)}` : null,
      dev.clock ? `Clock: ${formatSize(dev.clock).replace('Bytes','Hz').replace('B','Hz')}` : null,
    ].filter(Boolean).join('\n');
    navigator.clipboard.writeText(lines).then(() => {
      uiStore.addToast('Device info copied to clipboard', 'success');
    });
  }

  function confirmModuleAction(dev: any, rowKey: string, action: 'unload' | 'reload') {
    closeDeviceMenu();
    const mod = getDeviceState(rowKey).module;
    if (!mod) return;

    const isUnload = action === 'unload';
    uiStore.confirm(
      isUnload ? `Unload kernel module "${mod}"?` : `Reload kernel module "${mod}"?`,
      isUnload
        ? `This will unload the "${mod}" kernel module from the running system.\n\nThe device "${dev.product || dev.description}" may stop working until the module is reloaded or the system is restarted.\n\nDo NOT unload modules for critical devices (storage, GPU, USB hubs).`
        : `This will reload the "${mod}" kernel module.\n\nThe device will briefly disconnect and reconnect.`,
      async () => {
        deviceModuleState[rowKey] = { ...getDeviceState(rowKey), actionBusy: true };
        try {
          const result: string = await invoke('device_set_module_state', { module: mod, action });
          uiStore.addToast(result, 'success');
        } catch (e) {
          uiStore.addToast(`Failed: ${e}`, 'error');
        } finally {
          deviceModuleState[rowKey] = { ...getDeviceState(rowKey), actionBusy: false };
        }
      },
      isUnload // danger=true only for unload
    );
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

<!-- Close menu on outside click -->
<svelte:window onclick={(e) => {
  if (openMenuKey && !(e.target as Element)?.closest?.('.device-menu-anchor')) {
    closeDeviceMenu();
  }
}} />

<div class="module-page">
  <PageHeader title="Device Manager" icon={HardDrive} description="Hardware inventory, disk diagnostics & motherboard topology">
    <TabGroup
      tabs={[
        { id: 'list', label: 'Device Inventory' },
        { id: 'smart', label: 'S.M.A.R.T Diagnostics' },
        { id: 'topology', label: 'Bus Topology' }
      ]}
      bind:activeTab={activeTab}
    />
  </PageHeader>

  {#if activeTab === 'list'}
    <!-- Device Inventory Tab -->
    <div class="inventory-toolbar">
      <div class="toolbar-left">
        <SearchBar
          bind:value={searchQuery}
          placeholder="Search devices, vendors…"
          count={searchQuery ? totalDeviceCount : undefined}
          style="width: 260px;"
        />
        <Button variant="ghost" size="sm" onclick={() => toggleAll(true)}>Expand All</Button>
        <Button variant="ghost" size="sm" onclick={() => toggleAll(false)}>Collapse All</Button>
      </div>
      <Button variant="outline" size="sm" onclick={loadData} disabled={loading}>
        <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} />
        Rescan
      </Button>
    </div>

    <div class="module-content-scroll inventory-list">
      {#if loading}
        <div class="loading-state">
          <RefreshCw size={22} class="animate-spin-slow" />
          <span>Scanning hardware…</span>
        </div>
      {:else if Object.keys(filteredDevices).length === 0}
        <EmptyState
          icon={Search}
          title="No devices found"
          description={searchQuery ? `No hardware matched "${searchQuery}". Try a different term.` : 'No hardware detected.'}
          actionLabel={searchQuery ? 'Clear search' : ''}
          onAction={() => { searchQuery = ''; }}
        />
      {:else}
        {#each Object.entries(filteredDevices) as [cls, devices]}
          {@const Icon = getIconForClass(cls)}
          <div class="accordion-card">
            <button
              class="accordion-header"
              onclick={() => toggleClass(cls)}
              aria-expanded={isClassExpanded(cls)}
              aria-controls="acc-body-{cls}"
            >
              <div class="accordion-header-left">
                <span class="cls-icon-chip">
                  <Icon size={14} class="cls-icon" />
                </span>
                <span class="cls-label">{cls}</span>
                <Badge variant="muted">{devices.length}</Badge>
              </div>
              <ChevronDown size={15} class="chevron {isClassExpanded(cls) ? 'rotated' : ''}" />
            </button>

            {#if isClassExpanded(cls)}
              <div class="accordion-body" id="acc-body-{cls}">
                {#each devices as dev, i}
                  {@const rowKey = `${cls}:${i}`}
                  {@const devState = getDeviceState(rowKey)}
                  <div class="device-row {i < devices.length - 1 ? 'has-divider' : ''}">
                    <div class="device-main">
                      <span class="device-name">{dev.product || dev.description || dev.id}</span>
                      <div class="device-row-actions">
                        {#if dev.vendor}
                          <span class="device-vendor">{dev.vendor}</span>
                        {/if}
                        <!-- ⋯ menu -->
                        <div class="device-menu-anchor">
                          <button
                            class="action-dot-btn"
                            title="Device actions"
                            onclick={(e) => { e.stopPropagation(); if (openMenuKey === rowKey) { closeDeviceMenu(); } else { openMenuKey = rowKey; openDeviceMenu(dev, rowKey); } }}
                            disabled={devState.actionBusy}
                          >
                            {#if devState.actionBusy}
                              <RefreshCw size={13} class="animate-spin-slow" />
                            {:else}
                              <MoreHorizontal size={14} />
                            {/if}
                          </button>

                          {#if openMenuKey === rowKey}
                            <div class="device-dropdown" role="menu">
                              <!-- Copy info — always available -->
                              <button class="dropdown-item" onclick={() => copyDeviceInfo(dev)}>
                                <Copy size={13} />
                                <span>Copy device info</span>
                              </button>

                              {#if devState.loading}
                                <div class="dropdown-loading">
                                  <RefreshCw size={12} class="animate-spin-slow" />
                                  <span>Checking driver…</span>
                                </div>
                              {:else if devState.module}
                                <div class="dropdown-divider"></div>
                                <div class="dropdown-module-badge">
                                  <Info size={11} />
                                  <code>{devState.module}</code>
                                </div>
                                <button class="dropdown-item warning" onclick={() => confirmModuleAction(dev, rowKey, 'unload')}>
                                  <Unplug size={13} />
                                  <span>Unload module</span>
                                </button>
                                <button class="dropdown-item" onclick={() => confirmModuleAction(dev, rowKey, 'reload')}>
                                  <RotateCcw size={13} />
                                  <span>Reload module</span>
                                </button>
                              {:else if devState.module === ''}
                                <div class="dropdown-divider"></div>
                                <div class="dropdown-loading">
                                  <span style="color: var(--color-text-muted);">No kernel driver bound</span>
                                </div>
                              {/if}
                            </div>
                          {/if}
                        </div>
                      </div>
                    </div>
                    <div class="device-meta">
                      {#if dev.logicalname}
                        <span class="meta-item">
                          <span class="meta-label">Logical</span>
                          <code class="meta-val accent">{Array.isArray(dev.logicalname) ? dev.logicalname.join(', ') : dev.logicalname}</code>
                        </span>
                      {/if}
                      {#if dev.size || dev.capacity}
                        <span class="meta-item">
                          <span class="meta-label">Size</span>
                          <span class="meta-val">{formatSize(dev.size || dev.capacity)}</span>
                        </span>
                      {/if}
                      {#if dev.clock}
                        <span class="meta-item">
                          <span class="meta-label">Clock</span>
                          <span class="meta-val">{formatSize(dev.clock).replace('Bytes', 'Hz').replace('B', 'Hz')}</span>
                        </span>
                      {/if}
                      {#if dev.width}
                        <span class="meta-item">
                          <span class="meta-label">Width</span>
                          <span class="meta-val">{dev.width} bits</span>
                        </span>
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
    <div class="smart-toolbar">
      <div class="toolbar-left">
        <span class="toolbar-label">Drive</span>
        <div style="width: 220px;">
          <Select bind:value={selectedDrive} onchange={(e: any) => { triggerMessage = ''; loadSmartData(e.target.value); }} disabled={loadingSmartDrives}>
            {#each smartDrives as drv}
              <option value={drv.name}>{drv.name} [{drv.protocol}]</option>
            {/each}
          </Select>
        </div>
        <Button variant="ghost" size="sm" onclick={loadSmartDrives} disabled={loadingSmartDrives}>
          <RefreshCw size={13} class={loadingSmartDrives ? 'animate-spin-slow' : ''} />
          Rescan
        </Button>
      </div>

      {#if selectedDrive}
        <div class="toolbar-right">
          <Button variant="outline" size="sm" onclick={() => triggerSelfTest('short')} disabled={testingDrive || loadingSmartData}>
            <Play size={12} />
            Short Self-Test
          </Button>
          <Button variant="outline" size="sm" onclick={() => triggerSelfTest('long')} disabled={testingDrive || loadingSmartData}>
            <Activity size={12} />
            Long Self-Test
          </Button>
        </div>
      {/if}
    </div>

    <div class="module-content-scroll smart-content">
      {#if loadingSmartData || loadingSmartDrives}
        <div class="loading-state">
          <RefreshCw size={22} class="animate-spin-slow" />
          <span>Loading S.M.A.R.T data…</span>
        </div>
      {:else if smartError}
        <div class="error-card">
          <div class="error-header">
            <ShieldAlert size={18} />
            <span>Failed to Retrieve S.M.A.R.T Attributes</span>
          </div>
          <p class="error-body">{smartError}</p>
          <p class="error-hint">Ensure Root permission is enabled in the side panel, or verify the drive supports S.M.A.R.T operations.</p>
        </div>
      {:else if smartData}
        <!-- Summary row -->
        <div class="smart-summary-row">
          <!-- Health Status card -->
          <Card title="Health Status" icon={ShieldCheck}>
            <div class="smart-health-body">
              {#if smartData.smart_status}
                {@const passed = smartData.smart_status.passed}
                <div class="health-indicator {passed ? 'health-ok' : 'health-fail'}">
                  {#if passed}
                    <CheckCircle size={32} />
                  {:else}
                    <AlertTriangle size={32} />
                  {/if}
                  <div class="health-text-group">
                    <span class="health-verdict">{passed ? 'PASSED' : 'FAILED'}</span>
                    <span class="health-subtitle">{passed ? 'Drive is healthy' : 'Drive needs attention'}</span>
                  </div>
                </div>
              {:else}
                <div class="health-unknown">Unknown health status</div>
              {/if}

              <div class="divider"></div>

              <div class="smart-stat-grid">
                <div class="smart-stat">
                  <Thermometer size={14} class="stat-icon" />
                  <div>
                    <span class="stat-label">Temperature</span>
                    <span class="stat-value">{smartData.temperature?.current ? `${smartData.temperature.current} °C` : '—'}</span>
                  </div>
                </div>
                <div class="smart-stat">
                  <Clock size={14} class="stat-icon" />
                  <div>
                    <span class="stat-label">Power-On Hours</span>
                    <span class="stat-value">{smartData.power_on_time?.hours ? `${smartData.power_on_time.hours} h` : '—'}</span>
                  </div>
                </div>
              </div>
            </div>
          </Card>

          <!-- Drive Specs card -->
          <Card title="Drive Specifications" icon={Database}>
            <div class="spec-list">
              <div class="spec-row">
                <span class="spec-label">Model Family</span>
                <span class="spec-val">{smartData.model_family || '—'}</span>
              </div>
              <div class="divider"></div>
              <div class="spec-row">
                <span class="spec-label">Device Model</span>
                <span class="spec-val">{smartData.device?.model_name || smartData.model_name || '—'}</span>
              </div>
              <div class="divider"></div>
              <div class="spec-row">
                <span class="spec-label">Serial Number</span>
                <code class="spec-val mono">{smartData.serial_number || '—'}</code>
              </div>
              <div class="divider"></div>
              <div class="spec-row">
                <span class="spec-label">Capacity</span>
                <span class="spec-val">{smartData.user_capacity?.bytes ? formatSize(smartData.user_capacity.bytes) : '—'}</span>
              </div>
              <div class="divider"></div>
              <div class="spec-row">
                <span class="spec-label">Interface</span>
                <span class="spec-val upper">{smartData.device?.protocol || '—'}</span>
              </div>
            </div>
          </Card>
        </div>

        {#if triggerMessage}
          <div class="test-output">{triggerMessage}</div>
        {/if}

        <!-- ATA SMART Attributes Table -->
        {#if smartData.ata_smart_attributes?.table}
          <div class="accordion-card" style="margin-top: 0;">
            <div class="section-header">
              <span class="section-title">S.M.A.R.T Raw Attributes</span>
              <Badge variant="muted">{smartData.ata_smart_attributes.table.length}</Badge>
            </div>
            <div class="table-wrap" style="border: none; border-radius: 0; max-height: 420px; overflow-y: auto;">
              <table>
                <thead>
                  <tr>
                    <th style="width: 52px;">ID</th>
                    <th>Attribute</th>
                    <th style="text-align: center;">Value</th>
                    <th style="text-align: center;">Worst</th>
                    <th style="text-align: center;">Thresh</th>
                    <th style="text-align: right;">Raw</th>
                    <th style="width: 90px; text-align: center;">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {#each smartData.ata_smart_attributes.table as attr}
                    <tr>
                      <td><code style="font-size: 11px;">{attr.id}</code></td>
                      <td style="font-weight: 600; color: var(--color-text-primary);">{attr.name}</td>
                      <td style="text-align: center; font-family: var(--font-mono);">{attr.value}</td>
                      <td style="text-align: center; font-family: var(--font-mono);">{attr.worst}</td>
                      <td style="text-align: center; font-family: var(--font-mono);">{attr.thresh || '—'}</td>
                      <td style="text-align: right; font-family: var(--font-mono); color: var(--color-text-secondary);">{attr.raw?.value ?? attr.raw_string}</td>
                      <td style="text-align: center;">
                        <Badge variant={attr.when_failed ? 'error' : 'success'} style="font-size: 10px;">
                          {attr.when_failed ? 'FAIL' : 'OK'}
                        </Badge>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {:else if smartData.nvme_smart_health_information_log}
          {@const nvme = smartData.nvme_smart_health_information_log}
          <div class="accordion-card" style="margin-top: 0;">
            <div class="section-header">
              <span class="section-title">NVMe SMART / Health Log</span>
            </div>
            <div class="nvme-grid">
              <div class="nvme-row">
                <span class="spec-label">Critical Warning</span>
                <Badge variant={nvme.critical_warning === 0 ? 'success' : 'error'}>{nvme.critical_warning}</Badge>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Wear (% Used)</span>
                <span class="spec-val mono">{nvme.percentage_used}%</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Data Units Read</span>
                <span class="spec-val mono">{nvme.data_units_read || '—'}</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Data Units Written</span>
                <span class="spec-val mono">{nvme.data_units_written || '—'}</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Media Errors</span>
                <Badge variant={nvme.media_errors === 0 ? 'success' : 'error'}>{nvme.media_errors}</Badge>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Unsafe Shutdowns</span>
                <span class="spec-val mono">{nvme.unsafe_shutdowns || 0}</span>
              </div>
            </div>
          </div>
        {/if}
      {:else}
        <EmptyState
          icon={HardDrive}
          title="No S.M.A.R.T data"
          description="Select a drive above and rescan, or ensure smartmontools is installed."
          actionLabel="Rescan drives"
          onAction={loadSmartDrives}
        />
      {/if}
    </div>

  {:else if activeTab === 'topology'}
    <!-- Hardware Bus Topology Tab — each card scrolls independently -->
    <div class="module-content-scroll topology-scroll-area">
      {#if loadingTopology}
        <div class="loading-state">
          <RefreshCw size={22} class="animate-spin-slow" />
          <span>Reading hardware buses…</span>
        </div>
      {:else}
        <div class="topology-grid">
          <!-- PCI Bus Card -->
          <div class="accordion-card topo-card">
            <div class="section-header">
              <div class="section-header-left">
                <Cpu size={15} style="color: var(--color-accent);" />
                <span class="section-title">PCI Express Bus</span>
              </div>
              <Badge variant="muted">{pciDevices.length} devices</Badge>
            </div>
            {#if pciDevices.length === 0}
              <div class="topo-empty">No PCI devices detected.</div>
            {:else}
              <div class="topo-list">
                {#each pciDevices as pci, i}
                  <div class="topo-row {i < pciDevices.length - 1 ? 'has-divider' : ''}">
                    <div class="topo-row-top">
                      <code class="topo-slot">{pci.slot}</code>
                      {#if pci.rev}
                        <Badge variant="muted" style="font-size: 9px;">{pci.rev}</Badge>
                      {/if}
                    </div>
                    <span class="topo-class">{pci.class}</span>
                    <span class="topo-vendor">{pci.vendor_device}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- USB Bus Card -->
          <div class="accordion-card topo-card">
            <div class="section-header">
              <div class="section-header-left">
                <Usb size={15} style="color: var(--color-accent);" />
                <span class="section-title">USB Connections</span>
              </div>
              <Badge variant="muted">{usbDevices.length} devices</Badge>
            </div>
            {#if usbDevices.length === 0}
              <div class="topo-empty">No USB devices detected.</div>
            {:else}
              {@const buses = [...new Set(usbDevices.map(u => u.bus))].sort()}
              <div class="topo-list">
                {#each buses as bus, bi}
                  <div class="usb-bus-group {bi < buses.length - 1 ? 'has-divider' : ''}">
                    <div class="usb-bus-label">
                      <Usb size={12} style="color: var(--color-accent);" />
                      <span>Bus {bus}</span>
                    </div>
                    {#each usbDevices.filter(u => u.bus === bus) as usb, ui}
                      {@const busDevices = usbDevices.filter(u => u.bus === bus)}
                      <div class="usb-device-row {ui < busDevices.length - 1 ? 'has-divider' : ''}">
                        <div class="usb-device-info">
                          <span class="device-name">{usb.name}</span>
                          <span class="usb-device-num">Device {usb.device}</span>
                        </div>
                        <code class="topo-slot">{usb.id}</code>
                      </div>
                    {/each}
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── Toolbar ─────────────────────────────────────────────── */
  .inventory-toolbar,
  .smart-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 8px;
  }

  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .toolbar-right {
    display: flex;
    gap: 8px;
  }

  .toolbar-label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
    white-space: nowrap;
  }

  /* ── Layouts ─────────────────────────────────────────────── */
  .inventory-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .smart-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .smart-summary-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 16px;
    flex-shrink: 0;
  }

  /* Topology: outer area is the scroll container */
  .topology-scroll-area {
    /* Give an explicit height so the grid's min-height:100% resolves against it */
    height: 100%;
  }

  .topology-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 16px;
    /* fill the scroll container so cards stretch even when content is short */
    min-height: 100%;
    align-items: stretch;
  }

  /* Cards fill the grid row height; the list scrolls inside */
  .topo-card {
    display: flex;
    flex-direction: column;
    /* no max-height — card fills available height naturally */
  }

  .topo-card .topo-list {
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  /* ── Accordion / Card ─────────────────────────────────────── */
  /* Elevation ladder: page canvas (bg-base) → body (bg-card) → title band (bg-raised) */
  .accordion-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    flex-shrink: 0;
    /* No overflow:hidden — would clip the device action dropdown */
  }

  /* Header is a distinct raised band so it reads as the TITLE, not as content */
  .accordion-header {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    background: var(--color-bg-raised);
    border: none;
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    font-family: var(--font-sans);
    transition: background 0.15s;
    gap: 8px;
    border-radius: 12px 12px 0 0;
    text-align: left;
  }

  /* When accordion is collapsed the header is also the bottom — round all corners */
  .accordion-header[aria-expanded="false"] {
    border-bottom: none;
    border-radius: 12px;
  }

  .accordion-header:hover {
    background: var(--color-bg-hover);
  }

  .accordion-header:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }

  .accordion-header-left {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
  }

  /* Accent chip anchors the eye on the title row */
  .cls-icon-chip {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--color-module-icon-bg);
    border: 1px solid var(--color-module-icon-border);
    flex-shrink: 0;
  }

  :global(.cls-icon) {
    color: var(--color-accent);
    flex-shrink: 0;
  }

  /* Uppercase micro-heading — never mistaken for the device names below it */
  .cls-label {
    font-weight: 700;
    font-size: 11.5px;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--color-text-primary);
    white-space: nowrap;
  }

  :global(.chevron) {
    color: var(--color-text-muted);
    transition: transform 0.2s ease, color 0.15s ease;
    flex-shrink: 0;
  }

  :global(.chevron.rotated) {
    transform: rotate(180deg);
    color: var(--color-accent);
  }

  .accordion-body {
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card);
  }

  /* ── Device rows ─────────────────────────────────────────── */
  .device-row {
    padding: 10px 16px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    position: relative;
    transition: background 0.12s;
  }

  /* Hover makes the content zone read as a scannable data region */
  .device-row:hover {
    background: var(--color-bg-hover);
  }

  .device-row.has-divider {
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .device-main {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .device-name {
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text-primary);
    min-width: 0;
    flex: 1;
  }

  .device-row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .device-vendor {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  /* ── Device ⋯ action menu ────────────────────────────────── */
  .device-menu-anchor {
    position: relative;
  }

  .action-dot-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.12s, background 0.12s, color 0.12s;
    flex-shrink: 0;
  }

  .device-row:hover .action-dot-btn,
  .action-dot-btn:focus-visible {
    opacity: 1;
  }

  .action-dot-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .device-dropdown {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    min-width: 190px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    z-index: 500;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12.5px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s, color 0.12s;
    width: 100%;
  }

  .dropdown-item:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .dropdown-item.warning {
    color: var(--color-warning);
  }

  .dropdown-item.warning:hover {
    background: rgba(245, 158, 11, 0.1);
    color: var(--color-warning);
  }

  .dropdown-divider {
    height: 1px;
    background: var(--color-border);
    margin: 3px 6px;
  }

  .dropdown-module-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .dropdown-module-badge code {
    color: var(--color-accent);
    font-size: 11px;
  }

  .dropdown-loading {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .device-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    font-size: 11.5px;
  }

  .meta-item {
    display: flex;
    gap: 5px;
    align-items: center;
  }

  .meta-label {
    color: var(--color-text-muted);
    font-size: 11px;
  }

  .meta-val {
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .meta-val.accent {
    color: var(--color-accent);
  }

  /* section-header (non-collapsible card header) also needs top rounding */
  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-raised, var(--color-bg-surface));
    flex-shrink: 0;
    border-radius: 12px 12px 0 0;
  }

  .section-header-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .section-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  /* ── S.M.A.R.T Health card ───────────────────────────────── */
  .smart-health-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 0;
  }

  .health-indicator {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 8px;
  }

  .health-ok {
    background: rgba(34, 197, 94, 0.08);
    color: var(--color-success);
  }

  .health-fail {
    background: rgba(239, 68, 68, 0.08);
    color: var(--color-error);
  }

  .health-text-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .health-verdict {
    font-size: 16px;
    font-weight: 800;
    letter-spacing: 0.04em;
    font-family: var(--font-mono);
  }

  .health-subtitle {
    font-size: 11.5px;
    opacity: 0.75;
  }

  .health-unknown {
    font-size: 12.5px;
    color: var(--color-text-muted);
    padding: 12px 0;
    text-align: center;
  }

  .smart-stat-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .smart-stat {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  :global(.stat-icon) {
    color: var(--color-accent);
    margin-top: 2px;
    flex-shrink: 0;
  }

  .stat-label {
    display: block;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.04em;
    margin-bottom: 2px;
  }

  .stat-value {
    display: block;
    font-size: 14px;
    font-weight: 700;
    font-family: var(--font-mono);
    color: var(--color-text-primary);
  }

  /* ── Spec list ───────────────────────────────────────────── */
  .spec-list {
    display: flex;
    flex-direction: column;
    font-size: 12.5px;
  }

  .spec-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0;
    gap: 8px;
  }

  .spec-label {
    color: var(--color-text-muted);
    flex-shrink: 0;
    font-size: 12.5px;
  }

  .spec-val {
    color: var(--color-text-primary);
    font-weight: 600;
    text-align: right;
    word-break: break-all;
  }

  .spec-val.mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
  }

  .spec-val.upper {
    text-transform: uppercase;
  }

  .nvme-grid {
    display: flex;
    flex-direction: column;
    padding: 8px 16px;
  }

  .nvme-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0;
    font-size: 12.5px;
  }

  /* ── Topology ────────────────────────────────────────────── */
  .topo-list {
    display: flex;
    flex-direction: column;
  }

  .topo-row {
    padding: 10px 16px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .topo-row.has-divider {
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .topo-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .topo-slot {
    font-size: 11px;
    color: var(--color-accent);
    font-family: var(--font-mono);
  }

  .topo-class {
    font-weight: 600;
    font-size: 12.5px;
    color: var(--color-text-primary);
  }

  .topo-vendor {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  .topo-empty {
    padding: 32px 16px;
    text-align: center;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }

  .usb-bus-group {
    padding: 12px 16px;
  }

  .usb-bus-group.has-divider {
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .usb-bus-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 700;
    color: var(--color-text-primary);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .usb-device-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 0;
  }

  .usb-device-row.has-divider {
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .usb-device-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .usb-device-num {
    font-size: 10.5px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  /* ── Shared ──────────────────────────────────────────────── */
  .divider {
    height: 1px;
    background: var(--color-border);
    opacity: 0.5;
  }

  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }

  .error-card {
    background: rgba(239, 68, 68, 0.04);
    border: 1px solid rgba(239, 68, 68, 0.25);
    border-radius: 12px;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .error-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-error);
    font-weight: 700;
    font-size: 13.5px;
  }

  .error-body {
    margin: 0;
    font-size: 12.5px;
    color: var(--color-text-secondary);
    line-height: 1.6;
    font-family: var(--font-mono);
    white-space: pre-wrap;
  }

  .error-hint {
    margin: 0;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .test-output {
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 14px 16px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    line-height: 1.6;
  }
</style>
