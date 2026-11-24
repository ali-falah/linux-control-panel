<script lang="ts">
  import { onMount } from 'svelte';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Clock, Plus, Trash2, RefreshCw, ShieldAlert, FolderOpen,
    Copy, Check, Timer, CalendarClock, Info, Lock, User, PlayCircle
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import {
    scheduleError, nextRun, humanizeSchedule,
    relativeLabel, relativePast, absoluteLabel, epochLabel
  } from '../utils/cronSchedule.ts';

  interface CronJob {
    id: string;
    schedule: string;
    command: string;
    user: string;
    source: 'user' | 'system';
    file: string;
    raw: string;
    editable: boolean;
    occurrence: number;
  }

  interface CronState {
    user: string;
    jobs: CronJob[];
    errors: string[];
  }

  interface SystemdTimer {
    unit: string;
    activates: string;
    description: string;
    enabled: boolean;
    active: boolean;
    state: string;
    status: string;
    next_run: number | null;
    last_run: number | null;
  }

  type View = 'cron' | 'timers';
  type Scope = 'all' | 'user' | 'system';
  type TimerScope = 'all' | 'active' | 'inactive';

  let view = $state<View>(
    uiStore.targetSubTab && ['cron', 'timers'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as View)
      : 'cron'
  );
  if (uiStore.targetSubTab && ['cron', 'timers'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  let cronState = $state<CronState>({ user: '', jobs: [], errors: [] });
  let timers = $state<SystemdTimer[]>([]);
  let loading = $state(true);
  let loaded = $state(false);
  let loadError = $state('');
  let timerError = $state('');

  let showAdd = $state(false);
  let adding = $state(false);
  let newSchedule = $state('0 3 * * *');
  let newCommand = $state('');
  let filterQuery = $state('');
  let scope = $state<Scope>('all');
  let timerScope = $state<TimerScope>('all');
  let copiedId = $state<string | null>(null);
  let nowTick = $state(Date.now());

  // ── View data ─────────────────────────────────────────────────────────
  const scheduleViews = $derived.by(() => {
    nowTick;
    const views = new Map<string, { human: string; next: Date | null; relative: string; absolute: string }>();
    for (const job of cronState.jobs) {
      if (views.has(job.schedule)) continue;
      const next = nextRun(job.schedule);
      views.set(job.schedule, {
        human: humanizeSchedule(job.schedule),
        next,
        relative: next
          ? relativeLabel(next, nowTick)
          : (job.schedule.trim().toLowerCase() === '@reboot' ? 'At boot' : '—'),
        absolute: next ? absoluteLabel(next) : ''
      });
    }
    return views;
  });

  const userJobCount = $derived(cronState.jobs.filter((job) => job.source === 'user').length);
  const systemJobCount = $derived(cronState.jobs.filter((job) => job.source === 'system').length);
  const enabledTimerCount = $derived(timers.filter((timer) => timer.enabled).length);
  const activeTimerCount = $derived(timers.filter((timer) => timer.active).length);

  const nextEvent = $derived.by(() => {
    nowTick;
    let best: { at: number; label: string; hint: string; short: string } | null = null;

    for (const timer of timers) {
      if (!timer.next_run) continue;
      const at = timer.next_run * 1000;
      if (!best || at < best.at) {
        best = {
          at,
          label: timer.unit,
          hint: 'systemd timer',
          short: timer.unit.replace(/\.timer$/, '')
        };
      }
    }
    for (const job of cronState.jobs) {
      const next = scheduleViews.get(job.schedule)?.next;
      if (!next) continue;
      if (!best || next.getTime() < best.at) {
        const binary = job.command.trim().split(/\s+/)[0] ?? job.command;
        best = {
          at: next.getTime(),
          label: job.command,
          hint: job.schedule,
          short: binary.split('/').pop() || binary
        };
      }
    }
    return best;
  });

  const visibleJobs = $derived.by(() => {
    const query = filterQuery.trim().toLowerCase();
    const list = cronState.jobs.filter((job) => {
      if (scope === 'user' && job.source !== 'user') return false;
      if (scope === 'system' && job.source !== 'system') return false;
      if (!query) return true;
      return [
        job.command,
        job.schedule,
        job.user,
        job.file,
        scheduleViews.get(job.schedule)?.human ?? ''
      ].some((value) => value.toLowerCase().includes(query));
    });

    return list.sort((a, b) => {
      if (a.source !== b.source) return a.source === 'user' ? -1 : 1;
      const nextA = scheduleViews.get(a.schedule)?.next?.getTime() ?? Number.MAX_SAFE_INTEGER;
      const nextB = scheduleViews.get(b.schedule)?.next?.getTime() ?? Number.MAX_SAFE_INTEGER;
      return nextA - nextB || a.raw.localeCompare(b.raw);
    });
  });

  const visibleTimers = $derived.by(() => {
    const query = filterQuery.trim().toLowerCase();
    return timers.filter((timer) => {
      if (timerScope === 'active' && !timer.active) return false;
      if (timerScope === 'inactive' && timer.active) return false;
      if (!query) return true;
      return [timer.unit, timer.activates, timer.description, timer.status]
        .some((value) => value.toLowerCase().includes(query));
    });
  });

  const errors = $derived([
    ...cronState.errors,
    ...(timerError ? [`Systemd timers: ${timerError}`] : [])
  ]);

  const scheduleValid = $derived(scheduleError(newSchedule) === null);
  const previewHuman = $derived(scheduleValid ? humanizeSchedule(newSchedule) : '');
  const previewNext = $derived.by(() => {
    nowTick;
    if (!scheduleValid) return '';
    const next = nextRun(newSchedule);
    return next ? `${relativeLabel(next, nowTick)} · ${absoluteLabel(next)}` : 'Runs once at boot';
  });
  const canSave = $derived(newCommand.trim().length > 0 && scheduleValid);

  // ── Data ──────────────────────────────────────────────────────────────
  async function loadData() {
    loading = true;
    statusStore.setBusy('Loading scheduled tasks…');
    try {
      const [cronResult, timerResult] = await Promise.allSettled([
        invoke<CronState>('list_cron_jobs'),
        invoke<SystemdTimer[]>('cron_list_timers')
      ]);

      let anySuccess = false;
      if (cronResult.status === 'fulfilled') {
        cronState = cronResult.value;
        anySuccess = true;
      } else {
        cronState = {
          user: cronState.user,
          jobs: [],
          errors: [`Your crontab could not be read: ${cronResult.reason}`]
        };
      }

      if (timerResult.status === 'fulfilled') {
        timers = timerResult.value;
        timerError = '';
        anySuccess = true;
      } else {
        timers = [];
        timerError = String(timerResult.reason);
      }

      loadError = anySuccess ? '' : 'Scheduled task data is unavailable.';
      statusStore.setLastCommand('crontab -l; systemctl list-timers', 0, anySuccess);
    } catch (error) {
      loadError = String(error);
      statusStore.setLastCommand('crontab -l; systemctl list-timers', 1, false);
    } finally {
      loading = false;
      loaded = true;
      statusStore.clearBusy();
    }
  }

  async function addJob() {
    if (!canSave || adding) return;
    adding = true;
    statusStore.setBusy('Adding cron job…');
    const entry = `${newSchedule.trim()} ${newCommand.trim()}`;
    try {
      await invoke('add_cron_job', { schedule: newSchedule, command: newCommand });
      statusStore.setLastCommand(`crontab - (append) <<< "${entry}"`, 0, true);
      uiStore.addToast('Job added to your crontab', 'success');
      showAdd = false;
      newCommand = '';
      await loadData();
    } catch (error) {
      statusStore.setLastCommand(`crontab - (append) <<< "${entry}"`, 1, false);
      uiStore.addToast(`Could not add the job: ${error}`, 'error');
    } finally {
      adding = false;
      statusStore.clearBusy();
    }
  }

  function confirmDelete(job: CronJob) {
    uiStore.confirm(
      'Delete scheduled task',
      `This removes the line from your crontab:\n\n${job.raw}\n\n${humanizeSchedule(job.schedule)}`,
      () => doDelete(job),
      true
    );
  }

  async function doDelete(job: CronJob) {
    statusStore.setBusy('Deleting cron job…');
    try {
      await invoke('delete_cron_job', { raw: job.raw, occurrence: job.occurrence });
      statusStore.setLastCommand(`crontab - (remove) <<< "${job.raw}"`, 0, true);
      uiStore.addToast('Job deleted', 'success');
      await loadData();
    } catch (error) {
      statusStore.setLastCommand(`crontab - (remove) <<< "${job.raw}"`, 1, false);
      uiStore.addToast(`Could not delete the job: ${error}`, 'error');
      await loadData();
    } finally {
      statusStore.clearBusy();
    }
  }

  async function copyJob(job: CronJob) {
    try {
      await navigator.clipboard.writeText(job.raw);
      copiedId = job.id;
      uiStore.addToast('Crontab line copied', 'success');
      setTimeout(() => { copiedId = null; }, 1500);
    } catch {
      uiStore.addToast('Could not copy the line', 'error');
    }
  }

  async function toggleTimer(timer: SystemdTimer, enable: boolean) {
    statusStore.setBusy(`${enable ? 'Enabling' : 'Disabling'} ${timer.unit}…`);
    try {
      await invoke('cron_toggle_timer', { unit: timer.unit, enable });
      uiStore.addToast(`Timer ${timer.unit} ${enable ? 'enabled' : 'disabled'}`, 'success');
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} --now ${timer.unit}`, 0, true);
      await loadData();
    } catch (error) {
      const message = String(error);
      if (message.includes('Admin mode is off')) {
        uiStore.addToast('Turn on Admin mode in the status bar to change timers.', 'warning');
      } else {
        uiStore.addToast(`Could not update ${timer.unit}: ${message}`, 'error');
      }
      statusStore.setLastCommand(`systemctl ${enable ? 'enable' : 'disable'} --now ${timer.unit}`, 1, false);
      // The switch flips locally first; reload so it mirrors the real cronState.
      await loadData();
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
      if (selected && typeof selected === 'string') newCommand = selected;
    } catch (error) {
      uiStore.addToast(`Could not open the file dialog: ${error}`, 'error');
    }
  }

  onMount(() => {
    loadData();
    const timer = setInterval(() => { nowTick = Date.now(); }, 30_000);
    return () => clearInterval(timer);
  });
</script>

<div class="module-page">
  <PageHeader title="Scheduled Tasks" subtitle="Cron jobs and systemd timers on this machine" icon={Clock}>
    <TabGroup
      size="sm"
      tabs={[
        { id: 'cron', label: 'Cron Jobs', count: cronState.jobs.length },
        { id: 'timers', label: 'Systemd Timers', count: timers.length }
      ]}
      bind:activeTab={view}
      onchange={() => (filterQuery = '')}
    />
    <button class="header-refresh-btn" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </button>
    {#if view === 'cron'}
      <Button size="sm" variant="primary" onclick={() => (showAdd = true)}>
        <Plus size={14} /> Add Job
      </Button>
    {/if}
  </PageHeader>

  <div class="cron-overview">
    <KpiCard icon={User} value={userJobCount} label="Your jobs" subtext={cronState.user || 'crontab'} />
    <KpiCard icon={Lock} value={systemJobCount} label="System jobs" subtext="read-only" />
    <KpiCard
      icon={Timer}
      value={enabledTimerCount}
      label="Timers enabled"
      subtext={`${activeTimerCount} running`}
    />
    <KpiCard
      icon={PlayCircle}
      value={nextEvent ? relativeLabel(new Date(nextEvent.at), nowTick) : '—'}
      label="Next run"
      subtext={nextEvent ? nextEvent.short : 'Nothing yet'}
      title={nextEvent ? `${nextEvent.label} — ${nextEvent.hint}` : 'No upcoming run found'}
    />
  </div>

  {#if errors.length > 0}
    <div class="cron-notice" role="status">
      <ShieldAlert size={16} />
      <div>
        <strong>Some schedules could not be read</strong>
        {#each errors as error}
          <p>{error}</p>
        {/each}
      </div>
    </div>
  {/if}

  {#if !loaded && loading}
    <div class="cron-card cron-loading">
      <RefreshCw size={22} class="animate-spin-slow" />
      <span>Loading scheduled tasks…</span>
    </div>
  {:else if !loaded && loadError}
    <EmptyState
      icon={ShieldAlert}
      title="Scheduled task data unavailable"
      description={loadError}
      actionLabel="Retry"
      actionIcon={RefreshCw}
      onAction={loadData}
    />
  {:else if view === 'cron'}
    <section class="cron-card">
      <div class="card-head">
        <span class="head-chip"><CalendarClock size={15} /></span>
        <div class="head-copy">
          <h3>Command schedules</h3>
          <p>
            {cronState.user ? `${cronState.user}'s crontab` : 'Your crontab'}
            {#if systemJobCount > 0}· plus {systemJobCount} read-only system {systemJobCount === 1 ? 'entry' : 'entries'}{/if}
          </p>
        </div>
        <div class="head-tools">
          <div class="seg" role="group" aria-label="Filter by scope">
            {#each [
              { id: 'all' as Scope, label: 'All', count: cronState.jobs.length },
              { id: 'user' as Scope, label: 'Yours', count: userJobCount },
              { id: 'system' as Scope, label: 'System', count: systemJobCount }
            ] as option}
              <button
                type="button"
                class:active={scope === option.id}
                aria-pressed={scope === option.id}
                onclick={() => (scope = option.id)}
              >
                {option.label}<span>{option.count}</span>
              </button>
            {/each}
          </div>
          <SearchBar
            bind:value={filterQuery}
            placeholder="Filter jobs…"
            style="width: 200px"
          />
        </div>
      </div>

      {#if visibleJobs.length === 0}
        <div class="card-empty">
          <EmptyState
            icon={filterQuery || scope !== 'all' ? Info : Clock}
            title={filterQuery || scope !== 'all' ? 'No jobs match this view' : 'No scheduled commands'}
            description={filterQuery
              ? `Nothing matched "${filterQuery}". Try a different term.`
              : 'Commands added here run on the schedule you choose, using your account.'}
            actionLabel={filterQuery || scope !== 'all' ? 'Clear filters' : 'Add your first job'}
            actionIcon={filterQuery || scope !== 'all' ? undefined : Plus}
            onAction={() => {
              if (filterQuery || scope !== 'all') {
                filterQuery = '';
                scope = 'all';
              } else {
                showAdd = true;
              }
            }}
          />
        </div>
      {:else}
        <div class="task-list">
          {#each visibleJobs as job (job.id)}
            {@const scheduleView = scheduleViews.get(job.schedule)}
            <div class="task-row" class:readonly={!job.editable}>
              <span class="task-chip"><Clock size={13} /></span>

              <div class="task-main">
                <div class="task-top">
                  <span class="task-title">{scheduleView?.human ?? 'Custom schedule'}</span>
                  <code class="task-raw">{job.schedule}</code>
                  <span class="next-chip" title={scheduleView?.absolute || 'No upcoming run'}>
                    <PlayCircle size={11} />{scheduleView?.relative ?? '—'}
                  </span>
                </div>
                <div class="task-bottom">
                  <code class="task-command" title={job.command}>{job.command}</code>
                </div>
              </div>

              <div class="task-meta">
                {#if job.source === 'user'}
                  <Badge variant="info"><User size={10} /> {job.user}</Badge>
                {:else}
                  <Badge variant="muted" title={job.file}><Lock size={10} /> {job.user}</Badge>
                  <span class="source-path" title={job.file}>{job.file}</span>
                {/if}
              </div>

              <div class="task-actions">
                <button
                  type="button"
                  class="action-btn"
                  onclick={() => copyJob(job)}
                  title="Copy the crontab line"
                  aria-label="Copy the crontab line"
                >
                  {#if copiedId === job.id}<Check size={14} />{:else}<Copy size={14} />{/if}
                </button>
                {#if job.editable}
                  <button
                    type="button"
                    class="action-btn danger"
                    onclick={() => confirmDelete(job)}
                    title="Delete this job"
                    aria-label="Delete this job"
                  >
                    <Trash2 size={14} />
                  </button>
                {:else}
                  <span class="lock-note" title="System cron files are shown read-only">read-only</span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {:else}
    <section class="cron-card">
      <div class="card-head">
        <span class="head-chip"><Timer size={15} /></span>
        <div class="head-copy">
          <h3>Systemd timers</h3>
          <p>Boot-enabled state and the unit each timer activates</p>
        </div>
        <div class="head-tools">
          <div class="seg" role="group" aria-label="Filter by run state">
            {#each [
              { id: 'all' as TimerScope, label: 'All', count: timers.length },
              { id: 'active' as TimerScope, label: 'Running', count: activeTimerCount },
              { id: 'inactive' as TimerScope, label: 'Stopped', count: timers.length - activeTimerCount }
            ] as option}
              <button
                type="button"
                class:active={timerScope === option.id}
                aria-pressed={timerScope === option.id}
                onclick={() => (timerScope = option.id)}
              >
                {option.label}<span>{option.count}</span>
              </button>
            {/each}
          </div>
          <SearchBar
            bind:value={filterQuery}
            placeholder="Filter timers…"
            style="width: 200px"
          />
        </div>
      </div>

      {#if visibleTimers.length === 0}
        <div class="card-empty">
          <EmptyState
            icon={filterQuery || timerScope !== 'all' ? Info : Timer}
            title={filterQuery || timerScope !== 'all' ? 'No timers match this view' : 'No systemd timers'}
            description={filterQuery
              ? `Nothing matched "${filterQuery}". Try a different term.`
              : 'This system has no timer units installed.'}
            actionLabel={filterQuery || timerScope !== 'all' ? 'Clear filters' : ''}
            onAction={() => {
              filterQuery = '';
              timerScope = 'all';
            }}
          />
        </div>
      {:else}
        <div class="task-list">
          {#each visibleTimers as timer (timer.unit)}
            <div class="task-row">
              <span class="task-chip timer"><Timer size={13} /></span>

              <div class="task-main">
                <div class="task-top">
                  <code class="task-unit">{timer.unit}</code>
                  <span class="state-chip" class:on={timer.active} class:off={!timer.active}>
                    <span class="state-dot"></span>{timer.status}
                  </span>
                  {#if timer.next_run}
                    <span class="next-chip" title={epochLabel(timer.next_run)}>
                      <PlayCircle size={11} />{relativeLabel(new Date(timer.next_run * 1000), nowTick)}
                    </span>
                  {:else}
                    <span class="next-chip muted"><PlayCircle size={11} />no next run</span>
                  {/if}
                </div>
                <div class="task-bottom">
                  <span class="task-desc" title={timer.description || timer.activates}>
                    {timer.description || `Activates ${timer.activates}`}
                  </span>
                  {#if timer.last_run}
                    <span class="task-sub">· last ran {relativePast(timer.last_run, nowTick)}</span>
                  {/if}
                </div>
              </div>

              <div class="task-meta">
                <code class="source-path" title={timer.activates}>{timer.activates}</code>
              </div>

              <div class="task-actions timer-actions">
                <span class="switch-caption" class:on={timer.enabled}>
                  {timer.enabled ? 'Enabled' : 'Disabled'}
                </span>
                <Toggle
                  checked={timer.enabled}
                  disabled={loading}
                  onToggle={(value) => toggleTimer(timer, value)}
                  aria-label={`Enable ${timer.unit} at boot`}
                  title="Enabled at boot — switching also starts or stops the timer"
                />
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

<SideDrawer bind:isOpen={showAdd} title="New scheduled task" width="520px">
  <div class="drawer-form">
    <div class="target-note">
      <User size={15} />
      <div>
        <strong>Added to {cronState.user ? `${cronState.user}'s` : 'your'} crontab</strong>
        <span>The command runs with your permissions. System cron files stay read-only.</span>
      </div>
    </div>

    <div class="field">
      <label class="field-label" for="cron-schedule">Schedule</label>
      <input
        id="cron-schedule"
        class="input mono-input"
        bind:value={newSchedule}
        spellcheck="false"
        autocomplete="off"
        aria-invalid={!scheduleValid}
      />
      <div class="schedule-preview" class:invalid={!scheduleValid}>
        <CalendarClock size={14} />
        <div class="preview-text">
          <strong>{scheduleValid ? previewHuman : 'Invalid schedule'}</strong>
          <span>
            {scheduleValid
              ? previewNext
              : scheduleError(newSchedule)}
          </span>
        </div>
      </div>
      <div class="preset-grid">
        {#each [
          { label: 'Every minute', value: '* * * * *' },
          { label: 'Every 5 min', value: '*/5 * * * *' },
          { label: 'Hourly', value: '0 * * * *' },
          { label: 'Daily 03:00', value: '0 3 * * *' },
          { label: 'Weekdays 09:00', value: '0 9 * * 1-5' },
          { label: 'At boot', value: '@reboot' }
        ] as preset}
          <button
            type="button"
            class="preset-btn"
            class:selected={newSchedule === preset.value}
            onclick={() => setPreset(preset.value)}
          >
            {preset.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="field">
      <label class="field-label" for="cron-command">Command</label>
      <div class="value-group">
        <input
          id="cron-command"
          class="input mono-input"
          bind:value={newCommand}
          placeholder="/usr/local/bin/backup.sh"
          spellcheck="false"
          autocomplete="off"
          onkeydown={(event) => event.key === 'Enter' && addJob()}
        />
        <button type="button" class="group-btn" onclick={browseFile} title="Browse for a script">
          <FolderOpen size={14} />
        </button>
      </div>
      <span class="field-hint">Use an absolute path — cron starts jobs with a minimal PATH.</span>
    </div>

    <div class="line-preview">
      <span>Crontab line</span>
      <code>{newSchedule.trim() || '* * * * *'} {newCommand.trim() || '…'}</code>
    </div>

    <div class="drawer-actions">
      <Button variant="ghost" onclick={() => (showAdd = false)}>Cancel</Button>
      <Button variant="primary" onclick={addJob} disabled={!canSave || adding} loading={adding}>
        <Plus size={14} /> Add job
      </Button>
    </div>
  </div>
</SideDrawer>

<style>
  .cron-overview {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 8px;
    flex-shrink: 0;
  }

  .cron-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex: 0 1 auto;
    min-height: 0;
  }

  .cron-loading {
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 54px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .cron-notice {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 10px 13px;
    border-radius: 9px;
    background: var(--color-warning-muted);
    color: var(--color-warning);
    font-size: 11.5px;
    flex-shrink: 0;
  }

  .cron-notice strong {
    display: block;
    font-size: 12px;
    margin-bottom: 2px;
  }

  .cron-notice p {
    margin: 1px 0 0;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  /* ── Card header ─────────────────────────────────────────────── */
  .card-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    background: var(--color-bg-raised);
    border-bottom: 1px solid var(--color-border-subtle);
    flex-wrap: wrap;
  }

  .head-chip {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .head-copy {
    min-width: 0;
  }

  .head-copy h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .head-copy p {
    margin: 1px 0 0;
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  .head-tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .seg {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--color-bg-card);
  }

  .seg button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }

  .seg button span {
    font-family: var(--font-mono);
    font-size: 9.5px;
    padding: 0 4px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-text-muted) 14%, transparent);
  }

  .seg button:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-hover);
  }

  .seg button.active {
    background: var(--color-accent-muted);
    color: var(--color-accent);
  }

  .seg button.active span {
    background: color-mix(in srgb, var(--color-accent) 22%, transparent);
  }

  /* ── Rows ────────────────────────────────────────────────────── */
  .task-list {
    overflow-y: auto;
    min-height: 0;
  }

  .task-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 14px;
    border-bottom: 1px solid var(--color-border-subtle);
    transition: background 0.14s ease;
  }

  .task-row:last-child {
    border-bottom: none;
  }

  .task-row:hover {
    background: var(--color-bg-hover);
  }

  .task-row.readonly {
    background: color-mix(in srgb, var(--color-text-muted) 3%, transparent);
  }

  .task-chip {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .task-row.readonly .task-chip {
    background: color-mix(in srgb, var(--color-text-muted) 12%, transparent);
    color: var(--color-text-muted);
  }

  .task-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .task-top {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .task-title {
    font-size: 12.5px;
    font-weight: 650;
    color: var(--color-text-primary);
    white-space: nowrap;
  }

  .task-raw,
  .task-unit {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    padding: 0 5px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--color-text-muted) 8%, transparent);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .task-unit {
    font-size: 12px;
    color: var(--color-text-primary);
    background: transparent;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .next-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-accent);
    background: var(--color-accent-muted);
    border-radius: 999px;
    padding: 1px 7px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .next-chip.muted {
    color: var(--color-text-muted);
    background: color-mix(in srgb, var(--color-text-muted) 8%, transparent);
  }

  .task-bottom {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .task-command {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .task-row.readonly .task-command {
    color: var(--color-text-muted);
  }

  .task-desc {
    font-size: 11.5px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .task-sub {
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .state-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .state-chip.on {
    color: var(--color-success);
  }

  .state-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-text-muted);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-text-muted) 18%, transparent);
  }

  .state-chip.on .state-dot {
    background: var(--color-success);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-success) 20%, transparent);
  }

  .task-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    max-width: 190px;
  }

  .source-path {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 130px;
  }

  .task-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    min-width: 64px;
    justify-content: flex-end;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: background 0.14s ease, color 0.14s ease;
  }

  .action-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .action-btn.danger:hover {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .lock-note {
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .timer-actions {
    gap: 8px;
    min-width: 118px;
  }

  .switch-caption {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .switch-caption.on {
    color: var(--color-success);
  }

  .card-empty {
    padding: 18px;
  }

  /* The card is already a frame; flatten the empty state inside it. */
  .card-empty :global(.empty-state-wrap),
  :global(html.light-mode) .card-empty :global(.empty-state-wrap) {
    border: none;
    background: transparent !important;
    padding: 34px 22px;
  }

  /* ── Drawer ──────────────────────────────────────────────────── */
  .drawer-form {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .target-note {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
  }

  .target-note strong {
    display: block;
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .target-note span {
    display: block;
    margin-top: 2px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--color-text-secondary);
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

  .field-hint {
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  .schedule-preview {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
  }

  .preview-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .preview-text strong {
    font-size: 12px;
    color: var(--color-text-primary);
  }

  .preview-text span {
    font-size: 10.5px;
    color: var(--color-text-secondary);
  }

  .schedule-preview.invalid {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .schedule-preview.invalid .preview-text strong {
    color: var(--color-error);
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }

  .preset-btn {
    width: 100%;
  }

  .preset-btn {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid transparent;
    background: color-mix(in srgb, var(--color-text-muted) 8%, transparent);
    color: var(--color-text-secondary);
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: background 0.14s ease, color 0.14s ease;
  }

  .preset-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .preset-btn.selected {
    background: var(--color-accent-muted);
    color: var(--color-accent);
  }

  .value-group {
    display: flex;
    align-items: stretch;
  }

  .value-group .input {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    min-width: 0;
    flex: 1;
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
    transition: background 0.15s ease, color 0.15s ease;
  }

  .group-btn:hover {
    color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .line-preview {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 9px 11px;
    border-radius: 8px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
  }

  .line-preview span {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .line-preview code {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    overflow-wrap: anywhere;
  }

  .drawer-actions {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 2px;
  }
</style>
