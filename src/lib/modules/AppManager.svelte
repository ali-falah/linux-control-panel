<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { Command } from '@tauri-apps/plugin-shell';
  import { AppWindow, Search, RefreshCw, Trash2, LayoutGrid, Terminal, X, Clock, HardDrive } from '@lucide/svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface DesktopApp {
    name: string;
    exec: string;
    source: string;
    package_id: string | null;
    file_path: string;
    sizeBytes?: number;
    installDate?: number;
  }

  interface AppDetails {
    version: string;
    description: string;
    files: string[];
  }

  let apps = $state<DesktopApp[]>([]);
  let loading = $state(false);
  let filter = $state('');

  type SourceFilter = 'All' | 'RPM' | 'Flatpak';
  let sourceFilter = $state<SourceFilter>('All');

  type SortOption = 'name' | 'size' | 'date' | 'source';
  let sortBy = $state<SortOption>('size');

  // Terminal log state
  let uninstallLog = $state<string[]>([]);
  let uninstallingApp = $state<DesktopApp | null>(null);
  let isUninstalling = $state(false);
  let logContainer = $state<HTMLElement | null>(null);

  // Side panel state
  let selectedAppForDetails = $state<DesktopApp | null>(null);
  let appDetails = $state<AppDetails | null>(null);
  let loadingDetails = $state(false);
  let showMoreFiles = $state(false);

  // Tooltip state
  let hoverTooltip = $state<{ text: string, x: number, y: number } | null>(null);
  let hoverTimer: any = null;

  const filteredApps = $derived(apps.filter(app => {
    const matchesName = app.name.toLowerCase().includes(filter.toLowerCase());
    const matchesSource = sourceFilter === 'All' || app.source === sourceFilter;
    return matchesName && matchesSource;
  }).sort((a, b) => {
    if (sortBy === 'name') return a.name.localeCompare(b.name);
    if (sortBy === 'source') return a.source.localeCompare(b.source);
    if (sortBy === 'size') return (b.sizeBytes || 0) - (a.sizeBytes || 0);
    if (sortBy === 'date') return (b.installDate || 0) - (a.installDate || 0);
    return 0;
  }));

  function formatBytes(bytes?: number) {
    if (bytes === undefined) return '';
    if (bytes === 0) return 'Unknown Size';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  function formatDate(unixSecs?: number) {
    if (!unixSecs) return 'Unknown';
    return new Date(unixSecs * 1000).toLocaleDateString();
  }

  async function loadApps() {
    loading = true;
    statusStore.setBusy('Scanning applications…');
    try {
      apps = await invoke<DesktopApp[]>('list_desktop_apps');
      
      // Async size fetching
      for (let i = 0; i < apps.length; i++) {
        if (apps[i].package_id) {
          invoke<{size_bytes: number, install_date: number}>('get_app_meta', { 
            packageId: apps[i].package_id, 
            source: apps[i].source 
          }).then(meta => {
            apps[i] = { ...apps[i], sizeBytes: meta.size_bytes, installDate: meta.install_date };
          }).catch(() => {});
        }
      }
    } catch (e) {
      uiStore.addToast(`Failed to load apps: ${e}`, 'error');
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  async function openDetails(app: DesktopApp) {
    selectedAppForDetails = app;
    appDetails = null;
    loadingDetails = true;
    showMoreFiles = false;
    
    if (app.package_id) {
      try {
        appDetails = await invoke<AppDetails>('get_app_details', { 
          packageId: app.package_id, 
          source: app.source 
        });
      } catch (e) {
        appDetails = { version: 'Error', description: String(e), files: [] };
      }
    } else {
      appDetails = { version: 'Unknown', description: 'No package ID available.', files: [] };
    }
    loadingDetails = false;
  }

  function closeDetails() {
    selectedAppForDetails = null;
    appDetails = null;
  }

  function appendLog(line: string) {
    uninstallLog = [...uninstallLog, line];
    if (logContainer) {
      setTimeout(() => {
        if (logContainer) logContainer.scrollTop = logContainer.scrollHeight;
      }, 50);
    }
  }

    async function performUninstall(app: DesktopApp) {
    if (!app.package_id) {
      uiStore.addToast('Cannot uninstall: Unknown package ID', 'error');
      return;
    }

    closeDetails();
    uninstallingApp = app;
    isUninstalling = true;
    uninstallLog = [];
    appendLog(`Starting uninstallation for ${app.name} (${app.package_id})...`);
    try {
      let cmdString = app.source === 'Flatpak' 
        ? `flatpak uninstall -y ${app.package_id}`
        : `dnf remove -y ${app.package_id}`;
      
      await invoke('uninstall_app', { packageId: app.package_id, source: app.source });
      appendLog(`\nSuccessfully uninstalled ${app.name} and cleaned dependencies.`);
      statusStore.setLastCommand(cmdString, 0, true);
      uiStore.addToast(`Removed ${app.name}`, 'success');
      loadApps();
    } catch (e) {
      appendLog(`\nExecution error: ${e}`);
      let cmdString = app.source === 'Flatpak' 
        ? `flatpak uninstall -y ${app.package_id}`
        : `dnf remove -y ${app.package_id}`;
      statusStore.setLastCommand(cmdString, 1, false);
    } finally {
      isUninstalling = false;
    }
  }

  function confirmUninstall(app: DesktopApp) {
    if (!app.package_id) {
      uiStore.addToast('Cannot uninstall: Unknown package ID', 'error');
      return;
    }
    
    uiStore.confirm(
      'Uninstall Application',
      `Are you sure you want to uninstall ${app.name}?\nSource: ${app.source}\nPackage: ${app.package_id}`,
      () => performUninstall(app),
      true
    );
  }

  function onMouseEnter(e: MouseEvent, text: string) {
    clearTimeout(hoverTimer);
    const x = e.clientX;
    const y = e.clientY + 20;
    hoverTimer = setTimeout(() => {
      hoverTooltip = { text, x, y };
    }, 400);
  }

  function onMouseLeave() {
    clearTimeout(hoverTimer);
    hoverTooltip = null;
  }

  $effect(() => { loadApps(); });
</script>

{#if hoverTooltip}
  <div class="custom-tooltip" style="left: {hoverTooltip.x}px; top: {hoverTooltip.y}px;">
    {hoverTooltip.text}
  </div>
{/if}

<div class="module-page" class:panel-open={selectedAppForDetails}>
  <PageHeader title="App Manager" subtitle="Manage installed graphical applications" icon={LayoutGrid}>
    <Button variant="ghost" onclick={loadApps} disabled={loading || isUninstalling}>
      <RefreshCw size={14} class={loading ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  <div class="controls-row">
    <div style="display:flex; gap:16px; align-items:center; flex-wrap:wrap;">
      <SearchBar bind:value={filter} placeholder="Search installed apps..." disabled={isUninstalling} style="flex: 1; max-width: 300px;" />
      
      <TabGroup 
        tabs={[
          {id: 'All', label: 'All Sources'},
          {id: 'RPM', label: 'RPM'},
          {id: 'Flatpak', label: 'Flatpak'}
        ]}
        bind:activeTab={sourceFilter}
        disabled={isUninstalling}
      />
    </div>
    
    <div style="display:flex;align-items:center;gap:16px;">
      <div style="display:flex; align-items:center; gap:8px;">
        <span style="font-size: 12px; color: var(--color-text-muted);">Sort by:</span>
        <Select bind:value={sortBy} disabled={isUninstalling}>
          <option value="name">Name</option>
          <option value="size">Size</option>
          <option value="date">Install Date</option>
          <option value="source">Source</option>
        </Select>
      </div>
      
      <div style="display:flex;align-items:center;gap:8px;padding:0 12px;height:40px;box-sizing:border-box;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.08);border-radius:8px;">
        <span style="font-size:14px;font-weight:700;color:var(--color-text-primary);line-height:1;">{apps.length}</span>
        <span style="font-size:10px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.06em;font-weight:600;">Apps</span>
      </div>
    </div>
  </div>

  <div class="content">
    {#if uninstallingApp}
      <Card title="Uninstallation Progress: {uninstallingApp.name}" icon={Terminal}>
        <div class="terminal-log" bind:this={logContainer}>
          {#each uninstallLog as line}
            <div class="log-line">{line}</div>
          {/each}
        </div>
        
        <div style="margin-top: 16px; display: flex; justify-content: flex-end;">
          {#if !isUninstalling}
            <Button variant="primary" onclick={() => uninstallingApp = null}>Done</Button>
          {:else}
            <Button variant="secondary" disabled>Uninstalling...</Button>
          {/if}
        </div>
      </Card>
    {:else}
      {#if loading && apps.length === 0}
        <div class="empty-state">
          <RefreshCw size={32} class="animate-spin-slow" style="color:var(--color-text-muted); margin-bottom: 16px;" />
          <div>Scanning for applications...</div>
        </div>
      {:else if apps.length > 0 && filteredApps.length === 0}
        <div class="empty-state">
          <Search size={48} style="color:var(--color-text-muted); margin-bottom: 16px; opacity: 0.5;" />
          <div style="font-size: 16px; font-weight: 500; color: var(--color-text-primary);">No apps found for '{filter}'</div>
          <Button variant="outline" class="btn-sm" style="margin-top: 16px;" onclick={() => filter = ''}>Clear Search</Button>
        </div>
      {:else if apps.length === 0}
        <div class="empty-state">
          <AppWindow size={48} style="color:var(--color-text-muted); margin-bottom: 16px; opacity: 0.5;" />
          <div>No applications found.</div>
        </div>
      {:else}
        <div class="app-grid">
          {#each filteredApps as app}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="app-card" onclick={() => openDetails(app)}>
              <div class="app-icon">
                <AppWindow size={32} />
              </div>
              <div class="app-info">
                <div 
                  class="app-name"
                  onmouseenter={(e) => { if (app.package_id) onMouseEnter(e, app.package_id); }}
                  onmouseleave={onMouseLeave}
                >
                  {app.name}
                </div>
                <div class="app-meta">
                  <span class="badge" class:flatpak={app.source === 'Flatpak'} class:rpm={app.source === 'RPM'}>
                    {app.source}
                  </span>
                  
                  {#if app.sizeBytes === undefined}
                    <div class="skeleton-text" style="width: 60px; height: 12px;"></div>
                  {:else}
                    <span class="size-text"><HardDrive size={12} style="display:inline; margin-right:4px;" />{formatBytes(app.sizeBytes)}</span>
                  {/if}
                </div>
              </div>
              <div class="app-actions">
                <Button variant="ghost" class="text-danger" disabled={!app.package_id} onclick={(e: any) => { e.stopPropagation(); confirmUninstall(app); }} title="Uninstall">
                  <Trash2 size={16} />
                </Button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <!-- Side Panel -->
  {#if selectedAppForDetails}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="panel-backdrop" onclick={closeDetails}></div>
    <div class="side-panel">
      <div class="panel-header">
        <div class="panel-title">{selectedAppForDetails.name}</div>
        <button class="panel-close-btn" onclick={closeDetails}><X size={20} /></button>
      </div>
      
      <div class="panel-content">
        <div class="panel-section">
          <div class="detail-label">Full Package Name</div>
          <div class="detail-value" style="font-family: var(--font-mono); color: var(--color-text-accent);">{selectedAppForDetails.package_id || 'Unknown'}</div>
        </div>

        <div style="display:flex; gap:16px; margin-bottom: 24px;">
          <div class="panel-section" style="margin-bottom:0; flex:1;">
            <div class="detail-label">Source</div>
            <div class="detail-value">
              <span class="badge" class:flatpak={selectedAppForDetails.source === 'Flatpak'} class:rpm={selectedAppForDetails.source === 'RPM'} style="font-size: 11px;">
                {selectedAppForDetails.source}
              </span>
            </div>
          </div>
          <div class="panel-section" style="margin-bottom:0; flex:1;">
            <div class="detail-label">Disk Size</div>
            <div class="detail-value">
              {#if selectedAppForDetails.sizeBytes === undefined}
                <div class="skeleton-text" style="width: 80%; height: 14px;"></div>
              {:else}
                {formatBytes(selectedAppForDetails.sizeBytes)}
              {/if}
            </div>
          </div>
        </div>

        <div style="display:flex; gap:16px; margin-bottom: 24px;">
          <div class="panel-section" style="margin-bottom:0; flex:1;">
            <div class="detail-label">Install Date</div>
            <div class="detail-value">
              {#if selectedAppForDetails.installDate === undefined}
                <div class="skeleton-text" style="width: 80%; height: 14px;"></div>
              {:else}
                {formatDate(selectedAppForDetails.installDate)}
              {/if}
            </div>
          </div>
        </div>

        {#if loadingDetails}
          <div style="display:flex; justify-content:center; padding: 32px 0;">
            <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-text-muted)" />
          </div>
        {:else if appDetails}
          <div class="panel-section">
            <div class="detail-label">Version</div>
            <div class="detail-value">{appDetails.version}</div>
          </div>

          <div class="panel-section">
            <div class="detail-label">Description</div>
            <div class="detail-value" style="line-height: 1.5; color: var(--color-text-secondary); white-space: pre-wrap;">{appDetails.description}</div>
          </div>

          {#if appDetails.files && appDetails.files.length > 0}
            <div class="panel-section">
              <div class="detail-label">Owned Files ({appDetails.files.length})</div>
              <div class="files-list">
                {#each (showMoreFiles ? appDetails.files : appDetails.files.slice(0, 20)) as file}
                  <div class="file-item">{file}</div>
                {/each}
              </div>
              {#if appDetails.files.length > 20}
                <Button variant="ghost" class="btn-sm" style="width: 100%; margin-top: 8px;" onclick={() => showMoreFiles = !showMoreFiles}>
                  {showMoreFiles ? 'Show Less' : `Show All ${appDetails.files.length} Files`}
                </Button>
              {/if}
            </div>
          {/if}

          <div style="margin-top: auto; padding-top: 24px;">
            <Button variant="outline" class="text-danger" style="width: 100%; border-color: rgba(239,68,68,0.2);" onclick={() => confirmUninstall(selectedAppForDetails!)}>
              <Trash2 size={16} style="margin-right:8px;" /> Uninstall Application
            </Button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .module-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .controls-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 24px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-base);
    flex-shrink: 0;
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    background: var(--color-bg-subtle);
  }

  .app-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }

  .app-card {
    display: flex;
    align-items: center;
    gap: 16px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 16px;
    transition: all 0.2s ease;
    cursor: pointer;
  }

  .app-card:hover {
    border-color: rgba(255, 255, 255, 0.15);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  }

  .app-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-primary);
    flex-shrink: 0;
  }

  .app-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .app-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .app-meta {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .size-text {
    font-size: 11px;
    color: var(--color-text-muted);
    display: flex;
    align-items: center;
  }

  .badge {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .badge.flatpak {
    background: rgba(99, 102, 241, 0.15);
    color: #818cf8;
    border: 1px solid rgba(99, 102, 241, 0.25);
  }

  .badge.rpm {
    background: rgba(16, 185, 129, 0.15);
    color: #34d399;
    border: 1px solid rgba(16, 185, 129, 0.25);
  }

  .app-actions {
    display: flex;
    align-items: center;
  }

  .terminal-log {
    background: #0f111a;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    height: 300px;
    overflow-y: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    color: #a6accd;
    line-height: 1.5;
  }

  .log-line {
    white-space: pre-wrap;
    word-break: break-all;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 300px;
    color: var(--color-text-muted);
    font-size: 14px;
  }

  /* Tooltip */
  .custom-tooltip {
    position: fixed;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    padding: 6px 10px;
    border-radius: 6px;
    font-size: 12px;
    pointer-events: none;
    z-index: 10000;
    box-shadow: 0 4px 12px rgba(0,0,0,0.5);
    font-family: var(--font-mono);
    max-width: 300px;
    word-break: break-all;
  }

  /* Skeleton Loading */
  .skeleton-text {
    background: linear-gradient(90deg, rgba(255,255,255,0.05) 25%, rgba(255,255,255,0.1) 50%, rgba(255,255,255,0.05) 75%);
    background-size: 200% 100%;
    animation: skeleton-loading 1.5s infinite;
    border-radius: 4px;
  }

  @keyframes skeleton-loading {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  /* Side Panel */
  .panel-backdrop {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0,0,0,0.4);
    z-index: 40;
    backdrop-filter: blur(2px);
    animation: fade-in 0.2s ease-out forwards;
  }

  .side-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 360px;
    background: var(--color-bg-base);
    border-left: 1px solid var(--color-border);
    z-index: 50;
    display: flex;
    flex-direction: column;
    box-shadow: -8px 0 24px rgba(0,0,0,0.3);
    animation: slide-in 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-raised);
  }

  .panel-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .panel-close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
    display: flex;
    transition: all 0.2s;
  }

  .panel-close-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255,255,255,0.05);
  }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    display: flex;
    flex-direction: column;
  }

  .panel-section {
    margin-bottom: 24px;
  }

  .detail-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 700;
    color: var(--color-text-muted);
    margin-bottom: 6px;
  }

  .detail-value {
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .files-list {
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 0;
    max-height: 250px;
    overflow-y: auto;
  }

  .file-item {
    padding: 4px 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  .file-item:hover {
    background: rgba(255,255,255,0.03);
    color: var(--color-text-primary);
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes slide-in {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }
</style>
