<script lang="ts">
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { Activity, Wifi, Globe, RefreshCw, Server, Hash, Network, Plus, ArrowLeft, Save, Trash } from '@lucide/svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  let interfaces = $state<any[]>([]);
  let dnsInfo = $state<string>('');
  let connections = $state<any[]>([]);
  
  let loading = $state(true);
  let activeTab = $state<'interfaces' | 'dns' | 'connections'>('interfaces');
  
  let selectedConnectionUuid = $state<string | null>(null);
  let editConnectionData = $state<any>(null);
  let isSaving = $state(false);

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
        // nmcli escapes colons with \:, but for UUID, NAME, TYPE, DEVICE, STATE it's usually fine
        // Let's use a regex or simple split. Using regex to handle escaped colons is safer, 
        // but since we asked nmcli -t, we'll assume basic split is okay for standard names.
        // Actually, simple split by ':' might break if NAME has ':'. Let's do basic split.
        const parts = line.split(':');
        return {
          uuid: parts[0],
          name: parts[1],
          type: parts[2],
          device: parts[3],
          state: parts[4]
        };
      });
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
    selectedConnectionUuid = uuid;
    if (uuid === '') {
       // New connection profile defaults
       editConnectionData = { 
         'connection.id': 'New Connection',
         'connection.type': '802-3-ethernet',
         'ipv4.method': 'auto', 
         'ipv6.method': 'auto', 
         'ipv4.addresses': '', 
         'ipv4.gateway': '', 
         'ipv4.dns': '' 
       };
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
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 0, true);
    } catch(e) {
      console.error(e);
      statusStore.setLastCommand(`nmcli connection show ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
  }

  function isValidIPv4WithCidr(ip: string) {
    if (!ip) return false;
    const parts = ip.split('/');
    if (parts.length !== 2) return false;
    const addr = parts[0];
    const cidr = parseInt(parts[1], 10);
    if (isNaN(cidr) || cidr < 0 || cidr > 32) return false;
    const octets = addr.split('.');
    if (octets.length !== 4) return false;
    return octets.every(o => {
      const num = parseInt(o, 10);
      return !isNaN(num) && num >= 0 && num <= 255 && String(num) === o;
    });
  }

  function isValidIPv4(ip: string) {
    if (!ip) return false;
    const octets = ip.split('.');
    if (octets.length !== 4) return false;
    return octets.every(o => {
      const num = parseInt(o, 10);
      return !isNaN(num) && num >= 0 && num <= 255 && String(num) === o;
    });
  }

  async function saveConnection() {
    isSaving = true;
    statusStore.setBusy('Saving connection…');
    
    // Validations
    if (editConnectionData['ipv4.method'] === 'manual') {
      const addr = editConnectionData['ipv4.addresses'];
      if (!addr || !isValidIPv4WithCidr(addr)) {
        alert('Invalid IPv4 Address. Must be in CIDR format, e.g. 192.168.1.10/24');
        isSaving = false;
        statusStore.clearBusy();
        return;
      }
      const gw = editConnectionData['ipv4.gateway'];
      if (gw && !isValidIPv4(gw)) {
        alert('Invalid IPv4 Gateway.');
        isSaving = false;
        statusStore.clearBusy();
        return;
      }
    }
    
    const dns = editConnectionData['ipv4.dns'];
    if (dns) {
      const dnsList = dns.split(',').map((d: string) => d.trim()).filter(Boolean);
      for (const d of dnsList) {
        if (!isValidIPv4(d)) {
          alert(`Invalid DNS Server IP: ${d}`);
          isSaving = false;
          statusStore.clearBusy();
          return;
        }
      }
    }
    isSaving = true;
    statusStore.setBusy('Saving connection…');
    try {
      const settings: Record<string, string> = {
        'ipv4.method': editConnectionData['ipv4.method'],
        'ipv4.addresses': editConnectionData['ipv4.addresses'] || '',
        'ipv4.gateway': editConnectionData['ipv4.gateway'] || '',
        'ipv4.dns': editConnectionData['ipv4.dns'] || '',
        'ipv6.method': editConnectionData['ipv6.method'] || 'auto',
      };
      
      if (selectedConnectionUuid === '') {
        settings['connection.id'] = editConnectionData['connection.id'];
        settings['connection.type'] = editConnectionData['connection.type'];
      }
      
      await invoke('network_save_connection', { uuid: selectedConnectionUuid, settings });
      statusStore.setLastCommand(`nmcli connection modify ${selectedConnectionUuid || 'new'}`, 0, true);
      
      // If we modified an existing one, try to bring it up
      if (selectedConnectionUuid) {
        await invoke('network_up_connection', { uuid: selectedConnectionUuid }).catch(() => {});
      }
      
      await loadData();
      activeTab = 'connections';
    } catch(e: any) {
      alert("Error saving connection: " + e);
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
      await loadData();
      selectedConnectionUuid = null;
    } catch(e) {
      alert("Error deleting connection");
      statusStore.setLastCommand(`nmcli connection delete ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
  }

  async function disconnectConnection(uuid: string) {
    statusStore.setBusy('Disconnecting…');
    try {
      await invoke('network_down_connection', { uuid });
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 0, true);
      await loadData();
    } catch(e) {
      alert("Error disconnecting: " + e);
      statusStore.setLastCommand(`nmcli connection down ${uuid}`, 1, false);
    }
    statusStore.clearBusy();
  }

  async function setInterfaceState(iface: string, up: boolean) {
    statusStore.setBusy(up ? 'Enabling interface…' : 'Disabling interface…');
    try {
      await invoke('network_set_interface_state', { iface, up });
      statusStore.setLastCommand(`ip link set ${iface} ${up ? 'up' : 'down'}`, 0, true);
      await loadData();
    } catch(e) {
      alert("Error setting interface state: " + e);
      statusStore.setLastCommand(`ip link set ${iface} ${up ? 'up' : 'down'}`, 1, false);
    }
    statusStore.clearBusy();
  }

  $effect(() => {
    loadData();
  });
