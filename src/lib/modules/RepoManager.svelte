<script lang="ts">
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

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
    <Button variant="primary" class="" onclick={() => showAddDialog = true}>
      <Plus size={14} /> Add Repo
    </Button>
    <KebabMenu>
      <button class="menu-item" onclick={makecache} disabled={loading}>
        <RefreshCw size={14} /> makecache
      </button>
      <button class="menu-item" onclick={loadRepos} disabled={loading}>
        <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    </KebabMenu>
  </PageHeader>

  <!-- Controls: Stats & Search -->
  <div style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    {#if repos.length > 0}
      <div class="stat-cards" style="margin: 0;">
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

    <!-- Search -->
    <div class="search-bar" style="flex:1; min-width:200px; margin: 0;">
      <Search size={14} style="color: var(--color-text-muted)" />
      <input bind:value={filter} placeholder="Filter repositories…" />
      {#if filter}
        <Button class="btn btn-sm -ghost" onclick={() => filter = ''} style="padding:2px 6px">✕</Button>
      {/if}
    </div>
  </div>

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
          <Button variant="outline" class="" style="margin-top:24px;" onclick={() => showAddDialog = true}>
            <Plus size={14} /> Add Your First Repo
          </Button>
        {/if}
      </div>
    {:else}
      <div class="table-wrap" style="border:none; border-radius:0">
        <table use:tableFeatures>
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
                    class="ui-toggle"
                    class:on={repo.enabled}
                    onclick={() => toggleRepo(repo)}
                    disabled={togglingId === repo.id}
                    title="{repo.enabled ? 'Disable' : 'Enable'} repo"
                    aria-checked={repo.enabled}
                    role="switch"
                  >
                    <span class="ui-toggle-thumb"></span>
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

<svelte:window onkeydown={(e) => { if (showAddDialog && e.key === 'Escape') showAddDialog = false; }} />

{#if showAddDialog}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal-backdrop"
    onclick={(e) => { if(e.target === e.currentTarget) showAddDialog = false; }}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onkeydown={(e) => e.key === 'Escape' && (showAddDialog = false)}
  >
    <div class="modal-content" style="max-width: 400px;">
      <h3 style="margin-top:0; color:var(--color-text-primary);">Add Repository</h3>
      <p style="font-size:13px; color:var(--color-text-muted); margin-bottom:16px;">
        Enter the URL for the repository configuration file (e.g. .repo file) or baseurl.
      </p>
      
      <div style="margin-bottom:20px;">
        <label for="add-repo-url" style="display:block; font-size:12px; margin-bottom:4px; color:var(--color-text-secondary);">Repository URL</label>
        <input 
          id="add-repo-url"
          class="w-full input"
          bind:value={addUrl}
          placeholder="https://example.com/repo.repo"
          onkeydown={(e) => e.key === 'Enter' && addRepo()}
        />
      </div>

      <div style="display:flex; justify-content:flex-end; gap:8px;">
        <Button variant="outline" onclick={() => showAddDialog = false} disabled={addLoading}>Cancel</Button>
        <Button variant="primary" onclick={addRepo} disabled={addLoading || !addUrl.trim()}>
          {#if addLoading}<RefreshCw size={14} class="animate-spin-slow" />{/if}
          Add
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .disabled-row td:not(:last-child) {
    opacity: 0.45;
  }

  /* Stat cards */
  .stat-cards {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: stretch;
  }
  .stat-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
  }
  .stat-value {
    font-size: 16px;
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
    font-weight: 600;
  }

</style>
