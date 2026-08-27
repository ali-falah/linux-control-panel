<script lang="ts">
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { TerminalSquare, Save, RefreshCw, AlertTriangle, ShieldCheck, ShieldAlert, History, Plus } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';

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
  let hasBackup = $state(false);
  let restoringBackup = $state(false);

  let editedTimeout = $state(5);
  let editedHidden = $state(false);
  let editedCmdline = $state('');
  let editedDefault = $state('saved');

  // Common safe kernel parameters for quick insertion
  const SUGGESTED_PARAMS = [
    { name: 'quiet', desc: 'Suppress verbose kernel boot messages' },
    { name: 'rhgb', desc: 'Red Hat Graphical Boot screen' },
    { name: 'nomodeset', desc: 'Disable kernel modesetting (GPU fallback)' },
    { name: 'audit=1', desc: 'Enable Linux Security Audit daemon' },
    { name: 'mitigations=auto', desc: 'Auto-apply CPU vulnerability mitigations' },
  ];

  async function loadConfig() {
    loading = true;
    statusStore.setBusy('Reading /etc/default/grub…');
    try {
      config = await invoke<GrubConfig>('read_grub_config');
      editedTimeout = config.timeout;
      editedHidden = config.hidden_timeout;
      editedCmdline = config.cmdline_linux;
      editedDefault = config.default_entry;
      hasBackup = await invoke<boolean>('grub_has_backup').catch(() => false);
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

  // Real-time client-side validation
  let validation = $derived.by(() => {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Timeout checks
    if (editedTimeout < -1 || editedTimeout > 300) {
      errors.push(`Timeout (${editedTimeout}s) must be between -1 and 300 seconds.`);
    }
    if (editedTimeout === 0 && editedHidden) {
      warnings.push("Both Timeout = 0s and Hidden Menu are enabled. In a boot failure or kernel panic, accessing the GRUB recovery menu will be very difficult.");
    }

    // Shell injection & syntax checks
    if (editedCmdline.includes('\n') || editedCmdline.includes('\r')) {
      errors.push("Kernel parameters must not contain line breaks.");
    }
    if (editedCmdline.includes(';') || editedCmdline.includes('&') || editedCmdline.includes('|') || editedCmdline.includes('`') || editedCmdline.includes('$(')) {
      errors.push("Shell metacharacters (;, &, |, `, $()) are forbidden in kernel parameters to prevent boot script corruption.");
    }

    // Unbalanced quotes
    const singleQuotes = (editedCmdline.match(/'/g) || []).length;
    const doubleQuotes = (editedCmdline.match(/"/g) || []).length;
    if (singleQuotes % 2 !== 0 || doubleQuotes % 2 !== 0) {
      errors.push("Unbalanced quotes detected in kernel parameters. Unclosed quotes will corrupt /etc/default/grub.");
    }

    // Destructive / Risky parameters
    const tokens = editedCmdline.trim().split(/\s+/);
    for (const t of tokens) {
      const lower = t.toLowerCase();
      if (lower === 'init=/bin/false' || lower === 'init=/dev/null') {
        errors.push(`Fatal parameter '${t}' will prevent system boot.`);
      } else if (lower.startsWith('init=/bin/sh') || lower.startsWith('init=/bin/bash')) {
        warnings.push(`Parameter '${t}' bypasses systemd and boots directly into an unauthenticated root shell.`);
      } else if (lower === 'emergency' || lower === 'rd.break' || lower === 'single') {
        warnings.push(`Parameter '${t}' forces single-user or emergency recovery mode.`);
      } else if (lower === 'mem=0' || lower === 'maxcpus=0') {
        errors.push(`Fatal parameter '${t}' will starve kernel resources and panic.`);
      } else if (lower === 'selinux=0' || lower === 'enforcing=0') {
        warnings.push(`Parameter '${t}' disables SELinux kernel security enforcement.`);
      }
    }

    // Default entry check
    if (editedDefault.includes('\n') || editedDefault.includes('"') || editedDefault.includes(';')) {
      errors.push("Default boot entry contains invalid characters or newlines.");
    }

    return {
      isValid: errors.length === 0,
      errors,
      warnings,
    };
  });

  function addSuggestedParam(param: string) {
    const currentTokens = editedCmdline.trim().split(/\s+/).filter(Boolean);
    if (!currentTokens.includes(param)) {
      currentTokens.push(param);
      editedCmdline = currentTokens.join(' ');
    }
  }

  function confirmSave() {
    if (!validation.isValid) {
      uiStore.addToast("Cannot save: fix configuration errors first.", "error");
      return;
    }

    const warningText = validation.warnings.length > 0
      ? `\n\n⚠️ CAUTION:\n• ${validation.warnings.join('\n• ')}`
      : '';

    uiStore.confirm(
      'Save GRUB Configuration',
      `Are you sure you want to write changes to /etc/default/grub?\n\nA safety backup will be saved automatically to /etc/default/grub.bak.${warningText}`,
      () => doSave(),
      validation.warnings.length > 0
    );
  }

  async function doSave() {
    if (!config || !validation.isValid) return;
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
      const msg = await invoke<string>('write_grub_config', { config: newConfig });
      statusStore.setLastCommand('write_grub_config', 0, true);
      uiStore.addToast(msg, 'success');
      await loadConfig();
    } catch (e) {
      uiStore.addToast(`Failed to save config: ${e}`, 'error');
      statusStore.setLastCommand('write_grub_config', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  function confirmRestoreBackup() {
    uiStore.confirm(
      'Restore Previous GRUB Backup',
      'This will restore /etc/default/grub from /etc/default/grub.bak and discard any unapplied changes.\n\nDo you want to proceed?',
      () => doRestoreBackup(),
      false
    );
  }

  async function doRestoreBackup() {
    restoringBackup = true;
    statusStore.setBusy('Restoring /etc/default/grub from backup…');
    try {
      const msg = await invoke<string>('grub_restore_backup');
      uiStore.addToast(msg, 'success');
      await loadConfig();
    } catch (e) {
      uiStore.addToast(`Failed to restore backup: ${e}`, 'error');
    } finally {
      restoringBackup = false;
      statusStore.clearBusy();
    }
  }

  function confirmRebuild() {
    if (hasChanges) {
      uiStore.confirm(
        'Unsaved Changes Detected',
        'You have unsaved changes in the editor. Rebuilding now will only apply the previously saved configuration on disk.\n\nDo you want to rebuild GRUB now?',
        () => doRebuild(),
        true
      );
      return;
    }

    uiStore.confirm(
      'Rebuild Bootloader (grub2-mkconfig)',
      'This will run grub2-mkconfig to regenerate /boot/grub2/grub.cfg with the current configuration.\n\nAre you sure you want to proceed?',
      () => doRebuild(),
      false
    );
  }

  async function doRebuild() {
    rebuilding = true;
    statusStore.setBusy('Running grub2-mkconfig…');
    try {
      const msg = await invoke<string>('rebuild_grub');
      statusStore.setLastCommand('grub2-mkconfig -o /boot/grub2/grub.cfg', 0, true);
      uiStore.addToast(msg, 'success');
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

<div class="module-page" style="overflow-y: auto; padding-bottom: 40px;">
  <PageHeader title="GRUB Configurator" subtitle="Safely manage bootloader settings, kernel parameters, and system recovery" icon={TerminalSquare}>
    {#if hasBackup}
      <Button variant="outline" onclick={confirmRestoreBackup} disabled={loading || saving || rebuilding || restoringBackup}>
        <History size={14} /> Restore Backup
      </Button>
    {/if}
    <Button variant="ghost" onclick={loadConfig} disabled={loading || saving || rebuilding}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    <Button class="btn -warning" onclick={confirmRebuild} disabled={loading || saving || rebuilding || !validation.isValid}>
      <RefreshCw size={14} class={rebuilding ? 'animate-spin-slow' : ''} /> Rebuild GRUB
    </Button>
    <Button variant="primary" onclick={confirmSave} disabled={!hasChanges || saving || rebuilding || !validation.isValid}>
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
        icon={validation.isValid ? ShieldCheck : ShieldAlert}
        value={validation.isValid ? 'Valid' : 'Errors'}
        label="Config Safety"
        subtext={validation.errors.length > 0 ? `${validation.errors.length} fatal errors` : 'Parameters verified'}
        statusText={validation.isValid ? 'Protected' : 'Invalid'}
        statusType={validation.isValid ? 'success' : 'danger'}
        iconBg={validation.isValid ? 'rgba(16, 185, 129, 0.12)' : 'rgba(239, 68, 68, 0.12)'}
        iconColor={validation.isValid ? 'var(--color-success)' : 'var(--color-danger)'}
      />
    </div>

    <!-- Fatal Validation Error Banner -->
    {#if validation.errors.length > 0}
      <div style="margin-bottom:14px; padding:12px 16px; border-radius:8px; background:rgba(239, 68, 68, 0.1); border:1px solid rgba(239, 68, 68, 0.3); display:flex; flex-direction:column; gap:6px;">
        <div style="display:flex; align-items:center; gap:8px; color:var(--color-danger); font-weight:600; font-size:13px;">
          <ShieldAlert size={18} /> Configuration Validation Failed (Saving Blocked)
        </div>
        <ul style="margin:0; padding-left:24px; font-size:12px; color:var(--color-text-primary); line-height:1.5;">
          {#each validation.errors as err}
            <li>{err}</li>
          {/each}
        </ul>
      </div>
    {/if}

    <!-- High-Risk Warning Banner -->
    {#if validation.warnings.length > 0}
      <div style="margin-bottom:14px; padding:12px 16px; border-radius:8px; background:rgba(245, 158, 11, 0.1); border:1px solid rgba(245, 158, 11, 0.3); display:flex; flex-direction:column; gap:6px;">
        <div style="display:flex; align-items:center; gap:8px; color:var(--color-warning); font-weight:600; font-size:13px;">
          <AlertTriangle size={18} /> High-Risk Bootloader Parameter Warnings
        </div>
        <ul style="margin:0; padding-left:24px; font-size:12px; color:var(--color-text-primary); line-height:1.5;">
          {#each validation.warnings as warn}
            <li>{warn}</li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if hasChanges}
      <div style="margin-bottom:14px; padding:10px 16px; border-radius:8px; background:rgba(0, 218, 243, 0.08); border:1px solid rgba(0, 218, 243, 0.25); display:flex; align-items:center; gap:12px">
        <AlertTriangle size={18} style="color:var(--color-accent)" />
        <span style="font-size:12.5px; color:var(--color-text-primary)">
          You have unsaved changes. Click <strong>Save Changes</strong> to safely write to <code>/etc/default/grub</code>, then <strong>Rebuild GRUB</strong> to apply to the bootloader.
        </span>
      </div>
    {/if}

    <div class="module-content-scroll">
      <div style="display:flex; flex-direction:column; gap:16px; padding-bottom: 24px;">
      
      <div class="card" style="display:flex; flex-direction:column; gap:16px">
        <div>
          <h3 style="margin-top:0; color:var(--color-text-primary); font-size:15px; margin-bottom:12px; font-weight:700;">Boot Menu Settings</h3>
          
          <div style="display:flex; gap:24px; flex-wrap:wrap;">
            <div style="flex:1; min-width:220px;">
              <label for="grub-timeout" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary); font-weight:600;">Timeout (seconds)</label>
              <input id="grub-timeout" type="number" class="input" bind:value={editedTimeout} min="-1" max="300" />
              <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px">Delay before booting default entry (-1 means wait indefinitely).</div>
            </div>
            
            <div style="flex:1; min-width:220px; display:flex; flex-direction:column; justify-content:center">
              <label for="grub-hidden-toggle" style="display:flex; align-items:center; gap:8px; font-size:13.5px; color:var(--color-text-primary); cursor:pointer; font-weight:600;">
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
                Hides GRUB menu during boot. Press <kbd style="padding:1px 4px; border:1px solid var(--color-border); border-radius:3px; background:var(--color-bg-base);">Esc</kbd> to reveal.
              </div>
            </div>
          </div>
        </div>

        <hr style="border:0; border-top:1px solid var(--color-border)" />

        <div>
          <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:6px;">
            <h3 style="margin:0; color:var(--color-text-primary); font-size:15px; font-weight:700;">Kernel Parameters (GRUB_CMDLINE_LINUX)</h3>
            <span style="font-size:11px; color:var(--color-text-muted);">Passed directly to Linux kernel at boot</span>
          </div>
          <input class="input" style="font-family:var(--font-mono); font-size:12.5px" bind:value={editedCmdline} />
          
          <!-- Suggested Safe Parameters -->
          <div style="margin-top:8px; display:flex; align-items:center; gap:6px; flex-wrap:wrap;">
            <span style="font-size:11px; color:var(--color-text-muted); font-weight:600;">Quick Add:</span>
            {#each SUGGESTED_PARAMS as sp}
              {@const isPresent = editedCmdline.includes(sp.name)}
              <button
                type="button"
                class="badge {isPresent ? 'badge-muted' : 'badge-info'}"
                style="cursor:{isPresent ? 'default' : 'pointer'}; border:1px solid var(--color-border); font-size:11px; padding:2px 8px; display:inline-flex; align-items:center; gap:3px;"
                disabled={isPresent}
                onclick={() => addSuggestedParam(sp.name)}
                title="{sp.desc}{isPresent ? ' (Already present)' : ''}"
              >
                {#if !isPresent}<Plus size={10} />{/if} {sp.name}
              </button>
            {/each}
          </div>
        </div>

        <div>
          <h3 style="margin-top:0; color:var(--color-text-primary); font-size:15px; margin-bottom:6px; font-weight:700;">Default Boot Entry (GRUB_DEFAULT)</h3>
          <input class="input" style="font-family:var(--font-mono); font-size:12.5px" bind:value={editedDefault} />
          <div style="font-size:11px; color:var(--color-text-muted); margin-top:4px">
            Usually <code>saved</code> to boot the last chosen kernel, or a specific kernel index like <code>0</code>.
          </div>
        </div>
      </div>

      <div class="card" style="flex:1; display:flex; flex-direction:column; padding:0; overflow:hidden">
        <div style="padding:12px 16px; border-bottom:1px solid var(--color-border); display:flex; justify-content:space-between; align-items:center; background:var(--color-bg-base);">
          <h3 style="margin:0; color:var(--color-text-primary); font-size:13px; font-weight:700;">Active /etc/default/grub File</h3>
          {#if hasBackup}
            <span class="badge badge-success" style="font-size:10px;">Backup Present (/etc/default/grub.bak)</span>
          {/if}
        </div>
        <div style="height:260px">
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

