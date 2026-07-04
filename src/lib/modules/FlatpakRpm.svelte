<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Layers, RefreshCw, Trash2, AlertTriangle, CheckCircle, Search, Package } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import PageHeader from '../components/PageHeader.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';

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
  <PageHeader title="Flatpak vs RPM" subtitle="Detect duplicate packages and manage installations" icon={Layers}>
    <button class="btn btn-ghost" onclick={loadAll} disabled={loadingDuplicates}>
      <RefreshCw size={14} class={loadingDuplicates ? 'animate-spin-slow' : ''} /> Refresh
    </button>
  </PageHeader>

  <!-- Controls: Stats, Search & Tabs -->
  <div style="display:flex; gap:16px; align-items:stretch; flex-wrap:wrap; margin-bottom: 16px;">
    <!-- Stats -->
    <div style="display:flex; gap:12px; flex-wrap:wrap; margin: 0; align-items:stretch;">
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-warning);line-height:1;">{duplicates.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Duplicates</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-info);line-height:1;">{flatpaks.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Flatpaks</span>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:8px 16px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:10px;backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);">
        <span style="font-size:16px;font-weight:700;color:var(--color-text-primary);line-height:1;">{rpms.length}</span>
        <span style="font-size:11px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">RPMs</span>
      </div>
    </div>

    <!-- Search -->
    {#if activeTab !== 'duplicates'}
      <div class="search-bar" style="flex:1; min-width:200px; margin: 0;">
        <Search size={14} style="color:var(--color-text-muted)" />
        <input bind:value={filter} placeholder="Filter packages…" />
      </div>
    {:else}
      <div style="flex:1"></div>
    {/if}

    <!-- Tabs -->
    <div style="display:flex; gap:2px; background:var(--color-bg-raised); padding:4px; border-radius:10px; width:fit-content; margin: 0;">
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
  </div>

  <!-- Content -->
  {#if activeTab === 'duplicates'}
    <div class="card module-content-scroll" style="padding:0">
      {#if loadingDuplicates}
        <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
          <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
            <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          </div>
          <span style="font-weight:500">Scanning for duplicates…</span>
        </div>
      {:else if duplicates.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:rgba(34,197,94,0.1); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <CheckCircle size={32} style="color:var(--color-success); margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
            No duplicate packages found
          </span>
          <span style="color:var(--color-text-muted); margin-top:8px;">
            Your Flatpak and RPM installs don't overlap.
          </span>
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
        <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
          <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
            <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          </div>
          <span style="font-weight:500">Loading Flatpaks…</span>
        </div>
      {:else if filteredFlatpaks.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <Layers size={32} class="empty-state-icon" style="margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
            No Flatpaks Found
          </span>
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
                <th style="text-align:right">Actions</th>
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
                  <td style="text-align:right">
                    <KebabMenu>
                      <button
                        class="menu-item danger"
                        onclick={() => confirmRemoveFlatpak(app)}
                        disabled={removingId === app.app_id}
                      >
                        <Trash2 size={14} /> Remove
                      </button>
                    </KebabMenu>
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
        <div style="padding:48px 32px;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--color-text-muted)">
          <div style="position:relative; width:48px; height:48px; display:flex; align-items:center; justify-content:center; border-radius:50%; background:var(--color-bg-raised);">
            <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          </div>
          <span style="font-weight:500">Loading RPMs…</span>
        </div>
      {:else if filteredRpms.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <div style="width:64px; height:64px; border-radius:50%; background:var(--color-bg-raised); display:flex; align-items:center; justify-content:center; margin:0 auto 16px;">
            <Package size={32} class="empty-state-icon" style="margin:0" />
          </div>
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
            No RPM Packages Found
          </span>
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
                <th style="text-align:right">Actions</th>
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
                  <td style="text-align:right">
                    <KebabMenu>
                      <button
                        class="menu-item danger"
                        onclick={() => confirmRemoveRpm(pkg)}
                        disabled={removingId === pkg.name}
                      >
                        <Trash2 size={14} /> Remove
                      </button>
                    </KebabMenu>
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
