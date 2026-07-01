<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Package, RefreshCw, Plus, ToggleLeft, ToggleRight, Link, Search } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface RepoEntry {
    id: string;
    name: string;
    baseurl: string;
    enabled: boolean;
    file_path: string;
    metalink: string | null;
    mirrorlist: string | null;
    gpgcheck: boolean;
  }

  let repos = $state<RepoEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let showAddDialog = $state(false);
  let addUrl = $state('');
  let addLoading = $state(false);
  let togglingId = $state<string | null>(null);

  const filteredRepos = $derived(
    repos.filter(r =>
      r.name.toLowerCase().includes(filter.toLowerCase()) ||
      r.id.toLowerCase().includes(filter.toLowerCase())
    )
  );

  async function loadRepos() {
    loading = true;
    statusStore.setBusy('Loading repos…');
    try {
      repos = await invoke<RepoEntry[]>('list_repos');
      statusStore.setLastCommand('list_repos', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load repos: ${e}`, 'error');
      statusStore.setLastCommand('list_repos', 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function toggleRepo(repo: RepoEntry) {
    togglingId = repo.id;
    const newEnabled = !repo.enabled;
    try {
      await invoke('toggle_repo', {
        repoId: repo.id,
        enabled: newEnabled,
        filePath: repo.file_path,
      });
      repo.enabled = newEnabled;
      repos = [...repos]; // trigger reactivity
      uiStore.addToast(
        `Repo "${repo.name}" ${newEnabled ? 'enabled' : 'disabled'}`,
        'success'
      );
      statusStore.setLastCommand(`toggle_repo ${repo.id}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle repo: ${e}`, 'error');
      statusStore.setLastCommand(`toggle_repo ${repo.id}`, 1, false);
    } finally {
      togglingId = null;
    }
  }

  async function addRepo() {
    if (!addUrl.trim()) return;
    addLoading = true;
    try {
      await invoke('add_repo', { url: addUrl.trim() });
      uiStore.addToast('Repository added successfully', 'success');
      showAddDialog = false;
      addUrl = '';
      await loadRepos();
      statusStore.setLastCommand(`add_repo ${addUrl}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to add repo: ${e}`, 'error');
      statusStore.setLastCommand('add_repo', 1, false);
    } finally {
      addLoading = false;
    }
  }

  async function makecache() {
    statusStore.setBusy('Running dnf makecache…');
    try {
      await invoke('run_makecache');
      uiStore.addToast('dnf makecache completed', 'success');
      statusStore.setLastCommand('dnf makecache', 0, true);
    } catch (e) {
      uiStore.addToast(`makecache failed: ${e}`, 'error');
      statusStore.setLastCommand('dnf makecache', 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  $effect(() => {
    loadRepos();
  });
</script>

<div class="module-page">
  <!-- Header -->
  <div class="module-header">
    <div class="module-icon"><Package size={20} /></div>
    <div>
      <h1 class="module-title">Repo Manager</h1>
      <p class="module-subtitle">Manage DNF repositories from /etc/yum.repos.d/</p>
    </div>
    <div style="margin-left:auto; display:flex; gap:8px;">
      <button class="btn btn-ghost" onclick={makecache} disabled={loading}>
        <RefreshCw size={14} /> makecache
      </button>
      <button class="btn btn-ghost" onclick={loadRepos} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} />
        Refresh
      </button>
      <button class="btn btn-primary" onclick={() => showAddDialog = true}>
        <Plus size={14} /> Add Repo
      </button>
    </div>
  </div>

  <!-- Search -->
  <div class="search-bar">
    <Search size={14} style="color: var(--color-text-muted)" />
    <input bind:value={filter} placeholder="Filter repositories…" />
    {#if filter}
      <button class="btn btn-sm btn-ghost" onclick={() => filter = ''} style="padding:2px 6px">
        ✕
      </button>
    {/if}
  </div>

  <!-- Add Repo Dialog -->
  {#if showAddDialog}
    <div class="card animate-fade-slide" style="border-color: var(--color-border-focus)">
      <h3 style="font-size:14px; font-weight:600; margin:0 0 12px; color:var(--color-text-primary)">
        Add Repository
      </h3>
      <div style="display:flex; gap:8px; align-items:center">
        <div class="search-bar" style="flex:1">
          <Link size={14} style="color:var(--color-text-muted)" />
          <input
            bind:value={addUrl}
            placeholder="https://example.com/repo.repo or URL"
            onkeydown={(e) => e.key === 'Enter' && addRepo()}
          />
        </div>
        <button class="btn btn-primary" onclick={addRepo} disabled={addLoading || !addUrl.trim()}>
          {addLoading ? 'Adding…' : 'Add'}
        </button>
        <button class="btn btn-ghost" onclick={() => { showAddDialog = false; addUrl = ''; }}>
          Cancel
        </button>
      </div>
    </div>
  {/if}

  <!-- Stats -->
  {#if repos.length > 0}
    <div style="display:flex; gap:12px; flex-wrap:wrap">
      <div class="card-raised" style="display:flex; align-items:center; gap:10px; padding:12px 16px">
        <span style="font-size:22px; font-weight:700; color:var(--color-text-primary)">{repos.length}</span>
        <span style="font-size:12px; color:var(--color-text-muted)">Total Repos</span>
      </div>
      <div class="card-raised" style="display:flex; align-items:center; gap:10px; padding:12px 16px">
        <span style="font-size:22px; font-weight:700; color:var(--color-success)">{repos.filter(r => r.enabled).length}</span>
        <span style="font-size:12px; color:var(--color-text-muted)">Enabled</span>
      </div>
      <div class="card-raised" style="display:flex; align-items:center; gap:10px; padding:12px 16px">
        <span style="font-size:22px; font-weight:700; color:var(--color-text-muted)">{repos.filter(r => !r.enabled).length}</span>
        <span style="font-size:12px; color:var(--color-text-muted)">Disabled</span>
      </div>
    </div>
  {/if}

  <!-- Repo List -->
  <div class="card module-content-scroll" style="padding:0">
    {#if loading}
      <div style="padding:32px; display:flex; align-items:center; justify-content:center; gap:10px; color:var(--color-text-muted)">
        <RefreshCw size={16} class="animate-spin-slow" />
        <span>Loading repositories…</span>
      </div>
    {:else if filteredRepos.length === 0}
      <div class="empty-state">
        <Package size={40} class="empty-state-icon" />
        <span>{filter ? 'No repos match your search' : 'No repositories found in /etc/yum.repos.d/'}</span>
      </div>
    {:else}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table>
          <thead>
            <tr>
              <th>Repository</th>
              <th>ID</th>
              <th>URL / Metalink</th>
              <th style="text-align:center">GPG</th>
              <th style="text-align:center">Status</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredRepos as repo (repo.id)}
              <tr class:disabled-row={!repo.enabled}>
                <td>
                  <div style="font-weight:500; color:var(--color-text-primary)">{repo.name}</div>
                  <div style="font-size:11px; color:var(--color-text-muted); font-family:var(--font-mono)">{repo.file_path.split('/').pop()}</div>
                </td>
                <td>
                  <code style="font-size:11px; color:var(--color-text-accent)">{repo.id}</code>
                </td>
                <td style="max-width:280px">
                  <div style="font-size:11px; color:var(--color-text-secondary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-family:var(--font-mono)">
                    {repo.metalink ?? repo.mirrorlist ?? repo.baseurl ?? '—'}
                  </div>
                </td>
                <td style="text-align:center">
                  <span class="badge {repo.gpgcheck ? 'badge-success' : 'badge-muted'}">
                    {repo.gpgcheck ? 'on' : 'off'}
                  </span>
                </td>
                <td style="text-align:center">
                  <label class="toggle" title="{repo.enabled ? 'Disable' : 'Enable'} repo">
                    <input
                      type="checkbox"
                      checked={repo.enabled}
                      disabled={togglingId === repo.id}
                      onchange={() => toggleRepo(repo)}
                    />
                    <span class="toggle-slider"></span>
                  </label>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<style>
  .disabled-row td:not(:last-child) {
    opacity: 0.45;
  }
</style>
