<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Globe, RefreshCw, Plus, Trash2, Save, Eye, EyeOff, Search } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface HostEntry {
    id: string;
    ip: string;
    hostnames: string[];
    comment: string;
    enabled: boolean;
    category: string;
  }

  let entries = $state<HostEntry[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let hasChanges = $state(false);
  let filter = $state('');

  // New entry form
  let newIp = $state('');
  let newHostnames = $state('');
  let newComment = $state('');
  let showAddForm = $state(false);

  const categories = $derived(
    [...new Set(entries.filter(e => e.ip).map(e => e.category))]
  );

  const filteredEntries = $derived(
    entries.filter(e => {
      if (!e.ip) return false;
      const q = filter.toLowerCase();
      return !q ||
        e.ip.includes(q) ||
        e.hostnames.some(h => h.toLowerCase().includes(q)) ||
        e.comment.toLowerCase().includes(q) ||
        e.category.toLowerCase().includes(q);
    })
  );

  async function load() {
    loading = true;
    hasChanges = false;
    statusStore.setBusy('Reading /etc/hosts…');
    try {
      entries = await invoke<HostEntry[]>('read_hosts');
      statusStore.setLastCommand('read /etc/hosts', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to read /etc/hosts: ${e}`, 'error');
      statusStore.setLastCommand('read /etc/hosts', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  function confirmSave() {
    const hasLocalhost = entries.some(e => e.ip === '127.0.0.1' && e.hostnames.includes('localhost') && e.enabled);
    let warning = 'Are you sure you want to save changes to /etc/hosts?';
    if (!hasLocalhost) {
      warning += '\n\nWARNING: You have removed or disabled the 127.0.0.1 localhost entry! This will break local networking for many applications.';
    }
    
    uiStore.confirm(
      'Confirm Save /etc/hosts',
      warning,
      () => save(),
      true
    );
  }

  async function save() {
    saving = true;
    statusStore.setBusy('Writing /etc/hosts…');
    try {
      await invoke('write_hosts', { entries });
      uiStore.addToast('/etc/hosts saved successfully', 'success');
      hasChanges = false;
      statusStore.setLastCommand('write /etc/hosts', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save /etc/hosts: ${e}`, 'error');
      statusStore.setLastCommand('write /etc/hosts', 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  function toggleEntry(entry: HostEntry) {
    entry.enabled = !entry.enabled;
    entries = [...entries];
    hasChanges = true;
  }

  function addEntry() {
    if (!newIp.trim() || !newHostnames.trim()) return;
    const newEntry: HostEntry = {
      id: `entry-${Date.now()}`,
      ip: newIp.trim(),
      hostnames: newHostnames.trim().split(/\s+/).filter(Boolean),
      comment: newComment.trim(),
      enabled: true,
      category: 'Custom',
    };
    entries = [...entries, newEntry];
    newIp = '';
    newHostnames = '';
    newComment = '';
    showAddForm = false;
    hasChanges = true;
  }

  function removeEntry(entry: HostEntry) {
    let warning = `Remove entry for ${entry.ip} → ${entry.hostnames.join(', ')}?`;
    if (entry.ip === '127.0.0.1' || entry.hostnames.includes('localhost')) {
      warning += '\n\nWARNING: Deleting localhost entries can severely break system networking!';
    }
    uiStore.confirm(
      'Remove Host Entry',
      warning,
      () => {
        entries = entries.filter(e => e.id !== entry.id);
        hasChanges = true;
      },
      true
    );
  }

  function updateHostnames(entry: HostEntry, value: string) {
    entry.hostnames = value.split(/\s+/).filter(Boolean);
    entries = [...entries];
    hasChanges = true;
  }

  $effect(() => { load(); });
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><Globe size={20} /></div>
    <div>
      <h1 class="module-title">Hosts Manager</h1>
      <p class="module-subtitle">Edit /etc/hosts — requires polkit authentication to save</p>
    </div>
    <div style="margin-left:auto; display:flex; gap:8px">
      <button class="btn btn-ghost" onclick={load} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
      </button>
      <button class="btn btn-ghost" onclick={() => showAddForm = !showAddForm}>
        <Plus size={14} /> Add Entry
      </button>
      <button
        class="btn btn-primary"
        onclick={confirmSave}
        disabled={saving || !hasChanges}
        style={hasChanges ? 'animation: pulse-glow 2s infinite' : ''}
      >
        {#if saving}
          <RefreshCw size={14} class="animate-spin-slow" /> Saving…
        {:else}
          <Save size={14} /> {hasChanges ? 'Save Changes' : 'Saved'}
        {/if}
      </button>
    </div>
  </div>

  {#if hasChanges}
    <div style="padding:10px 14px;background:var(--color-warning-muted);border:1px solid rgba(251,191,36,0.2);border-radius:8px;font-size:12px;color:var(--color-warning);display:flex;align-items:center;gap:8px">
      <span>⚠</span>
      <span>You have unsaved changes. Click "Save Changes" to write to /etc/hosts (requires polkit authentication).</span>
    </div>
  {/if}

  <!-- Add Entry Form -->
  {#if showAddForm}
    <div class="card animate-fade-slide" style="border-color:var(--color-border-focus)">
      <h3 style="font-size:14px;font-weight:600;margin:0 0 14px;color:var(--color-text-primary)">Add Host Entry</h3>
      <div style="display:grid;grid-template-columns:140px 1fr 1fr;gap:10px;align-items:end">
        <div>
          <label class="field-label" for="new-ip">IP Address</label>
          <input id="new-ip" class="input" bind:value={newIp} placeholder="127.0.0.1" />
        </div>
        <div>
          <label class="field-label" for="new-hostnames">Hostnames (space-separated)</label>
          <input id="new-hostnames" class="input" bind:value={newHostnames} placeholder="myhost.local myhost" />
        </div>
        <div>
          <label class="field-label" for="new-comment">Comment (optional)</label>
          <input id="new-comment" class="input" bind:value={newComment} placeholder="My custom host" />
        </div>
      </div>
      <div style="display:flex;gap:8px;margin-top:12px;justify-content:flex-end">
        <button class="btn btn-ghost" onclick={() => { showAddForm = false; newIp = ''; newHostnames = ''; }}>Cancel</button>
        <button
          class="btn btn-primary"
          onclick={addEntry}
          disabled={!newIp.trim() || !newHostnames.trim()}
        >
          <Plus size={14} /> Add
        </button>
      </div>
    </div>
  {/if}

  <!-- Stats -->
  <div style="display:flex; gap:12px; flex-wrap:wrap">
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-text-primary)">{entries.filter(e => e.ip).length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">Entries</span>
    </div>
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-success)">{entries.filter(e => e.ip && e.enabled).length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">Active</span>
    </div>
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-accent)">{categories.length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">Categories</span>
    </div>
  </div>

  <!-- Search -->
  <div class="search-bar">
    <Search size={14} style="color:var(--color-text-muted)" />
    <input bind:value={filter} placeholder="Filter by IP, hostname, or comment…" />
  </div>

  <!-- Grouped by category -->
  {#if loading}
    <div class="card" style="display:flex;align-items:center;justify-content:center;gap:10px;padding:40px;color:var(--color-text-muted)">
      <RefreshCw size={16} class="animate-spin-slow" /> Reading /etc/hosts…
    </div>
  {:else if filteredEntries.length === 0}
    <div class="empty-state card">
      <Globe size={40} class="empty-state-icon" />
      <span>{filter ? 'No entries match your search' : 'No valid host entries found'}</span>
    </div>
  {:else}
    {#each categories as category}
      {@const catEntries = filteredEntries.filter(e => e.category === category)}
      {#if catEntries.length > 0}
        <div>
          <h3 class="cat-header">
            <span class="cat-dot"></span>
            {category}
            <span class="badge badge-muted">{catEntries.length}</span>
          </h3>
          <div class="card module-content-scroll" style="padding:0">
            <div class="table-wrap" style="border:none;border-radius:0">
              <table>
                <thead>
                  <tr>
                    <th style="width:140px">IP Address</th>
                    <th>Hostnames</th>
                    <th>Comment</th>
                    <th style="text-align:center">Active</th>
                    <th style="text-align:right"></th>
                  </tr>
                </thead>
                <tbody>
                  {#each catEntries as entry (entry.id)}
                    <tr class:disabled-row={!entry.enabled}>
                      <td>
                        <code class="ip-cell" style={!entry.enabled ? 'opacity:0.5' : ''}>{entry.ip}</code>
                      </td>
                      <td>
                        <input
                          class="inline-input"
                          value={entry.hostnames.join(' ')}
                          oninput={(e) => updateHostnames(entry, (e.target as HTMLInputElement).value)}
                          style={!entry.enabled ? 'opacity:0.5' : ''}
                        />
                      </td>
                      <td>
                        <input
                          class="inline-input"
                          style="color:var(--color-text-muted);font-size:11px;{!entry.enabled ? 'opacity:0.5' : ''}"
                          value={entry.comment}
                          oninput={(e) => { entry.comment = (e.target as HTMLInputElement).value; hasChanges = true; }}
                          placeholder="—"
                        />
                      </td>
                      <td style="text-align:center">
                        <label class="toggle">
                          <input
                            type="checkbox"
                            checked={entry.enabled}
                            onchange={() => toggleEntry(entry)}
                          />
                          <span class="toggle-slider"></span>
                        </label>
                      </td>
                      <td style="text-align:right">
                        <button
                          class="btn btn-sm btn-danger"
                          onclick={() => removeEntry(entry)}
                        >
                          <Trash2 size={11} />
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .field-label {
    display: block;
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-muted);
    margin-bottom: 5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .cat-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 0 0 8px;
  }

  .cat-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-accent);
    flex-shrink: 0;
  }

  .ip-cell {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-accent);
    white-space: nowrap;
  }

  .inline-input {
    background: none;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-mono);
    width: 100%;
    padding: 2px 4px;
    border-radius: 4px;
    transition: background 0.15s;
  }

  .inline-input:focus {
    background: var(--color-bg-hover);
    outline: 1px solid var(--color-border-focus);
  }

  .disabled-row {
    text-decoration: line-through;
    opacity: 0.5;
  }
  .disabled-row:hover { opacity: 0.7; }
</style>
