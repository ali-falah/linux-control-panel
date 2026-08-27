const fs = require('fs');

const filePath = '/home/ali/Desktop/MyActiveCodes/linux-control-panel/src/lib/modules/ShellEnv.svelte';
let content = fs.readFileSync(filePath, 'utf8');

const varsTabRegex = /(<!-- ══ VARIABLES ══════════════════════════════════════════════════════ -->\n\s*\{#if activeTab === 'variables'\})[\s\S]*?(?=<!-- ══ PATH MANAGER)/;

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
    <!-- ══ PATH MANAGER`;

if (!varsTabRegex.test(content)) {
  console.log('Regex did not match!');
  process.exit(1);
}
content = content.replace(varsTabRegex, newVarsTab);
fs.writeFileSync(filePath, content);
console.log('Success');
