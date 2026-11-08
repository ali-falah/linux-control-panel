<script lang="ts">
  import { tick } from 'svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { FileText, Variable, Plus, Trash2, RefreshCw, Save, FolderOpen } from '@lucide/svelte';
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
  let showDiffModal = $state(false);

  // Dirty state is derived from the file content instead of tracked by hand
  const hasChanges = $derived(serializeEnvVars(vars) !== serializeEnvVars(originalVars));

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

  function requestReload() {
    if (!hasChanges) {
      loadVars();
      return;
    }
    uiStore.confirm(
      'Discard unsaved changes?',
      'You have unsaved changes to /etc/environment. Reloading will discard them.',
      () => loadVars(),
      true
    );
  }

  async function addVar() {
    vars = [...vars, { key: '', value: '', raw: '' }];
    await tick();
    document.getElementById(`env-name-${vars.length - 1}`)?.focus();
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
  <PageHeader title="Environment Variables" subtitle="Manage system-wide environment variables (/etc/environment)" icon={FileText}>
    <Button variant="ghost" onclick={requestReload} disabled={loading || saving} title="Reload /etc/environment from disk">
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    <Button
      variant="primary"
      onclick={confirmSave}
      disabled={loading || saving || !hasChanges}
      title={hasChanges ? 'Save changes to /etc/environment' : 'No changes to save'}
    >
      <Save size={14} /> Save Changes
    </Button>
  </PageHeader>

  {#if loading && vars.length === 0}
    <div class="env-loading">
      <RefreshCw size={22} class="animate-spin-slow" />
      <span>Reading /etc/environment…</span>
    </div>
  {:else}
    <div class="card env-card">
      <!-- Card header band: file context + count + primary action -->
      <div class="env-card-header">
        <span class="env-file-chip"><FileText size={14} /></span>
        <code class="env-file-path">/etc/environment</code>
        <Badge variant="muted">{vars.length} {vars.length === 1 ? 'variable' : 'variables'}</Badge>
        <div class="env-card-actions">
          <Button variant="outline" size="sm" onclick={addVar}>
            <Plus size={13} /> Add Variable
          </Button>
        </div>
      </div>

      {#if vars.length === 0}
        <div class="env-empty">
          <EmptyState
            icon={Variable}
            title="No environment variables"
            description="/etc/environment has no entries yet. Add a variable to make it available to every user on the next login."
            actionLabel="Add Variable"
            actionIcon={Plus}
            onAction={addVar}
          />
        </div>
      {:else}
        <div class="env-table-wrap">
          <table>
            <thead>
              <tr>
                <th class="env-col-icon"></th>
                <th style="width: 260px">Variable Name</th>
                <th>Value</th>
                <th style="width: 64px; text-align: right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each vars as v, i}
                <tr>
                  <td class="env-col-icon">
                    <span class="env-row-chip"><Variable size={11} /></span>
                  </td>
                  <td class="env-name-cell">
                    <Input
                      id={`env-name-${i}`}
                      class="env-name-input"
                      bind:value={v.key}
                      placeholder="e.g. JAVA_HOME"
                      spellcheck="false"
                      autocomplete="off"
                    />
                  </td>
                  <td>
                    <div class="env-value-group">
                      <Input
                        class="env-value-input"
                        bind:value={v.value}
                        placeholder="/usr/lib/jvm/java-11-openjdk"
                        spellcheck="false"
                        autocomplete="off"
                      />
                      <button
                        type="button"
                        class="env-browse-btn"
                        onclick={() => browseFile(i)}
                        title="Browse for a file or directory"
                      >
                        <FolderOpen size={14} />
                      </button>
                    </div>
                  </td>
                  <td class="env-actions">
                    <button
                      type="button"
                      class="action-btn"
                      onclick={() => removeVar(i)}
                      title="Remove variable"
                      aria-label="Remove {v.key || 'variable'}"
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

<style>
  /* ── Card shell ─────────────────────────────────────────────── */
  /* Hugs its content instead of stretching into an empty slab */
  .env-card {
    padding: 0;
    overflow: hidden;
    flex: 0 1 auto;
    max-height: 100%;
    display: flex;
    flex-direction: column;
  }

  /* Header band — raised surface so it reads as a header, not as content */
  .env-card-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: var(--color-bg-raised);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .env-file-chip {
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

  .env-file-path {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .env-card-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* ── Table ──────────────────────────────────────────────────── */
  .env-table-wrap {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }

  .env-table-wrap :global(tbody td) {
    padding: 7px 12px;
  }

  .env-col-icon {
    width: 34px;
    padding-left: 16px;
    padding-right: 0;
  }

  .env-row-chip {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
  }

  /* ── Row fields ─────────────────────────────────────────────── */
  .env-name-cell :global(.input) {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 12.5px;
    height: 32px;
    padding: 6px 10px;
  }

  .env-value-group {
    display: flex;
    align-items: stretch;
    gap: 0;
  }

  .env-value-group :global(.input) {
    font-family: var(--font-mono);
    font-size: 12.5px;
    height: 32px;
    padding: 6px 10px;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    min-width: 0;
  }

  /* Browse is an input affordance — styled as part of the field group */
  .env-browse-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 32px;
    flex-shrink: 0;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-left: none;
    border-radius: 0 6px 6px 0;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .env-browse-btn:hover {
    color: var(--color-accent);
    background: var(--color-accent-muted);
    border-color: var(--color-accent);
  }

  /* ── Row actions ────────────────────────────────────────────── */
  .env-actions {
    text-align: right;
    white-space: nowrap;
  }

  .env-actions :global(.action-btn) {
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 6px;
    color: var(--color-text-muted);
  }

  /* Destructive action stays quiet until hovered */
  .env-actions :global(.action-btn:hover) {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  /* ── Empty & loading ───────────────────────────────────────── */
  .env-empty {
    padding: 20px;
  }

  .env-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 14px;
  }
</style>
