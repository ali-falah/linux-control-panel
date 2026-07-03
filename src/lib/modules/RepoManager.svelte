<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Package, RefreshCw, Plus, ToggleLeft, ToggleRight, Link, Search, Database } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';

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
  <PageHeader title="Repo Manager" subtitle="Manage DNF repositories from /etc/yum.repos.d/" icon={Package}>
    <button class="btn btn-primary" onclick={() => showAddDialog = true}>
      <Plus size={14} /> Add Repo
    </button>
    <KebabMenu>
      <button class="menu-item" onclick={makecache} disabled={loading}>
        <RefreshCw size={14} /> makecache
      </button>
      <button class="menu-item" onclick={loadRepos} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    </KebabMenu>
  </PageHeader>

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

  <SideDrawer bind:isOpen={showAddDialog} title="Add Repository" width="400px">
    <div style="display:flex; flex-direction:column; gap:16px;">
      <p style="color:var(--color-text-secondary); margin:0;">
        Enter the URL of a .repo file or a baseurl to add it to your system.
      </p>
      <div class="search-bar">
        <Link size={14} style="color:var(--color-text-muted)" />
        <input
          bind:value={addUrl}
          placeholder="https://example.com/repo.repo"
          onkeydown={(e) => e.key === 'Enter' && addRepo()}
          style="width: 100%"
        />
      </div>
      <div style="display:flex; gap:10px; justify-content:flex-end; margin-top:10px;">
        <button class="btn btn-ghost" onclick={() => { showAddDialog = false; addUrl = ''; }}>
          Cancel
        </button>
        <button class="btn btn-primary" onclick={addRepo} disabled={addLoading || !addUrl.trim()}>
          {addLoading ? 'Adding…' : 'Add Repository'}
        </button>
      </div>
    </div>
  </SideDrawer>

  <!-- Stats -->
  {#if repos.length > 0}
    <div class="stat-cards">
      <div class="stat-card">
        <span class="stat-value">{repos.length}</span>
        <span class="stat-label">Total</span>
      </div>
      <div class="stat-card">
        <span class="stat-value enabled">{repos.filter(r => r.enabled).length}</span>
        <span class="stat-label">Enabled</span>
      </div>
      <div class="stat-card">
        <span class="stat-value disabled">{repos.filter(r => !r.enabled).length}</span>
        <span class="stat-label">Disabled</span>
      </div>
      <div class="stat-card">
        <span class="stat-value errors">{repos.filter(r => !r.baseurl && !r.metalink && !r.mirrorlist).length}</span>
        <span class="stat-label">Errors</span>
      </div>
    </div>
  {/if}

  <!-- Repo List -->
  <div class="card module-content-scroll" style="padding:0">
    {#if loading}
      <div style="padding: 16px; display: flex; flex-direction: column; gap: 8px;">
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
      </div>
    {:else if filteredRepos.length === 0}
      <div class="empty-state" style="padding: 64px 32px;">
        <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
          <Database size={32} class="empty-state-icon" style="margin:0" />
        </div>
        <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
          {filter ? 'No results found' : 'No Repositories'}
        </span>
        <span style="color:var(--color-text-muted); margin-top:8px;">
          {filter ? 'Try adjusting your search criteria.' : 'No repositories found in /etc/yum.repos.d/.'}
        </span>
        {#if !filter}
          <button class="btn btn-outline" style="margin-top:24px;" onclick={() => showAddDialog = true}>
            <Plus size={14} /> Add Your First Repo
          </button>
        {/if}
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
              <th style="text-align:center">Enabled</th>
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
                  <button
                    class="repo-toggle"
                    class:on={repo.enabled}
                    onclick={() => toggleRepo(repo)}
                    disabled={togglingId === repo.id}
                    title="{repo.enabled ? 'Disable' : 'Enable'} repo"
                    aria-checked={repo.enabled}
                    role="switch"
                  >
                    <span class="repo-toggle-thumb"></span>
                  </button>
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

  /* Stat cards */
  .stat-cards {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  .stat-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 14px 20px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    min-width: 90px;
    backdrop-filter: blur(8px);
  }
  .stat-value {
    font-size: 22px;
    font-weight: 700;
    line-height: 1;
    color: var(--color-text-primary);
  }
  .stat-value.enabled { color: var(--color-success); }
  .stat-value.disabled { color: var(--color-text-muted); }
  .stat-value.errors { color: #f87171; }
  .stat-label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 500;
  }

  /* ── Repo toggle switch ─────────────────────────────────────── */
  .repo-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    border: none;
    background: #2a2f3e;
    cursor: pointer;
    transition: background 0.2s cubic-bezier(0.4, 0, 0.2, 1),
                box-shadow 0.2s ease;
    flex-shrink: 0;
    outline: none;
    padding: 0;
  }
  .repo-toggle:focus-visible {
    box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.4);
  }
  .repo-toggle.on {
    background: #4f46e5;
    box-shadow: 0 0 8px rgba(79, 70, 229, 0.45);
  }
  .repo-toggle:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .repo-toggle-thumb {
    position: absolute;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.9);
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1),
                box-shadow 0.2s ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }
  .repo-toggle.on .repo-toggle-thumb {
    transform: translateX(16px);
  }
</style>
