<script lang="ts">
  import { onDestroy } from 'svelte';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Select from '../components/ui/Select.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Table from '../components/ui/Table.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    HardDrive, Cpu, RefreshCw, Monitor, Network, MemoryStick, Usb, Settings,
    ShieldCheck, Thermometer, Clock, Database, Activity, Play, ShieldAlert,
    CheckCircle, AlertTriangle, Search, Copy, Unplug, RotateCcw, ChevronRight,
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

  // ── Device inventory ──────────────────────────────────────────────────────
  let allDevices = $state<any[]>([]);
  let loading = $state(true);
  let detailedLoading = $state(false);
  let listLoaded = $state(false);
  let searchQuery = $state('');
  let activeClass = $state('all');
  let lastScanned = $state<Date | null>(null);
  let listError = $state('');

  // Selected device + driver context for the details drawer.
  let selectedDevice = $state<any>(null);
  let deviceDrawerOpen = $state(false);
  let driverState = $state<{ loading: boolean; module: string; error: string }>({
    loading: false,
    module: '',
    error: '',
  });
  let driverRequest = 0;
  let driverBusyText = $state('');

  interface SmartDrive {
    name: string;
    info_name: string;
    type: string;
    protocol: string;
  }
  let smartDrives = $state<SmartDrive[]>([]);
  let smartDrivesLoaded = $state(false);
  let selectedDrive = $state<string>('');
  let loadingSmartDrives = $state(false);
  let smartData = $state<any>(null);
  let loadingSmartData = $state(false);
  let smartError = $state<string>('');
  let smartNotices = $state<string[]>([]);
  let smartCheckedAt = $state<Date | null>(null);
  let triggerMessage = $state<string>('');
  let testingDrive = $state(false);
  let selfTestTimer: ReturnType<typeof setInterval> | null = null;
  let selfTestPollBusy = false;
  let selfTestLive = $state(false);
  let selfTestPollUntil = 0;

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
  let topologyLoaded = $state(false);
  let topologyQuery = $state('');
  let lastTopologyScan = $state<Date | null>(null);
  let topologyError = $state('');

  const CLASS_LABELS: Record<string, string> = {
    processor: 'Processor',
    memory: 'Memory',
    storage: 'Storage',
    disk: 'Disk',
    volume: 'Volume',
    network: 'Network',
    display: 'Display',
    bridge: 'Bridge',
    communication: 'Communication',
    input: 'Input',
    multimedia: 'Multimedia',
    generic: 'Generic',
    usb: 'USB',
  };

  const CLASS_ORDER = [
    'processor', 'memory', 'storage', 'disk', 'volume', 'network', 'display',
    'bridge', 'communication', 'input', 'multimedia', 'generic', 'usb',
  ];

  function classRank(cls: string): number {
    const rank = CLASS_ORDER.indexOf(cls);
    return rank === -1 ? CLASS_ORDER.length : rank;
  }

  function classLabel(cls: string): string {
    if (!cls) return 'Unknown';
    return CLASS_LABELS[cls] ?? cls.replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
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
      case 'usb':
      case 'input': return Usb;
      default: return Settings;
    }
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

  function deviceKey(dev: any, index = -1): string {
    if (dev.handle) return String(dev.handle);
    const base = [dev.class, dev.id, dev.businfo, formatLogicalName(dev.logicalname), dev.product]
      .filter(Boolean)
      .join('|');
    return index >= 0 ? `${base}#${index}` : base;
  }

  function formatLogicalName(value: any): string {
    return Array.isArray(value) ? value.join(', ') : value ? String(value) : '';
  }

  function devicePrimary(dev: any): string {
    return dev.product || dev.description || dev.id || classLabel(dev.class);
  }

  function deviceSecondary(dev: any): string {
    if (dev.product && dev.description && dev.product !== dev.description) return dev.description;
    return '';
  }

  function deviceSearchBlob(dev: any): string {
    const config = dev.configuration && typeof dev.configuration === 'object'
      ? Object.entries(dev.configuration).flat().join(' ')
      : '';
    const caps = dev.capabilities
      ? Array.isArray(dev.capabilities)
        ? dev.capabilities.join(' ')
        : Object.entries(dev.capabilities).flat().join(' ')
      : '';
    return [
      dev.class, dev.id, dev.product, dev.vendor, dev.description, dev.businfo,
      dev.serial, formatLogicalName(dev.logicalname), config, caps,
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase();
  }

  function compareDevices(a: any, b: any): number {
    const byRank = classRank(a.class) - classRank(b.class);
    if (byRank !== 0) return byRank;
    const byClass = classLabel(a.class).localeCompare(classLabel(b.class));
    if (byClass !== 0) return byClass;
    return devicePrimary(a).localeCompare(devicePrimary(b));
  }

  let filteredDevices = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    return allDevices.filter(
      (dev) =>
        (activeClass === 'all' || dev.class === activeClass) &&
        (!q || deviceSearchBlob(dev).includes(q))
    );
  });

  let classOptions = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const dev of allDevices) {
      const cls = dev.class || 'unknown';
      counts.set(cls, (counts.get(cls) ?? 0) + 1);
    }
    return [
      { id: 'all', label: 'All', count: allDevices.length },
      ...[...counts.entries()]
        .sort(([a], [b]) => classRank(a) - classRank(b) || classLabel(a).localeCompare(classLabel(b)))
        .map(([id, count]) => ({ id, label: classLabel(id), count })),
    ];
  });

  $effect(() => {
    if (activeClass !== 'all' && !classOptions.some((option) => option.id === activeClass)) {
      activeClass = 'all';
    }
  });

  function applyInventory(raw: any) {
    allDevices = flattenLshw(raw).sort(compareDevices);
    lastScanned = new Date();
    listLoaded = true;

    // Keep an open drawer pointed at the same hardware after a rescan.
    if (selectedDevice) {
      const key = deviceKey(selectedDevice);
      const refreshed = allDevices.find((dev) => deviceKey(dev) === key) ?? null;
      if (refreshed) {
        selectedDevice = refreshed;
        const slot = pciSlot(refreshed);
        if (slot) void loadDriver(slot);
      } else {
        deviceDrawerOpen = false;
        selectedDevice = null;
      }
    }
  }

  async function loadData() {
    loading = true;
    listError = '';
    statusStore.setBusy('Scanning system devices…');
    try {
      const jsonStr: string = await invoke('device_get_all');
      applyInventory(JSON.parse(jsonStr));
      statusStore.setLastCommand('lshw -json', 0, true);
    } catch (e) {
      console.error(e);
      listError = String(e);
      statusStore.setLastCommand('lshw -json', 1, false);
      uiStore.addToast(`Failed to load system devices: ${e}`, 'error');
    } finally {
      loading = false;
      listLoaded = true;
      statusStore.clearBusy();
    }
  }

  async function loadDetailedData() {
    if (detailedLoading) return;
    detailedLoading = true;
    statusStore.setBusy('Reading a detailed hardware inventory…');
    try {
      const jsonStr: string = await invoke('device_get_all_detailed');
      applyInventory(JSON.parse(jsonStr));
      statusStore.setLastCommand('lshw -json', 0, true);
      uiStore.addToast('Detailed hardware inventory loaded', 'success');
    } catch (e) {
      console.error(e);
      if (allDevices.length === 0) listError = String(e);
      statusStore.setLastCommand('lshw -json', 1, false);
      uiStore.addToast(`Detailed scan failed: ${e}`, 'error');
    } finally {
      detailedLoading = false;
      listLoaded = true;
      statusStore.clearBusy();
    }
  }

  function openDevice(dev: any) {
    selectedDevice = dev;
    deviceDrawerOpen = true;
    driverRequest += 1;
    driverState = { loading: false, module: '', error: '' };
    const slot = pciSlot(dev);
    if (slot) void loadDriver(slot);
  }

  function pciSlot(dev: any): string {
    const bus = String(dev?.businfo || '');
    return bus.startsWith('pci@') ? bus.slice(4) : '';
  }

  async function loadDriver(slot: string) {
    const request = ++driverRequest;
    driverBusyText = 'Looking up the bound driver…';
    driverState = { loading: true, module: '', error: '' };
    try {
      const moduleName: string = await invoke('device_get_pci_driver', { slot });
      if (request === driverRequest) {
        driverState = { loading: false, module: moduleName, error: '' };
      }
    } catch (e) {
      if (request === driverRequest) {
        driverState = { loading: false, module: '', error: String(e) };
      }
    }
  }

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast(label, 'info');
    } catch (e) {
      uiStore.addToast(`Could not copy: ${e}`, 'error');
    }
  }

  function copyDeviceInfo(dev: any) {
    const lines = [
      `Name: ${devicePrimary(dev)}`,
      dev.vendor ? `Vendor: ${dev.vendor}` : null,
      dev.class ? `Class: ${classLabel(dev.class)}` : null,
      dev.businfo ? `Bus info: ${dev.businfo}` : null,
      dev.logicalname ? `Logical: ${formatLogicalName(dev.logicalname)}` : null,
      dev.serial ? `Serial: ${dev.serial}` : null,
      dev.size || dev.capacity ? `Capacity: ${formatSize(dev.size || dev.capacity)}` : null,
      dev.clock ? `Clock: ${formatClock(dev.clock)}` : null,
      dev.width ? `Bus width: ${dev.width} bits` : null,
      driverState.module ? `Driver: ${driverState.module}` : null,
    ].filter(Boolean).join('\n');
    void copyText(lines, 'Device info copied to clipboard');
  }

  function confirmModuleAction(dev: any, action: 'unload' | 'reload') {
    const moduleName = driverState.module;
    if (!moduleName) return;

    const isUnload = action === 'unload';
    const slot = pciSlot(dev);
    uiStore.confirm(
      isUnload ? `Unload kernel module "${moduleName}"?` : `Reload kernel module "${moduleName}"?`,
      isUnload
        ? `This will unload the "${moduleName}" kernel module from the running system.\n\nThe device "${devicePrimary(dev)}" may stop working until the module is reloaded or the system is restarted.\n\nDo NOT unload modules for critical devices (storage, GPU, USB hubs).`
        : `This will reload the "${moduleName}" kernel module.\n\nThe device will briefly disconnect and reconnect.`,
      async () => {
        driverBusyText = isUnload ? 'Unloading the module…' : 'Reloading the module…';
        driverState = { ...driverState, loading: true };
        try {
          const result: string = await invoke('device_set_module_state', {
            module: moduleName,
            action,
          });
          uiStore.addToast(result, 'success');
          if (action === 'reload') {
            // udev needs a moment to bind the driver again after modprobe.
            await new Promise((resolve) => setTimeout(resolve, 600));
          }
          if (slot) await loadDriver(slot);
        } catch (e) {
          uiStore.addToast(`Failed: ${e}`, 'error');
        } finally {
          driverState = { ...driverState, loading: false };
        }
      },
      isUnload
    );
  }

  function resetPciDriver(dev: any, slot: string) {
    if (!slot) return;
    uiStore.confirm(
      `Reset the driver for "${devicePrimary(dev)}"?`,
      'This unbinds and rebinds the PCI driver for this device. The device disconnects briefly and reconnects; no kernel module is unloaded.',
      async () => {
        driverBusyText = 'Resetting the driver…';
        driverState = { ...driverState, loading: true };
        try {
          const result: string = await invoke('device_reset_pci_driver', { slot });
          uiStore.addToast(result, 'success');
          await new Promise((resolve) => setTimeout(resolve, 600));
          await loadDriver(slot);
        } catch (e) {
          uiStore.addToast(`Failed to reset the driver: ${e}`, 'error');
        } finally {
          driverState = { ...driverState, loading: false };
        }
      },
      false
    );
  }

  let selectedPrimary = $derived(selectedDevice ? devicePrimary(selectedDevice) : '');
  let selectedSecondary = $derived(selectedDevice ? deviceSecondary(selectedDevice) : '');
  let selectedPciSlot = $derived(selectedDevice ? pciSlot(selectedDevice) : '');
  let selectedResetBlock = $derived(selectedDevice ? resetBlockedReason(selectedDevice) : '');
  let selectedDetails = $derived.by(() => {
    const dev = selectedDevice;
    if (!dev) return [] as Array<[string, string]>;
    const rows: Array<[string, string]> = [];
    const add = (label: string, value: unknown) => {
      if (value === undefined || value === null || value === '') return;
      rows.push([label, String(value)]);
    };
    add('Vendor', dev.vendor);
    add('Class', classLabel(dev.class));
    add('Device ID', dev.id);
    add('Bus address', dev.businfo);
    add('Logical name', formatLogicalName(dev.logicalname));
    add('Serial number', dev.serial);
    if (dev.size || dev.capacity) add('Capacity', formatSize(dev.size || dev.capacity));
    if (dev.clock) add('Clock', formatClock(dev.clock));
    if (dev.width) add('Bus width', `${dev.width} bits`);
    return rows;
  });
  let selectedCapabilities = $derived.by(() => {
    const caps = selectedDevice?.capabilities;
    if (!caps) return [] as string[];
    if (Array.isArray(caps)) return caps.map(String);
    return Object.entries(caps)
      .filter(([, value]) => value)
      .map(([key]) => key.replace(/_/g, ' '));
  });
  let selectedConfig = $derived.by(() => {
    const config = selectedDevice?.configuration;
    if (!config || typeof config !== 'object') return [] as Array<[string, string]>;
    return Object.entries(config)
      .filter(([key, value]) => key !== 'driver' && value !== '' && value !== null && value !== undefined)
      .map(([key, value]) => [
        key.replace(/_/g, ' '),
        Array.isArray(value) ? value.join(', ') : String(value),
      ] as [string, string]);
  });

  function formatSize(bytes: number) {
    if (!bytes || bytes <= 0) return '—';
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.min(
      sizes.length - 1,
      Math.floor(Math.log(bytes) / Math.log(1024))
    );
    return `${Math.round((bytes / Math.pow(1024, i)) * 100) / 100} ${sizes[i]}`;
  }

  function formatClock(hz: number) {
    if (!hz || hz <= 0) return '—';
    const units = ['Hz', 'kHz', 'MHz', 'GHz', 'THz'];
    let value = hz;
    let unit = 0;
    while (value >= 1000 && unit < units.length - 1) {
      value /= 1000;
      unit += 1;
    }
    return `${Number(value.toFixed(value >= 100 ? 0 : 2))} ${units[unit]}`;
  }

  function deviceSpec(dev: any): string {
    const capacity = dev.size || dev.capacity;
    if (capacity) return formatSize(capacity);
    if (dev.clock) return formatClock(dev.clock);
    if (dev.width) return `${dev.width}-bit`;
    return '—';
  }

  function deviceLocation(dev: any): string {
    // Interface and block-device names are what users recognise; fall back to
    // the bus address for devices without a logical name.
    return formatLogicalName(dev.logicalname) || dev.businfo || '—';
  }

  function resetBlockedReason(dev: any): string {
    if (!dev) return 'No device selected.';
    if (['storage', 'disk', 'volume'].includes(dev.class)) {
      return 'Reset is disabled for storage controllers; it can take the system down.';
    }
    if (dev.class === 'display') {
      return 'Reset is disabled for display controllers.';
    }
    if (dev.class === 'bridge') {
      return 'Reset is disabled for bridge devices.';
    }
    return '';
  }

  // ── S.M.A.R.T data ───────────────────────────────────────────────────────
  interface HealthIndicator {
    label: string;
    value: string;
    tone: 'error' | 'warning' | 'success';
  }

  const HEALTH_WATCH: Array<{ id: number; label: string; tone: 'error' | 'warning' }> = [
    { id: 5, label: 'Reallocated sectors', tone: 'error' },
    { id: 187, label: 'Reported uncorrectable', tone: 'error' },
    { id: 197, label: 'Pending sectors', tone: 'error' },
    { id: 198, label: 'Offline uncorrectable', tone: 'error' },
    { id: 199, label: 'UDMA CRC errors', tone: 'warning' },
  ];

  function rawAttributeValue(attr: any): number {
    const raw = attr?.raw?.value ?? attr?.raw_string ?? attr?.raw?.string;
    if (typeof raw === 'number') return raw;
    const match = String(raw ?? '').match(/-?\d+(\.\d+)?/);
    return match ? Number(match[0]) : 0;
  }

  let smartIndicators = $derived.by<HealthIndicator[]>(() => {
    const dev = smartData;
    if (!dev) return [];
    const attrs: any[] = dev.ata_smart_attributes?.table ?? [];
    const indicators = new Map<string, HealthIndicator>();

    if (dev.smart_status && dev.smart_status.passed === false) {
      indicators.set('Overall SMART status', {
        label: 'Overall SMART status',
        value: 'FAILED',
        tone: 'error',
      });
    }

    for (const watch of HEALTH_WATCH) {
      const attr = attrs.find((entry) => Number(entry.id) === watch.id);
      if (!attr) continue;
      const value = rawAttributeValue(attr);
      if (value > 0) {
        indicators.set(watch.label, {
          label: watch.label,
          value: String(value),
          tone: watch.tone,
        });
      }
    }

    for (const attr of attrs) {
      if (attr.when_failed) {
        indicators.set(String(attr.name || attr.id), {
          label: attr.name || `Attribute ${attr.id}`,
          value: String(attr.when_failed),
          tone: 'error',
        });
      }
    }

    const nvme = dev.nvme_smart_health_information_log;
    if (nvme) {
      if (Number(nvme.critical_warning) > 0) {
        indicators.set('NVMe critical warning', {
          label: 'NVMe critical warning',
          value: `0x${Number(nvme.critical_warning).toString(16)}`,
          tone: 'error',
        });
      }
      if (Number(nvme.media_errors) > 0) {
        indicators.set('NVMe media errors', {
          label: 'NVMe media errors',
          value: String(nvme.media_errors),
          tone: 'error',
        });
      }
      const wear = Number(nvme.percentage_used ?? 0);
      if (wear >= 80) {
        indicators.set('NVMe wear', {
          label: 'NVMe wear',
          value: `${wear}% used`,
          tone: wear >= 90 ? 'error' : 'warning',
        });
      }
    }

    return [...indicators.values()];
  });

  interface SelfTestView {
    running: boolean;
    text: string;
    type: string;
    remaining: number | null;
    entries: any[];
  }

  function readSelfTest(data: any): SelfTestView {
    const log = data?.ata_smart_self_test_log ?? data?.scsi_self_test_log ?? data?.nvme_self_test_log;
    const entries: any[] = Array.isArray(log?.table)
      ? log.table
      : Array.isArray(log?.standard)
        ? log.standard
        : [];
    const latest = entries[0] ?? null;
    const status = latest?.status;
    const text = typeof status === 'string' ? status : status?.string ?? '';
    const remaining =
      typeof status?.remaining_percent === 'number' ? status.remaining_percent : null;
    return {
      running: /in progress/i.test(text),
      text,
      type: latest?.type?.string ?? latest?.self_test_type ?? '',
      remaining,
      entries,
    };
  }

  let selfTest = $derived.by(() => readSelfTest(smartData));

  async function loadSmartDrives() {
    loadingSmartDrives = true;
    stopSelfTestPolling();
    try {
      const scanStr: string = await invoke('device_get_smart_drives');
      const scan = JSON.parse(scanStr);
      smartDrives = scan.devices || [];
      if (smartDrives.length > 0) {
        if (!selectedDrive || !smartDrives.some((d) => d.name === selectedDrive)) {
          selectedDrive = smartDrives[0].name;
        }
        await loadSmartData(selectedDrive);
      }
    } catch (e) {
      console.error(e);
      smartError = String(e);
      smartData = null;
      smartNotices = [];
      uiStore.addToast(`Failed to scan SMART drives: ${e}`, 'error');
    } finally {
      loadingSmartDrives = false;
      smartDrivesLoaded = true;
    }
  }

  async function loadSmartData(dev: string, silent = false) {
    if (!dev) return;
    if (!silent) {
      stopSelfTestPolling();
      loadingSmartData = true;
      smartError = '';
      smartNotices = [];
      smartData = null;
    }
    try {
      const drive = smartDrives.find((entry) => entry.name === dev);
      const deviceType = drive?.type?.trim() || null;
      const smartStr: string = await invoke('device_get_smart_data', {
        device: dev,
        deviceType,
      });
      const parsed = JSON.parse(smartStr);
      smartData = parsed;
      smartCheckedAt = new Date();
      smartNotices = (parsed.smartctl?.messages ?? [])
        .filter((message: any) => message.severity === 'error')
        .map((message: any) => String(message.string ?? ''))
        .filter(Boolean);
      smartError = '';
      if (readSelfTest(parsed).running) startSelfTestPolling();
    } catch (e) {
      console.error(e);
      if (!silent) {
        smartError = String(e);
        smartData = null;
        smartNotices = [];
      }
      stopSelfTestPolling();
    } finally {
      if (!silent) loadingSmartData = false;
    }
  }

  let smartErrorHint = $derived.by(() => {
    const message = smartError.toLowerCase();
    if (message.includes('admin mode')) {
      return 'Turn on Admin Mode in the status bar, then try again.';
    }
    if (message.includes('does not support') && message.includes('helper')) {
      return 'The installed administrator helper is older than this app. Reinstall the current helper (or the Control Panel package).';
    }
    if (message.includes('administrator helper')) {
      return 'The administrator helper is missing. Reinstall Control Panel (or its lcp-admin package), then try again.';
    }
    if (message.includes('install') || message.includes('not found') || message.includes('could not be started')) {
      return 'Install the smartmontools package, then rescan the drives.';
    }
    if (message.includes('authoriz') || message.includes('permission') || message.includes('polkit')) {
      return 'Approve the administrator request, then try again.';
    }
    if (message.includes('unsupported') || message.includes('unknown usb') || message.includes('not supported')) {
      return 'This drive or adapter does not expose S.M.A.R.T data.';
    }
    return 'Check the drive connection and that smartmontools is installed.';
  });

  async function triggerSelfTest(testType: 'short' | 'long') {
    if (!selectedDrive) return;
    testingDrive = true;
    triggerMessage = '';
    try {
      const output: string = await invoke('device_trigger_self_test', {
        device: selectedDrive,
        testType,
      });
      const label = testType === 'short' ? 'Short' : 'Long';
      triggerMessage = output.trim() || `${label} self-test started. The result will appear below when the drive finishes.`;
      uiStore.addToast(`S.M.A.R.T self-test initiated: ${testType}`, 'success');
      selfTestPollUntil = Date.now() + 5 * 60 * 1000;
      setTimeout(() => {
        if (selectedDrive) void loadSmartData(selectedDrive, true);
      }, 2500);
      startSelfTestPolling();
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Failed to trigger self-test: ${e}`, 'error');
    } finally {
      testingDrive = false;
    }
  }

  function startSelfTestPolling() {
    if (selfTestTimer) return;
    if (Date.now() >= selfTestPollUntil) {
      // Long self-tests would otherwise ask for administrator rights again
      // every five minutes. Poll for a bounded window; Refresh data resumes it.
      selfTestPollUntil = Date.now() + 5 * 60 * 1000;
    }
    selfTestLive = true;
    selfTestTimer = setInterval(async () => {
      if (Date.now() > selfTestPollUntil) {
        stopSelfTestPolling();
        return;
      }
      if (!selectedDrive || selfTestPollBusy) return;
      selfTestPollBusy = true;
      try {
        await loadSmartData(selectedDrive, true);
      } finally {
        selfTestPollBusy = false;
      }
      const view = readSelfTest(smartData);
      if (!view.running) {
        stopSelfTestPolling();
        if (triggerMessage && view.text) {
          triggerMessage = `${view.type || 'Self-test'}: ${view.text}`;
        }
      }
    }, 15000);
  }

  function stopSelfTestPolling() {
    if (selfTestTimer) {
      clearInterval(selfTestTimer);
      selfTestTimer = null;
    }
    selfTestLive = false;
  }

  onDestroy(stopSelfTestPolling);

  function formatNvmeUnits(units: unknown): string {
    const value = Number(units);
    if (!Number.isFinite(value) || value <= 0) return '—';
    // NVMe counts data units in 512,000-byte increments.
    return formatSize(value * 512000);
  }

  // ── Bus topology ─────────────────────────────────────────────────────────
  async function loadTopology() {
    loadingTopology = true;
    topologyError = '';
    try {
      const res: any = await invoke('device_get_topology');
      pciDevices = res.pci_devices || [];
      usbDevices = res.usb_devices || [];
      lastTopologyScan = new Date();
    } catch (e) {
      console.error(e);
      topologyError = String(e);
      uiStore.addToast(`Failed to load hardware topology: ${e}`, 'error');
    } finally {
      loadingTopology = false;
      topologyLoaded = true;
    }
  }

  let filteredPci = $derived.by(() => {
    const q = topologyQuery.trim().toLowerCase();
    if (!q) return pciDevices;
    return pciDevices.filter((pci) =>
      [pci.slot, pci.class, pci.vendor_device, pci.rev]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q)
    );
  });

  let usbGroups = $derived.by(() => {
    const q = topologyQuery.trim().toLowerCase();
    const visible = q
      ? usbDevices.filter((usb) =>
          [usb.bus, usb.device, usb.id, usb.name]
            .filter(Boolean)
            .join(' ')
            .toLowerCase()
            .includes(q)
        )
      : usbDevices;
    const groups = new Map<string, UsbDevice[]>();
    for (const usb of visible) {
      const list = groups.get(usb.bus) ?? [];
      list.push(usb);
      groups.set(usb.bus, list);
    }
    return [...groups.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([bus, devices]) => ({ bus, devices }));
  });

  let visibleUsbCount = $derived(usbGroups.reduce((total, group) => total + group.devices.length, 0));

  let topologyErrorHint = $derived.by(() => {
    const message = topologyError.toLowerCase();
    if (message.includes('lspci') || message.includes('pciutils')) {
      return 'Install the pciutils package to read the PCI bus.';
    }
    if (message.includes('lsusb') || message.includes('usbutils')) {
      return 'Install the usbutils package to read USB devices.';
    }
    return 'Check that pciutils and usbutils are installed, then rescan.';
  });

  $effect(() => {
    if (activeTab === 'list' && !listLoaded) {
      loadData();
    } else if (activeTab === 'smart' && !smartDrivesLoaded) {
      loadSmartDrives();
    } else if (activeTab === 'topology' && !topologyLoaded) {
      loadTopology();
    }
  });
</script>

<div class="module-page">
  <PageHeader title="Device Manager" icon={HardDrive} description="Hardware inventory, disk diagnostics & motherboard topology">
    <TabGroup size="sm"
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
          placeholder="Search name, vendor, bus, serial…"
          count={searchQuery || activeClass !== 'all' ? filteredDevices.length : undefined}
          style="width: 300px; max-width: 100%;"
        />
        <span class="toolbar-count">
          {filteredDevices.length === allDevices.length
            ? `${allDevices.length} device${allDevices.length === 1 ? '' : 's'}`
            : `${filteredDevices.length} of ${allDevices.length}`}
        </span>
      </div>
      <div class="toolbar-right">
        {#if lastScanned}
          <span class="scan-time">
            Scanned {lastScanned.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        {/if}
        <Button
          variant="ghost"
          size="sm"
          onclick={loadDetailedData}
          disabled={detailedLoading || loading}
          title="Read serial numbers and capabilities with administrator rights"
        >
          {#if detailedLoading}
            <RefreshCw size={12} class="animate-spin-slow" />
            Reading…
          {:else}
            <ShieldCheck size={12} />
            Detailed scan
          {/if}
        </Button>
        <button
          class="header-refresh-btn"
          onclick={loadData}
          disabled={loading || detailedLoading}
          title="Rescan hardware"
        >
          <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} />
          Rescan
        </button>
      </div>
    </div>

    {#if !loading && classOptions.length > 1}
      <div class="inventory-filters">
        <FilterChips
          options={classOptions}
          bind:value={activeClass}
          label="Filter devices by class"
        />
      </div>
    {/if}

    <div class="module-content-scroll inventory-scroll">
      {#if loading}
        <div class="loading-state">
          <RefreshCw size={22} class="animate-spin-slow" />
          <span>Scanning hardware…</span>
        </div>
      {:else if allDevices.length === 0}
        <EmptyState
          icon={Search}
          title={listError ? 'Hardware scan failed' : 'No devices detected'}
          description={listError || 'The hardware inventory returned no devices. Try rescanning, or check that lshw is installed.'}
          actionLabel="Rescan"
          onAction={loadData}
        />
      {:else if filteredDevices.length === 0}
        <EmptyState
          icon={Search}
          title="No matching devices"
          description="No hardware matches the current search and class filter."
          actionLabel="Clear filters"
          onAction={() => { searchQuery = ''; activeClass = 'all'; }}
        />
      {:else}
        <Table class="inventory-table" showDensityToggle={false}>
          <thead>
            <tr>
              <th style="width:36%">Device</th>
              <th style="width:17%">Vendor</th>
              <th style="width:11%">Class</th>
              <th style="width:20%">Bus / logical</th>
              <th style="width:12%" title="Capacity, clock speed, or bus width">Size / speed</th>
              <th style="width:36px"></th>
            </tr>
          </thead>
          <tbody>
            {#each filteredDevices as dev, index (deviceKey(dev, index))}
              {@const Icon = getIconForClass(dev.class)}
              <tr class="device-table-row">
                <td>
                  <button
                    type="button"
                    class="device-open-btn"
                    onclick={() => openDevice(dev)}
                    title="View {devicePrimary(dev)} details"
                  >
                    <span class="device-table-icon"><Icon size={14} /></span>
                    <span class="device-table-text">
                      <span class="device-table-name">{devicePrimary(dev)}</span>
                      {#if deviceSecondary(dev)}
                        <span class="device-table-sub">{deviceSecondary(dev)}</span>
                      {/if}
                    </span>
                  </button>
                </td>
                <td class="device-vendor-cell" title={dev.vendor || ''}>{dev.vendor || '—'}</td>
                <td><Badge variant="muted">{classLabel(dev.class)}</Badge></td>
                <td
                  class="device-mono-cell"
                  title={deviceLocation(dev)}
                >
                  {deviceLocation(dev)}
                </td>
                <td class="device-mono-cell">{deviceSpec(dev)}</td>
                <td><ChevronRight size={14} class="device-row-arrow" /></td>
              </tr>
            {/each}
          </tbody>
        </Table>
      {/if}
    </div>

    <SideDrawer
      bind:isOpen={deviceDrawerOpen}
      title={selectedPrimary}
      subtitle={selectedSecondary}
      width="540px"
    >
      <div class="device-drawer">
        <div class="drawer-section">
          <span class="drawer-section-title">Hardware</span>
          <div class="detail-grid">
            {#each selectedDetails as [label, value] (label)}
              <div class="detail-row">
                <span class="detail-label">{label}</span>
                <span
                  class="detail-value"
                  class:mono={label === 'Bus address' || label === 'Logical name' || label === 'Serial number'}
                >{value}</span>
              </div>
            {/each}
          </div>
        </div>

        {#if selectedDevice?.businfo?.startsWith('pci@')}
          <div class="drawer-section">
            <span class="drawer-section-title">Kernel driver</span>
            {#if driverState.loading}
              <div class="drawer-note">
                <RefreshCw size={12} class="animate-spin-slow" />
                {driverBusyText || 'Looking up the bound driver…'}
              </div>
            {:else if driverState.module}
              <div class="driver-box">
                <div class="driver-name">
                  <span class="detail-label">Module</span>
                  <code>{driverState.module}</code>
                </div>
                {#if selectedResetBlock}
                  <p class="drawer-hint">{selectedResetBlock}</p>
                {:else}
                  <div class="drawer-actions">
                    <Button
                      variant="outline"
                      size="sm"
                      onclick={() => resetPciDriver(selectedDevice, selectedPciSlot)}
                    >
                      <RotateCcw size={13} />
                      Reset driver
                    </Button>
                    <Button
                      variant="danger"
                      size="sm"
                      onclick={() => confirmModuleAction(selectedDevice, 'unload')}
                    >
                      <Unplug size={13} />
                      Unload
                    </Button>
                  </div>
                  <p class="drawer-hint">
                    Reset reinitializes the bound driver. Unloading the module is only possible
                    when it is not in use; detach the device first.
                  </p>
                {/if}
              </div>
            {:else}
              <div class="drawer-note" title={driverState.error}>No kernel driver bound.</div>
            {/if}
          </div>
        {/if}

        {#if selectedCapabilities.length > 0}
          <div class="drawer-section">
            <span class="drawer-section-title">Capabilities</span>
            <div class="capability-chips">
              {#each selectedCapabilities as capability (capability)}
                <Badge variant="muted">{capability}</Badge>
              {/each}
            </div>
          </div>
        {/if}

        {#if selectedConfig.length > 0}
          <div class="drawer-section">
            <span class="drawer-section-title">Configuration</span>
            <div class="detail-grid">
              {#each selectedConfig as [label, value] (label)}
                <div class="detail-row">
                  <span class="detail-label">{label}</span>
                  <span class="detail-value mono">{value}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div class="drawer-footer">
          <Button
            variant="primary"
            size="sm"
            onclick={() => selectedDevice && copyDeviceInfo(selectedDevice)}
          >
            <Copy size={13} />
            Copy device info
          </Button>
        </div>
      </div>
    </SideDrawer>

  {:else if activeTab === 'smart'}
    <!-- S.M.A.R.T Disk Diagnostics Tab -->
    <div class="smart-toolbar">
      <div class="toolbar-left">
        <span class="toolbar-label">Drive</span>
        <div style="width: 240px;">
          <Select
            bind:value={selectedDrive}
            size="sm"
            onchange={(e: any) => {
              stopSelfTestPolling();
              triggerMessage = '';
              void loadSmartData(e.target.value);
            }}
            disabled={loadingSmartDrives}
          >
            {#each smartDrives as drv}
              <option value={drv.name}>{drv.name}{drv.protocol ? ` [${drv.protocol}]` : ''}</option>
            {/each}
          </Select>
        </div>
        <button class="header-refresh-btn" onclick={loadSmartDrives} disabled={loadingSmartDrives}>
          <RefreshCw size={13} class={loadingSmartDrives ? 'animate-spin-slow' : ''} />
          Rescan drives
        </button>
        <Button
          variant="ghost"
          size="sm"
          onclick={() => selectedDrive && loadSmartData(selectedDrive)}
          disabled={!selectedDrive || loadingSmartData || loadingSmartDrives}
          title="Reload the selected drive's S.M.A.R.T data"
        >
          <RefreshCw size={12} />
          Refresh data
        </Button>
        {#if smartCheckedAt}
          <span class="scan-time">
            Updated {smartCheckedAt.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        {/if}
      </div>

      {#if selectedDrive}
        <div class="toolbar-right">
          <Button
            variant="outline"
            size="sm"
            onclick={() => triggerSelfTest('short')}
            disabled={testingDrive || loadingSmartData || selfTest.running}
          >
            <Play size={12} />
            Short Self-Test
          </Button>
          <Button
            variant="outline"
            size="sm"
            onclick={() => triggerSelfTest('long')}
            disabled={testingDrive || loadingSmartData || selfTest.running}
          >
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
          <span>{loadingSmartDrives ? 'Scanning for drives…' : 'Loading S.M.A.R.T data…'}</span>
        </div>
      {:else if smartError}
        <div class="error-card">
          <div class="error-header">
            <ShieldAlert size={18} />
            <span>Failed to retrieve S.M.A.R.T data</span>
          </div>
          <p class="error-body">{smartError}</p>
          <p class="error-hint">{smartErrorHint}</p>
          <div class="error-actions">
            <Button variant="outline" size="sm" onclick={loadSmartDrives}>
              <RefreshCw size={12} />
              Rescan drives
            </Button>
          </div>
        </div>
      {:else if smartData}
        {#if smartNotices.length > 0}
          <div class="smart-notice">
            <AlertTriangle size={15} />
            <div class="smart-notice-body">
              {#each smartNotices as notice}
                <span>{notice}</span>
              {/each}
            </div>
          </div>
        {/if}

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
                    <span class="stat-value">{smartData.temperature?.current != null ? `${smartData.temperature.current} °C` : '—'}</span>
                  </div>
                </div>
                <div class="smart-stat">
                  <Clock size={14} class="stat-icon" />
                  <div>
                    <span class="stat-label">Power-On Hours</span>
                    <span class="stat-value">{smartData.power_on_time?.hours != null ? `${smartData.power_on_time.hours} h` : '—'}</span>
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

        <!-- Pre-fail indicators -->
        <div class="accordion-card">
          <div class="section-header">
            <div class="section-header-left">
              {#if smartIndicators.length > 0}
                <AlertTriangle size={15} style="color: var(--color-warning);" />
              {:else}
                <CheckCircle size={15} style="color: var(--color-success);" />
              {/if}
              <span class="section-title">Health indicators</span>
            </div>
            <Badge
              variant={smartIndicators.some((i) => i.tone === 'error')
                ? 'error'
                : smartIndicators.length > 0
                  ? 'warning'
                  : 'success'}
            >
              {smartIndicators.length > 0
                ? `${smartIndicators.length} issue${smartIndicators.length === 1 ? '' : 's'}`
                : 'No issues'}
            </Badge>
          </div>
          {#if smartIndicators.length > 0}
            <div class="indicator-grid">
              {#each smartIndicators as indicator (indicator.label)}
                <div class="indicator-item {indicator.tone}">
                  <span class="indicator-label">{indicator.label}</span>
                  <span class="indicator-value mono">{indicator.value}</span>
                </div>
              {/each}
            </div>
          {:else}
            <div class="indicator-ok">
              <CheckCircle size={14} />
              No pre-fail attributes or media errors are currently reported.
            </div>
          {/if}
        </div>

        {#if selfTest.running}
          <div class="selftest-progress">
            <div class="selftest-head">
              <RefreshCw size={14} class="animate-spin-slow" />
              <strong>{selfTest.type || 'Self-test'} in progress</strong>
              <span class="selftest-status">{selfTest.text}</span>
            </div>
            {#if selfTest.remaining !== null}
              <div
                class="progress-track"
                role="progressbar"
                aria-label="{selfTest.type || 'Self-test'} progress"
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow={Math.max(0, 100 - selfTest.remaining)}
              >
                <span style="width: {Math.max(2, 100 - selfTest.remaining)}%"></span>
              </div>
              <span class="selftest-remaining">{selfTest.remaining}% remaining</span>
            {/if}
            {#if !selfTestLive}
              <span class="selftest-paused">
                Live progress paused to avoid repeated administrator prompts. Use Refresh data to update.
              </span>
            {/if}
          </div>
        {:else if triggerMessage}
          <div class="test-output">{triggerMessage}</div>
        {/if}

        <!-- ATA SMART Attributes Table -->
        {#if smartData.ata_smart_attributes?.table}
          <div class="accordion-card" style="margin-top: 0;">
            <div class="section-header">
              <span class="section-title">S.M.A.R.T raw attributes</span>
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
          {@const wear = Math.min(100, Math.max(0, Number(nvme.percentage_used) || 0))}
          <div class="accordion-card" style="margin-top: 0;">
            <div class="section-header">
              <span class="section-title">NVMe SMART / health log</span>
            </div>
            <div class="nvme-grid">
              <div class="nvme-row">
                <span class="spec-label">Critical Warning</span>
                <Badge variant={Number(nvme.critical_warning) === 0 ? 'success' : 'error'}>{nvme.critical_warning ?? '—'}</Badge>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Wear (% used)</span>
                <span class="spec-val mono">{wear}%</span>
              </div>
              <div
                class="wear-track"
                title="{wear}% of the drive's rated endurance used"
                role="progressbar"
                aria-label="NVMe endurance used"
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow={wear}
              >
                <span
                  class:wear-high={wear >= 80}
                  style="width: {wear}%"
                ></span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Data Read</span>
                <span class="spec-val mono">{formatNvmeUnits(nvme.data_units_read)}</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Data Written</span>
                <span class="spec-val mono">{formatNvmeUnits(nvme.data_units_written)}</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Media Errors</span>
                <Badge variant={Number(nvme.media_errors) === 0 ? 'success' : 'error'}>{nvme.media_errors ?? 0}</Badge>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Unsafe Shutdowns</span>
                <span class="spec-val mono">{nvme.unsafe_shutdowns ?? 0}</span>
              </div>
              <div class="divider"></div>
              <div class="nvme-row">
                <span class="spec-label">Power Cycles</span>
                <span class="spec-val mono">{nvme.power_cycles ?? '—'}</span>
              </div>
            </div>
          </div>
        {/if}

        {#if selfTest.entries.length > 0}
          <div class="accordion-card">
            <div class="section-header">
              <span class="section-title">Self-test log</span>
              <Badge variant="muted">{selfTest.entries.length}</Badge>
            </div>
            <div class="table-wrap" style="border: none; border-radius: 0; max-height: 260px; overflow-y: auto;">
              <table>
                <thead>
                  <tr>
                    <th>Test</th>
                    <th>Result</th>
                    <th style="width: 110px; text-align: right;">Lifetime hours</th>
                    <th style="width: 90px; text-align: right;">LBA</th>
                  </tr>
                </thead>
                <tbody>
                  {#each selfTest.entries.slice(0, 10) as entry, index (index)}
                    <tr>
                      <td>{entry.type?.string ?? entry.self_test_type ?? '—'}</td>
                      <td>{entry.status?.string ?? entry.self_test_result ?? '—'}</td>
                      <td style="text-align: right; font-family: var(--font-mono);">{entry.lifetime_hours ?? '—'}</td>
                      <td style="text-align: right; font-family: var(--font-mono);">{entry.LBA_first_error ?? entry.lba ?? '—'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        {/if}

        {#if smartData.ata_smart_error_log?.table?.length}
          <div class="accordion-card">
            <div class="section-header">
              <div class="section-header-left">
                <AlertTriangle size={14} style="color: var(--color-warning);" />
                <span class="section-title">Drive error log</span>
              </div>
              <Badge variant="warning">{smartData.ata_smart_error_log.summary?.count ?? smartData.ata_smart_error_log.table.length}</Badge>
            </div>
            <div class="error-log-list">
              {#each smartData.ata_smart_error_log.table.slice(0, 5) as entry, index (index)}
                <div class="error-log-row">
                  <span class="error-log-index mono">#{entry.error_number ?? index + 1}</span>
                  <span class="error-log-desc">{entry.error_description ?? entry.status?.string ?? 'Error logged'}</span>
                  <span class="error-log-hours mono">{entry.lifetime_hours ?? '—'} h</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {:else}
        <EmptyState
          icon={HardDrive}
          title="No S.M.A.R.T data"
          description={smartDrivesLoaded
            ? 'No SMART-capable drives were detected. Check that the drive supports SMART and that smartmontools is installed.'
            : 'Select a drive above and rescan, or ensure smartmontools is installed.'}
          actionLabel="Rescan drives"
          onAction={loadSmartDrives}
        />
      {/if}
    </div>

  {:else if activeTab === 'topology'}
    <!-- Hardware Bus Topology Tab -->
    <div class="topology-toolbar">
      <div class="toolbar-left">
        <SearchBar
          bind:value={topologyQuery}
          placeholder="Filter PCI addresses, classes, USB IDs…"
          count={topologyQuery ? filteredPci.length + visibleUsbCount : undefined}
          style="width: 320px; max-width: 100%;"
        />
        <span class="toolbar-count">
          {pciDevices.length + usbDevices.length} device{pciDevices.length + usbDevices.length === 1 ? '' : 's'}
        </span>
      </div>
      <div class="toolbar-right">
        {#if lastTopologyScan}
          <span class="scan-time">
            Scanned {lastTopologyScan.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        {/if}
        <button class="header-refresh-btn" onclick={loadTopology} disabled={loadingTopology}>
          <RefreshCw size={13} class={loadingTopology ? 'animate-spin-slow' : ''} />
          Rescan
        </button>
      </div>
    </div>

    <div class="module-content-scroll topology-scroll-area">
      {#if loadingTopology}
        <div class="loading-state">
          <RefreshCw size={22} class="animate-spin-slow" />
          <span>Reading hardware buses…</span>
        </div>
      {:else if topologyError}
        <div class="error-card">
          <div class="error-header">
            <ShieldAlert size={18} />
            <span>Failed to read the hardware buses</span>
          </div>
          <p class="error-body">{topologyError}</p>
          <p class="error-hint">{topologyErrorHint}</p>
          <div class="error-actions">
            <Button variant="outline" size="sm" onclick={loadTopology}>
              <RefreshCw size={12} />
              Rescan
            </Button>
          </div>
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
              <Badge variant="muted">
                {topologyQuery
                  ? `${filteredPci.length} of ${pciDevices.length}`
                  : `${pciDevices.length} device${pciDevices.length === 1 ? '' : 's'}`}
              </Badge>
            </div>
            {#if filteredPci.length === 0}
              <div class="topo-empty">
                {pciDevices.length === 0
                  ? 'No PCI devices detected.'
                  : 'No PCI devices match the current filter.'}
              </div>
            {:else}
              <div class="topo-list">
                {#each filteredPci as pci, i (pci.slot)}
                  <div class="topo-row {i < filteredPci.length - 1 ? 'has-divider' : ''}">
                    <div class="topo-row-top">
                      <button
                        type="button"
                        class="topo-copy-btn"
                        onclick={() => copyText(pci.slot, 'PCI address copied')}
                        title="Copy PCI address {pci.slot}"
                        aria-label="Copy PCI address {pci.slot}"
                      >
                        <code class="topo-slot">{pci.slot}</code>
                        <Copy size={11} />
                      </button>
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
              <Badge variant="muted">
                {topologyQuery
                  ? `${visibleUsbCount} of ${usbDevices.length}`
                  : `${usbDevices.length} device${usbDevices.length === 1 ? '' : 's'}`}
              </Badge>
            </div>
            {#if usbGroups.length === 0}
              <div class="topo-empty">
                {usbDevices.length === 0
                  ? 'No USB devices detected.'
                  : 'No USB devices match the current filter.'}
              </div>
            {:else}
              <div class="topo-list">
                {#each usbGroups as group, bi (group.bus)}
                  <div class="usb-bus-group {bi < usbGroups.length - 1 ? 'has-divider' : ''}">
                    <div class="usb-bus-label">
                      <Usb size={12} style="color: var(--color-accent);" />
                      <span>Bus {group.bus}</span>
                      <Badge variant="muted" style="font-size: 9px;">{group.devices.length}</Badge>
                    </div>
                    {#each group.devices as usb, ui (usb.bus + ':' + usb.device)}
                      <div class="usb-device-row {ui < group.devices.length - 1 ? 'has-divider' : ''}">
                        <div class="usb-device-info">
                          <span class="device-name">{usb.name}</span>
                          <span class="usb-device-num">Device {usb.device}</span>
                        </div>
                        <button
                          type="button"
                          class="topo-copy-btn"
                          onclick={() => copyText(usb.id, 'USB ID copied')}
                          title="Copy USB ID {usb.id}"
                          aria-label="Copy USB ID {usb.id}"
                        >
                          <code class="topo-slot">{usb.id}</code>
                          <Copy size={11} />
                        </button>
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
  .smart-toolbar,
  .topology-toolbar {
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
    flex-wrap: wrap;
  }

  .toolbar-label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
    white-space: nowrap;
  }

  /* ── Layouts ─────────────────────────────────────────────── */

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
    min-height: 0;
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
    /* No overflow:hidden: section shadows and focus rings stay visible. */
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

  /* ── Inventory table ─────────────────────────────────────── */
  .toolbar-count,
  .scan-time {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .inventory-filters {
    flex-shrink: 0;
    margin-bottom: 12px;
    overflow-x: auto;
    padding-bottom: 2px;
  }

  .inventory-scroll {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  :global(.inventory-table) {
    border-radius: 12px;
  }

  :global(.inventory-table table) {
    min-width: 760px;
    table-layout: fixed;
  }

  .device-table-row {
    transition: background 0.12s ease;
  }

  .device-table-row:hover {
    background: var(--color-bg-hover);
  }

  .device-open-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 2px 0;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
    border-radius: 6px;
  }

  .device-open-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .device-table-icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: var(--color-accent-muted);
    border: 1px solid var(--color-border);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .device-table-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .device-table-name {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .device-table-sub {
    font-size: 11px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .device-vendor-cell {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .device-mono-cell {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
  }

  :global(.device-row-arrow) {
    color: var(--color-text-muted);
    opacity: 0.55;
  }

  .device-table-row:hover :global(.device-row-arrow) {
    opacity: 1;
    color: var(--color-accent);
  }

  .device-name {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Device details drawer ───────────────────────────────── */
  .device-drawer {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding-bottom: 12px;
  }

  .drawer-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .drawer-section-title {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .detail-grid {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    overflow: hidden;
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 16px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: 12px;
  }

  .detail-row:last-child {
    border-bottom: none;
  }

  .detail-label {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .detail-value {
    color: var(--color-text-primary);
    font-weight: 600;
    text-align: right;
    word-break: break-word;
  }

  .detail-value.mono {
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: 11.5px;
    font-weight: 500;
  }

  .driver-box {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
  }

  .driver-name {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 12px;
  }

  .driver-name code {
    color: var(--color-accent);
    font-family: var(--font-mono);
  }

  .drawer-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .drawer-hint {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--color-text-muted);
  }

  .drawer-note {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .capability-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .drawer-footer {
    display: flex;
    justify-content: flex-end;
    padding-top: 12px;
    border-top: 1px solid var(--color-border-subtle);
  }

  /* ── S.M.A.R.T enhancements ──────────────────────────────── */
  .error-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .smart-notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid rgba(245, 158, 11, 0.3);
    background: var(--color-warning-muted);
    color: var(--color-warning);
    font-size: 12px;
    line-height: 1.45;
  }

  .smart-notice-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .indicator-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 8px;
    padding: 12px 16px;
  }

  .indicator-item {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--color-border-subtle);
    border-radius: 7px;
    background: var(--color-bg-base);
    font-size: 12px;
  }

  .indicator-item.error {
    border-color: rgba(239, 68, 68, 0.35);
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .indicator-item.warning {
    border-color: rgba(245, 158, 11, 0.35);
    background: var(--color-warning-muted);
    color: var(--color-warning);
  }

  .indicator-label {
    font-weight: 500;
  }

  .indicator-value {
    font-weight: 700;
    font-family: var(--font-mono);
  }

  .indicator-ok {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 16px;
    color: var(--color-success);
    font-size: 12px;
  }

  .selftest-progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-card);
  }

  .selftest-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 12.5px;
    color: var(--color-text-primary);
  }

  .selftest-status {
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .progress-track {
    height: 6px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    overflow: hidden;
  }

  .progress-track > span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-accent);
    transition: width 0.3s ease;
  }

  .selftest-remaining {
    font-size: 11px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
  }

  .selftest-paused {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .wear-track {
    height: 5px;
    margin: 0 0 8px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    overflow: hidden;
  }

  .wear-track > span {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--color-success);
  }

  .wear-track > span.wear-high {
    background: var(--color-warning);
  }

  .error-log-list {
    display: flex;
    flex-direction: column;
  }

  .error-log-row {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: 12px;
  }

  .error-log-row:last-child {
    border-bottom: none;
  }

  .error-log-index {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }

  .error-log-desc {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-secondary);
  }

  .error-log-hours {
    flex-shrink: 0;
    color: var(--color-text-muted);
  }

  /* ── Topology rows ───────────────────────────────────────── */
  .topo-copy-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }

  .topo-copy-btn:hover {
    color: var(--color-accent);
  }

  .topo-copy-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  :global(.topo-copy-btn svg) {
    opacity: 0;
    transition: opacity 0.12s ease;
  }

  .topo-copy-btn:hover :global(svg),
  .topo-copy-btn:focus-visible :global(svg) {
    opacity: 1;
  }

  @media (max-width: 1050px) {
    .smart-summary-row,
    .topology-grid {
      grid-template-columns: minmax(0, 1fr);
    }

    :global(.inventory-table table) {
      min-width: 680px;
    }
  }

  @media (max-width: 720px) {
    .inventory-toolbar,
    .smart-toolbar,
    .topology-toolbar {
      align-items: stretch;
    }

    .toolbar-left,
    .toolbar-right {
      width: 100%;
    }

    .toolbar-right {
      justify-content: flex-end;
    }
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