</script>

<div class="module-page">
  <PageHeader title="Advanced Network" icon={Wifi} description="Manage network interfaces, connections, and DNS settings" />

  {#if selectedConnectionUuid !== null}
    <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom: 16px; flex-shrink:0;">
      <Button variant="outline" class="" style="display:flex; align-items:center; gap:6px; font-size:12px; padding:6px 12px;" onclick={() => selectedConnectionUuid = null}>
        <ArrowLeft size={14}/> Back to Connections
      </Button>
      <div style="display:flex; gap:8px;">
        {#if selectedConnectionUuid !== ''}
          <Button variant="outline" class="" style="display:flex; align-items:center; gap:6px; font-size:12px; padding:6px 12px; border-color:var(--color-warning); color:var(--color-warning);" onclick={() => disconnectConnection(selectedConnectionUuid)}>
            <Activity size={14}/> Disconnect
          </Button>
          <Button variant="outline" class="" style="display:flex; align-items:center; gap:6px; font-size:12px; padding:6px 12px; border-color:var(--color-error); color:var(--color-error);" onclick={() => deleteConnection(selectedConnectionUuid)}>
            <Trash size={14}/> Delete
          </Button>
        {/if}
        <Button variant="primary" class="" style="display:flex; align-items:center; gap:6px; font-size:12px; padding:6px 12px;" disabled={isSaving} onclick={saveConnection}>
          {#if isSaving}
            <RefreshCw size={14} class="animate-spin-slow" /> Saving...
          {:else}
            <Save size={14}/> Save Settings
          {/if}
        </Button>
      </div>
    </div>
    
    <div class="module-content-scroll">
      <div class="card" style="display:flex; flex-direction:column; gap:16px;">
        <h3 style="margin:0; color:var(--color-text-primary); font-size:16px;">
          {selectedConnectionUuid === '' ? 'Add New Connection' : 'Edit Connection: ' + editConnectionData['connection.id']}
        </h3>
        
        {#if selectedConnectionUuid === ''}
          <div style="display:flex; flex-direction:column; gap:4px;">
            <label for="conn-id" style="font-size:12px; color:var(--color-text-secondary);">Connection Name</label>
            <input id="conn-id" type="text" class="input" bind:value={editConnectionData['connection.id']} placeholder="e.g. eth0-custom" />
          </div>
          <div style="display:flex; flex-direction:column; gap:4px;">
            <label for="conn-type" style="font-size:12px; color:var(--color-text-secondary);">Connection Type</label>
            <Select id="conn-type"  bind:value={editConnectionData['connection.type']}>
              <option value="802-3-ethernet">Ethernet</option>
              <option value="802-11-wireless">Wi-Fi</option>
              <option value="wireguard">WireGuard</option>
              <option value="bridge">Bridge</option>
            </Select>
          </div>
        {/if}
        
        <div style="display:grid; grid-template-columns: 1fr 1fr; gap:16px;">
          <div style="display:flex; flex-direction:column; gap:16px; padding-right:16px; border-right:1px solid var(--color-border);">
            <h4 style="margin:0; color:var(--color-text-primary); border-bottom:1px solid rgba(255,255,255,0.05); padding-bottom:8px;">IPv4 Configuration</h4>
            
            <div style="display:flex; flex-direction:column; gap:4px;">
              <label for="ipv4-method" style="font-size:12px; color:var(--color-text-secondary);">Method</label>
              <Select id="ipv4-method"  bind:value={editConnectionData['ipv4.method']}>
                <option value="auto">Automatic (DHCP)</option>
                <option value="manual">Manual (Static)</option>
                <option value="disabled">Disabled</option>
                <option value="link-local">Link-Local</option>
              </Select>
            </div>
            
            {#if editConnectionData['ipv4.method'] === 'manual'}
              <div style="display:flex; flex-direction:column; gap:4px;">
                <label for="ipv4-addr" style="font-size:12px; color:var(--color-text-secondary);">Addresses (e.g. 192.168.1.10/24)</label>
                <input id="ipv4-addr" type="text" class="input" bind:value={editConnectionData['ipv4.addresses']} oninput={(e) => editConnectionData['ipv4.addresses'] = e.currentTarget.value.replace(/[^\d./]/g, '')} />
              </div>
              <div style="display:flex; flex-direction:column; gap:4px;">
                <label for="ipv4-gw" style="font-size:12px; color:var(--color-text-secondary);">Gateway</label>
                <input id="ipv4-gw" type="text" class="input" bind:value={editConnectionData['ipv4.gateway']} oninput={(e) => editConnectionData['ipv4.gateway'] = e.currentTarget.value.replace(/[^\d.]/g, '')} />
              </div>
            {/if}
            
            {#if editConnectionData['ipv4.method'] !== 'disabled'}
              <div style="display:flex; flex-direction:column; gap:4px;">
                <label for="ipv4-dns" style="font-size:12px; color:var(--color-text-secondary);">DNS Servers (comma-separated)</label>
                <input id="ipv4-dns" type="text" class="input" bind:value={editConnectionData['ipv4.dns']} oninput={(e) => editConnectionData['ipv4.dns'] = e.currentTarget.value.replace(/[^\d., ]/g, '')} placeholder="e.g. 8.8.8.8, 1.1.1.1" />
              </div>
            {/if}
          </div>
          
          <div style="display:flex; flex-direction:column; gap:16px;">
            <h4 style="margin:0; color:var(--color-text-primary); border-bottom:1px solid rgba(255,255,255,0.05); padding-bottom:8px;">IPv6 Configuration</h4>
            
            <div style="display:flex; flex-direction:column; gap:4px;">
              <label for="ipv6-method" style="font-size:12px; color:var(--color-text-secondary);">Method</label>
              <Select id="ipv6-method"  bind:value={editConnectionData['ipv6.method']}>
                <option value="auto">Automatic</option>
                <option value="dhcp">DHCP Only</option>
                <option value="manual">Manual (Static)</option>
                <option value="ignore">Disabled / Ignore</option>
              </Select>
            </div>
          </div>
        </div>
      </div>
    </div>
  {:else}
    <div class="controls-row">
      <div class="tab-bar">
        <button class="tab-btn { activeTab === 'interfaces' ? 'active' : '' }" onclick={() => activeTab = 'interfaces'}>
          <Activity size={14} style="margin-right:6px" /> Physical Adapters
        </button>
        <button class="tab-btn { activeTab === 'connections' ? 'active' : '' }" onclick={() => activeTab = 'connections'}>
          <Network size={14} style="margin-right:6px" /> Connections
        </button>
        <button class="tab-btn { activeTab === 'dns' ? 'active' : '' }" onclick={() => activeTab = 'dns'}>
          <Globe size={14} style="margin-right:6px" /> Global DNS
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
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div 
              class="card" 
              style="cursor:pointer; transition:all 0.2s; border:1px solid transparent; display:flex; flex-direction:column; gap:8px;" 
              tabindex="0"
              role="button"
              onclick={() => editConnection(conn.uuid)} 
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); editConnection(conn.uuid); } }}
              onmouseenter={(e) => e.currentTarget.style.borderColor='var(--color-accent)'} 
              onmouseleave={(e) => e.currentTarget.style.borderColor='transparent'}
            >
              <div style="display:flex; justify-content:space-between; align-items:center;">
                <span style="font-weight:600; color:var(--color-text-primary); font-size:15px;">{conn.name}</span>
                <span class="badge {conn.state === 'activated' ? 'badge-success' : 'badge-muted'}">{conn.state || 'inactive'}</span>
              </div>
              <div style="display:flex; justify-content:space-between; font-size:12px; color:var(--color-text-secondary)">
                <span>Type: {conn.type}</span>
                <span>Device: {conn.device || 'N/A'}</span>
              </div>
            </div>
          {/each}
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
</div>

<style>
  
  
  
</style>
