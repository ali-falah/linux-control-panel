<script lang="ts">
  import { Check, ChevronRight, Copy, FileCode, Folder, Play, Plus, Rocket, Sparkles, Trash2 } from '@lucide/svelte';
  import Stepper from '../../components/ui/Stepper.svelte';
  import Select from '../../components/ui/Select.svelte';
  import Button from '../../components/ui/Button.svelte';
  import { portal } from '../../actions/portal.ts';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { uiStore } from '../../stores/ui.svelte.ts';
  import type { AutostartEntry } from './types';

  let {
    userScope,
    createOpen = $bindable(false),
    autostartEntry = $bindable<AutostartEntry | null | undefined>(undefined),
    oncreated,
    onautostartsaved,
  }: {
    userScope: boolean;
    createOpen: boolean;
    autostartEntry: AutostartEntry | null | undefined;
    oncreated: (name: string) => void | Promise<void>;
    onautostartsaved: () => void | Promise<void>;
  } = $props();

  // ─── New Unit Creation Wizard State ──────────────────────────────────────────
  let createUnitStep = $state(1);
  let unitForm = $state({
    name: '',
    description: '',
    execStart: '',
    workingDir: '',
    user: 'root',
    group: 'root',
    type: 'simple',
    restartPolicy: 'on-failure',
    restartSec: '5s',
    timeoutSec: '30s',
    afterTargets: ['network.target', 'network-online.target'],
    wantsTargets: ['network-online.target'],
    wantedBy: 'multi-user.target',
    envVars: [{ key: 'NODE_ENV', value: 'production' }]
  });
  let isSubmittingUnit = $state(false);
  let copiedUnitCode = $state(false);

  let unitFormTouched = $state({
    name: false,
    execStart: false,
    workingDir: false,
    user: false,
    restartSec: false,
    timeoutSec: false
  });

  let unitFormErrors = $derived.by(() => {
    const errors: Record<string, string> = {};

    // Unit Name
    const rawName = unitForm.name.trim();
    if (!rawName) {
      errors.name = 'Unit name is required.';
    } else if (/\s/.test(rawName)) {
      errors.name = 'Unit name cannot contain whitespace.';
    } else if (!/^[a-zA-Z0-9_\-@\.]+$/.test(rawName)) {
      errors.name = 'Invalid characters (only letters, numbers, -, _, @, . allowed).';
    }

    // ExecStart
    const exec = unitForm.execStart.trim();
    if (!exec) {
      errors.execStart = 'ExecStart command / binary path is required.';
    } else if (!exec.startsWith('/') && !exec.startsWith('@') && !exec.startsWith('-') && !exec.startsWith(':')) {
      errors.execStart = 'ExecStart should start with an absolute path (e.g. /usr/bin/node ...).';
    }

    // Working Directory
    const cwd = unitForm.workingDir.trim();
    if (cwd && !cwd.startsWith('/') && !cwd.startsWith('~')) {
      errors.workingDir = 'Working directory must be an absolute path (starts with /).';
    }

    // Execution User
    const usr = unitForm.user.trim();
    if (usr && !/^[a-z_][a-z0-9_-]*[$]?$/i.test(usr)) {
      errors.user = 'Invalid username format (alphanumeric and dashes only).';
    }

    // RestartSec
    const rsec = unitForm.restartSec.trim();
    if (rsec && !/^\d+(\.\d+)?(ms|s|min|h|d|w)?$/.test(rsec)) {
      errors.restartSec = 'Invalid time span (e.g. 5s, 500ms, 2min).';
    }

    // TimeoutSec
    const tsec = unitForm.timeoutSec.trim();
    if (tsec && !/^\d+(\.\d+)?(ms|s|min|h|d|w)?$/.test(tsec)) {
      errors.timeoutSec = 'Invalid time span (e.g. 30s, 1min).';
    }

    // Env vars
    for (const ev of unitForm.envVars) {
      const k = ev.key.trim();
      if (k && !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(k)) {
        errors.envVars = `Invalid environment variable name "${k}". Must be a valid identifier.`;
        break;
      }
    }

    return errors;
  });

  let isStep1Valid = $derived(!unitFormErrors.name && !unitFormErrors.execStart && !unitFormErrors.workingDir && !unitFormErrors.user);
  let isStep2Valid = $derived(!unitFormErrors.restartSec && !unitFormErrors.timeoutSec && !unitFormErrors.envVars);

  // Generate standard systemd .service unit content
  let generatedUnitContent = $derived.by(() => {
    const lines: string[] = [];
    lines.push('[Unit]');
    lines.push(`Description=${unitForm.description || unitForm.name || 'Custom Systemd Service'}`);
    if (unitForm.afterTargets.length > 0) {
      lines.push(`After=${unitForm.afterTargets.join(' ')}`);
    }
    if (unitForm.wantsTargets.length > 0) {
      lines.push(`Wants=${unitForm.wantsTargets.join(' ')}`);
    }
    lines.push('');
    lines.push('[Service]');
    lines.push(`Type=${unitForm.type}`);
    if (unitForm.user && !userScope) lines.push(`User=${unitForm.user}`);
    if (unitForm.group && !userScope) lines.push(`Group=${unitForm.group}`);
    if (unitForm.workingDir) lines.push(`WorkingDirectory=${unitForm.workingDir}`);
    lines.push(`ExecStart=${unitForm.execStart || '/usr/bin/executable'}`);
    lines.push(`Restart=${unitForm.restartPolicy}`);
    if (unitForm.restartPolicy !== 'no') {
      lines.push(`RestartSec=${unitForm.restartSec || '5s'}`);
    }
    if (unitForm.timeoutSec) {
      lines.push(`TimeoutSec=${unitForm.timeoutSec}`);
    }
    for (const ev of unitForm.envVars) {
      if (ev.key.trim()) {
        const escaped = ev.value.trim().replace(/\\/g, '\\\\').replace(/"/g, '\\"');
        lines.push(`Environment="${ev.key.trim()}=${escaped}"`);
      }
    }
    lines.push('');
    lines.push('[Install]');
    lines.push(`WantedBy=${unitForm.wantedBy || 'multi-user.target'}`);
    lines.push('');
    return lines.join('\n');
  });

  async function browseExecStartBinary() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Executable Binary / Script'
      });
      if (selected && typeof selected === 'string') {
        unitForm.execStart = selected;
        if (!unitForm.name.trim()) {
          const base = selected.split('/').pop()?.replace(/\.[^/.]+$/, '') || '';
          if (base) unitForm.name = base;
        }
        if (!unitForm.workingDir.trim()) {
          const dir = selected.substring(0, selected.lastIndexOf('/'));
          if (dir) unitForm.workingDir = dir;
        }
      }
    } catch (err) {
      console.warn('Dialog error:', err);
    }
  }

  async function browseUnitWorkingDir() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: 'Select Working Directory'
      });
      if (selected && typeof selected === 'string') {
        unitForm.workingDir = selected;
      }
    } catch (err) {
      console.warn('Dialog error:', err);
    }
  }

  function resetUnitForm() {
    unitForm = {
      name: '',
      description: '',
      execStart: '',
      workingDir: '',
      user: 'root',
      group: 'root',
      type: 'simple',
      restartPolicy: 'on-failure',
      restartSec: '5s',
      timeoutSec: '30s',
      afterTargets: ['network.target', 'network-online.target'],
      wantsTargets: ['network-online.target'],
      wantedBy: 'multi-user.target',
      envVars: [{ key: 'NODE_ENV', value: 'production' }],
    };
    unitFormTouched = {
      name: false,
      execStart: false,
      workingDir: false,
      user: false,
      restartSec: false,
      timeoutSec: false,
    };
    copiedUnitCode = false;
    createUnitStep = 1;
  }

  async function handleCreateUnit(startImmediately = false) {
    if (Object.keys(unitFormErrors).length > 0) {
      unitFormTouched = {
        name: true,
        execStart: true,
        workingDir: true,
        user: true,
        restartSec: true,
        timeoutSec: true,
      };
      uiStore.addToast(Object.values(unitFormErrors)[0] ?? 'Fix the highlighted fields first.', 'warning');
      createUnitStep = 1;
      return;
    }
    if (!unitForm.name.trim()) {
      uiStore.addToast('Please specify a unit name (e.g. my-app.service)', 'warning');
      createUnitStep = 1;
      return;
    }
    if (!unitForm.execStart.trim()) {
      uiStore.addToast('Please specify the ExecStart command', 'warning');
      createUnitStep = 1;
      return;
    }

    let fullName = unitForm.name.trim();
    if (!fullName.endsWith('.service')) {
      fullName += '.service';
    }

    try {
      const exists = await invoke<boolean>('unit_file_exists', { name: fullName, userMode: userScope });
      if (exists) {
        uiStore.addToast(
          `A unit file named "${fullName}" already exists. Edit its override instead.`,
          'error'
        );
        createUnitStep = 1;
        return;
      }
    } catch (e) {
      uiStore.addToast(`Could not check whether ${fullName} exists: ${e}`, 'error');
      return;
    }

    if (!userScope) {
      uiStore.confirm(
        'Create System Unit as Root',
        `This will write a root-owned systemd unit file for "${fullName}" and run daemon-reload.\n\nOnly continue if you trust this unit definition.\n\n${generatedUnitContent}`,
        () => doCreateUnit(fullName, startImmediately),
        true
      );
      return;
    }
    await doCreateUnit(fullName, startImmediately);
  }

  async function doCreateUnit(fullName: string, startImmediately: boolean) {
    isSubmittingUnit = true;
    try {
      await invoke('create_unit_file', {
        name: fullName,
        content: generatedUnitContent,
        userMode: userScope
      });

      uiStore.addToast(`Unit "${fullName}" created successfully!`, 'success');
      createOpen = false;

      if (startImmediately) {
        try {
          await invoke('unit_action', { name: fullName, action: 'start', userMode: userScope });
          uiStore.addToast(`Started "${fullName}"`, 'success');
        } catch (err) {
          uiStore.addToast(`Failed to start unit: ${err}`, 'warning');
        }
      }

      await oncreated(fullName);
    } catch (e: any) {
      uiStore.addToast(`Failed to create unit file: ${e}`, 'error');
    } finally {
      isSubmittingUnit = false;
    }
  }


  let wizardWasOpen = false;
  $effect(() => {
    if (createOpen && !wizardWasOpen) resetUnitForm();
    wizardWasOpen = createOpen;
  });

  let showAutostartModal = $state(false);
  let isEditingAutostart = $state(false);
  let autostartSaving = $state(false);
  let autostartOriginalFilePath = $state<string | null>(null);

  let autostartForm = $state({
    name: '',
    exec: '',
    comment: '',
    enabled: true
  });

  let autostartFormTouched = $state({
    name: false,
    exec: false
  });

  let autostartFormErrors = $derived.by(() => {
    const errors: Record<string, string> = {};
    const name = autostartForm.name.trim();
    if (!name) {
      errors.name = 'Application name is required.';
    } else if (name.includes('\n') || name.includes('\r')) {
      errors.name = 'Application name cannot contain newline characters.';
    }

    const exec = autostartForm.exec.trim();
    if (!exec) {
      errors.exec = 'Command / Executable path is required.';
    } else if (exec.includes('\n') || exec.includes('\r')) {
      errors.exec = 'Command cannot contain newline characters.';
    }

    if (autostartForm.comment.includes('\n') || autostartForm.comment.includes('\r')) {
      errors.comment = 'Comment cannot contain newline characters.';
    }

    return errors;
  });

  let isAutostartFormValid = $derived(!autostartFormErrors.name && !autostartFormErrors.exec && !autostartFormErrors.comment);

  let appliedAutostartEntry: AutostartEntry | null | undefined = undefined;

  $effect(() => {
    const request = autostartEntry;
    if (request === undefined) {
      appliedAutostartEntry = undefined;
      return;
    }
    if (appliedAutostartEntry === request) return;
    appliedAutostartEntry = request;

    if (request === null) {
      isEditingAutostart = false;
      autostartOriginalFilePath = null;
      autostartForm = { name: '', exec: '', comment: '', enabled: true };
      autostartFormTouched = { name: false, exec: false };
    } else {
      isEditingAutostart = true;
      autostartOriginalFilePath = request.file_path;
      autostartForm = {
        name: request.name,
        exec: request.exec,
        comment: request.comment || '',
        enabled: request.enabled,
      };
      autostartFormTouched = { name: false, exec: false };
    }
  });

  async function browseAutostartExecBinary() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: 'Select Executable Binary / Script'
      });
      if (selected && typeof selected === 'string') {
        autostartForm.exec = selected;
        autostartFormTouched.exec = true;
        if (!autostartForm.name.trim()) {
          const base = selected.split('/').pop()?.replace(/\.[^/.]+$/, '') || '';
          if (base) {
            autostartForm.name = base;
            autostartFormTouched.name = true;
          }
        }
      }
    } catch (err) {
      console.warn('Autostart browse dialog error:', err);
    }
  }

  async function saveAutostart() {
    autostartFormTouched.name = true;
    autostartFormTouched.exec = true;
    if (!isAutostartFormValid) return;

    autostartSaving = true;
    try {
      await invoke('save_autostart_entry', {
        originalFilePath: autostartOriginalFilePath,
        name: autostartForm.name.trim(),
        exec: autostartForm.exec.trim(),
        comment: autostartForm.comment.trim(),
        enabled: autostartForm.enabled
      });
      uiStore.addToast(
        isEditingAutostart
          ? `Autostart "${autostartForm.name.trim()}" updated successfully`
          : `Autostart "${autostartForm.name.trim()}" created successfully`,
        'success'
      );
      autostartEntry = undefined;
      await onautostartsaved();
    } catch (e) {
      uiStore.addToast(`Failed to save autostart entry: ${e}`, 'error');
    } finally {
      autostartSaving = false;
    }
  }

