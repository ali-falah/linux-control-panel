const fs = require('fs');
const path = require('path');

const filePath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/ShellEnv.svelte';
let content = fs.readFileSync(filePath, 'utf8');

// 1. Replace the tab bar (lines 292-308ish)
const tabRegex = /<div class="tab-bar">[\s\S]*?<\/div>/;
const newTabs = `<div class="custom-tabs-container">
    <button class="custom-tab" class:active={activeTab === 'variables'} onclick={() => (activeTab = 'variables')}>
      <Variable size={14} /> Variables
    </button>
    <button class="custom-tab" class:active={activeTab === 'path'} onclick={() => (activeTab = 'path')}>
      <FolderOpen size={14} /> PATH Manager
    </button>
    <button class="custom-tab" class:active={activeTab === 'files'} onclick={() => (activeTab = 'files')}>
      <FileCode size={14} /> Profile Files
    </button>
    <button class="custom-tab" class:active={activeTab === 'preview'} onclick={() => (activeTab = 'preview')}>
      <Eye size={14} /> Live Preview
    </button>
  </div>`;
content = content.replace(tabRegex, newTabs);

// 2. Replace the variables tab layout
// It starts around "<!-- ══ VARIABLES TAB ══════════════════════════════════════════════════ -->"
// and ends before "<!-- ══ PATH MANAGER ════════════════════════════════════════════════════ -->"
const varsTabRegex = /(<!-- ══ VARIABLES TAB ══════════════════════════════════════════════════ -->\n\s*\{#if activeTab === 'variables'\})[\s\S]*?(?=<!-- ══ PATH MANAGER)/;

const newVarsTab = `$1
      <div class="tab-section" style="padding: 0;">
        <div class="custom-toolbar">
          <div class="search-box">
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
            <!-- Start Add Var uses the first writable profile by default, or just shows a modal? We'll show the form below -->
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
                <div class="sg-header" tabindex="0" role="button" onclick={() => toggleGroupCollapse(group.source_path)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleGroupCollapse(group.source_path); } }} id={\`shell-group-\${group.source_path}\`}>
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
                            <span class="src-badge">src: {shortPath(v.source_path)}:{v.line_number}</span>
                          {/if}
                        </div>
                        <div class="col-actions">
                          {#if editingVar?.v === v}
                            <button class="icon-btn" style="color:var(--color-accent)" onclick={saveEditVar} disabled={savingVar}>
                              {#if savingVar}<div class="spinner-sm"></div>{:else}<Save size={14} />{/if}
                            </button>
                            <button class="icon-btn" onclick={() => (editingVar = null)}><XCircle size={14} /></button>
                          {:else}
                            <button class="icon-btn" onclick={() => startEditVar(v)} title="Edit"><Variable size={14}/></button>
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
    <!-- ══ PATH MANAGER`;

content = content.replace(varsTabRegex, newVarsTab);

// 3. Add the new CSS
const newCss = `
  /* ─── ShellEnv Custom Design ────────────────────────────── */
  .custom-tabs-container {
    display: inline-flex;
    gap: 4px;
    background: rgba(0, 0, 0, 0.2);
    padding: 6px;
    border-radius: 12px;
    margin-bottom: 24px;
    align-self: flex-start;
  }
  .custom-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }
  .custom-tab:hover {
    color: #fff;
  }
  .custom-tab.active {
    background: rgba(255,255,255,0.03);
    border-color: rgba(139, 92, 246, 0.3); /* subtle purple border */
    color: #fff;
  }

  .custom-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    flex-wrap: wrap;
    gap: 16px;
  }
  .search-box {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(0, 0, 0, 0.2);
    padding: 0 16px;
    border-radius: 10px;
    height: 40px;
    width: 400px;
    border: 1px solid rgba(255,255,255,0.05);
    color: var(--color-text-muted);
  }
  .search-box:focus-within {
    border-color: rgba(255,255,255,0.15);
  }
  .search-box input {
    background: transparent;
    border: none;
    outline: none;
    color: #fff;
    font-size: 13px;
    flex: 1;
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
    transition: background 0.2s;
  }
  .sg-header:hover {
    background: rgba(255,255,255,0.02);
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
`;

content = content.replace('</style>', newCss + '\n</style>');

fs.writeFileSync(filePath, content);
console.log('Done rewriting ShellEnv.svelte');
