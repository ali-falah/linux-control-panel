<script lang="ts">
  import { onMount } from 'svelte';
  import { 
    Activity, Wifi, Globe, RefreshCw, Server, Hash, Network, Plus, ArrowLeft, Save, Trash2,
    Play, Square, FileUp, ShieldCheck, ShieldAlert, Key, Link, AlertTriangle, CheckCircle2,
    XCircle, Info, Sliders, Zap, Check, Copy, Sparkles, Cpu, Laptop, Radio, ExternalLink,
    AlertCircle, CornerDownRight, CheckCheck, HelpCircle
  } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import Button from '../components/ui/Button.svelte';
  import Select from '../components/ui/Select.svelte';
  import { 
    validateIpv4WithCidr, 
    validateGateway, 
    validateDnsServers, 
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
  let loadingVpn = $state(false);
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
  let isSaving = $state(false);

  // Manual VPN form
  let showVpnDrawer = $state(false);
  let vpnForm = $state({
    name: 'My VPN Connection',
    type: 'openvpn', // 'openvpn' or 'wireguard'
    gateway: '',
    username: '',
    password: ''
  });
  
  // Speed Test States
  let isTestingSpeed = $state(false);
  let speedProgress = $state(''); // 'ping' | 'download' | 'upload' | 'done' | ''
  let speedPing = $state<number | null>(null);
  let speedDownload = $state<number | null>(null);
  let speedUpload = $state<number | null>(null);
  let speedTestError = $state('');

  // ─── Real-Time Network & Subnet Calculations (Svelte 5 Reactive Runes) ───────

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
    return validateIpv4WithCidr(editConnectionData['ipv4.addresses'] || '');
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
    return validateIpv6Address(editConnectionData['ipv6.addresses'] || '', editConnectionData['ipv6.gateway'] || '');
  });

  let hasBlockingErrors = $derived.by<boolean>(() => {
    if (!editConnectionData) return false;
    if (editConnectionData['ipv4.method'] === 'manual') {
      if (!ipv4Validation.isValid || !gatewayValidation.isValid || !dnsValidation.isValid) {
        return true;
      }
    }
    if (editConnectionData['ipv6.method'] === 'manual') {
      if (!ipv6Validation.isValid) return true;
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

  async function loadVpnProfiles() {
    loadingVpn = true;
    try {
      vpnProfiles = await invoke<any[]>('network_get_vpn_profiles');
    } catch(e) {
      console.error("Failed to load VPN profiles", e);
    } finally {
      loadingVpn = false;
    }
  }

  async function loadData() {
    loading = true;
    selectedConnectionUuid = null;
    statusStore.setBusy('Loading network information…');
    try {
      const ifacesRaw: string = await invoke('network_get_interfaces');
      interfaces = JSON.parse(ifacesRaw);
      dnsInfo = await invoke('network_get_dns');
      
      const connsRaw: string = await invoke('network_list_connections');
      connections = connsRaw.split('\n').filter(l => l.trim()).map(line => {
        const parts = line.split(':');
        return {
          uuid: parts[0],
          name: parts[1],
          type: parts[2],
          device: parts[3],
          state: parts[4]
        };
      });

      await loadVpnProfiles();
      statusStore.setLastCommand('nmcli connection show', 0, true);
    } catch (e) {
      console.error(e);
      statusStore.setLastCommand('nmcli connection show', 1, false);
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
        'ipv4.method': 'auto', 
        'ipv6.method': 'auto', 
        'ipv4.addresses': '', 
        'ipv4.gateway': '', 
        'ipv4.dns': '',
        'ipv6.addresses': '',
        'ipv6.gateway': '',
        'ipv6.dns': ''
      };
      selectedConnectionUuid = uuid;
      return;
    }
    
    statusStore.setBusy('Loading connection properties…');
    try {
      const raw: string = await invoke('network_get_connection', { uuid });
      const parsed: any = {};
      raw.split('\n').forEach(line => {
        const idx = line.indexOf(':');
        if (idx > -1) {
          parsed[line.slice(0, idx)] = line.slice(idx + 1);
        }
      });
      editConnectionData = parsed;
      selectedConnectionUuid = uuid;
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 0, true);
    } catch(e) {
      console.error(e);
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
  }

  async function saveConnection() {
    if (hasBlockingErrors) {
      const errorList = [
        ...ipv4Validation.errors,
        ...gatewayValidation.errors,
        ...dnsValidation.errors,
        ...ipv6Validation.errors
      ];
      uiStore.addToast(`Cannot save: ${errorList[0] || 'Please resolve network configuration errors.'}`, 'error');
      return;
    }

    isSaving = true;
    statusStore.setBusy('Saving connection settings…');
    
    try {
      const settings: Record<string, string> = {
        'ipv4.method': editConnectionData['ipv4.method'],
        'ipv4.addresses': editConnectionData['ipv4.method'] === 'manual' ? (editConnectionData['ipv4.addresses'] || '') : '',
        'ipv4.gateway': editConnectionData['ipv4.method'] === 'manual' ? (editConnectionData['ipv4.gateway'] || '') : '',
        'ipv4.dns': editConnectionData['ipv4.dns'] || '',
        'ipv6.method': editConnectionData['ipv6.method'] || 'auto',
      };
      
      if (editConnectionData['ipv6.method'] === 'manual') {
        settings['ipv6.addresses'] = editConnectionData['ipv6.addresses'] || '';
        settings['ipv6.gateway'] = editConnectionData['ipv6.gateway'] || '';
        settings['ipv6.dns'] = editConnectionData['ipv6.dns'] || '';
      }

      if (selectedConnectionUuid === '') {
        settings['connection.id'] = editConnectionData['connection.id'];
        settings['connection.type'] = editConnectionData['connection.type'];
      }
      
      await invoke('network_save_connection', { uuid: selectedConnectionUuid, settings });
      statusStore.setLastCommand(`nmcli connection modify ${selectedConnectionUuid || 'new'}`, 0, true);
      uiStore.addToast('Connection settings saved successfully', 'success');
      
      // If we modified an existing connection, try to reactivate it
      if (selectedConnectionUuid) {
        await invoke('network_up_connection', { uuid: selectedConnectionUuid }).catch(() => {});
      }
      
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
  
  async function deleteConnection(uuid: string) {
    if (!confirm("Are you sure you want to delete this connection?")) return;
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

  async function disconnectConnection(uuid: string) {
    statusStore.setBusy('Disconnecting…');
    try {
      await invoke('network_down_connection', { uuid });
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 0, true);
      uiStore.addToast('Connection deactivated', 'info');
      await loadData();
    } catch(e) {
      uiStore.addToast(`Error disconnecting: ${e}`, 'error');
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
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
    if (!vpnForm.gateway) {
      uiStore.addToast('Gateway/Endpoint is required', 'warning');
      return;
    }
    
    statusStore.setBusy('Creating VPN profile…');
    try {
      const res = await invoke<string>('network_create_vpn_profile', {
        name: vpnForm.name,
        vpnType: vpnForm.type,
        gateway: vpnForm.gateway,
        username: vpnForm.type === 'openvpn' ? vpnForm.username : null,
        password: vpnForm.type === 'openvpn' ? vpnForm.password : null
      });
      
      uiStore.addToast(res, 'success');
      showVpnDrawer = false;
      await loadData();
    } catch (e) {
      console.error(e);
      uiStore.addToast(`Creation failed: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function connectVpn(uuid: string) {
    statusStore.setBusy('Connecting VPN…');
    try {
      await invoke('network_up_connection', { uuid });
      uiStore.addToast('VPN Connected successfully', 'success');
      await loadData();
    } catch(e) {
      uiStore.addToast(`VPN Connection failed: ${e}`, 'error');
    } finally {
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

  $effect(() => {
    loadData();
  });
</script>

<div class="module-page">
  <PageHeader title="Advanced Network" icon={Wifi} />

  {#if selectedConnectionUuid !== null}
    <!-- ══════════════════════════════════════════════════════════════════════════
         CONNECTION EDITOR & SUBNET CALCULATOR VIEW
         ══════════════════════════════════════════════════════════════════════════ -->
    <div class="editor-header-bar">
      <Button variant="outline" class="btn-sm" onclick={() => { selectedConnectionUuid = null; editConnectionData = null; }}>
        <ArrowLeft size={14} /> Back to Connections
      </Button>
      
      <div class="editor-actions">
        {#if selectedConnectionUuid !== ''}
          <Button variant="outline" class="btn-sm btn-warn-outline" onclick={() => disconnectConnection(selectedConnectionUuid!)}>
            <Activity size={13} /> Deactivate
          </Button>
          <Button variant="outline" class="btn-sm btn-danger-outline" onclick={() => deleteConnection(selectedConnectionUuid!)}>
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

        {#if selectedConnectionUuid === ''}
          <div class="form-grid-2">
            <div class="form-group">
              <label for="conn-id">Connection Profile Name</label>
              <input id="conn-id" type="text" class="form-input" bind:value={editConnectionData['connection.id']} placeholder="e.g. eth0-static" />
            </div>
            <div class="form-group">
              <label for="conn-type">Connection Type</label>
              <Select id="conn-type" bind:value={editConnectionData['connection.type']}>
                <option value="802-3-ethernet">Ethernet (Wired LAN)</option>
                <option value="802-11-wireless">Wi-Fi (Wireless)</option>
                <option value="wireguard">WireGuard Tunnel</option>
                <option value="bridge">Network Bridge (br0)</option>
              </Select>
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
                  class="form-input font-mono" 
                  bind:value={editConnectionData['ipv6.dns']} 
                  placeholder="e.g. 2606:4700:4700::1111, 2001:4860:4860::8888" 
                />
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
        <Button variant="outline" class="btn-sm" onclick={loadData}>
          <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Refresh
        </Button>
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
        <div class="card" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted)">
          <RefreshCw size={24} class="animate-spin-slow" />
        </div>
      {:else if activeTab === 'interfaces'}
        <div style="display:grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px;">
          {#each interfaces as iface (iface.ifindex)}
            <div class="card" style="display:flex; flex-direction:column; gap:12px;">
              <div style="display:flex; justify-content:space-between; align-items:center; border-bottom:1px solid var(--color-border); padding-bottom:8px;">
                <div style="display:flex; align-items:center; gap:8px;">
                  <Hash size={16} style="color:var(--color-accent)" />
                  <span style="font-weight:600; color:var(--color-text-primary); font-size:16px;">{iface.ifname}</span>
                </div>
                <div style="display:flex; gap:8px; align-items:center;">
                  <Button variant="outline" class="" style="padding:4px 8px; font-size:11px;" onclick={() => setInterfaceState(iface.ifname, iface.operstate !== 'UP')}>
                    {iface.operstate === 'UP' ? 'Disable' : 'Enable'}
                  </Button>
                  <span class="badge {iface.operstate === 'UP' ? 'badge-success' : (iface.operstate === 'UNKNOWN' ? 'badge-warning' : 'badge-muted')}">{iface.operstate}</span>
                </div>
              </div>
              
              <div style="font-size:13px; color:var(--color-text-secondary); display:flex; flex-direction:column; gap:6px;">
                <div style="display:flex; justify-content:space-between;">
                  <span>MAC Address</span>
                  <span style="font-family:var(--font-mono); color:var(--color-text-primary)">{iface.address === '00:00:00:00:00:00' ? 'N/A' : iface.address}</span>
                </div>
                <div style="display:flex; justify-content:space-between;">
                  <span>MTU</span>
                  <span style="color:var(--color-text-primary)">{iface.mtu}</span>
                </div>
                <div style="display:flex; justify-content:space-between;">
                  <span>Type</span>
                  <span style="color:var(--color-text-primary)">{iface.link_type}</span>
                </div>
                
                {#if iface.addr_info && iface.addr_info.length > 0}
                  <div style="margin-top:8px; border-top:1px dashed var(--color-border); padding-top:8px;">
                    {#each iface.addr_info as addr}
                      <div style="display:flex; justify-content:space-between; margin-bottom:4px;">
                        <span style="font-size:11px; text-transform:uppercase; color:var(--color-text-muted)">{addr.family}</span>
                        <span style="font-family:var(--font-mono); color:var(--color-text-primary); font-size:12px;">{addr.local}/{addr.prefixlen}</span>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <div style="margin-top:8px; border-top:1px dashed var(--color-border); padding-top:8px; text-align:center; font-size:12px; font-style:italic;">
                    No IP addresses assigned
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else if activeTab === 'connections'}
        <div style="display:grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px;">
          {#each connections as conn}
            <div 
              class="card connection-card" 
              tabindex="0"
              role="button"
              onclick={() => editConnection(conn.uuid)} 
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); editConnection(conn.uuid); } }}
            >
              <div style="display:flex; justify-content:space-between; align-items:center;">
                <div style="display:flex; align-items:center; gap:8px;">
                  <Network size={16} class="text-accent" />
                  <span style="font-weight:600; color:var(--color-text-primary); font-size:15px;">{conn.name}</span>
                </div>
                <span class="badge {conn.state === 'activated' ? 'badge-success' : 'badge-muted'}">{conn.state || 'inactive'}</span>
              </div>
              <div style="display:flex; justify-content:space-between; font-size:12px; color:var(--color-text-secondary); margin-top:4px;">
                <span>Type: <strong>{conn.type}</strong></span>
                <span>Device: <code style="font-size:11px;">{conn.device || 'N/A'}</code></span>
              </div>
              <div style="display:flex; justify-content:flex-end; margin-top:6px;">
                <span style="font-size:11.5px; color:var(--color-accent); font-weight:600;">Configure IP &amp; Subnet &rarr;</span>
              </div>
            </div>
          {/each}
        </div>
      {:else if activeTab === 'vpn'}
        <div style="display:flex; flex-direction:column; gap:16px;">
          {#if loadingVpn}
            <div class="card" style="display:flex;align-items:center;justify-content:center;padding:40px;color:var(--color-text-muted)">
              <RefreshCw size={24} class="animate-spin-slow" />
            </div>
          {:else if vpnProfiles.length === 0}
            <div class="card empty-state" style="padding: 64px 32px; text-align:center;">
              <Key size={32} class="empty-state-icon" style="margin:0 0 16px; color:var(--color-text-muted);" />
              <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No VPN Profiles</span>
              <span style="color:var(--color-text-muted); margin-top:8px; display:block;">Import an OpenVPN (.ovpn) or WireGuard (.conf) file, or create one manually.</span>
              <div style="display:flex; justify-content:center; gap:12px; margin-top:20px;">
                <Button variant="outline" onclick={importVpnFile}>
                  <FileUp size={14} style="margin-right:6px;" /> Import Profile
                </Button>
                <Button variant="primary" onclick={() => showVpnDrawer = true}>
                  <Plus size={14} style="margin-right:6px;" /> Create VPN
                </Button>
              </div>
            </div>
          {:else}
            <div style="display:grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px;">
              {#each vpnProfiles as vpn}
                <div class="card" style="display:flex; flex-direction:column; gap:12px; border: 1px solid var(--color-border);">
                  <div style="display:flex; justify-content:space-between; align-items:center;">
                    <div style="display:flex; align-items:center; gap:8px;">
                      <Key size={16} style="color:var(--color-accent);" />
                      <span style="font-weight:600; color:var(--color-text-primary); font-size:15px;">{vpn.name}</span>
                    </div>
                    <span class="badge {vpn.active ? 'badge-success' : 'badge-muted'}">{vpn.active ? 'active' : 'inactive'}</span>
                  </div>
                  <div style="display:flex; justify-content:space-between; font-size:12px; color:var(--color-text-secondary);">
                    <span>Protocol: {vpn.vpn_type === 'vpn' ? 'OpenVPN' : 'WireGuard'}</span>
                  </div>
                  <div style="display:flex; justify-content:flex-end; gap:8px; margin-top:8px; border-top:1px solid rgba(255,255,255,0.03); padding-top:10px;">
                    <Button variant="outline" style="padding: 4px 8px; font-size:12px; color:var(--color-error); border-color:var(--color-error);" onclick={() => deleteConnection(vpn.uuid)}>
                      <Trash2 size={12} style="margin-right:4px;" /> Delete
                    </Button>
                    <Button variant={vpn.active ? 'outline' : 'primary'} style="padding: 4px 10px; font-size:12px;" onclick={() => vpn.active ? disconnectConnection(vpn.uuid) : connectVpn(vpn.uuid)}>
                      {#if vpn.active}
                        <Square size={12} style="margin-right:4px;" /> Disconnect
                      {:else}
                        <Play size={12} style="margin-right:4px;" /> Connect
                      {/if}
                    </Button>
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
            Measure your network connection latency, download speed, and upload speed using our fast CDN endpoint.
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
          <h3 style="margin-top:0; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
            <Server size={18} style="color:var(--color-info)" /> System DNS Configuration
          </h3>
          <p style="font-size:13px; color:var(--color-text-secondary); margin-bottom:16px;">
            Current DNS resolution settings from systemd-resolved or /etc/resolv.conf.
          </p>
          <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:8px; padding:12px; overflow:auto; flex:1;">
            <pre style="margin:0; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); white-space:pre-wrap;">{dnsInfo}</pre>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <SideDrawer bind:isOpen={showVpnDrawer} title="Create VPN Profile" width="480px">
    <div style="display:flex; flex-direction:column; gap:16px; padding:8px 0;">
      <div class="form-group">
        <label for="vpn-name">Profile Name</label>
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
          {vpnForm.type === 'openvpn' ? 'Gateway Address' : 'Peer Endpoint (e.g. 198.51.100.1:51820)'}
        </label>
        <input id="vpn-gateway" type="text" class="form-input" bind:value={vpnForm.gateway} placeholder={vpnForm.type === 'openvpn' ? 'vpn.example.com' : '198.51.100.1:51820'} />
      </div>
      
      {#if vpnForm.type === 'openvpn'}
        <div class="form-group">
          <label for="vpn-username">Username (Optional)</label>
          <input id="vpn-username" type="text" class="form-input" bind:value={vpnForm.username} placeholder="vpnuser" />
        </div>
        <div class="form-group">
          <label for="vpn-password">Password (Optional)</label>
          <input id="vpn-password" type="password" class="form-input" bind:value={vpnForm.password} placeholder="••••••••" />
        </div>
      {/if}
      
      <div style="display:flex; justify-content:flex-end; gap:8px; margin-top:16px;">
        <Button variant="outline" onclick={() => showVpnDrawer = false}>Cancel</Button>
        <Button variant="primary" onclick={handleCreateVpn}>Create Profile</Button>
      </div>
    </div>
  </SideDrawer>
</div>

<style>
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

  .form-group label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  :global(html.light-mode) .form-group label {
    color: #475569;
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
  .text-danger { color: var(--color-error); }
  .flex-shrink-0 { flex-shrink: 0; }
</style>