</script>

<!-- MODAL: ADD / EDIT AUTOSTART APPLICATION -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if autostartEntry !== undefined}
  <div use:portal class="modal-backdrop" onclick={() => { if (!autostartSaving) autostartEntry = undefined; }} role="presentation">
    <div class="modal-card" style="max-width: 520px;" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-modal="true">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <Rocket size={18} class="text-accent" />
          <h3>{isEditingAutostart ? 'Edit Autostart Application' : 'Add Autostart Application'}</h3>
        </div>
        <button class="modal-close-btn" onclick={() => { if (!autostartSaving) autostartEntry = undefined; }}>×</button>
      </div>

      <div class="modal-body">
        <!-- Application Name -->
        <div class="form-group">
          <label for="autostart-name-input" class="form-label">
            Application Name <span class="text-rose">*</span>
          </label>
          <input
            id="autostart-name-input"
            type="text"
            placeholder="e.g. Discord or Custom Script"
            bind:value={autostartForm.name}
            oninput={() => autostartFormTouched.name = true}
            class="form-input"
            class:input-error={autostartFormTouched.name && autostartFormErrors.name}
          />
          {#if autostartFormTouched.name && autostartFormErrors.name}
            <span class="form-error-msg">{autostartFormErrors.name}</span>
          {/if}
        </div>

        <!-- Command / Executable -->
        <div class="form-group">
          <label for="autostart-exec-input" class="form-label">
            Command / Executable <span class="text-rose">*</span>
          </label>
          <div class="input-browse-wrap">
            <input
              id="autostart-exec-input"
              type="text"
              placeholder="e.g. /usr/bin/discord or custom-daemon.sh"
              bind:value={autostartForm.exec}
              oninput={() => autostartFormTouched.exec = true}
              class="form-input font-mono"
              class:input-error={autostartFormTouched.exec && autostartFormErrors.exec}
            />
            <Button
              variant="outline"
              size="sm"
              onclick={browseAutostartExecBinary}
              title="Browse for executable binary or script"
              type="button"
            >
              <Folder size={13} />
              <span>Browse…</span>
            </Button>
          </div>
          {#if autostartFormTouched.exec && autostartFormErrors.exec}
            <span class="form-error-msg">{autostartFormErrors.exec}</span>
          {:else}
            <span class="form-help">Absolute path to executable binary, script, or command</span>
          {/if}
        </div>

        <!-- Comment / Description -->
        <div class="form-group">
          <label for="autostart-comment-input" class="form-label">
            Comment / Description <span class="text-muted" style="font-weight: normal;">(Optional)</span>
          </label>
          <input
            id="autostart-comment-input"
            type="text"
            placeholder="e.g. Automatically launch on session login"
            bind:value={autostartForm.comment}
            class="form-input"
            class:input-error={autostartFormErrors.comment}
          />
          {#if autostartFormErrors.comment}
            <span class="form-error-msg">{autostartFormErrors.comment}</span>
          {/if}
        </div>

        <!-- Enable Autostart Toggle -->
        <div style="display:flex; align-items:center; justify-content:space-between; padding:10px 12px; background:var(--color-bg-surface); border:1px solid var(--color-border-subtle); border-radius:8px; margin-top:4px;">
          <div style="display:flex; flex-direction:column; gap:2px;">
            <span style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Enable Autostart</span>
            <span style="font-size:11px; color:var(--color-text-muted);">Launch this application automatically when you log in</span>
          </div>
          <button
            type="button"
            class="ui-toggle"
            class:on={autostartForm.enabled}
            onclick={() => autostartForm.enabled = !autostartForm.enabled}
            aria-checked={autostartForm.enabled}
            role="switch"
            aria-label="Enable autostart"
          >
            <span class="ui-toggle-thumb"></span>
          </button>
        </div>
      </div>

      <div class="modal-footer">
        <Button
          variant="outline"
          onclick={() => autostartEntry = undefined}
          disabled={autostartSaving}
        >
          Cancel
        </Button>
        <Button
          variant="primary"
          onclick={saveAutostart}
          disabled={!isAutostartFormValid || autostartSaving}
          loading={autostartSaving}
        >
          <Check size={14} />
          <span>{isEditingAutostart ? 'Save Changes' : 'Add Application'}</span>
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- ═════════════════════════════════════════════════════════════════════════ -->
<!-- MODAL: NEW SYSTEMD UNIT FILE WIZARD -->
<!-- ═════════════════════════════════════════════════════════════════════════ -->
{#if createOpen}
  <div use:portal class="modal-backdrop" onclick={() => { if (!isSubmittingUnit) createOpen = false; }} role="presentation">
    <div class="modal-card modal-wizard-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-modal="true">
      <div class="modal-header">
        <div class="modal-title-wrap">
          <FileCode size={18} class="text-accent" />
          <h3>Create Systemd Service Unit</h3>
        </div>
        <button class="modal-close-btn" onclick={() => { if (!isSubmittingUnit) createOpen = false; }}>×</button>
      </div>

      <!-- Stepper Progress Bar -->
      <Stepper
        bind:currentStep={createUnitStep}
        allowClick={false}
        steps={[
          { label: 'Unit Identity' },
          { label: 'Lifecycle & Exec' },
          { label: 'Dependencies' },
          { label: 'Pre-flight Preview' }
        ]}
        onchange={(s) => {
          if (s > 1 && !unitForm.name.trim()) {
            uiStore.addToast('Please specify a unit name first', 'warning');
            createUnitStep = 1;
          }
        }}
      />

      <div class="modal-body wizard-body">
        {#if createUnitStep === 1}
          <!-- ─── STEP 1: UNIT IDENTITY & EXECUTABLE ─── -->
          <div class="step-pane">
            <div class="form-row">
              <div class="form-group">
                <label for="unit-name-input" class="form-label">Service Unit Name <span class="text-rose">*</span></label>
                <input
                  id="unit-name-input"
                  type="text"
                  placeholder="e.g. api-service or worker"
                  bind:value={unitForm.name}
                  oninput={() => unitFormTouched.name = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.name && unitFormErrors.name}
                />
                {#if unitFormTouched.name && unitFormErrors.name}
                  <small class="form-error-msg">{unitFormErrors.name}</small>
                {:else}
                  <small class="form-help">Will be saved as <code>{unitForm.name.trim() ? (unitForm.name.endsWith('.service') ? unitForm.name : unitForm.name + '.service') : 'name.service'}</code></small>
                {/if}
              </div>

              <div class="form-group">
                <label for="unit-desc-input" class="form-label">Description</label>
                <input
                  id="unit-desc-input"
                  type="text"
                  placeholder="e.g. My Production Backend Daemon"
                  bind:value={unitForm.description}
                  class="form-input"
                />
                <small class="form-help">Readable summary displayed in systemctl status</small>
              </div>
            </div>

            <div class="form-group">
              <label for="unit-exec-input" class="form-label">ExecStart Command <span class="text-rose">*</span></label>
              <div class="input-browse-wrap">
                <input
                  id="unit-exec-input"
                  type="text"
                  placeholder="/usr/bin/node /var/www/app/dist/main.js or /usr/bin/python3 main.py"
                  bind:value={unitForm.execStart}
                  oninput={() => unitFormTouched.execStart = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.execStart && unitFormErrors.execStart}
                />
                <Button variant="outline" onclick={browseExecStartBinary} title="Browse for executable file">
                  <Folder size={14} class="text-accent" />
                  <span>Browse...</span>
                </Button>
              </div>
              {#if unitFormTouched.execStart && unitFormErrors.execStart}
                <small class="form-error-msg">{unitFormErrors.execStart}</small>
              {:else}
                <small class="form-help">Absolute binary path with launch arguments</small>
              {/if}
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="unit-cwd-input" class="form-label">Working Directory (WorkingDirectory)</label>
                <div class="input-browse-wrap">
                  <input
                    id="unit-cwd-input"
                    type="text"
                    placeholder="/var/www/my-app"
                    bind:value={unitForm.workingDir}
                    oninput={() => unitFormTouched.workingDir = true}
                    class="form-input font-mono"
                    class:input-error={unitFormTouched.workingDir && unitFormErrors.workingDir}
                  />
                  <Button variant="outline" onclick={browseUnitWorkingDir} title="Browse working directory">
                    <Folder size={14} />
                  </Button>
                </div>
                {#if unitFormTouched.workingDir && unitFormErrors.workingDir}
                  <small class="form-error-msg">{unitFormErrors.workingDir}</small>
                {:else}
                  <small class="form-help">Working folder for the daemon process</small>
                {/if}
              </div>

              <div class="form-group">
                <label for="unit-user-input" class="form-label">Execution User & Group</label>
                <div style="display:grid; grid-template-columns:1fr 1fr; gap:8px;">
                  <input
                    id="unit-user-input"
                    type="text"
                    placeholder="User (e.g. root, node)"
                    bind:value={unitForm.user}
                    disabled={userScope}
                    oninput={() => unitFormTouched.user = true}
                    class="form-input"
                    class:input-error={unitFormTouched.user && unitFormErrors.user}
                  />
                  <input
                    type="text"
                    placeholder="Group (e.g. root, node)"
                    bind:value={unitForm.group}
                    disabled={userScope}
                    class="form-input"
                  />
                </div>
                {#if unitFormTouched.user && unitFormErrors.user}
                  <small class="form-error-msg">{unitFormErrors.user}</small>
                {:else}
                  <small class="form-help">{userScope ? 'User mode inherits current user' : 'Run daemon under non-root account for security'}</small>
                {/if}
              </div>
            </div>
          </div>

        {:else if createUnitStep === 2}
          <!-- ─── STEP 2: LIFECYCLE & EXECUTION ─── -->
          <div class="step-pane">
            <div class="form-row">
              <div class="form-group">
                <label for="unit-type-select" class="form-label">Service Type (Type=)</label>
                <Select id="unit-type-select" bind:value={unitForm.type}>
                  <option value="simple">simple — Standard daemon process (Default)</option>
                  <option value="forking">forking — Process calls fork() (e.g. traditional daemons)</option>
                  <option value="oneshot">oneshot — Short-lived batch script/task</option>
                  <option value="notify">notify — Daemon sends readiness signal with sd_notify()</option>
                </Select>
              </div>

              <div class="form-group">
                <label for="unit-restart-select" class="form-label">Restart Policy (Restart=)</label>
                <Select id="unit-restart-select" bind:value={unitForm.restartPolicy}>
                  <option value="on-failure">on-failure — Restart only on crash / non-zero exit (Recommended)</option>
                  <option value="always">always — Always restart unconditionally</option>
                  <option value="on-abort">on-abort — Restart on uncaught signal / abort</option>
                  <option value="no">no — Never restart automatically</option>
                </Select>
              </div>
            </div>

            <div class="form-row">
              <div class="form-group">
                <label for="unit-restart-sec" class="form-label">Restart Delay (RestartSec=)</label>
                <input
                  id="unit-restart-sec"
                  type="text"
                  placeholder="5s"
                  bind:value={unitForm.restartSec}
                  oninput={() => unitFormTouched.restartSec = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.restartSec && unitFormErrors.restartSec}
                />
                {#if unitFormTouched.restartSec && unitFormErrors.restartSec}
                  <small class="form-error-msg">{unitFormErrors.restartSec}</small>
                {/if}
              </div>
              <div class="form-group">
                <label for="unit-timeout-sec" class="form-label">Stop Timeout (TimeoutSec=)</label>
                <input
                  id="unit-timeout-sec"
                  type="text"
                  placeholder="30s"
                  bind:value={unitForm.timeoutSec}
                  oninput={() => unitFormTouched.timeoutSec = true}
                  class="form-input font-mono"
                  class:input-error={unitFormTouched.timeoutSec && unitFormErrors.timeoutSec}
                />
                {#if unitFormTouched.timeoutSec && unitFormErrors.timeoutSec}
                  <small class="form-error-msg">{unitFormErrors.timeoutSec}</small>
                {/if}
              </div>
            </div>

            <!-- Environment Variables list -->
            <div class="form-group">
              <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:4px;">
                <span class="form-label" style="margin:0;">Environment Variables</span>
                <button
                  type="button"
                  class="btn-text"
                  onclick={() => unitForm.envVars = [...unitForm.envVars, { key: '', value: '' }]}
                  style="font-size:11.5px; color:var(--color-accent); display:flex; align-items:center; gap:4px; background:transparent; border:none; cursor:pointer;"
                >
                  <Plus size={12} /> Add Variable
                </button>
              </div>
              {#if unitFormErrors.envVars}
                <small class="form-error-msg" style="margin-bottom:6px;">{unitFormErrors.envVars}</small>
              {/if}
              <div style="display:flex; flex-direction:column; gap:6px; max-height:130px; overflow-y:auto; padding-right:2px;">
                {#each unitForm.envVars as ev, idx}
                  <div style="display:grid; grid-template-columns:1fr 1fr 28px; gap:8px; align-items:center;">
                    <input type="text" placeholder="KEY (e.g. PORT)" bind:value={ev.key} class="form-input font-mono" />
                    <input type="text" placeholder="VALUE (e.g. 3000)" bind:value={ev.value} class="form-input font-mono" />
                    <button
                      type="button"
                      onclick={() => unitForm.envVars = unitForm.envVars.filter((_, i) => i !== idx)}
                      style="background:transparent; border:none; color:var(--color-error); cursor:pointer; display:flex; align-items:center; justify-content:center;"
                      title="Remove variable"
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                {/each}
              </div>
            </div>
          </div>

        {:else if createUnitStep === 3}
          <!-- ─── STEP 3: DEPENDENCIES & TARGETS ─── -->
          <div class="step-pane">
            <div class="form-group">
              <span class="form-label">Startup Ordering (After=)</span>
              <div class="preset-pill-row">
                {#each ['network.target', 'network-online.target', 'docker.service', 'nginx.service', 'postgresql.service', 'mysqld.service', 'redis.service'] as target}
                  <button
                    type="button"
                    class="preset-pill"
                    class:active-pill={unitForm.afterTargets.includes(target)}
                    onclick={() => {
                      if (unitForm.afterTargets.includes(target)) {
                        unitForm.afterTargets = unitForm.afterTargets.filter(t => t !== target);
                      } else {
                        unitForm.afterTargets = [...unitForm.afterTargets, target];
                      }
                    }}
                  >
                    {target}
                  </button>
                {/each}
              </div>
              <small class="form-help">Selected services must finish starting before this unit begins execution</small>
            </div>

            <div class="form-group">
              <span class="form-label">Soft Dependencies (Wants=)</span>
              <div class="preset-pill-row">
                {#each ['network-online.target', 'docker.service', 'postgresql.service', 'redis.service'] as target}
                  <button
                    type="button"
                    class="preset-pill"
                    class:active-pill={unitForm.wantsTargets.includes(target)}
                    onclick={() => {
                      if (unitForm.wantsTargets.includes(target)) {
                        unitForm.wantsTargets = unitForm.wantsTargets.filter(t => t !== target);
                      } else {
                        unitForm.wantsTargets = [...unitForm.wantsTargets, target];
                      }
                    }}
                  >
                    {target}
                  </button>
                {/each}
              </div>
              <small class="form-help">Services started alongside this unit without hard failure dependencies</small>
            </div>

            <div class="form-group">
              <label for="wanted-by-select" class="form-label">Install Target (WantedBy=)</label>
              <Select id="wanted-by-select" bind:value={unitForm.wantedBy}>
                <option value="multi-user.target">multi-user.target — Normal boot / non-graphical runlevel (Standard)</option>
                <option value="graphical.target">graphical.target — Full graphical desktop environment</option>
                <option value="default.target">default.target — Current system default target</option>
              </Select>
            </div>
          </div>

        {:else if createUnitStep === 4}
          <!-- ─── STEP 4: PRE-FLIGHT SYNTAX PREVIEW ─── -->
          <div class="step-pane">
            <div style="display:flex; align-items:center; justify-content:space-between;">
              <span style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Generated Systemd Unit File Syntax</span>
              <button
                type="button"
                class="btn-copy-syntax"
                onclick={() => {
                  navigator.clipboard.writeText(generatedUnitContent);
                  copiedUnitCode = true;
                  setTimeout(() => copiedUnitCode = false, 1800);
                }}
              >
                {#if copiedUnitCode}
                  <Check size={12} style="color:var(--color-success);" />
                  <span>Copied!</span>
                {:else}
                  <Copy size={12} />
                  <span>Copy Syntax</span>
                {/if}
              </button>
            </div>

            <div class="terminal-preview-card font-mono">
              <pre style="margin:0; font-size:11.5px; line-height:1.5; color:#f1f5f9; white-space:pre-wrap; word-break:break-all;">{generatedUnitContent}</pre>
            </div>

            <div class="step-info-card">
              <Sparkles size={16} class="text-accent flex-shrink-0" />
              <div class="text-xs text-secondary">
                This unit file will be written to <code>{userScope ? '~/.config/systemd/user/' : '/etc/systemd/system/'}{unitForm.name.endsWith('.service') ? unitForm.name : unitForm.name + '.service'}</code> and systemd will automatically reload daemon.
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Modal Footer -->
      <div class="modal-footer">
        {#if createUnitStep > 1}
          <Button variant="outline" onclick={() => createUnitStep--} disabled={isSubmittingUnit}>Back</Button>
        {:else}
          <Button variant="outline" onclick={() => createOpen = false} disabled={isSubmittingUnit}>Cancel</Button>
        {/if}

        {#if createUnitStep < 4}
          <Button variant="primary" onclick={() => {
            if (createUnitStep === 1) {
              unitFormTouched.name = true;
              unitFormTouched.execStart = true;
              unitFormTouched.workingDir = true;
              unitFormTouched.user = true;
              if (!isStep1Valid) {
                uiStore.addToast(unitFormErrors.name || unitFormErrors.execStart || unitFormErrors.workingDir || 'Please fix step errors', 'warning');
                return;
              }
            } else if (createUnitStep === 2) {
              unitFormTouched.restartSec = true;
              unitFormTouched.timeoutSec = true;
              if (!isStep2Valid) {
                uiStore.addToast(unitFormErrors.restartSec || unitFormErrors.timeoutSec || unitFormErrors.envVars || 'Please fix step errors', 'warning');
                return;
              }
            }
            createUnitStep++;
          }}>
            <span>Next</span>
            <ChevronRight size={13} />
          </Button>
        {:else}
          <Button variant="outline" onclick={() => handleCreateUnit(false)} disabled={isSubmittingUnit}>
            <FileCode size={13} />
            <span>Create Unit File</span>
          </Button>
          <Button variant="primary" onclick={() => handleCreateUnit(true)} disabled={isSubmittingUnit}>
            <Play size={13} />
            <span>Create &amp; Start</span>
          </Button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
/* ── Service Creation Wizard Modal Styles ── */ .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 99990;
    padding: 20px;
  }.modal-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
    width: 100%;
    max-width: 580px;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
    position: relative;
  }.modal-wizard-card {
    width: 920px;
    max-width: calc(100vw - 40px);
    height: 620px;
    max-height: 92vh;
  }.modal-header {
    padding: 14px 20px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--color-border-subtle);
    border-top-left-radius: 13px;
    border-top-right-radius: 13px;
  }.modal-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }.modal-title-wrap h3 {
    font-size: 15px;
    font-weight: 600;
    margin: 0;
  }.modal-close-btn {
    background: transparent;
    border: none;
    font-size: 20px;
    color: var(--color-text-muted);
    cursor: pointer;
  }.modal-body {
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }.wizard-body {
    flex: 1;
    min-height: 0;
    padding: 18px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }.step-pane {
    display: flex;
    flex-direction: column;
    gap: 13px;
    flex: 1;
  }.step-info-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    border-radius: 8px;
    margin-top: auto;
  }.form-group {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }.form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }.form-label {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }.form-input {
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12.5px;
    color: var(--color-text-primary);
    font-family: inherit;
    outline: none;
  }.form-input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-accent-muted);
  }.form-input.input-error {
    border-color: var(--color-error, #f43f5e) !important;
    box-shadow: 0 0 0 2px rgba(244, 63, 94, 0.18) !important;
  }.form-error-msg {
    font-size: 11px;
    font-weight: 500;
    color: var(--color-error, #f43f5e);
    margin-top: 2px;
  }.form-help {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }.input-browse-wrap {
    display: flex;
    gap: 8px;
    align-items: center;
  }.input-browse-wrap .form-input {
    flex: 1;
    min-width: 0;
  }.preset-pill-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }.preset-pill {
    padding: 3px 9px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 500;
    background: var(--color-bg-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-subtle);
    cursor: pointer;
    transition: all 0.12s ease;
  }.preset-pill:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }.preset-pill.active-pill {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    color: var(--color-accent, #00daf3);
    border-color: var(--color-accent, #00daf3);
    font-weight: 600;
  }.terminal-preview-card {
    background: #08111e;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 14px;
    max-height: 250px;
    overflow-y: auto;
  }.btn-copy-syntax {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text-secondary);
    cursor: pointer;
  }.btn-copy-syntax:hover {
    color: var(--color-text-primary);
  }.modal-footer {
    padding: 12px 20px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-surface);
    border-bottom-left-radius: 13px;
    border-bottom-right-radius: 13px;
  }:global(html.light-mode) .modal-wizard-card {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.15);
  }:global(html.light-mode) .modal-header, :global(html.light-mode) .modal-footer {
    background: #f8fafc;
    border-color: #e2e8f0;
  }:global(html.light-mode) .terminal-preview-card {
    background: #f8fafc;
    border-color: #cbd5e1;
  }:global(html.light-mode) .terminal-preview-card pre {
    color: #0f172a !important;
  }
  .text-rose { color: var(--color-error); }
  .text-secondary { color: var(--color-text-secondary); }
</style>
