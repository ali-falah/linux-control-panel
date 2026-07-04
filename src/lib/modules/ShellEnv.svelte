<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import {
    Terminal, Variable, FolderOpen, Eye, RefreshCw, Plus, Trash2,
    Save, AlertTriangle, CheckCircle2, XCircle, ChevronDown, ChevronRight,
    FileCode, ArchiveRestore, GripVertical, Search, Folder
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

  // ─── Types ─────────────────────────────────────────────────────────────────

  interface ShellVar {
    name: string; value: string; raw_line: string;
    line_number: number; source_path: string; live_value: string | null;
  }
  interface ShellVarGroup {
    source_path: string; display_name: string; is_system: boolean; vars: ShellVar[];
  }
  interface ProfileFile {
    path: string; display_name: string; last_modified: string;
    writable: boolean; line_count: number; is_system: boolean;
  }
  interface PathEntry { directory: string; source_path: string | null; exists: boolean; }
  interface ShellBackup { backup_path: string; original_path: string; timestamp: string; filename: string; }
  interface LiveEnvVar {
    name: string; live_value: string; defined_value: string | null;
    in_sync: boolean; source_path: string | null;
  }

  // ─── State ─────────────────────────────────────────────────────────────────

  let activeTab = $state<'variables' | 'path' | 'files' | 'preview'>('variables');

  // Variables tab
  let varGroups = $state<ShellVarGroup[]>([]);
  let varsLoading = $state(false);
  let collapsedGroups = $state<Set<string>>(new Set());
  let editingVar = $state<{ v: ShellVar; newName: string; newValue: string } | null>(null);
  let addVarForm = $state<{ targetFile: string; name: string; value: string } | null>(null);
  let savingVar = $state(false);
  let filterVars = $state('');
  let showLiveValues = $state(false);
  let liveValuesMap = $state<Map<string, string>>(new Map());
  let loadingLive = $state(false);

  // PATH tab
  let pathEntries = $state<PathEntry[]>([]);
  let pathLoading = $state(false);
  let addPathForm = $state<{ directory: string; profile_path: string } | null>(null);
  let profileFiles = $state<ProfileFile[]>([]);

  // Files tab
  let filesLoading = $state(false);
  let selectedFile = $state<ProfileFile | null>(null);
  let fileContent = $state('');
  let savedFileContent = $state('');
  let fileEditorLoading = $state(false);
  let fileSaving = $state(false);
  let backups = $state<ShellBackup[]>([]);
  let backupsLoading = $state(false);
  let showBackupsFor = $state<string | null>(null);
  let newProfileDName = $state('');
  let showNewFileForm = $state(false);
  let wordWrap = $state(true);

  // Preview tab
  let liveEnv = $state<LiveEnvVar[]>([]);
  let previewLoading = $state(false);
  let filterLive = $state('');
  let showOnlyUnsynced = $state(false);

  // ─── Computed ─────────────────────────────────────────────────────────────

  const conflictMap = $derived(() => {
    const m = new Map<string, string[]>();
    for (const g of varGroups) {
      for (const v of g.vars) {
        const arr = m.get(v.name) ?? [];
        arr.push(g.source_path);
        m.set(v.name, arr);
      }
    }
    return m;
  });

  const filteredGroups = $derived(
    filterVars
      ? varGroups.map(g => ({
          ...g,
          vars: g.vars.filter(
            v =>
              v.name.toLowerCase().includes(filterVars.toLowerCase()) ||
              v.value.toLowerCase().includes(filterVars.toLowerCase()),
          ),
        })).filter(g => g.vars.length > 0)
      : varGroups
  );

  const filteredLiveEnv = $derived(
    liveEnv.filter(v => {
      if (showOnlyUnsynced && v.in_sync) return false;
      if (filterLive) {
        return (
          v.name.toLowerCase().includes(filterLive.toLowerCase()) ||
          v.live_value.toLowerCase().includes(filterLive.toLowerCase())
        );
      }
      return true;
    })
  );

  const hasFileChanges = $derived(fileContent !== savedFileContent);

  // ─── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    loadVarGroups();
    loadProfileFiles();
  });

  $effect(() => {
    if (activeTab === 'path' && pathEntries.length === 0) loadPathEntries();
  });

  // ─── Variables tab ─────────────────────────────────────────────────────────

  async function loadVarGroups() {
    varsLoading = true;
    statusStore.setBusy('Parsing profile files…');
    try {
      varGroups = await invoke<ShellVarGroup[]>('shell_parse_all_exports');
      statusStore.setLastCommand('shell_parse_all_exports', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to parse profiles: ${e}`, 'error');
      statusStore.setLastCommand('shell_parse_all_exports', 1, false);
    } finally {
      varsLoading = false;
      statusStore.clearBusy();
    }
  }

  async function loadLiveValues() {
    loadingLive = true;
    try {
      const names = varGroups.flatMap(g => g.vars.map(v => v.name));
      const unique = [...new Set(names)];
      const map = new Map<string, string>();
      await Promise.all(
        unique.map(async name => {
          try {
            const val = await invoke<string>('shell_get_live_value', { name });
            map.set(name, val);
          } catch {}
        }),
      );
      liveValuesMap = map;
    } finally {
      loadingLive = false;
    }
  }

  function toggleGroupCollapse(path: string) {
    const next = new Set(collapsedGroups);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    collapsedGroups = next;
  }

  function startAddVar(targetFile: string) {
    addVarForm = { targetFile, name: '', value: '' };
  }

  function startEditVar(v: ShellVar) {
    editingVar = { v, newName: v.name, newValue: v.value };
  }

  async function saveVar() {
    if (!addVarForm) return;
    if (!addVarForm.name.trim() || !addVarForm.targetFile) {
      uiStore.addToast('Name and target file are required', 'warning');
      return;
    }
    savingVar = true;
    statusStore.setBusy(`Writing export to ${addVarForm.targetFile}…`);
    try {
      await invoke('shell_write_var', {
        path: addVarForm.targetFile,
        name: addVarForm.name.trim(),
        value: addVarForm.value,
        oldLine: null,
      });
      uiStore.addToast(`Added export ${addVarForm.name} ✓`, 'success');
      statusStore.setLastCommand(`shell_write_var ${addVarForm.name}`, 0, true);
      addVarForm = null;
      await loadVarGroups();
    } catch (e) {
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      statusStore.setLastCommand('shell_write_var', 1, false);
    } finally {
      savingVar = false;
      statusStore.clearBusy();
    }
  }

  async function saveEditVar() {
    if (!editingVar) return;
    savingVar = true;
    statusStore.setBusy('Rewriting export line…');
    try {
      await invoke('shell_write_var', {
        path: editingVar.v.source_path,
        name: editingVar.newName.trim(),
        value: editingVar.newValue,
        oldLine: editingVar.v.raw_line,
      });
      uiStore.addToast(`Updated ${editingVar.newName} ✓`, 'success');
      statusStore.setLastCommand(`shell_write_var ${editingVar.newName}`, 0, true);
      editingVar = null;
      await loadVarGroups();
    } catch (e) {
      uiStore.addToast(`Failed to save: ${e}`, 'error');
      statusStore.setLastCommand('shell_write_var', 1, false);
    } finally {
      savingVar = false;
      statusStore.clearBusy();
    }
  }

  function confirmDeleteVar(v: ShellVar) {
    uiStore.confirm(
      'Delete Variable',
      `Remove "export ${v.name}" from ${v.source_path}?\n\nThis will delete the line:\n${v.raw_line}`,
      async () => {
        try {
          await invoke('shell_delete_var', { path: v.source_path, rawLine: v.raw_line });
          uiStore.addToast(`Deleted ${v.name} ✓`, 'success');
          statusStore.setLastCommand(`shell_delete_var ${v.name}`, 0, true);
          await loadVarGroups();
        } catch (e) {
          uiStore.addToast(`Delete failed: ${e}`, 'error');
        }
      },
      true,
    );
  }

  // ─── PATH tab ──────────────────────────────────────────────────────────────

  async function loadPathEntries() {
    pathLoading = true;
    statusStore.setBusy('Parsing $PATH…');
    try {
      pathEntries = await invoke<PathEntry[]>('shell_parse_path');
      statusStore.setLastCommand('shell_parse_path', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to parse PATH: ${e}`, 'error');
    } finally {
      pathLoading = false;
      statusStore.clearBusy();
    }
  }

  async function addPathEntry() {
    if (!addPathForm?.directory.trim() || !addPathForm?.profile_path) {
      uiStore.addToast('Directory and target profile are required', 'warning');
      return;
    }
    try {
      await invoke('shell_add_path_entry', {
        directory: addPathForm.directory.trim(),
        profilePath: addPathForm.profile_path,
      });
      uiStore.addToast(`Added ${addPathForm.directory} to PATH ✓`, 'success');
      statusStore.setLastCommand('shell_add_path_entry', 0, true);
      addPathForm = null;
      await loadPathEntries();
    } catch (e) {
      uiStore.addToast(`Failed to add PATH entry: ${e}`, 'error');
    }
  }

  function confirmRemovePath(entry: PathEntry) {
    if (!entry.source_path) {
      uiStore.addToast('Cannot remove: source file unknown', 'warning');
      return;
    }
    uiStore.confirm(
      'Remove PATH Entry',
      `Remove "${entry.directory}" from $PATH?\nThis will remove the matching line from ${entry.source_path}.`,
      async () => {
        try {
          await invoke('shell_remove_path_entry', {
            directory: entry.directory,
            profilePath: entry.source_path,
          });
          uiStore.addToast(`Removed ${entry.directory} from PATH ✓`, 'success');
          await loadPathEntries();
        } catch (e) {
          uiStore.addToast(`Remove failed: ${e}`, 'error');
        }
      },
      true,
    );
  }

  // ─── Files tab ─────────────────────────────────────────────────────────────

  async function loadProfileFiles() {
    filesLoading = true;
    try {
      profileFiles = await invoke<ProfileFile[]>('shell_list_profile_files');
    } catch (e) {
      uiStore.addToast(`Failed to list profile files: ${e}`, 'error');
    } finally {
      filesLoading = false;
    }
  }

  async function openFile(f: ProfileFile) {
    selectedFile = f;
    fileEditorLoading = true;
    try {
      fileContent = await invoke<string>('shell_read_profile_file', { path: f.path });
      savedFileContent = fileContent;
    } catch (e) {
      uiStore.addToast(`Cannot read ${f.path}: ${e}`, 'error');
    } finally {
      fileEditorLoading = false;
    }
  }

  async function saveFile() {
    if (!selectedFile) return;
    fileSaving = true;
    statusStore.setBusy(`Saving ${selectedFile.display_name}…`);
    try {
      await invoke('shell_write_profile_file', { path: selectedFile.path, content: fileContent });
      uiStore.addToast(`Saved ${selectedFile.display_name} ✓`, 'success');
      statusStore.setLastCommand(`shell_write_profile_file`, 0, true);
      savedFileContent = fileContent;
    } catch (e) {
      uiStore.addToast(`Save failed: ${e}`, 'error');
      statusStore.setLastCommand('shell_write_profile_file', 1, false);
    } finally {
      fileSaving = false;
      statusStore.clearBusy();
    }
  }

  async function loadBackups() {
    backupsLoading = true;
    try {
      backups = await invoke<ShellBackup[]>('shell_list_backups');
    } catch (e) {
      uiStore.addToast(`Failed to load backups: ${e}`, 'error');
    } finally {
      backupsLoading = false;
    }
  }

  function confirmRestoreBackup(bk: ShellBackup) {
    uiStore.confirm(
      'Restore Backup',
      `Restore "${bk.filename}" to "${bk.original_path}"?\nCurrent file will be backed up first.`,
      async () => {
        try {
          await invoke('shell_restore_backup', { backupPath: bk.backup_path, originalPath: bk.original_path });
          uiStore.addToast('Backup restored ✓', 'success');
          if (selectedFile?.path === bk.original_path) {
            fileContent = await invoke<string>('shell_read_profile_file', { path: bk.original_path });
            savedFileContent = fileContent;
          }
        } catch (e) {
          uiStore.addToast(`Restore failed: ${e}`, 'error');
        }
      },
    );
  }

  async function createProfileDFile() {
    if (!newProfileDName.trim()) {
      uiStore.addToast('Enter a file name', 'warning');
      return;
    }
    try {
      const path = await invoke<string>('shell_create_profile_d_file', { name: newProfileDName });
      uiStore.addToast(`Created ${path} ✓`, 'success');
      showNewFileForm = false;
      newProfileDName = '';
      await loadProfileFiles();
    } catch (e) {
      uiStore.addToast(`Create failed: ${e}`, 'error');
    }
  }

  // ─── Preview tab ───────────────────────────────────────────────────────────

  async function loadLiveEnv() {
    previewLoading = true;
    statusStore.setBusy('Running bash -l -c env…');
    try {
      liveEnv = await invoke<LiveEnvVar[]>('shell_get_live_env');
      statusStore.setLastCommand('bash -l -c env', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to get live env: ${e}`, 'error');
      statusStore.setLastCommand('bash -l -c env', 1, false);
    } finally {
      previewLoading = false;
      statusStore.clearBusy();
    }
  }

  async function sourceFile(f: ProfileFile) {
    statusStore.setBusy(`source ${f.path}…`);
    try {
      const vars = await invoke<LiveEnvVar[]>('shell_source_file', { path: f.path });
      // merge into liveEnv
      const map = new Map(liveEnv.map(v => [v.name, v]));
      for (const v of vars) map.set(v.name, v);
      liveEnv = [...map.values()];
      uiStore.addToast(`Sourced ${f.display_name} ✓`, 'success');
      statusStore.setLastCommand(`source ${f.path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Source failed: ${e}`, 'error');
      statusStore.setLastCommand(`source ${f.path}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  // ─── Helpers ───────────────────────────────────────────────────────────────

  function shortPath(p: string): string {
    const home = p.match(/^\/home\/[^/]+/) ?? [];
    return p.replace(home[0] ?? '', '~');
  }
</script>

<!-- ─── Page ────────────────────────────────────────────────────────────── -->
<div class="module-page">
  <!-- Header -->
  <PageHeader title="Shell Environment" subtitle="Manage bash profile files, exported variables, and PATH" icon={Terminal} />

  <!-- Tab Bar -->
  <div class="tab-bar">
    <button class="tab-btn" class:active={activeTab === 'variables'} onclick={() => (activeTab = 'variables')}>
      <Variable size={14} /> Variables
    </button>
    <button class="tab-btn" class:active={activeTab === 'path'} onclick={() => (activeTab = 'path')}>
      <FolderOpen size={14} /> PATH Manager
    </button>
    <button class="tab-btn" class:active={activeTab === 'files'} onclick={() => (activeTab = 'files')}>
      <FileCode size={14} /> Profile Files
    </button>
    <button class="tab-btn" class:active={activeTab === 'preview'} onclick={() => (activeTab = 'preview')}>
      <Eye size={14} /> Live Preview
    </button>
  </div>

  <div class="tab-content module-content-scroll">

    <!-- ══ VARIABLES ══════════════════════════════════════════════════════ -->
    {#if activeTab === 'variables'}
      <div class="tab-section" style="padding: 0;">
        <div class="custom-toolbar">
          <div class="search-bar" style="flex:1; margin:0">
            <Search size={16} />
            <input bind:value={filterVars} placeholder="Filter variables..." />
            {#if filterVars}<Button class="btn btn-sm -ghost" style="padding:2px; height:24px" onclick={() => filterVars = ''}>✕</Button>{/if}
          </div>
          <div class="toolbar-actions">
            <button class="toolbar-btn" onclick={loadVarGroups} disabled={varsLoading}>
              <RefreshCw size={14} class={varsLoading ? 'animate-spin-slow' : ''} /> Refresh
            </button>
            <button class="toolbar-btn" onclick={() => { showLiveValues = !showLiveValues; if(showLiveValues) loadLiveValues(); }}>
              <Eye size={14} /> Live values
            </button>
            <button class="toolbar-btn primary-btn" onclick={() => startAddVar(profileFiles[0]?.path || '')}>
              <Plus size={14} /> Add Variable
            </button>
          </div>
        </div>

        {#if addVarForm}
          <div class="card add-var-form" style="margin-bottom:24px">
            <div class="form-title">Add exported variable</div>
            <div class="add-var-grid">
              <label class="form-field">
                <span>Variable name</span>
                <input type="text" bind:value={addVarForm.name} placeholder="JAVA_HOME" class="mono-input" id="shell-new-var-name" />
              </label>
              <label class="form-field">
                <span>Value</span>
                <input type="text" bind:value={addVarForm.value} placeholder="/usr/lib/jvm/java-21" class="mono-input" id="shell-new-var-value" />
              </label>
              <label class="form-field form-full">
                <span>Target profile file</span>
                <select bind:value={addVarForm.targetFile} class="form-select" id="shell-new-var-target">
                  {#each profileFiles as f}
                    <option value={f.path}>{f.display_name}</option>
                  {/each}
                </select>
              </label>
            </div>
            {#if addVarForm.name}
              <div class="preview-line">Preview: <code>export {addVarForm.name}="{addVarForm.value}"</code></div>
            {/if}
            <div class="form-actions">
              <Button variant="ghost" class="" onclick={() => (addVarForm = null)}>Cancel</Button>
              <Button variant="primary" class="" onclick={saveVar} disabled={savingVar} id="shell-save-new-var">
                {#if savingVar}<div class="spinner-sm"></div>{/if} <Save size={13} /> Save
              </Button>
            </div>
          </div>
        {/if}

        {#if varsLoading}
          <div class="center-state"><div class="spinner"></div> Loading...</div>
        {:else if filteredGroups.length === 0}
          <div class="empty-state">No exported variables found in profile files</div>
        {:else}
          <div class="shell-groups-list">
            {#each filteredGroups as group}
              <div class="shell-group-card">
                <div class="sg-header" tabindex="0" role="button" onclick={() => toggleGroupCollapse(group.source_path)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleGroupCollapse(group.source_path); } }} id={`shell-group-${group.source_path}`}>
                  {#if collapsedGroups.has(group.source_path)}<ChevronRight size={16} />{:else}<ChevronDown size={16} />{/if}
                  <span class="sg-name">{group.display_name}</span>
                  <div class="sg-badges">
                    {#if group.is_system}
                      <span class="user-badge" style="color:var(--color-warning)">system</span>
                    {:else}
                      <span class="user-badge">user</span>
                    {/if}
                    <span class="var-count">{group.vars.length} var{group.vars.length !== 1 ? 's' : ''}</span>
                  </div>
                </div>

                {#if !collapsedGroups.has(group.source_path)}
                  <div class="sg-content">
                    <div class="sg-table-header">
                      <div class="col-name">NAME</div>
                      <div class="col-value">DEFINED VALUE</div>
                      {#if showLiveValues}<div class="col-value">LIVE VALUE</div>{/if}
                      <div class="col-meta">METADATA</div>
                      <div class="col-actions">ACTIONS</div>
                    </div>
                    
                    {#each group.vars as v}
                      {@const conflicts = conflictMap().get(v.name) ?? []}
                      {@const liveVal = liveValuesMap.get(v.name)}
                      {@const outOfSync = showLiveValues && liveVal !== undefined && liveVal !== v.value}
                      
                      <div class="sg-row">
                        <div class="col-name">
                          {#if editingVar?.v === v}
                            <input class="sg-input" bind:value={editingVar.newName} />
                          {:else}
                            {v.name}
                            {#if conflicts.length > 1}
                              <span class="badge badge-warning conflict-badge" title="Defined in: {conflicts.join(', ')}">⚠</span>
                            {/if}
                          {/if}
                        </div>
                        <div class="col-value">
                          {#if editingVar?.v === v}
                            <input class="sg-input" bind:value={editingVar.newValue} />
                          {:else}
                            <input class="sg-input" value={v.value || ''} readonly />
                          {/if}
                        </div>
                        {#if showLiveValues}
                          <div class="col-value">
                            {#if liveVal !== undefined}
                              <input class="sg-input" value={liveVal || ''} readonly style="color: {outOfSync ? 'var(--color-warning)' : 'var(--color-success)'}" />
                            {:else}
                              <span class="text-muted">—</span>
                            {/if}
                          </div>
                        {/if}
                        <div class="col-meta">
                          {#if editingVar?.v !== v}
                            <span class="src-badge">src: {group.display_name}:{v.line_number}</span>
                          {/if}
                        </div>
                        <div class="col-actions">
                          {#if editingVar?.v === v}
                            <button class="icon-btn" style="color:var(--color-accent)" onclick={saveEditVar} disabled={savingVar}>
                              {#if savingVar}<div class="spinner-sm"></div>{:else}<Save size={14} />{/if}
                            </button>
                            <button class="icon-btn" onclick={() => (editingVar = null)}><XCircle size={14} /></button>
                          {:else}
                            <button class="icon-btn" onclick={() => startEditVar(v)} title="Edit">
                              <!-- Inline SVG for Edit icon if Lucide's Edit2 is missing, but I can just use a text pencil or keep the custom layout -->
                              ✎
                            </button>
                            <button class="icon-btn" onclick={() => confirmDeleteVar(v)} title="Delete"><Trash2 size={14}/></button>
                          {/if}
                        </div>
                      </div>
                    {/each}

                    <div class="sg-add-row" onclick={() => startAddVar(group.source_path)}>
                      <Plus size={14} /> Add another variable to {group.display_name}
                    </div>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    <!-- ══ PATH MANAGER ════════════════════════════════════════════════════ -->
    {:else if activeTab === 'path'}
      <div class="tab-section" style="display:flex; flex-direction:column; flex:1; min-height:0; padding: 0;">
        <div class="custom-toolbar">
          <div style="display:flex; align-items:center;">
            <h3 style="margin:0; font-size:15px; font-weight:600; color:var(--color-text-primary);">$PATH Entries</h3>
          </div>
          <div class="row-actions">
            <Button variant="ghost" class=" btn-sm" onclick={loadPathEntries} id="shell-reload-path">
              <RefreshCw size={13} /> Refresh
            </Button>
            <Button variant="primary" class=" btn-sm" onclick={() => (addPathForm = { directory: '', profile_path: profileFiles[0]?.path ?? '' })} id="shell-add-path">
              <Plus size={13} /> Add Entry
            </Button>
          </div>
        </div>
        <div style="display:flex; flex-direction:column; padding: 24px; gap: 16px; flex: 1; min-height:0;">
        {#if addPathForm}
          <div class="card add-var-form">
            <div class="form-title">Add PATH entry</div>
            <div class="add-var-grid">
              <label class="form-field">
                <span>Directory</span>
                <input type="text" bind:value={addPathForm.directory} placeholder="/usr/local/go/bin" class="mono-input" id="shell-path-dir" />
              </label>
              <label class="form-field">
                <span>Append to</span>
                <select bind:value={addPathForm.profile_path} class="form-select" id="shell-path-target">
                  {#each profileFiles as f}
                    <option value={f.path}>{f.display_name}</option>
                  {/each}
                </select>
              </label>
            </div>
            {#if addPathForm.directory}
              <div class="preview-line">
                Preview: <code>export PATH="$PATH:{addPathForm.directory}"</code>
              </div>
            {/if}
            <div class="form-actions">
              <Button variant="ghost" class="" onclick={() => (addPathForm = null)}>Cancel</Button>
              <Button variant="primary" class="" onclick={addPathEntry} id="shell-save-path">
                <Save size={13} /> Save
              </Button>
            </div>
          </div>
        {/if}

        {#if pathLoading}
          <div class="center-state"><div class="spinner"></div></div>
        {:else if pathEntries.length === 0}
          <div class="empty-state">$PATH is empty or could not be parsed</div>
        {:else}
          <div class="table-wrap module-content-scroll" style="margin:0; border:none; border-radius:12px; flex:1; min-height:0; border: 1px solid var(--color-border);">
            <table use:tableFeatures>
              <thead>
                <tr>
                  <th>#</th>
                  <th>Directory</th>
                  <th>Source file</th>
                  <th style="text-align:center">Exists</th>
                  <th style="text-align:right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each pathEntries as entry, i}
                  <tr class:path-missing={!entry.exists}>
                    <td class="path-index">{i + 1}</td>
                    <td>
                      <div class="path-dir-row">
                        <Folder size={13} style="flex-shrink:0;color:var(--color-text-muted)" />
                        <code class="path-dir {!entry.exists ? 'text-error' : ''}">{entry.directory}</code>
                        {#if !entry.exists}
                          <AlertTriangle size={12} style="color:var(--color-error);flex-shrink:0" />
                        {/if}
                      </div>
                    </td>
                    <td>
                      {#if entry.source_path}
                        <span class="source-badge">{shortPath(entry.source_path)}</span>
                      {:else}
                        <span class="text-muted">—</span>
                      {/if}
                    </td>
                    <td style="text-align:center">
                      {#if entry.exists}
                        <CheckCircle2 size={14} style="color:var(--color-success)" />
                      {:else}
                        <XCircle size={14} style="color:var(--color-error)" />
                      {/if}
                    </td>
                    <td style="text-align:right">
                      {#if entry.source_path}
                        <Button class="btn btn-sm -danger" onclick={() => confirmRemovePath(entry)} id={`shell-rm-path-${i}`}>
                          <Trash2 size={11} /> Remove
                        </Button>
                      {:else}
                        <span class="text-muted text-xs">system</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <div class="path-legend">
            <XCircle size={12} style="color:var(--color-error)" /> Red = directory does not exist on disk
          </div>
          {/if}
        </div><!-- module-content-scroll -->
      </div><!-- tab-section -->

    <!-- ══ PROFILE FILES ══════════════════════════════════════════════════ -->
    {:else if activeTab === 'files'}
      <div class="files-layout">
        <!-- File list -->
        <div class="files-sidebar">
          <div class="files-sidebar-header">
            <span>Profile Files</span>
            <div class="row-actions">
              <Button variant="ghost" class=" btn-sm" onclick={loadProfileFiles} id="shell-reload-files"><RefreshCw size={12} /></Button>
              <Button variant="outline" class=" btn-sm" onclick={() => (showNewFileForm = !showNewFileForm)} id="shell-new-profile-d">
                <Plus size={12} />
              </Button>
            </div>
          </div>

          {#if showNewFileForm}
            <div class="new-file-form">
              <input type="text" bind:value={newProfileDName} placeholder="custom-vars.sh" id="shell-new-file-name" />
              <Button variant="primary" class=" btn-sm" onclick={createProfileDFile} id="shell-create-file">Create</Button>
              <Button variant="ghost" class=" btn-sm" onclick={() => (showNewFileForm = false)}>✕</Button>
            </div>
          {/if}

          {#if filesLoading}
            <div class="center-state"><div class="spinner-sm"></div></div>
          {:else}
            {#each [false, true] as isSystem}
              {@const group = profileFiles.filter(f => f.is_system === isSystem)}
              {#if group.length > 0}
                <div class="file-group-label">{isSystem ? '/etc/profile.d/' : 'User files'}</div>
                {#each group as f}
                  <button
                    class="file-item"
                    class:selected={selectedFile?.path === f.path}
                    onclick={() => openFile(f)}
                    id={`shell-file-${f.path}`}
                  >
                    <FileCode size={12} />
                    <span class="file-item-name">{f.display_name}</span>
                    <span class="file-item-lines">{f.line_count}L</span>
                  </button>
                {/each}
              {/if}
            {/each}
          {/if}

          <div class="files-sidebar-sep"></div>
          <Button variant="outline" class=" btn-sm sidebar-backup-btn" onclick={() => { showBackupsFor = 'all'; loadBackups(); }} id="shell-show-backups">
            <ArchiveRestore size={12} /> Backups
          </Button>

          {#if showBackupsFor}
            <div class="backup-list">
              <div class="backup-list-header">
                <span>Backups</span>
                <Button variant="ghost" class=" btn-sm" onclick={() => (showBackupsFor = null)}>✕</Button>
              </div>
              {#if backupsLoading}
                <div class="center-state"><div class="spinner-sm"></div></div>
              {:else if backups.length === 0}
                <p class="empty-state" style="font-size:12px">No backups</p>
              {:else}
                {#each backups as bk}
                  <div class="backup-item">
                    <div class="backup-name">{bk.filename}</div>
                    <div class="backup-ts">{bk.timestamp}</div>
                    <Button class="btn btn-sm -outline" onclick={() => confirmRestoreBackup(bk)} id={`shell-restore-${bk.filename}`}>
                      <ArchiveRestore size={11} /> Restore
                    </Button>
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>

        <!-- Editor panel -->
        <div class="files-editor">
          {#if selectedFile}
            <div class="editor-toolbar">
              <span class="editor-filename">
                <FileCode size={14} />{selectedFile.display_name}
                {#if selectedFile.is_system}
                  <span class="badge badge-warning" style="font-size:10px">pkexec</span>
                {/if}
              </span>
              <div class="editor-tools">
                <span class="text-muted text-xs">{selectedFile.last_modified}</span>
                <Button variant="ghost" class=" btn-sm" onclick={() => (wordWrap = !wordWrap)} id="shell-word-wrap">
                  Wrap: {wordWrap ? 'On' : 'Off'}
                </Button>
                <Button
                  variant="primary" class=" btn-sm"
                  onclick={saveFile}
                  disabled={fileSaving || !hasFileChanges}
                  id="shell-save-file"
                >
                  {#if fileSaving}<div class="spinner-sm"></div>{:else}<Save size={12} />{/if}
                  Save
                </Button>
              </div>
            </div>
            {#if hasFileChanges}
              <div class="unsaved-warning">
                <AlertTriangle size={13} /> Unsaved changes — backup will be created automatically before writing
              </div>
            {/if}
            {#if fileEditorLoading}
              <div class="center-state"><div class="spinner"></div></div>
            {:else}
              <textarea
                class="code-editor"
                class:wrap={wordWrap}
                bind:value={fileContent}
                spellcheck={false}
                id="shell-editor-textarea"
              ></textarea>
            {/if}
          {:else}
            <div class="editor-empty">
              <FileCode size={40} />
              <p>Select a profile file to edit</p>
            </div>
          {/if}
        </div>
      </div>

    <!-- ══ LIVE PREVIEW ════════════════════════════════════════════════════ -->
    {:else if activeTab === 'preview'}
      <div class="tab-section">
        <div class="section-header">
          <h3>Resolved Environment</h3>
          <div class="row-actions">
            <Button variant="primary" class=" btn-sm" onclick={loadLiveEnv} disabled={previewLoading} id="shell-load-env">
              {#if previewLoading}<div class="spinner-sm"></div>{:else}<RefreshCw size={13} />{/if}
              Preview env
            </Button>
          </div>
        </div>

        {#if liveEnv.length > 0}
          <div class="preview-toolbar">
            <div class="search-bar" style="flex:1; margin:0">
              <Search size={13} style="color:var(--color-text-muted)" />
              <input bind:value={filterLive} placeholder="Filter variables…" id="shell-live-filter" />
            </div>
            
            <label class="toggle-label-row" style="background:rgba(255,255,255,0.03); padding:8px 12px; border-radius:8px; border:1px solid var(--color-border);">
              <button
                class="ui-toggle"
                class:on={showOnlyUnsynced}
                onclick={() => showOnlyUnsynced = !showOnlyUnsynced}
                role="switch"
                aria-checked={showOnlyUnsynced}
                type="button"
                id="shell-unsynced-only"
                aria-label="Toggle show only out-of-sync"
                style="transform: scale(0.8); margin-right:4px;"
              >
                <span class="ui-toggle-thumb"></span>
              </button>
              Show out-of-sync only
            </label>
            <span class="text-muted text-xs" style="background:rgba(255,255,255,0.03); padding:8px 12px; border-radius:8px; border:1px solid var(--color-border);">{filteredLiveEnv.length} / {liveEnv.length} vars</span>
          </div>

          <!-- Source buttons -->
          <div class="source-buttons">
            <span class="text-muted text-xs" style="padding:4px 0">Source a file to refresh:</span>
            {#each profileFiles.filter(f => !f.is_system) as f}
              <Button variant="outline" class=" btn-sm" onclick={() => sourceFile(f)} id={`shell-source-${f.path}`}>
                source {f.display_name}
              </Button>
            {/each}
          </div>

          <!-- Diff table -->
          <div class="table-wrap">
            <table use:tableFeatures>
              <thead>
                <tr>
                  <th>Variable</th>
                  <th>Live value (bash -l)</th>
                  <th>Defined value (file)</th>
                  <th style="text-align:center">In sync</th>
                  <th>Source</th>
                </tr>
              </thead>
              <tbody>
                {#each filteredLiveEnv as v}
                  <tr class:row-unsynced={!v.in_sync}>
                    <td><code class="var-name">{v.name}</code></td>
                    <td><span class="var-value">{v.live_value || '(empty)'}</span></td>
                    <td>
                      {#if v.defined_value !== null}
                        <span class="var-value {!v.in_sync ? 'text-warn' : ''}">{v.defined_value}</span>
                      {:else}
                        <span class="text-muted">not in files</span>
                      {/if}
                    </td>
                    <td style="text-align:center">
                      {#if v.defined_value === null}
                        <span class="text-muted">—</span>
                      {:else if v.in_sync}
                        <CheckCircle2 size={14} style="color:var(--color-success)" />
                      {:else}
                        <AlertTriangle size={14} style="color:var(--color-warning)" />
                      {/if}
                    </td>
                    <td>
                      {#if v.source_path}
                        <span class="source-badge">{shortPath(v.source_path)}</span>
                      {:else}
                        <span class="text-muted">—</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else if !previewLoading}
          <div class="empty-state">
            <Eye size={40} style="color:var(--color-text-muted)" />
            <p>Click "Preview env" to run bash -l -c env and see your resolved environment</p>
          </div>
        {:else}
          <div class="center-state"><div class="spinner"></div> Running bash login shell…</div>
        {/if}
      </div>
    {/if}

  </div><!-- end tab-content -->
</div>

<style>
  /* ─── Tab bar ─────────────────────────────────────────────────────── */
  
  :global(.tab-btn) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
    margin-bottom: -1px;
  }
  :global(.tab-btn:hover) { color: var(--color-text-primary); }
  :global(.tab-btn.active) { color: var(--color-accent-soft); border-bottom-color: var(--color-accent); }

  .tab-content { flex: 1; overflow-y: auto; }
  .tab-section { padding: 24px; display: flex; flex-direction: column; gap: 16px; }

  /* ─── Spinners ────────────────────────────────────────────────────── */
  .spinner {
    width: 22px; height: 22px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner-sm {
    width: 13px; height: 13px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }
  .center-state {
    display: flex; align-items: center; justify-content: center;
    gap: 10px; padding: 48px; color: var(--color-text-muted);
  }
  .empty-state {
    padding: 40px; text-align: center; color: var(--color-text-muted);
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    font-size: 13px;
  }

  /* ─── Section header ─────────────────────────────────────────────── */
  .section-header {
    display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  }
  .section-header h3 { margin: 0; font-size: 15px; font-weight: 600; }
  .row-actions { display: flex; gap: 6px; }
  .justify-end { justify-content: flex-end; }

  /* ─── Variable groups ────────────────────────────────────────────── */
  .var-group { display: flex; flex-direction: column; }
  :global(.group-add-btn) { flex-shrink: 0; padding: 4px 8px; }
  .group-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: rgba(0,0,0,0.4);
    border: 1px solid var(--color-border);
    border-radius: 10px 10px 0 0;
    cursor: pointer;
    border-bottom: none;
    font-size: 13px;
    font-family: var(--font-sans);
    color: var(--color-text-primary);
    font-weight: 600;
    transition: background 0.15s;
    text-align: left;
  }
  .group-header:hover { background: rgba(0,0,0,0.6); }
  .group-name { flex: 1; font-family: var(--font-mono); font-size: 12px; }
  .group-count { font-size: 11px; color: var(--color-text-muted); margin-right: 4px; }
  :global(.ml-auto) { margin-left: auto; }

  /* ─── Variable table cells ───────────────────────────────────────── */
  .var-name {
    font-family: var(--font-mono); font-size: 12px;
    color: var(--color-accent-soft); font-weight: 600;
  }
  .var-value {
    font-family: var(--font-mono); font-size: 12px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }
  .source-badge {
    display: inline-block;
    font-size: 10px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    padding: 1px 5px;
    background: rgba(255,255,255,0.04);
    border-radius: 4px;
    margin-left: 6px;
  }
  .conflict-badge { margin-left: 6px; }
  .out-of-sync { background: rgba(245,158,11,0.05); }
  .row-unsynced { background: rgba(245,158,11,0.06); }
  .text-warn { color: var(--color-warning) !important; }
  .text-ok { color: var(--color-success) !important; }
  .text-error { color: var(--color-error) !important; }
  .text-muted { color: var(--color-text-muted) !important; }
  .text-xs { font-size: 11px; }
  :global(.icon-ok) { color: var(--color-success); flex-shrink: 0; }

  .inline-edit {
    padding: 4px 8px;
    font-family: var(--font-mono);
    font-size: 12px;
    width: 100%;
  }
  .mono-input { font-family: var(--font-mono); font-size: 13px; }

  /* ─── Add var form ───────────────────────────────────────────────── */
  .add-var-form { margin-bottom: 8px; }
  .form-title { font-size: 13px; font-weight: 600; margin-bottom: 12px; color: var(--color-text-primary); }
  .add-var-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 12px;
  }
  .form-field { display: flex; flex-direction: column; gap: 5px; font-size: 12px; color: var(--color-text-secondary); }
  .form-field.form-full { grid-column: 1 / -1; }
  .form-select {
    padding: 8px 12px;
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: 12px;
    outline: none;
    color-scheme: dark;
  }
  .form-select option { background: #0f0f18; color: var(--color-text-primary); }
  .form-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  .preview-line {
    padding: 8px 12px;
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font-size: 12px;
    color: var(--color-text-muted);
    margin-top: 4px;
  }
  .preview-line code { color: var(--color-accent-soft); font-family: var(--font-mono); }

  /* ─── PATH tab ───────────────────────────────────────────────────── */
  .path-index { color: var(--color-text-muted); font-size: 11px; width: 30px; }
  .path-dir-row { display: flex; align-items: center; gap: 8px; }
  .path-dir { font-family: var(--font-mono); font-size: 12px; color: var(--color-text-primary); }
  .path-missing td { opacity: 0.75; }
  .path-legend { font-size: 11px; color: var(--color-text-muted); display: flex; align-items: center; gap: 5px; }

  /* ─── Files tab ──────────────────────────────────────────────────── */
  .files-layout { display: flex; height: 100%; overflow: hidden; }
  .files-sidebar {
    width: 240px; min-width: 240px;
    border-right: 1px solid var(--color-border);
    display: flex; flex-direction: column;
    overflow-y: auto; padding: 8px 0;
  }
  .files-sidebar-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 4px 12px 8px;
    font-size: 11px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 4px;
  }
  .files-sidebar-sep { border-top: 1px solid var(--color-border); margin: 8px 12px; }
  :global(.sidebar-backup-btn) { margin: 0 12px 8px; }
  .new-file-form {
    display: flex; gap: 6px; padding: 8px 12px; align-items: center;
    border-bottom: 1px solid var(--color-border);
  }
  .new-file-form input { flex: 1; padding: 5px 8px; font-size: 12px; }
  .file-group-label {
    padding: 6px 12px 3px;
    font-size: 10px; font-weight: 700; letter-spacing: 0.08em;
    text-transform: uppercase; color: #475569;
  }
  .file-item {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 12px;
    border: none; background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px; font-family: var(--font-mono);
    cursor: pointer; text-align: left;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap; overflow: hidden;
  }
  .file-item:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
  .file-item.selected { background: var(--color-active-bg); color: var(--color-accent-soft); }
  .file-item-name { flex: 1; overflow: hidden; text-overflow: ellipsis; }
  .file-item-lines { font-size: 10px; color: var(--color-text-muted); flex-shrink: 0; }

  .files-editor { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .editor-toolbar {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 16px; border-bottom: 1px solid var(--color-border);
    flex-shrink: 0; gap: 12px;
  }
  .editor-tools { display: flex; gap: 8px; align-items: center; }
  .editor-filename {
    display: flex; align-items: center; gap: 6px;
    font-family: var(--font-mono); font-size: 13px;
    color: var(--color-text-primary); font-weight: 600;
  }
  .unsaved-warning {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 16px;
    background: var(--color-warning-muted); color: var(--color-warning);
    font-size: 12px; border-bottom: 1px solid rgba(245,158,11,0.2);
    flex-shrink: 0;
  }
  .editor-empty {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 12px; color: var(--color-text-muted);
  }
  .code-editor {
    flex: 1; resize: none; border: none; outline: none;
    background: rgba(0,0,0,0.25);
    color: var(--color-text-primary);
    font-family: var(--font-mono); font-size: 13px; line-height: 1.6;
    padding: 16px; overflow-y: auto;
    white-space: pre;
  }
  .code-editor.wrap { white-space: pre-wrap; word-break: break-all; }

  .backup-list { display: flex; flex-direction: column; }
  .backup-list-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 4px 12px; font-size: 11px; font-weight: 600;
    color: var(--color-text-muted);
    border-top: 1px solid var(--color-border);
  }
  .backup-item {
    display: flex; flex-direction: column; gap: 3px;
    padding: 8px 12px; border-bottom: 1px solid var(--color-border);
    font-size: 11px;
  }
  .backup-name { font-family: var(--font-mono); color: var(--color-text-primary); font-size: 10px; word-break: break-all; }
  .backup-ts { color: var(--color-text-muted); font-size: 10px; }

  /* ─── Preview tab ────────────────────────────────────────────────── */
  .preview-toolbar {
    display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  }
  .toggle-label-row {
    display: flex; align-items: center; gap: 6px;
    font-size: 12px; color: var(--color-text-secondary); cursor: pointer;
    white-space: nowrap;
  }
  .source-buttons {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    padding: 8px 0;
  }
  .ml-4 { margin-left: 4px; }

  /* ─── ShellEnv Custom Design ────────────────────────────── */
  
  
  
  

  .custom-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    flex-wrap: wrap;
    gap: 16px;
  }
  
  
  
  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .toolbar-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px;
    border-radius: 10px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }
  .toolbar-btn:hover {
    background: rgba(255,255,255,0.05);
    color: #fff;
  }
  .primary-btn {
    background: #7c3aed;
    color: #fff;
  }
  .primary-btn:hover {
    background: #6d28d9;
  }

  .shell-groups-list {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding-bottom: 32px;
  }
  .shell-group-card {
    background: rgba(255,255,255,0.015);
    border: 1px solid rgba(255,255,255,0.04);
    border-radius: 12px;
    overflow: hidden;
  }
  .sg-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.4);
    border-bottom: 1px solid rgba(255,255,255,0.04);
    transition: background 0.2s;
  }
  .sg-header:hover {
    background: rgba(0, 0, 0, 0.5);
  }
  .sg-name {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 14px;
    color: #fff;
  }
  .sg-badges {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .user-badge {
    background: rgba(255,255,255,0.05);
    color: var(--color-text-secondary);
    padding: 4px 12px;
    border-radius: 12px;
    font-size: 11px;
    font-weight: 500;
  }
  .var-count {
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .sg-content {
    border-top: 1px solid rgba(255,255,255,0.04);
  }
  .sg-table-header {
    display: flex;
    padding: 12px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.04);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }
  .sg-row {
    display: flex;
    align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.04);
  }
  .sg-row:last-child {
    border-bottom: none;
  }
  .col-name { width: 250px; font-family: var(--font-mono); font-weight: 600; color: #e2e8f0; font-size: 13px; }
  .col-value { flex: 1; padding-right: 20px; min-width: 0; }
  .col-meta { width: 160px; }
  .col-actions { width: 100px; display: flex; justify-content: flex-end; gap: 6px; }

  .sg-input {
    width: 100%;
    background: rgba(0,0,0,0.25);
    border: 1px solid rgba(255,255,255,0.04);
    border-radius: 6px;
    padding: 8px 12px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: 13px;
    outline: none;
    transition: all 0.2s;
  }
  .sg-input:focus, .sg-input:not([readonly]) {
    border-color: rgba(255,255,255,0.15);
    color: #fff;
    background: rgba(0,0,0,0.4);
  }
  .src-badge {
    background: rgba(20, 184, 166, 0.1);
    color: #2dd4bf;
    padding: 4px 10px;
    border-radius: 12px;
    font-size: 11px;
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .icon-btn {
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.05);
    border-radius: 6px;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.2s;
  }
  .icon-btn:hover {
    background: rgba(255,255,255,0.08);
    color: #fff;
  }
  .icon-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .sg-add-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 20px;
    color: var(--color-text-muted);
    font-size: 13px;
    cursor: pointer;
    transition: color 0.2s;
  }
  .sg-add-row:hover {
    color: #fff;
  }

  /* ─── End Custom ShellEnv Design ────────────────────────────── */


  /* ─── ShellEnv Custom Design ────────────────────────────── */
  
  
  
  

  .custom-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    flex-wrap: wrap;
    gap: 16px;
  }
  
  
  
  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .toolbar-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px;
    border-radius: 10px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }
  .toolbar-btn:hover {
    background: rgba(255,255,255,0.05);
    color: #fff;
  }
  .primary-btn {
    background: #7c3aed;
    color: #fff;
  }
  .primary-btn:hover {
    background: #6d28d9;
  }

  .shell-groups-list {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding-bottom: 32px;
  }
  .shell-group-card {
    background: rgba(255,255,255,0.015);
    border: 1px solid rgba(255,255,255,0.04);
    border-radius: 12px;
    overflow: hidden;
  }
  .sg-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.4);
    border-bottom: 1px solid rgba(255,255,255,0.04);
    transition: background 0.2s;
  }
  .sg-header:hover {
    background: rgba(0, 0, 0, 0.5);
  }
  .sg-name {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 14px;
    color: #fff;
  }
  .sg-badges {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .user-badge {
    background: rgba(255,255,255,0.05);
    color: var(--color-text-secondary);
    padding: 4px 12px;
    border-radius: 12px;
    font-size: 11px;
    font-weight: 500;
  }
  .var-count {
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .sg-content {
    border-top: 1px solid rgba(255,255,255,0.04);
  }
  .sg-table-header {
    display: flex;
    padding: 12px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.04);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }
  .sg-row {
    display: flex;
    align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.04);
  }
  .sg-row:last-child {
    border-bottom: none;
  }
  .col-name { width: 250px; font-family: var(--font-mono); font-weight: 600; color: #e2e8f0; font-size: 13px; }
  .col-value { flex: 1; padding-right: 20px; min-width: 0; }
  .col-meta { width: 160px; }
  .col-actions { width: 100px; display: flex; justify-content: flex-end; gap: 6px; }

  .sg-input {
    width: 100%;
    background: rgba(0,0,0,0.25);
    border: 1px solid rgba(255,255,255,0.04);
    border-radius: 6px;
    padding: 8px 12px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
    font-size: 13px;
    outline: none;
    transition: all 0.2s;
  }
  .sg-input:focus, .sg-input:not([readonly]) {
    border-color: rgba(255,255,255,0.15);
    color: #fff;
    background: rgba(0,0,0,0.4);
  }
  .src-badge {
    background: rgba(20, 184, 166, 0.1);
    color: #2dd4bf;
    padding: 4px 10px;
    border-radius: 12px;
    font-size: 11px;
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .icon-btn {
    background: rgba(255,255,255,0.03);
    border: 1px solid rgba(255,255,255,0.05);
    border-radius: 6px;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.2s;
  }
  .icon-btn:hover {
    background: rgba(255,255,255,0.08);
    color: #fff;
  }
  .icon-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .sg-add-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 20px;
    color: var(--color-text-muted);
    font-size: 13px;
    cursor: pointer;
    transition: color 0.2s;
  }
  .sg-add-row:hover {
    color: #fff;
  }

  /* ─── End Custom ShellEnv Design ────────────────────────────── */

</style>
