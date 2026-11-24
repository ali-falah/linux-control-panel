<script lang="ts">
  import { onMount } from 'svelte';
  import { 
    Activity, Wifi, Globe, RefreshCw, Server, Hash, Network, Plus, ArrowLeft, Save, Trash2,
    Play, Square, FileUp, ShieldCheck, ShieldAlert, Key, Link, AlertTriangle, CheckCircle2,
    XCircle, Info, Sliders, Zap, Check, Copy, Sparkles, Cpu, Laptop, Radio, ExternalLink,
    AlertCircle, CornerDownRight, CheckCheck, HelpCircle, MoreHorizontal, Power, Pencil, Search
  } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import Button from '../components/ui/Button.svelte';
  import Select from '../components/ui/Select.svelte';
  import Input from '../components/ui/Input.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import ContextMenu, { type ContextMenuItem } from '../components/ui/ContextMenu.svelte';
  import { 
    validateIpv4WithCidr, 
    validateGateway, 
    validateDnsServers,
    validateIpv6DnsServers,
    validateIpv6Address,
    COMMON_SUBNET_MASKS, 
    POPULAR_DNS_PRESETS,
    cidrToSubnetMask,
    subnetMaskToCidr,
    type Ipv4ValidationResult,
    type GatewayValidationResult,
    type DnsValidationResult,
    type Ipv6ValidationResult
  } from '../utils/networkCalc';

  // ─── State ───────────────────────────────────────────────────────────────────

  let interfaces = $state<any[]>([]);
  let dnsInfo = $state<string>('');
  let connections = $state<any[]>([]);
  let vpnProfiles = $state<any[]>([]);
  
  let loading = $state(true);
  let loadError = $state('');
  let loadingVpn = $state(false);
  let connectionFilter = $state('');
  let connectionStatusFilter = $state<'all' | 'active' | 'inactive'>('all');
  let interfaceFilter = $state('');
  let connectionBusyUuid = $state<string | null>(null);
  let activeTab = $state<'interfaces' | 'dns' | 'connections' | 'vpn' | 'speedtest'>(
    uiStore.targetSubTab && ['interfaces', 'dns', 'connections', 'vpn', 'speedtest'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'interfaces'
  );
  if (uiStore.targetSubTab && ['interfaces', 'dns', 'connections', 'vpn', 'speedtest'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  
  let selectedConnectionUuid = $state<string | null>(null);
  let editConnectionData = $state<any>(null);
  let editorOriginalSnapshot = $state('');
  let editingWasActive = $state(false);
  let wifiPassword = $state('');
  let bridgePort = $state('');
  let isSaving = $state(false);

  let connectionMenu = $state<{ open: boolean; x: number; y: number; conn: any | null }>({
    open: false, x: 0, y: 0, conn: null,
  });
  let interfaceMenu = $state<{ open: boolean; x: number; y: number; iface: any | null }>({
    open: false, x: 0, y: 0, iface: null,
  });

  // Manual VPN form
  let showVpnDrawer = $state(false);
  let vpnForm = $state({
    name: 'My VPN Connection',
    type: 'openvpn', // 'openvpn' or 'wireguard'
    gateway: '',
    username: '',
    password: '',
    peerPublicKey: '',
    address: '',
    caCert: '',
    clientCert: '',
    clientKey: ''
  });
  let vpnFilter = $state('');
  /** uuid of the profile currently connecting/disconnecting (disables its buttons) */
  let vpnBusyUuid = $state<string | null>(null);

  // Never keep a VPN password in state after its drawer closes (Cancel, X,
  // Escape or a successful create).
  $effect(() => {
    if (!showVpnDrawer) {
      vpnForm.password = '';
    }
  });

  const filteredVpnProfiles = $derived.by(() => {
    const q = vpnFilter.trim().toLowerCase();
    if (!q) return vpnProfiles;
    return vpnProfiles.filter(
      (v: any) =>
        (v.name || '').toLowerCase().includes(q) ||
        (v.vpn_type || '').toLowerCase().includes(q) ||
        (v.device || '').toLowerCase().includes(q)
    );
  });

  let filteredConnections = $derived.by(() => {
    const q = connectionFilter.trim().toLowerCase();
    return connections.filter((conn) => {
      const active = conn.state === 'activated';
      if (connectionStatusFilter === 'active' && !active) return false;
      if (connectionStatusFilter === 'inactive' && active) return false;
      if (!q) return true;
      return [conn.name, conn.type, conn.device, conn.uuid, conn.state]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q);
    });
  });

  let connectionStatusOptions = $derived([
    { id: 'all', label: 'All', count: connections.length },
    { id: 'active', label: 'Active', count: connections.filter((conn) => conn.state === 'activated').length },
    { id: 'inactive', label: 'Inactive', count: connections.filter((conn) => conn.state !== 'activated').length },
  ]);

  let filteredInterfaces = $derived.by(() => {
    const q = interfaceFilter.trim().toLowerCase();
    if (!q) return interfaces;
    return interfaces.filter((iface) => {
      const addresses = (iface.addr_info ?? [])
        .map((addr: any) => addr.local || '')
        .join(' ');
      return [iface.ifname, iface.address, iface.link_type, iface.operstate, addresses]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q);
    });
  });

  let editorDirty = $derived(
    selectedConnectionUuid !== null &&
      !!editConnectionData &&
      editorOriginalSnapshot !== '' &&
      (JSON.stringify(editConnectionData) !== editorOriginalSnapshot ||
        wifiPassword.length > 0 ||
        bridgePort.length > 0)
  );

  let connectionMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const conn = connectionMenu.conn;
    if (!conn) return [];
    const active = conn.state === 'activated';
    return [
      { label: 'Edit IP settings', icon: Pencil, action: () => editConnection(conn.uuid) },
      {
        label: active ? 'Deactivate connection' : 'Activate connection',
        icon: Power,
        color: active ? 'var(--color-warning)' : 'var(--color-success)',
        disabled: connectionBusyUuid === conn.uuid,
        action: () => (active ? disconnectConnection(conn.uuid) : activateConnection(conn.uuid)),
      },
      { label: '', divider: true },
      { label: 'Copy connection name', icon: Copy, action: () => copyText(conn.name, 'Connection name copied') },
      { label: 'Copy UUID', icon: Copy, action: () => copyText(conn.uuid, 'Connection UUID copied') },
      { label: '', divider: true },
      {
        label: 'Delete profile',
        icon: Trash2,
        danger: true,
        disabled: isLoopbackConnection(conn),
        action: () => deleteConnection(conn.uuid, conn.name),
      },
    ];
  });

  let interfaceMenuItems = $derived.by<ContextMenuItem[]>(() => {
    const iface = interfaceMenu.iface;
    if (!iface) return [];
    const isLoopback = isLoopbackInterface(iface.ifname);
    const addresses = (iface.addr_info ?? []).map((addr: any) => `${addr.local}/${addr.prefixlen}`);
    const activeConnection = connections.find(
      (conn) => conn.device === iface.ifname && conn.state === 'activated'
    );
    return [
      ...(activeConnection
        ? [
            {
              label: 'Edit active connection',
              icon: Pencil,
              action: () => {
                activeTab = 'connections';
                editConnection(activeConnection.uuid);
              },
            },
          ]
        : []),
      {
        label: iface.operstate === 'UP' ? 'Disable interface' : 'Enable interface',
        icon: Power,
        color: iface.operstate === 'UP' ? 'var(--color-warning)' : 'var(--color-success)',
        disabled: isLoopback,
        action: () => setInterfaceState(iface.ifname, iface.operstate !== 'UP'),
      },
      { label: '', divider: true },
      { label: 'Copy interface name', icon: Copy, action: () => copyText(iface.ifname, 'Interface name copied') },
      ...(iface.address && iface.address !== '00:00:00:00:00:00'
        ? [{ label: 'Copy MAC address', icon: Copy, action: () => copyText(iface.address, 'MAC address copied') }]
        : []),
      ...(addresses.length > 0
        ? [{ label: 'Copy IP addresses', icon: Copy, action: () => copyText(addresses.join(', '), 'IP addresses copied') }]
        : []),
    ];
  });

  function vpnProtocolLabel(vpn: any) {
    return vpn.vpn_type === 'wireguard' ? 'WireGuard' : 'OpenVPN';
  }

  /** WireGuard needs the server's public key; OpenVPN needs a name and gateway. */
  const vpnFormValid = $derived.by(() => {
    if (!vpnForm.name.trim()) return false;
    if (!vpnForm.gateway.trim()) return false;
    if (vpnForm.type === 'wireguard') {
      return (
        vpnForm.peerPublicKey.trim().length === 44 &&
        /^[A-Za-z0-9+/]{43}=$/.test(vpnForm.peerPublicKey.trim()) &&
        vpnForm.address.trim().length > 0
      );
    }
    return true;
  });

  async function pickVpnFile(field: 'caCert' | 'clientCert' | 'clientKey') {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Certificates and keys', extensions: ['crt', 'pem', 'key', 'ca', 'cert'] }]
      });
      if (selected && typeof selected === 'string') {
        vpnForm[field] = selected;
      }
    } catch (e) {
      uiStore.addToast(`Failed to open file dialog: ${e}`, 'error');
    }
  }
  
  // Speed Test States
  let isTestingSpeed = $state(false);
  let speedProgress = $state(''); // 'ping' | 'download' | 'upload' | 'done' | ''
  let speedPing = $state<number | null>(null);
  let speedDownload = $state<number | null>(null);
  let speedUpload = $state<number | null>(null);
  let speedTestError = $state('');

  // ─── Real-Time Network & Subnet Calculations (Svelte 5 Reactive Runes) ───────

  function splitAddressParts(raw: string): string[] {
    return raw.split(/[,\s]+/).map((part) => part.trim()).filter(Boolean);
  }

  let ipv4Validation = $derived.by<Ipv4ValidationResult>(() => {
    if (!editConnectionData || editConnectionData['ipv4.method'] !== 'manual') {
      return {
        raw: '', ip: '', cidr: 24, maskStr: '255.255.255.0', wildcardStr: '0.0.0.255',
        networkIp: '', broadcastIp: '', firstUsableIp: '', lastUsableIp: '',
        totalHosts: 0, usableHosts: 0, scope: '', isNetworkAddress: false,
        isBroadcastAddress: false, isLoopback: false, isLinkLocal: false, isMulticast: false,
        isValid: true, errors: [], warnings: []
      };
    }
    const raw = editConnectionData['ipv4.addresses'] || '';
    const parts = splitAddressParts(raw);
    if (parts.length === 0) return validateIpv4WithCidr(raw);
    // The calculator describes the first address; every address is validated.
    const first = validateIpv4WithCidr(parts[0]);
    const extraErrors = parts.slice(1).flatMap((part) => validateIpv4WithCidr(part).errors);
    return {
      ...first,
      raw,
      errors: [...first.errors, ...extraErrors],
      isValid: first.isValid && extraErrors.length === 0
    };
  });

  let gatewayValidation = $derived.by<GatewayValidationResult>(() => {
    if (!editConnectionData || editConnectionData['ipv4.method'] !== 'manual') {
      return {
        raw: '', ip: '', isValid: true, inSameSubnet: true, isConflictWithHost: false,
        isNetworkOrBroadcast: false, suggestedGateway: '', errors: [], warnings: []
      };
    }
    return validateGateway(editConnectionData['ipv4.gateway'] || '', ipv4Validation);
  });

  let dnsValidation = $derived.by<DnsValidationResult>(() => {
    if (!editConnectionData || editConnectionData['ipv4.method'] === 'disabled') {
      return { raw: '', servers: [], isValid: true, errors: [] };
    }
    return validateDnsServers(editConnectionData['ipv4.dns'] || '');
  });

  let ipv6Validation = $derived.by<Ipv6ValidationResult>(() => {
    if (!editConnectionData || editConnectionData['ipv6.method'] !== 'manual') {
      return { raw: '', ip: '', prefix: 64, gateway: '', isValid: true, errors: [] };
    }
    const raw = editConnectionData['ipv6.addresses'] || '';
    const parts = splitAddressParts(raw);
    if (parts.length === 0) {
      return validateIpv6Address(raw, editConnectionData['ipv6.gateway'] || '');
    }
    const first = validateIpv6Address(parts[0], editConnectionData['ipv6.gateway'] || '');
    const extraErrors = parts
      .slice(1)
      .flatMap((part) => validateIpv6Address(part).errors);
    return {
      ...first,
      raw,
      errors: [...first.errors, ...extraErrors],
      isValid: first.isValid && extraErrors.length === 0
    };
  });

  let ipv6DnsValidation = $derived.by(() => {
    if (!editConnectionData || editConnectionData['ipv6.method'] !== 'manual') {
      return { raw: '', servers: [], isValid: true, errors: [] as string[] };
    }
    return validateIpv6DnsServers(editConnectionData['ipv6.dns'] || '');
  });

  let hasBlockingErrors = $derived.by<boolean>(() => {
    if (!editConnectionData) return false;
    if (editConnectionData['ipv4.method'] === 'manual') {
      if (!ipv4Validation.isValid || !gatewayValidation.isValid || !dnsValidation.isValid) {
        return true;
      }
    }
    if (editConnectionData['ipv6.method'] === 'manual') {
      if (!ipv6Validation.isValid || !ipv6DnsValidation.isValid) return true;
    }
    return false;
  });

  // ─── Subnet & IP Convenience Actions ─────────────────────────────────────────

  function handleMaskDropdownChange(newCidr: number) {
    if (!editConnectionData) return;
    const currentAddr = (editConnectionData['ipv4.addresses'] || '').trim();
    if (!currentAddr) {
      editConnectionData['ipv4.addresses'] = `192.168.1.10/${newCidr}`;
      return;
    }
    const ipPart = currentAddr.split('/')[0].trim();
    editConnectionData['ipv4.addresses'] = `${ipPart || '192.168.1.10'}/${newCidr}`;
  }

  function applyDnsPreset(preset: { primary: string; secondary: string; name: string }) {
    if (!editConnectionData) return;
    editConnectionData['ipv4.dns'] = `${preset.primary}, ${preset.secondary}`;
    uiStore.addToast(`Applied ${preset.name} DNS servers (${preset.primary}, ${preset.secondary})`, 'success');
  }

  function fixGateway(suggested: string) {
    if (!editConnectionData || !suggested) return;
    editConnectionData['ipv4.gateway'] = suggested;
    uiStore.addToast(`Gateway updated to ${suggested}`, 'info');
  }

  // ─── Data Fetching ───────────────────────────────────────────────────────────

  /**
   * nmcli's terse output escapes the ':' separator and '\' inside field values.
   * Splitting on a raw ':' truncates a profile named "Corp: US-East" and shifts
   * every following column, which makes the profile look like a different type
   * and can drop it from the list entirely.
   */
  function splitNmcliFields(line: string): string[] {
    const fields: string[] = [];
    let current = '';
    let escaped = false;

    for (const ch of line) {
      if (escaped) {
        current += ch;
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === ':') {
        fields.push(current);
        current = '';
      } else {
        current += ch;
      }
    }
    fields.push(current);
    return fields;
  }

  function unescapeNmcli(value: string): string {
    let result = '';
    let escaped = false;
    for (const ch of value) {
      if (escaped) {
        result += ch;
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else {
        result += ch;
      }
    }
    if (escaped) result += '\\';
    return result;
  }

  /** Split one terse `key:value` line at the first unescaped colon. */
  function splitNmcliKeyValue(line: string): [string, string] | null {
    let escaped = false;
    for (let index = 0; index < line.length; index += 1) {
      const ch = line[index];
      if (escaped) {
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === ':') {
        return [line.slice(0, index), unescapeNmcli(line.slice(index + 1))];
      }
    }
    return null;
  }

  function isLoopbackInterface(name: string | undefined | null): boolean {
    const value = (name || '').trim().toLowerCase();
    return value === 'lo' || value.startsWith('loopback');
  }

  function isLoopbackConnection(conn: any): boolean {
    return isLoopbackInterface(conn?.name) || isLoopbackInterface(conn?.device);
  }

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast(label, 'info');
    } catch (e) {
      uiStore.addToast(`Could not copy: ${e}`, 'error');
    }
  }

  function menuPoint(e: MouseEvent): { x: number; y: number } {
    const fromKeyboard = e.type === 'click' && e.clientX === 0 && e.clientY === 0;
    const rect = fromKeyboard
      ? (e.currentTarget as HTMLElement | null)?.getBoundingClientRect()
      : null;
    return rect ? { x: rect.left, y: rect.bottom + 4 } : { x: e.clientX, y: e.clientY };
  }

  function openConnectionMenu(e: MouseEvent, conn: any) {
    e.preventDefault();
    e.stopPropagation();
    connectionMenu = { ...menuPoint(e), open: true, conn };
  }

  function openInterfaceMenu(e: MouseEvent, iface: any) {
    e.preventDefault();
    e.stopPropagation();
    interfaceMenu = { ...menuPoint(e), open: true, iface };
  }

  async function loadVpnProfiles() {
    loadingVpn = true;
    try {
      vpnProfiles = await invoke<any[]>('network_get_vpn_profiles');
    } catch(e) {
      console.error("Failed to load VPN profiles", e);
      uiStore.addToast(`Failed to load VPN profiles: ${e}`, 'error');
    } finally {
      loadingVpn = false;
    }
  }

  async function loadData() {
    loading = true;
    loadError = '';
    selectedConnectionUuid = null;
    editConnectionData = null;
    statusStore.setBusy('Loading network information…');
    try {
      const ifacesRaw: string = await invoke('network_get_interfaces');
      interfaces = JSON.parse(ifacesRaw);
      dnsInfo = await invoke('network_get_dns');
      
      const connsRaw: string = await invoke('network_list_connections');
      connections = connsRaw.split('\n').filter(l => l.trim()).map(line => {
        const parts = splitNmcliFields(line);
        return {
          uuid: parts[0] ?? '',
          name: parts[1] ?? '',
          type: parts[2] ?? '',
          device: parts[3] ?? '',
          state: parts[4] ?? ''
        };
      });

      await loadVpnProfiles();
      statusStore.setLastCommand('nmcli connection show', 0, true);
    } catch (e) {
      console.error(e);
      loadError = String(e);
      statusStore.setLastCommand('nmcli connection show', 1, false);
      uiStore.addToast(`Failed to load network information: ${e}`, 'error');
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function editConnection(uuid: string) {
    if (uuid === '') {
      // New connection profile defaults
      editConnectionData = { 
        'connection.id': 'New Connection',
        'connection.type': '802-3-ethernet',
        'connection.autoconnect': 'yes',
        '802-11-wireless.ssid': '',
        '802-11-wireless-security.key-mgmt': 'wpa-psk',
        'connection.interface-name': 'br0',
        'ipv4.method': 'auto', 
        'ipv6.method': 'auto', 
        'ipv4.addresses': '', 
        'ipv4.gateway': '', 
        'ipv4.dns': '',
        'ipv6.addresses': '',
        'ipv6.gateway': '',
        'ipv6.dns': ''
      };
      editingWasActive = false;
      wifiPassword = '';
      bridgePort = '';
      selectedConnectionUuid = uuid;
      editorOriginalSnapshot = JSON.stringify(editConnectionData);
      return;
    }
    
    statusStore.setBusy('Loading connection properties…');
    try {
      const raw: string = await invoke('network_get_connection', { uuid });
      const parsed: any = {};
      raw.split('\n').forEach(line => {
        const pair = splitNmcliKeyValue(line);
        if (pair) parsed[pair[0]] = pair[1];
      });
      editConnectionData = parsed;
      wifiPassword = '';
      bridgePort = '';
      editingWasActive =
        connections.find((candidate) => candidate.uuid === uuid)?.state === 'activated';
      selectedConnectionUuid = uuid;
      editorOriginalSnapshot = JSON.stringify(editConnectionData);
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 0, true);
    } catch(e) {
      console.error(e);
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 1, false);
      uiStore.addToast(`Failed to load the connection: ${e}`, 'error');
    }
    statusStore.clearBusy();
  }

  async function saveConnection() {
    const isNew = selectedConnectionUuid === '';
    const connectionType = editConnectionData?.['connection.type'];
    if (!String(editConnectionData?.['connection.id'] || '').trim()) {
      uiStore.addToast('A connection profile name is required.', 'warning');
      return;
    }
    if (isNew && connectionType === '802-11-wireless') {
      const ssid = String(editConnectionData['802-11-wireless.ssid'] || '').trim();
      if (!ssid || ssid.length > 32) {
        uiStore.addToast('A Wi-Fi SSID of 1–32 characters is required.', 'warning');
        return;
      }
      if (
        editConnectionData['802-11-wireless-security.key-mgmt'] === 'wpa-psk' &&
        (wifiPassword.length < 8 || wifiPassword.length > 63)
      ) {
        uiStore.addToast('A WPA-PSK passphrase must be 8–63 characters.', 'warning');
        return;
      }
    }
    if (isNew && connectionType === 'bridge') {
      const bridgeName = String(editConnectionData['connection.interface-name'] || '').trim();
      if (!/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,14}$/.test(bridgeName)) {
        uiStore.addToast('Enter a valid bridge interface name (max 15 characters).', 'warning');
        return;
      }
      if (!bridgePort || bridgePort === bridgeName || isLoopbackInterface(bridgePort)) {
        uiStore.addToast('Choose a physical interface for the bridge port.', 'warning');
        return;
      }
    }
    if (hasBlockingErrors) {
      const errorList = [
        ...ipv4Validation.errors,
        ...gatewayValidation.errors,
        ...dnsValidation.errors,
        ...ipv6Validation.errors,
        ...ipv6DnsValidation.errors
      ];
      uiStore.addToast(`Cannot save: ${errorList[0] || 'Please resolve network configuration errors.'}`, 'error');
      return;
    }

    isSaving = true;
    statusStore.setBusy('Saving connection settings…');
    
    try {
      const ipv4Manual = editConnectionData['ipv4.method'] === 'manual';
      const ipv6Manual = editConnectionData['ipv6.method'] === 'manual';
      const settings: Record<string, string> = {
        'ipv4.method': editConnectionData['ipv4.method'],
        'ipv4.addresses': ipv4Manual ? (editConnectionData['ipv4.addresses'] || '') : '',
        'ipv4.gateway': ipv4Manual ? (editConnectionData['ipv4.gateway'] || '') : '',
        'ipv4.dns': editConnectionData['ipv4.method'] === 'disabled' ? '' : (editConnectionData['ipv4.dns'] || ''),
        'ipv6.method': editConnectionData['ipv6.method'] || 'auto',
        'ipv6.addresses': ipv6Manual ? (editConnectionData['ipv6.addresses'] || '') : '',
        'ipv6.gateway': ipv6Manual ? (editConnectionData['ipv6.gateway'] || '') : '',
        'ipv6.dns': ipv6Manual ? (editConnectionData['ipv6.dns'] || '') : '',
        'connection.autoconnect': editConnectionData['connection.autoconnect'] === 'no' ? 'no' : 'yes',
      };

      settings['connection.id'] = editConnectionData['connection.id'];
      if (isNew) {
        settings['connection.type'] = connectionType;
        if (connectionType === '802-11-wireless') {
          settings['802-11-wireless.ssid'] = String(
            editConnectionData['802-11-wireless.ssid'] || ''
          ).trim();
          settings['802-11-wireless.mode'] = 'infrastructure';
          const security = editConnectionData['802-11-wireless-security.key-mgmt'] || 'wpa-psk';
          settings['802-11-wireless-security.key-mgmt'] = security;
        }
        if (connectionType === 'bridge') {
          settings['connection.interface-name'] = String(
            editConnectionData['connection.interface-name'] || ''
          ).trim();
        }
      }

      if (isNew && connectionType === 'bridge') {
        // The backend creates the bridge and its port together, and rolls the
        // bridge back if the port profile cannot be added.
        await invoke('network_create_bridge', {
          name: editConnectionData['connection.id'],
          bridgeInterface: String(editConnectionData['connection.interface-name'] || '').trim(),
          portInterface: bridgePort,
          settings,
        });
      } else {
        await invoke('network_save_connection', { uuid: selectedConnectionUuid, settings });

        // The PSK is deliberately not part of `settings`: it is written to the
        // root-only keyfile by the helper over stdin, never through argv.
        if (
          isNew &&
          connectionType === '802-11-wireless' &&
          editConnectionData['802-11-wireless-security.key-mgmt'] === 'wpa-psk' &&
          wifiPassword
        ) {
          try {
            await invoke('network_store_connection_secret', {
              connection: editConnectionData['connection.id'],
              kind: 'wifi-psk',
              secret: wifiPassword,
            });
          } catch (secretError) {
            // The profile itself exists; do not pretend the whole save failed.
            uiStore.addToast(
              `Wi-Fi profile created, but its password could not be stored: ${secretError}`,
              'warning'
            );
          }
        }
      }
      statusStore.setLastCommand(`nmcli connection modify ${selectedConnectionUuid || 'new'}`, 0, true);
      uiStore.addToast('Connection settings saved successfully', 'success');
      
      // Reapply the profile only when it was active before editing; saving an
      // inactive profile must not silently activate it.
      if (selectedConnectionUuid && editingWasActive) {
        await invoke('network_up_connection', { uuid: selectedConnectionUuid }).catch(() => {});
      }
      
      wifiPassword = '';
      bridgePort = '';
      await loadData();
      activeTab = 'connections';
    } catch(e: any) {
      uiStore.addToast(`Error saving connection: ${e}`, 'error');
      statusStore.setLastCommand(`nmcli connection modify`, 1, false);
    } finally {
      isSaving = false;
      statusStore.clearBusy();
    }
  }
  
  function deleteConnection(uuid: string, name?: string) {
    uiStore.confirm(
      'Delete Connection Profile',
      `Are you sure you want to delete${name ? ` "${name}"` : ' this connection profile'}? This cannot be undone.`,
      () => doDeleteConnection(uuid),
      true
    );
  }

  async function doDeleteConnection(uuid: string) {
    statusStore.setBusy('Deleting connection…');
    try {
      await invoke('network_delete_connection', { uuid });
      statusStore.setLastCommand(`nmcli connection delete ${uuid}`, 0, true);
      uiStore.addToast('Connection profile deleted', 'info');
      await loadData();
      selectedConnectionUuid = null;
    } catch(e) {
      uiStore.addToast(`Error deleting connection: ${e}`, 'error');
      statusStore.setLastCommand(`nmcli connection delete ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
  }

  async function activateConnection(uuid: string) {
    connectionBusyUuid = uuid;
    statusStore.setBusy('Activating connection…');
    try {
      await invoke('network_up_connection', { uuid });
      statusStore.setLastCommand(`nmcli connection up ${uuid}`, 0, true);
      uiStore.addToast('Connection activated', 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Could not activate connection: ${e}`, 'error');
      statusStore.setLastCommand(`nmcli connection up ${uuid}`, 1, false);
    } finally {
      connectionBusyUuid = null;
      statusStore.clearBusy();
    }
  }

  async function disconnectConnection(uuid: string) {
    connectionBusyUuid = uuid;
    statusStore.setBusy('Disconnecting…');
    try {
      await invoke('network_down_connection', { uuid });
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 0, true);
      uiStore.addToast('Connection deactivated', 'info');
      await loadData();
    } catch(e) {
      uiStore.addToast(`Error disconnecting: ${e}`, 'error');
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 1, false);
    } finally {
      connectionBusyUuid = null;
      statusStore.clearBusy();
    }
  }

  async function setInterfaceState(iface: string, up: boolean) {
    statusStore.setBusy(up ? 'Enabling interface…' : 'Disabling interface…');
    try {
      await invoke('network_set_interface_state', { iface, up });
      statusStore.setLastCommand(`ip link set ${iface} ${up ? 'up' : 'down'}`, 0, true);
      uiStore.addToast(`Interface ${iface} turned ${up ? 'UP' : 'DOWN'}`, 'success');
      await loadData();
    } catch(e) {
      uiStore.addToast(`Error setting interface state: ${e}`, 'error');
      statusStore.setLastCommand(`ip link set ${iface} ${up ? 'up' : 'down'}`, 1, false);
    }
    statusStore.clearBusy();
  }

  async function importVpnFile() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: 'VPN Configuration (*.ovpn, *.conf)', extensions: ['ovpn', 'conf'] }
        ]
      });
      
      if (!selected || typeof selected !== 'string') return;
      
      statusStore.setBusy('Importing VPN profile…');
      const res = await invoke<string>('network_import_vpn_profile', {
        name: '',
        filePath: selected
      });
      
      uiStore.addToast(res, 'success');
      await loadData();
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Import failed: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function handleCreateVpn() {
    if (!vpnForm.gateway.trim()) {
      uiStore.addToast('Gateway/Endpoint is required', 'warning');
      return;
    }
    if (!vpnFormValid) {
      uiStore.addToast(
        vpnForm.type === 'wireguard'
          ? 'WireGuard profiles need a peer public key'
          : 'Profile name and gateway are required',
        'warning'
      );
      return;
    }
    
    statusStore.setBusy('Creating VPN profile…');
    try {
      const res = await invoke<string>('network_create_vpn_profile', {
        name: vpnForm.name,
        vpnType: vpnForm.type,
        gateway: vpnForm.gateway.trim(),
        username: vpnForm.type === 'openvpn' ? (vpnForm.username || null) : null,
        password: vpnForm.type === 'openvpn' ? (vpnForm.password || null) : null,
        peerPublicKey: vpnForm.type === 'wireguard' ? vpnForm.peerPublicKey.trim() : null,
        address: vpnForm.type === 'wireguard' ? vpnForm.address.trim() : null,
        caCert: vpnForm.type === 'openvpn' ? (vpnForm.caCert || null) : null,
        clientCert: vpnForm.type === 'openvpn' ? (vpnForm.clientCert || null) : null,
        clientKey: vpnForm.type === 'openvpn' ? (vpnForm.clientKey || null) : null
      });
      
      uiStore.addToast(res, 'success');
      showVpnDrawer = false;
      vpnForm.peerPublicKey = '';
      vpnForm.address = '';
      vpnForm.caCert = '';
      vpnForm.clientCert = '';
      vpnForm.clientKey = '';
      await loadData();
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Creation failed: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function connectVpn(uuid: string) {
    vpnBusyUuid = uuid;
    statusStore.setBusy('Connecting VPN…');
    try {
      await invoke('network_up_connection', { uuid });
      uiStore.addToast('VPN Connected successfully', 'success');
      await loadData();
    } catch(e) {
      uiStore.addToast(`VPN Connection failed: ${e}`, 'error');
    } finally {
      vpnBusyUuid = null;
      statusStore.clearBusy();
    }
  }

  async function disconnectVpn(uuid: string) {
    vpnBusyUuid = uuid;
    statusStore.setBusy('Disconnecting VPN…');
    try {
      await invoke('network_down_connection', { uuid });
      uiStore.addToast('VPN disconnected', 'info');
      await loadData();
    } catch(e) {
      uiStore.addToast(`Error disconnecting VPN: ${e}`, 'error');
    } finally {
      vpnBusyUuid = null;
      statusStore.clearBusy();
    }
  }

  async function runSpeedTest() {
    isTestingSpeed = true;
    speedTestError = '';
    speedPing = null;
    speedDownload = null;
    speedUpload = null;
    
    try {
      speedProgress = 'ping';
      speedPing = await invoke<number>('network_test_ping');
      
      speedProgress = 'download';
      speedDownload = await invoke<number>('network_test_download');
      
      speedProgress = 'upload';
      speedUpload = await invoke<number>('network_test_upload');
      
      speedProgress = 'done';
    } catch (e) {
      console.error(e);
      speedTestError = String(e);
      speedProgress = '';
    } finally {
      isTestingSpeed = false;
    }
  }

  onMount(() => {
    void loadData();
    const unregister = uiStore.registerLeaveGuard(() => editorDirty);
    return unregister;
  });

  function closeEditor() {
    if (!editorDirty) {
      selectedConnectionUuid = null;
      editConnectionData = null;
      return;
    }
    uiStore.confirm(
      'Discard unsaved connection changes?',
      'The settings you changed in this connection have not been saved.',
      () => {
        selectedConnectionUuid = null;
        editConnectionData = null;
      },
      true
    );
  }
</script>

<div class="module-page">
  <PageHeader title="Advanced Network" icon={Wifi} description="Manage network interfaces, connection profiles, subnets, and routing" />

  {#if selectedConnectionUuid !== null}
    <!-- ══════════════════════════════════════════════════════════════════════════
         CONNECTION EDITOR & SUBNET CALCULATOR VIEW
         ══════════════════════════════════════════════════════════════════════════ -->
    <div class="editor-header-bar">
      <Button variant="outline" class="btn-sm" onclick={closeEditor}>
        <ArrowLeft size={14} /> Back to Connections
      </Button>
      
      <div class="editor-actions">
        {#if selectedConnectionUuid !== ''}
          {#if editingWasActive}
            <Button variant="outline" class="btn-sm btn-warn-outline" onclick={() => disconnectConnection(selectedConnectionUuid!)}>
              <Activity size={13} /> Deactivate
            </Button>
          {:else}
            <Button variant="outline" class="btn-sm" onclick={() => activateConnection(selectedConnectionUuid!)}>
              <Play size={13} /> Activate
            </Button>
          {/if}
          <Button variant="outline" class="btn-sm btn-danger-outline" onclick={() => deleteConnection(selectedConnectionUuid!, editConnectionData?.['connection.id'])}>
            <Trash2 size={13} /> Delete Profile
          </Button>
        {/if}
        <Button 
          variant="primary" 
          class="btn-sm" 
          disabled={isSaving || hasBlockingErrors} 
          onclick={saveConnection}
          title={hasBlockingErrors ? 'Resolve validation errors to save' : 'Save connection settings'}
        >
          {#if isSaving}
            <RefreshCw size={13} class="animate-spin-slow" /> Saving...
          {:else}
            <Save size={13} /> Save Settings
          {/if}
        </Button>
      </div>
    </div>
    
    <div class="module-content-scroll editor-scroll-body">
      <!-- ── Live Blocking Errors Alert Banner ── -->
      {#if hasBlockingErrors}
        <div class="net-alert-box net-alert-error">
          <div class="net-alert-header">
            <AlertCircle size={18} class="text-danger flex-shrink-0" />
            <strong>Configuration Errors Detected (Please fix before saving):</strong>
          </div>
          <ul class="net-error-list">
            {#each ipv4Validation.errors as err}
              <li><strong>IPv4 Error:</strong> {err}</li>
            {/each}
            {#each gatewayValidation.errors as err}
              <li><strong>Gateway Error:</strong> {err}</li>
            {/each}
            {#each dnsValidation.errors as err}
              <li><strong>DNS Error:</strong> {err}</li>
            {/each}
            {#each ipv6Validation.errors as err}
              <li><strong>IPv6 Error:</strong> {err}</li>
            {/each}
            {#each ipv6DnsValidation.errors as err}
              <li><strong>IPv6 DNS Error:</strong> {err}</li>
            {/each}
          </ul>
        </div>
      {/if}

      <div class="card editor-main-card">
        <div class="card-title-row">
          <div class="card-title-group">
            <Network size={18} class="text-accent" />
            <h3>
              {selectedConnectionUuid === '' ? 'Add New Network Connection' : 'Edit Connection: ' + (editConnectionData['connection.id'] || 'Profile')}
            </h3>
          </div>
          <span class="badge {editConnectionData['ipv4.method'] === 'manual' ? 'badge-warning' : 'badge-success'}">
            IPv4: {editConnectionData['ipv4.method'] === 'manual' ? 'Static IP' : editConnectionData['ipv4.method'] === 'auto' ? 'DHCP' : editConnectionData['ipv4.method']}
          </span>
        </div>

        <div class="connection-options-row">
          <div class="connection-toggle-label">
            <span>Connect automatically</span>
            <span class="field-hint">Bring this profile up when its device is available.</span>
          </div>
          <Toggle
            checked={editConnectionData['connection.autoconnect'] !== 'no'}
            onToggle={(checked) => (editConnectionData['connection.autoconnect'] = checked ? 'yes' : 'no')}
            title="Connect automatically"
          />
        </div>

        <div class="form-grid-2">
          <div class="form-group">
            <label for="conn-id">Connection Profile Name</label>
            <input id="conn-id" type="text" class="form-input" bind:value={editConnectionData['connection.id']} placeholder="e.g. eth0-static" />
          </div>
          <div class="form-group">
            <span class="form-label">Connection Type</span>
            {#if selectedConnectionUuid === ''}
              <Select id="conn-type" bind:value={editConnectionData['connection.type']}>
                <option value="802-3-ethernet">Ethernet (Wired LAN)</option>
                <option value="802-11-wireless">Wi-Fi (Wireless)</option>
                <option value="bridge">Network Bridge</option>
              </Select>
              <span class="field-hint">
                VPN profiles are created in the VPN tab.
              </span>
            {:else}
              <span id="conn-type" class="static-value">
                {editConnectionData['connection.type'] === '802-11-wireless'
                  ? 'Wi-Fi (Wireless)'
                  : editConnectionData['connection.type'] === 'wireguard'
                    ? 'WireGuard Tunnel'
                    : editConnectionData['connection.type'] === 'bridge'
                      ? 'Network Bridge'
                      : 'Ethernet (Wired LAN)'}
              </span>
            {/if}
          </div>
        </div>

        {#if selectedConnectionUuid === '' && editConnectionData['connection.type'] === '802-11-wireless'}
          <div class="type-specific-grid">
            <Input
              label="Wi-Fi SSID"
              bind:value={editConnectionData['802-11-wireless.ssid']}
              placeholder="Home network"
            />
            <div class="form-group">
              <span class="form-label">Security</span>
              <Select
                bind:value={editConnectionData['802-11-wireless-security.key-mgmt']}
              >
                <option value="wpa-psk">WPA/WPA2 Personal (PSK)</option>
                <option value="none">Open network</option>
              </Select>
            </div>
            {#if editConnectionData['802-11-wireless-security.key-mgmt'] === 'wpa-psk'}
              <Input
                label="Wi-Fi Password"
                type="password"
                bind:value={wifiPassword}
                placeholder="8–63 characters"
              />
            {/if}
          </div>
        {/if}

        {#if selectedConnectionUuid === '' && editConnectionData['connection.type'] === 'bridge'}
          <div class="type-specific-grid">
            <Input
              label="Bridge Interface Name"
              bind:value={editConnectionData['connection.interface-name']}
              placeholder="br0"
            />
            <div class="form-group">
              <span class="form-label">Port Interface</span>
              <Select bind:value={bridgePort}>
                <option value="">Choose an interface…</option>
                {#each interfaces as iface (iface.ifindex)}
                  {#if !isLoopbackInterface(iface.ifname) && iface.ifname !== editConnectionData['connection.interface-name']}
                    <option value={iface.ifname}>{iface.ifname} — {iface.link_type}</option>
                  {/if}
                {/each}
              </Select>
              <span class="field-hint">
                The bridge gets the IP configuration; the port becomes its slave.
              </span>
            </div>
          </div>
        {/if}
        
        <div class="network-columns-layout">
          <!-- ══ LEFT COLUMN: IPv4 & SUBNET CALCULATOR ════════════════════════ -->
          <div class="net-column">
            <div class="section-title">
              <span class="section-tag">IPv4</span>
              <h4>IPv4 Addressing &amp; Subnetting</h4>
            </div>
            
            <div class="form-group">
              <label for="ipv4-method">Configuration Method</label>
              <Select id="ipv4-method" bind:value={editConnectionData['ipv4.method']}>
                <option value="auto">Automatic (DHCP) — Recommended for Home/Office</option>
                <option value="manual">Manual (Static IP &amp; Subnet Mask)</option>
                <option value="link-local">Link-Local Only (169.254.x.x)</option>
                <option value="disabled">Disabled</option>
              </Select>
            </div>
            
            {#if editConnectionData['ipv4.method'] === 'manual'}
              <!-- ── IP Address & CIDR Prefix Input ── -->
              <div class="form-group">
                <div class="label-with-badge">
                  <label for="ipv4-addr">Host IP Address &amp; CIDR Prefix</label>
                  {#if ipv4Validation.isValid}
                    <span class="status-chip chip-success"><Check size={11} /> {ipv4Validation.scope}</span>
                  {:else if ipv4Validation.raw}
                    <span class="status-chip chip-error"><AlertTriangle size={11} /> Invalid IP / Subnet</span>
                  {/if}
                </div>
                
                <div class="input-with-select">
                  <input 
                    id="ipv4-addr" 
                    type="text" 
                    class="form-input font-mono {ipv4Validation.errors.length > 0 ? 'input-error' : ipv4Validation.isValid ? 'input-valid' : ''}" 
                    bind:value={editConnectionData['ipv4.addresses']} 
                    placeholder="e.g. 192.168.1.10/24" 
                  />
                  
                  <select 
                    class="cidr-quick-select" 
                    value={ipv4Validation.cidr} 
                    onchange={(e) => handleMaskDropdownChange(parseInt(e.currentTarget.value, 10))}
                    title="Quick Subnet Mask Preset"
                  >
                    {#each COMMON_SUBNET_MASKS as sm}
                      <option value={sm.cidr}>{sm.label}</option>
                    {/each}
                  </select>
                </div>

                <!-- Instant Subnet Error Feedback -->
                {#if ipv4Validation.errors.length > 0}
                  <div class="field-error-box">
                    <AlertCircle size={13} class="flex-shrink-0" />
                    <span>{ipv4Validation.errors[0]}</span>
                  </div>
                {:else if ipv4Validation.warnings.length > 0}
                  <div class="field-warn-box">
                    <AlertTriangle size={13} class="flex-shrink-0" />
                    <span>{ipv4Validation.warnings[0]}</span>
                  </div>
                {:else}
                  <span class="field-hint">Format: <code>IP/CIDR</code> (e.g. <code>192.168.1.50/24</code> = Netmask <code>255.255.255.0</code>).</span>
                {/if}
              </div>

              <!-- ── Gateway Input with Subnet Cross-Check ── -->
              <div class="form-group">
                <div class="label-with-badge">
                  <label for="ipv4-gw">Default Gateway (Router IP)</label>
                  {#if editConnectionData['ipv4.gateway']}
                    {#if gatewayValidation.isValid && gatewayValidation.inSameSubnet}
                      <span class="status-chip chip-success"><Check size={11} /> In Subnet</span>
                    {:else}
                      <span class="status-chip chip-error"><XCircle size={11} /> Subnet Mismatch</span>
                    {/if}
                  {/if}
                </div>

                <input 
                  id="ipv4-gw" 
                  type="text" 
                  class="form-input font-mono {gatewayValidation.errors.length > 0 ? 'input-error' : (gatewayValidation.isValid && editConnectionData['ipv4.gateway']) ? 'input-valid' : ''}" 
                  bind:value={editConnectionData['ipv4.gateway']} 
                  placeholder="e.g. 192.168.1.1" 
                />

                <!-- Instant Gateway Subnet Error Feedback -->
                {#if gatewayValidation.errors.length > 0}
                  <div class="field-error-box gateway-error-box">
                    <div class="err-text">
                      <AlertCircle size={13} class="flex-shrink-0" />
                      <span>{gatewayValidation.errors[0]}</span>
                    </div>
                    {#if gatewayValidation.suggestedGateway && ipv4Validation.isValid}
                      <button type="button" class="btn-fix-gateway" onclick={() => fixGateway(gatewayValidation.suggestedGateway)}>
                        <Sparkles size={11} /> Auto-Fix: Set to {gatewayValidation.suggestedGateway}
                      </button>
                    {/if}
                  </div>
                {:else}
                  <span class="field-hint">Must belong to the same subnet (<code>{ipv4Validation.networkIp || '192.168.1.0'}/{ipv4Validation.cidr || 24}</code>).</span>
                {/if}
              </div>

              <!-- ── Live Subnet Inspector & Diagnostics Widget ── -->
              {#if ipv4Validation.isValid}
                <div class="subnet-inspector-card">
                  <div class="inspector-header">
                    <Sliders size={14} class="text-accent" />
                    <strong>Subnet Inspection &amp; Routing Table</strong>
                    <span class="inspector-badge">{ipv4Validation.scope}</span>
                  </div>

                  <div class="inspector-grid">
                    <div class="inspector-stat">
                      <span class="stat-label">Network ID</span>
                      <span class="stat-val font-mono">{ipv4Validation.networkIp}/{ipv4Validation.cidr}</span>
                    </div>
                    <div class="inspector-stat">
                      <span class="stat-label">Subnet Mask</span>
                      <span class="stat-val font-mono">{ipv4Validation.maskStr}</span>
                    </div>
                    <div class="inspector-stat">
                      <span class="stat-label">Usable Host Range</span>
                      <span class="stat-val font-mono text-accent">{ipv4Validation.firstUsableIp} – {ipv4Validation.lastUsableIp}</span>
                    </div>
                    <div class="inspector-stat">
                      <span class="stat-label">Usable Capacity</span>
                      <span class="stat-val">{ipv4Validation.usableHosts.toLocaleString()} Hosts</span>
                    </div>
                    <div class="inspector-stat">
                      <span class="stat-label">Broadcast Address</span>
                      <span class="stat-val font-mono">{ipv4Validation.broadcastIp}</span>
                    </div>
                    <div class="inspector-stat">
                      <span class="stat-label">Wildcard Mask</span>
                      <span class="stat-val font-mono">{ipv4Validation.wildcardStr}</span>
                    </div>
                  </div>
                </div>
              {/if}
            {/if}
            
            <!-- ── DNS Servers with 1-Click Presets ── -->
            {#if editConnectionData['ipv4.method'] !== 'disabled'}
              <div class="form-group">
                <div class="label-with-badge">
                  <label for="ipv4-dns">DNS Nameservers (Comma-Separated)</label>
                  {#if dnsValidation.errors.length > 0}
                    <span class="status-chip chip-error"><AlertTriangle size={11} /> Invalid DNS IP</span>
                  {/if}
                </div>

                <input 
                  id="ipv4-dns" 
                  type="text" 
                  class="form-input font-mono {dnsValidation.errors.length > 0 ? 'input-error' : ''}" 
                  bind:value={editConnectionData['ipv4.dns']} 
                  placeholder="e.g. 1.1.1.1, 1.0.0.1" 
                />

                {#if dnsValidation.errors.length > 0}
                  <div class="field-error-box">
                    <AlertCircle size={13} class="flex-shrink-0" />
                    <span>{dnsValidation.errors[0]}</span>
                  </div>
                {/if}

                <!-- Quick 1-Click DNS Presets -->
                <div class="dns-presets-row">
                  <span class="dns-preset-title">Fast Presets:</span>
                  {#each POPULAR_DNS_PRESETS as preset}
                    <button 
                      type="button" 
                      class="dns-preset-btn" 
                      onclick={() => applyDnsPreset(preset)}
                      title="{preset.name} ({preset.primary}, {preset.secondary}) — {preset.tag}"
                    >
                      <Zap size={11} /> {preset.name}
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
          
          <!-- ══ RIGHT COLUMN: IPv6 CONFIGURATION ══════════════════════════════ -->
          <div class="net-column">
            <div class="section-title">
              <span class="section-tag section-tag-purple">IPv6</span>
              <h4>IPv6 Next-Gen Configuration</h4>
            </div>
            
            <div class="form-group">
              <label for="ipv6-method">Configuration Method</label>
              <Select id="ipv6-method" bind:value={editConnectionData['ipv6.method']}>
                <option value="auto">Automatic (SLAAC / Router Advertisement)</option>
                <option value="dhcp">DHCPv6 Only</option>
                <option value="manual">Manual (Static IPv6 Address)</option>
                <option value="link-local">Link-Local Only (fe80::/64)</option>
                <option value="ignore">Disabled / Ignore</option>
              </Select>
            </div>

            {#if editConnectionData['ipv6.method'] === 'manual'}
              <div class="form-group">
                <label for="ipv6-addr">IPv6 Address &amp; Prefix Length</label>
                <input 
                  id="ipv6-addr" 
                  type="text" 
                  class="form-input font-mono {ipv6Validation.errors.length > 0 ? 'input-error' : ''}" 
                  bind:value={editConnectionData['ipv6.addresses']} 
                  placeholder="e.g. 2001:db8:1::10/64" 
                />
                {#if ipv6Validation.errors.length > 0}
                  <div class="field-error-box">
                    <AlertCircle size={13} class="flex-shrink-0" />
                    <span>{ipv6Validation.errors[0]}</span>
                  </div>
                {:else}
                  <span class="field-hint">Standard global unicast prefix is typically <code>/64</code>.</span>
                {/if}
              </div>

              <div class="form-group">
                <label for="ipv6-gw">IPv6 Gateway</label>
                <input 
                  id="ipv6-gw" 
                  type="text" 
                  class="form-input font-mono" 
                  bind:value={editConnectionData['ipv6.gateway']} 
                  placeholder="e.g. 2001:db8:1::1 or fe80::1" 
                />
              </div>

              <div class="form-group">
                <label for="ipv6-dns">IPv6 DNS Servers</label>
                <input
                  id="ipv6-dns"
                  type="text"
                  class="form-input font-mono {ipv6DnsValidation.errors.length > 0 ? 'input-error' : ''}"
                  bind:value={editConnectionData['ipv6.dns']}
                  placeholder="e.g. 2606:4700:4700::1111, 2001:4860:4860::8888"
                />
                {#if ipv6DnsValidation.errors.length > 0}
                  <div class="field-error-box">
                    <AlertCircle size={13} class="flex-shrink-0" />
                    <span>{ipv6DnsValidation.errors[0]}</span>
                  </div>
                {/if}
              </div>
            {/if}

            <div class="info-guide-box">
              <Info size={16} class="text-accent flex-shrink-0" />
              <div class="info-guide-text">
                <strong>Networking Tip:</strong>
                <span>NetworkManager dynamically generates on-link routing rules. For static IP setups, ensure your host IP address and gateway share the exact same network prefix mask.</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  {:else}
    <!-- ══════════════════════════════════════════════════════════════════════════
         MAIN NAVIGATION TABS
         ══════════════════════════════════════════════════════════════════════════ -->
    <div class="controls-row">
      <div class="tab-bar">
        <button class="tab-btn { activeTab === 'interfaces' ? 'active' : '' }" onclick={() => activeTab = 'interfaces'}>
          <Activity size={14} style="margin-right:6px" /> Physical Adapters
        </button>
        <button class="tab-btn { activeTab === 'connections' ? 'active' : '' }" onclick={() => activeTab = 'connections'}>
          <Network size={14} style="margin-right:6px" /> Connections ({connections.length})
        </button>
        <button class="tab-btn { activeTab === 'vpn' ? 'active' : '' }" onclick={() => activeTab = 'vpn'}>
          <Key size={14} style="margin-right:6px" /> VPN Profiles ({vpnProfiles.length})
        </button>
        <button class="tab-btn { activeTab === 'dns' ? 'active' : '' }" onclick={() => activeTab = 'dns'}>
          <Globe size={14} style="margin-right:6px" /> Global DNS
        </button>
        <button class="tab-btn { activeTab === 'speedtest' ? 'active' : '' }" onclick={() => activeTab = 'speedtest'}>
          <Wifi size={14} style="margin-right:6px" /> Speed Test
        </button>
      </div>

      <div class="tab-actions">
        <button class="header-refresh-btn" onclick={loadData} disabled={loading}>
          <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Refresh
        </button>
        {#if activeTab === 'connections'}
          <Button variant="primary" class="btn-sm" onclick={() => editConnection('')}>
            <Plus size={13}/> Add Connection
          </Button>
        {:else if activeTab === 'vpn'}
          <Button variant="outline" class="btn-sm" onclick={importVpnFile}>
            <FileUp size={13} style="margin-right:4px;" /> Import Profile
          </Button>
          <Button variant="primary" class="btn-sm" onclick={() => showVpnDrawer = true}>
            <Plus size={13} style="margin-right:4px;" /> Create VPN
          </Button>
        {/if}
      </div>
    </div>

    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:24px;">
      {#if loading}
        <div class="network-loading" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted);gap:10px;">
          <RefreshCw size={24} class="animate-spin-slow" />
          <span>Loading network information…</span>
        </div>
      {:else if loadError}
        <EmptyState
          icon={AlertCircle}
          title="Network information unavailable"
          description={loadError}
          actionLabel="Try again"
          onAction={loadData}
        />
      {:else if activeTab === 'interfaces'}
        <div class="sub-toolbar">
          <div class="sub-toolbar-left">
            <SearchBar
              bind:value={interfaceFilter}
              placeholder="Search interfaces, MAC, IP, type…"
              count={interfaceFilter ? filteredInterfaces.length : undefined}
              style="width: 300px; max-width: 100%;"
            />
          </div>
          <span class="toolbar-count">
            {filteredInterfaces.length === interfaces.length
              ? `${interfaces.length} interface${interfaces.length === 1 ? '' : 's'}`
              : `${filteredInterfaces.length} of ${interfaces.length}`}
          </span>
        </div>

        {#if filteredInterfaces.length === 0}
          <EmptyState
            icon={Network}
            title={interfaces.length === 0 ? 'No network interfaces' : 'No matching interfaces'}
            description={interfaces.length === 0 ? 'No network interfaces were reported by ip.' : 'Try a different search term.'}
            actionLabel={interfaces.length === 0 ? 'Refresh' : 'Clear search'}
            onAction={() => (interfaces.length === 0 ? loadData() : (interfaceFilter = ''))}
          />
        {:else}
          <div class="interface-grid">
            {#each filteredInterfaces as iface (iface.ifindex)}
              {@const loopback = isLoopbackInterface(iface.ifname)}
              <div
                class="card interface-card"
                role="group"
                aria-label="Interface {iface.ifname}"
                oncontextmenu={(e) => openInterfaceMenu(e, iface)}
              >
                <div class="card-head">
                  <div class="card-head-left">
                    <span class="card-icon"><Hash size={15} /></span>
                    <span class="card-title">{iface.ifname}</span>
                    {#if loopback}<Badge variant="info">loopback</Badge>{/if}
                  </div>
                  <div class="card-head-right">
                    <span class="badge {iface.operstate === 'UP' ? 'badge-success' : (iface.operstate === 'UNKNOWN' ? 'badge-warning' : 'badge-muted')}">
                      {iface.operstate}
                    </span>
                    <button
                      type="button"
                      class="row-menu-btn"
                      onclick={(e) => { e.stopPropagation(); openInterfaceMenu(e, iface); }}
                      title="Interface actions"
                      aria-label="Interface actions for {iface.ifname}"
                    >
                      <MoreHorizontal size={15} />
                    </button>
                  </div>
                </div>

                <div class="card-kv-list">
                  <div class="card-kv">
                    <span>MAC address</span>
                    <span class="mono-value">{iface.address === '00:00:00:00:00:00' ? 'N/A' : iface.address}</span>
                  </div>
                  <div class="card-kv">
                    <span>MTU</span>
                    <span class="mono-value">{iface.mtu}</span>
                  </div>
                  <div class="card-kv">
                    <span>Type</span>
                    <span class="mono-value">{iface.link_type}</span>
                  </div>
                </div>

                {#if iface.addr_info && iface.addr_info.length > 0}
                  <div class="card-address-list">
                    {#each iface.addr_info as addr}
                      <div class="address-row">
                        <span class="addr-family">{addr.family}</span>
                        <span class="addr-value">{addr.local}/{addr.prefixlen}</span>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <div class="card-empty-note">No IP addresses assigned</div>
                {/if}

                <div class="card-actions-row">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={loopback}
                    onclick={() => setInterfaceState(iface.ifname, iface.operstate !== 'UP')}
                    title={loopback ? 'Loopback is protected' : 'Change interface state'}
                  >
                    <Power size={12} />
                    {iface.operstate === 'UP' ? 'Disable' : 'Enable'}
                  </Button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {:else if activeTab === 'connections'}
        <div class="sub-toolbar">
          <div class="sub-toolbar-left">
            <SearchBar
              bind:value={connectionFilter}
              placeholder="Search profiles, UUID, type, device…"
              count={connectionFilter || connectionStatusFilter !== 'all' ? filteredConnections.length : undefined}
              style="width: 300px; max-width: 100%;"
            />
            <FilterChips
              options={connectionStatusOptions}
              bind:value={connectionStatusFilter}
              label="Filter connections by state"
            />
          </div>
          <span class="toolbar-count">
            {filteredConnections.length === connections.length
              ? `${connections.length} profile${connections.length === 1 ? '' : 's'}`
              : `${filteredConnections.length} of ${connections.length}`}
          </span>
        </div>

        {#if connections.length === 0}
          <EmptyState
            icon={Network}
            title="No connection profiles"
            description="Create a wired connection profile to configure addressing manually."
            actionLabel="Add connection"
            onAction={() => editConnection('')}
          />
        {:else if filteredConnections.length === 0}
          <EmptyState
            icon={Search}
            title="No matching profiles"
            description="No connection profiles match the current search and state filter."
            actionLabel="Clear filters"
            onAction={() => { connectionFilter = ''; connectionStatusFilter = 'all'; }}
          />
        {:else}
          <div class="connection-grid">
            {#each filteredConnections as conn (conn.uuid)}
              {@const loopback = isLoopbackConnection(conn)}
              <div
                class="card connection-card"
                tabindex="0"
                role="button"
                onclick={() => editConnection(conn.uuid)}
                onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); editConnection(conn.uuid); } }}
                oncontextmenu={(e) => openConnectionMenu(e, conn)}
              >
                <div class="card-head">
                  <div class="card-head-left">
                    <span class="card-icon"><Network size={15} /></span>
                    <span class="card-title">{conn.name}</span>
                    {#if loopback}<Badge variant="info">loopback</Badge>{/if}
                  </div>
                  <div class="card-head-right">
                    <span class="badge {conn.state === 'activated' ? 'badge-success' : 'badge-muted'}">{conn.state || 'inactive'}</span>
                    <button
                      type="button"
                      class="row-menu-btn"
                      onclick={(e) => { e.stopPropagation(); openConnectionMenu(e, conn); }}
                      title="Connection actions"
                      aria-label="Connection actions for {conn.name}"
                    >
                      <MoreHorizontal size={15} />
                    </button>
                  </div>
                </div>
                <div class="connection-meta-row">
                  <span>Type <strong>{conn.type}</strong></span>
                  <span>Device <code>{conn.device || 'N/A'}</code></span>
                </div>
                <div class="card-actions-row">
                  <span class="card-open-hint">Configure IP &amp; subnet <CornerDownRight size={12} /></span>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {:else if activeTab === 'vpn'}
        <div class="vpn-layout">
          <!-- Header band: context + count + filter -->
          <div class="vpn-header-band">
            <span class="vpn-header-chip"><Key size={14} /></span>
            <span class="vpn-header-title">VPN Profiles</span>
            <Badge variant="muted">
              {filteredVpnProfiles.length}
              {filteredVpnProfiles.length === 1 ? 'profile' : 'profiles'}
            </Badge>
            <div class="vpn-header-actions">
              <SearchBar
                bind:value={vpnFilter}
                placeholder="Filter profiles…"
                style="width: 220px"
              />
            </div>
          </div>

          {#if loadingVpn}
            <div class="vpn-loading">
              <RefreshCw size={22} class="animate-spin-slow" />
              <span>Loading VPN profiles…</span>
            </div>
          {:else if vpnProfiles.length === 0}
            <div class="vpn-empty">
              <EmptyState
                icon={Key}
                title="No VPN Profiles"
                description="Import an OpenVPN (.ovpn) or WireGuard (.conf) file, or create a profile manually."
                actionLabel="Create VPN"
                actionIcon={Plus}
                onAction={() => (showVpnDrawer = true)}
              />
            </div>
          {:else if filteredVpnProfiles.length === 0}
            <div class="vpn-empty">
              <EmptyState
                icon={Info}
                title="No profiles match your filter"
                description={`Nothing matched "${vpnFilter}". Try a different term.`}
                actionLabel="Clear filter"
                onAction={() => (vpnFilter = '')}
              />
            </div>
          {:else}
            <div class="vpn-grid">
              {#each filteredVpnProfiles as vpn (vpn.uuid)}
                <div class="vpn-card" class:is-active={vpn.active}>
                  <div class="vpn-card-top">
                    <div class="vpn-card-id">
                      <span class="vpn-card-icon">
                        {#if vpn.vpn_type === 'wireguard'}<ShieldCheck size={14} />{:else}<Key size={14} />{/if}
                      </span>
                      <div class="vpn-card-name-wrap">
                        <span class="vpn-card-name" title={vpn.name}>{vpn.name}</span>
                        <span class="vpn-card-proto">{vpnProtocolLabel(vpn)}</span>
                      </div>
                    </div>
                    <Badge variant={vpn.active ? 'success' : 'muted'}>
                      {vpn.active ? 'connected' : 'inactive'}
                    </Badge>
                  </div>

                  <div class="vpn-card-meta">
                    {#if vpn.active && vpn.device}
                      <span class="vpn-meta-item">
                        <Laptop size={11} /> <code>{vpn.device}</code>
                      </span>
                    {:else}
                      <span class="vpn-meta-item vpn-meta-dim">
                        <Link size={11} /> Not connected
                      </span>
                    {/if}
                  </div>

                  <div class="vpn-card-actions">
                    <Button
                      variant="ghost"
                      size="sm"
                      onclick={() => deleteConnection(vpn.uuid, vpn.name)}
                      title="Delete profile"
                      aria-label="Delete {vpn.name}"
                    >
                      <Trash2 size={13} />
                    </Button>
                    {#if vpn.active}
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={vpnBusyUuid === vpn.uuid}
                        onclick={() => disconnectVpn(vpn.uuid)}
                      >
                        {#if vpnBusyUuid === vpn.uuid}
                          <RefreshCw size={12} class="animate-spin-slow" /> Disconnecting
                        {:else}
                          <Square size={12} /> Disconnect
                        {/if}
                      </Button>
                    {:else}
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={vpnBusyUuid === vpn.uuid}
                        onclick={() => connectVpn(vpn.uuid)}
                      >
                        {#if vpnBusyUuid === vpn.uuid}
                          <RefreshCw size={12} class="animate-spin-slow" /> Connecting
                        {:else}
                          <Play size={12} /> Connect
                        {/if}
                      </Button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {:else if activeTab === 'speedtest'}
        <div class="card" style="display:flex; flex-direction:column; align-items:center; gap:24px; padding:32px;">
          <h3 style="margin:0; color:var(--color-text-primary); font-size:18px; text-align:center;">Network Speed Test</h3>
          <p style="margin:0; font-size:13px; color:var(--color-text-secondary); text-align:center; max-width:480px;">
            Measure latency, download speed, and upload speed against Cloudflare's public speed endpoint.
            The test downloads 5 MB and uploads 2 MB of data.
          </p>
          
          {#if speedTestError}
            <div style="color:var(--color-error); background:rgba(255,71,87,0.1); border:1px solid var(--color-error); padding:12px 16px; border-radius:6px; font-size:13px; text-align:center; width:100%; max-width:480px;">
              Error: {speedTestError}
            </div>
          {/if}
          
          <div style="display:flex; justify-content:center; align-items:center; position:relative; width:220px; height:220px; border-radius:50%; background: radial-gradient(circle, var(--color-bg-raised) 60%, rgba(0,0,0,0.3) 100%); border:4px solid var(--color-border); margin:12px 0; box-shadow:0 10px 30px rgba(0,0,0,0.3);">
            <div style="text-align:center; z-index:2; display:flex; flex-direction:column; align-items:center; gap:6px;">
              {#if isTestingSpeed}
                <RefreshCw size={36} class="animate-spin-slow" style="color:var(--color-accent); margin-bottom:8px;" />
                <span style="font-size:12px; font-weight:700; text-transform:uppercase; color:var(--color-text-muted); letter-spacing:1px;">
                  {#if speedProgress === 'ping'}
                    Checking Latency
                  {:else if speedProgress === 'download'}
                    Testing Download
                  {:else}
                    Testing Upload
                  {/if}
                </span>
              {:else if speedDownload !== null}
                <span style="font-size:38px; font-weight:800; color:var(--color-text-primary); font-family:var(--font-mono); line-height:1;">
                  {speedDownload.toFixed(1)}
                </span>
                <span style="font-size:11px; font-weight:700; color:var(--color-accent); letter-spacing:1px; text-transform:uppercase;">
                  Mbps Download
                </span>
              {:else}
                <Wifi size={36} style="color:var(--color-text-muted); margin-bottom:8px;" />
                <span style="font-size:12px; font-weight:600; color:var(--color-text-secondary);">Ready</span>
              {/if}
            </div>
            
            {#if isTestingSpeed}
              <div style="position:absolute; top:-4px; left:-4px; right:-4px; bottom:-4px; border-radius:50%; border:4px solid transparent; border-top-color:var(--color-accent); animation: spin 1.5s linear infinite;"></div>
            {/if}
          </div>
          
          <div style="display:grid; grid-template-columns: repeat(3, 1fr); gap:16px; width:100%; max-width:540px; margin-top:8px;">
            <div class="card" style="display:flex; flex-direction:column; align-items:center; padding:16px 12px; gap:6px; background:rgba(255,255,255,0.02); text-align:center;">
              <span style="font-size:11px; font-weight:600; color:var(--color-text-muted); text-transform:uppercase;">Latency</span>
              <strong style="font-size:18px; color:var(--color-text-primary); font-family:var(--font-mono);">
                {speedPing !== null ? speedPing.toFixed(0) + ' ms' : '—'}
              </strong>
            </div>
            <div class="card" style="display:flex; flex-direction:column; align-items:center; padding:16px 12px; gap:6px; background:rgba(255,255,255,0.02); text-align:center;">
              <span style="font-size:11px; font-weight:600; color:var(--color-text-muted); text-transform:uppercase;">Download</span>
              <strong style="font-size:18px; color:var(--color-text-primary); font-family:var(--font-mono);">
                {speedDownload !== null ? speedDownload.toFixed(1) + ' Mbps' : '—'}
              </strong>
            </div>
            <div class="card" style="display:flex; flex-direction:column; align-items:center; padding:16px 12px; gap:6px; background:rgba(255,255,255,0.02); text-align:center;">
              <span style="font-size:11px; font-weight:600; color:var(--color-text-muted); text-transform:uppercase;">Upload</span>
              <strong style="font-size:18px; color:var(--color-text-primary); font-family:var(--font-mono);">
                {speedUpload !== null ? speedUpload.toFixed(1) + ' Mbps' : '—'}
              </strong>
            </div>
          </div>
          
          <Button variant="primary" class="" style="width:100%; max-width:240px; margin-top:8px; display:flex; align-items:center; justify-content:center; gap:8px;" disabled={isTestingSpeed} onclick={runSpeedTest}>
            <Play size={14} /> {isTestingSpeed ? 'Testing...' : 'Start Speed Test'}
          </Button>
        </div>
      {:else if activeTab === 'dns'}
        <div class="card" style="display:flex; flex-direction:column; height: 100%;">
          <div class="dns-card-head">
            <div>
              <h3 style="margin:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
                <Server size={18} style="color:var(--color-info)" /> System DNS Configuration
              </h3>
              <p style="font-size:13px; color:var(--color-text-secondary); margin:6px 0 0;">
                Current DNS resolution settings from systemd-resolved or /etc/resolv.conf.
              </p>
            </div>
            <div class="card-actions-row" style="margin:0;">
              <Button variant="ghost" size="sm" onclick={() => copyText(dnsInfo, 'DNS configuration copied')}>
                <Copy size={13} /> Copy
              </Button>
              <Button variant="outline" size="sm" onclick={loadData}>
                <RefreshCw size={13} /> Refresh
              </Button>
            </div>
          </div>
          <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:8px; padding:12px; overflow:auto; flex:1;">
            <pre style="margin:0; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); white-space:pre-wrap;">{dnsInfo}</pre>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <ContextMenu
    bind:isOpen={connectionMenu.open}
    x={connectionMenu.x}
    y={connectionMenu.y}
    icon={Network}
    title={connectionMenu.conn?.name ?? ''}
    subtitle={connectionMenu.conn
      ? `${connectionMenu.conn.type} · ${connectionMenu.conn.device || 'no device'}`
      : ''}
    badge={connectionMenu.conn
      ? {
          text: connectionMenu.conn.state === 'activated' ? 'Active' : 'Inactive',
          variant: connectionMenu.conn.state === 'activated' ? 'success' : 'muted',
        }
      : undefined}
    items={connectionMenuItems}
    minWidth="280px"
  />

  <ContextMenu
    bind:isOpen={interfaceMenu.open}
    x={interfaceMenu.x}
    y={interfaceMenu.y}
    icon={Hash}
    title={interfaceMenu.iface?.ifname ?? ''}
    subtitle={interfaceMenu.iface
      ? `${interfaceMenu.iface.link_type} · ${interfaceMenu.iface.operstate}`
      : ''}
    badge={interfaceMenu.iface
      ? {
          text: interfaceMenu.iface.operstate,
          variant: interfaceMenu.iface.operstate === 'UP' ? 'success' : 'muted',
        }
      : undefined}
    items={interfaceMenuItems}
    minWidth="250px"
  />

  <SideDrawer bind:isOpen={showVpnDrawer} title="Create VPN Profile" width="520px">
    <div class="vpn-drawer-form">
      <div class="form-group">
        <label for="vpn-name">Profile Name <span class="req">*</span></label>
        <input id="vpn-name" type="text" class="form-input" bind:value={vpnForm.name} placeholder="e.g. Work VPN" />
      </div>
      
      <div class="form-group">
        <label for="vpn-type-select">VPN Protocol</label>
        <Select id="vpn-type-select" bind:value={vpnForm.type}>
          <option value="openvpn">OpenVPN</option>
          <option value="wireguard">WireGuard</option>
        </Select>
      </div>
      
      <div class="form-group">
        <label for="vpn-gateway">
          {vpnForm.type === 'openvpn' ? 'Gateway Address' : 'Peer Endpoint (e.g. 198.51.100.1:51820)'} <span class="req">*</span>
        </label>
        <input id="vpn-gateway" type="text" class="form-input" bind:value={vpnForm.gateway} placeholder={vpnForm.type === 'openvpn' ? 'vpn.example.com' : '198.51.100.1:51820'} />
      </div>
      
      {#if vpnForm.type === 'wireguard'}
        <div class="form-group">
          <label for="vpn-peer-key">Peer Public Key <span class="req">*</span></label>
          <input
            id="vpn-peer-key"
            type="text"
            class="form-input mono-field"
            bind:value={vpnForm.peerPublicKey}
            placeholder="yjhphUwefv4p4ey2z6mMWnqsJBYo3oV0aivdUSi8Nq0="
            spellcheck="false"
            autocomplete="off"
          />
          <span class="form-hint">
            The server's public key (<code>wg show</code> on the server), as a 44-character base64 key.
            Without it the tunnel cannot authenticate and will never connect.
          </span>
        </div>
        <div class="form-group">
          <label for="vpn-address">Interface Address <span class="req">*</span></label>
          <input
            id="vpn-address"
            type="text"
            class="form-input mono-field"
            bind:value={vpnForm.address}
            placeholder="10.8.0.2/32"
            spellcheck="false"
            autocomplete="off"
          />
          <span class="form-hint">
            The address assigned to this client inside the tunnel, from your server's
            <code>AllowedIPs</code>/peer configuration.
          </span>
        </div>
      {:else}
        <div class="form-group">
          <label for="vpn-username">Username (Optional)</label>
          <input id="vpn-username" type="text" class="form-input" bind:value={vpnForm.username} placeholder="vpnuser" />
        </div>
        <div class="form-group">
          <label for="vpn-password">Password (Optional)</label>
          <input id="vpn-password" type="password" class="form-input" bind:value={vpnForm.password} placeholder="••••••••" />
        </div>

        <div class="form-group">
          <span class="form-label">Certificates (Optional)</span>
          <span class="form-hint">
            Importing a <code>.ovpn</code> file fills these in automatically. For a manual profile they are
            required for the tunnel to authenticate.
          </span>
          <div class="cert-row">
            <input class="form-input mono-field" value={vpnForm.caCert} placeholder="CA certificate" readonly />
            <Button variant="outline" size="sm" onclick={() => pickVpnFile('caCert')} title="Choose CA certificate">
              <FileUp size={13} />
            </Button>
          </div>
          <div class="cert-row">
            <input class="form-input mono-field" value={vpnForm.clientCert} placeholder="Client certificate" readonly />
            <Button variant="outline" size="sm" onclick={() => pickVpnFile('clientCert')} title="Choose client certificate">
              <FileUp size={13} />
            </Button>
          </div>
          <div class="cert-row">
            <input class="form-input mono-field" value={vpnForm.clientKey} placeholder="Client key" readonly />
            <Button variant="outline" size="sm" onclick={() => pickVpnFile('clientKey')} title="Choose client key">
              <FileUp size={13} />
            </Button>
          </div>
        </div>
      {/if}
      
      <div class="drawer-footer-row">
        <Button variant="outline" onclick={() => showVpnDrawer = false}>Cancel</Button>
        <Button variant="primary" onclick={handleCreateVpn} disabled={!vpnFormValid}>Create Profile</Button>
      </div>
    </div>
  </SideDrawer>
</div>

<style>
  /* ── VPN Profiles tab ────────────────────────────────────────── */
  .vpn-layout {
    display: flex;
    flex-direction: column;
    gap: 14px;
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  /* Header band — raised surface so it reads as a header, not as content */
  .vpn-header-band {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    flex-shrink: 0;
  }

  .vpn-header-chip {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--color-module-icon-bg);
    border: 1px solid var(--color-module-icon-border);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .vpn-header-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
  }

  .vpn-header-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .vpn-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 14px;
  }

  .vpn-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    transition: border-color 0.15s ease;
  }

  .vpn-card:hover {
    border-color: var(--color-border-hover);
  }

  /* A connected profile is marked with an accent rail, not just a badge */
  .vpn-card.is-active {
    border-color: var(--color-accent-glow);
    box-shadow: inset 3px 0 0 var(--color-accent);
  }

  .vpn-card-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .vpn-card-id {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
  }

  .vpn-card-icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .vpn-card.is-active .vpn-card-icon {
    background: var(--color-accent-muted);
    border: 1px solid var(--color-accent-glow);
    color: var(--color-accent);
  }

  .vpn-card-name-wrap {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .vpn-card-name {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .vpn-card-proto {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .vpn-card-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 11.5px;
    color: var(--color-text-secondary);
  }

  .vpn-meta-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .vpn-meta-item code {
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .vpn-meta-dim {
    color: var(--color-text-muted);
  }

  .vpn-card-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    border-top: 1px solid var(--color-border-subtle);
    padding-top: 10px;
  }

  .vpn-empty {
    padding: 20px;
  }

  .vpn-loading {
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

  /* ── Create VPN drawer ──────────────────────────────────────── */
  .vpn-drawer-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 4px 0;
  }

  .req {
    color: var(--color-error);
    font-weight: 700;
  }

  .mono-field {
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .form-hint {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.5;
  }

  .form-hint code {
    font-family: var(--font-mono);
    font-size: 10.5px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--color-bg-raised);
  }

  .cert-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }

  .cert-row .form-input {
    flex: 1;
    min-width: 0;
  }

  .drawer-footer-row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  /* ── Layout & Structure ─────────────────────────────────────────────────── */
  .editor-header-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    flex-shrink: 0;
  }

  .editor-actions {
    display: flex;
    gap: 8px;
  }

  .editor-scroll-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 32px;
  }

  .editor-main-card {
    display: flex;
    flex-direction: column;
    gap: 20px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 20px;
  }

  :global(html.light-mode) .editor-main-card {
    background: #FFFFFF;
    border-color: #E2E8F0;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .card-title-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 12px;
  }

  :global(html.light-mode) .card-title-row {
    border-bottom-color: #E2E8F0;
  }

  .card-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-title-group h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .connection-options-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
  }

  .connection-toggle-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .type-specific-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
  }

  .form-grid-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .network-columns-layout {
    display: grid;
    grid-template-columns: 1.2fr 0.8fr;
    gap: 24px;
  }

  @media (max-width: 1000px) {
    .network-columns-layout {
      grid-template-columns: 1fr;
    }
  }

  .net-column {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--color-border);
  }

  :global(html.light-mode) .section-title {
    border-bottom-color: #E2E8F0;
  }

  .section-title h4 {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .section-tag {
    font-size: 10px;
    font-weight: 800;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
    letter-spacing: 0.5px;
  }

  .section-tag-purple {
    background: rgba(168, 85, 247, 0.15);
    color: #A855F7;
  }

  /* ── Form Groups & Inputs ───────────────────────────────────────────────── */
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .form-group label,
  .form-group .form-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  :global(html.light-mode) .form-group label,
  :global(html.light-mode) .form-group .form-label {
    color: #475569;
  }

  .static-value {
    display: flex;
    align-items: center;
    min-height: 38px;
    padding: 9px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
    color: var(--color-text-primary);
    font-size: 13px;
  }

  .label-with-badge {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .form-input {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 9px 12px;
    font-size: 13px;
    color: var(--color-text-primary);
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .form-input {
    background: #F8FAFC;
    border-color: #CBD5E1;
    color: #0F172A;
  }

  .form-input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(0, 218, 243, 0.2);
  }

  .form-input.input-error {
    border-color: var(--color-error) !important;
    background: rgba(239, 68, 68, 0.05) !important;
  }

  .form-input.input-valid {
    border-color: var(--color-success) !important;
  }

  .input-with-select {
    display: flex;
    gap: 8px;
  }

  .input-with-select input {
    flex: 1;
  }

  .cidr-quick-select {
    width: 200px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 10px;
    font-size: 12px;
    color: var(--color-text-primary);
    cursor: pointer;
  }

  :global(html.light-mode) .cidr-quick-select {
    background: #F8FAFC;
    border-color: #CBD5E1;
    color: #0F172A;
  }

  .field-hint {
    font-size: 11.5px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .field-hint code {
    background: rgba(255, 255, 255, 0.08);
    padding: 1px 4px;
    border-radius: 4px;
    font-family: var(--font-mono);
  }

  /* ── Error & Warning Callouts ───────────────────────────────────────────── */
  .net-alert-box {
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .net-alert-error {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--color-error);
    color: var(--color-error);
  }

  .net-alert-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
  }

  .net-error-list {
    margin: 0;
    padding-left: 24px;
    font-size: 12.5px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .net-error-list {
    color: #991B1B;
  }

  .field-error-box {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 11.5px;
    color: var(--color-error);
    margin-top: 2px;
  }

  :global(html.light-mode) .field-error-box {
    background: #FEF2F2;
    border-color: #FECACA;
    color: #B91C1C;
  }

  .gateway-error-box {
    flex-direction: column;
    gap: 6px;
  }

  .gateway-error-box .err-text {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }

  .btn-fix-gateway {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--color-accent);
    color: #000000;
    font-weight: 700;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 4px;
    border: none;
    cursor: pointer;
    margin-top: 2px;
    transition: opacity 0.15s ease;
  }

  .btn-fix-gateway:hover {
    opacity: 0.9;
  }

  .field-warn-box {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    background: rgba(245, 158, 11, 0.12);
    border: 1px solid rgba(245, 158, 11, 0.3);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 11.5px;
    color: var(--color-warning);
    margin-top: 2px;
  }

  /* ── Status Chips ───────────────────────────────────────────────────────── */
  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 4px;
  }

  .chip-success {
    background: rgba(16, 185, 129, 0.15);
    color: var(--color-success);
  }

  .chip-error {
    background: rgba(239, 68, 68, 0.15);
    color: var(--color-error);
  }

  /* ── Live Subnet Inspector Card ─────────────────────────────────────────── */
  .subnet-inspector-card {
    background: rgba(0, 218, 243, 0.03);
    border: 1px solid rgba(0, 218, 243, 0.2);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  :global(html.light-mode) .subnet-inspector-card {
    background: #F0F9FF;
    border-color: #BAE6FD;
  }

  .inspector-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .inspector-badge {
    margin-left: auto;
    font-size: 10.5px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 10px;
    background: rgba(0, 218, 243, 0.15);
    color: var(--color-accent);
  }

  .inspector-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }

  @media (max-width: 720px) {
    .inspector-grid {
      grid-template-columns: 1fr 1fr;
    }
  }

  .inspector-stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: rgba(0, 0, 0, 0.2);
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
  }

  :global(html.light-mode) .inspector-stat {
    background: #FFFFFF;
    border-color: #E2E8F0;
  }

  .stat-label {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .stat-val {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  /* ── DNS Presets ────────────────────────────────────────────────────────── */
  .dns-presets-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
  }

  .dns-preset-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
  }

  .dns-preset-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .dns-preset-btn {
    background: #F1F5F9;
    border-color: #CBD5E1;
    color: #334155;
  }

  .dns-preset-btn:hover {
    background: rgba(0, 218, 243, 0.15);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  /* ── Info Box ───────────────────────────────────────────────────────────── */
  .info-guide-box {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px;
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  :global(html.light-mode) .info-guide-box {
    background: #F8FAFC;
    border-color: #E2E8F0;
    color: #475569;
  }

  .info-guide-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .info-guide-text strong {
    color: var(--color-text-primary);
  }

  .dns-card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    margin-bottom: 16px;
  }

  /* ── Tabs: search, filters and cards ───────────────────────── */
  .sub-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .sub-toolbar-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .toolbar-count {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .network-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-bg-card);
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .interface-grid,
  .connection-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }

  .interface-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 9px;
  }

  .card-head-left,
  .card-head-right {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .card-icon {
    display: grid;
    place-items: center;
    width: 27px;
    height: 27px;
    border-radius: 7px;
    background: var(--color-accent-muted);
    border: 1px solid var(--color-border);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .card-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-kv-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12.5px;
    color: var(--color-text-secondary);
  }

  .card-kv {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .mono-value {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-address-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-top: 1px dashed var(--color-border);
    padding-top: 8px;
  }

  .address-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .addr-family {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
  }

  .addr-value {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-empty-note {
    border-top: 1px dashed var(--color-border);
    padding-top: 8px;
    text-align: center;
    font-size: 11.5px;
    font-style: italic;
    color: var(--color-text-muted);
  }

  .card-actions-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: auto;
    padding-top: 4px;
  }

  .connection-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .connection-meta-row code {
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .card-open-hint {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-accent);
  }

  .row-menu-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    flex-shrink: 0;
  }

  .row-menu-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .row-menu-btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }

  /* ── Connection Cards ───────────────────────────────────────────────────── */
  .connection-card {
    cursor: pointer;
    transition: all 0.18s ease;
    border: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .connection-card:hover {
    border-color: var(--color-accent);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  }

  /* ── Outline Buttons ────────────────────────────────────────────────────── */
  :global(.btn-warn-outline) {
    border-color: var(--color-warning) !important;
    color: var(--color-warning) !important;
  }

  :global(.btn-danger-outline) {
    border-color: var(--color-error) !important;
    color: var(--color-error) !important;
  }

  .font-mono {
    font-family: var(--font-mono);
  }

  .text-accent { color: var(--color-accent); }
</style>
