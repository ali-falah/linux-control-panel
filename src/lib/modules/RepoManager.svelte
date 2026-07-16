<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { Package, RefreshCw, Plus, ToggleLeft, ToggleRight, Link, Search, Database, Settings, Activity } from '@lucide/svelte';
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
    priority: number | null;
  }

  interface SpeedResult {
    url: string;
    speed_ms: number | null;
  }

  let repos = $state<RepoEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let showAddDialog = $state(false);
  let addUrl = $state('');
  let addLoading = $state(false);
  let togglingId = $state<string | null>(null);

  // Edit Side Drawer State
  let editOpen = $state(false);
  let selectedRepo = $state<RepoEntry | null>(null);
  let editName = $state('');
  let editBaseurl = $state('');
  let editMetalink = $state('');
  let editMirrorlist = $state('');
  let editGpgcheck = $state(false);
  let editPriority = $state<number | null>(null);
  let saving = $state(false);

  // Speed Test State
  let testingSpeeds = $state(false);
  let speedResults = $state<SpeedResult[]>([]);

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
      statusStore.setLastCommand('dnf repolist -v', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load repos: ${e}`, 'error');
      statusStore.setLastCommand('dnf repolist -v', 1, false);
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
      statusStore.setLastCommand(`dnf config-manager --${newEnabled ? 'enable' : 'disable'} ${repo.id}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle repo: ${e}`, 'error');
      statusStore.setLastCommand(`dnf config-manager --${newEnabled ? 'enable' : 'disable'} ${repo.id}`, 1, false);
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
      statusStore.setLastCommand(`dnf config-manager --add-repo ${addUrl}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to add repo: ${e}`, 'error');
      statusStore.setLastCommand(`dnf config-manager --add-repo ${addUrl}`, 1, false);
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

  function openEditDrawer(repo: RepoEntry) {
    selectedRepo = repo;
    editName = repo.name;
    editBaseurl = repo.baseurl;
    editMetalink = repo.metalink || '';
    editMirrorlist = repo.mirrorlist || '';
    editGpgcheck = repo.gpgcheck;
    editPriority = repo.priority;
    speedResults = [];
    testingSpeeds = false;
    editOpen = true;
  }

  async function saveRepo() {
    if (!selectedRepo) return;
    saving = true;
    statusStore.setBusy('Saving repository changes…');
    try {
      await invoke('save_repo_details', {
        repoId: selectedRepo.id,
        filePath: selectedRepo.file_path,
        name: editName,
        baseurl: editBaseurl,
        metalink: editMetalink ? editMetalink : null,
        mirrorlist: editMirrorlist ? editMirrorlist : null,
        gpgcheck: editGpgcheck,
        priority: editPriority !== null ? editPriority : null,
      });
      uiStore.addToast(`Repository "${selectedRepo.id}" updated`, 'success');
      editOpen = false;
      await loadRepos();
      statusStore.setLastCommand(`nano ${selectedRepo.file_path}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save repository: ${e}`, 'error');
      statusStore.setLastCommand(`nano ${selectedRepo.file_path}`, 1, false);
    } finally {
      saving = false;
      statusStore.clearBusy();
    }
  }

  async function testSpeeds() {
    testingSpeeds = true;
    speedResults = [];
    try {
      speedResults = await invoke<SpeedResult[]>('test_repo_mirror_speeds', {
        baseurl: editBaseurl,
        mirrorlist: editMirrorlist ? editMirrorlist : null,
        metalink: editMetalink ? editMetalink : null,
      });
      uiStore.addToast('Mirror speed test completed', 'success');
    } catch (e) {
      uiStore.addToast(`Failed to test speeds: ${e}`, 'error');
    } finally {
      testingSpeeds = false;
    }
  }

  $effect(() => {
    loadRepos();
  });
</script>

<div class="module-page"> 
  <PageHeader title="Repo Manager" subtitle="Manage DNF repositories from /etc/yum.repos.d/" icon={Package}>
    <Button variant="primary" onclick={() => showAddDialog = true}>
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
    <SearchBar bind:value={filter} placeholder="Filter repositories…" style="flex:1; min-width:200px; margin: 0;" />
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
          <Button variant="outline" style="margin-top:24px;" onclick={() => showAddDialog = true}>
            <Plus size={14} /> Add Your First Repo
          </Button>
        {/if}
      </div>
    {:else}
      <div class="table-wrap" style="border:none; border-radius:0; overflow-x:auto;">
        <table use:tableFeatures style="min-width: 900px;">
          <thead>
            <tr>
              <th>Repository</th>
              <th>ID</th>
              <th>URL / Metalink</th>
              <th style="text-align:center">Priority</th>
              <th style="text-align:center">GPG</th>
              <th style="text-align:center">Enabled</th>
              <th style="width:50px"></th>
            </tr>
          </thead>
          <tbody>
            {#each filteredRepos as repo (repo.id)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <tr class:disabled-row={!repo.enabled} onclick={() => openEditDrawer(repo)} style="cursor: pointer;">
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
                  <span style="font-family:var(--font-mono); color:var(--color-text-secondary)">
                    {repo.priority ?? '99'}
                  </span>
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
                    onclick={(e) => { e.stopPropagation(); toggleRepo(repo); }}
                    disabled={togglingId === repo.id}
                    title="{repo.enabled ? 'Disable' : 'Enable'} repo"
                    aria-checked={repo.enabled}
                    role="switch"
                  >
                    <span class="ui-toggle-thumb"></span>
                  </button>
                </td>
                <td style="text-align:center">
                  <button 
                    class="action-btn"
                    onclick={(e) => { e.stopPropagation(); openEditDrawer(repo); }}
                    title="Configure repository"
                  >
                    <Settings size={14} />
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

<svelte:window onkeydown={(e) => { 
  if (e.key === 'Escape') {
    if (showAddDialog) showAddDialog = false;
    if (editOpen) editOpen = false;
  }
}} />

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
    <div class="modal" style="width: 400px; max-width: calc(100vw - 32px);">
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

<!-- Edit Repository Drawer -->
<SideDrawer bind:isOpen={editOpen} title="Repository Configurations" width="550px">
  {#if selectedRepo}
    <div style="display:flex; flex-direction:column; gap:16px; flex:1;">
      
      <!-- Top info -->
      <div style="background:rgba(255,255,255,0.02); border:1px solid var(--color-border); border-radius:8px; padding:12px; font-size:12px;">
        <div style="display:flex; justify-content:space-between; margin-bottom:4px;">
          <span style="color:var(--color-text-muted);">Repository ID:</span>
          <code style="color:var(--color-text-accent); font-weight:600;">{selectedRepo.id}</code>
        </div>
        <div style="display:flex; justify-content:space-between;">
          <span style="color:var(--color-text-muted);">Config File Path:</span>
          <code style="color:var(--color-text-secondary); text-overflow:ellipsis; overflow:hidden; white-space:nowrap; max-width:300px;" title={selectedRepo.file_path}>
            {selectedRepo.file_path}
          </code>
        </div>
      </div>

      <!-- Form Inputs -->
      <div style="display:flex; flex-direction:column; gap:12px;">
        <div>
          <label class="form-label" for="repo-name">Display Name</label>
          <input id="repo-name" class="input" type="text" bind:value={editName} />
        </div>

        <div>
          <label class="form-label" for="repo-baseurl">Base URL (Base Mirror URL)</label>
          <input id="repo-baseurl" class="input" type="text" bind:value={editBaseurl} placeholder="e.g. http://download.fedoraproject.org/pub/fedora/..." />
        </div>

        <div>
          <label class="form-label" for="repo-metalink">Metalink URL</label>
          <input id="repo-metalink" class="input" type="text" bind:value={editMetalink} placeholder="e.g. https://mirrors.fedoraproject.org/metalink?repo=..." />
        </div>

        <div>
          <label class="form-label" for="repo-mirrorlist">Mirrorlist URL</label>
          <input id="repo-mirrorlist" class="input" type="text" bind:value={editMirrorlist} placeholder="e.g. http://mirrors.fedoraproject.org/mirrorlist?repo=..." />
        </div>

        <div style="display:grid; grid-template-columns:1fr 1fr; gap:16px; align-items:center;">
          <div>
            <label class="form-label" for="repo-priority">Priority (1-99, lower is higher)</label>
            <input id="repo-priority" class="input" type="number" min="1" max="99" bind:value={editPriority} placeholder="99" />
          </div>

          <div style="display:flex; flex-direction:column; gap:4px; justify-content:center;">
            <span class="form-label">GPG Key Verification</span>
            <div style="display:flex; align-items:center; gap:8px;">
              <Toggle bind:checked={editGpgcheck} />
              <span style="font-size:13px; color:var(--color-text-secondary);">
                {editGpgcheck ? 'Enabled' : 'Disabled'}
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- Mirror Latency Speed Test Section -->
      <div class="card" style="padding:16px; display:flex; flex-direction:column; gap:12px; margin-top:8px;">
        <div style="display:flex; justify-content:space-between; align-items:center;">
          <h4 style="margin:0; font-size:14px; font-weight:600; color:var(--color-text-primary); display:flex; align-items:center; gap:6px;">
            <Activity size={16} style="color:var(--color-accent)" /> Mirror Speed Analyzer
          </h4>
          <Button variant="outline" size="sm" onclick={testSpeeds} disabled={testingSpeeds || (!editBaseurl && !editMetalink && !editMirrorlist)}>
            {#if testingSpeeds}
              <RefreshCw size={12} class="animate-spin-slow" /> Testing...
            {:else}
              Run speed test
            {/if}
          </Button>
        </div>

        {#if testingSpeeds}
          <div style="display:flex; align-items:center; justify-content:center; padding:12px; color:var(--color-text-muted); font-size:12px; gap:8px;">
            <RefreshCw size={14} class="animate-spin-slow" /> Pinging connection latency of available repository mirrors...
          </div>
        {:else if speedResults.length > 0}
          <div class="table-wrap" style="max-height:160px; overflow-y:auto; border-radius:6px;">
            <table>
              <thead>
                <tr>
                  <th>Mirror Host URL</th>
                  <th style="width:100px; text-align:center">Latency</th>
                </tr>
              </thead>
              <tbody>
                {#each speedResults as res, index}
                  <tr>
                    <td class="mono-col" style="max-width:280px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title={res.url}>
                      {res.url}
                    </td>
                    <td style="text-align:center">
                      {#if res.speed_ms !== null}
                        <span class="badge {index === 0 ? 'badge-success' : res.speed_ms < 150 ? 'badge-info' : 'badge-warning'}">
                          {res.speed_ms} ms
                        </span>
                      {:else}
                        <span class="badge badge-error">offline</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div style="text-align:center; padding:8px; color:var(--color-text-muted); font-size:12px;">
            Run a speed test to evaluate mirror latencies and connection speeds.
          </div>
        {/if}
      </div>

      <!-- Action Buttons -->
      <div style="display:flex; justify-content:flex-end; gap:10px; margin-top:auto; padding-top:20px; border-top:1px solid var(--color-border);">
        <Button variant="ghost" onclick={() => editOpen = false} disabled={saving}>Cancel</Button>
        <Button variant="primary" onclick={saveRepo} disabled={saving || !editName.trim()}>
          {#if saving}<RefreshCw size={14} class="animate-spin-slow" />{/if}
          Save Changes
        </Button>
      </div>

    </div>
  {/if}
</SideDrawer>

<style>
  .disabled-row td:not(:last-child):not(:nth-last-child(2)) {
    opacity: 0.45;
  }

  .form-label {
    display: block;
    font-size: 12px;
    margin-bottom: 4px;
    color: var(--color-text-secondary);
    font-weight: 500;
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
