<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import Select from '../components/ui/Select.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { ShieldAlert, Shield, ShieldCheck, Power, RefreshCw, Trash2, Plus, Network, Sparkles } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import GuideBanner from '../components/ui/GuideBanner.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';

  let aiPromptInput = $state('');
  let showAiPromptBox = $state(false);

  function triggerAiRuleGen() {
    if (!aiPromptInput.trim()) return;
    aiStore.generateFirewallRule(aiPromptInput);
    showAiPromptBox = false;
    aiPromptInput = '';
  }

  // ── GROUP 5 confirmation plumbing ───────────────────────────────────────
  //
  // Mirrors the helper's fail-closed coverage check (`firewall_port_covers_22`):
  // an unparseable port string is treated as covering 22, never silently
  // cleared. Only the exact single/range shapes the helper parses can be a
  // negative answer; everything else fails closed.
  function portNumber(value: string): number | null {
    return /^\d+$/.test(value) ? Number(value) : null;
  }

  function portStringCovers22(port: string): boolean {
    const parts = port.split('/')[0].split('-');
    // More than one separator is not a shape the helper can parse.
    if (parts.length > 2) return true;
    const start = portNumber(parts[0]);
    const end = portNumber(parts[parts.length - 1]);
    // Empty, non-numeric or out-of-range values fail closed like the helper.
    if (
      start === null ||
      end === null ||
      start > 65535 ||
      end > 65535
    ) {
      return true;
    }
    return start <= 22 && 22 <= end;
  }

  function ruleRemovalCutsSsh(type: 'port' | 'service', val: string): boolean {
    return (type === 'service' && val === 'ssh') || (type === 'port' && portStringCovers22(val));
  }

  /// True when the rich rule the user is building drops or rejects SSH.
  /// Protocol elements cannot target ports, so they are never SSH-blocking.
  function richRuleBlocksSsh(): boolean {
    return (
      (ruleBuilder.action === 'drop' || ruleBuilder.action === 'reject') &&
      ((ruleBuilder.elemType === 'service' && ruleBuilder.elemVal.trim() === 'ssh') ||
        (ruleBuilder.elemType === 'port' && portStringCovers22(ruleBuilder.elemVal.trim())))
    );
  }

  /// Backend `CONFIRMATION_REQUIRED:` errors are retry contracts: the error
  /// is shown verbatim (never swallowed or rewritten) in a danger dialog,
  /// and accepting it retries the same operation with the explicit
  /// confirmation value. Everything else is an ordinary failure toast.
  function handleSshConfirmation(error: unknown, context: string, retry: () => Promise<void>) {
    const message = String(error);
    if (message.startsWith('CONFIRMATION_REQUIRED:')) {
      uiStore.confirm('SSH Session At Risk', message, retry, true);
    } else {
      uiStore.addToast(`${context}: ${message}`, 'error');
    }
  }

  interface FirewallState {
    is_running: boolean;
    is_panic: boolean;
    active_zones: string[];
    default_zone: string;
  }

  interface ZoneRules {
    zone: string;
    services: string[];
    ports: string[];
  }

  let firewallState = $state<FirewallState | null>(null);
  let loading = $state(true);
  
  let activeZone = $state<string>('');
  let rules = $state<ZoneRules | null>(null);
  let loadingRules = $state(false);

  let newPort = $state('');
  let newService = $state('');

  // Port Listener Checker State
  let portCheckResult = $state<{ port: number; proto: string; is_listening: boolean; process_name: string; pid: string } | null>(null);
  let checkingPort = $state(false);
  let portDebounceTimer: any = null;

  function handlePortInput(val: string) {
    newPort = val;
    if (portDebounceTimer) clearTimeout(portDebounceTimer);
    const cleaned = val.trim();
    if (!cleaned) {
      portCheckResult = null;
      return;
    }
    const parts = cleaned.split('/');
    const portNum = parseInt(parts[0]);
    const proto = parts.length > 1 ? parts[1] : 'tcp';
    if (isNaN(portNum) || portNum <= 0 || portNum > 65535) {
      portCheckResult = null;
      return;
    }

    portDebounceTimer = setTimeout(async () => {
      checkingPort = true;
      try {
        portCheckResult = await invoke('firewall_check_port_listener', { port: portNum, proto });
      } catch (e) {
        portCheckResult = null;
      } finally {
        checkingPort = false;
      }
    }, 200);
  }

  // Phase 3 Firewall Rich Rules & Interfaces states
  let activeSubTab = $state<'rules' | 'rich' | 'interfaces'>(
    uiStore.targetSubTab && ['rules', 'rich', 'interfaces'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'rules'
  );
  if (uiStore.targetSubTab && ['rules', 'rich', 'interfaces'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  let richRules = $state<string[]>([]);
  let zoneInterfaces = $state<string[]>([]);
  let allInterfaces = $state<string[]>([]);
  
  let loadingRich = $state(false);
  let loadingInterfaces = $state(false);
  
  let ruleBuilder = $state({
    family: 'ipv4',
    source: '',
    dest: '',
    elemType: 'service', // 'service' | 'port' | 'protocol'
    elemVal: '',
    action: 'accept' // 'accept' | 'reject' | 'drop'
  });

  async function loadRichRules(zone: string) {
    loadingRich = true;
    try {
      richRules = await invoke<string[]>('firewall_get_rich_rules', { zone });
    } catch(e) {
      console.error(e);
    } finally {
      loadingRich = false;
    }
  }

  async function loadInterfaces(zone: string) {
    loadingInterfaces = true;
    try {
      zoneInterfaces = await invoke<string[]>('firewall_get_zone_interfaces', { zone });
      allInterfaces = await invoke<string[]>('firewall_get_all_interfaces');
    } catch(e) {
      console.error(e);
    } finally {
      loadingInterfaces = false;
    }
  }

  async function loadState() {
    loading = true;
    statusStore.setBusy('Loading firewalld state…');
    try {
      firewallState = await invoke<FirewallState>('get_firewall_state');
      statusStore.setLastCommand('firewall-cmd --state', 0, true);
      
      if (firewallState.is_running && firewallState.active_zones.length > 0) {
        if (!activeZone || !firewallState.active_zones.includes(activeZone)) {
          activeZone = firewallState.default_zone || firewallState.active_zones[0];
        }
        await loadRules(activeZone);
      }
    } catch (e) {
      uiStore.addToast(`Failed to load firewall state: ${e}`, 'error');
      statusStore.setLastCommand('firewall-cmd', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function loadRules(zone: string) {
    loadingRules = true;
    try {
      rules = await invoke<ZoneRules>('get_zone_rules', { zone });
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 0, true);
      
      await Promise.all([
        loadRichRules(zone),
        loadInterfaces(zone)
      ]);
    } catch (e) {
      uiStore.addToast(`Failed to load rules for zone ${zone}: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 1, false);
    } finally {
      loadingRules = false;
    }
  }

  function confirmTogglePanic() {
    const enable = !firewallState?.is_panic;
    if (!enable) {
      // Disabling restores normal firewall rules: not a destructive change,
      // so no dialog; the backend requires no confirmation either.
      doTogglePanic(false, false);
      return;
    }
    uiStore.confirm(
      'Enable Panic Mode',
      'WARNING: Enabling Panic Mode will immediately drop all incoming and outgoing network connections! You will lose all connectivity.',
      () => doTogglePanic(enable, true),
      true
    );
  }

  async function doTogglePanic(enable: boolean, confirmed = false) {
    statusStore.setBusy(`${enable ? 'Enabling' : 'Disabling'} panic mode…`);
    try {
      await invoke('toggle_panic_mode', { enable, confirmed });
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 0, true);
      uiStore.addToast(`Panic mode ${enable ? 'enabled' : 'disabled'}`, 'success');
      await loadState();
    } catch (e) {
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 1, false);
      handleSshConfirmation(e, 'Failed to toggle panic mode', () =>
        doTogglePanic(enable, true)
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  async function addRule(type: 'port' | 'service') {
    let val = type === 'port' ? newPort.trim() : newService.trim();
    if (!val) return;
    
    // Auto-append /tcp if the user just typed a number like "80"
    if (type === 'port' && val.match(/^\d+$/)) {
      val = val + '/tcp';
    }

    statusStore.setBusy(`Adding ${type} ${val}…`);
    try {
      await invoke('modify_firewall_rule', { zone: activeZone, ruleType: type, value: val, add: true });
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --add-${type}=${val}`, 0, true);
      uiStore.addToast(`Added ${type} ${val} successfully`, 'success');
      if (type === 'port') newPort = '';
      if (type === 'service') newService = '';
      await loadRules(activeZone);
    } catch (e) {
      uiStore.addToast(`Failed to add ${type}: ${e}`, 'error');
      statusStore.setLastCommand(`firewall-cmd --zone=${activeZone} --add-${type}=${val}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmRemoveRule(type: 'port' | 'service', val: string) {
    // Capture the zone at initiation: a confirmation retry must target the
    // exact zone the user acted on, not whichever zone is selected later.
    const zone = activeZone;
    const message = ruleRemovalCutsSsh(type, val)
      ? `WARNING: ${val} is what your active SSH session is connected through. Removing it can cut off your session and permanently lock you out of the firewall.`
      : `Are you sure you want to block ${type} ${val} by removing it from the firewall?`;
    uiStore.confirm(
      `Remove ${type === 'port' ? 'Port' : 'Service'}`,
      message,
      () => doRemoveRule(zone, type, val, true),
      true
    );
  }

  async function doRemoveRule(zone: string, type: 'port' | 'service', val: string, confirmed = false) {
    statusStore.setBusy(`Removing ${type} ${val}…`);
    try {
      await invoke('modify_firewall_rule', { zone, ruleType: type, value: val, add: false, confirmed });
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --remove-${type}=${val}`, 0, true);
      uiStore.addToast(`Removed ${type} ${val} successfully`, 'success');
      await loadRules(zone);
    } catch (e) {
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --remove-${type}=${val}`, 1, false);
      handleSshConfirmation(e, `Failed to remove ${type} ${val}`, () =>
        doRemoveRule(zone, type, val, true)
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  async function addRichRule() {
    // Capture the zone at initiation: the confirmation dialog and any retry
    // must target the exact zone this rule was built for.
    const zone = activeZone;
    let rule = `rule family="${ruleBuilder.family}"`;
    if (ruleBuilder.source.trim()) {
      rule += ` source address="${ruleBuilder.source.trim()}"`;
    }
    if (ruleBuilder.dest.trim()) {
      rule += ` destination address="${ruleBuilder.dest.trim()}"`;
    }
    
    if (ruleBuilder.elemType === 'service') {
      if (!ruleBuilder.elemVal.trim()) {
        uiStore.addToast('Service name is required', 'warning');
        return;
      }
      rule += ` service name="${ruleBuilder.elemVal.trim()}"`;
    } else if (ruleBuilder.elemType === 'port') {
      if (!ruleBuilder.elemVal.trim()) {
        uiStore.addToast('Port is required', 'warning');
        return;
      }
      let val = ruleBuilder.elemVal.trim();
      if (val.match(/^\d+$/)) val += '/tcp';
      const parts = val.split('/');
      if (parts.length === 2) {
        rule += ` port port="${parts[0]}" protocol="${parts[1]}"`;
      } else {
        rule += ` port port="${val}" protocol="tcp"`;
      }
    } else {
      if (!ruleBuilder.elemVal.trim()) {
        uiStore.addToast('Protocol is required', 'warning');
        return;
      }
      rule += ` protocol value="${ruleBuilder.elemVal.trim()}"`;
    }
    
    rule += ` ${ruleBuilder.action}`;

    const submitAdd = async (confirmed: boolean) => {
      statusStore.setBusy('Adding rich rule…');
      try {
        await invoke('firewall_modify_rich_rule', { zone, rule, add: true, confirmed });
        uiStore.addToast('Rich rule added successfully', 'success');
        ruleBuilder.elemVal = '';
        ruleBuilder.source = '';
        ruleBuilder.dest = '';
        await loadRichRules(zone);
      } catch (e) {
        handleSshConfirmation(e, 'Failed to add rich rule', () => submitAdd(true));
      } finally {
        statusStore.clearBusy();
      }
    };

    // A drop/reject rule for SSH would sever the active session: explicit
    // danger dialog before sending it. Everything else needs no dialog; the
    // backend remains the authoritative boundary either way.
    if (richRuleBlocksSsh()) {
      uiStore.confirm(
        'SSH Rule May Cut Your Connection',
        `WARNING: this ${ruleBuilder.action} rule targets ssh and can interrupt your active SSH session, possibly locking you out of the firewall.`,
        () => submitAdd(true),
        true
      );
    } else {
      await submitAdd(false);
    }
  }

  async function removeRichRule(rule: string) {
    statusStore.setBusy('Removing rich rule…');
    try {
      await invoke('firewall_modify_rich_rule', { zone: activeZone, rule, add: false, confirmed: false });
      uiStore.addToast('Rich rule removed successfully', 'success');
      await loadRichRules(activeZone);
    } catch (e) {
      uiStore.addToast(`Failed to remove rich rule: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function bindInterface(zone: string, iface: string, confirmed = false) {
    statusStore.setBusy(`Binding interface ${iface}…`);
    try {
      await invoke('firewall_change_interface_zone', { zone, interface: iface, confirmed });
      uiStore.addToast(`Interface ${iface} bound to ${zone}`, 'success');
      await loadInterfaces(zone);
    } catch (e) {
      handleSshConfirmation(e, `Failed to bind interface ${iface}`, () =>
        bindInterface(zone, iface, true)
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadState(); });
</script>

<div class="module-page">
  <PageHeader title="Firewall Manager" subtitle="Manage firewalld zones, open ports, and allowed services" icon={Shield}>
    {#if aiStore.enabled}
      <Button variant="outline" size="sm" onclick={() => showAiPromptBox = !showAiPromptBox} title="Generate firewalld rule with AI">
        <Sparkles size={14} style="color:var(--color-accent);" /> AI Rule Generator
      </Button>
    {/if}
    <Button variant="ghost" class="" onclick={loadState} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    {#if firewallState?.is_running}
      <Button 
        variant={firewallState.is_panic ? 'primary' : 'danger'}
        onclick={confirmTogglePanic}
      >
        <Power size={14} /> {firewallState.is_panic ? 'Disable Panic Mode' : 'Panic Mode'}
      </Button>
    {/if}
  </PageHeader>

  {#if showAiPromptBox}
    <div style="margin: 0 0 16px 0; padding: 14px 16px; background: rgba(0,218,243,0.06); border: 1px solid rgba(0,218,243,0.2); border-radius: 10px; display: flex; flex-direction: column; gap: 10px;">
      <div style="display: flex; align-items: center; justify-content: space-between;">
        <span style="font-size: 13px; font-weight: 600; color: var(--color-text-primary); display: flex; align-items: center; gap: 6px;">
          <Sparkles size={15} style="color: var(--color-accent);" /> AI Firewall Rule Generator
        </span>
        <button type="button" onclick={() => showAiPromptBox = false} style="background:none; border:none; color:var(--color-text-muted); cursor:pointer; font-size:12px;">Cancel</button>
      </div>
      <div style="display: flex; gap: 8px;">
        <input
          type="text"
          bind:value={aiPromptInput}
          placeholder="e.g. Block port 8080 from public except IP 192.168.1.50..."
          onkeydown={(e) => { if (e.key === 'Enter') triggerAiRuleGen(); }}
          style="flex: 1; padding: 8px 12px; background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-primary); font-size: 12.5px;"
        />
        <Button variant="primary" size="sm" onclick={triggerAiRuleGen} disabled={!aiPromptInput.trim()}>
          Generate Rule
        </Button>
      </div>
    </div>
  {/if}

  {#if loading && !firewallState}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading Firewall State…</span>
    </div>
  {:else if firewallState && !firewallState.is_running}
    <div class="empty-state" style="padding: 64px 32px;">
      <div style="width:64px; height:64px; border-radius:50%; background:rgba(255,71,87,0.1); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
        <ShieldAlert size={32} class="empty-state-icon" style="color:var(--color-danger); margin:0;" />
      </div>
      <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
        Firewall is Offline
      </span>
      <span style="color:var(--color-text-muted); margin-top:8px;">
        firewalld is not currently running. You can start it from the Service Manager.
      </span>
    </div>
  {:else if firewallState}
    <GuideBanner
      icon={Shield}
      title="Dynamic Zone Management"
      description="Firewall rules are grouped by security zones. Traffic received on interfaces bound to a zone is filtered by that zone's services and port definitions."
      variant="info"
      style="margin-bottom: -4px;"
    />

    {#if firewallState.is_panic}
      <div style="margin-bottom:16px; padding:16px; border-radius:12px; background:rgba(255, 71, 87, 0.1); border:1px solid rgba(255, 71, 87, 0.3); display:flex; align-items:center; gap:12px">
        <ShieldAlert size={24} style="color:var(--color-danger)" />
        <div>
          <div style="font-weight:600; color:var(--color-danger)">Panic Mode Active</div>
          <div style="font-size:13px; color:var(--color-text-primary)">All incoming and outgoing network traffic is currently being blocked.</div>
        </div>
      </div>
    {/if}

    <div style="display:flex; gap:16px; flex:1; min-height:0">
      <!-- Zones Sidebar -->
      <div class="card" style="width: 250px; display:flex; flex-direction:column; gap:8px">
        <h3 style="margin-top:0; color:var(--color-text-primary); font-size:14px">Active Zones</h3>
        {#each firewallState.active_zones as zone}
          <Button 
            class="zone-btn {activeZone === zone ? 'active' : ''}"
            onclick={() => { activeZone = zone; loadRules(zone); }}
          >
            <ShieldCheck size={16} />
            {zone}
            {#if zone === firewallState.default_zone}
              <span class="badge badge-info" style="margin-left:auto; font-size:10px; padding:2px 6px">Default</span>
            {/if}
          </Button>
        {/each}
      </div>

      <!-- Rules Content -->
      <div class="card module-content-scroll" style="flex:1; display:flex; flex-direction:column; gap:20px">
        {#if loadingRules}
          <div style="display:flex; align-items:center; gap:8px; color:var(--color-text-muted)">
            <RefreshCw size={14} class="animate-spin" /> Loading rules…
          </div>
        {:else if rules}
          <!-- Sub-tabs bar -->
          <div class="tab-bar" style="margin-bottom:8px; flex-shrink:0;">
            <button class="tab-btn { activeSubTab === 'rules' ? 'active' : '' }" onclick={() => activeSubTab = 'rules'} style="padding: 6px 12px; font-size:12.5px;">
              Basic Rules
            </button>
            <button class="tab-btn { activeSubTab === 'rich' ? 'active' : '' }" onclick={() => activeSubTab = 'rich'} style="padding: 6px 12px; font-size:12.5px;">
              Rich Rules
            </button>
            <button class="tab-btn { activeSubTab === 'interfaces' ? 'active' : '' }" onclick={() => activeSubTab = 'interfaces'} style="padding: 6px 12px; font-size:12.5px;">
              Interface Bindings
            </button>
          </div>

          {#if activeSubTab === 'rules'}
            <div>
              <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Allowed Services</h3>
              <div style="display:flex; gap:8px; margin-bottom:12px">
                <input class="input" bind:value={newService} placeholder="e.g. http, https, ssh" onkeydown={(e) => e.key === 'Enter' && addRule('service')} />
                <Button variant="outline" class="" onclick={() => addRule('service')} disabled={!newService.trim()}>
                  <Plus size={14} /> Add
                </Button>
              </div>
              
              {#if rules.services.length === 0}
                <div style="font-size:13px; color:var(--color-text-muted); font-style:italic">No services allowed.</div>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:8px">
                  {#each rules.services as svc}
                    <div class="rule-chip">
                      {svc}
                      <button class="rule-chip-del" onclick={() => confirmRemoveRule('service', svc)}>
                        <Trash2 size={12} />
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>

            <hr style="border:0; border-top:1px solid var(--color-border); margin:12px 0" />

              <div>
                <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Open Ports</h3>
                <div style="display:flex; gap:8px; margin-bottom:8px">
                  <input class="input" value={newPort} oninput={(e) => handlePortInput(e.currentTarget.value)} placeholder="e.g. 8080/tcp, 53/udp" onkeydown={(e) => e.key === 'Enter' && addRule('port')} />
                  <Button variant="outline" class="" onclick={() => addRule('port')} disabled={!newPort.trim()}>
                    <Plus size={14} /> Add
                  </Button>
                </div>

                {#if checkingPort}
                  <div style="font-size:11.5px; color:var(--color-text-muted); display:flex; align-items:center; gap:6px; margin-top:-4px; margin-bottom:8px;">
                    <RefreshCw size={12} class="animate-spin-slow" /> Checking local port listener...
                  </div>
                {:else if portCheckResult}
                  {#if portCheckResult.is_listening}
                    <div style="font-size:11.5px; color:var(--color-success); display:flex; align-items:center; gap:6px; margin-top:-4px; margin-bottom:8px;">
                      <ShieldCheck size={13} /> Active local listener detected: <strong>{portCheckResult.process_name || 'System service'}</strong> (PID: {portCheckResult.pid || '—'})
                    </div>
                  {:else}
                    <div style="font-size:11.5px; color:var(--color-text-muted); display:flex; align-items:center; gap:6px; margin-top:-4px; margin-bottom:8px;">
                      <span>⚪ No active local process currently listening on port {portCheckResult.port}/{portCheckResult.proto}</span>
                    </div>
                  {/if}
                {/if}

              {#if rules.ports.length === 0}
                <div style="font-size:13px; color:var(--color-text-muted); font-style:italic">No specific ports opened.</div>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:8px">
                  {#each rules.ports as port}
                    <div class="rule-chip port-chip">
                      {port}
                      <button class="rule-chip-del" onclick={() => confirmRemoveRule('port', port)}>
                        <Trash2 size={12} />
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {:else if activeSubTab === 'rich'}
            <div>
              <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Rich Rules Builder</h3>
              <div class="card" style="background:rgba(0,0,0,0.1); padding:16px; display:flex; flex-direction:column; gap:12px; margin-bottom:16px; border: 1px solid var(--color-border);">
                <div style="display:grid; grid-template-columns: repeat(3, 1fr); gap:12px;">
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">IP Family</span>
                    <Select bind:value={ruleBuilder.family}>
                      <option value="ipv4">IPv4</option>
                      <option value="ipv6">IPv6</option>
                    </Select>
                  </div>
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">Source Address (Optional)</span>
                    <input type="text" class="input" bind:value={ruleBuilder.source} placeholder="e.g. 192.168.1.0/24" />
                  </div>
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">Destination (Optional)</span>
                    <input type="text" class="input" bind:value={ruleBuilder.dest} placeholder="e.g. 10.0.0.5/32" />
                  </div>
                </div>

                <div style="display:grid; grid-template-columns: repeat(3, 1fr); gap:12px;">
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">Rule Target Element</span>
                    <Select bind:value={ruleBuilder.elemType}>
                      <option value="service">Service Name</option>
                      <option value="port">Port / Protocol</option>
                      <option value="protocol">Protocol Name</option>
                    </Select>
                  </div>
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">Element Value</span>
                    <input type="text" class="input" bind:value={ruleBuilder.elemVal} placeholder={ruleBuilder.elemType === 'service' ? 'e.g. http' : (ruleBuilder.elemType === 'port' ? 'e.g. 8080/tcp' : 'e.g. icmp')} />
                  </div>
                  <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11.5px; color:var(--color-text-secondary);">Firewall Action</span>
                    <Select bind:value={ruleBuilder.action}>
                      <option value="accept">Accept</option>
                      <option value="reject">Reject</option>
                      <option value="drop">Drop</option>
                    </Select>
                  </div>
                </div>

                <div style="display:flex; justify-content:flex-end; margin-top:8px;">
                  <Button variant="primary" onclick={addRichRule}>Add Rich Rule</Button>
                </div>
              </div>

              <h4 style="margin: 16px 0 8px; color:var(--color-text-primary); font-size:14px;">Active Rich Rules</h4>
              {#if richRules.length === 0}
                <div style="font-size:13px; color:var(--color-text-muted); font-style:italic;">No rich rules active for this zone.</div>
              {:else}
                <div style="display:flex; flex-direction:column; gap:8px;">
                  {#each richRules as rule}
                    <div style="display:flex; justify-content:space-between; align-items:center; background:rgba(255,255,255,0.01); padding:10px; border-radius:6px; border:1px solid var(--color-border); gap:12px;">
                      <code style="font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); word-break:break-all;">{rule}</code>
                      <Button variant="outline" style="color:var(--color-error); border-color:var(--color-error); padding: 4px 8px; font-size:11px; flex-shrink:0;" onclick={() => removeRichRule(rule)}>
                        Remove
                      </Button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {:else if activeSubTab === 'interfaces'}
            <div>
              <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Bound Network Interfaces</h3>
              {#if zoneInterfaces.length === 0}
                <div style="font-size:13px; color:var(--color-text-muted); font-style:italic; margin-bottom:16px;">No interfaces currently bound to this zone. Traffic is routed through the default zone.</div>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:8px; margin-bottom:16px;">
                  {#each zoneInterfaces as iface}
                    <div class="rule-chip" style="background: rgba(0, 218, 243, 0.05); border-color: var(--color-accent);">
                      <Network size={12} style="margin-right:6px; color:var(--color-accent);" /> {iface}
                    </div>
                  {/each}
                </div>
              {/if}

              <hr style="border:0; border-top:1px solid var(--color-border); margin:16px 0" />

              <h4 style="margin:0 0 8px; color:var(--color-text-primary); font-size:14px;">Bind System Interface to {activeZone}</h4>
              <p style="font-size:12.5px; color:var(--color-text-secondary); margin-bottom:12px; line-height:1.5;">
                Select an available hardware/virtual device adapter on your system to bind it to this firewall zone. Any incoming and outgoing packets on this adapter will be evaluated by this zone's rules.
              </p>
              
              <div style="display:flex; gap:12px; max-width:360px;">
                <Select id="select-bind-iface" value="">
                  <option value="" disabled>Select adapter...</option>
                  {#each allInterfaces as iface}
                    {#if !zoneInterfaces.includes(iface)}
                      <option value={iface}>{iface}</option>
                    {/if}
                  {/each}
                </Select>
                <Button variant="primary" style="flex-shrink:0;" onclick={() => {
                  const sel = document.getElementById('select-bind-iface') as HTMLSelectElement;
                  if (sel && sel.value) {
                    bindInterface(activeZone, sel.value);
                  }
                }}>
                  Bind Interface
                </Button>
              </div>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  :global(.zone-btn) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    color: var(--color-text-secondary);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: all 0.2s ease;
  }
  :global(.zone-btn:hover) {
    background: var(--color-bg-raised);
    color: var(--color-text-primary);
  }
  :global(.zone-btn.active) {
    background: var(--color-bg-raised);
    border-color: var(--color-border-focus);
    color: var(--color-accent);
  }

  .rule-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    padding: 4px 8px 4px 12px;
    border-radius: 16px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-primary);
  }
  .port-chip {
    font-family: var(--font-mono);
  }
  .rule-chip-del {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }
  .rule-chip-del:hover {
    color: var(--color-danger);
    background: rgba(255, 71, 87, 0.1);
  }
</style>
