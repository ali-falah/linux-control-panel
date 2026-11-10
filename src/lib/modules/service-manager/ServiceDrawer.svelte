<script lang="ts">
  import { Activity, ChevronRight, FileText, RefreshCw, ShieldAlert, ShieldCheck } from '@lucide/svelte';
  import { invoke } from '@tauri-apps/api/core';
  import SideDrawer from '../../components/SideDrawer.svelte';
  import Button from '../../components/ui/Button.svelte';
  import TabGroup from '../../components/ui/TabGroup.svelte';
  import CodeEditor from '../../components/CodeEditor.svelte';
  import ConfigDiffModal from '../../components/ConfigDiffModal.svelte';
  import { uiStore } from '../../stores/ui.svelte.ts';
  import { statusStore } from '../../stores/status.svelte.ts';
  import { reduceRecoveryWarning } from '../../utils/serviceRecovery';
  import type { ServiceUnit, UnitDeps } from './types';

  let {
    open = $bindable(false),
    unit,
    panel,
    scope,
    recoveryWarning,
    onwarning,
    onretry,
  }: {
    open: boolean;
    unit: ServiceUnit | null;
    panel: 'logs' | 'editor' | 'dependencies' | null;
    scope: boolean;
    recoveryWarning: string | null;
    onwarning: (next: string | null) => void;
    onretry: () => void;
  } = $props();

  let logs = $state('');
  let logsLoading = $state(false);
  let unitFileContent = $state('');
  let unitFileLoading = $state(false);
  let editorTab = $state<'override' | 'effective'>('override');
  let effectiveUnitContent = $state('');
  let effectiveUnitLoading = $state(false);
  let effectiveUnitError = $state<string | null>(null);
  let editedContent = $state('');
  let saving = $state(false);
  let showUnitDiffModal = $state(false);
  let unitDeps = $state<UnitDeps | null>(null);
  let depsLoading = $state(false);
  let depsError = $state<string | null>(null);

  let requestId = 0;

  $effect(() => {
    const isOpen = open;
    const currentPanel = panel;
    const target = unit;
    const currentScope = scope;
    if (!isOpen || !currentPanel || !target) return;

    const id = ++requestId;
    if (currentPanel === 'logs') {
      void loadLogs(target.name, currentScope, id);
    } else if (currentPanel === 'editor') {
      void loadEditor(target.name, currentScope, id);
    } else {
      void loadDependencies(target.name, currentScope, id);
    }
  });

  async function loadLogs(name: string, currentScope: boolean, id: number) {
    logsLoading = true;
    logs = '';
    try {
      const content = await invoke<string>('get_service_logs', { name, lines: 100, userMode: currentScope });
      if (id !== requestId) return;
      logs = content;
      statusStore.setLastCommand(`journalctl ${currentScope ? '--user' : ''} -u ${name} -n 100`, 0, true);
    } catch (e) {
      if (id !== requestId) return;
      logs = `Failed to load logs: ${e}`;
      uiStore.addToast(`Failed to load logs: ${e}`, 'error');
      statusStore.setLastCommand(`journalctl ${currentScope ? '--user' : ''} -u ${name} -n 100`, 1, false);
    } finally {
      if (id === requestId) logsLoading = false;
    }
  }

  async function loadEditor(name: string, currentScope: boolean, id: number) {
    unitFileLoading = true;
    unitFileContent = '';
    editorTab = 'override';
    effectiveUnitContent = '';
    effectiveUnitError = null;
    effectiveUnitLoading = false;
    try {
      const content = await invoke<string>('read_unit_override', { name, userMode: currentScope });
      if (id !== requestId) return;
      unitFileContent = content;
      editedContent = content;
      statusStore.setLastCommand(`cat ${currentScope ? '~/.config/systemd/user' : '/etc/systemd/system'}/${name}.d/override.conf`, 0, true);
    } catch (e) {
      if (id !== requestId) return;
      unitFileContent = '';
      editedContent = '';
      uiStore.addToast(`Failed to read override for ${name}: ${e}`, 'error');
      statusStore.setLastCommand(`cat ${currentScope ? '~/.config/systemd/user' : '/etc/systemd/system'}/${name}.d/override.conf`, 1, false);
    } finally {
      if (id === requestId) unitFileLoading = false;
    }
  }

  async function loadDependencies(name: string, currentScope: boolean, id: number) {
    depsLoading = true;
    unitDeps = null;
    depsError = null;
    try {
      const deps = await invoke<UnitDeps>('get_unit_dependencies', { name, userMode: currentScope });
      if (id !== requestId) return;
      unitDeps = deps;
      statusStore.setLastCommand(`systemctl show ${name} --property=Requires,Wants,After,Before`, 0, true);
    } catch (e) {
      if (id !== requestId) return;
      depsError = `Failed to load dependencies: ${e}`;
      uiStore.addToast(depsError, 'error');
      statusStore.setLastCommand(`systemctl show ${name} --property=Requires,Wants,After,Before`, 1, false);
    } finally {
      if (id === requestId) depsLoading = false;
    }
  }

  async function loadEffectiveUnit() {
    if (!unit || effectiveUnitLoading) return;
    const name = unit.name;
    const currentScope = scope;
    effectiveUnitLoading = true;
    effectiveUnitError = null;
    try {
      const content = await invoke<string>('read_unit_file', { name, userMode: currentScope });
      if (unit?.name !== name) return;
      effectiveUnitContent = content;
      statusStore.setLastCommand(`systemctl ${currentScope ? '--user ' : ''}cat ${name}`, 0, true);
    } catch (e) {
      if (unit?.name !== name) return;
      effectiveUnitError = `Failed to read effective unit: ${e}`;
      statusStore.setLastCommand(`systemctl ${currentScope ? '--user ' : ''}cat ${name}`, 1, false);
    } finally {
      effectiveUnitLoading = false;
    }
  }

  function confirmSaveUnitFile() {
    showUnitDiffModal = true;
  }

  function overridePath(name: string, currentScope: boolean): string {
    return currentScope
      ? `~/.config/systemd/user/${name}.d/override.conf`
      : `/etc/systemd/system/${name}.d/override.conf`;
  }

  async function saveUnitFile() {
    if (!unit) return;
    saving = true;
    try {
      await invoke('write_unit_file', {
        name: unit.name,
        content: editedContent,
        userMode: scope,
      });
      const targetPath = overridePath(unit.name, scope);
      statusStore.setLastCommand(`echo "..." > ${targetPath} && systemctl ${scope ? '--user' : ''} daemon-reload`, 0, true);
      uiStore.addToast(`Unit override saved for ${unit.name}`, 'success');
      onwarning(reduceRecoveryWarning(recoveryWarning, { kind: 'recovery-succeeded' }));
      unitFileContent = editedContent;
      effectiveUnitContent = '';
    } catch (e) {
      onwarning(reduceRecoveryWarning(recoveryWarning, { kind: 'write-failed', error: e }));
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      statusStore.setLastCommand(`echo "..." > ${overridePath(unit.name, scope)}`, 1, false);
    } finally {
      saving = false;
    }
  }
