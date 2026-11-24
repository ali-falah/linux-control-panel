<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Select from '../components/ui/Select.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';

  import {
    Shield, ShieldAlert, ShieldCheck, Power, RefreshCw, Trash2, Plus,
    Network, Sparkles, Copy, ArrowRight, Info, CheckCircle2, Server,
    Wifi, ListFilter
  } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';

  // ── Types ────────────────────────────────────────────────────────────────────

  interface FirewallState {
    is_running: boolean;
    is_panic: boolean;
    active_zones: string[];
    default_zone: string;
    backend: string;
    zones: string[];
  }

  interface ZoneRules {
    zone: string;
    services: string[];
    ports: string[];
  }

  interface PortListenerInfo {
    port: number;
    proto: string;
    is_listening: boolean;
    process_name: string;
    pid: string;
    address: string;
  }

  type SubTab = 'rules' | 'rich' | 'interfaces';

  // ── State ────────────────────────────────────────────────────────────────────

  let firewallState = $state<FirewallState | null>(null);
  let loading = $state(true);
  let loadError = $state('');

  let activeZone = $state('');
  let rules = $state<ZoneRules | null>(null);
  let loadingRules = $state(false);
  let rulesError = $state('');

  let newPort = $state('');
  let newService = $state('');
  let portCheckResult = $state<PortListenerInfo | null>(null);
  let portCheckError = $state('');
  let checkingPort = $state(false);
  let portDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  let activeSubTab = $state<SubTab>(
    uiStore.targetSubTab && ['rules', 'rich', 'interfaces'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as SubTab)
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
  let richError = $state('');
  let interfacesError = $state('');
  let richSearch = $state('');
  let interfaceToBind = $state('');

  let ruleBuilder = $state({
    family: 'ipv4',
    source: '',
    dest: '',
    elemType: 'service',
    elemVal: '',
    action: 'accept'
  });

  let aiPromptInput = $state('');
  let showAiPromptBox = $state(false);

  // ── Derived ──────────────────────────────────────────────────────────────────

  let isUfw = $derived(firewallState?.backend === 'ufw');
  let supportsRich = $derived(!isUfw && !!firewallState?.is_running);
  let supportsInterfaces = $derived(!isUfw && !!firewallState?.is_running);
  let zones = $derived(
    firewallState
      ? ((firewallState.zones?.length ?? 0) > 0 ? firewallState.zones : firewallState.active_zones)
      : []
  );
  let activeZoneSet = $derived(new Set(firewallState?.active_zones ?? []));
  let activeZoneIsDefault = $derived(activeZone !== '' && activeZone === firewallState?.default_zone);
  let availableInterfaces = $derived(
    allInterfaces.filter((iface) => !zoneInterfaces.includes(iface))
  );
  let filteredRichRules = $derived.by(() => {
    const query = richSearch.trim().toLowerCase();
    return query
      ? richRules.filter((rule) => rule.toLowerCase().includes(query))
      : richRules;
  });
  let richPreview = $derived.by(() =>
    ruleBuilder.elemVal.trim() ? composeRichRule() : ''
  );
  let subTabs = $derived([
    { id: 'rules', label: 'Basic Rules', icon: ShieldCheck, count: (rules?.services.length ?? 0) + (rules?.ports.length ?? 0) },
    ...(supportsRich ? [{ id: 'rich', label: 'Rich Rules', icon: ListFilter, count: richRules.length }] : []),
    ...(supportsInterfaces ? [{ id: 'interfaces', label: 'Interfaces', icon: Network, count: zoneInterfaces.length }] : [])
  ]);

  // ── SSH lockout guard plumbing ───────────────────────────────────────────────
  // Kept byte-for-byte compatible with the helper's fail-closed semantics.

  function portNumber(value: string): number | null {
    return /^\d+$/.test(value) ? Number(value) : null;
  }

  function portStringCovers22(port: string): boolean {
    const parts = port.split('/')[0].split('-');
    if (parts.length > 2) return true;
    const start = portNumber(parts[0]);
    const end = portNumber(parts[parts.length - 1]);
    if (start === null || end === null || start > 65535 || end > 65535) return true;
    return start <= 22 && 22 <= end;
  }

  function ruleRemovalCutsSsh(type: 'port' | 'service', val: string): boolean {
    return (type === 'service' && val === 'ssh') || (type === 'port' && portStringCovers22(val));
  }

  function richRuleBlocksSsh(): boolean {
    return (
      (ruleBuilder.action === 'drop' || ruleBuilder.action === 'reject') &&
      ((ruleBuilder.elemType === 'service' && ruleBuilder.elemVal.trim() === 'ssh') ||
        (ruleBuilder.elemType === 'port' && portStringCovers22(ruleBuilder.elemVal.trim())))
    );
  }

  function handleSshConfirmation(error: unknown, context: string, retry: () => Promise<void>) {
    const message = String(error);
    if (message.startsWith('CONFIRMATION_REQUIRED:')) {
      uiStore.confirm('SSH Session At Risk', message, retry, true);
    } else {
      uiStore.addToast(`${context}: ${message}`, 'error');
    }
  }

  function composeRichRule(): string {
    let rule = `rule family="${ruleBuilder.family}"`;
    if (ruleBuilder.source.trim()) {
      rule += ` source address="${ruleBuilder.source.trim()}"`;
    }
    if (ruleBuilder.dest.trim()) {
      rule += ` destination address="${ruleBuilder.dest.trim()}"`;
    }
    if (ruleBuilder.elemType === 'service') {
      rule += ` service name="${ruleBuilder.elemVal.trim()}"`;
    } else if (ruleBuilder.elemType === 'port') {
      let val = ruleBuilder.elemVal.trim();
      if (/^\d+$/.test(val)) val += '/tcp';
      const parts = val.split('/');
      rule += parts.length === 2
        ? ` port port="${parts[0]}" protocol="${parts[1]}"`
        : ` port port="${val}" protocol="tcp"`;
    } else {
      rule += ` protocol value="${ruleBuilder.elemVal.trim()}"`;
    }
    rule += ` ${ruleBuilder.action}`;
    return rule;
  }

  // ── Loading ──────────────────────────────────────────────────────────────────

  onMount(() => {
    loadState();
    return () => {
      if (portDebounceTimer) clearTimeout(portDebounceTimer);
    };
  });

  async function loadState() {
    loading = true;
    loadError = '';
    rulesError = '';
    richError = '';
    interfacesError = '';
    statusStore.setBusy('Loading firewall state…');
    try {
      const state = await invoke<FirewallState>('get_firewall_state');
      firewallState = state;
      if (!state.is_running) {
        rules = null;
        richRules = [];
        zoneInterfaces = [];
        allInterfaces = [];
        activeZone = state.default_zone || '';
        return;
      }

      const knownZones = (state.zones?.length ?? 0) > 0 ? state.zones : state.active_zones;
      const preferred =
        (activeZone && knownZones.includes(activeZone) ? activeZone : '') ||
        (state.default_zone && knownZones.includes(state.default_zone) ? state.default_zone : '') ||
        state.active_zones[0] ||
        knownZones[0] ||
        '';

      activeZone = preferred;
      if (state.backend === 'ufw' && activeSubTab !== 'rules') activeSubTab = 'rules';
      if (preferred) {
        await loadRules(preferred);
      } else {
        rules = { zone: '', services: [], ports: [] };
        richRules = [];
        zoneInterfaces = [];
        allInterfaces = [];
      }
      statusStore.setLastCommand('firewall-cmd --state', 0, true);
    } catch (e) {
      loadError = String(e);
      firewallState = null;
      statusStore.setLastCommand('firewall-cmd', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function loadRules(zone: string) {
    if (!zone) return;
    loadingRules = true;
    rulesError = '';
    try {
      rules = await invoke<ZoneRules>('get_zone_rules', { zone });
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 0, true);
      if (firewallState?.backend !== 'ufw') {
        await Promise.allSettled([loadRichRules(zone), loadInterfaces(zone)]);
      }
    } catch (e) {
      rulesError = String(e);
      statusStore.setLastCommand(`firewall-cmd --zone=${zone} --list-all`, 1, false);
    } finally {
      loadingRules = false;
    }
  }

  async function loadRichRules(zone: string) {
    if (!zone || isUfw) return;
    loadingRich = true;
    richError = '';
    try {
      richRules = await invoke<string[]>('firewall_get_rich_rules', { zone });
    } catch (e) {
      richError = String(e);
    } finally {
      loadingRich = false;
    }
  }

  async function loadInterfaces(zone: string) {
    if (!zone || isUfw) return;
    loadingInterfaces = true;
    interfacesError = '';
    try {
      zoneInterfaces = await invoke<string[]>('firewall_get_zone_interfaces', { zone });
      allInterfaces = await invoke<string[]>('firewall_get_all_interfaces');
      if (interfaceToBind && zoneInterfaces.includes(interfaceToBind)) {
        interfaceToBind = '';
      }
    } catch (e) {
      interfacesError = String(e);
    } finally {
      loadingInterfaces = false;
    }
  }

  function selectZone(zone: string) {
    if (zone === activeZone) return;
    activeZone = zone;
    if (firewallState?.backend === 'ufw' && activeSubTab !== 'rules') activeSubTab = 'rules';
    portCheckResult = null;
    portCheckError = '';
    loadRules(zone);
  }

  // ── Port listener checker ────────────────────────────────────────────────────

  function handlePortInput(value: string) {
    newPort = value;
    if (portDebounceTimer) clearTimeout(portDebounceTimer);
    const cleaned = value.trim();
    portCheckError = '';
    if (!cleaned) {
      portCheckResult = null;
      return;
    }
    const parts = cleaned.split('/');
    const portNum = parseInt(parts[0], 10);
    const proto = parts.length > 1 ? parts[1] : 'tcp';
    if (Number.isNaN(portNum) || portNum <= 0 || portNum > 65535) {
      portCheckResult = null;
      return;
    }
    portDebounceTimer = setTimeout(async () => {
      checkingPort = true;
      try {
        portCheckResult = await invoke<PortListenerInfo>('firewall_check_port_listener', { port: portNum, proto });
      } catch (e) {
        portCheckResult = null;
        portCheckError = String(e);
      } finally {
        checkingPort = false;
      }
    }, 220);
  }

  function listenerScope(address: string): string {
    if (!address) return '';
    if (address.startsWith('0.0.0.0:') || address.startsWith('[::]:') || address.startsWith('*:')) {
      return 'All interfaces';
    }
    if (address.startsWith('127.') || address.startsWith('[::1]')) return 'Loopback only';
    return 'Specific address';
  }

  // ── Basic rules ──────────────────────────────────────────────────────────────

  async function addRule(type: 'port' | 'service') {
    let val = type === 'port' ? newPort.trim() : newService.trim().toLowerCase();
    if (!val) return;
    if (type === 'port' && /^\d+$/.test(val)) val += '/tcp';

    const helperZone = firewallState?.backend === 'ufw' ? '' : activeZone;
    statusStore.setBusy(`Adding ${type} ${val}…`);
    try {
      await invoke('modify_firewall_rule', { zone: helperZone, ruleType: type, value: val, add: true });
      statusStore.setLastCommand(`${isUfw ? 'ufw' : 'firewall-cmd --zone=' + activeZone} --add-${type}=${val}`, 0, true);
      uiStore.addToast(`Added ${type} ${val}.`, 'success');
      if (type === 'port') {
        newPort = '';
        portCheckResult = null;
        portCheckError = '';
      } else {
        newService = '';
      }
      await loadRules(activeZone);
    } catch (e) {
      statusStore.setLastCommand(`${isUfw ? 'ufw' : 'firewall-cmd --zone=' + activeZone} --add-${type}=${val}`, 1, false);
      uiStore.addToast(`Failed to add ${type}: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmRemoveRule(type: 'port' | 'service', val: string) {
    const viewZone = activeZone;
    const helperZone = firewallState?.backend === 'ufw' ? '' : viewZone;
    const message = ruleRemovalCutsSsh(type, val)
      ? `WARNING: ${val} is what your active SSH session is connected through. Removing it can cut off your session and permanently lock you out of the firewall.`
      : `Remove ${type} ${val} from ${isUfw ? 'UFW' : `zone ${viewZone}`}? This closes the corresponding inbound access.`;
    uiStore.confirm(
      `Remove ${type === 'port' ? 'Port' : 'Service'}`,
      message,
      () => doRemoveRule(viewZone, helperZone, type, val),
      true
    );
  }

  async function doRemoveRule(viewZone: string, helperZone: string, type: 'port' | 'service', val: string, confirmed = false) {
    statusStore.setBusy(`Removing ${type} ${val}…`);
    const label = isUfw ? 'ufw' : `firewall-cmd --zone=${viewZone}`;
    try {
      await invoke('modify_firewall_rule', { zone: helperZone, ruleType: type, value: val, add: false, confirmed });
      statusStore.setLastCommand(`${label} --remove-${type}=${val}`, 0, true);
      uiStore.addToast(`Removed ${type} ${val}.`, 'success');
      await loadRules(viewZone);
    } catch (e) {
      statusStore.setLastCommand(`${label} --remove-${type}=${val}`, 1, false);
      handleSshConfirmation(e, `Failed to remove ${type} ${val}`, () =>
        doRemoveRule(viewZone, helperZone, type, val, true)
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  // ── Rich rules ───────────────────────────────────────────────────────────────

  async function addRichRule() {
    if (!ruleBuilder.elemVal.trim()) {
      uiStore.addToast('A service, port, or protocol value is required.', 'warning');
      return;
    }
    const zone = activeZone;
    const rule = composeRichRule();

    const submit = async (confirmed: boolean) => {
      statusStore.setBusy('Adding rich rule…');
      try {
        await invoke('firewall_modify_rich_rule', { zone, rule, add: true, confirmed });
        uiStore.addToast('Rich rule added.', 'success');
        ruleBuilder.elemVal = '';
        ruleBuilder.source = '';
        ruleBuilder.dest = '';
        await loadRichRules(zone);
      } catch (e) {
        handleSshConfirmation(e, 'Failed to add rich rule', () => submit(true));
      } finally {
        statusStore.clearBusy();
      }
    };

    if (richRuleBlocksSsh()) {
      uiStore.confirm(
        'SSH Rule May Cut Your Connection',
        `This ${ruleBuilder.action} rule targets SSH and can interrupt your active SSH session, possibly locking you out of the firewall.`,
        () => submit(true),
        true
      );
    } else {
      await submit(false);
    }
  }

  function askRemoveRichRule(rule: string) {
    uiStore.confirm(
      'Remove rich rule',
      `Remove this rich rule from zone ${activeZone}? Traffic it currently matches will be evaluated by the remaining rules.`,
      () => doRemoveRichRule(rule),
      false
    );
  }

  async function doRemoveRichRule(rule: string) {
    statusStore.setBusy('Removing rich rule…');
    try {
      await invoke('firewall_modify_rich_rule', { zone: activeZone, rule, add: false, confirmed: false });
      uiStore.addToast('Rich rule removed.', 'success');
      await loadRichRules(activeZone);
    } catch (e) {
      uiStore.addToast(`Failed to remove rich rule: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  // ── Interface bindings ───────────────────────────────────────────────────────

  async function bindInterface(zone: string, iface: string, confirmed = false) {
    if (!zone || !iface) return;
    statusStore.setBusy(`Binding ${iface} to ${zone}…`);
    try {
      await invoke('firewall_change_interface_zone', { zone, interface: iface, confirmed });
      uiStore.addToast(`Interface ${iface} bound to ${zone}.`, 'success');
      interfaceToBind = '';
      await loadInterfaces(activeZone);
      await loadState();
    } catch (e) {
      handleSshConfirmation(e, `Failed to bind interface ${iface}`, () =>
        bindInterface(zone, iface, true)
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  function bindSelectedInterface() {
    if (interfaceToBind) bindInterface(activeZone, interfaceToBind);
  }

  function moveInterfaceToDefault(iface: string) {
    const defaultZone = firewallState?.default_zone;
    if (!defaultZone || defaultZone === activeZone) return;
    uiStore.confirm(
      'Move interface to default zone',
      `Move ${iface} from ${activeZone} to the default zone ${defaultZone}? The interface may lose rules currently applied by ${activeZone}.`,
      () => bindInterface(defaultZone, iface),
      false
    );
  }

  // ── Panic mode ───────────────────────────────────────────────────────────────

  function confirmTogglePanic() {
    const enable = !firewallState?.is_panic;
    if (!enable) {
      doTogglePanic(false, false);
      return;
    }
    uiStore.confirm(
      'Enable Panic Mode',
      'WARNING: Enabling Panic Mode immediately drops all incoming and outgoing network traffic. If you are connected over SSH, the connection and any other remote administration will be cut.',
      () => doTogglePanic(enable, true),
      true
    );
  }

  async function doTogglePanic(enable: boolean, confirmed = false) {
    statusStore.setBusy(`${enable ? 'Enabling' : 'Disabling'} panic mode…`);
    try {
      await invoke('toggle_panic_mode', { enable, confirmed });
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 0, true);
      uiStore.addToast(`Panic mode ${enable ? 'enabled' : 'disabled'}.`, 'success');
      await loadState();
    } catch (e) {
      statusStore.setLastCommand(`firewall-cmd --panic-${enable ? 'on' : 'off'}`, 1, false);
      handleSshConfirmation(e, 'Failed to toggle panic mode', () => doTogglePanic(enable, true));
    } finally {
      statusStore.clearBusy();
    }
  }

  // ── AI rule generator ────────────────────────────────────────────────────────

  function triggerAiRuleGen() {
    if (!aiPromptInput.trim()) return;
    aiStore.generateFirewallRule(aiPromptInput);
    showAiPromptBox = false;
    aiPromptInput = '';
  }

  async function copyToClipboard(value: string, label: string) {
    try {
      await navigator.clipboard.writeText(value);
      uiStore.addToast(`${label} copied.`, 'success');
    } catch {
      uiStore.addToast('Clipboard access was denied.', 'error');
    }
  }
</script>

<div class="module-page firewall-page">
  <PageHeader title="Firewall Manager" subtitle="Manage zones, services, ports, and interface bindings" icon={Shield}>
    {#if firewallState}
      <Badge variant={isUfw ? 'warning' : 'info'}>{isUfw ? 'UFW' : 'firewalld'}</Badge>
      {#if firewallState.is_panic}
        <Badge variant="error">Panic mode active</Badge>
      {:else if firewallState.is_running}
        <Badge variant="success">Active</Badge>
      {:else}
        <Badge variant="muted">Stopped</Badge>
      {/if}
    {/if}
    {#if aiStore.enabled}
      <Button variant="outline" size="sm" onclick={() => (showAiPromptBox = !showAiPromptBox)} title="Generate a firewalld rule with AI">
        <Sparkles size={14} color="var(--color-accent)" /> AI rule
      </Button>
    {/if}
    <button class="header-refresh-btn" onclick={loadState} disabled={loading} title="Reload firewall state">
      <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </button>
    {#if firewallState?.is_running && !isUfw}
      <Button
        size="sm"
        variant={firewallState.is_panic ? 'primary' : 'danger'}
        onclick={confirmTogglePanic}
      >
        <Power size={14} /> {firewallState.is_panic ? 'Disable panic' : 'Panic mode'}
      </Button>
    {/if}
  </PageHeader>

  {#if showAiPromptBox}
    <section class="fw-card ai-card">
      <div class="ai-head">
        <div>
          <h3><Sparkles size={15} /> AI firewall rule generator</h3>
          <p>Describe the rule; the AI assistant will build a firewalld rich rule for review.</p>
        </div>
        <Button variant="ghost" size="sm" onclick={() => (showAiPromptBox = false)}>Cancel</Button>
      </div>
      <div class="add-row">
        <Input
          bind:value={aiPromptInput}
          placeholder="e.g. Allow 192.168.1.50 to reach port 8080 only"
          aria-label="Describe the firewall rule"
          onkeydown={(event) => event.key === 'Enter' && triggerAiRuleGen()}
        />
        <Button variant="primary" size="sm" onclick={triggerAiRuleGen} disabled={!aiPromptInput.trim()}>
          Generate
        </Button>
      </div>
    </section>
  {/if}

  {#if loading && !firewallState}
    <div class="fw-card fw-loading">
      <RefreshCw size={22} class="animate-spin-slow" />
      <span>Loading firewall state…</span>
    </div>
  {:else if loadError}
    <EmptyState
      icon={ShieldAlert}
      title="Firewall data unavailable"
      description={loadError}
      actionLabel="Retry"
      actionIcon={RefreshCw}
      onAction={loadState}
    />
  {:else if firewallState && !firewallState.is_running}
    <EmptyState
      icon={ShieldAlert}
      title={isUfw ? 'UFW is not active' : 'Firewall is not running'}
      description={isUfw
        ? 'UFW is installed but inactive. Enable it from a terminal with "sudo ufw enable", then reload this page.'
        : 'firewalld is installed but not running. Start it from the Service Manager, then reload this page.'}
      actionLabel={isUfw ? undefined : 'Open Service Manager'}
      actionIcon={isUfw ? undefined : Server}
      onAction={isUfw ? undefined : () => uiStore.navigateTo('service-manager')}
    />
  {:else if firewallState}
    <div class="fw-overview">
      <KpiCard icon={Wifi} value={zones.length} label="Zones" subtext={`${firewallState.active_zones.length} active`} />
      <KpiCard icon={ShieldCheck} value={rules?.services.length ?? 0} label="Services" subtext={`Zone ${activeZone || '—'}`} />
      <KpiCard icon={Network} value={rules?.ports.length ?? 0} label="Open ports" subtext={`Zone ${activeZone || '—'}`} />
      <KpiCard
        icon={ListFilter}
        value={supportsRich ? richRules.length : '—'}
        label="Rich rules"
        subtext={supportsRich ? 'Ordered rules' : 'firewalld only'}
      />
    </div>

    {#if firewallState.is_panic}
      <div class="fw-callout danger">
        <ShieldAlert size={18} />
        <div>
          <strong>Panic mode is active</strong>
          <p>All incoming and outgoing network traffic is currently blocked. Disable panic mode to restore normal rules.</p>
        </div>
        <Button variant="primary" size="sm" onclick={confirmTogglePanic}>
          <Power size={13} /> Disable panic
        </Button>
      </div>
    {/if}

    <div class="fw-layout" class:single-column={isUfw}>
      {#if !isUfw}
        <aside class="fw-card zone-panel">
          <div class="card-head">
            <div>
              <h3>Zones</h3>
              <p>Traffic is filtered by the zone it enters.</p>
            </div>
            <Badge variant="muted">{zones.length}</Badge>
          </div>
          <div class="zone-list">
            {#each zones as zone (zone)}
              <button
                type="button"
                class="zone-item"
                class:active={activeZone === zone}
                onclick={() => selectZone(zone)}
                title={`Inspect ${zone} zone`}
              >
                <span class="zone-icon"><Shield size={14} /></span>
                <span class="zone-name mono">{zone}</span>
                {#if activeZoneSet.has(zone)}
                  <span class="active-dot" title="Active zone" aria-label="Active zone"></span>
                {/if}
                {#if zone === firewallState.default_zone}<Badge variant="muted">Default</Badge>{/if}
              </button>
            {/each}
          </div>
          {#if !activeZoneIsDefault && firewallState.default_zone}
            <p class="zone-hint">Default zone: <strong class="mono">{firewallState.default_zone}</strong></p>
          {/if}
        </aside>
      {/if}

      <main class="fw-main">
        <div class="fw-card zone-header">
          <div>
            <h2>{isUfw ? 'UFW rules' : activeZone || 'No zone selected'}</h2>
            <p>
              {#if isUfw}
                UFW applies global rules; rich rules and interface-zone bindings are not available.
              {:else if activeZone}
                Services and ports allowed for traffic entering <strong>{activeZone}</strong>.
              {/if}
            </p>
          </div>
          <div class="zone-header-badges">
            {#if activeZone && activeZoneSet.has(activeZone)}<Badge variant="success">Active</Badge>{/if}
            {#if activeZoneIsDefault}<Badge variant="muted">Default</Badge>{/if}
            {#if !isUfw && activeZone}
              <Button variant="ghost" size="sm" onclick={() => loadRules(activeZone)} title="Reload this zone">
                <RefreshCw size={12} class={loadingRules ? 'animate-spin-slow' : ''} /> Refresh
              </Button>
            {/if}
          </div>
        </div>

        {#if rulesError}
          <div class="fw-callout danger" role="alert">
            <ShieldAlert size={16} />
            <div><strong>Could not load this zone</strong><p>{rulesError}</p></div>
          </div>
        {/if}

        {#if rules}
          <TabGroup tabs={subTabs} bind:activeTab={activeSubTab} size="sm" />

          {#if loadingRules}
            <div class="fw-card fw-loading"><RefreshCw size={18} class="animate-spin-slow" /><span>Loading zone rules…</span></div>
          {:else if activeSubTab === 'rules'}
            <div class="rules-grid">
              <section class="fw-card">
                <div class="card-head">
                  <div><h3>Allowed services</h3><p>Named service definitions allowed in this zone.</p></div>
                  <Badge variant="muted">{rules.services.length}</Badge>
                </div>
                <div class="add-row">
                  <Input
                    bind:value={newService}
                    placeholder="http, https, ssh…"
                    aria-label="Service name to allow"
                    onkeydown={(event) => event.key === 'Enter' && addRule('service')}
                  />
                  <Button variant="outline" size="sm" onclick={() => addRule('service')} disabled={!newService.trim()}>
                    <Plus size={13} /> Add
                  </Button>
                </div>
                {#if rules.services.length === 0}
                  <div class="mini-empty"><ShieldCheck size={18} /><span>No services are allowed in this zone.</span></div>
                {:else}
                  <div class="rule-list">
                    {#each rules.services as svc (svc)}
                      <div class="rule-row">
                        <span class="rule-icon"><ShieldCheck size={13} /></span>
                        <span class="rule-value mono">{svc}</span>
                        <div class="rule-actions">
                          <button class="icon-btn" title="Copy service name" aria-label="Copy service name" onclick={() => copyToClipboard(svc, 'Service name')}><Copy size={12} /></button>
                          <button class="icon-btn danger" title="Remove service" aria-label="Remove service" onclick={() => confirmRemoveRule('service', svc)}><Trash2 size={12} /></button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </section>

              <section class="fw-card">
                <div class="card-head">
                  <div><h3>Open ports</h3><p>Explicit port/protocol openings in this zone.</p></div>
                  <Badge variant="muted">{rules.ports.length}</Badge>
                </div>
                <div class="add-row">
                  <Input
                    bind:value={newPort}
                    oninput={(event) => handlePortInput(event.currentTarget.value)}
                    placeholder="8080/tcp, 53/udp…"
                    aria-label="Port and protocol to open"
                    onkeydown={(event) => event.key === 'Enter' && addRule('port')}
                  />
                  <Button variant="outline" size="sm" onclick={() => addRule('port')} disabled={!newPort.trim()}>
                    <Plus size={13} /> Add
                  </Button>
                </div>

                {#if checkingPort}
                  <div class="listener-hint"><RefreshCw size={12} class="animate-spin-slow" /> Checking local listener…</div>
                {:else if portCheckError}
                  <div class="listener-hint warning"><Info size={12} /> Could not inspect listeners: {portCheckError}</div>
                {:else if portCheckResult}
                  <div class="listener-hint" class:listening={portCheckResult.is_listening}>
                    {#if portCheckResult.is_listening}
                      <CheckCircle2 size={13} />
                      <span><strong>{portCheckResult.process_name || 'Local process'}</strong> is listening{portCheckResult.pid ? ` (PID ${portCheckResult.pid})` : ''} on {portCheckResult.address} · {listenerScope(portCheckResult.address)}</span>
                    {:else}
                      <Info size={13} />
                      <span>No local process is listening on {portCheckResult.port}/{portCheckResult.proto}.</span>
                    {/if}
                  </div>
                {/if}

                {#if rules.ports.length === 0}
                  <div class="mini-empty"><Network size={18} /><span>No explicit ports are open in this zone.</span></div>
                {:else}
                  <div class="rule-list">
                    {#each rules.ports as port (port)}
                      <div class="rule-row">
                        <span class="rule-icon"><Network size={13} /></span>
                        <span class="rule-value mono">{port}</span>
                        <div class="rule-actions">
                          <button class="icon-btn" title="Copy port" aria-label="Copy port" onclick={() => copyToClipboard(port, 'Port')}><Copy size={12} /></button>
                          <button class="icon-btn danger" title="Remove port" aria-label="Remove port" onclick={() => confirmRemoveRule('port', port)}><Trash2 size={12} /></button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </section>
            </div>
          {:else if activeSubTab === 'rich'}
            {#if !supportsRich}
              <div class="fw-callout info"><Info size={15} /><span>Rich rules are a firewalld feature and are not available with UFW.</span></div>
            {:else}
              <section class="fw-card">
                <div class="card-head">
                  <div><h3>Rich rule builder</h3><p>Fine-grained source/destination rules using firewalld rich language.</p></div>
                  <Badge variant="info">firewalld</Badge>
                </div>
                <div class="builder-grid">
                  <Select label="IP family" bind:value={ruleBuilder.family}>
                    <option value="ipv4">IPv4</option>
                    <option value="ipv6">IPv6</option>
                  </Select>
                  <Input label="Source address (optional)" bind:value={ruleBuilder.source} placeholder="192.168.1.0/24" class="mono" />
                  <Input label="Destination (optional)" bind:value={ruleBuilder.dest} placeholder="10.0.0.5/32" class="mono" />
                  <Select label="Match element" bind:value={ruleBuilder.elemType}>
                    <option value="service">Service name</option>
                    <option value="port">Port / protocol</option>
                    <option value="protocol">Protocol</option>
                  </Select>
                  <Input
                    label="Element value"
                    bind:value={ruleBuilder.elemVal}
                    placeholder={ruleBuilder.elemType === 'service' ? 'http' : ruleBuilder.elemType === 'port' ? '8080/tcp' : 'icmp'}
                    class="mono"
                  />
                  <Select label="Action" bind:value={ruleBuilder.action}>
                    <option value="accept">Accept</option>
                    <option value="reject">Reject</option>
                    <option value="drop">Drop</option>
                  </Select>
                </div>
                {#if richPreview}
                  <div class="rule-preview"><span class="preview-label">Rule preview</span><code>{richPreview}</code></div>
                {/if}
                <div class="builder-footer">
                  <span class="field-hint">Use a source or destination to scope the rule; leave both empty to match all traffic.</span>
                  <Button variant="primary" onclick={addRichRule}>Add rich rule</Button>
                </div>
              </section>

              {#if richError}
                <div class="fw-callout danger" role="alert"><ShieldAlert size={15} /><span>{richError}</span></div>
              {/if}

              <section class="fw-card">
                <div class="card-head">
                  <div><h3>Active rich rules</h3><p>Evaluated in order before the zone's basic rules.</p></div>
                  <Badge variant="muted">{richRules.length}</Badge>
                </div>
                {#if richRules.length > 3}
                  <SearchBar bind:value={richSearch} placeholder="Search rich rules…" count={filteredRichRules.length} total={richRules.length} />
                {/if}
                {#if loadingRich}
                  <div class="mini-empty"><RefreshCw size={16} class="animate-spin-slow" /><span>Loading rich rules…</span></div>
                {:else if filteredRichRules.length === 0}
                  <div class="mini-empty"><ListFilter size={18} /><span>{richSearch ? 'No rules match this search.' : 'No rich rules in this zone.'}</span></div>
                {:else}
                  <div class="rich-list">
                    {#each filteredRichRules as rule (rule)}
                      <div class="rich-row">
                        <code>{rule}</code>
                        <div class="rule-actions">
                          <button class="icon-btn" title="Copy rich rule" aria-label="Copy rich rule" onclick={() => copyToClipboard(rule, 'Rich rule')}><Copy size={12} /></button>
                          <button class="icon-btn danger" title="Remove rich rule" aria-label="Remove rich rule" onclick={() => askRemoveRichRule(rule)}><Trash2 size={12} /></button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </section>
            {/if}
          {:else if activeSubTab === 'interfaces'}
            {#if !supportsInterfaces}
              <div class="fw-callout info"><Info size={15} /><span>Interface-zone bindings are a firewalld feature and are not available with UFW.</span></div>
            {:else}
              <div class="rules-grid">
                <section class="fw-card">
                  <div class="card-head">
                    <div><h3>Interfaces in {activeZone}</h3><p>Packets entering these interfaces follow this zone's rules.</p></div>
                    <Badge variant="muted">{zoneInterfaces.length}</Badge>
                  </div>
                  {#if interfacesError}
                    <div class="fw-callout danger" role="alert"><ShieldAlert size={14} /><span>{interfacesError}</span></div>
                  {/if}
                  {#if loadingInterfaces}
                    <div class="mini-empty"><RefreshCw size={16} class="animate-spin-slow" /><span>Loading interfaces…</span></div>
                  {:else if zoneInterfaces.length === 0}
                    <div class="mini-empty"><Network size={18} /><span>No interfaces are bound to this zone.</span></div>
                  {:else}
                    <div class="interface-list">
                      {#each zoneInterfaces as iface (iface)}
                        <div class="interface-row">
                          <span class="rule-icon"><Network size={13} /></span>
                          <span class="rule-value mono">{iface}</span>
                          {#if firewallState.default_zone && firewallState.default_zone !== activeZone}
                            <Button variant="ghost" size="sm" onclick={() => moveInterfaceToDefault(iface)} title={`Move to ${firewallState.default_zone}`}>
                              <ArrowRight size={12} /> Default zone
                            </Button>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  {/if}
                </section>

                <section class="fw-card">
                  <div class="card-head">
                    <div><h3>Bind an interface</h3><p>Move an available interface into {activeZone}.</p></div>
                    <Badge variant="info">{availableInterfaces.length} available</Badge>
                  </div>
                  {#if availableInterfaces.length === 0}
                    <div class="mini-empty"><CheckCircle2 size={18} /><span>All detected interfaces are already bound to this zone.</span></div>
                  {:else}
                    <div class="bind-row">
                      <Select bind:value={interfaceToBind} label="Network interface" disabled={loadingInterfaces}>
                        <option value="">Select an interface…</option>
                        {#each availableInterfaces as iface (iface)}
                          <option value={iface}>{iface}</option>
                        {/each}
                      </Select>
                      <Button variant="primary" onclick={bindSelectedInterface} disabled={!interfaceToBind}>
                        <Network size={13} /> Bind
                      </Button>
                    </div>
                    <p class="field-hint">Binding changes where the interface enters the firewall. The SSH session guard will ask before a change that could cut your connection.</p>
                  {/if}
                </section>
              </div>
            {/if}
          {/if}
        {:else}
          <div class="fw-card fw-loading"><RefreshCw size={18} class="animate-spin-slow" /><span>Loading zone rules…</span></div>
        {/if}
      </main>
    </div>
  {/if}
</div>

<style>
  .firewall-page { display: flex; flex-direction: column; gap: var(--page-content-gap); padding: 24px; min-height: 100%; box-sizing: border-box; }
  .header-refresh-btn {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 6px 10px; border-radius: 7px;
    border: 1px solid var(--color-border); background: var(--color-bg-card);
    color: var(--color-text-secondary); font-size: 12px; cursor: pointer;
  }
  .header-refresh-btn:hover:not(:disabled) { color: var(--color-text-primary); border-color: var(--color-accent); }
  .header-refresh-btn:disabled { opacity: .55; cursor: default; }

  .fw-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 14px 16px;
    box-sizing: border-box;
    display: flex; flex-direction: column; gap: 10px;
    min-width: 0;
  }
  .fw-loading {
    flex-direction: row; align-items: center; justify-content: center;
    gap: 10px; padding: 34px; color: var(--color-text-muted);
  }
  .fw-overview { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 8px; }
  .fw-layout { display: grid; grid-template-columns: 235px minmax(0, 1fr); gap: 12px; align-items: start; }
  .fw-layout.single-column { grid-template-columns: minmax(0, 1fr); }
  .fw-main { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .fw-main :global(.tab-bar.size-sm) { align-self: flex-start !important; }

  .zone-panel { position: sticky; top: 0; gap: 8px; }
  .zone-list {
    display: flex; flex-direction: column; gap: 4px;
    max-height: calc(100vh - 350px); overflow-y: auto; padding-right: 2px;
    scrollbar-width: thin;
  }
  .zone-item {
    display: flex; align-items: center; gap: 7px;
    width: 100%; padding: 7px 8px; border-radius: 7px;
    border: 1px solid transparent; background: transparent;
    color: var(--color-text-secondary); cursor: pointer; text-align: left;
    font-size: 12px;
  }
  .zone-item:hover { background: var(--color-bg-raised); color: var(--color-text-primary); }
  .zone-item.active { background: var(--color-bg-raised); border-color: var(--color-border); color: var(--color-text-primary); }
  .zone-icon { color: var(--color-text-muted); display: inline-flex; }
  .zone-item.active .zone-icon { color: var(--color-accent); }
  .zone-name { flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .active-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: var(--color-success);
    box-shadow: 0 0 6px color-mix(in srgb, var(--color-success) 65%, transparent);
  }
  .zone-hint { margin: 0; font-size: 10.5px; color: var(--color-text-muted); }

  .card-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; }
  .card-head h3 { margin: 0; font-size: 13px; font-weight: 650; color: var(--color-text-primary); }
  .card-head p { margin: 2px 0 0; font-size: 10.5px; line-height: 1.35; color: var(--color-text-muted); }

  .zone-header { flex-direction: row; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .zone-header h2 { margin: 0; font-size: 17px; letter-spacing: -.01em; color: var(--color-text-primary); }
  .zone-header p { margin: 2px 0 0; font-size: 11.5px; color: var(--color-text-muted); }
  .zone-header-badges { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }

  .rules-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 10px; align-items: start; }
  .add-row { display: flex; align-items: center; gap: 8px; }
  .add-row > :global(.input-wrap), .add-row > :global(div) { flex: 1; min-width: 0; }
  .add-row :global(input) { width: 100%; }
  /* Inputs, the Add button and the interface Select share one row height, so
     their tops and bottoms line up instead of the button floating short. */
  .add-row :global(input.input),
  .bind-row :global(input.input) {
    height: 36px; padding: 0 10px; font-size: 12.5px;
  }
  .add-row :global(.btn),
  .bind-row :global(.btn) {
    height: 36px !important;
    min-height: 36px !important;
    max-height: 36px !important;
    border-radius: 7px !important;
  }
  .bind-row { display: flex; align-items: flex-end; gap: 8px; }
  .bind-row > :global(div) { flex: 1; min-width: 0; }

  .rule-list, .rich-list, .interface-list { display: flex; flex-direction: column; gap: 4px; }
  .rule-row, .interface-row {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 8px; border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--color-border) 65%, transparent);
    background: var(--color-bg-surface);
    min-width: 0;
  }
  .rule-icon { color: var(--color-text-muted); display: inline-flex; }
  .rule-value { flex: 1; font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; }
  .rule-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    border: 0; background: transparent; color: var(--color-text-muted);
    cursor: pointer; padding: 3px; border-radius: 4px;
  }
  .icon-btn:hover { color: var(--color-text-primary); background: var(--color-bg-hover); }
  .icon-btn.danger:hover { color: var(--color-error); background: color-mix(in srgb, var(--color-error) 10%, transparent); }

  .rich-row {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 9px; border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--color-border) 65%, transparent);
    background: var(--color-bg-surface);
  }
  .rich-row code { flex: 1; min-width: 0; font-size: 10.5px; color: var(--color-text-primary); overflow-wrap: anywhere; }
  .rule-preview {
    display: flex; flex-direction: column; gap: 3px;
    padding: 8px 10px; border-radius: 7px;
    border: 1px solid var(--color-border); background: var(--color-bg-surface);
  }
  .preview-label { font-size: 9.5px; font-weight: 700; letter-spacing: .05em; text-transform: uppercase; color: var(--color-text-muted); }
  .rule-preview code { font-size: 10.5px; color: var(--color-text-primary); overflow-wrap: anywhere; }

  .builder-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 9px; }
  .builder-footer { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .field-hint { margin: 0; font-size: 10.5px; color: var(--color-text-muted); line-height: 1.4; }

  .mini-empty {
    display: flex; align-items: center; gap: 8px;
    padding: 18px 10px; border: 1px dashed var(--color-border);
    border-radius: 8px; color: var(--color-text-muted); font-size: 11.5px;
  }
  .listener-hint {
    display: flex; align-items: center; gap: 6px;
    font-size: 10.5px; color: var(--color-text-muted); line-height: 1.4;
  }
  .listener-hint.listening { color: var(--color-success); }
  .listener-hint.warning { color: var(--color-warning); }

  .fw-callout {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 10px 12px; border-radius: 8px; font-size: 11.5px;
    border: 1px solid var(--color-border); background: var(--color-bg-surface);
    color: var(--color-text-secondary);
  }
  .fw-callout > div { flex: 1; }
  .fw-callout strong { display: block; font-size: 12px; color: var(--color-text-primary); margin-bottom: 2px; }
  .fw-callout p { margin: 0; line-height: 1.4; }
  .fw-callout.danger { border-color: color-mix(in srgb, var(--color-error) 32%, transparent); background: color-mix(in srgb, var(--color-error) 8%, transparent); }
  .fw-callout.info { border-color: color-mix(in srgb, var(--color-info) 28%, transparent); background: color-mix(in srgb, var(--color-info) 7%, transparent); }

  .ai-card { gap: 10px; }
  .ai-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; }
  .ai-head h3 { display: flex; align-items: center; gap: 6px; margin: 0; font-size: 13px; color: var(--color-text-primary); }
  .ai-head p { margin: 2px 0 0; font-size: 10.5px; color: var(--color-text-muted); }

  @media (max-width: 860px) {
    .fw-layout { grid-template-columns: 1fr; }
    .zone-panel { position: static; }
    .zone-list { flex-direction: row; flex-wrap: wrap; max-height: none; overflow: visible; }
    .zone-item { width: auto; }
    .builder-grid { grid-template-columns: 1fr 1fr; }
  }
  @media (max-width: 620px) {
    .firewall-page { padding: 16px; }
    .rules-grid, .builder-grid { grid-template-columns: 1fr; }
    .add-row, .bind-row { flex-direction: column; align-items: stretch; }
  }
</style>
