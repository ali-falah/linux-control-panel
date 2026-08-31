<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { Command } from '@tauri-apps/plugin-shell';
  import { 
    AppWindow, Search, RefreshCw, Trash2, LayoutGrid, Terminal, X, Clock, 
    HardDrive, Database, Code2, AlertTriangle, CheckCircle, Layers,
    Play, FolderOpen, Copy, Info, Shield, Globe
  } from '@lucide/svelte';
  import ContextMenu from '../components/ui/ContextMenu.svelte';
  import { open as openUrl } from '@tauri-apps/plugin-shell';
  import Button from '../components/ui/Button.svelte';
  import BulkActionBar from '../components/ui/BulkActionBar.svelte';
  import Input from '../components/ui/Input.svelte';
  import Card from '../components/ui/Card.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Select from '../components/ui/Select.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
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

  function getAppIcon(name: string) {
    const lower = name.toLowerCase();
    if (lower.includes('database') || lower.includes('beekeeper') || lower.includes('sql') || lower.includes('mongo') || lower.includes('redis') || lower.includes('postgres') || lower.includes('mysql')) {
      return Database;
    }
    if (lower.includes('code') || lower.includes('visual studio') || lower.includes('developer') || lower.includes('editor') || lower.includes('sublime') || lower.includes('atom') || lower.includes('intellij') || lower.includes('neovim') || lower.includes('vim') || lower.includes('emacs')) {
      return Code2;
    }
    if (lower.includes('terminal') || lower.includes('shell') || lower.includes('bash') || lower.includes('command') || lower.includes('console')) {
      return Terminal;
    }
    return AppWindow; // default
  }

  interface AppDetails {
    version: string;
    description: string;
    files: string[];
  }

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

  let apps = $state<DesktopApp[]>([]);
  let selectedAppNames = $state<Set<string>>(new Set());
  let duplicates = $state<DuplicateEntry[]>([]);
  let loadingDuplicates = $state(false);
  let loading = $state(false);
  let filter = $state('');

  type SourceFilter = 'All' | 'RPM' | 'Flatpak' | 'AppImage' | 'Waydroid' | 'Duplicates';
  let sourceFilter = $state<SourceFilter>(
    uiStore.appSourceFilter
      ? (uiStore.appSourceFilter as SourceFilter)
      : (uiStore.targetSubTab && ['All', 'RPM', 'Flatpak', 'AppImage', 'Waydroid', 'Duplicates'].includes(uiStore.targetSubTab))
      ? (uiStore.targetSubTab as any)
      : 'All'
  );
  if (uiStore.appSourceFilter) {
    uiStore.appSourceFilter = null;
  }
  if (uiStore.targetSubTab && ['All', 'RPM', 'Flatpak', 'AppImage', 'Waydroid', 'Duplicates'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  type SortOption = 'name' | 'size' | 'date' | 'source';
  let sortBy = $state<SortOption>('size');

  // Terminal log state
  let uninstallLog = $state<string[]>([]);
  let uninstallingApp = $state<DesktopApp | null>(null);
  let isUninstalling = $state(false);

  function toggleAppSelection(appName: string) {
    if (selectedAppNames.has(appName)) {
      selectedAppNames.delete(appName);
    } else {
      selectedAppNames.add(appName);
    }
    selectedAppNames = new Set(selectedAppNames);
  }

  function deselectAllApps() {
    selectedAppNames = new Set();
  }

  async function bulkUninstallSelected() {
    if (selectedAppNames.size === 0) return;
    const list = apps.filter(a => selectedAppNames.has(a.name));
    const count = list.length;
    uiStore.confirm(
      'Batch Uninstall Applications',
      `Are you sure you want to uninstall ${count} selected applications?\n\n${list.map(a => `• ${a.name} (${a.source})`).join('\n')}`,
      async () => {
        uiStore.addToast(`Starting batch uninstall of ${count} applications…`, 'info');
        for (const app of list) {
          await executeUninstall(app);
        }
        selectedAppNames = new Set();
      },
      true
    );
  }
  let logContainer = $state<HTMLElement | null>(null);

  // Side panel state
  let isAppDrawerOpen = $state(false);
  let selectedAppForDetails = $state<DesktopApp | null>(null);
  let appDetails = $state<AppDetails | null>(null);
  let loadingDetails = $state(false);
  let showMoreFiles = $state(false);

  // Details Side Panel tabs state
  type DetailTab = 'details' | 'permissions' | 'dependencies';
  let activeDetailTab = $state<DetailTab>('details');

  // Flatpak permissions state
  interface FlatpakPermissions {
    network: boolean;
    ipc: boolean;
    fallback_x11: boolean;
    x11: boolean;
    wayland: boolean;
    pulseaudio: boolean;
    gpu: boolean;
    host_files: boolean;
    home_files: boolean;
  }
  let flatpakPermissions = $state<FlatpakPermissions | null>(null);

  // Context Menu State
  let contextMenu = $state<{
    x: number;
    y: number;
    show: boolean;
    app: DesktopApp | null;
  }>({ x: 0, y: 0, show: false, app: null });

  function handleAppContextMenu(e: MouseEvent, app: DesktopApp) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      show: true,
      app
    };
  }

  async function searchWeb(query: string) {
    const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
    try {
      await openUrl(url);
      uiStore.addToast('Opening web search...', 'info');
    } catch {
      window.open(url, '_blank');
    }
  }

  function closeContextMenu() {
    contextMenu.show = false;
  }

  async function handleLaunchApp(app: DesktopApp) {
    closeContextMenu();
    try {
      uiStore.addToast(`Launching ${app.name}...`, 'info');
      await invoke('launch_desktop_app', { exec: app.exec });
    } catch (err: any) {
      uiStore.addToast(`Failed to launch: ${err?.message || err}`, 'error');
    }
  }

  async function handleOpenDesktopFile(app: DesktopApp) {
    closeContextMenu();
    if (app.file_path) {
      try {
        await invoke('reveal_in_file_manager', { path: app.file_path });
        uiStore.addToast(`Revealed file in manager`, 'success');
      } catch (err) {
        navigator.clipboard.writeText(app.file_path);
        uiStore.addToast(`Copied desktop file path`, 'info');
      }
    } else {
      uiStore.addToast('No desktop file path available', 'warning');
    }
  }

  function handleCopyAppId(app: DesktopApp) {
    closeContextMenu();
    const id = app.package_id || app.name;
    navigator.clipboard.writeText(id);
    uiStore.addToast(`Copied App ID: ${id}`, 'info');
  }
  let loadingPermissions = $state(false);

  // Dependencies state
  let dependencies = $state<string[]>([]);
  let loadingDependencies = $state(false);

  // AppImage shortcut state
  let appimageShortcutEnabled = $state(false);
  let registeringShortcut = $state(false);

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
    if (bytes === 0) return '—';
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
    loadingDuplicates = true;
    statusStore.setBusy('Scanning applications…');
    try {
      const [appsResult, duplicatesResult, appimagesResult] = await Promise.all([
        invoke<DesktopApp[]>('list_desktop_apps'),
        invoke<DuplicateEntry[]>('detect_duplicates').catch(() => []),
        invoke<DesktopApp[]>('scan_local_appimages').catch(() => [])
      ]);
      apps = [...appsResult, ...appimagesResult];
      duplicates = duplicatesResult;
      
      // Async size fetching
      for (let i = 0; i < apps.length; i++) {
        const app = apps[i];
        if (app.source === 'AppImage') {
          if (app.sizeBytes === undefined) {
            apps[i] = { ...app, sizeBytes: 0 };
          }
          continue;
        }
        if (app.package_id && (app.source === 'RPM' || app.source === 'Flatpak')) {
          invoke<{size_bytes: number, install_date: number}>('get_app_meta', { 
            packageId: app.package_id, 
            source: app.source 
          }).then(meta => {
            apps[i] = { ...apps[i], sizeBytes: meta.size_bytes, installDate: meta.install_date };
          }).catch(() => {
            apps[i] = { ...apps[i], sizeBytes: 0 };
          });
        } else {
          // For Waydroid, Local apps or apps without package_id, resolve size immediately
          apps[i] = { ...apps[i], sizeBytes: 0 };
        }
      }
    } catch (e) {
      uiStore.addToast(`Failed to load apps: ${e}`, 'error');
    } finally {
      loading = false;
      loadingDuplicates = false;
      statusStore.clearBusy();
    }
  }

  async function openDetails(app: DesktopApp) {
    selectedAppForDetails = app;
    isAppDrawerOpen = true;
    appDetails = null;
    loadingDetails = true;
    showMoreFiles = false;
    activeDetailTab = 'details';
    dependencies = [];
    flatpakPermissions = null;
    
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

    // Load dependencies
    if (app.source === 'RPM' || app.source === 'Flatpak') {
      loadingDependencies = true;
      try {
        dependencies = await invoke<string[]>('get_app_dependencies', {
          packageId: app.package_id || app.name.toLowerCase(),
          source: app.source
        });
      } catch (e) {
        console.error("Failed to load dependencies", e);
      } finally {
        loadingDependencies = false;
      }
    }

    // Load Flatpak permissions
    if (app.source === 'Flatpak' && app.package_id) {
      loadingPermissions = true;
      try {
        flatpakPermissions = await invoke<FlatpakPermissions>('get_flatpak_permissions', {
          appId: app.package_id
        });
      } catch (e) {
        console.error("Failed to load Flatpak permissions", e);
      } finally {
        loadingPermissions = false;
      }
    }

    // Load AppImage shortcut state
    if (app.source === 'AppImage') {
      appimageShortcutEnabled = app.package_id !== "";
    }
  }

  async function selectDetailTab(tab: DetailTab) {
    activeDetailTab = tab;
    if (tab === 'dependencies' && selectedAppForDetails && dependencies.length === 0 && !loadingDependencies) {
      if (selectedAppForDetails.source === 'RPM' || selectedAppForDetails.source === 'Flatpak') {
        loadingDependencies = true;
        try {
          dependencies = await invoke<string[]>('get_app_dependencies', {
            packageId: selectedAppForDetails.package_id || selectedAppForDetails.name.toLowerCase(),
            source: selectedAppForDetails.source
          });
        } catch (e) {
          console.error("Failed to load dependencies", e);
        } finally {
          loadingDependencies = false;
        }
      }
    }
  }

  async function toggleFlatpakPermission(key: string, value: boolean) {
    if (!selectedAppForDetails || !selectedAppForDetails.package_id) return;
    try {
      await invoke('set_flatpak_permission', {
        appId: selectedAppForDetails.package_id,
        permission: key,
        enable: value
      });
      uiStore.addToast(`Updated Flatpak sandbox override: ${key} = ${value ? 'Allowed' : 'Denied'}`, 'success');
      if (flatpakPermissions) {
        (flatpakPermissions as any)[key] = value;
      }
    } catch (e) {
      uiStore.addToast(`Failed to toggle permission: ${e}`, 'error');
      // Revert local state
      if (flatpakPermissions) {
        (flatpakPermissions as any)[key] = !value;
      }
    }
  }

  async function toggleAppImageShortcut(value: boolean) {
    if (!selectedAppForDetails) return;
    registeringShortcut = true;
    try {
      await invoke('register_appimage', {
        name: selectedAppForDetails.name,
        execPath: selectedAppForDetails.exec,
        icon: 'system-run',
        createShortcut: value
      });
      
      appimageShortcutEnabled = value;
      
      // Update local state in apps array
      const index = apps.findIndex(a => a.exec === selectedAppForDetails?.exec);
      if (index !== -1) {
        const stem = selectedAppForDetails.name.replace(' ', '-').toLowerCase();
        const desktopFilename = value ? `appimage-${stem}.desktop` : "";
        apps[index].package_id = desktopFilename;
        selectedAppForDetails.package_id = desktopFilename;
      }
      uiStore.addToast(value ? 'Desktop entry created successfully' : 'Desktop entry removed', 'success');
    } catch (e) {
      uiStore.addToast(`Failed to update AppImage shortcut: ${e}`, 'error');
    } finally {
      registeringShortcut = false;
    }
  }

  function closeDetails() {
    isAppDrawerOpen = false;
    selectedAppForDetails = null;
    appDetails = null;
    dependencies = [];
    flatpakPermissions = null;
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

  function confirmRemoveFlatpak(app: FlatpakApp) {
    uiStore.confirm(
      'Remove Flatpak App',
      `Remove "${app.name}" (${app.app_id})?\n\nInstallation: ${app.installation}`,
      async () => {
        statusStore.setBusy(`Removing Flatpak: ${app.name}...`);
        try {
          await invoke('remove_flatpak', {
            appId: app.app_id,
            systemWide: app.installation === 'system',
          });
          uiStore.addToast(`Removed Flatpak: ${app.name}`, 'success');
          statusStore.setLastCommand(`flatpak uninstall -y ${app.app_id}`, 0, true);
          await loadApps();
        } catch (e) {
          uiStore.addToast(`Failed to remove Flatpak: ${e}`, 'error');
          statusStore.setLastCommand(`flatpak uninstall -y ${app.app_id}`, 1, false);
        } finally {
          statusStore.clearBusy();
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
        statusStore.setBusy(`Removing RPM: ${pkg.name}...`);
        try {
          await invoke('remove_rpm', { name: pkg.name });
          uiStore.addToast(`Removed RPM: ${pkg.name}`, 'success');
          statusStore.setLastCommand(`dnf remove -y ${pkg.name}`, 0, true);
          await loadApps();
        } catch (e) {
          uiStore.addToast(`Failed to remove RPM: ${e}`, 'error');
          statusStore.setLastCommand(`dnf remove -y ${pkg.name}`, 1, false);
        } finally {
          statusStore.clearBusy();
        }
      },
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

<div class="module-page">
  <PageHeader title="App Manager" icon={LayoutGrid}>
    <Button variant="ghost" size="sm" onclick={loadApps} disabled={loading || isUninstalling}>
      <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Refresh
    </Button>
  </PageHeader>

  {#if duplicates.length > 0 && sourceFilter !== 'Duplicates'}
    <div 
      style="margin-bottom: 20px; border: 1px solid rgba(245, 158, 11, 0.25); background: rgba(245, 158, 11, 0.04); padding: 10px 14px; border-radius: 8px; display: flex; align-items: center; justify-content: space-between; font-size: 12px;"
    >
      <div style="display: flex; align-items: center; gap: 8px; color: var(--color-text-primary);">
        <AlertTriangle size={15} style="color: var(--color-warning);" />
        <span>Detected {duplicates.length} duplicate app installations (both RPM and Flatpak).</span>
      </div>
      <button 
        onclick={() => sourceFilter = 'Duplicates'} 
        style="background: transparent; border: none; color: var(--color-accent); font-weight: 700; cursor: pointer; text-decoration: underline; padding: 0; font-size: 12px;"
      >
        View and Clean Up
      </button>
    </div>
  {/if}

  <div class="controls-row">
    <div style="display:flex; gap:16px; align-items:center; flex-wrap:wrap;">
      <SearchBar bind:value={filter} placeholder="Search installed apps..." disabled={isUninstalling} style="flex: 1; max-width: 300px;" />
      
      <TabGroup 
        tabs={[
          {id: 'All', label: 'All Sources'},
          {id: 'RPM', label: 'RPM'},
          {id: 'Flatpak', label: 'Flatpak'},
          {id: 'AppImage', label: 'AppImage'},
          ...(apps.some(a => a.source === 'Waydroid') ? [{id: 'Waydroid', label: 'Waydroid'}] : []),
          {id: 'Duplicates', label: `Duplicates (${duplicates.length})`}
        ]}
        bind:activeTab={sourceFilter}
        disabled={isUninstalling}
      />
    </div>
    
    <div style="display:flex;align-items:center;gap:16px;">
      <div style="display:flex; align-items:center; gap:8px;">
        <span style="font-size: 12px; color: var(--color-text-muted);width:-webkit-fill-available;">Sort by:</span>
        <Select bind:value={sortBy} disabled={isUninstalling}>
          <option value="name">Name</option>
          <option value="size">Size</option>
          <option value="date">Install Date</option>
          <option value="source">Source</option>
        </Select>
      </div>
      
      <div style="display:flex;align-items:center;gap:8px;padding:0 12px;height:36px;box-sizing:border-box;background:var(--color-bg-input);border:1px solid var(--color-border);border-radius:6px;">
        <span style="font-size:14px;font-weight:700;color:var(--color-accent);font-family:var(--font-mono);line-height:1;">{apps.length}</span>
        <span style="font-size:10px;color:var(--color-text-muted);text-transform:uppercase;letter-spacing:0.07em;font-weight:700;">Apps</span>
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
      {#if sourceFilter === 'Duplicates'}
        {#if duplicates.length === 0}
          <div class="empty-state" style="padding: 64px 32px; background: var(--color-bg-card); border-radius: 8px; border: 1px solid var(--color-border); width: 100%;">
            <div style="width:64px; height:64px; border-radius:50%; background:rgba(16, 185, 129, 0.08); border: 1px solid rgba(16, 185, 129, 0.15); display:flex; align-items:center; justify-content:center; margin:0 auto 16px; box-shadow: 0 0 12px rgba(16, 185, 129, 0.1);">
              <CheckCircle size={32} style="color:var(--color-success); margin:0" />
            </div>
            <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">
              No duplicate packages found
            </span>
            <span style="color:var(--color-text-muted); margin-top:8px; font-size:13px;">
              Your Flatpak and RPM installs don't overlap.
            </span>
          </div>
        {:else}
          <div style="display: flex; flex-direction: column; gap: 12px; width: 100%;">
            {#each duplicates as dup (dup.common_name)}
              <Card 
                style="display: flex; flex-direction: column; gap: 8px; padding: 16px; border: 1px solid var(--color-border); border-radius: 8px;"
              >
                <div style="font-weight: 700; font-size: 14px; color: var(--color-accent);">{dup.common_name}</div>
                <div style="font-size: 12px; color: var(--color-text-muted); margin-bottom: 6px;">{dup.recommendation}</div>
                <div style="display: flex; gap: 12px; flex-wrap: wrap;">
                  {#if dup.flatpak}
                    <div style="flex: 1; min-width: 240px; display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; background: rgba(0, 218, 243, 0.02); border: 1px solid rgba(0, 218, 243, 0.12); border-radius: 6px;">
                      <div style="display: flex; flex-direction: column; gap: 2px;">
                        <span style="font-size: 9px; font-weight: 700; color: var(--color-text-muted); text-transform: uppercase;">Flatpak</span>
                        <span style="font-size: 12px; font-family: var(--font-mono); color: var(--color-text-primary);">{dup.flatpak.app_id} (v{dup.flatpak.version})</span>
                      </div>
                      <Button
                        variant="danger"
                        size="sm"
                        style="padding: 4px 8px; font-size: 11px;"
                        onclick={() => dup.flatpak && confirmRemoveFlatpak(dup.flatpak)}
                      >
                        <Trash2 size={11} style="margin-right: 4px;" /> Remove Flatpak
                      </Button>
                    </div>
                  {/if}
                  {#if dup.rpm}
                    <div style="flex: 1; min-width: 240px; display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; background: rgba(188, 199, 222, 0.02); border: 1px solid rgba(188, 199, 222, 0.12); border-radius: 6px;">
                      <div style="display: flex; flex-direction: column; gap: 2px;">
                        <span style="font-size: 9px; font-weight: 700; color: var(--color-text-muted); text-transform: uppercase;">RPM</span>
                        <span style="font-size: 12px; font-family: var(--font-mono); color: var(--color-text-primary);">{dup.rpm.name} (v{dup.rpm.version})</span>
                      </div>
                      <Button
                        variant="danger"
                        size="sm"
                        style="padding: 4px 8px; font-size: 11px;"
                        onclick={() => dup.rpm && confirmRemoveRpm(dup.rpm)}
                      >
                        <Trash2 size={11} style="margin-right: 4px;" /> Remove RPM
                      </Button>
                    </div>
                  {/if}
                </div>
              </Card>
            {/each}
          </div>
        {/if}
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
            {@const AppIcon = getAppIcon(app.name)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <Card 
              class="app-card" 
              onclick={() => openDetails(app)} 
              oncontextmenu={(e) => handleAppContextMenu(e, app)}
              style="display: flex; align-items: center; gap: 10px; padding: 10px 12px; cursor: pointer; transition: all 0.2s ease;"
            >
              <div onclick={(e) => e.stopPropagation()} style="display: flex; align-items: center;">
                <input
                  type="checkbox"
                  class="form-checkbox"
                  checked={selectedAppNames.has(app.name)}
                  onchange={() => toggleAppSelection(app.name)}
                />
              </div>
              <div class="app-icon-wrapper" style="width: 36px; height: 36px; border-radius: 8px; background: var(--color-module-icon-bg, var(--color-bg-raised)); border: 1px solid var(--color-module-icon-border, var(--color-border)); display: flex; align-items: center; justify-content: center; flex-shrink: 0;">
                <AppIcon size={18} style="color: var(--color-accent);" />
              </div>
              <div class="app-info" style="flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;">
                <div 
                  class="app-name"
                  style="font-size: 13.5px; font-weight: 700; color: var(--color-text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;"
                  onmouseenter={(e) => { if (app.package_id) onMouseEnter(e, app.package_id); }}
                  onmouseleave={onMouseLeave}
                >
                  {app.name}
                </div>
                <div class="app-meta" style="display: flex; align-items: center; gap: 8px;">
                  <span class="badge" class:flatpak={app.source === 'Flatpak'} class:rpm={app.source === 'RPM'} class:appimage={app.source === 'AppImage'} class:waydroid={app.source === 'Waydroid'} class:local={app.source === 'Local'} style="font-size: 9.5px;">
                    {app.source}
                  </span>
                  
                  {#if app.source === 'AppImage'}
                    <span style="font-size: 11px; color: var(--color-text-muted);">
                      Local Binary
                    </span>
                  {:else if app.source === 'Waydroid'}
                    <span style="font-size: 11px; color: var(--color-text-muted);">
                      Waydroid App
                    </span>
                  {:else if app.source === 'Local'}
                    <span style="font-size: 11px; color: var(--color-text-muted);">
                      Local App
                    </span>
                  {:else if app.sizeBytes === undefined}
                    <div class="skeleton-text" style="width: 60px; height: 12px;"></div>
                  {:else if app.sizeBytes === 0}
                    <span class="size-text" style="font-size: 11px; color: var(--color-text-muted); display: flex; align-items: center; gap: 4px;">
                      <HardDrive size={11} style="opacity: 0.7;" />
                      —
                    </span>
                  {:else}
                    <span class="size-text" style="font-size: 11px; color: var(--color-text-muted); display: flex; align-items: center; gap: 4px;">
                      <HardDrive size={11} style="opacity: 0.7;" />
                      {formatBytes(app.sizeBytes)}
                    </span>
                  {/if}
                </div>
              </div>
              <div class="app-actions">
                <Button 
                  variant="ghost" 
                  class="trash-action-btn" 
                  style="color: var(--color-text-muted); padding: 4px;"
                  disabled={!app.package_id} 
                  onclick={(e: any) => { e.stopPropagation(); confirmUninstall(app); }} 
                  title="Uninstall"
                >
                  <Trash2 size={15} />
                </Button>
              </div>
            </Card>
          {/each}
        </div>
      {/if}
    {/if}
  {/if}
  </div>

  <!-- App Details SideDrawer -->
  {#if selectedAppForDetails}
    <SideDrawer
      bind:isOpen={isAppDrawerOpen}
      title={selectedAppForDetails.name}
      width="560px"
      dockable={true}
    >
      {#snippet headerActions()}
        <span class="badge" class:flatpak={selectedAppForDetails.source === 'Flatpak'} class:rpm={selectedAppForDetails.source === 'RPM'} class:appimage={selectedAppForDetails.source === 'AppImage'} style="font-size: 11px; margin-right: 8px;">
          {selectedAppForDetails.source}
        </span>
      {/snippet}

      <!-- Detail Panel Tabs -->
      <div class="panel-tabs" style="display:flex; border-bottom:1px solid var(--color-border); background:var(--color-bg-card); padding: 0 16px; position:relative; z-index:10; margin: -12px -16px 12px -16px;">
        <button 
          type="button"
          class="panel-tab-btn" 
          class:active={activeDetailTab === 'details'} 
          onclick={() => selectDetailTab('details')}
        >
          Details
        </button>
        {#if selectedAppForDetails.source === 'Flatpak'}
          <button 
            type="button"
            class="panel-tab-btn" 
            class:active={activeDetailTab === 'permissions'} 
            onclick={() => selectDetailTab('permissions')}
          >
            Permissions
          </button>
        {/if}
        {#if selectedAppForDetails.source !== 'AppImage'}
          <button 
            type="button"
            class="panel-tab-btn" 
            class:active={activeDetailTab === 'dependencies'} 
            onclick={() => selectDetailTab('dependencies')}
          >
            Dependencies
          </button>
        {/if}
      </div>
      
      <div class="panel-content" style="padding: 0;">

        <!-- ====== DETAILS TAB ====== -->
        {#if activeDetailTab === 'details'}
          <div class="panel-section">
            <div class="detail-label">Full Package Name</div>
            <div class="detail-value" style="font-family: var(--font-mono); color: var(--color-text-accent); word-break: break-all;">{selectedAppForDetails.package_id || 'Unknown'}</div>
          </div>

          <div class="panel-section">
            <div class="detail-label">Exec Command</div>
            <div class="detail-value" style="font-family: var(--font-mono); font-size:11px; word-break: break-all;">{selectedAppForDetails.exec || 'N/A'}</div>
          </div>

          <div class="panel-section">
            <div class="detail-label">Desktop Entry Path</div>
            <div class="detail-value" style="font-family: var(--font-mono); font-size:11px; word-break: break-all; color: var(--color-text-muted);">{selectedAppForDetails.file_path}</div>
          </div>

          <div class="panel-section">
            <div class="detail-label">Version</div>
            <div class="detail-value">{appDetails?.version || 'Loading...'}</div>
          </div>

          <div class="panel-section">
            <div class="detail-label">Description</div>
            <div class="detail-value" style="font-size:12px; line-height: 1.5; color: var(--color-text-secondary);">{appDetails?.description || 'No description available.'}</div>
          </div>

          {#if appDetails?.files && appDetails.files.length > 0}
            <div class="panel-section">
              <div class="detail-label">Installed Files ({appDetails.files.length})</div>
              <div class="files-list">
                {#each showMoreFiles ? appDetails.files : appDetails.files.slice(0, 15) as file}
                  <div class="file-item">{file}</div>
                {/each}
                {#if appDetails.files.length > 15 && !showMoreFiles}
                  <Button variant="ghost" size="sm" onclick={() => showMoreFiles = true} style="margin-top: 8px; width: 100%;">
                    Show all {appDetails.files.length} files
                  </Button>
                {/if}
              </div>
            </div>
          {/if}

        <!-- ====== PERMISSIONS TAB ====== -->
        {:else if activeDetailTab === 'permissions'}
          {#if loadingFlatpakPermissions}
            <div style="display:flex; justify-content:center; padding: 48px 0;">
              <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-text-muted)" />
            </div>
          {:else if flatpakPermissions}
            <div style="display:flex; flex-direction:column; gap:16px;">
              <div style="font-size:12px; color:var(--color-text-muted);">
                Granular sandbox overrides and system access rights:
              </div>

              <!-- Permission Toggles -->
              <div style="display:flex; flex-direction:column; gap:12px; background:rgba(0,0,0,0.15); border:1px solid var(--color-border); border-radius:8px; padding:12px;">
                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">Network Access</span>
                    <span style="font-size:11px; color:var(--color-text-muted);">Allow outgoing and incoming internet traffic</span>
                  </div>
                  <Toggle checked={flatpakPermissions.network} onchange={(e: any) => toggleFlatpakPermission('network', e.target.checked)} />
                </div>

                <div style="height:1px; background:var(--color-border); opacity:0.5;"></div>

                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">IPC (Inter-Process)</span>
                    <span style="font-size:11px; color:var(--color-text-muted);">Shared IPC namespace with host</span>
                  </div>
                  <Toggle checked={flatpakPermissions.ipc} onchange={(e: any) => toggleFlatpakPermission('ipc', e.target.checked)} />
                </div>

                <div style="height:1px; background:var(--color-border); opacity:0.5;"></div>

                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">X11 Windowing System</span>
                    <span style="font-size:11px; color:var(--color-text-muted);">Legacy display protocol fallback</span>
                  </div>
                  <Toggle checked={flatpakPermissions.x11} onchange={(e: any) => toggleFlatpakPermission('x11', e.target.checked)} />
                </div>

                <div style="height:1px; background:var(--color-border); opacity:0.5;"></div>

                <div style="display:flex; align-items:center; justify-content:space-between;">
                  <div style="display:flex; flex-direction:column;">
                    <span style="font-size:13px; font-weight:600; color:var(--color-text-primary);">Wayland Windowing System</span>
                    <span style="font-size:11px; color:var(--color-text-muted);">Native Wayland graphical display support</span>
                  </div>
                  <Toggle checked={flatpakPermissions.wayland} onchange={(e: any) => toggleFlatpakPermission('wayland', e.target.checked)} />
                </div>
              </div>
            </div>
          {:else}
            <div style="text-align:center; padding:32px; color:var(--color-text-muted); font-size:13px;">
              No permission data available for this package.
            </div>
          {/if}

        <!-- ====== DEPENDENCIES TAB ====== -->
        {:else if activeDetailTab === 'dependencies'}
          {#if loadingDependencies}
            <div style="display:flex; justify-content:center; padding: 48px 0;">
              <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-text-muted)" />
            </div>
          {:else if dependencies && dependencies.length > 0}
            <div style="display:flex; flex-direction:column; gap:12px;">
              <div style="font-size:12px; color:var(--color-text-muted);">
                Required libraries and runtime packages ({dependencies.length}):
              </div>
              <div class="files-list" style="max-height:400px;">
                {#each dependencies as dep}
                  <div class="file-item" style="display:flex; align-items:center; gap:8px;">
                    <Layers size={13} style="color:var(--color-accent); flex-shrink:0;" />
                    <span>{dep}</span>
                  </div>
                {/each}
              </div>
            </div>
          {:else}
            <div style="text-align:center; padding:48px 32px; color:var(--color-text-muted); font-size:13px; display:flex; flex-direction:column; align-items:center; gap:12px;">
              <Layers size={32} style="opacity:0.3;" />
              <span>No dependencies found for this package.</span>
            </div>
          {/if}
        {/if}

      </div>

      <!-- Fixed Footer Action Bar -->
      {#if selectedAppForDetails.source !== 'AppImage'}
        <div class="panel-footer" style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--color-border);">
          <Button variant="outline" class="text-danger" style="width: 100%; border-color: rgba(239,68,68,0.25);" onclick={() => confirmUninstall(selectedAppForDetails!)}>
            <Trash2 size={16} style="margin-right:8px;" /> Uninstall Application
          </Button>
        </div>
      {/if}
    </SideDrawer>
  {/if}
</div>

{#if contextMenu.app}
  <ContextMenu
    bind:isOpen={contextMenu.show}
    x={contextMenu.x}
    y={contextMenu.y}
    title={contextMenu.app.name}
    subtitle={contextMenu.app.exec || contextMenu.app.file_path || 'Desktop Application'}
    badge={{ text: contextMenu.app.source, variant: contextMenu.app.source === 'Flatpak' ? 'info' : (contextMenu.app.source === 'RPM' ? 'warning' : 'muted') }}
    icon={AppWindow}
    items={[
      {
        label: 'Launch Application',
        icon: Play,
        action: () => handleLaunchApp(contextMenu.app!)
      },
      {
        label: 'Inspect App Details',
        icon: Info,
        action: () => openDetails(contextMenu.app!)
      },
      ...(contextMenu.app.source === 'Flatpak' ? [{
        label: 'Inspect Flatpak Sandbox Permissions',
        icon: Shield,
        action: () => {
          openDetails(contextMenu.app!);
          activeDetailTab = 'permissions';
        }
      }] : [{
        label: 'Open Desktop File Location',
        icon: FolderOpen,
        disabled: !contextMenu.app.file_path,
        action: () => handleOpenDesktopFile(contextMenu.app!)
      }]),
      {
        label: 'Search App on Web',
        icon: Globe,
        action: () => searchWeb(`${contextMenu.app!.name} linux desktop application`)
      },
      { divider: true, label: '' },
      {
        label: `Copy App ID (${contextMenu.app.package_id || contextMenu.app.name})`,
        icon: Copy,
        action: () => handleCopyAppId(contextMenu.app!)
      },
      {
        label: 'Copy Launch Command',
        icon: Terminal,
        action: () => {
          navigator.clipboard.writeText(contextMenu.app!.exec || contextMenu.app!.name);
          uiStore.addToast('Copied launch command to clipboard', 'info');
        }
      },
      ...(contextMenu.app.source !== 'AppImage' ? [
        { divider: true, label: '' },
        {
          label: 'Uninstall Application',
          icon: Trash2,
          danger: true,
          action: () => confirmUninstall(contextMenu.app!)
        }
      ] : [])
    ]}
  />
{/if}

<!-- Bulk Action Bar for Installed Apps -->
<BulkActionBar
  selectedCount={selectedAppNames.size}
  itemLabel="apps"
  onclear={deselectAllApps}
>
  <button
    type="button"
    class="btn-bulk-action btn-bulk-danger"
    onclick={bulkUninstallSelected}
  >
    <Trash2 size={12} />
    <span>Batch Uninstall ({selectedAppNames.size})</span>
  </button>
</BulkActionBar>

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
    padding: 8px 16px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-base);
    flex-shrink: 0;
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    background: var(--color-bg-subtle);
  }

  .app-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 8px;
  }

  .app-card:hover {
    border-color: var(--color-accent) !important;
    transform: translateY(-2px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
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
    background: rgba(188, 199, 222, 0.1);
    color: var(--color-text-secondary);
    border: 1px solid rgba(188, 199, 222, 0.2);
  }

  .badge.rpm {
    background: rgba(0, 218, 243, 0.1);
    color: var(--color-accent);
    border: 1px solid rgba(0, 218, 243, 0.2);
  }

  .badge.waydroid {
    background: rgba(34, 197, 94, 0.1);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.25);
  }

  .badge.local {
    background: rgba(168, 85, 247, 0.1);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.25);
  }

  .badge.appimage {
    background: rgba(234, 179, 8, 0.1);
    color: #facc15;
    border: 1px solid rgba(234, 179, 8, 0.25);
  }

  .app-actions {
    display: flex;
    align-items: center;
  }

  :global(.trash-action-btn) {
    border: none !important;
    background: transparent !important;
  }

  :global(.trash-action-btn:hover) {
    color: var(--color-error) !important;
    background: rgba(255, 180, 171, 0.08) !important;
    border: none !important;
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

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
    display: flex;
    flex-direction: column;
  }

  .panel-footer {
    padding: 14px 20px;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-card, var(--color-bg-raised));
    display: flex;
    align-items: center;
  }

  .panel-section {
    margin-bottom: 20px;
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
    background: var(--color-surface, rgba(0,0,0,0.2));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px 0;
    max-height: 220px;
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

  .panel-tab-btn {
    padding: 12px 16px;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--color-text-muted);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }
  .panel-tab-btn:hover {
    color: var(--color-text-primary);
  }
  .panel-tab-btn.active {
    color: var(--color-accent);
    border-bottom-color: var(--color-accent);
  }

  .badge.appimage {
    background: rgba(16, 185, 129, 0.1);
    color: var(--color-success);
    border: 1px solid rgba(16, 185, 129, 0.2);
  }

  /* ── Custom Context Menu ────────────────────────────────────────── */
  .custom-context-menu {
    background: var(--color-bg-card, #131b26);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px;
    min-width: 220px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-menu-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    gap: 8px;
  }

  .context-menu-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 130px;
  }

  .context-menu-badge {
    font-size: 9.5px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(0, 218, 243, 0.12);
    color: var(--color-accent);
    text-transform: uppercase;
  }

  .context-menu-divider {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .context-menu-item:hover:not(:disabled) {
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--color-text-primary);
  }

  .context-menu-item:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .context-menu-item.text-danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: var(--color-error);
  }

  :global(html.light-mode) .files-list {
    background: #F1F5F9 !important;
    border-color: #CBD5E1 !important;
  }
  :global(html.light-mode) .file-item {
    color: #334155 !important;
  }
  :global(html.light-mode) .file-item:hover {
    background: #E2E8F0 !important;
    color: #0F172A !important;
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
