<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import KpiCard from '../components/ui/KpiCard.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FileText, Variable, Plus, Trash2, RefreshCw, Save, FolderOpen,
    AlertTriangle, Info, ShieldCheck, Layers
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import ConfigDiffModal from '../components/ConfigDiffModal.svelte';

  type LineKind = 'entry' | 'comment' | 'blank' | 'raw';

  interface EnvLine {
    kind: LineKind;
    text: string;
    key: string;
    value: string;
  }

  interface EnvFile {
    path: string;
    lines: EnvLine[];
  }

  let lines = $state<EnvLine[]>([]);
  let originalLines = $state<EnvLine[]>([]);
  let filePath = $state('/etc/environment');
  let loading = $state(true);
  let loaded = $state(false);
  let loadError = $state('');
  let saving = $state(false);
  let showDiffModal = $state(false);
  let filterQuery = $state('');

  // ── Document model ────────────────────────────────────────────────────
  const KEY_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

  function escapeValue(value: string): string {
    return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }

  /** Same shape the backend writes, so the preview is the real file text. */
  function serializeDocument(list: EnvLine[]): string {
    if (list.length === 0) return '';
    return list
      .map((line) => (line.kind === 'entry' ? `${line.key}="${escapeValue(line.value)}"` : line.text))
      .join('\n') + '\n';
  }

  const entryRows = $derived(
    lines
      .map((line, index) => ({ line, index }))
      .filter((row) => row.line.kind === 'entry')
  );

  const preservedLines = $derived(
    lines.filter((line) => line.kind === 'comment' || line.kind === 'raw')
  );

  const visibleRows = $derived.by(() => {
    const query = filterQuery.trim().toLowerCase();
    if (!query) return entryRows;
    return entryRows.filter(
      (row) =>
        row.line.key.toLowerCase().includes(query) ||
        row.line.value.toLowerCase().includes(query)
    );
  });

  const keyCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const row of entryRows) {
      const key = row.line.key.trim();
      if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  });

  const duplicateKeys = $derived(
    entryRows.filter((row) => (keyCounts.get(row.line.key.trim()) ?? 0) > 1)
  );

  const invalidRows = $derived(
    entryRows.filter((row) => !KEY_PATTERN.test(row.line.key.trim()))
  );

  const hasChanges = $derived(
    serializeDocument(lines) !== serializeDocument(originalLines)
  );

  const statusValue = $derived(
    invalidRows.length > 0 || duplicateKeys.length > 0
      ? 'Invalid'
      : hasChanges
        ? 'Unsaved'
        : 'Saved'
  );

  const canSave = $derived(
    hasChanges && invalidRows.length === 0 && duplicateKeys.length === 0
  );

  // ── Data ──────────────────────────────────────────────────────────────
  function copyLines(source: EnvLine[]): EnvLine[] {
    return source.map((line) => ({
      kind: line.kind,
      text: line.text,
      key: line.key,
      value: line.value
    }));
  }

  async function loadFile() {
    loading = true;
    statusStore.setBusy('Reading /etc/environment…');
    try {
      const file = await invoke<EnvFile>('read_env_file');
      filePath = file.path || '/etc/environment';
      lines = file.lines;
      originalLines = copyLines(file.lines);
      loadError = '';
      statusStore.setLastCommand('cat /etc/environment', 0, true);
    } catch (error) {
      loadError = String(error);
      uiStore.addToast(`Could not read /etc/environment: ${error}`, 'error');
      statusStore.setLastCommand('cat /etc/environment', 1, false);
    } finally {
      loading = false;
      loaded = true;
      statusStore.clearBusy();
    }
  }

  function requestReload() {
    if (!hasChanges) {
      loadFile();
      return;
    }
    uiStore.confirm(
      'Discard unsaved changes?',
      'You have unsaved changes to /etc/environment. Reloading will discard them.',
      () => loadFile(),
      true
    );
  }

  async function addVar() {
    lines = [...lines, { kind: 'entry', text: '', key: '', value: '' }];
    await tick();
    document.getElementById(`env-key-${lines.length - 1}`)?.focus();
  }

  function removeVar(index: number) {
    lines = lines.filter((_, i) => i !== index);
  }

  async function browseFile(index: number) {
    try {
      const selected = await open({ multiple: false });
      if (selected && typeof selected === 'string') {
        lines[index].value = selected;
      }
    } catch (error) {
      uiStore.addToast(`Could not open the file dialog: ${error}`, 'error');
    }
  }

  function confirmSave() {
    if (!canSave) return;
    showDiffModal = true;
  }

  async function doSave() {
    if (!canSave) return;
    saving = true;
    statusStore.setBusy('Saving /etc/environment…');
    try {
      await invoke('write_env_file', { lines: copyLines(lines) });
      statusStore.setLastCommand('pkexec … write /etc/environment', 0, true);
      uiStore.addToast('/etc/environment saved', 'success');
      showDiffModal = false;
      await loadFile();
    } catch (error) {
      const message = String(error);
      if (message.includes('Admin mode is off')) {
        uiStore.addToast('Turn on Admin mode in the status bar to save system files.', 'warning');
      } else {
        uiStore.addToast(`Could not save /etc/environment: ${message}`, 'error');
      }
      statusStore.setLastCommand('pkexec … write /etc/environment', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  onMount(() => {
    loadFile();
  });
</script>

<div class="module-page">
  <PageHeader
    title="Environment"
    subtitle="System-wide variables from /etc/environment"
    icon={FileText}
  >
    {#if invalidRows.length > 0 || duplicateKeys.length > 0}
      <Badge variant="error">Needs attention</Badge>
    {:else if hasChanges}
      <Badge variant="warning">Unsaved changes</Badge>
    {:else if loaded}
      <Badge variant="muted">Saved</Badge>
    {/if}
    <button class="header-refresh-btn" onclick={requestReload} disabled={loading || saving}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </button>
    <Button
      size="sm"
      variant="primary"
      onclick={confirmSave}
      disabled={loading || saving || !canSave}
      title={canSave ? 'Review and save changes' : 'No valid changes to save'}
    >
      <Save size={14} /> Save Changes
    </Button>
  </PageHeader>

  <div class="env-overview">
    <KpiCard
      icon={Variable}
      value={entryRows.length}
      label="Variables"
      subtext="editable"
    />
    <KpiCard
      icon={Layers}
      value={preservedLines.length}
      label="Preserved lines"
      subtext="comments & raw"
    />
    <KpiCard
      icon={statusValue === 'Saved' ? ShieldCheck : AlertTriangle}
      value={statusValue}
      label="File state"
      subtext={duplicateKeys.length > 0
        ? `${duplicateKeys.length} duplicate${duplicateKeys.length === 1 ? '' : 's'}`
        : invalidRows.length > 0
          ? `${invalidRows.length} invalid name${invalidRows.length === 1 ? '' : 's'}`
          : 'ready to write'}
    />
  </div>

  {#if !loaded && loading}
    <div class="env-card env-loading">
      <RefreshCw size={22} class="animate-spin-slow" />
      <span>Reading /etc/environment…</span>
    </div>
  {:else if loadError && lines.length === 0}
    <EmptyState
      icon={AlertTriangle}
      title="/etc/environment is unavailable"
      description={loadError}
      actionLabel="Retry"
      actionIcon={RefreshCw}
      onAction={loadFile}
    />
  {:else}
    {#if duplicateKeys.length > 0 || invalidRows.length > 0}
      <div class="env-notice" class:error={invalidRows.length > 0}>
        <AlertTriangle size={16} />
        <div>
          <strong>
            {invalidRows.length > 0
              ? 'Some variable names are not valid'
              : 'Duplicate variable names'}
          </strong>
          <p>
            {#if invalidRows.length > 0}
              Names must start with a letter or underscore and contain only letters, digits and
              underscores.
            {:else}
              Only the last value of a repeated name is kept at login — rename or remove the
              duplicate.
            {/if}
          </p>
        </div>
      </div>
    {/if}

    <section class="env-card">
      <div class="card-head">
        <span class="head-chip"><FileText size={15} /></span>
        <div class="head-copy">
          <h3>/etc/environment</h3>
          <p>
            {entryRows.length} {entryRows.length === 1 ? 'variable' : 'variables'}
            {#if preservedLines.length > 0}
              · {preservedLines.length} line{preservedLines.length === 1 ? '' : 's'} preserved
            {/if}
          </p>
        </div>
        <div class="head-tools">
          <SearchBar bind:value={filterQuery} placeholder="Filter variables…" style="width: 210px" />
          <Button size="sm" variant="outline" onclick={addVar}>
            <Plus size={13} /> Add variable
          </Button>
        </div>
      </div>

      {#if entryRows.length === 0}
        <div class="card-empty">
          <EmptyState
            icon={Variable}
            title="No environment variables"
            description="/etc/environment is empty. Add a variable to make it available to every user on the next login."
            actionLabel="Add variable"
            actionIcon={Plus}
            onAction={addVar}
          />
        </div>
      {:else if visibleRows.length === 0}
        <div class="card-empty">
          <EmptyState
            icon={Info}
            title="No variables match"
            description={`Nothing matched "${filterQuery}". Try a different name or value.`}
            actionLabel="Clear filter"
            onAction={() => (filterQuery = '')}
          />
        </div>
      {:else}
        <div class="env-table-wrap">
          <table>
            <thead>
              <tr>
                <th class="env-col-icon"></th>
                <th style="width: 250px">Variable</th>
                <th>Value</th>
                <th style="width: 52px"></th>
              </tr>
            </thead>
            <tbody>
              {#each visibleRows as row (row.index)}
                {@const key = row.line.key.trim()}
                {@const badKey = !KEY_PATTERN.test(key)}
                {@const duplicate = (keyCounts.get(key) ?? 0) > 1}
                <tr class:invalid={badKey || duplicate}>
                  <td class="env-col-icon">
                    <span class="env-row-chip"><Variable size={12} /></span>
                  </td>
                  <td>
                    <input
                      id={`env-key-${row.index}`}
                      class="input mono-input"
                      class:input-bad={badKey || duplicate}
                      bind:value={row.line.key}
                      placeholder="VARIABLE_NAME"
                      spellcheck="false"
                      autocomplete="off"
                      aria-invalid={badKey || duplicate}
                    />
                  </td>
                  <td>
                    <div class="env-value-group">
                      <input
                        class="input mono-input"
                        bind:value={row.line.value}
                        placeholder="value"
                        spellcheck="false"
                        autocomplete="off"
                      />
                      <button
                        type="button"
                        class="group-btn"
                        onclick={() => browseFile(row.index)}
                        title="Browse for a file or directory"
                        aria-label="Browse for a value"
                      >
                        <FolderOpen size={14} />
                      </button>
                    </div>
                  </td>
                  <td class="env-actions">
                    <button
                      type="button"
                      class="action-btn danger"
                      onclick={() => removeVar(row.index)}
                      title="Remove variable"
                      aria-label={`Remove ${key || 'variable'}`}
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

      {#if preservedLines.length > 0}
        <details class="preserved">
          <summary>
            <Layers size={13} />
            {preservedLines.length} preserved line{preservedLines.length === 1 ? '' : 's'} will be written back unchanged
          </summary>
          <div class="preserved-list">
            {#each preservedLines as line}
              <code>{line.text}</code>
            {/each}
          </div>
        </details>
      {/if}
    </section>
  {/if}

  <ConfigDiffModal
    bind:show={showDiffModal}
    filePath={filePath}
    title="Review /etc/environment changes"
    oldContent={serializeDocument(originalLines)}
    newContent={serializeDocument(lines)}
    warningMessage="Global environment changes take effect for all users on their next login or reboot."
    isSaving={saving}
    onconfirm={doSave}
    oncancel={() => (showDiffModal = false)}
  />
</div>

<style>
  .env-overview {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 8px;
    flex-shrink: 0;
  }

  .env-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex: 0 1 auto;
    min-height: 0;
  }

  .env-loading {
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 54px 24px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .env-notice {
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

  .env-notice.error {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .env-notice strong {
    display: block;
    font-size: 12px;
    margin-bottom: 2px;
  }

  .env-notice p {
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
    font-family: var(--font-mono);
    font-size: 12.5px;
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

  /* ── Table ───────────────────────────────────────────────────── */
  .env-table-wrap {
    overflow: auto;
    min-height: 0;
  }

  .env-table-wrap table {
    width: 100%;
    table-layout: fixed;
  }

  .env-table-wrap :global(tbody td) {
    padding: 7px 12px;
  }

  .env-table-wrap :global(tbody tr) {
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .env-table-wrap :global(tbody tr:last-child) {
    border-bottom: none;
  }

  .env-table-wrap :global(tbody tr.invalid) {
    background: color-mix(in srgb, var(--color-error) 5%, transparent);
  }

  .env-col-icon {
    width: 38px;
    padding-left: 14px !important;
    padding-right: 0 !important;
  }

  .env-row-chip {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
  }

  .mono-input {
    font-family: var(--font-mono);
    font-size: 12.5px;
    font-weight: 600;
    height: 32px;
  }

  .input.input-bad {
    border-color: var(--color-error);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-error) 35%, transparent);
  }

  .env-value-group {
    display: flex;
    align-items: stretch;
  }

  .env-value-group .input {
    flex: 1;
    min-width: 0;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }

  .group-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
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

  .env-actions {
    text-align: right;
    white-space: nowrap;
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

  .action-btn.danger:hover {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  /* ── Preserved lines ─────────────────────────────────────────── */
  .preserved {
    border-top: 1px solid var(--color-border-subtle);
  }

  .preserved summary {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    font-size: 10.5px;
    color: var(--color-text-muted);
    cursor: pointer;
    list-style: none;
    user-select: none;
  }

  .preserved summary::-webkit-details-marker {
    display: none;
  }

  .preserved summary:hover {
    color: var(--color-text-secondary);
    background: var(--color-bg-hover);
  }

  .preserved-list {
    max-height: 160px;
    overflow: auto;
    padding: 0 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .preserved-list code {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .card-empty {
    padding: 18px;
  }

  .card-empty :global(.empty-state-wrap),
  :global(html.light-mode) .card-empty :global(.empty-state-wrap) {
    border: none;
    background: transparent !important;
    padding: 32px 22px;
  }
</style>
