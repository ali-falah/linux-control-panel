<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { ShieldAlert, Shield, RefreshCw, Power } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';

  interface SelinuxStatus {
    status: string;
    current_mode: string;
    config_mode: string;
  }

  interface Denial {
    raw: string;
    timestamp: string;
    scontext: string;
    tcontext: string;
    tclass: string;
  }

  let status = $state<SelinuxStatus | null>(null);
  let denials = $state<Denial[]>([]);
  let loading = $state(true);

  // Tab control
  let activeTab = $state<'status' | 'booleans'>(
    uiStore.targetSubTab && ['status', 'booleans'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'status'
  );
  if (uiStore.targetSubTab && ['status', 'booleans'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  // Booleans list
  let booleans = $state<any[]>([]);
  let searchQuery = $state('');
  let loadingBools = $state(false);
  let permanentChange = $state(true);

  // Smart AVC Troubleshooter
  let selectedDenial = $state<any | null>(null);
  let showTroubleshooter = $state(false);
  let audit2allowExplanation = $state('');
  let loadingExplanation = $state(false);
  let customModuleName = $state('my_allow_policy');
  let applyingOverride = $state(false);

  // Pagination for booleans
  let currentPage = $state(1);
  const itemsPerPage = 15;

  $effect(() => {
    searchQuery;
    currentPage = 1;
  });

  let filteredBooleans = $derived.by(() => {
    if (!searchQuery) return booleans;
    const lower = searchQuery.toLowerCase();
    return booleans.filter(b => b.name.toLowerCase().includes(lower));
  });

  let totalPages = $derived(Math.ceil(filteredBooleans.length / itemsPerPage) || 1);

  let paginatedBooleans = $derived.by(() => {
    const safePage = Math.max(1, Math.min(currentPage, totalPages));
    const start = (safePage - 1) * itemsPerPage;
    return filteredBooleans.slice(start, start + itemsPerPage);
  });
  
  async function loadBooleans() {
    loadingBools = true;
    try {
      booleans = await invoke<any[]>('selinux_get_booleans');
    } catch(e) {
      console.error(e);
    } finally {
      loadingBools = false;
    }
  }
  
  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading SELinux status…');
    try {
      status = await invoke<SelinuxStatus>('get_selinux_status');
      if (status.status !== 'disabled') {
        denials = await invoke<Denial[]>('get_selinux_denials');
        await loadBooleans();
      }
      statusStore.setLastCommand('sestatus / ausearch', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load SELinux data: ${e}`, 'error');
      statusStore.setLastCommand('sestatus', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function toggleBoolean(bool: any) {
    const nextState = !bool.value;
    const actionStr = nextState ? 'ON (Enable)' : 'OFF (Disable)';
    const scopeStr = permanentChange ? 'Persistent (saved permanently with -P)' : 'Runtime only (will reset after reboot)';

    if (bool.is_critical) {
      uiStore.confirm(
        '⚠️ Critical SELinux Boolean Warning',
        `You are changing critical boolean '${bool.name}' to ${actionStr}.\n\nImpact:\n${bool.risk_description || 'This boolean controls core system daemon privileges or user authentication.'}\n\nPersistence: ${scopeStr}\n\nDo you want to proceed?`,
        () => executeToggleBoolean(bool.name, bool.value),
        true
      );
    } else {
      uiStore.confirm(
        'Confirm SELinux Boolean Change',
        `Are you sure you want to change '${bool.name}' to ${actionStr}?\n\nPersistence: ${scopeStr}`,
        () => executeToggleBoolean(bool.name, bool.value),
        !nextState
      );
    }
  }

  async function executeToggleBoolean(name: string, currentValue: boolean) {
    const newValue = !currentValue;
    statusStore.setBusy(`Updating boolean ${name}…`);
    try {
      const msg = await invoke<string>('selinux_set_boolean', {
        name,
        value: newValue,
        permanent: permanentChange
      });
      uiStore.addToast(msg, 'success');
      await loadBooleans();
    } catch (e) {
      uiStore.addToast(`Failed to set boolean: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function explainDenial(denial: any) {
    selectedDenial = denial;
    showTroubleshooter = true;
    loadingExplanation = true;
    audit2allowExplanation = '';
    
    const parts = denial.tcontext.split(':');
    const targetType = parts.length > 2 ? parts[2] : 'custom';
    customModuleName = `allow_${targetType}_${denial.tclass}`.replace(/[^a-zA-Z0-9_]/g, '_');
    
    try {
      audit2allowExplanation = await invoke<string>('selinux_explain_denial', {
        rawLog: denial.raw
      });
    } catch (e) {
      console.error(e);
      audit2allowExplanation = `Failed to analyze denial: ${e}`;
    } finally {
      loadingExplanation = false;
    }
  }

  let isModuleNameValid = $derived.by(() => {
    return /^[a-zA-Z][a-zA-Z0-9_]{1,63}$/.test(customModuleName.trim());
  });

  async function applyPolicyOverride() {
    if (!selectedDenial || !isModuleNameValid) {
      uiStore.addToast('Please provide a valid policy module name (alphanumeric and underscores, starting with a letter).', 'warning');
      return;
    }
    applyingOverride = true;
    statusStore.setBusy(`Compiling and installing SELinux module '${customModuleName}'…`);
    try {
      const msg = await invoke<string>('selinux_apply_policy_override', {
        name: customModuleName.trim(),
        rawLog: selectedDenial.raw
      });
      uiStore.addToast(msg, 'success');
      showTroubleshooter = false;
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to install policy override: ${e}`, 'error');
    } finally {
      applyingOverride = false;
      statusStore.clearBusy();
    }
  }

  function confirmChangeMode(newMode: 'enforcing' | 'permissive') {
    uiStore.confirm(
      'Change SELinux Mode',
      `Switch SELinux runtime mode to ${newMode.toUpperCase()}?\n\n` +
      (newMode === 'permissive' 
        ? 'WARNING: In Permissive mode, SELinux will log security violations (AVCs) but will NOT block unauthorized actions. Use this only for temporary debugging.'
        : 'In Enforcing mode, SELinux will actively protect the system and block all unauthorized access attempts.'),
      () => setRuntimeMode(newMode),
      newMode === 'permissive'
    );
  }

  async function setRuntimeMode(newMode: 'enforcing' | 'permissive') {
    statusStore.setBusy(`Setting SELinux mode to ${newMode}…`);
    try {
      const msg = await invoke<string>('set_selinux_mode', { mode: newMode });
      uiStore.addToast(msg, 'success');
      statusStore.setLastCommand(`setenforce ${newMode === 'enforcing' ? '1' : '0'}`, 0, true);
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to change SELinux mode: ${e}`, 'error');
      statusStore.setLastCommand(`setenforce ${newMode === 'enforcing' ? '1' : '0'}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page" style="overflow-y: auto; padding-bottom: 40px;">
  <PageHeader title="SELinux Manager" icon={ShieldAlert}>
    {#if status && status.status !== 'disabled'}
      <TabGroup
        tabs={[
          { id: 'status', label: 'Status & Denials' },
          { id: 'booleans', label: booleans.length > 0 ? `SELinux Booleans (${booleans.length})` : 'SELinux Booleans' }
        ]}
        bind:activeTab={activeTab}
      />
    {/if}
    <Button variant="ghost" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
  </PageHeader>

  {#if loading && !status}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading SELinux State…</span>
    </div>
  {:else if status}
    {#if activeTab === 'status'}
      <!-- Compact Current Status Card -->
      <div class="card" style="padding: 10px 16px; margin-bottom: 12px;">
        {#if status.status === 'disabled'}
          <div style="padding:12px; border-radius:8px; background:rgba(255, 71, 87, 0.1); color:var(--color-danger); display:flex; align-items:center; gap:12px">
            <Power size={20} />
            <div>
              <div style="font-weight:600; font-size:13px;">SELinux is Disabled</div>
              <div style="font-size:12px;">Your system does not have SELinux active. Reboot may be required to enable it.</div>
            </div>
          </div>
        {:else}
          <div style="display:flex; gap:16px; align-items:center; justify-content:space-between; flex-wrap:wrap;">
            <div style="display:flex; gap:20px; align-items:center;">
              <div>
                <div style="font-size:10.5px; color:var(--color-text-muted); font-weight:700; text-transform:uppercase; letter-spacing:0.04em;">Runtime Mode</div>
                <div style="font-size:14px; font-weight:700; color:var(--color-text-primary); text-transform:capitalize; display:flex; align-items:center; gap:6px; margin-top:2px;">
                  {status.current_mode}
                  {#if status.current_mode === 'enforcing'}
                    <Shield size={14} style="color:var(--color-success);" />
                  {:else if status.current_mode === 'permissive'}
                    <ShieldAlert size={14} style="color:var(--color-warning);" />
                  {/if}
                </div>
              </div>

              <div style="width:1px; height:24px; background:var(--color-border);"></div>

              <div>
                <div style="font-size:10.5px; color:var(--color-text-muted); font-weight:700; text-transform:uppercase; letter-spacing:0.04em;">Config Mode (Boot)</div>
                <div style="font-size:13px; font-weight:600; color:var(--color-text-secondary); text-transform:capitalize; margin-top:2px;">
                  {status.config_mode}
                </div>
              </div>
            </div>

            <div style="display:flex; gap:6px;">
              <Button class="btn btn-sm {status.current_mode === 'enforcing' ? 'btn-primary' : '-outline'}" disabled={status.current_mode === 'enforcing'} onclick={() => confirmChangeMode('enforcing')}>
                Enforcing
              </Button>
              <Button class="btn btn-sm {status.current_mode === 'permissive' ? 'btn-warning' : '-outline'}" disabled={status.current_mode === 'permissive'} onclick={() => confirmChangeMode('permissive')}>
                Permissive
              </Button>
            </div>
          </div>
        {/if}
      </div>

      <!-- AVC Denials Section -->
      <div class="card" style="padding:0; overflow:hidden; display:flex; flex-direction:column;">
        <div style="padding:10px 16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center; background:var(--color-bg-base);">
          <h3 style="margin:0; font-size:13px; font-weight:700; color:var(--color-text-primary);">Recent Access Denials (AVC)</h3>
          <span class="badge {denials.length > 0 ? 'badge-error' : 'badge-success'}">{denials.length} events</span>
        </div>
        
        <div style="padding:14px; display:flex; flex-direction:column; gap:10px;">
          {#if denials.length === 0}
            <EmptyState
              icon={Shield}
              title="No Recent Denials"
              description="SELinux has not blocked any actions recently. All subsystem operations are passing security verification."
            />
          {:else}
            <div style="font-size:12px; color:var(--color-text-secondary); margin-bottom:4px;">
              Click on any denial event below to analyze and troubleshoot it using <code>audit2allow</code>.
            </div>
            {#each denials as denial}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div 
                class="card" 
                style="cursor:pointer; transition:all 0.15s ease; border:1px solid var(--color-border); background:var(--color-bg-raised); padding:10px 12px;"
                onclick={() => explainDenial(denial)}
                onmouseenter={(e) => e.currentTarget.style.borderColor='var(--color-danger)'}
                onmouseleave={(e) => e.currentTarget.style.borderColor='var(--color-border)'}
              >
                <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:6px;">
                  <span class="badge badge-error" style="font-size:10px;">AVC DENIED</span>
                  <span style="font-size:11.5px; font-family:var(--font-mono); color:var(--color-text-muted);">{denial.timestamp}</span>
                </div>
                <div style="font-size:12px; font-family:var(--font-mono); color:var(--color-text-primary); margin-bottom:3px; word-break:break-all;">
                  <strong style="color:var(--color-text-secondary);">Source:</strong> {denial.scontext}
                </div>
                <div style="font-size:12px; font-family:var(--font-mono); color:var(--color-text-primary); margin-bottom:3px; word-break:break-all;">
                  <strong style="color:var(--color-text-secondary);">Target:</strong> {denial.tcontext}
                </div>
                <div style="font-size:12px; font-family:var(--font-mono); color:var(--color-text-primary);">
                  <strong style="color:var(--color-text-secondary);">Class:</strong> <code style="color:var(--color-accent);">{denial.tclass}</code>
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>

    {:else if activeTab === 'booleans'}
      <!-- SELinux Booleans Tab -->
      <div class="card" style="padding:0; overflow:hidden; display:flex; flex-direction:column;">
        <div style="padding:10px 16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:10px; background:var(--color-bg-base);">
          <SearchBar 
            bind:value={searchQuery} 
            placeholder="Search booleans (e.g. 'httpd', 'samba', 'ftp')..." 
            count={filteredBooleans.length} 
            total={booleans.length} 
            style="max-width:340px; flex:1;" 
          />
          <div style="display:flex; align-items:center; gap:8px;">
            <span style="font-size:12px; color:var(--color-text-secondary);">Save persistently (-P)</span>
            <Toggle checked={permanentChange} onToggle={(checked) => permanentChange = checked} />
          </div>
        </div>

        {#if loadingBools}
          <div style="padding:40px; text-align:center; color:var(--color-text-muted);">
            <RefreshCw size={20} class="animate-spin-slow" /> Loading booleans...
          </div>
        {:else if filteredBooleans.length === 0}
          <EmptyState
            icon={Shield}
            title="No Booleans Found"
            description={searchQuery ? `No SELinux booleans matched "${searchQuery}".` : 'No booleans returned by getsebool.'}
            actionLabel={searchQuery ? 'Clear Search' : undefined}
            onAction={searchQuery ? () => searchQuery = '' : undefined}
          />
        {:else}
          <div class="table-wrap" style="border:none; border-radius:0; box-shadow:none; overflow-x:auto;">
            <table>
              <thead>
                <tr>
                  <th>Boolean Name</th>
                  <th style="width:100px; text-align:center;">State</th>
                  <th style="width:130px; text-align:center;">Action</th>
                </tr>
              </thead>
              <tbody>
                {#each paginatedBooleans as bool (bool.name)}
                  <tr>
                    <td style="font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); font-weight:500;">
                      <div style="display:flex; align-items:center; gap:8px; flex-wrap:wrap;">
                        <span>{bool.name}</span>
                        {#if bool.is_critical}
                          <span class="badge badge-warning" style="font-size:9.5px; padding:1px 6px; display:inline-flex; align-items:center; gap:3px;" title={bool.risk_description || 'Critical system boolean'}>
                            <ShieldAlert size={10} /> CRITICAL
                          </span>
                        {/if}
                      </div>
                      {#if bool.risk_description}
                        <div style="font-size:11px; font-family:var(--font-sans, system-ui); color:var(--color-text-muted); margin-top:2px; font-weight:400;">
                          {bool.risk_description}
                        </div>
                      {/if}
                    </td>
                    <td style="text-align:center;">
                      <span class="badge {bool.value ? 'badge-success' : 'badge-muted'}">
                        {bool.value ? 'ON' : 'OFF'}
                      </span>
                    </td>
                    <td style="text-align:center;">
                      <Button variant="outline" style="padding:2px 10px; height:26px; font-size:11.5px;" onclick={() => toggleBoolean(bool)}>
                        Toggle {bool.value ? 'OFF' : 'ON'}
                      </Button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>

          {#if totalPages > 1}
            <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:10px 16px; border-top:1px solid var(--color-border); background:var(--color-bg-base);">
              <Button variant="outline" style="padding:2px 10px; height:26px; font-size:11.5px;" disabled={currentPage === 1} onclick={() => currentPage--}>Previous</Button>
              <span style="font-size:12px; color:var(--color-text-secondary); font-weight:500;">Page {currentPage} of {totalPages} ({filteredBooleans.length} items)</span>
              <Button variant="outline" style="padding:2px 10px; height:26px; font-size:11.5px;" disabled={currentPage === totalPages} onclick={() => currentPage++}>Next</Button>
            </div>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}

  <SideDrawer bind:isOpen={showTroubleshooter} title="AVC Denial Troubleshooter" width="540px" dockable={true}>
    {#if selectedDenial}
      <div style="display:flex; flex-direction:column; gap:16px; padding:8px 0;">
        <div>
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">Raw Audit Event Log</h4>
          <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:6px; padding:10px; max-height:100px; overflow:auto;">
            <pre style="margin:0; font-family:var(--font-mono); font-size:11px; color:var(--color-text-muted); white-space:pre-wrap;">{selectedDenial.raw}</pre>
          </div>
        </div>

        <div>
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">smart audit2allow Explanation</h4>
          {#if loadingExplanation}
            <div style="color:var(--color-text-muted); font-size:13px; display:flex; align-items:center; gap:6px;">
              <RefreshCw size={14} class="animate-spin-slow" /> Analyzing AVC logs...
            </div>
          {:else}
            <div style="background:rgba(0,218,243,0.02); border:1px solid var(--color-border); border-radius:6px; padding:12px; display:flex; flex-direction:column; gap:8px;">
              <pre style="margin:0; font-family:var(--font-mono); font-size:12px; color:var(--color-text-primary); white-space:pre-wrap;">{audit2allowExplanation || 'No suggestions could be generated.'}</pre>
              
              <!-- Check for suggested boolean -->
              {#if audit2allowExplanation.match(/boolean '([^']+)'/)}
                {@const suggestedBool = audit2allowExplanation.match(/boolean '([^']+)'/)[1]}
                <div style="margin-top:10px; padding:10px; border-radius:6px; background:rgba(0,218,243,0.05); border:1px solid rgba(0,218,243,0.2); display:flex; flex-direction:column; gap:6px;">
                  <span style="font-size:12px; color:var(--color-text-primary); font-weight:600;">Suggested Solution</span>
                  <span style="font-size:12px; color:var(--color-text-secondary); line-height:1.4;">
                    Enable the <code>{suggestedBool}</code> boolean to allow this activity.
                  </span>
                  <div style="display:flex; justify-content:flex-end;">
                    <Button variant="primary" style="padding: 4px 10px; font-size:12px;" onclick={() => { toggleBoolean({ name: suggestedBool, value: false, is_critical: false }); showTroubleshooter = false; }}>
                      Enable Boolean
                    </Button>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <div style="border-top:1px solid var(--color-border); padding-top:16px;">
          <h4 style="margin:0 0 6px; color:var(--color-text-primary); font-size:13px;">Compile Policy Override</h4>
          <p style="font-size:12px; color:var(--color-text-secondary); margin-bottom:12px; line-height:1.4;">
            If no boolean matches, you can compile a custom policy module override containing the rules shown above. This compiles a secure type enforcement module and loads it using <code>semodule</code>.
          </p>

          <div style="display:flex; flex-direction:column; gap:12px;">
            <div style="display:flex; flex-direction:column; gap:4px;">
              <label for="policy-mod-name" style="font-size:11.5px; color:var(--color-text-muted);">Policy Module Name</label>
              <input id="policy-mod-name" type="text" class="input" bind:value={customModuleName} placeholder="e.g. allow_http_read" />
              {#if !isModuleNameValid && customModuleName}
                <span style="font-size:11px; color:var(--color-danger);">Must start with a letter and contain only alphanumeric characters and underscores (2-64 chars).</span>
              {/if}
            </div>
            <div style="display:flex; justify-content:flex-end;">
              <Button variant="primary" disabled={applyingOverride || loadingExplanation || !isModuleNameValid} onclick={applyPolicyOverride}>
                {applyingOverride ? 'Installing Module...' : 'Install Policy Override'}
              </Button>
            </div>
          </div>
        </div>
      </div>
    {/if}
  </SideDrawer>
</div>
