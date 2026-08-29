<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { LayoutGrid, Search, Plus, Minus, Package, ExternalLink, RefreshCw, CheckCircle2 } from '@lucide/svelte';
  import { open } from '@tauri-apps/plugin-shell';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';

  interface CoprProject {
    full_name: string;
    description: string;
    chroot_repos: string[];
    contact: string | null;
    homepage: string | null;
    instructions: string | null;
    packages_count: number;
  }

  interface SystemCoprRepo {
    copr_name: string;
    repo_id: string;
    name: string;
    enabled: boolean;
    file_path: string;
    baseurl: string;
  }

  let query = $state('');
  let results = $state<CoprProject[]>([]);
  let loading = $state(false);
  let enablingRepo = $state<string | null>(null);
  let hasSearched = $state(false);

  // System COPR tracking state
  let systemCoprs = $state<SystemCoprRepo[]>([]);
  let loadingSystemCoprs = $state(false);
  let activeSubTab = $state<'search' | 'installed'>('search');

  const installedMap = $derived(
    new Map(systemCoprs.map(c => [c.copr_name.toLowerCase(), c]))
  );

  async function loadSystemCoprs() {
    loadingSystemCoprs = true;
    try {
      systemCoprs = await invoke<SystemCoprRepo[]>('list_system_coprs');
    } catch (e) {
      console.error("Failed to list system COPRs", e);
    } finally {
      loadingSystemCoprs = false;
    }
  }

  $effect(() => {
    loadSystemCoprs();
  });

  async function search() {
    if (!query.trim()) return;
    loading = true;
    hasSearched = true;
    statusStore.setBusy(`Searching Copr for "${query}"…`);
    try {
      results = await invoke<CoprProject[]>('search_copr', { query: query.trim() });
      statusStore.setLastCommand(`copr search ${query}`, 0, true);
    } catch (e) {
      uiStore.addToast(`Copr search failed: ${e}`, 'error');
      statusStore.setLastCommand(`copr search ${query}`, 1, false);
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function enableRepo(repoName: string) {
    enablingRepo = repoName;
    uiStore.confirm(
      'Enable Copr Repository',
      `Enable "${repoName}"?\n\nThis will add the repository to your system via "dnf copr enable". A polkit authentication prompt will appear.`,
      async () => {
        statusStore.setBusy(`Enabling ${repoName}…`);
        try {
          await invoke('enable_copr', { repo: repoName });
          uiStore.addToast(`Copr repo "${repoName}" enabled`, 'success');
          statusStore.setLastCommand(`dnf copr enable ${repoName}`, 0, true);
          await loadSystemCoprs();
        } catch (e) {
          uiStore.addToast(`Failed to enable repo: ${e}`, 'error');
          statusStore.setLastCommand(`dnf copr enable ${repoName}`, 1, false);
        } finally {
          enablingRepo = null;
          statusStore.clearBusy();
        }
      }
    );
    enablingRepo = null;
  }

  async function disableRepo(repoName: string) {
    uiStore.confirm(
      'Disable Copr Repository',
      `Disable "${repoName}"? This will run "dnf copr disable" with polkit authentication.`,
      async () => {
        statusStore.setBusy(`Disabling ${repoName}…`);
        try {
          await invoke('disable_copr', { repo: repoName });
          uiStore.addToast(`Copr repo "${repoName}" disabled`, 'info');
          statusStore.setLastCommand(`dnf copr disable ${repoName}`, 0, true);
          await loadSystemCoprs();
        } catch (e) {
          uiStore.addToast(`Failed to disable repo: ${e}`, 'error');
          statusStore.setLastCommand(`dnf copr disable ${repoName}`, 1, false);
        } finally {
          statusStore.clearBusy();
        }
      },
      true
    );
  }

  interface Props {
    embedded?: boolean;
  }
  let { embedded = true }: Props = $props();

  function quickSearch(tag: string) {
    query = tag;
    search();
  }
</script>

<div class={embedded ? 'copr-tab-content' : 'module-page'} style={embedded ? '' : 'overflow-y: auto; padding-bottom: 24px;'}>
  {#if !embedded}
    <PageHeader title="Copr Browser" icon={LayoutGrid} />
  {/if}

  <!-- Sub-Tabs Navigation -->
  <div style="display:flex; align-items:center; justify-content:space-between; flex-shrink:0; border-bottom:1px solid var(--color-border); padding-bottom:8px; margin-bottom:4px;">
    <div style="display:flex; gap:8px;">
      <button 
        type="button" 
        class="copr-subtab-btn" 
        class:active={activeSubTab === 'search'} 
        onclick={() => activeSubTab = 'search'}
      >
        <Search size={13} /> Search COPR Repos
      </button>
      <button 
        type="button" 
        class="copr-subtab-btn" 
        class:active={activeSubTab === 'installed'} 
        onclick={() => activeSubTab = 'installed'}
      >
        <Package size={13} /> Installed on System ({systemCoprs.length})
      </button>
    </div>

    {#if activeSubTab === 'installed'}
      <Button variant="ghost" class="btn-sm" onclick={loadSystemCoprs} disabled={loadingSystemCoprs}>
        <RefreshCw size={13} class={loadingSystemCoprs ? 'animate-spin-slow' : ''} /> Refresh
      </Button>
    {/if}
  </div>

  {#if activeSubTab === 'installed'}
    <!-- INSTALLED COPRS VIEW -->
    {#if loadingSystemCoprs}
      <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
        <span style="font-weight:500">Checking system COPR repositories…</span>
      </div>
    {:else if systemCoprs.length === 0}
      <EmptyState 
        icon={Package}
        title="No COPR Repositories Installed"
        description="You do not have any community COPR repositories enabled on your system yet."
        actionLabel="Search COPR Repositories"
        onAction={() => activeSubTab = 'search'}
      />
    {:else}
      <div class="copr-results-scroll">
        {#each systemCoprs as sysRepo (sysRepo.repo_id)}
          <div class="card animate-fade-slide copr-card">
            <div class="copr-header">
              <div class="copr-meta">
                <h3 class="copr-name">{sysRepo.copr_name}</h3>
                {#if sysRepo.enabled}
                  <span class="copr-active-badge">
                    <CheckCircle2 size={12} /> Active on System
                  </span>
                {:else}
                  <span class="badge badge-muted" style="font-size:11px;">
                    ⚪ Disabled
                  </span>
                {/if}
              </div>

              <div class="copr-actions">
                <button
                  type="button"
                  class="copr-btn copr-btn-link"
                  title="View on Copr project page"
                  onclick={() => open(`https://copr.fedorainfracloud.org/coprs/${sysRepo.copr_name}/`)}
                >
                  <ExternalLink size={13} />
                </button>

                {#if sysRepo.enabled}
                  <button
                    type="button"
                    class="copr-btn copr-btn-disable"
                    onclick={() => disableRepo(sysRepo.copr_name)}
                    disabled={enablingRepo === sysRepo.copr_name}
                  >
                    <Minus size={13} /> Disable
                  </button>
                {:else}
                  <button
                    type="button"
                    class="copr-btn copr-btn-enable"
                    onclick={() => enableRepo(sysRepo.copr_name)}
                    disabled={enablingRepo === sysRepo.copr_name}
                  >
                    <Plus size={13} /> Enable
                  </button>
                {/if}
              </div>
            </div>

            <div style="font-size:11.5px; color:var(--color-text-muted); font-family:var(--font-mono); word-break:break-all;">
              Repo ID: {sysRepo.repo_id}
            </div>
          </div>
        {/each}
      </div>
    {/if}

  {:else}
    <!-- SEARCH COPRS VIEW -->
    <form class="copr-search-form" onsubmit={(e) => { e.preventDefault(); search(); }}>
      <SearchBar 
        bind:value={query} 
        placeholder="Search Copr projects (e.g. 'vscode', 'neovim', 'gaming', 'hyprland')…" 
        style="flex: 1;" 
      />
      <Button variant="primary" type="submit" disabled={loading || !query.trim()} style="height: 32px; padding: 0 16px;">
        {#if loading}
          <RefreshCw size={13} class="animate-spin-slow" /> <span>Searching…</span>
        {:else}
          <Search size={13} /> <span>Search</span>
        {/if}
      </Button>
    </form>

    {#if loading}
      <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
        <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
          <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
        </div>
        <span style="font-weight:500">Searching Copr API…</span>
      </div>
    {:else if hasSearched && results.length === 0}
      <EmptyState 
        icon={LayoutGrid}
        title="No COPR Projects Found"
        description={`No Copr repositories matched "${query}". Try searching with alternate package names or broader terms.`}
        actionLabel="Clear Search"
        onAction={() => { query = ''; results = []; hasSearched = false; }}
      />
    {:else if results.length > 0}
      <div style="display:flex; align-items:center; justify-content:space-between; flex-shrink:0; margin-bottom:4px">
        <span style="font-size:13px; color:var(--color-text-muted); font-weight:500;">{results.length} project{results.length !== 1 ? 's' : ''} found</span>
      </div>
      <div class="copr-results-scroll">
        {#each results as project (project.full_name)}
          {@const sysRepo = installedMap.get(project.full_name.toLowerCase())}
          <div class="card animate-fade-slide copr-card">
            <div class="copr-header">
              <div class="copr-meta">
                <h3 class="copr-name">{project.full_name}</h3>
                {#if project.packages_count > 0}
                  <span class="badge badge-accent">
                    <Package size={10} /> {project.packages_count} pkg{project.packages_count !== 1 ? 's' : ''}
                  </span>
                {/if}

                {#if sysRepo}
                  {#if sysRepo.enabled}
                    <span class="copr-active-badge">
                      <CheckCircle2 size={12} /> Active on System
                    </span>
                  {:else}
                    <span class="badge badge-muted" style="font-size:11px;">
                      ⚪ Disabled
                    </span>
                  {/if}
                {/if}
              </div>

              <!-- Context-Sensitive Single Action Button -->
              <div class="copr-actions">
                <button
                  type="button"
                  class="copr-btn copr-btn-link"
                  title="View on Copr project page"
                  onclick={() => open(`https://copr.fedorainfracloud.org/coprs/${project.full_name}/`)}
                >
                  <ExternalLink size={13} />
                </button>

                {#if sysRepo && sysRepo.enabled}
                  <!-- IF ENABLED ON SYSTEM: SHOW ONLY DISABLE BUTTON -->
                  <button
                    type="button"
                    class="copr-btn copr-btn-disable"
                    onclick={() => disableRepo(project.full_name)}
                    disabled={enablingRepo === project.full_name}
                  >
                    <Minus size={13} /> Disable
                  </button>
                {:else}
                  <!-- IF NOT ENABLED ON SYSTEM: SHOW ONLY ENABLE BUTTON -->
                  <button
                    type="button"
                    class="copr-btn copr-btn-enable"
                    onclick={() => enableRepo(project.full_name)}
                    disabled={enablingRepo === project.full_name}
                  >
                    <Plus size={13} /> Enable
                  </button>
                {/if}
              </div>
            </div>

            {#if project.description}
              <p class="copr-desc">{project.description}</p>
            {/if}

            {#if project.chroot_repos.length > 0}
              <div class="copr-chroots">
                {#each project.chroot_repos.slice(0, 6) as chroot}
                  <span class="badge badge-muted">{chroot}</span>
                {/each}
                {#if project.chroot_repos.length > 6}
                  <span class="badge badge-muted">+{project.chroot_repos.length - 6} more</span>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="copr-initial-state">
        <EmptyState
          icon={Search}
          title="Search Copr Repositories"
          description="Find community-maintained RPM packages for tools, drivers, and development builds."
        />
        
        <div class="copr-suggestion-box">
          <span class="suggestion-label">Popular Searches:</span>
          <div class="suggestion-chips">
            {#each ['vscode', 'neovim', 'gaming', 'wine', 'docker', 'rust', 'mesa', 'hyprland'] as tag}
              <button type="button" class="copr-chip" onclick={() => quickSearch(tag)}>
                + {tag}
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .copr-search-form {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    flex-shrink: 0;
  }

  .copr-initial-state {
    display: flex;
    flex-direction: column;
    gap: 16px;
    align-items: center;
    width: 100%;
    margin-top: 12px;
  }

  .copr-suggestion-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    padding: 12px 18px;
    border-radius: 10px;
    width: 100%;
    max-width: 540px;
    box-sizing: border-box;
  }

  :global(html.light-mode) .copr-suggestion-box {
    background: #F8FAFC;
    border-color: #E2E8F0;
  }

  .suggestion-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .suggestion-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: center;
  }

  .copr-chip {
    background: rgba(0, 218, 243, 0.08);
    border: 1px solid rgba(0, 218, 243, 0.2);
    color: var(--color-accent);
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 11.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .copr-chip {
    background: #EFF6FF;
    border-color: #BFDBFE;
    color: #2563EB;
  }

  .copr-chip:hover {
    background: var(--color-accent);
    color: #000000;
    transform: translateY(-1px);
  }

  :global(html.light-mode) .copr-chip:hover {
    background: #2563EB;
    color: #FFFFFF;
  }

  .copr-tab-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 4px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 4px;
    padding-bottom: 24px;
  }

  .copr-subtab-btn {
    padding: 6px 14px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--color-text-muted);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: all 0.2s;
  }
  .copr-subtab-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.05);
  }
  .copr-subtab-btn.active {
    color: var(--color-accent);
    background: rgba(59, 130, 246, 0.12);
  }

  .copr-results-scroll {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 6px;
    padding-bottom: 24px;
  }

  .copr-card {
    transition: border-color 0.2s, box-shadow 0.2s, transform 0.15s;
  }
  .copr-card:hover {
    border-color: var(--color-border-hover);
    transform: translateY(-1px);
    box-shadow: 0 8px 24px rgba(0,0,0,0.2);
  }

  .copr-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }

  .copr-meta {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .copr-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-accent);
    margin: 0;
    font-family: var(--font-mono);
  }

  .copr-active-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 9px;
    border-radius: 20px;
    font-size: 11px;
    font-weight: 600;
    background: rgba(16, 185, 129, 0.12);
    color: #10b981;
    border: 1px solid rgba(16, 185, 129, 0.3);
  }
  :global(html.light-mode) .copr-active-badge {
    background: #ECFDF5;
    color: #059669;
    border-color: #6EE7B7;
  }

  .copr-desc {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin: 0 0 10px;
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .copr-chroots {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  /* Action Buttons */
  .copr-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .copr-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    padding: 6px 14px;
    border-radius: 7px;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    outline: none;
    user-select: none;
  }

  .copr-btn-link {
    background: var(--color-surface, rgba(255, 255, 255, 0.05));
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    padding: 6px 10px;
  }
  .copr-btn-link:hover {
    background: rgba(59, 130, 246, 0.12);
    border-color: rgba(59, 130, 246, 0.4);
    color: #3b82f6;
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(59, 130, 246, 0.2);
  }

  .copr-btn-disable {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.28);
    color: #ef4444;
  }
  :global(html.light-mode) .copr-btn-disable {
    background: #FEF2F2;
    border-color: #FCA5A5;
    color: #DC2626;
  }
  .copr-btn-disable:hover {
    background: #EF4444 !important;
    border-color: #EF4444 !important;
    color: #FFFFFF !important;
    transform: translateY(-1.5px);
    box-shadow: 0 4px 14px rgba(239, 68, 68, 0.4);
  }

  .copr-btn-enable {
    background: rgba(16, 185, 129, 0.1);
    border: 1px solid rgba(16, 185, 129, 0.3);
    color: #10b981;
  }
  :global(html.light-mode) .copr-btn-enable {
    background: #ECFDF5;
    border-color: #6EE7B7;
    color: #059669;
  }
  .copr-btn-enable:hover {
    background: #10B981 !important;
    border-color: #10B981 !important;
    color: #FFFFFF !important;
    transform: translateY(-1.5px);
    box-shadow: 0 4px 14px rgba(16, 185, 129, 0.4);
  }
</style>
