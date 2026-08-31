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
  import { Globe, Plus, Trash2, RefreshCw, Save, FolderOpen } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';

  interface EnvVar {
    key: string;
    value: string;
    raw: string;
  }

  let vars = $state<EnvVar[]>([]);
  let originalVars = $state<EnvVar[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let hasChanges = $state(false);
  let showDiffModal = $state(false);

  function serializeEnvVars(list: EnvVar[]): string {
    return list
      .filter(v => v.key.trim())
      .map(v => `${v.key.trim()}="${v.value.trim()}"`)
      .join('\n');
  }

  async function loadVars() {
    loading = true;
    statusStore.setBusy('Reading /etc/environment…');
    try {
      vars = await invoke<EnvVar[]>('read_env_vars');
      originalVars = JSON.parse(JSON.stringify(vars));
      statusStore.setLastCommand('cat /etc/environment', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load env vars: ${e}`, 'error');
      statusStore.setLastCommand('cat /etc/environment', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function addVar() {
    vars = [...vars, { key: '', value: '', raw: '' }];
  }

  function removeVar(index: number) {
    vars = vars.filter((_, i) => i !== index);
  }

  function confirmSave() {
    showDiffModal = true;
  }

  async function doSave() {
    // Filter out empties
    const validVars = vars.filter(v => v.key.trim() !== '');
    saving = true;
    statusStore.setBusy('Saving /etc/environment…');
    try {
      await invoke('write_env_vars', { vars: validVars });
      statusStore.setLastCommand('echo "..." > /etc/environment', 0, true);
      uiStore.addToast('/etc/environment saved successfully', 'success');
      hasChanges = false;
      originalVars = JSON.parse(JSON.stringify(vars));
      showDiffModal = false;
      await loadVars();
    } catch (e) {
      uiStore.addToast(`Failed to save env vars: ${e}`, 'error');
      statusStore.setLastCommand('echo "..." > /etc/environment', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  async function browseFile(index: number) {
    try {
      const selected = await open({
        multiple: false
      });
      if (selected && typeof selected === 'string') {
        vars[index].value = selected;
      }
    } catch (e) {
      uiStore.addToast(`Failed to open file dialog: ${e}`, 'error');
    }
  }

  $effect(() => { loadVars(); });
</script>

<div class="module-page">
  <PageHeader title="Environment Variables" icon={Globe}>
    <Button variant="ghost" size="sm" onclick={loadVars} disabled={loading || saving}>
      <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    <Button variant="primary" size="sm" onclick={confirmSave} disabled={loading || saving}>
      <Save size={13} /> Save Changes
    </Button>
  </PageHeader>

  {#if loading && vars.length === 0}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Loading Environment…</span>
    </div>
  {:else}
    <div class="card module-content-scroll" style="padding:0">
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
          <thead>
            <tr>
              <th style="width:30%">Variable Name</th>
              <th>Value</th>
              <th style="width:80px; text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each vars as v, i}
              <tr>
                <td>
                  <input class="input" style="font-family:var(--font-mono); font-weight:600; font-size:12px" bind:value={v.key} placeholder="e.g. JAVA_HOME" />
                </td>
                <td>
                  <div style="display:flex; gap:8px">
                    <input class="input" style="font-family:var(--font-mono); font-size:12px" bind:value={v.value} placeholder="/usr/lib/jvm/java-11-openjdk" />
                    <Button class="btn btn-sm -outline" style="padding: 0 8px" onclick={() => browseFile(i)} title="Browse for file or directory">
                      <FolderOpen size={14} />
                    </Button>
                  </div>
                </td>
                <td style="text-align:right">
                  <Button class="btn btn-sm -ghost" style="color:var(--color-danger)" onclick={() => removeVar(i)} title="Remove Variable">
                    <Trash2 size={14} />
                  </Button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <div style="padding:16px; border-top:1px solid var(--color-border); background:var(--color-bg-raised)">
        <Button variant="outline" class="" onclick={addVar}>
          <Plus size={14} /> Add Variable
        </Button>
      </div>
    </div>
  {/if}

  <!-- Config Diff Modal -->
  <ConfigDiffModal
    bind:show={showDiffModal}
    filePath="/etc/environment"
    title="Review /etc/environment Changes"
    oldContent={serializeEnvVars(originalVars)}
    newContent={serializeEnvVars(vars)}
    warningMessage="Note: Global environment changes will take effect for all users upon next login or reboot."
    isSaving={saving}
    onconfirm={doSave}
    oncancel={() => showDiffModal = false}
  />
</div>
