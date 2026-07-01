<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Clock, Plus, Trash2, RefreshCw, ShieldAlert, Shield, FolderOpen } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface CronJob {
    raw: string;
    schedule: string;
    command: string;
    is_root: boolean;
  }

  let jobs = $state<CronJob[]>([]);
  let loading = $state(true);

  let showAdd = $state(false);
  let isRootJob = $state(false);
  let newSchedule = $state('* * * * *');
  let newCommand = $state('');

  async function loadJobs() {
    loading = true;
    statusStore.setBusy('Loading cron jobs…');
    try {
      jobs = await invoke<CronJob[]>('list_cron_jobs');
      statusStore.setLastCommand('crontab -l', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load cron jobs: ${e}`, 'error');
      statusStore.setLastCommand('crontab', 1, false);
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
      uiStore.addToast('Cron job added', 'success');
      showAdd = false;
      newCommand = '';
      await loadJobs();
    } catch (e) {
      uiStore.addToast(`Failed to add cron job: ${e}`, 'error');
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
      uiStore.addToast('Cron job deleted', 'success');
      await loadJobs();
    } catch (e) {
      uiStore.addToast(`Failed to delete cron job: ${e}`, 'error');
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

  $effect(() => { loadJobs(); });
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><Clock size={20} /></div>
    <div>
      <h1 class="module-title">Scheduled Tasks</h1>
      <p class="module-subtitle">Manage system and user cron jobs</p>
    </div>
    <div style="margin-left:auto; display:flex; gap:8px">
      <button class="btn btn-ghost" onclick={loadJobs} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
      </button>
      <button class="btn btn-primary" onclick={() => showAdd = !showAdd}>
        <Plus size={14} /> Add Job
      </button>
    </div>
  </div>

  {#if showAdd}
    <div class="card" style="margin-bottom: 16px; border: 1px solid var(--color-border-focus)">
      <h3 style="margin-top:0; color:var(--color-text-primary)">Create Scheduled Task</h3>
      
      <div style="display:flex; gap:16px; margin-bottom:12px">
        <div style="flex:1">
          <label style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Schedule</label>
          <input class="w-full" style="font-family:var(--font-mono)" bind:value={newSchedule} />
          
          <div style="display:flex; gap:4px; margin-top:8px">
            <button class="btn btn-sm btn-outline" onclick={() => setPreset('* * * * *')}>Every Min</button>
            <button class="btn btn-sm btn-outline" onclick={() => setPreset('0 * * * *')}>Hourly</button>
            <button class="btn btn-sm btn-outline" onclick={() => setPreset('0 0 * * *')}>Daily</button>
            <button class="btn btn-sm btn-outline" onclick={() => setPreset('0 0 * * 0')}>Weekly</button>
            <button class="btn btn-sm btn-outline" onclick={() => setPreset('@reboot')}>On Boot</button>
          </div>
        </div>
        
        <div style="flex:2">
          <label style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary)">Command to Execute</label>
          <div style="display:flex; gap:8px">
            <input class="w-full" style="font-family:var(--font-mono)" bind:value={newCommand} placeholder="/path/to/script.sh" />
            <button class="btn btn-outline" onclick={browseFile} title="Browse for script file">
              <FolderOpen size={16} />
            </button>
          </div>
        </div>
      </div>

      <div style="display:flex; align-items:center; justify-content:space-between">
        <label style="display:flex; align-items:center; gap:8px; font-size:14px; cursor:pointer; color:var(--color-text-primary)">
          <input type="checkbox" bind:checked={isRootJob} />
          Run as Root User (System-wide)
        </label>
        
        <div style="display:flex; gap:8px">
          <button class="btn btn-outline" onclick={() => showAdd = false}>Cancel</button>
          <button class="btn btn-primary" onclick={addJob} disabled={!newCommand.trim()}>Save Job</button>
        </div>
      </div>
    </div>
  {/if}

  <div class="card module-content-scroll" style="padding:0">
    <div class="table-wrap" style="border:none; border-radius:0">
      <table>
        <thead>
          <tr>
            <th style="width:140px">Schedule</th>
            <th>Command</th>
            <th style="width:100px">User</th>
            <th style="text-align:right; width:80px">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#if jobs.length === 0}
            <tr>
              <td colspan="4" style="text-align:center; padding:32px; color:var(--color-text-muted)">
                No scheduled tasks found.
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
                  <button class="btn btn-sm btn-danger" onclick={() => confirmDelete(job)} title="Delete Task">
                    <Trash2 size={14} />
                  </button>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  </div>
</div>
