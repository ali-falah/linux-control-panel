<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { TerminalSquare, Save, RefreshCw, AlertTriangle } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import GuideBanner from '../components/ui/GuideBanner.svelte';

  interface GrubConfig {
    timeout: number;
    hidden_timeout: boolean;
    cmdline_linux: string;
    default_entry: string;
    raw_content: string;
  }

  let config = $state<GrubConfig | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let rebuilding = $state(false);

  let editedTimeout = $state(5);
  let editedHidden = $state(false);
  let editedCmdline = $state('');
  let editedDefault = $state('saved');

  async function loadConfig() {
    loading = true;
    statusStore.setBusy('Reading /etc/default/grub…');
    try {
      config = await invoke<GrubConfig>('read_grub_config');
      editedTimeout = config.timeout;
      editedHidden = config.hidden_timeout;
      editedCmdline = config.cmdline_linux;
      editedDefault = config.default_entry;
      statusStore.setLastCommand('cat /etc/default/grub', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load config: ${e}`, 'error');
      statusStore.setLastCommand('cat /etc/default/grub', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function getHasChanges() {
    if (!config) return false;
    return editedTimeout !== config.timeout ||
           editedHidden !== config.hidden_timeout ||
           editedCmdline !== config.cmdline_linux ||
           editedDefault !== config.default_entry;
  }

  let hasChanges = $derived(getHasChanges());

  function confirmSave() {
    uiStore.confirm(
      'Save GRUB Configuration',
      'Are you sure you want to save these changes to /etc/default/grub?\n\nWARNING: Invalid kernel parameters can prevent your system from booting.',
      () => doSave(),
      true
    );
  }

  async function doSave() {
    if (!config) return;
    saving = true;
    statusStore.setBusy('Saving /etc/default/grub…');
    try {
      let newConfig = {
        ...config,
        timeout: editedTimeout,
        hidden_timeout: editedHidden,
        cmdline_linux: editedCmdline,
        default_entry: editedDefault,
      };
      await invoke('write_grub_config', { config: newConfig });
      statusStore.setLastCommand('echo "..." > /etc/default/grub', 0, true);
      uiStore.addToast('Saved /etc/default/grub successfully', 'success');
      await loadConfig();
    } catch (e) {
      uiStore.addToast(`Failed to save config: ${e}`, 'error');
      statusStore.setLastCommand('echo "..." > /etc/default/grub', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  function confirmRebuild() {
    uiStore.confirm(
      'Rebuild GRUB',
      'This will run grub2-mkconfig to apply your changes to the bootloader.\n\nAre you sure you want to proceed?',
      () => doRebuild(),
      true
    );
  }

  async function doRebuild() {
    rebuilding = true;
    statusStore.setBusy('Running grub2-mkconfig…');
    try {
      await invoke('rebuild_grub');
      statusStore.setLastCommand('grub2-mkconfig -o /boot/grub2/grub.cfg', 0, true);
      uiStore.addToast('GRUB configuration rebuilt successfully', 'success');
    } catch (e) {
      uiStore.addToast(`Failed to rebuild GRUB: ${e}`, 'error');
      statusStore.setLastCommand('grub2-mkconfig -o /boot/grub2/grub.cfg', 1, false);
    } finally {
      rebuilding = false;
      statusStore.clearBusy();
    }
  }

  $effect(() => { loadConfig(); });
</script>

<div class="module-page">
  <PageHeader title="GRUB Configurator" subtitle="Safely manage bootloader settings and kernel parameters" icon={TerminalSquare}>
    <Button variant="ghost" class="" onclick={loadConfig} disabled={loading || saving || rebuilding}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    <Button class="btn -warning" onclick={confirmRebuild} disabled={loading || saving || rebuilding || hasChanges}>
      <RefreshCw size={14} class={rebuilding ? 'animate-spin-slow' : ''} /> Rebuild GRUB
    </Button>
    <Button variant="primary" class="" onclick={confirmSave} disabled={!hasChanges || saving || rebuilding}>
      <Save size={14} /> Save Changes
    </Button>
  </PageHeader>

  {#if loading && !config}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading GRUB Config…</span>
    </div>
  {:else if config}
    <!-- Top KPI Row -->
    <div class="grub-kpi-grid">
      <KpiCard
        icon={TerminalSquare}
        value={config.default_entry || 'saved'}
        label="Default Boot Entry"
        subtext="GRUB_DEFAULT target"
        statusText="Default"
        statusType="info"
      />
      <KpiCard
        icon={RefreshCw}
        value={`${config.timeout}s`}
        label="Menu Timeout"
        subtext="Wait delay before boot"
        statusText={config.timeout > 0 ? 'Active' : 'Instant'}
        statusType={config.timeout > 0 ? 'success' : 'warning'}
        iconBg="rgba(16, 185, 129, 0.12)"
        iconColor="var(--color-success)"
      />
      <KpiCard
        icon={TerminalSquare}
        value={config.hidden_timeout ? 'Hidden' : 'Visible'}
        label="Menu Style"
        subtext="GRUB_TIMEOUT_STYLE"
        iconBg="rgba(0, 218, 243, 0.12)"
        iconColor="var(--color-accent)"
      />
      <KpiCard
        icon={AlertTriangle}
        value={config.cmdline_linux ? config.cmdline_linux.split(' ').length : 0}
        label="Kernel Arguments"
        subtext="Active boot params"
        iconBg="rgba(245, 158, 11, 0.12)"
        iconColor="var(--color-warning)"
      />
    </div>

    <GuideBanner
      icon={TerminalSquare}
      title="GRUB Bootloader Configuration"
      description="Modify boot timeouts, default kernel targets, and Linux cmdline arguments. Changes are written safely as root via base64 decoding to /etc/default/grub."
      variant="info"
    />

    {#if hasChanges}
      <div style="margin-bottom:16px; padding:12px 16px; border-radius:8px; background:rgba(255, 171, 0, 0.1); border:1px solid rgba(255, 171, 0, 0.3); display:flex; align-items:center; gap:12px">
        <AlertTriangle size={20} style="color:var(--color-warning)" />
        <span style="font-size:13px; color:var(--color-text-primary)">
          You have unsaved changes. Remember to <strong>Save Changes</strong> and then <strong>Rebuild GRUB</strong> for them to take effect on the next boot.
        </span>
      </div>
    {/if}

    <div class="module-content-scroll">
      <div style="display:flex; flex-direction:column; gap:16px; padding-bottom: 24px;">
      
      <div class="card" style="display:flex; flex-direction:column; gap:16px">
        <div>
          <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Boot Menu Settings</h3>
          
          <div style="display:flex; gap:24px">
            <div style="flex:1">
              <label for="grub-timeout" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Timeout (seconds)</label>
              <input id="grub-timeout" type="number" class="input" bind:value={editedTimeout} min="-1" />
              <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px">Time to wait before booting default entry (-1 means wait indefinitely).</div>
            </div>
            
            <div style="flex:1; display:flex; flex-direction:column; justify-content:center">
              <label for="grub-hidden-toggle" style="display:flex; align-items:center; gap:8px; font-size:14px; color:var(--color-text-primary); cursor:pointer">
                <button
                  id="grub-hidden-toggle"
                  class="ui-toggle"
                  class:on={editedHidden}
                  onclick={() => editedHidden = !editedHidden}
                  type="button"
                  role="switch"
                  aria-checked={editedHidden}
                  aria-label="Toggle hidden timeout style"
                >
                  <span class="ui-toggle-thumb"></span>
                </button>
                Hide Boot Menu (Timeout Style = hidden)
              </label>
              <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px; padding-left:22px">
                Check this to hide the GRUB menu entirely during boot. You can press Esc to show it.
              </div>
            </div>
          </div>
        </div>

        <hr style="border:0; border-top:1px solid var(--color-border)" />

        <div>
          <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Kernel Parameters (GRUB_CMDLINE_LINUX)</h3>
          <input class="input" style="font-family:var(--font-mono); font-size:12px" bind:value={editedCmdline} />
          <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px">
            These parameters are passed to the Linux kernel at boot time (e.g. <code>quiet</code>, <code>rhgb</code>, <code>nomodeset</code>).
          </div>
        </div>

        <div>
          <h3 style="margin-top:0; color:var(--color-text-primary); font-size:16px; margin-bottom:12px">Default Boot Entry (GRUB_DEFAULT)</h3>
          <input class="input" style="font-family:var(--font-mono); font-size:12px" bind:value={editedDefault} />
          <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px">
            Usually <code>saved</code> to remember the last booted OS, or an index like <code>0</code>.
          </div>
        </div>
      </div>

      <div class="card" style="flex:1; display:flex; flex-direction:column; padding:0; overflow:hidden">
        <div style="padding:16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center">
          <h3 style="margin:0; color:var(--color-text-primary); font-size:14px">Raw /etc/default/grub</h3>
        </div>
        <div style="height:300px">
          <CodeEditor value={config.raw_content} readonly={true} />
        </div>
      </div>

      </div>
    </div>
  {/if}
</div>

<style>
  .grub-kpi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 12px;
    margin-bottom: 16px;
  }
</style>
