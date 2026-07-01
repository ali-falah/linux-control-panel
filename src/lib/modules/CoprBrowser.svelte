<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { LayoutGrid, Search, Plus, Minus, Package, ExternalLink, RefreshCw } from '@lucide/svelte';
  import { open } from '@tauri-apps/plugin-shell';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface CoprProject {
    full_name: string;
    description: string;
    chroot_repos: string[];
    contact: string | null;
    homepage: string | null;
    instructions: string | null;
    packages_count: number;
  }

  let query = $state('');
  let results = $state<CoprProject[]>([]);
  let loading = $state(false);
  let enablingRepo = $state<string | null>(null);
  let hasSearched = $state(false);

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
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><LayoutGrid size={20} /></div>
    <div>
      <h1 class="module-title">Copr Browser</h1>
      <p class="module-subtitle">Search and manage Fedora Copr repositories</p>
    </div>
  </div>

  <!-- Search Box -->
  <div style="display:flex; gap:8px">
    <div class="search-bar" style="flex:1">
      <Search size={14} style="color:var(--color-text-muted)" />
      <input
        bind:value={query}
        placeholder="Search Copr projects (e.g. 'vscode', 'neovim', 'gaming')…"
        onkeydown={(e) => e.key === 'Enter' && search()}
      />
    </div>
    <button class="btn btn-primary" onclick={search} disabled={loading || !query.trim()}>
      {#if loading}
        <RefreshCw size={14} class="animate-spin-slow" /> Searching…
      {:else}
        <Search size={14} /> Search
      {/if}
    </button>
  </div>

  <!-- Results -->
  {#if loading}
    <div class="card" style="display:flex; align-items:center; justify-content:center; gap:12px; padding:40px; color:var(--color-text-muted)">
      <RefreshCw size={20} class="animate-spin-slow" />
      <span>Searching Copr API…</span>
    </div>
  {:else if hasSearched && results.length === 0}
    <div class="empty-state card">
      <LayoutGrid size={40} class="empty-state-icon" />
      <span>No Copr projects found for "{query}"</span>
    </div>
  {:else if results.length > 0}
    <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:4px">
      <span style="font-size:13px; color:var(--color-text-muted)">{results.length} project{results.length !== 1 ? 's' : ''} found</span>
    </div>
    <div class="module-content-scroll" style="display:flex; flex-direction:column; gap:10px; padding-bottom:12px; padding-right:6px;">
      {#each results as project (project.full_name)}
        <div class="card animate-fade-slide copr-card">
          <div class="copr-header">
            <div class="copr-meta">
              <h3 class="copr-name">{project.full_name}</h3>
              {#if project.packages_count > 0}
                <span class="badge badge-accent">
                  <Package size={10} /> {project.packages_count} pkg{project.packages_count !== 1 ? 's' : ''}
                </span>
              {/if}
            </div>
            <div style="display:flex; gap:6px; flex-shrink:0">
              <!-- Always show link to Copr project page -->
              <button
                class="btn btn-sm btn-ghost"
                title="View on Copr"
                onclick={() => open(`https://copr.fedorainfracloud.org/coprs/${project.full_name}/`)}
              >
                <ExternalLink size={12} />
              </button>

              <button
                class="btn btn-sm btn-danger"
                onclick={() => disableRepo(project.full_name)}
              >
                <Minus size={12} /> Disable
              </button>
              <button
                class="btn btn-sm btn-primary"
                onclick={() => enableRepo(project.full_name)}
              >
                <Plus size={12} /> Enable
              </button>
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
    <div class="empty-state card" style="border-style:dashed">
      <Search size={40} class="empty-state-icon" />
      <span>Search for Copr projects above</span>
      <span style="font-size:12px">Try "vscode", "gaming", "llvm", "wine"…</span>
    </div>
  {/if}
</div>

<style>
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
    align-items: flex-start;
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
</style>
