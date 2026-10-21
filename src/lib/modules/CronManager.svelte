<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Clock, Plus, Trash2, RefreshCw, ShieldAlert, Shield, FolderOpen,
    Copy, Check, Timer, CalendarClock, Info
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';

  interface CronJob {
    raw: string;
    schedule: string;
    command: string;
    is_root: boolean;
  }

  interface SystemdTimer {
    unit: string;
    activates: string;
    status: string;
    description: string;
  }

  type View = 'cron' | 'timers';

  let view = $state<View>(
    uiStore.targetSubTab && ['cron', 'timers'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'cron'
  );
  if (uiStore.targetSubTab && ['cron', 'timers'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }
  let jobs = $state<CronJob[]>([]);
  let timers = $state<SystemdTimer[]>([]);
  let loading = $state(true);

  let showAdd = $state(false);
  let isRootJob = $state(false);
  let newSchedule = $state('* * * * *');
  let newCommand = $state('');
  let filterQuery = $state('');
  let copiedKey = $state<string | null>(null);

  // ── Schedule helpers ──────────────────────────────────────────────────
  const DAY_NAMES = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

  const NAMED_SCHEDULES: Record<string, string> = {
    '@reboot': 'On every system boot',
    '@yearly': 'Once a year, on 1 January',
    '@annually': 'Once a year, on 1 January',
    '@monthly': 'Once a month, on day 1',
    '@weekly': 'Once a week, on Sunday',
    '@daily': 'Once a day, at midnight',
    '@midnight': 'Once a day, at midnight',
    '@hourly': 'Once an hour, on the hour'
  };

  function isValidSchedule(expr: string): boolean {
    const s = expr.trim().toLowerCase();
    if (!s) return false;
    if (s.startsWith('@')) return s in NAMED_SCHEDULES;
    const parts = s.split(/\s+/);
    if (parts.length !== 5) return false;
    return parts.every(p => /^[\d*/,-]+$/.test(p));
  }

  /** Turns a raw cron expression into something a human can read at a glance. */
  function humanizeSchedule(expr: string): string {
    const s = expr.trim().toLowerCase();
    if (NAMED_SCHEDULES[s]) return NAMED_SCHEDULES[s];

    const parts = s.split(/\s+/);
    if (parts.length !== 5) return 'Custom schedule';

    const [min, hour, dom, mon, dow] = parts;
    const hhmm = `${hour.padStart(2, '0')}:${min.padStart(2, '0')}`;

    if (min === '*' && hour === '*') return 'Every minute';
    if (/^\*\/\d+$/.test(min) && hour === '*') return `Every ${parseInt(min.slice(2), 10)} minutes`;
    if (/^\d+\/\d+$/.test(min) && hour === '*') return `Every ${parseInt(min.split('/')[1], 10)} minutes`;
    if (min === '0' && hour === '*') return 'Every hour, on the hour';
    if (/^\*\/\d+$/.test(hour) && min === '0') return `Every ${parseInt(hour.slice(2), 10)} hours`;
    if (dom === '*' && mon === '*' && dow === '*' && /^\d+$/.test(hour) && /^\d+$/.test(min)) {
      return `Every day at ${hhmm}`;
    }
    if (dom === '*' && mon === '*' && /^\d+$/.test(dow) && hour === '0' && min === '0') {
      return `Every ${DAY_NAMES[Number(dow) % 7]}`;
    }
    if (/^\d+$/.test(dom) && dom !== '*' && hour === '0' && min === '0') {
      return `On day ${dom} of each month`;
    }
    if (/^\d+$/.test(dom) && dom !== '*' && /^\d+$/.test(mon) && mon !== '*') {
      return `On day ${dom} of ${MONTH_NAMES[Number(mon) % 12]}`;
    }
    if (/^\d+$/.test(hour) && /^\d+$/.test(min)) return `At ${hhmm}`;
    return 'Custom schedule';
  }

  // ── Derived data ──────────────────────────────────────────────────────
  const filteredJobs = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    if (!q) return jobs;
    return jobs.filter(
      j =>
        j.command.toLowerCase().includes(q) ||
        j.schedule.toLowerCase().includes(q) ||
        humanizeSchedule(j.schedule).toLowerCase().includes(q) ||
        (j.is_root ? 'root system' : 'user').includes(q)
    );
  });

  const filteredTimers = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    if (!q) return timers;
    return timers.filter(
      t =>
        t.unit.toLowerCase().includes(q) ||
        t.activates.toLowerCase().includes(q) ||
        t.status.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q)
    );
  });

  const scheduleValid = $derived(isValidSchedule(newSchedule));
  const canSave = $derived(newCommand.trim().length > 0 && scheduleValid);

  // ── Data ──────────────────────────────────────────────────────────────
  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading scheduled tasks…');
    try {
      const [cj, st] = await Promise.all([
        invoke<CronJob[]>('list_cron_jobs'),
        invoke<SystemdTimer[]>('cron_list_timers')
      ]);
      jobs = cj;
      timers = st;
      statusStore.setLastCommand('crontab -l; systemctl list-timers', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load tasks: ${e}`, 'error');
      statusStore.setLastCommand('crontab -l; systemctl list-timers', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function addJob() {
    if (!canSave) return;
    statusStore.setBusy('Adding cron job…');
    try {
      await invoke('add_cron_job', { schedule: newSchedule, command: newCommand, isRoot: isRootJob });
      statusStore.setLastCommand(`(crontab -l; echo "${newSchedule} ${newCommand}") | crontab -`, 0, true);
      uiStore.addToast('Cron job added', 'success');
      showAdd = false;
      newCommand = '';
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to add cron job: ${e}`, 'error');
      statusStore.setLastCommand(`(crontab -l; echo "${newSchedule} ${newCommand}") | crontab -`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function confirmDelete(job: CronJob) {
    uiStore.confirm(
      'Delete Cron Job',
      `Are you sure you want to delete this scheduled task?\n\n${job.raw}`,
      () => doDelete(job),
      true
    );
  }

  async function doDelete(job: CronJob) {
    statusStore.setBusy('Deleting cron job…');
    try {
      await invoke('delete_cron_job', { raw: job.raw, isRoot: job.is_root });
      statusStore.setLastCommand(`crontab -l | grep -v "${job.raw}" | crontab -`, 0, true);
      uiStore.addToast('Cron job deleted', 'success');
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to delete cron job: ${e}`, 'error');
      statusStore.setLastCommand(`crontab -l | grep -v "${job.raw}" | crontab -`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  async function copyCommand(job: CronJob) {
    try {
      await navigator.clipboard.writeText(job.command);
      copiedKey = job.raw;
      uiStore.addToast('Command copied to clipboard', 'success');
      setTimeout(() => { copiedKey = null; }, 1500);
    } catch {
      uiStore.addToast('Failed to copy command', 'error');
    }
  }

  /** A timer is "on" when the unit is loaded and active — the badge and the
   *  switch must always agree, otherwise the control lies to the user. */
  function timerIsActive(timer: SystemdTimer) {
    return timer.status.toLowerCase().startsWith('active');
  }

  async function toggleTimer(timer: SystemdTimer) {
    const enable = !timerIsActive(timer);
    statusStore.setBusy(`${enable ? 'Enabling' : 'Disabling'} timer ${timer.unit}…`);
    try {
      await invoke('cron_toggle_timer', { unit: timer.unit, enable });
      uiStore.addToast(`Timer ${timer.unit} ${enable ? 'enabled' : 'disabled'}`, 'success');
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} --now ${timer.unit}`, 0, true);
      await loadData();
    } catch (e) {
      uiStore.addToast(`Failed to toggle timer: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} --now ${timer.unit}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  function setPreset(preset: string) {
    newSchedule = preset;
  }

  async function browseFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Scripts', extensions: ['sh', 'bash', 'py', 'pl'] }]
      });
      if (selected && typeof selected === 'string') {
        newCommand = selected;
      }
    } catch (e) {
      uiStore.addToast(`Failed to open file dialog: ${e}`, 'error');
    }
  }

  $effect(() => { loadData(); });
</script>

<div class="module-page">
  <PageHeader title="Scheduled Tasks" subtitle="Manage system and user cron jobs, and systemd timers" icon={Clock}>
    <TabGroup
      tabs={[
        { id: 'cron', label: 'Cron Jobs', count: jobs.length },
        { id: 'timers', label: 'Systemd Timers', count: timers.length }
      ]}
      bind:activeTab={view}
      onchange={() => (filterQuery = '')}
    />
    <Button variant="ghost" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    {#if view === 'cron'}
      <Button variant="primary" onclick={() => showAdd = true}>
        <Plus size={14} /> Add Job
      </Button>
    {/if}
  </PageHeader>

  <SideDrawer bind:isOpen={showAdd} title="Create Scheduled Task" width="520px">
    <div class="drawer-form">
      <div class="field">
        <label class="field-label" for="cron-schedule">Schedule</label>
        <input
          id="cron-schedule"
          class="input mono-input"
          bind:value={newSchedule}
          spellcheck="false"
          autocomplete="off"
        />
        <div class="schedule-preview" class:invalid={!scheduleValid}>
          <CalendarClock size={13} />
          <span>
            {scheduleValid
              ? humanizeSchedule(newSchedule)
              : 'Invalid schedule — use 5 fields (min hour dom mon dow) or @reboot / @hourly / @daily / @weekly'}
          </span>
        </div>
        <div class="preset-row">
          <Button variant="outline" size="sm" onclick={() => setPreset('* * * * *')}>Every Min</Button>
          <Button variant="outline" size="sm" onclick={() => setPreset('0 * * * *')}>Hourly</Button>
          <Button variant="outline" size="sm" onclick={() => setPreset('0 0 * * *')}>Daily</Button>
          <Button variant="outline" size="sm" onclick={() => setPreset('0 0 * * 0')}>Weekly</Button>
          <Button variant="outline" size="sm" onclick={() => setPreset('@reboot')}>On Boot</Button>
        </div>
      </div>

      <div class="field">
        <label class="field-label" for="cron-command">Command to Execute</label>
        <div class="value-group">
          <input
            id="cron-command"
            class="input mono-input"
            bind:value={newCommand}
            placeholder="/path/to/script.sh"
            spellcheck="false"
            autocomplete="off"
          />
          <button type="button" class="group-btn" onclick={browseFile} title="Browse for a script file">
            <FolderOpen size={14} />
          </button>
        </div>
      </div>

      <div class="field">
        <div class="toggle-row">
          <Toggle checked={isRootJob} onToggle={(val) => (isRootJob = val)} />
          <div class="toggle-text">
            <span class="toggle-title">Run as Root User</span>
            <span class="toggle-desc">
              {isRootJob
                ? 'System crontab (/etc/crontab) — runs as root on every boot'
                : 'Your user crontab (crontab -l) — runs as your account'}
            </span>
          </div>
        </div>
      </div>

      <div class="drawer-actions">
        <Button variant="ghost" onclick={() => showAdd = false}>Cancel</Button>
        <Button variant="primary" onclick={addJob} disabled={!canSave}>
          Save Job
        </Button>
      </div>
    </div>
  </SideDrawer>

  <div class="card task-card">
    <!-- Card header band: source context + count + filter -->
    <div class="task-card-header">
      <span class="task-file-chip">
        {#if view === 'cron'}<Clock size={14} />{:else}<Timer size={14} />{/if}
      </span>
      <span class="task-card-title">
        {view === 'cron' ? 'User & System Crontab' : 'Systemd Timers'}
      </span>
      <Badge variant="muted">
        {view === 'cron' ? filteredJobs.length : filteredTimers.length}
        {view === 'cron'
          ? (filteredJobs.length === 1 ? 'job' : 'jobs')
          : (filteredTimers.length === 1 ? 'timer' : 'timers')}
      </Badge>
      <div class="task-card-actions">
        <SearchBar
          bind:value={filterQuery}
          placeholder={view === 'cron' ? 'Filter jobs…' : 'Filter timers…'}
          style="width: 220px"
        />
      </div>
    </div>

    {#if loading}
      <div class="task-loading">
        <RefreshCw size={22} class="animate-spin-slow" />
        <span>{view === 'cron' ? 'Loading cron jobs…' : 'Loading systemd timers…'}</span>
      </div>
    {:else if view === 'cron'}
      {#if filteredJobs.length === 0}
        <div class="task-empty">
          <EmptyState
            icon={filterQuery ? Info : Clock}
            title={filterQuery ? 'No jobs match your filter' : 'No Scheduled Tasks'}
            description={filterQuery
              ? `Nothing matched "${filterQuery}". Try a different term.`
              : 'You haven\'t added any cron jobs yet. Create one to run a command on a schedule.'}
            actionLabel={filterQuery ? 'Clear filter' : 'Add First Job'}
            actionIcon={filterQuery ? undefined : Plus}
            onAction={() => (filterQuery ? (filterQuery = '') : (showAdd = true))}
          />
        </div>
      {:else}
        <div class="task-table-wrap">
          <table use:tableFeatures>
            <thead>
              <tr>
                <th class="task-col-icon"></th>
                <th style="width: 200px">Schedule</th>
                <th>Command</th>
                <th style="width: 100px">Scope</th>
                <th style="width: 76px; text-align: right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredJobs as job}
                <tr>
                  <td class="task-col-icon">
                    <span class="task-row-chip"><Clock size={11} /></span>
                  </td>
                  <td>
                    <div class="schedule-cell">
                      <span class="schedule-human">{humanizeSchedule(job.schedule)}</span>
                      <code class="schedule-raw">{job.schedule}</code>
                    </div>
                  </td>
                  <td>
                    <span class="task-command">{job.command}</span>
                  </td>
                  <td>
                    {#if job.is_root}
                      <Badge variant="error"><ShieldAlert size={10} /> Root</Badge>
                    {:else}
                      <Badge variant="info"><Shield size={10} /> User</Badge>
                    {/if}
                  </td>
                  <td class="task-actions">
                    <button
                      type="button"
                      class="action-btn"
                      onclick={() => copyCommand(job)}
                      title="Copy command"
                      aria-label="Copy command"
                    >
                      {#if copiedKey === job.raw}<Check size={14} />{:else}<Copy size={14} />{/if}
                    </button>
                    <button
                      type="button"
                      class="action-btn"
                      onclick={() => confirmDelete(job)}
                      title="Delete job"
                      aria-label="Delete job"
                    >
                      <Trash2 size={14} />
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {:else}
      {#if filteredTimers.length === 0}
        <div class="task-empty">
          <EmptyState
            icon={filterQuery ? Info : Timer}
            title={filterQuery ? 'No timers match your filter' : 'No Systemd Timers Found'}
            description={filterQuery
              ? `Nothing matched "${filterQuery}". Try a different term.`
              : 'This system has no systemd timer units.'}
            actionLabel={filterQuery ? 'Clear filter' : ''}
            onAction={() => (filterQuery = '')}
          />
        </div>
      {:else}
        <div class="task-table-wrap">
          <table use:tableFeatures>
            <thead>
              <tr>
                <th class="task-col-icon"></th>
                <th style="width: 210px">Timer Unit</th>
                <th style="width: 150px">Status</th>
                <th>Description</th>
                <th style="width: 76px; text-align: right">Enabled</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredTimers as timer}
                <tr>
                  <td class="task-col-icon">
                    <span class="task-row-chip"><Timer size={11} /></span>
                  </td>
                  <td>
                    <div class="unit-cell">
                      <code class="task-unit" title={timer.unit}>{timer.unit}</code>
                      <code class="unit-activates" title={timer.activates}>{timer.activates}</code>
                    </div>
                  </td>
                  <td>
                    <Badge variant={timerIsActive(timer) ? 'success' : 'muted'}>{timer.status}</Badge>
                  </td>
                  <td><span class="task-desc" title={timer.description || '—'}>{timer.description || '—'}</span></td>
                  <td class="task-actions">
                    <Toggle
                      checked={timerIsActive(timer)}
                      onToggle={() => toggleTimer(timer)}
                    />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  /* ── Card shell ─────────────────────────────────────────────── */
  /* Hugs its content instead of stretching into an empty slab */
  .task-card {
    padding: 0;
    overflow: hidden;
    flex: 0 1 auto;
    max-height: 100%;
    display: flex;
    flex-direction: column;
  }

  /* Header band — raised surface so it reads as a header, not as content */
  .task-card-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: var(--color-bg-raised);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .task-file-chip {
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

  .task-card-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
  }

  .task-card-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* ── Table ──────────────────────────────────────────────────── */
  .task-table-wrap {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }

  /* Honour the column widths so long values truncate instead of pushing
     the action column off-screen */
  .task-table-wrap :global(table) {
    table-layout: fixed;
    width: 100%;
  }

  .task-table-wrap :global(tbody td) {
    padding: 7px 12px;
  }

  .task-col-icon {
    width: 34px;
    padding-left: 16px;
    padding-right: 0;
  }

  .task-row-chip {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
  }

  /* Schedule: plain-language first, raw expression underneath */
  .schedule-cell {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .schedule-human {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .schedule-raw {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  .task-command {
    font-family: var(--font-mono);
    font-size: 12.5px;
    color: var(--color-text-primary);
    /* break on spaces only — never mid-path */
    word-break: normal;
    overflow-wrap: break-word;
  }

  .task-unit {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Unit + the service it activates, stacked — saves a whole column */
  .unit-cell {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .unit-activates {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .task-desc {
    font-size: 12px;
    color: var(--color-text-secondary);
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Row actions ────────────────────────────────────────────── */
  .task-actions {
    text-align: right;
    white-space: nowrap;
  }

  .task-actions :global(.action-btn) {
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 6px;
    color: var(--color-text-muted);
  }

  /* Destructive action stays quiet until hovered */
  .task-actions :global(.action-btn:last-child:hover) {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  /* ── Empty & loading ───────────────────────────────────────── */
  .task-empty {
    padding: 20px;
  }

  .task-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  /* ── Add-job drawer ────────────────────────────────────────── */
  .drawer-form {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-label {
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-secondary);
  }

  .mono-input {
    font-family: var(--font-mono);
    font-size: 12.5px;
  }

  .schedule-preview {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px;
    border-radius: 7px;
    background: var(--color-accent-muted);
    border: 1px solid var(--color-accent-glow);
    color: var(--color-accent);
    font-size: 11.5px;
    font-weight: 600;
  }

  .schedule-preview.invalid {
    background: var(--color-error-muted);
    border-color: rgba(239, 68, 68, 0.25);
    color: var(--color-error);
    font-weight: 500;
  }

  .preset-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 2px;
  }

  .value-group {
    display: flex;
    align-items: stretch;
    gap: 0;
  }

  .value-group .input {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    min-width: 0;
  }

  .group-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    flex-shrink: 0;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-left: none;
    border-radius: 0 6px 6px 0;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .group-btn:hover {
    color: var(--color-accent);
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
  }

  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .toggle-desc {
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .drawer-actions {
    display: flex;
    gap: 12px;
    justify-content: flex-end;
    margin-top: 4px;
  }
</style>
