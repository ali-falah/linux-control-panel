<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { RefreshCw, Plus, Database, Settings, Activity, ShieldAlert, AlertTriangle, CheckCircle2, Trash2, Zap, FileX } from '@lucide/svelte';
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
    /** dnf5 reports this repo from a package-owned directory; the file is read-only. */
    system_owned: boolean;
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
  let loadError = $state<string | null>(null);
  let filter = $state('');
  let statusFilter = $state<'all' | 'enabled' | 'disabled' | 'errors'>('all');
  const statusFilters = [
    { id: 'all', label: 'Total' },
    { id: 'enabled', label: 'Enabled' },
    { id: 'disabled', label: 'Disabled' },
    { id: 'errors', label: 'Issues' },
  ] as const;
  let showAddDialog = $state(false);
  let addUrl = $state('');
  let addLoading = $state(false);
  let togglingId = $state<string | null>(null);

  // Diagnostics State
  let validating = $state(false);
  let diagnostics = $state<RepoDiagnostic[]>([]);

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

  /** Host part of whichever URL dnf5 reports, for the compact Source column. */
  function repoSource(repo: RepoEntry): { label: string; full: string } {
    const raw = repo.baseurl || repo.metalink || repo.mirrorlist;
    if (!raw) return { label: '—', full: 'dnf5 reports no URL for this repository' };
    try {
      return { label: new URL(raw).host, full: raw };
    } catch {
      return { label: raw, full: raw };
    }
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
  const issueCount = $derived(
    diagnostics.filter(d => d.status !== 'healthy' && d.status !== 'disabled').length
  );

  function matchesStatus(repo: RepoEntry, status: 'all' | 'enabled' | 'disabled' | 'errors'): boolean {
    if (status === 'enabled') return repo.enabled;
    if (status === 'disabled') return !repo.enabled;
    if (status === 'errors') {
      const diag = getDiagnosticForRepo(repo.id, repo.file_path);
      if (diag) {
        return diag.status === 'unreachable' || diag.status === 'corrupted' ||
          diag.status === 'empty' || diag.is_empty_file || diag.is_corrupted_syntax;
      }
      return !repo.baseurl && !repo.metalink && !repo.mirrorlist;
    }
    return true;
  }

  // The text query always applies, whatever status chip is active.
  const filteredRepos = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return repos.filter(repo => {
      const haystack = `${repo.name}\n${repo.id}\n${repo.file_path}\n${repo.baseurl}\n${repo.metalink ?? ''}\n${repo.mirrorlist ?? ''}`.toLowerCase();
      return (!q || haystack.includes(q)) && matchesStatus(repo, statusFilter);
    });
  });

  const statusCounts = $derived({
    all: repos.length,
    enabled: repos.filter(r => r.enabled).length,
    disabled: repos.filter(r => !r.enabled).length,
    errors: repos.filter(r => matchesStatus(r, 'errors')).length,
  });

  type SortKey = 'name' | 'health' | 'source' | 'enabled';
  let sortKey = $state<SortKey>('name');
  let sortAsc = $state(true);

  // Untested and unhealthy rows sort first, so a probe result is visible at
  // the top of the table instead of buried under healthy mirrors.
  const healthRank: Record<RepoDiagnostic['status'], number> = {
    unreachable: 0,
    corrupted: 1,
    empty: 2,
    slow: 3,
    healthy: 4,
    disabled: 5,
  };

  function sortBy(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else {
      sortKey = key;
      sortAsc = true;
    }
  }

  function ariaSort(key: SortKey): 'ascending' | 'descending' | 'none' {
    if (sortKey !== key) return 'none';
    return sortAsc ? 'ascending' : 'descending';
  }

  function sortIndicator(key: SortKey): string {
    if (sortKey !== key) return '↕';
    return sortAsc ? '↑' : '↓';
  }

  const visibleRepos = $derived.by(() => {
    const direction = sortAsc ? 1 : -1;
    return [...filteredRepos].sort((a, b) => {
      let order = 0;
      if (sortKey === 'name') {
        order = a.name.localeCompare(b.name);
      } else if (sortKey === 'source') {
        order = repoSource(a).label.localeCompare(repoSource(b).label);
      } else if (sortKey === 'enabled') {
        order = Number(a.enabled) - Number(b.enabled);
      } else {
        const da = getDiagnosticForRepo(a.id, a.file_path);
        const db = getDiagnosticForRepo(b.id, b.file_path);
        const rankA = da ? healthRank[da.status] : 6;
        const rankB = db ? healthRank[db.status] : 6;
        order = rankA - rankB || (da?.latency_ms ?? 0) - (db?.latency_ms ?? 0);
      }
      return order * direction || a.name.localeCompare(b.name);
    });
  });

  async function loadRepos() {
    loading = true;
    loadError = null;
    statusStore.setBusy('Loading repos…');
    try {
      repos = await invoke<RepoEntry[]>('list_repos');
      statusStore.setLastCommand('dnf5 repo info --all --json', 0, true);
    } catch (e) {
      // dnf4-only hosts answer with one plain-language message; show it as-is
      // instead of prefixing a second error layer.
      loadError = String(e);
      repos = [];
      statusStore.setLastCommand('dnf5 repo info --all --json', 1, false);
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
      // The disable direction returns a CONFIRMATION_REQUIRED advisory (not an
      // error) when disabling would leave zero enabled repos system-wide.
      const result: string = await invoke('toggle_repo', {
        repoId: repo.id,
        enabled: newEnabled,
        filePath: repo.file_path,
        confirmed: false,
      });
      if (result.startsWith('CONFIRMATION_REQUIRED:')) {
        togglingId = null;
        uiStore.confirm(
          newEnabled ? 'Enable Repository' : 'Disable Last Enabled Repository',
          result.slice('CONFIRMATION_REQUIRED:'.length).trim(),
          () => toggleRepoConfirmed(repo),
          true
        );
        return;
      }
      repo.enabled = newEnabled;
      repos = [...repos]; // trigger reactivity
      uiStore.addToast(
        `Repo "${repo.name}" ${newEnabled ? 'enabled' : 'disabled'}`,
        'success'
      );
      statusStore.setLastCommand(`dnf5 config-manager setopt ${repo.id}.enabled=${newEnabled ? '1' : '0'}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to toggle repo: ${e}`, 'error');
      statusStore.setLastCommand(`dnf5 config-manager setopt ${repo.id}.enabled=${newEnabled ? '1' : '0'}`, 1, false);
    } finally {
      togglingId = null;
    }
  }

  async function toggleRepoConfirmed(repo: RepoEntry) {
    statusStore.setBusy(`Toggling repository ${repo.name || repo.id}…`);
    const newEnabled = !repo.enabled;
    try {
      await invoke('toggle_repo', {
        repoId: repo.id,
        enabled: newEnabled,
        filePath: repo.file_path,
        confirmed: true,
      });
      repo.enabled = newEnabled;
      repos = [...repos]; // trigger reactivity
      uiStore.addToast(
        `Repo "${repo.name}" ${newEnabled ? 'enabled' : 'disabled'}`,
        'success'
      );
      statusStore.setLastCommand(
        `dnf5 config-manager setopt ${repo.id}.enabled=${newEnabled ? '1' : '0'}`,
        0,
        true
      );
    } catch (e) {
      uiStore.addToast(`Failed to toggle repo: ${e}`, 'error');
      statusStore.setLastCommand(
        `dnf5 config-manager setopt ${repo.id}.enabled=${newEnabled ? '1' : '0'}`,
        1,
        false
      );
    } finally {
      statusStore.clearBusy();
    }
  }

  async function confirmDeleteRepo(repo: { id: string; file_path: string; name?: string; system_owned?: boolean }) {
    const displayName = repo.name || repo.id;
    if (repo.system_owned) return;
    // The backend always answers the unconfirmed call with a preview and never
    // changes anything; the answer also says when this would leave no enabled
    // repo system-wide.
    statusStore.setBusy(`Preparing to delete ${displayName}…`);
    try {
      const preview: string = await invoke('delete_repo', {
        repoId: repo.id,
        filePath: repo.file_path,
        confirmed: false,
      });
      const detail = preview.startsWith('CONFIRMATION_REQUIRED:')
        ? preview.slice('CONFIRMATION_REQUIRED:'.length).trim()
        : `Are you sure you want to remove this repository configuration from "${repo.file_path}"? This requires root privilege and cannot be undone.`;
      uiStore.confirm(
        `Delete Repository "${displayName}"?`,
        detail,
        () => deleteRepoConfirmed(repo),
        true
      );
    } catch (e) {
      uiStore.addToast(`Failed to delete repo: ${e}`, 'error');
    } finally {
      statusStore.clearBusy();
    }
  }

  async function deleteRepoConfirmed(repo: { id: string; file_path: string; name?: string }) {
    const displayName = repo.name || repo.id;
    statusStore.setBusy(`Deleting repository ${displayName}...`);
    try {
      await invoke('delete_repo', {
        repoId: repo.id,
        filePath: repo.file_path,
        confirmed: true,
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
    // Adding a repository lets its maintainer run code as root on every
    // future update. The backend gates the add on `confirmed` and returns a
    // CONFIRMATION_REQUIRED advisory (not an error) when it is absent.
    try {
      const result: string = await invoke('add_repo', {
        url: addUrl.trim(),
        confirmed: false,
      });
      if (result.startsWith('CONFIRMATION_REQUIRED:')) {
        addLoading = false;
        uiStore.confirm(
          'Add This Repository?',
          result.slice('CONFIRMATION_REQUIRED:'.length).trim(),
          () => addRepoConfirmed(),
          true
        );
        return;
      }
      finishAddRepo(result);
    } catch (e) {
      uiStore.addToast(`Failed to add repo: ${e}`, 'error');
      statusStore.setLastCommand(`dnf config-manager addrepo --from-repofile=${addUrl}`, 1, false);
    } finally {
      addLoading = false;
    }
  }

  async function addRepoConfirmed() {
    statusStore.setBusy('Adding repository…');
    try {
      const result: string = await invoke('add_repo', {
        url: addUrl.trim(),
        confirmed: true,
      });
      finishAddRepo(result);
    } catch (e) {
      uiStore.addToast(`Failed to add repo: ${e}`, 'error');
      statusStore.setLastCommand(`dnf config-manager addrepo --from-repofile=${addUrl}`, 1, false);
    } finally {
      statusStore.clearBusy();
    }
  }

  // Treats a successful add_repo result: refreshes the list and surfaces the
  /// unsigned-repo warning (appended by the backend when gpgcheck=0 is found)
  /// inline instead of hiding it in a success toast.
  function finishAddRepo(result: string) {
    if (result.includes('WARNING: This repository does not use GPG signature verification')) {
      uiStore.addToast('Repository configuration added, but its GPG check is disabled — updates from this source will not be authenticated', 'warning');
    } else {
      // Configuration success is not metadata success: at this point only a
      // .repo/.sources file exists. Reachability (mirror up, metadata
      // downloadable) is not verified until the user runs "Validate" — the
      // toast must not claim a healthy, usable repository.
      uiStore.addToast('Repository configuration added — its metadata was not verified yet. Use "Validate" to confirm the repo is reachable.', 'info');
    }
    showAddDialog = false;
    const addedUrl = addUrl;
    addUrl = '';
    loadRepos();
    statusStore.setLastCommand(`dnf config-manager addrepo --from-repofile=${addedUrl}`, 0, true);
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
    if (selectedRepo.system_owned) {
      uiStore.addToast('This repository file belongs to the distribution and is read-only here. You can still enable or disable it.', 'info');
      return;
    }
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
      statusStore.setLastCommand(`dnf5 repo info --all ${selectedRepo.id} --json`, 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to save repository: ${e}`, 'error');
      statusStore.setLastCommand(`dnf5 repo info --all ${selectedRepo.id} --json`, 1, false);
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
  <PageHeader
    title="Repo Manager"
    subtitle="Repositories dnf5 uses — enable, disable and health-check them"
    icon={Database}
  >
    <div style="display:flex; align-items:center; gap:10px; flex-wrap: wrap;">
      <TabGroup
        size="sm"
        tabs={[
          { id: 'repos', label: 'System Repos' },
          { id: 'copr', label: 'Community (COPR / PPA)' }
        ]}
        bind:activeTab={activeTab}
        onchange={(id) => activeTab = id as any}
      />

      {#if activeTab === 'repos'}
        <Button
          variant="outline"
          size="sm"
          onclick={runDiagnostics}
          disabled={validating || loading}
          title="Probe every mirror for latency, HTTP errors, repomd integrity and syntax"
        >
          <Activity size={14} class={validating ? 'animate-spin-slow' : ''} />
          {validating ? 'Validating…' : 'Validate'}
        </Button>

        <Button variant="primary" size="sm" onclick={() => showAddDialog = true}>
          <Plus size={14} /> Add Repo
        </Button>

        <KebabMenu>
          <button class="menu-item" onclick={makecache} disabled={loading}>
            <RefreshCw size={14} /> Update Metadata Cache
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

  {#if loadError}
    <div class="card" style="padding: 14px 18px; margin-bottom: 16px; display: flex; align-items: flex-start; gap: 12px; border: 1px solid var(--color-border);">
      <AlertTriangle size={18} style="color: var(--color-warning); flex-shrink: 0; margin-top: 2px;" />
      <div>
        <div style="font-weight: 700; font-size: 13.5px; color: var(--color-text-primary); margin-bottom: 4px;">
          Repository management unavailable
        </div>
        <div style="font-size: 13px; color: var(--color-text-secondary);">
          {loadError}
        </div>
      </div>
    </div>
  {/if}

  <!-- Probe result: one line, and only when something is actually wrong -->
  {#if issueCount > 0}
    <div class="diagnostics-alert" role="alert">
      <AlertTriangle size={15} />
      <strong>{issueCount} repositories need attention</strong>
      <span class="diagnostics-detail">
        {#if deadReposCount > 0}{deadReposCount} unreachable{/if}{#if deadReposCount > 0 && slowReposCount > 0} · {/if}{#if slowReposCount > 0}{slowReposCount} slow{/if}
      </span>
      <div class="diagnostics-actions">
        {#if activeDeadReposCount > 0}
          <Button
            variant="outline"
            size="xs"
            onclick={disableAllDeadRepos}
            title="Disable every enabled repository that failed the probe, so dnf cannot hang on it"
          >
            Disable {activeDeadReposCount} dead
          </Button>
        {/if}
        <Button variant="ghost" size="xs" onclick={runDiagnostics} disabled={validating}>
          <RefreshCw size={12} class={validating ? 'animate-spin-slow' : ''} /> Re-validate
        </Button>
      </div>
    </div>
  {/if}

  <!-- Filters and search: one compact row, so the table keeps the page -->
  <div class="controls-row">
    <FilterChips
      options={statusFilters.map(option => ({ ...option, count: statusCounts[option.id] }))}
      bind:value={statusFilter}
      label="Filter repositories by state"
    />

    <div class="tab-actions">
      <SearchBar
        bind:value={filter}
        count={filteredRepos.length}
        total={repos.length}
        placeholder="Filter by name, ID, host or file…"
        style="min-width:200px; max-width:380px; margin:0;"
      />
    </div>
  </div>

  <!-- Repo List -->
  <div class="card repo-table-card" style="padding:0; flex:1; min-height:0; display:flex; flex-direction:column; overflow:hidden;">
    {#if loading}
      <div style="padding: 16px; display: flex; flex-direction: column; gap: 8px;">
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
        <Skeleton height="54px" borderRadius="8px" />
      </div>
    {:else if loadError}
      <EmptyState
        icon={Database}
        title="Repository list unavailable"
        description={loadError}
        style="border:none; background:transparent;"
      />
    {:else if filteredRepos.length === 0}
      <EmptyState
        icon={Database}
        title={filter
          ? 'No matching repositories'
          : statusFilter === 'errors'
            ? 'No repositories need attention'
            : statusFilter === 'disabled'
              ? 'No disabled repositories'
              : statusFilter === 'enabled'
                ? 'No enabled repositories'
                : 'No repositories reported'}
        description={filter
          ? 'Try a different name, ID, host or file path.'
          : statusFilter !== 'all'
            ? 'Nothing matches this filter right now.'
            : 'dnf5 reports no repositories on this system.'}
        actionLabel={!filter && statusFilter === 'all' ? 'Add Your First Repo' : ''}
        onAction={() => (showAddDialog = true)}
        style="border:none; background:transparent;"
      />
    {:else}
      <Table
        tableAction={tableFeatures}
        actionParams={{ sortable: false }}
        style="border:none; border-radius:0;"
      >
        <thead>
          <tr>
            <th
              style="width:36%"
              class:sort-asc={sortKey === 'name' && sortAsc}
              class:sort-desc={sortKey === 'name' && !sortAsc}
              aria-sort={ariaSort('name')}
            >
              <button type="button" class="th-sort-btn" onclick={() => sortBy('name')}>
                Repository <span class="sort-indicator">{sortIndicator('name')}</span>
              </button>
            </th>
            <th
              style="width:16%"
              class:sort-asc={sortKey === 'health' && sortAsc}
              class:sort-desc={sortKey === 'health' && !sortAsc}
              aria-sort={ariaSort('health')}
              title="Latency and reachability from the last Validate run"
            >
              <button type="button" class="th-sort-btn" onclick={() => sortBy('health')}>
                Health <span class="sort-indicator">{sortIndicator('health')}</span>
              </button>
            </th>
            <th
              style="width:28%"
              class:sort-asc={sortKey === 'source' && sortAsc}
              class:sort-desc={sortKey === 'source' && !sortAsc}
              aria-sort={ariaSort('source')}
            >
              <button type="button" class="th-sort-btn" onclick={() => sortBy('source')}>
                Source <span class="sort-indicator">{sortIndicator('source')}</span>
              </button>
            </th>
            <th
              style="width:10%; text-align:center"
              class:sort-asc={sortKey === 'enabled' && sortAsc}
              class:sort-desc={sortKey === 'enabled' && !sortAsc}
              aria-sort={ariaSort('enabled')}
            >
              <button type="button" class="th-sort-btn" style="justify-content:center" onclick={() => sortBy('enabled')}>
                Enabled <span class="sort-indicator">{sortIndicator('enabled')}</span>
              </button>
            </th>
            <!-- The floating density control of the shared Table sits in this
                 corner; the header keeps its label for screen readers only. -->
            <th style="width:10%; text-align:right" aria-label="Actions"></th>
          </tr>
        </thead>
        <tbody>
          {#each visibleRepos as repo (repo.id)}
            {@const diag = getDiagnosticForRepo(repo.id, repo.file_path)}
            {@const source = repoSource(repo)}
            <tr>
              <td>
                <div class="repo-name" class:off={!repo.enabled} title={repo.name}>
                  <span class="repo-name-text">{repo.name}</span>
                  {#if repo.system_owned}
                    <Badge
                      variant="outline"
                      title="Provided by your distribution — the file is read-only, but the repository can still be enabled or disabled"
                    >
                      System
                    </Badge>
                  {/if}
                </div>
                <div class="repo-id" title={repo.file_path}>{repo.id}</div>
              </td>
              <td>
                {#if diag?.status === 'healthy'}
                  <Badge variant="success" title={`Mirror responding in ${diag.latency_ms} ms`}>
                    <CheckCircle2 size={11} /> {diag.latency_ms} ms
                  </Badge>
                {:else if diag?.status === 'slow'}
                  <Badge variant="warning" title={`${diag.error_message || 'Sluggish mirror'} (${diag.latency_ms} ms)`}>
                    <Zap size={11} /> {diag.latency_ms} ms slow
                  </Badge>
                {:else if diag?.status === 'unreachable'}
                  <Badge variant="error" title={diag.error_message || 'Host unreachable or HTTP error'}>
                    <AlertTriangle size={11} /> {diag.http_status ? `HTTP ${diag.http_status}` : 'Unreachable'}
                  </Badge>
                {:else if diag?.status === 'empty'}
                  <Badge variant="error" title="Empty 0-byte repository file">
                    <FileX size={11} /> Empty file
                  </Badge>
                {:else if diag?.status === 'corrupted'}
                  <Badge variant="error" title={diag.error_message || 'Corrupted repository metadata'}>
                    <ShieldAlert size={11} /> Corrupted
                  </Badge>
                {:else}
                  <span class="health-na" title="Run Validate to probe this mirror">—</span>
                {/if}
              </td>
              <td>
                <span class="repo-source" title={source.full}>{source.label}</span>
              </td>
              <td style="text-align:center">
                <!-- onclick (spread onto the button) replaces Toggle's own flip, so
                     the switch shows the state the backend confirmed, not an
                     optimistic guess that may be reverted on failure. -->
                <Toggle
                  checked={repo.enabled}
                  disabled={togglingId === repo.id}
                  title={`${repo.enabled ? 'Disable' : 'Enable'} ${repo.id}`}
                  onclick={() => toggleRepo(repo)}
                />
              </td>
              <td>
                <div class="row-actions">
                  <button
                    type="button"
                    class="action-btn"
                    aria-label={`Configure ${repo.id}`}
                    title="Configure repository"
                    onclick={() => openEditDrawer(repo)}
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
                    <button
                      class="menu-item"
                      style="color: var(--color-error);"
                      onclick={() => !repo.system_owned && confirmDeleteRepo(repo)}
                      disabled={repo.system_owned}
                      title={repo.system_owned ? 'This file belongs to your distribution and cannot be deleted here' : 'Delete this repository'}
                    >
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
        <Input
          id="add-repo-url"
          bind:value={addUrl}
          label="Repository URL"
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
<SideDrawer
  bind:isOpen={editOpen}
  title={selectedRepo ? `Repository — ${selectedRepo.name}` : 'Repository'}
  subtitle={selectedRepo?.id ?? ''}
  width="550px"
>
  {#snippet headerActions()}
    {#if selectedRepo}
      <Button
        variant="ghost"
        size="sm"
        style="color: var(--color-error);"
        onclick={() => !selectedRepo!.system_owned && confirmDeleteRepo(selectedRepo!)}
        disabled={selectedRepo.system_owned}
        title={selectedRepo.system_owned
          ? 'This file belongs to your distribution and cannot be deleted here'
          : 'Delete this repository'}
      >
        <Trash2 size={14} /> Delete
      </Button>
      <Button variant="outline" size="sm" onclick={() => (editOpen = false)} disabled={saving}>
        Cancel
      </Button>
      <Button
        variant="primary"
        size="sm"
        onclick={saveRepo}
        disabled={saving || selectedRepo.system_owned || !editName.trim()}
      >
        {saving ? 'Saving…' : 'Save'}
      </Button>
    {/if}
  {/snippet}

  {#if selectedRepo}
    <div style="display:flex; flex-direction:column; gap:16px; flex:1;">

      <!-- Top info -->
      <div style="background:rgba(255,255,255,0.02); border:1px solid var(--color-border); border-radius:8px; padding:12px; font-size:12px;">
        <div style="display:flex; justify-content:space-between; gap:12px;">
          <span style="color:var(--color-text-muted); flex-shrink:0;">Config File</span>
          <code style="color:var(--color-text-secondary); text-overflow:ellipsis; overflow:hidden; white-space:nowrap; max-width:320px;" title={selectedRepo.file_path}>
            {selectedRepo.file_path}
          </code>
        </div>
      </div>

      <!-- Read-only notice for distribution-owned repositories -->
      {#if selectedRepo.system_owned}
        <div style="display:flex; align-items:flex-start; gap:8px; background:rgba(255,255,255,0.03); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; font-size:12px; color:var(--color-text-secondary);">
          <ShieldAlert size={15} style="color:var(--color-warning); flex-shrink:0; margin-top:1px;" />
          <span>
            This repository is provided by your distribution. Its file is read-only here, so the fields below cannot be changed. You can still enable or disable it from the repository list.
          </span>
        </div>
      {/if}

      <!-- Form Inputs -->
      <div style="display:flex; flex-direction:column; gap:12px;">
        <div>
          <label class="form-label" for="repo-name">Display Name</label>
          <input id="repo-name" class="input" type="text" bind:value={editName} disabled={selectedRepo.system_owned} />
        </div>

        <div>
          <label class="form-label" for="repo-baseurl">Base URL (Base Mirror URL)</label>
          <input id="repo-baseurl" class="input" type="text" bind:value={editBaseurl} placeholder="e.g. http://download.fedoraproject.org/pub/fedora/..." disabled={selectedRepo.system_owned} />
        </div>

        <div>
          <label class="form-label" for="repo-metalink">Metalink URL</label>
          <input id="repo-metalink" class="input" type="text" bind:value={editMetalink} placeholder="e.g. https://mirrors.fedoraproject.org/metalink?repo=..." disabled={selectedRepo.system_owned} />
        </div>

        <div>
          <label class="form-label" for="repo-mirrorlist">Mirrorlist URL</label>
          <input id="repo-mirrorlist" class="input" type="text" bind:value={editMirrorlist} placeholder="e.g. http://mirrors.fedoraproject.org/mirrorlist?repo=..." disabled={selectedRepo.system_owned} />
        </div>

        <div style="display:grid; grid-template-columns:1fr 1fr; gap:16px; align-items:center;">
          <div>
            <label class="form-label" for="repo-priority">Priority (1-99, lower is higher)</label>
            <input id="repo-priority" class="input" type="number" min="1" max="99" bind:value={editPriority} placeholder="99" disabled={selectedRepo.system_owned} />
          </div>

          <div style="display:flex; flex-direction:column; gap:4px; justify-content:center;">
            <span class="form-label">GPG Key Verification</span>
            <div style="display:flex; align-items:center; gap:8px;">
              <Toggle bind:checked={editGpgcheck} disabled={selectedRepo.system_owned} />
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

    </div>
  {/if}
</SideDrawer>

<style>
  .form-label {
    display: block;
    font-size: 12px;
    margin-bottom: 4px;
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  /* One-line probe result; only rendered when a probe found problems. */
  .diagnostics-alert {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    margin-bottom: 12px;
    background: var(--color-warning-muted);
    border-left: 3px solid var(--color-warning);
    border-radius: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .diagnostics-alert strong {
    color: var(--color-text-primary);
    font-weight: 600;
  }

  .diagnostics-detail {
    color: var(--color-text-muted);
  }

  .diagnostics-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .repo-name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .repo-name.off {
    color: var(--color-text-muted);
  }

  .repo-name-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-id {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-source {
    display: block;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
  }

  .health-na {
    color: var(--color-text-muted);
  }

  /* Table owns its column widths: without a fixed layout the nowrap names and
     ids force the table past the card and push every other column off-screen. */
  :global(.repo-table-card table) {
    table-layout: fixed;
    width: 100%;
  }
</style>