</script>

    <SideDrawer
      bind:isOpen={open}
      title={panel === 'logs' ? `Logs — ${unit?.name}` : (panel === 'dependencies' ? `Dependencies — ${unit?.name}` : `Unit — ${unit?.name}`)}
      width="640px"
    >
      {#snippet headerActions()}
        {#if panel === 'logs' && unit}
          <Button variant="outline" class="btn-sm" onclick={() => uiStore.jumpToJournalService(unit!.name)}>
            <FileText size={13} /> Full Journal ↗
          </Button>
        {:else if panel === 'editor'}
          {#if editorTab === 'override'}
            <Button variant="outline" class="btn-sm" onclick={onretry}>
              Retry daemon-reload
            </Button>
            <Button variant="primary" class="btn-sm" onclick={confirmSaveUnitFile}
              disabled={saving || editedContent === unitFileContent}>
              {saving ? 'Saving…' : 'Save Override'}
            </Button>
          {:else}
            <span class="badge badge-muted">Read-only</span>
          {/if}
        {/if}
      {/snippet}

      {#if unit?.is_protected}
        <div class="drawer-protection-alert {unit.protection_level}">
          {#if unit.protection_level === 'critical'}
            <ShieldAlert size={18} class="protection-alert-icon critical" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-error); font-size: 12.5px;">Critical Operating System Core</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{unit.protection_reason || 'This unit is critical for operating system integrity. Masking, disabling, or stopping is strictly locked.'}</span>
            </div>
          {:else}
            <ShieldCheck size={18} class="protection-alert-icon essential" />
            <div class="protection-alert-text">
              <strong style="color: var(--color-accent); font-size: 12.5px;">Protected Infrastructure Service</strong>
              <span style="font-size: 11.5px; color: var(--color-text-secondary);">{unit.protection_reason || 'This unit provides essential security, network, or display services. Guarded against accidental masking or disablement.'}</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if panel === 'logs'}
        {#if logsLoading}
          <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
            <RefreshCw size={14} class="animate-spin-slow" /> Loading logs…
          </div>
        {:else}
          <pre class="log-output">{logs || 'No log output found.'}</pre>
        {/if}
      {:else if panel === 'dependencies'}
        {#if depsLoading}
          <div style="padding:24px;color:var(--color-text-muted);display:flex;align-items:center;justify-content:center;gap:8px">
            <RefreshCw size={18} class="animate-spin-slow" /> Querying systemd dependency tree…
          </div>
        {:else if depsError}
          <div class="empty-state" role="alert" style="padding:32px; color:var(--color-error);">{depsError}</div>
        {:else if unitDeps}
          <div style="display:flex; flex-direction:column; gap:16px; padding:4px 0;">
            <!-- Requires -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ShieldCheck size={14} style="color:var(--color-error);" /> Requires (Hard Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.requires.length}</span>
              </div>
              {#if unitDeps.requires.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.requires as req}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{req}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Wants -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <Activity size={14} style="color:var(--color-warning);" /> Wants (Weak Dependencies)
                </span>
                <span class="badge badge-muted">{unitDeps.wants.length}</span>
              </div>
              {#if unitDeps.wants.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.wants as want}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{want}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- After -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-accent);" /> Starts After (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.after.length}</span>
              </div>
              {#if unitDeps.after.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.after as aft}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{aft}</code>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Before -->
            <div class="card" style="padding:12px; background:var(--color-bg-base); border:1px solid var(--color-border);">
              <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:8px;">
                <span style="font-size:12.5px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
                  <ChevronRight size={14} style="color:var(--color-success);" /> Starts Before (Order)
                </span>
                <span class="badge badge-muted">{unitDeps.before.length}</span>
              </div>
              {#if unitDeps.before.length === 0}
                <span style="font-size:11.5px; color:var(--color-text-muted);">None declared</span>
              {:else}
                <div style="display:flex; flex-wrap:wrap; gap:6px;">
                  {#each unitDeps.before as bef}
                    <code style="font-size:11px; background:var(--color-bg-card); padding:2px 8px; border-radius:4px; border:1px solid var(--color-border); font-family:var(--font-mono);">{bef}</code>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {:else if panel === 'editor'}
        <div style="display:flex; flex-direction:column; height:100%; min-height:0; gap:10px;">
          <TabGroup
            tabs={[
              { id: 'override', label: 'Editable Override' },
              { id: 'effective', label: 'Effective Unit (read-only)' },
            ]}
            bind:activeTab={editorTab}
            size="sm"
            onchange={(tab) => {
              if (tab === 'effective' && effectiveUnitContent === '' && !effectiveUnitError) {
                loadEffectiveUnit();
              }
            }}
          />

          {#if editorTab === 'override'}
            {#if unitFileLoading}
              <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
                <RefreshCw size={14} class="animate-spin-slow" /> Loading override…
              </div>
            {:else}
              <div style="display:flex; flex-direction:column; flex:1; min-height:0;">
                <CodeEditor value={unitFileContent} height="100%" onchange={(v) => editedContent = v} />
              </div>
            {/if}
          {:else if effectiveUnitLoading}
            <div style="padding:16px;color:var(--color-text-muted);display:flex;align-items:center;gap:8px">
              <RefreshCw size={14} class="animate-spin-slow" /> Loading effective unit…
            </div>
          {:else if effectiveUnitError}
            <div class="empty-state" role="alert" style="padding:24px; color:var(--color-error); gap:10px;">
              <span>{effectiveUnitError}</span>
              <Button variant="outline" size="sm" onclick={loadEffectiveUnit}>Retry</Button>
            </div>
          {:else}
            <pre class="log-output" style="flex:1; min-height:0;">{effectiveUnitContent || 'No unit content returned.'}</pre>
          {/if}
        </div>
      {/if}
    </SideDrawer>
{#if unit}
  <ConfigDiffModal
    bind:show={showUnitDiffModal}
    filePath={overridePath(unit.name, scope)}
    title={`Review ${unit.name} Override Changes`}
    oldContent={unitFileContent}
    newContent={editedContent}
    warningMessage={unit.is_protected ? 'CAUTION: This is a protected system unit. Changes may affect operating system services.' : 'Ensure systemd directives and ExecStart paths are valid before saving.'}
    isSaving={saving}
    onconfirm={async () => {
      await saveUnitFile();
      showUnitDiffModal = false;
    }}
    oncancel={() => showUnitDiffModal = false}
  />
{/if}

<style>
.log-output {
    font-size: 11px;
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    margin: 0;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: rgba(0,0,0,0.2);
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-all;
  }.drawer-protection-alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 8px;
    margin-bottom: 16px;
  }.drawer-protection-alert.critical {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.25);
  }.drawer-protection-alert.essential {
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.25);
  }.protection-alert-icon.critical {
    color: var(--color-error);
    margin-top: 2px;
    flex-shrink: 0;
  }.protection-alert-icon.essential {
    color: var(--color-accent);
    margin-top: 2px;
    flex-shrink: 0;
  }.protection-alert-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    line-height: 1.4;
  }  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 12px;
  }
</style>
