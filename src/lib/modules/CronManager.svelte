<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Clock, Plus, Trash2, RefreshCw, ShieldAlert, Shield, FolderOpen } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';

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

  let view = $state<'cron' | 'timers'>(
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
    if (!newCommand.trim()) return;
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

  async function toggleTimer(timer: SystemdTimer) {
    const isEnabled = timer.status.startsWith('active');
    const enable = !isEnabled;
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
  <PageHeader title="Scheduled Tasks" icon={Clock}>
    <div style="display:flex; background:var(--color-bg-raised); padding:4px; border-radius:8px; gap:4px; margin-right: 8px;">
      <Button class="btn btn-sm {view === 'cron' ? 'btn-primary' : '-ghost'}" onclick={() => view = 'cron'}>Cron Jobs</Button>
      <Button class="btn btn-sm {view === 'timers' ? 'btn-primary' : '-ghost'}" onclick={() => view = 'timers'}>Systemd Timers</Button>
    </div>
    <Button variant="ghost" onclick={loadData} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    {#if view === 'cron'}
      <Button variant="primary" onclick={() => showAdd = true}>
        <Plus size={14} /> Add Job
      </Button>
    {/if}
  </PageHeader>

  <SideDrawer bind:isOpen={showAdd} title="Create Scheduled Task" width="500px">
    <div style="display:flex; flex-direction:column; gap:20px;">
      
      <div>
        <label for="cron-schedule" style="display:block; font-size:12px; margin-bottom:4px; font-weight:600; color:var(--color-text-secondary); text-transform:uppercase; letter-spacing:0.05em;">Schedule</label>
        <input id="cron-schedule" class="input" style="width: 100%; font-family:var(--font-mono)" bind:value={newSchedule} />
        
        <div style="display:flex; gap:8px; margin-top:12px; flex-wrap:wrap;">
          <Button class="btn btn-sm -outline" onclick={() => setPreset('* * * * *')}>Every Min</Button>
          <Button class="btn btn-sm -outline" onclick={() => setPreset('0 * * * *')}>Hourly</Button>
          <Button class="btn btn-sm -outline" onclick={() => setPreset('0 0 * * *')}>Daily</Button>
          <Button class="btn btn-sm -outline" onclick={() => setPreset('0 0 * * 0')}>Weekly</Button>
          <Button class="btn btn-sm -outline" onclick={() => setPreset('@reboot')}>On Boot</Button>
        </div>
      </div>
      
      <div>
        <label for="cron-command" style="display:block; font-size:12px; margin-bottom:4px; font-weight:600; color:var(--color-text-secondary); text-transform:uppercase; letter-spacing:0.05em;">Command to Execute</label>
        <div style="display:flex; gap:8px">
          <input id="cron-command" class="input" style="width: 100%; font-family:var(--font-mono)" bind:value={newCommand} placeholder="/path/to/script.sh" />
          <Button variant="outline" onclick={browseFile} title="Browse for script file">
            <FolderOpen size={16} />
          </Button>
        </div>
      </div>

      <div style="margin-top: 8px;">
        <label for="root-toggle" style="display:flex; align-items:center; gap:10px; font-size:14px; cursor:pointer; color:var(--color-text-primary)">
          <Toggle checked={isRootJob} onToggle={(val) => isRootJob = val} />
          Run as Root User (System-wide)
        </label>
      </div>
      
      <div style="display:flex; gap:12px; justify-content:flex-end; margin-top:16px;">
        <Button variant="ghost" onclick={() => showAdd = false}>Cancel</Button>
        <Button variant="primary" onclick={addJob} disabled={!newCommand.trim()}>Save Job</Button>
      </div>
    </div>
  </SideDrawer>

  <div class="card module-content-scroll" style="padding:0">
    {#if view === 'cron'}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th style="width:140px">Schedule</th>
              <th>Command</th>
              <th style="width:100px">User</th>
              <th style="text-align:right; width:80px">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#if loading}
              <tr>
                <td colspan="4">
                  <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
                    <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
                      <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
                    </div>
                    <span style="font-weight:500">Loading cron jobs…</span>
                  </div>
                </td>
              </tr>
            {:else if jobs.length === 0}
              <tr>
                <td colspan="4">
                  <div class="empty-state" style="padding: 64px 32px;">
                    <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
                      <Clock size={32} class="empty-state-icon" style="margin:0" />
                    </div>
                    <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
                      No Scheduled Tasks
                    </span>
                    <span style="color:var(--color-text-muted); margin-top:8px;">
                      You haven't added any cron jobs yet.
                    </span>
                    <Button variant="outline" style="margin-top:24px;" onclick={() => showAdd = true}>
                      <Plus size={14} /> Add First Job
                    </Button>
                  </div>
                </td>
              </tr>
            {:else}
              {#each jobs as job}
                <tr>
                  <td><code style="font-size:12px">{job.schedule}</code></td>
                  <td><span style="font-family:var(--font-mono); font-size:13px; color:var(--color-text-primary)">{job.command}</span></td>
                  <td>
                    {#if job.is_root}
                      <span class="badge badge-danger"><ShieldAlert size={10} style="margin-right:4px"/> Root</span>
                    {:else}
                      <span class="badge badge-info"><Shield size={10} style="margin-right:4px"/> User</span>
                    {/if}
                  </td>
                  <td style="text-align:right">
                    <KebabMenu>
                      <button class="menu-item danger" onclick={() => confirmDelete(job)}>
                        <Trash2 size={14} /> Delete Job
                      </button>
                    </KebabMenu>
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th>Timer Unit</th>
              <th>Activates</th>
              <th>Status</th>
              <th>Description</th>
              <th style="text-align:right">Enable / Disable</th>
            </tr>
          </thead>
          <tbody>
            {#if loading}
              <tr>
                <td colspan="5">
                  <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
                    <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
                      <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
                    </div>
                    <span style="font-weight:500">Loading timers…</span>
                  </div>
                </td>
              </tr>
            {:else if timers.length === 0}
              <tr>
                <td colspan="5">
                  <div class="empty-state" style="padding: 64px 32px;">
                    <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
                      No Systemd Timers Found
                    </span>
                  </div>
                </td>
              </tr>
            {:else}
              {#each timers as timer}
                <tr>
                  <td><code style="font-size:12px">{timer.unit}</code></td>
                  <td>{timer.activates}</td>
                  <td>
                    {#if timer.status.includes('active')}
                      <span class="badge badge-success">{timer.status}</span>
                    {:else}
                      <span class="badge badge-outline">{timer.status}</span>
                    {/if}
                  </td>
                  <td><span style="font-size:12px; color:var(--color-text-secondary);">{timer.description || '—'}</span></td>
                  <td style="text-align:right; display:flex; justify-content:flex-end;">
                    <Toggle 
                      checked={timer.status.includes('active') && (timer.status.includes('running') || timer.status.includes('waiting'))} 
                      onToggle={() => toggleTimer(timer)} 
                    />
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>
