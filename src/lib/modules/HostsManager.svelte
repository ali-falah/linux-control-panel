<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { Globe, RefreshCw, Plus, Trash2, Save, Eye, EyeOff, Search } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';

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
      statusStore.setLastCommand('cat /etc/hosts', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load hosts: ${e}`, 'error');
      statusStore.setLastCommand('cat /etc/hosts', 1, false);
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
      statusStore.setLastCommand('echo "..." > /etc/hosts', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save hosts: ${e}`, 'error');
      statusStore.setLastCommand('echo "..." > /etc/hosts', 1, false);
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
  <PageHeader title="Hosts Manager" subtitle="Edit /etc/hosts — requires polkit authentication to save" icon={Globe}>
    <Button variant="ghost" class="" onclick={load} disabled={loading}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Reload
    </Button>
    <Button variant="outline" class="" onclick={() => showAddForm = true}>
      <Plus size={14} /> Add Entry
    </Button>
    <Button
      variant="primary" class=""
      onclick={confirmSave}
      disabled={saving || !hasChanges}
      style={hasChanges ? 'animation: pulse-glow 2s infinite; box-shadow: 0 0 12px var(--color-accent-glow);' : ''}
    >
      {#if saving}
        <RefreshCw size={14} class="animate-spin-slow" /> Saving…
      {:else}
        <Save size={14} /> {hasChanges ? 'Save Changes' : 'Saved'}
      {/if}
    </Button>
  </PageHeader>

  {#if hasChanges}
    <div style="padding:10px 14px;background:var(--color-warning-muted);border:1px solid rgba(251,191,36,0.2);border-radius:8px;font-size:12px;color:var(--color-warning);display:flex;align-items:center;gap:8px">
      <span>⚠</span>
      <span>You have unsaved changes. Click "Save Changes" to write to /etc/hosts (requires polkit authentication).</span>
    </div>
  {/if}

  <SideDrawer bind:isOpen={showAddForm} title="Add Host Entry" width="400px">
    <div style="display:flex; flex-direction:column; gap:16px;">
      <p style="color:var(--color-text-secondary); margin:0 0 8px;">
        Add a new static mapping to your hosts file.
      </p>
      
      <div>
        <label class="field-label" for="new-ip">IP Address</label>
        <input id="new-ip" class="input" bind:value={newIp} placeholder="127.0.0.1" style="width: 100%" />
      </div>
      <div>
        <label class="field-label" for="new-hostnames">Hostnames (space-separated)</label>
        <input id="new-hostnames" class="input" bind:value={newHostnames} placeholder="myhost.local myhost" style="width: 100%" />
      </div>
      <div>
        <label class="field-label" for="new-comment">Comment (optional)</label>
        <input id="new-comment" class="input" bind:value={newComment} placeholder="My custom host" style="width: 100%" />
      </div>

      <div style="display:flex;gap:10px;justify-content:flex-end;margin-top:16px">
        <Button variant="ghost" class="" onclick={() => { showAddForm = false; newIp = ''; newHostnames = ''; }}>Cancel</Button>
        <Button
          variant="primary" class=""
          onclick={addEntry}
          disabled={!newIp.trim() || !newHostnames.trim()}
        >
          <Plus size={14} /> Add Entry
        </Button>
      </div>
    </div>
  </SideDrawer>

  <!-- Controls: Stats & Search -->
  <div style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    <div style="display:flex; gap:12px; flex-wrap:wrap; margin: 0; align-items:stretch;">
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-text-primary);line-height:1;">{entries.filter(e => e.ip).length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Entries</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-success);line-height:1;">{entries.filter(e => e.ip && e.enabled).length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Active</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-accent);line-height:1;">{categories.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Categories</span>
      </div>
    </div>

    <!-- Search -->
    <div class="search-bar" style="flex:1; min-width:200px; margin: 0;">
      <Search size={14} style="color:var(--color-text-muted)" />
      <input bind:value={filter} placeholder="Filter by IP, hostname, or comment…" />
    </div>
  </div>

  <!-- Grouped by category -->
  {#if loading}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Reading /etc/hosts…</span>
    </div>
  {:else if filteredEntries.length === 0}
    <div class="empty-state" style="padding: 64px 32px;">
      <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
        <Globe size={32} class="empty-state-icon" style="margin:0" />
      </div>
      <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
        {filter ? 'No results found' : 'No Hosts Found'}
      </span>
      <span style="color:var(--color-text-muted); margin-top:8px;">
        {filter ? 'Try adjusting your search criteria.' : 'No valid host entries found in /etc/hosts.'}
      </span>
      {#if !filter}
        <Button variant="outline" class="" style="margin-top:24px;" onclick={() => showAddForm = true}>
          <Plus size={14} /> Add First Entry
        </Button>
      {/if}
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
              <table use:tableFeatures>
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
                        <button
                          class="ui-toggle"
                          class:on={entry.enabled}
                          onclick={() => toggleEntry(entry)}
                          title="{entry.enabled ? 'Disable' : 'Enable'} entry"
                          aria-checked={entry.enabled}
                          role="switch"
                        >
                          <span class="ui-toggle-thumb"></span>
                        </button>
                      </td>
                      <td style="text-align:right">
                        <KebabMenu>
                          <button
                            class="menu-item danger"
                            onclick={() => removeEntry(entry)}
                          >
                            <Trash2 size={14} /> Delete Entry
                          </button>
                        </KebabMenu>
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
