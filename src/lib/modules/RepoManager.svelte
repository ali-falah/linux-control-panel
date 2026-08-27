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
  import { Package, RefreshCw, Plus, ToggleLeft, ToggleRight, Link, Search, Database, Settings, Activity, ShieldAlert, AlertTriangle, CheckCircle2, Trash2, Zap, HelpCircle, FileX } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import CoprBrowser from './CoprBrowser.svelte';
  import { portal } from '../actions/portal.ts';

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

  interface RepoDiagnostic {
    repo_id: string;
    name: string;
    file_path: string;
    enabled: boolean;
    status: 'healthy' | 'slow' | 'unreachable' | 'corrupted' | 'empty' | 'disabled';
    latency_ms: number | null;
    http_status: number | null;
    repomd_valid: boolean;
    error_message: string | null;
    tested_url: string | null;
    is_empty_file: boolean;
    is_corrupted_syntax: boolean;
  }

  let activeTab = $state<'repos' | 'copr'>(
    uiStore.targetSubTab && ['repos', 'copr'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'repos'
  );
  if (uiStore.targetSubTab && ['repos', 'copr'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  let repos = $state<RepoEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let statusFilter = $state<'all' | 'enabled' | 'disabled' | 'errors' | 'slow' | 'unreachable'>('all');
  let showAddDialog = $state(false);
  let addUrl = $state('');
  let addLoading = $state(false);
  let togglingId = $state<string | null>(null);

  // Diagnostics State
  let validating = $state(false);
  let diagnostics = $state<RepoDiagnostic[]>([]);
  let showDiagnosticsBanner = $state(false);

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

  function getDiagnosticForRepo(repoId: string, filePath: string): RepoDiagnostic | undefined {
    return diagnostics.find(d => 
      d.repo_id === repoId || 
      d.file_path === filePath ||
      (Boolean(filePath) && Boolean(d.file_path) && (d.file_path.endsWith('/' + repoId) || filePath.endsWith('/' + d.repo_id)))
    );
  }

  const deadReposCount = $derived(
    diagnostics.filter(d => d.status === 'unreachable' || d.status === 'corrupted' || d.status === 'empty').length
  );
  const activeDeadReposCount = $derived(
    diagnostics.filter(d => (d.status === 'unreachable' || d.status === 'corrupted' || d.status === 'empty') && d.enabled).length
  );
  const slowReposCount = $derived(
    diagnostics.filter(d => d.status === 'slow').length
  );
  const healthyReposCount = $derived(
    diagnostics.filter(d => d.status === 'healthy').length
  );

  const filteredRepos = $derived(
    repos.filter(r => {
      if (statusFilter === 'enabled' && !r.enabled) return false;
      if (statusFilter === 'disabled' && r.enabled) return false;
      if (statusFilter === 'healthy') {
        const diag = getDiagnosticForRepo(r.id, r.file_path);
        return diag?.status === 'healthy';
      }
      if (statusFilter === 'errors') {
        const diag = getDiagnosticForRepo(r.id, r.file_path);
        if (diag) {
          return diag.status === 'unreachable' || diag.status === 'corrupted' || diag.status === 'empty' || diag.is_empty_file || diag.is_corrupted_syntax;
        }
        return !r.baseurl && !r.metalink && !r.mirrorlist;
      }
      if (statusFilter === 'slow') {
        const diag = getDiagnosticForRepo(r.id, r.file_path);
        return diag?.status === 'slow';
      }
      if (statusFilter === 'unreachable') {
        const diag = getDiagnosticForRepo(r.id, r.file_path);
        return diag?.status === 'unreachable';
      }

      const q = filter.toLowerCase();
      return !q || r.name.toLowerCase().includes(q) || r.id.toLowerCase().includes(q) || r.file_path.toLowerCase().includes(q);
    })
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

  async function runDiagnostics() {
    validating = true;
    statusStore.setBusy('Validating & probing all DNF repository mirrors...');
    try {
      diagnostics = await invoke<RepoDiagnostic[]>('validate_all_repos');
      showDiagnosticsBanner = true;
      const issues = diagnostics.filter(d => d.status !== 'healthy' && d.status !== 'disabled').length;
      if (issues > 0) {
        uiStore.addToast(`Diagnostics complete: Found ${issues} problematic repo(s) (slow, corrupted, or 404).`, 'warning');
      } else {
        uiStore.addToast(`All active repositories tested healthy and responsive!`, 'success');
      }
      statusStore.setLastCommand('dnf check-update / validate repos', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to validate repos: ${e}`, 'error');
      statusStore.setLastCommand('dnf validate repos', 1, false);
    } finally {
      validating = false;
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

  function confirmDeleteRepo(repo: { id: string; file_path: string; name?: string }) {
    const displayName = repo.name || repo.id;
    uiStore.confirm(
      `Delete Repository "${displayName}"?`,
      `Are you sure you want to remove this repository configuration from "${repo.file_path}"? This requires root privilege and cannot be undone.`,
      async () => {
        statusStore.setBusy(`Deleting repository ${displayName}...`);
        try {
          await invoke('delete_repo', {
            repoId: repo.id,
            filePath: repo.file_path,
          });
          uiStore.addToast(`Repository "${displayName}" deleted successfully`, 'success');
          await loadRepos();
          if (diagnostics.length > 0) {
            diagnostics = diagnostics.filter(d => !(d.repo_id === repo.id && d.file_path === repo.file_path));
          }
          if (editOpen && selectedRepo?.id === repo.id) {
            editOpen = false;
          }
        } catch (e) {
          uiStore.addToast(`Failed to delete repo: ${e}`, 'error');
        } finally {
          statusStore.clearBusy();
        }
      },
      true
    );
  }

  async function disableAllDeadRepos() {
    const dead = diagnostics.filter(d => (d.status === 'unreachable' || d.status === 'corrupted' || d.status === 'empty') && d.enabled);
    if (dead.length === 0) {
      uiStore.addToast('No active dead repositories to disable', 'info');
      return;
    }

    uiStore.confirm(
      `Disable ${dead.length} Dead / Unreachable Repositories?`,
      `This will set 'enabled=0' for all ${dead.length} failing repositories to prevent DNF upgrade hangs and errors.`,
      async () => {
        statusStore.setBusy(`Disabling ${dead.length} dead repositories...`);
        try {
          const targets = dead.map(d => [d.repo_id, d.file_path]);
          const disabledCount = await invoke<number>('bulk_disable_repos', { repoTargets: targets });
          uiStore.addToast(`Successfully disabled ${disabledCount} dead repositories!`, 'success');
          await loadRepos();
          await runDiagnostics();
        } catch (e) {
          uiStore.addToast(`Failed to bulk disable repos: ${e}`, 'error');
        } finally {
          statusStore.clearBusy();
        }
      },
      false
    );
  }

  async function cleanCacheForRepo(repoId: string) {
    statusStore.setBusy(`Cleaning cache for repo "${repoId}"...`);
    try {
      await invoke('clean_repo_cache', { repoId });
      uiStore.addToast(`Cache cleaned for "${repoId}"`, 'success');
    } catch (e) {
      uiStore.addToast(`Failed to clean cache: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
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
  <PageHeader title="Repo Manager" icon={Package}>
    <div style="display:flex; align-items:center; gap:10px; flex-wrap: wrap;">
      <TabGroup
        tabs={[
          { id: 'repos', label: 'RPM Repos' },
          { id: 'copr', label: 'COPR Packages' }
        ]}
        bind:activeTab={activeTab}
        onchange={(id) => activeTab = id as any}
      />

      {#if activeTab === 'repos'}
        <button
          type="button"
          class="btn {validating ? 'btn-ghost' : 'btn-secondary'} btn-sm"
          onclick={runDiagnostics}
          disabled={validating || loading}
          title="Probe all repository mirrors for latency, HTTP 404s, repomd integrity, and syntax errors"
        >
          <Activity size={14} class={validating ? 'animate-spin-slow text-accent' : 'text-accent'} />
          <span>{validating ? 'Validating Mirrors...' : 'Validate & Diagnose Repos'}</span>
        </button>

        <Button variant="primary" onclick={() => showAddDialog = true}>
          <Plus size={14} /> Add Repo
        </Button>

        <KebabMenu>
          <button class="menu-item" onclick={runDiagnostics} disabled={validating}>
            <Activity size={14} /> Test All Mirror Speeds
          </button>
          <button class="menu-item" onclick={makecache} disabled={loading}>
            <RefreshCw size={14} /> Run dnf makecache
          </button>
          <button class="menu-item" onclick={loadRepos} disabled={loading}>
            <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh Repo List
          </button>
        </KebabMenu>
      {/if}
    </div>
  </PageHeader>

  {#if activeTab === 'copr'}
    <CoprBrowser />
  {:else}

  <!-- Diagnostics & Mirror Health Overview Banner -->
  {#if diagnostics.length > 0}
    <div class="card diagnostics-banner" style="padding: 14px 18px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 12px; background: rgba(15, 23, 42, 0.6); border: 1px solid var(--color-border);">
      <div style="display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
        <div style="display: flex; align-items: center; gap: 8px;">
          <Activity size={18} style="color: var(--color-accent);" />
          <span style="font-weight: 700; font-size: 13.5px; color: var(--color-text-primary);">
            Repository Health &amp; Mirror Speed Diagnostics
          </span>
          <span class="badge badge-info" style="font-size: 11px;">{diagnostics.length} Probed</span>
        </div>
        <div style="display: flex; align-items: center; gap: 8px;">
          {#if activeDeadReposCount > 0}
            <button
              type="button"
              class="btn btn-warning btn-sm"
              onclick={disableAllDeadRepos}
              title="Bulk disable all active unreachable, 404, or corrupted repositories to prevent DNF hangs"
            >
              <AlertTriangle size={13} />
              <span>Disable {activeDeadReposCount} Active Dead Repos</span>
            </button>
          {/if}
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            onclick={runDiagnostics}
            disabled={validating}
            title="Re-run speed and health tests on all repository mirrors"
          >
            <RefreshCw size={13} class={validating ? 'animate-spin-slow' : ''} />
            <span>Re-validate</span>
          </button>
        </div>
      </div>

      <!-- Quick Metrics Grid -->
      <div class="diag-metrics-grid">
        <button
          type="button"
          class="diag-metric-card"
          class:active={statusFilter === 'all'}
          onclick={() => statusFilter = 'all'}
        >
          <span class="metric-num">{diagnostics.length}</span>
          <span class="metric-lbl">Total Probed</span>
        </button>
        <button
          type="button"
          class="diag-metric-card healthy"
          class:active={statusFilter === 'healthy'}
          onclick={() => statusFilter = statusFilter === 'healthy' ? 'all' : 'healthy'}
        >
          <span class="metric-num text-success">{healthyReposCount}</span>
          <span class="metric-lbl">Healthy (&lt;1.5s)</span>
        </button>
        <button
          type="button"
          class="diag-metric-card slow"
          class:active={statusFilter === 'slow'}
          onclick={() => statusFilter = statusFilter === 'slow' ? 'all' : 'slow'}
        >
          <span class="metric-num text-warn">{slowReposCount}</span>
          <span class="metric-lbl">Slow Latency</span>
        </button>
        <button
          type="button"
          class="diag-metric-card dead"
          class:active={statusFilter === 'errors'}
          onclick={() => statusFilter = statusFilter === 'errors' ? 'all' : 'errors'}
        >
          <span class="metric-num text-danger">{deadReposCount}</span>
          <span class="metric-lbl">Dead / Corrupted</span>
        </button>
      </div>
    </div>
  {/if}

  <!-- Controls: Stats & Search -->
  <div style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    {#if repos.length > 0}
      <div class="stat-cards" style="margin: 0;">
        <button
          onclick={() => statusFilter = 'all'}
          class="stat-card"
          class:active={statusFilter === 'all'}
        >
          <span class="stat-value">{repos.length}</span>
          <span class="stat-label">Total</span>
        </button>
        <button
          onclick={() => statusFilter = statusFilter === 'enabled' ? 'all' : 'enabled'}
          class="stat-card"
          class:active={statusFilter === 'enabled'}
        >
          <span class="stat-value enabled">{repos.filter(r => r.enabled).length}</span>
          <span class="stat-label">Enabled</span>
        </button>
        <button
          onclick={() => statusFilter = statusFilter === 'disabled' ? 'all' : 'disabled'}
          class="stat-card"
          class:active={statusFilter === 'disabled'}
        >
          <span class="stat-value disabled">{repos.filter(r => !r.enabled).length}</span>
          <span class="stat-label">Disabled</span>
        </button>
        <button
          onclick={() => statusFilter = statusFilter === 'errors' ? 'all' : 'errors'}
          class="stat-card"
          class:active={statusFilter === 'errors'}
        >
          <span class="stat-value errors">
            {diagnostics.length > 0 ? deadReposCount : repos.filter(r => !r.baseurl && !r.metalink && !r.mirrorlist).length}
          </span>
          <span class="stat-label">Issues</span>
        </button>
      </div>
    {/if}

    <!-- Search -->
    <SearchBar bind:value={filter} placeholder="Filter repositories by ID, name, file path..." style="flex:1; min-width:200px; margin: 0;" />
  </div>

  <!-- Repo List -->
  <div class="card" style="padding:0; flex:1; min-height:0; display:flex; flex-direction:column; overflow:hidden;">
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
          {filter ? 'No matching repositories found' : (statusFilter === 'errors' ? 'No Dead or Corrupted Repositories' : (statusFilter === 'slow' ? 'No Slow Repositories' : (statusFilter === 'disabled' ? 'No Disabled Repositories' : (statusFilter === 'enabled' ? 'No Enabled Repositories' : 'No Repositories'))))}
        </span>
        <span style="color:var(--color-text-muted); margin-top:8px;">
          {filter ? 'Try adjusting your search criteria.' : (statusFilter !== 'all' ? 'All repositories match your current healthy operating criteria.' : 'No repositories found in /etc/yum.repos.d/.')}
        </span>
        {#if !filter && statusFilter === 'all'}
          <Button variant="outline" style="margin-top:24px;" onclick={() => showAddDialog = true}>
            <Plus size={14} /> Add Your First Repo
          </Button>
        {/if}
      </div>
    {:else}
      <Table tableAction={tableFeatures} style="min-width: 950px; border:none; border-radius:0;">
        <thead>
          <tr>
            <th>Repository</th>
            <th>ID</th>
            <th>Health &amp; Latency</th>
            <th>URL / Metalink</th>
            <th style="text-align:center">Priority</th>
            <th style="text-align:center">GPG</th>
            <th style="text-align:center">Enabled</th>
            <th style="width:70px; text-align:center">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredRepos as repo (repo.id)}
            {@const diag = getDiagnosticForRepo(repo.id, repo.file_path)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <tr class:disabled-row={!repo.enabled} onclick={() => openEditDrawer(repo)} style="cursor: pointer;">
              <td style="max-width:230px;">
                <div style="font-weight:600; color:var(--color-text-primary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title={repo.name}>
                  {repo.name}
                </div>
                <div style="font-size:11px; color:var(--color-text-muted); font-family:var(--font-mono); overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title={repo.file_path}>
                  {repo.file_path.split('/').pop()}
                </div>
              </td>
              <td style="max-width:160px;">
                <code style="font-size:11px; color:var(--color-text-accent); display:block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title={repo.id}>
                  {repo.id}
                </code>
              </td>
              <td style="min-width:130px;">
                {#if diag}
                  {#if diag.status === 'healthy'}
                    <span class="badge badge-success" title="Mirror responding fast ({diag.latency_ms}ms)">
                      <CheckCircle2 size={11} /> {diag.latency_ms}ms
                    </span>
                  {:else if diag.status === 'slow'}
                    <span class="badge badge-warning" title="{diag.error_message || 'Sluggish mirror'} ({diag.latency_ms}ms)">
                      <Zap size={11} /> {diag.latency_ms}ms (Slow)
                    </span>
                  {:else if diag.status === 'unreachable'}
                    <span class="badge badge-error" title="{diag.error_message || 'HTTP 404 / Host unreachable'}">
                      <AlertTriangle size={11} /> {diag.http_status ? `HTTP ${diag.http_status}` : 'Dead/404'}
                    </span>
                  {:else if diag.status === 'empty'}
                    <span class="badge badge-error" title="Empty 0-byte file (corrupted)">
                      <FileX size={11} /> 0-byte File
                    </span>
                  {:else if diag.status === 'corrupted'}
                    <span class="badge badge-error" title="{diag.error_message}">
                      <ShieldAlert size={11} /> Corrupted
                    </span>
                  {:else}
                    <span class="badge badge-muted">Disabled</span>
                  {/if}
                {:else if !repo.enabled}
                  <span class="badge badge-muted">Disabled</span>
                {:else}
                  <span class="badge badge-info" style="opacity:0.7">Ready</span>
                {/if}
              </td>
              <td style="max-width:200px;">
                <div style="font-size:11px; color:var(--color-text-secondary); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-family:var(--font-mono);" title={repo.metalink ?? repo.mirrorlist ?? repo.baseurl ?? '—'}>
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
              <td style="text-align:center" onclick={(e) => e.stopPropagation()}>
                <div style="display:flex; align-items:center; justify-content:center; gap:2px;">
                  <button 
                    class="action-btn"
                    onclick={() => openEditDrawer(repo)}
                    title="Configure repository"
                  >
                    <Settings size={14} />
                  </button>
                  <KebabMenu align="right">
                    <button class="menu-item" onclick={() => openEditDrawer(repo)}>
                      <Settings size={14} /> Edit Configuration
                    </button>
                    <button class="menu-item" onclick={() => cleanCacheForRepo(repo.id)}>
                      <RefreshCw size={14} /> Clean Expired Cache
                    </button>
                    <button class="menu-item" onclick={() => { openEditDrawer(repo); testSpeeds(); }}>
                      <Activity size={14} /> Test Mirror Speeds
                    </button>
                    <div style="height:1px; background:var(--color-border); margin:4px 0;"></div>
                    <button class="menu-item text-danger" onclick={() => confirmDeleteRepo(repo)}>
                      <Trash2 size={14} /> Delete Repository
                    </button>
                  </KebabMenu>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </Table>
      {/if}
  </div>
{/if}
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
    use:portal
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
      <div style="display:flex; justify-content:space-between; align-items:center; gap:10px; margin-top:auto; padding-top:20px; border-top:1px solid var(--color-border);">
        <button
          type="button"
          class="btn btn-ghost btn-sm text-danger"
          onclick={() => selectedRepo && confirmDeleteRepo(selectedRepo)}
          title="Delete this repository"
        >
          <Trash2 size={14} /> Delete Repo
        </button>

        <div style="display:flex; gap:8px;">
          <Button variant="ghost" onclick={() => editOpen = false} disabled={saving}>Cancel</Button>
          <Button variant="primary" onclick={saveRepo} disabled={saving || !editName.trim()}>
            {#if saving}<RefreshCw size={14} class="animate-spin-slow" />{/if}
            Save Changes
          </Button>
        </div>
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

  /* Diagnostics Banner & Metrics Grid */
  .diagnostics-banner {
    border-radius: 12px;
  }

  .diag-metrics-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: 10px;
  }

  .diag-metric-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 8px 12px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.18s ease;
  }

  .diag-metric-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border-hover);
  }

  .diag-metric-card.active {
    border-color: var(--color-accent);
    background: rgba(0, 218, 243, 0.08);
  }

  .diag-metric-card .metric-num {
    font-size: 18px;
    font-weight: 700;
    line-height: 1.2;
  }

  .diag-metric-card .metric-lbl {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-top: 2px;
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
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    color: var(--color-text-primary);
    font-family: inherit;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .stat-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-border-hover);
  }
  .stat-card.active {
    background: var(--color-accent) !important;
    border-color: var(--color-accent) !important;
    color: #FFFFFF !important;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.25) !important;
  }
  .stat-card.active * {
    color: #FFFFFF !important;
  }
  .stat-card.active .stat-value,
  .stat-card.active .stat-label {
    color: #FFFFFF !important;
    font-weight: 700;
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
