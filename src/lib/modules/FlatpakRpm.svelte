<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Layers, RefreshCw, Trash2, AlertTriangle, CheckCircle, Search } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface FlatpakApp {
    name: string;
    app_id: string;
    version: string;
    origin: string;
    installation: string;
  }

  interface RpmPackage {
    name: string;
    version: string;
    arch: string;
    summary: string;
  }

  interface DuplicateEntry {
    common_name: string;
    flatpak: FlatpakApp | null;
    rpm: RpmPackage | null;
    recommendation: string;
  }

  type Tab = 'duplicates' | 'flatpaks' | 'rpms';

  let activeTab = $state<Tab>('duplicates');
  let flatpaks = $state<FlatpakApp[]>([]);
  let rpms = $state<RpmPackage[]>([]);
  let duplicates = $state<DuplicateEntry[]>([]);
  let loadingFlatpak = $state(false);
  let loadingRpm = $state(false);
  let loadingDuplicates = $state(false);
  let removingId = $state<string | null>(null);
  let filter = $state('');

  const filteredFlatpaks = $derived(flatpaks.filter(f =>
    f.name.toLowerCase().includes(filter.toLowerCase()) ||
    f.app_id.toLowerCase().includes(filter.toLowerCase())
  ));

  const filteredRpms = $derived(
    rpms.filter(p => p.name.toLowerCase().includes(filter.toLowerCase()) || (p.summary && p.summary.toLowerCase().includes(filter.toLowerCase())))
  );

  let currentPage = $state(1);
  const pageSize = 100;

  $effect(() => {
    // Reset page when filter or tab changes
    filter;
    activeTab;
    currentPage = 1;
  });

  const paginatedFlatpaks = $derived(
    filteredFlatpaks.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  );

  const paginatedRpms = $derived(
    filteredRpms.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  );

  async function loadAll() {
    loadingFlatpak = true;
    loadingRpm = true;
    loadingDuplicates = true;
    statusStore.setBusy('Scanning packages…');

    try {
      [flatpaks, rpms, duplicates] = await Promise.all([
        invoke<FlatpakApp[]>('list_flatpaks'),
        invoke<RpmPackage[]>('list_rpms'),
        invoke<DuplicateEntry[]>('detect_duplicates'),
      ]);
      statusStore.setLastCommand('list_flatpaks + list_rpms', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load packages: ${e}`, 'error');
      statusStore.setLastCommand('list_flatpaks + list_rpms', 1, false);
    } finally {
      loadingFlatpak = false;
      loadingRpm = false;
      loadingDuplicates = false;
      statusStore.clearBusy();
    }
  }

  function confirmRemoveFlatpak(app: FlatpakApp) {
    uiStore.confirm(
      'Remove Flatpak App',
      `Remove "${app.name}" (${app.app_id})?\n\nInstallation: ${app.installation}`,
      async () => {
        removingId = app.app_id;
        try {
          await invoke('remove_flatpak', {
            appId: app.app_id,
            systemWide: app.installation === 'system',
          });
          uiStore.addToast(`Removed Flatpak: ${app.name}`, 'success');
          await loadAll();
        } catch (e) {
          uiStore.addToast(`Failed to remove Flatpak: ${e}`, 'error');
        } finally {
          removingId = null;
        }
      },
      true
    );
  }

  function confirmRemoveRpm(pkg: RpmPackage) {
    uiStore.confirm(
      'Remove RPM Package',
      `Remove "${pkg.name}" (${pkg.version})?\n\nThis may affect other packages that depend on it.`,
      async () => {
        removingId = pkg.name;
        try {
          await invoke('remove_rpm', { name: pkg.name });
          uiStore.addToast(`Removed RPM: ${pkg.name}`, 'success');
          await loadAll();
        } catch (e) {
          uiStore.addToast(`Failed to remove RPM: ${e}`, 'error');
        } finally {
          removingId = null;
        }
      },
      true
    );
  }

  $effect(() => { loadAll(); });
</script>

<div class="module-page">
  <div class="module-header">
    <div class="module-icon"><Layers size={20} /></div>
    <div>
      <h1 class="module-title">Flatpak vs RPM</h1>
      <p class="module-subtitle">Detect duplicate packages and manage installations</p>
    </div>
    <div style="margin-left:auto">
      <button class="btn btn-ghost" onclick={loadAll} disabled={loadingDuplicates}>
        <RefreshCw size={14} class={loadingDuplicates ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    </div>
  </div>

  <!-- Stats -->
  <div style="display:flex; gap:12px; flex-wrap:wrap">
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-warning)">{duplicates.length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">Duplicates</span>
    </div>
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-info)">{flatpaks.length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">Flatpaks</span>
    </div>
    <div class="card-raised" style="display:flex;align-items:center;gap:10px;padding:12px 16px">
      <span style="font-size:22px;font-weight:700;color:var(--color-text-primary)">{rpms.length}</span>
      <span style="font-size:12px;color:var(--color-text-muted)">RPMs</span>
    </div>
  </div>

  <!-- Tabs -->
  <div style="display:flex; gap:2px; background:var(--color-bg-raised); padding:4px; border-radius:10px; width:fit-content">
    {#each [['duplicates','Duplicates'],['flatpaks','Flatpaks'],['rpms','RPMs']] as [id, label]}
      <button
        class="tab-btn"
        class:active={activeTab === id}
        onclick={() => activeTab = id as Tab}
      >
        {label}
        {#if id === 'duplicates' && duplicates.length > 0}
          <span class="badge badge-warning" style="margin-left:4px;padding:1px 5px">{duplicates.length}</span>
        {/if}
      </button>
    {/each}
  </div>

  <!-- Search -->
  {#if activeTab !== 'duplicates'}
    <div class="search-bar">
      <Search size={14} style="color:var(--color-text-muted)" />
      <input bind:value={filter} placeholder="Filter packages…" />
    </div>
  {/if}

  <!-- Content -->
  {#if activeTab === 'duplicates'}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingDuplicates}
        <div style="padding:32px;display:flex;align-items:center;justify-content:center;gap:10px;color:var(--color-text-muted)">
          <RefreshCw size={16} class="animate-spin-slow" /> Scanning for duplicates…
        </div>
      {:else if duplicates.length === 0}
        <div class="empty-state">
          <CheckCircle size={40} style="color:var(--color-success);opacity:0.5" />
          <span>No duplicate packages found</span>
          <span style="font-size:12px">Your Flatpak and RPM installs don't overlap</span>
        </div>
      {:else}
        {#each duplicates as dup (dup.common_name)}
          <div class="dup-row">
            <div class="dup-header">
              <AlertTriangle size={16} style="color:var(--color-warning);flex-shrink:0" />
              <h4 class="dup-name">{dup.common_name}</h4>
            </div>
            <p class="dup-rec">{dup.recommendation}</p>
            <div class="dup-versions">
              {#if dup.flatpak}
                <div class="dup-version-card flatpak-card">
                  <div class="dup-version-label">Flatpak</div>
                  <div class="dup-version-id">{dup.flatpak.app_id}</div>
                  <div class="dup-version-ver">v{dup.flatpak.version} · {dup.flatpak.origin}</div>
                  <button
                    class="btn btn-sm btn-danger"
                    onclick={() => dup.flatpak && confirmRemoveFlatpak(dup.flatpak)}
                    disabled={removingId === dup.flatpak.app_id}
                  >
                    <Trash2 size={11} /> Remove Flatpak
                  </button>
                </div>
              {/if}
              {#if dup.rpm}
                <div class="dup-version-card rpm-card">
                  <div class="dup-version-label">RPM</div>
                  <div class="dup-version-id">{dup.rpm.name}</div>
                  <div class="dup-version-ver">v{dup.rpm.version} · {dup.rpm.arch}</div>
                  <button
                    class="btn btn-sm btn-danger"
                    onclick={() => dup.rpm && confirmRemoveRpm(dup.rpm)}
                    disabled={removingId === dup.rpm.name}
                  >
                    <Trash2 size={11} /> Remove RPM
                  </button>
                </div>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>

  {:else if activeTab === 'flatpaks'}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingFlatpak}
        <div style="padding:32px;display:flex;align-items:center;justify-content:center;gap:10px;color:var(--color-text-muted)">
          <RefreshCw size={16} class="animate-spin-slow" /> Loading Flatpaks…
        </div>
      {:else if filteredFlatpaks.length === 0}
        <div class="empty-state">
          <Layers size={40} class="empty-state-icon" />
          <span>No Flatpak apps installed</span>
        </div>
      {:else}
        <div class="table-wrap" style="border:none;border-radius:0">
          <table>
            <thead>
              <tr>
                <th>App Name</th>
                <th>App ID</th>
                <th>Version</th>
                <th>Origin</th>
                <th>Install</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each paginatedFlatpaks as app (app.app_id + '-' + app.installation)}
                <tr>
                  <td style="font-weight:500">{app.name}</td>
                  <td><code style="font-size:11px;color:var(--color-text-accent)">{app.app_id}</code></td>
                  <td><span class="badge badge-info">{app.version || '—'}</span></td>
                  <td style="color:var(--color-text-secondary)">{app.origin}</td>
                  <td><span class="badge badge-muted">{app.installation}</span></td>
                  <td>
                    <button
                      class="btn btn-sm btn-danger"
                      onclick={() => confirmRemoveFlatpak(app)}
                      disabled={removingId === app.app_id}
                    >
                      <Trash2 size={11} /> Remove
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if filteredFlatpaks.length > pageSize}
          <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:16px; border-top:1px solid var(--color-border); background:var(--color-bg-base)">
            <button class="btn btn-outline btn-sm" disabled={currentPage === 1} onclick={() => currentPage--}>Previous</button>
            <span style="font-size:12px; color:var(--color-text-secondary)">Page {currentPage} of {Math.ceil(filteredFlatpaks.length / pageSize)}</span>
            <button class="btn btn-outline btn-sm" disabled={currentPage === Math.ceil(filteredFlatpaks.length / pageSize)} onclick={() => currentPage++}>Next</button>
          </div>
        {/if}
      {/if}
    </div>

  {:else}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingRpm}
        <div style="padding:32px;display:flex;align-items:center;justify-content:center;gap:10px;color:var(--color-text-muted)">
          <RefreshCw size={16} class="animate-spin-slow" /> Loading RPMs…
        </div>
      {:else if filteredRpms.length === 0}
        <div class="empty-state">
          <Package size={40} class="empty-state-icon" />
          <span>No RPM packages found</span>
        </div>
      {:else}
        <div class="table-wrap" style="border:none;border-radius:0">
          <table>
            <thead>
              <tr>
                <th>Package</th>
                <th>Version</th>
                <th>Arch</th>
                <th>Description</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each paginatedRpms as pkg (pkg.name + '-' + pkg.version + '-' + pkg.arch)}
                <tr>
                  <td style="font-weight:500;font-family:var(--font-mono);font-size:12px">{pkg.name}</td>
                  <td><span class="badge badge-info">{pkg.version}</span></td>
                  <td><span class="badge badge-muted">{pkg.arch}</span></td>
                  <td style="color:var(--color-text-secondary);font-size:12px;max-width:250px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                    {pkg.summary}
                  </td>
                  <td>
                    <button
                      class="btn btn-sm btn-danger"
                      onclick={() => confirmRemoveRpm(pkg)}
                      disabled={removingId === pkg.name}
                    >
                      <Trash2 size={11} /> Remove
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if filteredRpms.length > pageSize}
          <div style="display:flex; justify-content:center; align-items:center; gap:16px; padding:16px; border-top:1px solid var(--color-border); background:var(--color-bg-base)">
            <button class="btn btn-outline btn-sm" disabled={currentPage === 1} onclick={() => currentPage--}>Previous</button>
            <span style="font-size:12px; color:var(--color-text-secondary)">Page {currentPage} of {Math.ceil(filteredRpms.length / pageSize)}</span>
            <button class="btn btn-outline btn-sm" disabled={currentPage === Math.ceil(filteredRpms.length / pageSize)} onclick={() => currentPage++}>Next</button>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .tab-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    font-family: var(--font-sans);
    display: flex;
    align-items: center;
    transition: all 0.15s;
  }
  .tab-btn.active {
    background: var(--color-bg-card);
    color: var(--color-text-primary);
  }

  .dup-row {
    padding: 16px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.04);
  }
  .dup-row:last-child { border-bottom: none; }

  .dup-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .dup-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 0;
  }

  .dup-rec {
    font-size: 12px;
    color: var(--color-text-muted);
    margin: 0 0 12px;
    padding-left: 24px;
  }

  .dup-versions {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    padding-left: 24px;
  }

  .dup-version-card {
    padding: 12px 14px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    min-width: 200px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .flatpak-card {
    border-color: rgba(56,189,248,0.2);
    background: rgba(56,189,248,0.04);
  }

  .rpm-card {
    border-color: rgba(251,191,36,0.2);
    background: rgba(251,191,36,0.04);
  }

  .dup-version-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-text-muted);
    margin-bottom: 2px;
  }

  .dup-version-id {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--color-text-accent);
  }

  .dup-version-ver {
    font-size: 11px;
    color: var(--color-text-muted);
    margin-bottom: 8px;
  }
</style>
