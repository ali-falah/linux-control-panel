<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import Toggle from '../components/ui/Toggle.svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { LayoutGrid, Search, Plus, Minus, Package, ExternalLink, RefreshCw } from '@lucide/svelte';
  import { open } from '@tauri-apps/plugin-shell';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';

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
  interface Props {
    embedded?: boolean;
  }
  let { embedded = true }: Props = $props();
</script>

<div class={embedded ? 'copr-tab-content' : 'module-page'}>
  {#if !embedded}
    <PageHeader title="Copr Browser" subtitle="Search and manage Fedora Copr repositories" icon={LayoutGrid} />
  {/if}

  <!-- Search Box -->
  <div style="display:flex; gap:8px">
    <SearchBar bind:value={query} placeholder="Search Copr projects (e.g. 'vscode', 'neovim', 'gaming')…" style="flex:1; border: 1px solid var(--color-border-focus)" />
    <Button variant="primary" class="" onclick={search} disabled={loading || !query.trim()}>
      {#if loading}
        <RefreshCw size={14} class="animate-spin-slow" /> Searching…
      {:else}
        <Search size={14} /> Search
      {/if}
    </Button>
  </div>

  <!-- Results -->
  {#if loading}
    <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
      <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
        <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
      </div>
      <span style="font-weight:500">Searching Copr API…</span>
    </div>
  {:else if hasSearched && results.length === 0}
    <div class="empty-state" style="padding: 64px 32px;">
      <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
        <LayoutGrid size={32} class="empty-state-icon" style="margin:0" />
      </div>
      <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
        No Projects Found
      </span>
      <span style="color:var(--color-text-muted); margin-top:8px;">
        No Copr projects matched "{query}".
      </span>
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
              <Button
                class="btn btn-sm -ghost"
                title="View on Copr"
                onclick={() => open(`https://copr.fedorainfracloud.org/coprs/${project.full_name}/`)}
              >
                <ExternalLink size={12} />
              </Button>

              <Button
                class="btn btn-sm -danger"
                onclick={() => disableRepo(project.full_name)}
              >
                <Minus size={12} /> Disable
              </Button>
              <Button
                class="btn btn-sm -primary"
                onclick={() => enableRepo(project.full_name)}
              >
                <Plus size={12} /> Enable
              </Button>
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
    <div class="empty-state" style="padding: 64px 32px;">
      <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
        <Search size={32} class="empty-state-icon" style="margin:0" />
      </div>
      <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
        Search Copr Repositories
      </span>
      <span style="color:var(--color-text-muted); margin-top:8px;">
        Try searching for "vscode", "gaming", "llvm", "wine"…
      </span>
    </div>
  {/if}
</div>

<style>
  .copr-tab-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-top: 4px;
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
