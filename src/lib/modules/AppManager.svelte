<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import {
    AppWindow, Search, RefreshCw, Trash2, LayoutGrid, Terminal, X,
    Layers, AlertTriangle, CheckCircle, Play, FolderOpen, Copy,
    Info, Package, Smartphone, ImagePlus, History, Eye
  } from '@lucide/svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import Button from '../components/ui/Button.svelte';
  import Card from '../components/ui/Card.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import Select from '../components/ui/Select.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import ToggleButton from '../components/ui/ToggleButton.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Table from '../components/ui/Table.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import ContextMenu, { type ContextMenuItem } from '../components/ui/ContextMenu.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  interface DesktopApp {
    name: string;
    exec: string;
    source: string;
    package_id: string | null;
    file_path: string;
    size_bytes?: number;
    sizeBytes?: number;
    removable?: boolean;
    hidden?: boolean;
    installDate?: number;
    version?: string;
    /** Inlined `data:` URL of the entry's icon, when one could be resolved. */
    icon?: string | null;
    /** AppImage only: the user desktop entry merged into this row, if any. */
    desktop_path?: string | null;
    install_date?: number;
  }

  interface AppMeta {
    size_bytes: number;
    install_date: number;
    version: string;
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

  type SourceFilter = 'All' | 'RPM' | 'Flatpak' | 'AppImage' | 'Waydroid' | 'Duplicates';
  type SortOption = 'name' | 'source' | 'size' | 'date';
  type DetailTab = 'details' | 'permissions' | 'dependencies';

  const SOURCE_FILTERS: SourceFilter[] = [
    'All', 'RPM', 'Flatpak', 'AppImage', 'Waydroid', 'Duplicates',
  ];

  let apps = $state<DesktopApp[]>([]);
  let duplicates = $state<DuplicateEntry[]>([]);
  let loading = $state(false);
  let filter = $state('');
  let sortBy = $state<SortOption>('name');
  let sortDir = $state<'asc' | 'desc'>('asc');
  /** `NoDisplay`/`Hidden` entries are excluded by default; the toggle reveals them. */
  let showHidden = $state(false);
  let lastScannedAt = $state<Date | null>(null);

  let sourceFilter = $state<SourceFilter>(
    uiStore.appSourceFilter && SOURCE_FILTERS.includes(uiStore.appSourceFilter as SourceFilter)
      ? (uiStore.appSourceFilter as SourceFilter)
      : uiStore.targetSubTab && SOURCE_FILTERS.includes(uiStore.targetSubTab as SourceFilter)
        ? (uiStore.targetSubTab as SourceFilter)
        : 'All'
  );
  if (uiStore.appSourceFilter) uiStore.appSourceFilter = null;
  if (uiStore.targetSubTab && SOURCE_FILTERS.includes(uiStore.targetSubTab as SourceFilter)) {
    uiStore.targetSubTab = null;
  }

  // ─── Load ──────────────────────────────────────────────────────────────────

  async function loadApps() {
    loading = true;
    statusStore.setBusy('Scanning applications…');
    try {
      const [appsResult, duplicatesResult, appimagesResult] = await Promise.all([
        invoke<DesktopApp[]>('list_desktop_apps'),
        invoke<DuplicateEntry[]>('detect_duplicates').catch(() => []),
        invoke<DesktopApp[]>('scan_local_appimages').catch(() => []),
      ]);

      // A desktop entry merged into its AppImage row must not stay in the list
      // as a second, Local row for the same application.
      const mergedDesktopPaths = new Set(
        appimagesResult
          .map((app) => app.desktop_path)
          .filter((path): path is string => Boolean(path))
      );
      let next: DesktopApp[] = [...appsResult, ...appimagesResult]
        .filter((app) => !mergedDesktopPaths.has(app.file_path))
        .map((app) => ({
          ...app,
          sizeBytes: app.size_bytes ?? 0,
          installDate: app.install_date || undefined,
        }));

      // One batched metadata call keeps the table stable while it loads: the
      // old per-row `get_app_meta` promises used to reorder the list mid-scan.
      const requests: { package_id: string; source: string }[] = [];
      const requestIndexes: number[] = [];
      next.forEach((app, index) => {
        if (app.package_id && (app.source === 'RPM' || app.source === 'Flatpak')) {
          requests.push({ package_id: app.package_id, source: app.source });
          requestIndexes.push(index);
        }
      });
      if (requests.length > 0) {
        try {
          const metas = await invoke<AppMeta[]>('get_app_metas', { items: requests });
          next = next.slice();
          requestIndexes.forEach((appIndex, metaIndex) => {
            const meta = metas[metaIndex];
            if (!meta) return;
            next[appIndex] = {
              ...next[appIndex],
              sizeBytes: meta.size_bytes,
              installDate: meta.install_date,
              version: meta.version,
            };
          });
        } catch (error) {
          console.warn('Could not load application metadata', error);
        }
      }

      apps = next;
      duplicates = duplicatesResult;
      lastScannedAt = new Date();
    } catch (e) {
      uiStore.addToast(`Failed to load applications: ${e}`, 'error');
    } finally {
      loading = false;
      statusStore.clearBusy();
    }
  }

  // ─── Derived views ─────────────────────────────────────────────────────────

  const baseApps = $derived(showHidden ? apps : apps.filter((app) => !app.hidden));
  const hiddenCount = $derived(apps.filter((app) => app.hidden).length);

  const sourceCounts = $derived.by(() => {
    const counts: Record<string, number> = {
      All: baseApps.length,
      Duplicates: duplicates.length,
    };
    for (const app of baseApps) {
      counts[app.source] = (counts[app.source] ?? 0) + 1;
    }
    return counts;
  });

  const sourceFilterOptions = $derived.by(() =>
    (['All', 'RPM', 'Flatpak', 'AppImage', 'Waydroid', 'Duplicates'] as SourceFilter[])
      .filter((id) => id !== 'Waydroid' || (sourceCounts.Waydroid ?? 0) > 0)
      .map((id) => ({ id, label: sourceFilterLabel(id), count: sourceCounts[id] ?? 0 }))
  );

  const filteredApps = $derived.by(() => {
    const query = filter.trim().toLowerCase();
    const direction = sortDir === 'asc' ? 1 : -1;
    const list = baseApps.filter((app) => {
      const matchesName =
        !query ||
        app.name.toLowerCase().includes(query) ||
        (app.package_id ?? '').toLowerCase().includes(query) ||
        app.file_path.toLowerCase().includes(query);
      const matchesSource = sourceFilter === 'All' || app.source === sourceFilter;
      return matchesName && matchesSource;
    });
    list.sort((a, b) => {
      let result = 0;
      if (sortBy === 'name') result = a.name.localeCompare(b.name);
      else if (sortBy === 'source') result = a.source.localeCompare(b.source);
      else if (sortBy === 'size') result = (a.sizeBytes ?? 0) - (b.sizeBytes ?? 0);
      else if (sortBy === 'date') result = (a.installDate ?? 0) - (b.installDate ?? 0);
      if (result === 0) result = a.name.localeCompare(b.name);
      return result * direction;
    });
    return list;
  });

  const filteredDuplicates = $derived.by(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return duplicates;
    return duplicates.filter(
      (duplicate) =>
        duplicate.common_name.toLowerCase().includes(query) ||
        (duplicate.flatpak?.app_id ?? '').toLowerCase().includes(query) ||
        (duplicate.rpm?.name ?? '').toLowerCase().includes(query)
    );
  });

  const dependencyGroups = $derived.by(() => {
    const packages: string[] = [];
    const system: string[] = [];
    for (const dependency of dependencies) {
      if (dependency.includes('(') || dependency.startsWith('/')) system.push(dependency);
      else packages.push(dependency);
    }
    return { packages, system };
  });

  function isRemovable(app: DesktopApp) {
    return app.removable !== false;
  }

  function sourceFilterLabel(id: SourceFilter) {
    if (id === 'RPM') return 'Native';
    if (id === 'Flatpak') return 'Flatpak';
    if (id === 'AppImage') return 'AppImage';
    if (id === 'Waydroid') return 'Waydroid';
    if (id === 'Duplicates') return 'Duplicates';
    return 'All';
  }

  function sourceLabel(source: string) {
    return source === 'RPM' ? 'Native' : source;
  }

  /** The context-menu header accepts a narrower set than Badge. */
  function contextBadgeVariant(source: string): 'info' | 'muted' | 'warning' | 'success' {
    const variant = sourceVariant(source);
    return variant === 'outline' ? 'muted' : variant;
  }

  function sourceVariant(source: string): 'info' | 'muted' | 'warning' | 'success' | 'outline' {
    if (source === 'RPM') return 'info';
    if (source === 'Flatpak') return 'muted';
    if (source === 'AppImage') return 'warning';
    if (source === 'Waydroid') return 'success';
    return 'outline';
  }

  function appIcon(app: DesktopApp) {
    if (app.source === 'RPM') return Package;
    if (app.source === 'Flatpak') return Layers;
    if (app.source === 'Waydroid') return Smartphone;
    if (app.source === 'Local') return Terminal;
    return AppWindow;
  }

  function formatBytes(bytes?: number) {
    if (!bytes) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    return `${parseFloat((bytes / Math.pow(1024, exponent)).toFixed(2))} ${units[exponent]}`;
  }

  function formatDate(unixSeconds?: number) {
    if (!unixSeconds) return '—';
    return new Date(unixSeconds * 1000).toLocaleDateString();
  }

  function toggleSort(column: SortOption) {
    if (sortBy === column) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortBy = column;
      sortDir = column === 'size' || column === 'date' ? 'desc' : 'asc';
    }
  }

  // ─── Side panel ────────────────────────────────────────────────────────────

  let selectedAppForDetails = $state<DesktopApp | null>(null);
  let appDetails = $state<AppDetails | null>(null);
  let loadingDetails = $state(false);
  let showMoreFiles = $state(false);
  let activeDetailTab = $state<DetailTab>('details');

  let flatpakPermissions = $state<FlatpakPermissions | null>(null);
  let loadingPermissions = $state(false);

  let dependencies = $state<string[]>([]);
  let loadingDependencies = $state(false);

  let appimageShortcutEnabled = $state(false);
  let registeringShortcut = $state(false);
  let readingIcon = $state(false);
  let deletingAppImage = $state(false);

  const PERMISSIONS: { key: keyof FlatpakPermissions; label: string; hint: string }[] = [
    { key: 'network', label: 'Network access', hint: 'Access the local network and the internet' },
    { key: 'ipc', label: 'Inter-process communication', hint: 'Share memory and signals with other processes' },
    { key: 'wayland', label: 'Wayland', hint: 'Native Wayland display support' },
    { key: 'x11', label: 'X11', hint: 'Legacy X11 display support' },
    { key: 'fallback_x11', label: 'Fallback X11', hint: 'X11 only when Wayland is unavailable' },
    { key: 'pulseaudio', label: 'Audio (PulseAudio)', hint: 'Sound output and microphone access' },
    { key: 'gpu', label: 'GPU access', hint: 'Hardware accelerated rendering' },
    { key: 'host_files', label: 'All host files', hint: 'Read and write every file on the system' },
    { key: 'home_files', label: 'Home folder', hint: 'Read and write files in your home folder' },
  ];

  async function openDetails(app: DesktopApp) {
    selectedAppForDetails = app;
    appDetails = null;
    loadingDetails = true;
    showMoreFiles = false;
    activeDetailTab = 'details';
    dependencies = [];
    flatpakPermissions = null;
    appimageShortcutEnabled = app.source === 'AppImage' && Boolean(app.package_id);

    // An AppImage has no package record; its panel describes the file itself.
    const detailRequest =
      app.package_id && app.source !== 'AppImage'
        ? invoke<AppDetails>('get_app_details', {
            packageId: app.package_id,
            source: app.source,
          }).catch((error) => ({ version: 'Unknown', description: String(error), files: [] }))
        : Promise.resolve({
            version: '',
            description: '',
            files: [] as string[],
          });

    appDetails = await detailRequest;
    loadingDetails = false;

    if (app.source === 'RPM' || app.source === 'Flatpak') {
      loadingDependencies = true;
      invoke<string[]>('get_app_dependencies', {
        packageId: app.package_id,
        source: app.source,
      })
        .then((result) => (dependencies = result))
        .catch((error) => console.error('Failed to load dependencies', error))
        .finally(() => (loadingDependencies = false));
    }

    if (app.source === 'Flatpak' && app.package_id) {
      loadingPermissions = true;
      invoke<FlatpakPermissions>('get_flatpak_permissions', { appId: app.package_id })
        .then((result) => (flatpakPermissions = result))
        .catch((error) => console.error('Failed to load Flatpak permissions', error))
        .finally(() => (loadingPermissions = false));
    }
  }

  function closeDetails() {
    selectedAppForDetails = null;
    appDetails = null;
    dependencies = [];
    flatpakPermissions = null;
  }

  async function toggleFlatpakPermission(key: keyof FlatpakPermissions, value: boolean) {
    if (!selectedAppForDetails?.package_id || !flatpakPermissions) return;
    const previous = flatpakPermissions[key];
    flatpakPermissions[key] = value;
    try {
      await invoke('set_flatpak_permission', {
        appId: selectedAppForDetails.package_id,
        permission: key,
        enable: value,
      });
      uiStore.addToast(
        `${PERMISSIONS.find((entry) => entry.key === key)?.label ?? key}: ${value ? 'allowed' : 'blocked'}`,
        'success'
      );
    } catch (error) {
      flatpakPermissions[key] = previous;
      uiStore.addToast(`Failed to change the permission: ${error}`, 'error');
    }
  }

  // ─── AppImage actions ──────────────────────────────────────────────────────

  /**
   * Re-read the AppImage rows after a menu-entry or icon change. Scanning them
   * again (it is cheap) gives the row the real desktop entry name, icon and
   * package ID, including one another tool created for the same file.
   */
  async function refreshAppImageRow(execPath: string) {
    const refreshed = await invoke<DesktopApp[]>('scan_local_appimages');
    const updated = refreshed.find((entry) => entry.file_path === execPath);
    if (!updated) return;
    const normalized = {
      ...updated,
      sizeBytes: updated.size_bytes ?? 0,
      installDate: updated.install_date || undefined,
    };
    apps = apps.map((entry) =>
      entry.file_path === execPath ? { ...entry, ...normalized } : entry
    );
    if (selectedAppForDetails?.file_path === execPath) {
      selectedAppForDetails = { ...selectedAppForDetails, ...normalized };
      appimageShortcutEnabled = Boolean(updated.package_id);
    }
  }

  async function setAppImageShortcut(app: DesktopApp, enabled: boolean, icon?: string | null) {
    if (registeringShortcut) return;
    registeringShortcut = true;
    // Creating an entry without a picked icon runs the AppImage's runtime to
    // read its `.DirIcon`; that can take a few seconds, so say so.
    const readsIcon = enabled && !icon;
    if (readsIcon) statusStore.setBusy('Reading the icon from the AppImage…');
    try {
      await invoke('register_appimage', {
        name: app.name,
        execPath: app.exec,
        icon: icon ?? null,
        createShortcut: enabled,
      });
      await refreshAppImageRow(app.file_path).catch((error) => {
        console.warn('Could not refresh the AppImage row', error);
      });
      uiStore.addToast(
        enabled ? 'Added to the applications menu' : 'Removed from the applications menu',
        'success'
      );
    } catch (error) {
      uiStore.addToast(`Could not update the menu entry: ${error}`, 'error');
    } finally {
      registeringShortcut = false;
      if (readsIcon) statusStore.clearBusy();
    }
  }

  async function readAppImageIcon(app: DesktopApp) {
    if (readingIcon) return;
    readingIcon = true;
    statusStore.setBusy('Reading the icon from the AppImage…');
    try {
      const icon = await invoke<string | null>('read_appimage_icon', {
        execPath: app.file_path,
        force: true,
      });
      if (!icon) {
        uiStore.addToast('This AppImage does not contain an icon.', 'info');
        return;
      }
      apps = apps.map((entry) =>
        entry.file_path === app.file_path ? { ...entry, icon } : entry
      );
      if (selectedAppForDetails?.file_path === app.file_path) {
        selectedAppForDetails = { ...selectedAppForDetails, icon };
      }
      uiStore.addToast('Read the icon from the AppImage', 'success');
    } catch (error) {
      uiStore.addToast(`Could not read the icon: ${error}`, 'error');
    } finally {
      readingIcon = false;
      statusStore.clearBusy();
    }
  }

  async function chooseAppImageIcon(app: DesktopApp) {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      // XPM cannot be drawn by the webview, so it is not offered.
      filters: [{ name: 'Images', extensions: ['png', 'svg'] }],
    });
    if (!selected || Array.isArray(selected)) return;
    await setAppImageShortcut(app, true, selected);
  }

  function confirmDeleteAppImage(app: DesktopApp) {
    uiStore.confirm(
      'Delete AppImage',
      [
        `Permanently delete "${app.name}"?`,
        '',
        `File: ${app.file_path}`,
        app.package_id ? 'The applications-menu entry is removed too.' : '',
        '',
        'The file is deleted from disk and cannot be restored.',
      ]
        .filter((line) => line !== '')
        .join('\n'),
      () => deleteAppImage(app),
      true
    );
  }

  async function deleteAppImage(app: DesktopApp) {
    deletingAppImage = true;
    statusStore.setBusy(`Deleting ${app.name}…`);
    try {
      await invoke('delete_appimage', { execPath: app.file_path });
      if (selectedAppForDetails?.file_path === app.file_path) closeDetails();
      uiStore.addToast(`Deleted ${app.name}`, 'success');
      await loadApps();
    } catch (error) {
      uiStore.addToast(`Could not delete the AppImage: ${error}`, 'error');
    } finally {
      deletingAppImage = false;
      statusStore.clearBusy();
    }
  }

  // ─── Launch / reveal / copy ────────────────────────────────────────────────

  async function launchApp(app: DesktopApp) {
    try {
      uiStore.addToast(`Launching ${app.name}…`, 'info');
      await invoke('launch_desktop_app', { filePath: app.file_path });
    } catch (error) {
      uiStore.addToast(`Failed to launch: ${error}`, 'error');
    }
  }

  async function revealFile(app: DesktopApp) {
    try {
      await invoke('reveal_in_file_manager', { path: app.file_path });
    } catch {
      await navigator.clipboard.writeText(app.file_path);
      uiStore.addToast('Copied the file path instead', 'info');
    }
  }

  async function copyPackageId(app: DesktopApp) {
    const id = app.source === 'AppImage' ? app.file_path : app.package_id || app.name;
    await navigator.clipboard.writeText(id);
    uiStore.addToast(`Copied: ${id}`, 'info');
  }

  // ─── Uninstall ─────────────────────────────────────────────────────────────

  let uninstallLog = $state<string[]>([]);
  let uninstallingApp = $state<DesktopApp | null>(null);
  let isUninstalling = $state(false);
  let uninstallSucceeded = $state<boolean | null>(null);
  let progressOpen = $state(false);
  let logContainer = $state<HTMLElement | null>(null);

  const MAX_LOG_LINES = 500;

  function appendLog(line: string) {
    // A long dnf run can print thousands of lines; keep the drawer readable
    // and the DOM bounded.
    const next = [...uninstallLog, line];
    uninstallLog = next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next;
    queueMicrotask(async () => {
      await tick();
      if (logContainer) logContainer.scrollTop = logContainer.scrollHeight;
    });
  }

  function commandLabel(app: DesktopApp) {
    if (app.source === 'Flatpak') return `flatpak uninstall --user -y ${app.package_id}`;
    if (app.source === 'AppImage' || app.source === 'Local') return `rm ${app.file_path}`;
    return `dnf remove -y ${app.package_id}`;
  }

  function previewPackages(preview: string): string[] {
    const packages: string[] = [];
    let inTable = false;
    for (const raw of preview.split('\n')) {
      const line = raw.trim();
      const lower = line.toLowerCase();
      if (lower.startsWith('removing:')) {
        inTable = true;
        continue;
      }
      if (lower.startsWith('transaction summary')) {
        inTable = false;
        continue;
      }
      const apt = line.match(/^Remv\s+(\S+)/);
      if (apt) {
        packages.push(apt[1]);
        continue;
      }
      if (!inTable || !line || line.startsWith('=')) continue;
      if (lower.startsWith('package ') || lower.startsWith('removing ')) continue;
      const token = line.split(/\s+/)[0];
      if (/^[A-Za-z0-9]/.test(token)) packages.push(token);
    }
    return [...new Set(packages)].sort();
  }

  async function confirmUninstall(app: DesktopApp) {
    if (app.source === 'AppImage') {
      confirmDeleteAppImage(app);
      return;
    }
    if (!app.package_id) {
      uiStore.addToast('Cannot uninstall: the package ID is unknown', 'error');
      return;
    }
    if (!isRemovable(app)) {
      uiStore.addToast(
        'This launcher is installed system-wide and is not owned by a package, so it can only be removed by an administrator.',
        'warning'
      );
      return;
    }

    let expectedPreview: string | undefined;
    if (app.source === 'RPM') {
      try {
        expectedPreview = await invoke<string>('preview_rpm_removal', { name: app.package_id });
      } catch (error) {
        uiStore.addToast(`Could not prepare a safe removal preview: ${error}`, 'error');
        return;
      }
    }

    const packages = expectedPreview ? previewPackages(expectedPreview) : [];
    const systemWide = app.source === 'Flatpak' && app.file_path.includes('/var/lib/flatpak/');
    const summary: string[] = [`Remove "${app.name}"?`, ''];
    if (app.source === 'RPM') {
      summary.push(
        packages.length > 1
          ? `The package manager will remove this application and ${packages.length - 1} related package${packages.length === 2 ? '' : 's'}.`
          : 'The package manager will remove this application.'
      );
    } else if (app.source === 'Flatpak') {
      summary.push(`Flatpak will uninstall it from the ${systemWide ? 'system' : 'user'} installation.`);
    } else if (app.source === 'Local') {
      summary.push('The desktop launcher is removed. The program itself stays on disk.');
    } else {
      summary.push('The application is removed from the device.');
    }
    if (packages.length > 0) {
      summary.push('', `Packages in this transaction (${packages.length}):`);
      summary.push(...packages.slice(0, 10).map((name) => `• ${name}`));
      if (packages.length > 10) summary.push(`…and ${packages.length - 10} more`);
    }
    summary.push('', 'This cannot be undone.');

    uiStore.confirm('Uninstall application', summary.join('\n'), () => performUninstall(app, expectedPreview), true);
  }

  async function performUninstall(app: DesktopApp, expectedPreview?: string) {
    if (app.source !== 'Local' && !app.package_id) {
      uiStore.addToast('Cannot uninstall: the package ID is unknown', 'error');
      return;
    }

    closeDetails();
    uninstallingApp = app;
    isUninstalling = true;
    uninstallSucceeded = null;
    uninstallLog = [];
    progressOpen = true;
    appendLog(`Removing ${app.name} (${app.package_id || app.file_path})…`);
    statusStore.setBusy(`Removing ${app.name}…`);

    try {
      await invoke('uninstall_app', {
        packageId: app.package_id ?? '',
        source: app.source,
        expectedPreview,
        systemWide: app.source === 'Flatpak' && app.file_path.includes('/var/lib/flatpak/'),
        filePath: app.file_path,
      });
      uninstallSucceeded = true;
      appendLog(`Finished removing ${app.name}.`);
      statusStore.setLastCommand(commandLabel(app), 0, true);
      uiStore.addToast(`Removed ${app.name}`, 'success');
      await loadApps();
    } catch (error) {
      uninstallSucceeded = false;
      appendLog(`Error: ${error}`);
      statusStore.setLastCommand(commandLabel(app), 1, false);
      uiStore.addToast(`Removal failed: ${error}`, 'error');
    } finally {
      isUninstalling = false;
      statusStore.clearBusy();
    }
  }

  function viewInHistory() {
    progressOpen = false;
    uiStore.navigateTo('dnf-history', 'history');
  }

  // ─── Duplicates ────────────────────────────────────────────────────────────

  function confirmRemoveFlatpak(app: FlatpakApp) {
    uiStore.confirm(
      'Remove Flatpak app',
      `Remove "${app.name}" (${app.app_id}) from the ${app.installation} installation?\n\nThis cannot be undone.`,
      async () => {
        statusStore.setBusy(`Removing ${app.name}…`);
        try {
          await invoke('remove_flatpak', {
            appId: app.app_id,
            systemWide: app.installation === 'system',
          });
          uiStore.addToast(`Removed ${app.name}`, 'success');
          statusStore.setLastCommand(`flatpak uninstall -y ${app.app_id}`, 0, true);
          await loadApps();
        } catch (error) {
          uiStore.addToast(`Failed to remove the Flatpak: ${error}`, 'error');
          statusStore.setLastCommand(`flatpak uninstall -y ${app.app_id}`, 1, false);
        } finally {
          statusStore.clearBusy();
        }
      },
      true
    );
  }

  async function confirmRemoveRpm(pkg: RpmPackage) {
    let expectedPreview: string;
    try {
      expectedPreview = await invoke<string>('preview_rpm_removal', { name: pkg.name });
    } catch (error) {
      uiStore.addToast(`Could not prepare a safe removal preview: ${error}`, 'error');
      return;
    }
    const packages = previewPackages(expectedPreview);
    uiStore.confirm(
      'Remove native package',
      [
        `Remove "${pkg.name}" (${pkg.version})?`,
        packages.length > 0 ? '' : null,
        packages.length > 0 ? `Packages in this transaction (${packages.length}):` : null,
        ...packages.slice(0, 10).map((name) => `• ${name}`),
        packages.length > 10 ? `…and ${packages.length - 10} more` : null,
        '',
        'This cannot be undone.',
      ]
        .filter((line): line is string => line !== null)
        .join('\n'),
      async () => {
        statusStore.setBusy(`Removing ${pkg.name}…`);
        try {
          await invoke('remove_rpm', { name: pkg.name, expectedPreview });
          uiStore.addToast(`Removed ${pkg.name}`, 'success');
          statusStore.setLastCommand(`dnf remove -y ${pkg.name}`, 0, true);
          await loadApps();
        } catch (error) {
          uiStore.addToast(`Failed to remove the package: ${error}`, 'error');
          statusStore.setLastCommand(`dnf remove -y ${pkg.name}`, 1, false);
        } finally {
          statusStore.clearBusy();
        }
      },
      true
    );
  }

  // ─── Context menu ──────────────────────────────────────────────────────────

  let contextMenu = $state<{ open: boolean; x: number; y: number; app: DesktopApp | null }>({
    open: false,
    x: 0,
    y: 0,
    app: null,
  });

  function openContextMenu(event: MouseEvent, app: DesktopApp) {
    event.preventDefault();
    event.stopPropagation();
    contextMenu = { open: true, x: event.clientX, y: event.clientY, app };
  }

  const contextMenuItems = $derived.by((): ContextMenuItem[] => {
    const app = contextMenu.app;
    if (!app) return [];
    const items: ContextMenuItem[] = [
      { label: 'Launch', icon: Play, action: () => launchApp(app) },
      { label: 'Inspect details', icon: Info, action: () => openDetails(app) },
      { label: 'Show in file manager', icon: FolderOpen, action: () => revealFile(app) },
      { label: 'Copy package ID', icon: Copy, action: () => copyPackageId(app) },
      { label: '', divider: true },
    ];
    if (app.source === 'AppImage') {
      const hasEntry = Boolean(app.package_id);
      items.push({
        label: hasEntry ? 'Remove from applications menu' : 'Add to applications menu',
        icon: hasEntry ? X : ImagePlus,
        action: () => setAppImageShortcut(app, !hasEntry),
      });
      items.push({ label: 'Delete AppImage…', icon: Trash2, danger: true, action: () => confirmDeleteAppImage(app) });
    } else if (isRemovable(app)) {
      items.push({ label: 'Uninstall application…', icon: Trash2, danger: true, action: () => confirmUninstall(app) });
    } else {
      items.push({
        label: 'System-wide launcher — no package owner',
        icon: AlertTriangle,
        disabled: true,
      });
    }
    return items;
  });

  // ─── Init ──────────────────────────────────────────────────────────────────

  onMount(() => {
    loadApps();
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    listen<string>('uninstall-log', (event) => appendLog(event.payload))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((error) => console.warn('Could not subscribe to removal output', error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<div class="module-page">
  <PageHeader title="App Manager" subtitle="Review and remove installed graphical applications" icon={LayoutGrid}>
    <span class="scan-summary">
      {#if loading}
        <RefreshCw size={13} class="animate-spin-slow" /> Scanning…
      {:else}
        {baseApps.length} apps{#if hiddenCount > 0} · {hiddenCount} hidden{/if}{#if lastScannedAt} · scanned {lastScannedAt.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}{/if}
      {/if}
    </span>
    <button class="header-refresh-btn" onclick={loadApps} disabled={loading || isUninstalling || deletingAppImage}>
      <RefreshCw size={13} class={loading ? 'animate-spin-slow' : ''} /> Refresh
    </button>
  </PageHeader>

  {#if duplicates.length > 0 && sourceFilter !== 'Duplicates'}
    <div class="duplicate-banner">
      <span class="duplicate-text">
        <AlertTriangle size={15} />
        {duplicates.length} application{duplicates.length === 1 ? '' : 's'} installed twice (native and Flatpak).
      </span>
      <Button variant="outline" size="sm" onclick={() => (sourceFilter = 'Duplicates')}>
        Review duplicates
      </Button>
    </div>
  {/if}

  <div class="controls-row">
    <div class="controls-left">
      <SearchBar
        bind:value={filter}
        placeholder="Search applications…"
        disabled={isUninstalling || deletingAppImage}
        style="flex: 1; max-width: 320px;"
      />
      <FilterChips
        options={sourceFilterOptions}
        bind:value={sourceFilter}
        label="Filter applications by source"
        disabled={isUninstalling || deletingAppImage}
      />
      <ToggleButton
        bind:active={showHidden}
        title={showHidden
          ? 'Hide entries that the application menu does not show'
          : 'Show entries marked NoDisplay or Hidden'}
        disabled={isUninstalling || deletingAppImage}
      >
        <Eye size={12} />
        Hidden
        <span class="hidden-count">{hiddenCount}</span>
      </ToggleButton>
    </div>

    <div class="controls-right">
      <span class="result-count">
        {sourceFilter === 'Duplicates' ? filteredDuplicates.length : filteredApps.length} shown
      </span>
      <label class="sort-control">
        <span>Sort</span>
        <Select bind:value={sortBy} size="sm" ariaLabel="Sort applications">
          <option value="name">Name</option>
          <option value="source">Source</option>
          <option value="size">Size</option>
          <option value="date">Install date</option>
        </Select>
      </label>
      <button
        type="button"
        class="sort-direction"
        onclick={() => (sortDir = sortDir === 'asc' ? 'desc' : 'asc')}
        title={sortDir === 'asc' ? 'Sort ascending' : 'Sort descending'}
        aria-label={sortDir === 'asc' ? 'Sort ascending' : 'Sort descending'}
      >
        {sortDir === 'asc' ? '↑' : '↓'}
      </button>
    </div>
  </div>

  <div class="content">
    {#if sourceFilter === 'Duplicates'}
      {#if duplicates.length === 0}
        <EmptyState
          icon={CheckCircle}
          title="No duplicate applications"
          description="No application is installed as both a native package and a Flatpak."
        />
      {:else if filteredDuplicates.length === 0}
        <EmptyState
          icon={Search}
          title="No duplicates match"
          description={`Nothing matches “${filter}”.`}
          actionLabel="Clear search"
          onAction={() => (filter = '')}
        />
      {:else}
        <div class="duplicate-list">
          {#each filteredDuplicates as duplicate (duplicate.common_name + (duplicate.flatpak?.app_id ?? '') + (duplicate.rpm?.name ?? ''))}
            <Card title={duplicate.common_name}>
              <p class="duplicate-recommendation">{duplicate.recommendation}</p>
              <div class="duplicate-options">
                {#if duplicate.flatpak}
                  <div class="duplicate-option">
                    <div class="duplicate-option-info">
                      <Badge variant="muted">Flatpak</Badge>
                      <span class="mono">{duplicate.flatpak.app_id}</span>
                      <span class="muted-small">v{duplicate.flatpak.version} · {duplicate.flatpak.installation}</span>
                    </div>
                    <Button variant="danger" size="sm" onclick={() => confirmRemoveFlatpak(duplicate.flatpak!)}>
                      <Trash2 size={12} /> Remove
                    </Button>
                  </div>
                {/if}
                {#if duplicate.rpm}
                  <div class="duplicate-option">
                    <div class="duplicate-option-info">
                      <Badge variant="info">Native</Badge>
                      <span class="mono">{duplicate.rpm.name}</span>
                      <span class="muted-small">v{duplicate.rpm.version} · {duplicate.rpm.arch}</span>
                    </div>
                    <Button variant="danger" size="sm" onclick={() => confirmRemoveRpm(duplicate.rpm!)}>
                      <Trash2 size={12} /> Remove
                    </Button>
                  </div>
                {/if}
              </div>
            </Card>
          {/each}
        </div>
      {/if}
    {:else if loading && apps.length === 0}
      <div class="loading-state">
        <RefreshCw size={24} class="animate-spin-slow" />
        <span>Scanning installed applications…</span>
      </div>
    {:else if apps.length === 0}
      <EmptyState
        icon={AppWindow}
        title="No applications found"
        description="The panel could not find any desktop entries in the usual application directories."
        actionLabel="Scan again"
        actionIcon={RefreshCw}
        onAction={loadApps}
      />
    {:else if filteredApps.length === 0}
      <EmptyState
        icon={Search}
        title="No applications match"
        description={`Nothing matches “${filter}” in this source filter.${
          hiddenCount > 0 && !showHidden
            ? ` ${hiddenCount} hidden entr${hiddenCount === 1 ? 'y is' : 'ies are'} not shown.`
            : ''
        }`}
        actionLabel="Clear search and filters"
        onAction={() => {
          filter = '';
          sourceFilter = 'All';
        }}
      />
    {:else}
      <Table
        tableAction={tableFeatures}
        actionParams={{ sortable: false }}
        style="border:none; border-radius:0;"
        aria-label="Installed applications"
      >
        <thead>
          <tr>
            <th aria-sort={sortBy === 'name' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
              <button type="button" class="th-sort-btn" onclick={() => toggleSort('name')}>
                Application
                {#if sortBy === 'name'}<span class="sort-mark">{sortDir === 'asc' ? '↑' : '↓'}</span>{/if}
              </button>
            </th>
            <th aria-sort={sortBy === 'source' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
              <button type="button" class="th-sort-btn" onclick={() => toggleSort('source')}>
                Source
                {#if sortBy === 'source'}<span class="sort-mark">{sortDir === 'asc' ? '↑' : '↓'}</span>{/if}
              </button>
            </th>
            <th>Version</th>
            <th aria-sort={sortBy === 'size' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}>
              <button type="button" class="th-sort-btn" onclick={() => toggleSort('size')}>
                Size
                {#if sortBy === 'size'}<span class="sort-mark">{sortDir === 'asc' ? '↑' : '↓'}</span>{/if}
              </button>
            </th>
            <th
              aria-sort={sortBy === 'date' ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
              title="Install time; for Flatpak the time of the last update"
            >
              <button type="button" class="th-sort-btn" onclick={() => toggleSort('date')}>
                Installed
                {#if sortBy === 'date'}<span class="sort-mark">{sortDir === 'asc' ? '↑' : '↓'}</span>{/if}
              </button>
            </th>
            <th class="col-actions"><span class="visually-hidden">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          {#each filteredApps as app (app.file_path)}
            {@const AppIcon = appIcon(app)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <tr
              tabindex="0"
              class="app-row"
              onclick={() => openDetails(app)}
              oncontextmenu={(event) => openContextMenu(event, app)}
              onkeydown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  openDetails(app);
                }
              }}
            >
              <td class="col-name">
                <span class="row-icon" class:has-image={app.icon}>
                  {#if app.icon}
                    <!-- The visible row title names the app, so the image is decorative. -->
                    <img class="row-icon-image" src={app.icon} alt="" aria-hidden="true" />
                  {:else}
                    <AppIcon size={16} />
                  {/if}
                </span>
                <span class="row-text">
                  <span class="row-title">
                    {app.name}
                    {#if app.hidden}
                      <span class="row-flag" title="Not shown in the application menu (NoDisplay/Hidden)">hidden</span>
                    {:else if !isRemovable(app)}
                      <span class="row-flag" title="System-wide launcher without a package owner">system</span>
                    {/if}
                  </span>
                  <span
                    class="row-subtitle"
                    title={app.source === 'AppImage' ? app.file_path : app.package_id || app.file_path}
                  >
                    {app.source === 'AppImage' ? app.file_path : app.package_id || app.exec}
                  </span>
                </span>
              </td>
              <td><Badge variant={sourceVariant(app.source)}>{sourceLabel(app.source)}</Badge></td>
              <td class="mono-col">{app.version || '—'}</td>
              <td class="mono-col">{formatBytes(app.sizeBytes)}</td>
              <td class="col-date">{formatDate(app.installDate)}</td>
              <!-- The row opens the details panel; the menu must not trigger it. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <td
                class="col-actions"
                onclick={(event) => event.stopPropagation()}
                onkeydown={(event) => event.stopPropagation()}
              >
                <KebabMenu title={`Actions for ${app.name}`}>
                  <button type="button" class="menu-item" onclick={() => launchApp(app)}>
                    <Play size={14} /> Launch
                  </button>
                  <button type="button" class="menu-item" onclick={() => openDetails(app)}>
                    <Info size={14} /> Inspect details
                  </button>
                  <button type="button" class="menu-item" onclick={() => revealFile(app)}>
                    <FolderOpen size={14} /> Show in file manager
                  </button>
                  <button type="button" class="menu-item" onclick={() => copyPackageId(app)}>
                    <Copy size={14} />
                    {app.source === 'AppImage' ? 'Copy file path' : 'Copy package ID'}
                  </button>
                  <div class="menu-divider"></div>
                  {#if app.source === 'AppImage'}
                    <button
                      type="button"
                      class="menu-item"
                      onclick={() => setAppImageShortcut(app, !app.package_id)}
                    >
                      {#if app.package_id}<X size={14} /> Remove from menu{:else}<ImagePlus size={14} /> Add to applications menu{/if}
                    </button>
                    <button
                      type="button"
                      class="menu-item text-danger"
                      onclick={() => confirmDeleteAppImage(app)}
                    >
                      <Trash2 size={14} /> Delete AppImage…
                    </button>
                  {:else if isRemovable(app)}
                    <button
                      type="button"
                      class="menu-item text-danger"
                      onclick={() => confirmUninstall(app)}
                    >
                      <Trash2 size={14} /> Uninstall application…
                    </button>
                  {:else}
                    <button type="button" class="menu-item" disabled>
                      <AlertTriangle size={14} /> System-wide launcher
                    </button>
                  {/if}
                </KebabMenu>
              </td>
            </tr>
          {/each}
        </tbody>
      </Table>
    {/if}
  </div>
</div>

<!-- Side panel -->
{#if selectedAppForDetails}
  {@const PanelIcon = appIcon(selectedAppForDetails)}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="panel-backdrop" onclick={closeDetails}></div>
  <aside class="side-panel" aria-label={`${selectedAppForDetails.name} details`}>
    <header class="panel-header">
      <div class="panel-heading">
        <span class="panel-app-icon" class:has-image={selectedAppForDetails.icon}>
          {#if selectedAppForDetails.icon}
            <img src={selectedAppForDetails.icon} alt="" aria-hidden="true" />
          {:else}
            <PanelIcon size={16} />
          {/if}
        </span>
        <span class="panel-title" title={selectedAppForDetails.name}>{selectedAppForDetails.name}</span>
        <Badge variant={sourceVariant(selectedAppForDetails.source)}>{sourceLabel(selectedAppForDetails.source)}</Badge>
      </div>
      <button type="button" class="panel-close-btn" onclick={closeDetails} aria-label="Close details">
        <X size={18} />
      </button>
    </header>

    <div class="panel-tabs">
      <button
        type="button"
        class="panel-tab-btn"
        class:active={activeDetailTab === 'details'}
        onclick={() => (activeDetailTab = 'details')}
      >
        Details
      </button>
      {#if selectedAppForDetails.source === 'Flatpak'}
        <button
          type="button"
          class="panel-tab-btn"
          class:active={activeDetailTab === 'permissions'}
          onclick={() => (activeDetailTab = 'permissions')}
        >
          Permissions
        </button>
      {/if}
      {#if selectedAppForDetails.source === 'RPM' || selectedAppForDetails.source === 'Flatpak'}
        <button
          type="button"
          class="panel-tab-btn"
          class:active={activeDetailTab === 'dependencies'}
          onclick={() => (activeDetailTab = 'dependencies')}
        >
          Dependencies
        </button>
      {/if}
    </div>

    <div class="panel-content">
      {#if selectedAppForDetails.hidden}
        <p class="panel-note">
          This entry is marked <code>NoDisplay</code>/<code>Hidden</code>: it does not appear in the
          application menu, but the package is installed.
        </p>
      {/if}
      {#if activeDetailTab === 'details'}
        <div class="detail-grid">
          <div class="detail-item">
            <span class="detail-label">{selectedAppForDetails.source === 'AppImage' ? 'File' : 'Package ID'}</span>
            <span class="detail-value mono break">
              {selectedAppForDetails.source === 'AppImage'
                ? selectedAppForDetails.file_path
                : selectedAppForDetails.package_id || selectedAppForDetails.file_path}
            </span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Version</span>
            <span class="detail-value">{appDetails?.version || selectedAppForDetails.version || '—'}</span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Disk size</span>
            <span class="detail-value">{formatBytes(selectedAppForDetails.sizeBytes)}</span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Installed</span>
            <span class="detail-value">{formatDate(selectedAppForDetails.installDate)}</span>
          </div>
        </div>

        {#if selectedAppForDetails.source === 'AppImage'}
          <section class="panel-section appimage-card">
            <h3 class="section-title">AppImage</h3>
            <p class="section-hint">
              AppImages are single files. Adding one to the applications menu makes it searchable in
              the GNOME app grid; deleting it removes the file from disk. The icon comes from an
              existing launcher, a cached copy, or the AppImage itself — reading it from the file runs
              it once. If none is found, the generic icon is used and you can pick one here.
            </p>
            <div class="switch-row">
              <div>
                <span class="switch-label">Applications menu entry</span>
                <span class="switch-hint">{appimageShortcutEnabled ? 'Visible in the app grid' : 'Not in the app grid'}</span>
              </div>
              <Toggle
                checked={appimageShortcutEnabled}
                disabled={registeringShortcut || readingIcon || deletingAppImage}
                onchange={(checked: boolean) =>
                  setAppImageShortcut(selectedAppForDetails!, checked)}
              />
            </div>
            <div class="button-grid">
              <Button variant="outline" size="sm" onclick={() => launchApp(selectedAppForDetails!)}>
                <Play size={13} /> Launch
              </Button>
              <Button variant="outline" size="sm" onclick={() => revealFile(selectedAppForDetails!)}>
                <FolderOpen size={13} /> Show file
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={registeringShortcut || readingIcon}
                onclick={() => chooseAppImageIcon(selectedAppForDetails!)}
              >
                <ImagePlus size={13} /> Choose icon…
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={readingIcon || registeringShortcut}
                title="Run the AppImage once to read its icon"
                onclick={() => readAppImageIcon(selectedAppForDetails!)}
              >
                <RefreshCw size={13} /> {selectedAppForDetails.icon ? 'Refresh icon' : 'Read icon'}
              </Button>
              <Button
                variant="danger"
                size="sm"
                disabled={deletingAppImage || readingIcon || isUninstalling}
                onclick={() => confirmDeleteAppImage(selectedAppForDetails!)}
              >
                <Trash2 size={13} /> Delete AppImage…
              </Button>
            </div>
          </section>
        {/if}

        {#if loadingDetails}
          <div class="loading-inline"><RefreshCw size={20} class="animate-spin-slow" /></div>
        {:else if appDetails && selectedAppForDetails.source !== 'AppImage'}
          <section class="panel-section">
            <h3 class="section-title">Description</h3>
            <p class="detail-description">{appDetails.description || 'No description available.'}</p>
          </section>

          {#if appDetails.files?.length}
            <section class="panel-section">
              <h3 class="section-title">Owned files <span class="count">{appDetails.files.length}</span></h3>
              <div class="code-list">
                {#each (showMoreFiles ? appDetails.files : appDetails.files.slice(0, 25)) as file (file)}
                  <div class="code-line">{file}</div>
                {/each}
              </div>
              {#if appDetails.files.length > 25}
                <Button variant="ghost" size="sm" style="width:100%; justify-content:center;" onclick={() => (showMoreFiles = !showMoreFiles)}>
                  {showMoreFiles ? 'Show less' : `Show all ${appDetails.files.length} files`}
                </Button>
              {/if}
            </section>
          {/if}
        {/if}
      {:else if activeDetailTab === 'permissions'}
        {#if loadingPermissions}
          <div class="loading-inline"><RefreshCw size={20} class="animate-spin-slow" /></div>
        {:else if flatpakPermissions}
          <p class="section-hint">
            Sandbox overrides are saved for your user only. Changing one here changes what the
            application can reach.
          </p>
          <div class="permission-list">
            {#each PERMISSIONS as permission (permission.key)}
              <div class="permission-row">
                <div>
                  <span class="switch-label">{permission.label}</span>
                  <span class="switch-hint">{permission.hint}</span>
                </div>
                <Toggle
                  checked={flatpakPermissions[permission.key]}
                  onchange={(checked: boolean) =>
                    toggleFlatpakPermission(permission.key, checked)}
                />
              </div>
            {/each}
          </div>
        {:else}
          <p class="section-hint">No permission data is available for this application.</p>
        {/if}
      {:else if activeDetailTab === 'dependencies'}
        {#if loadingDependencies}
          <div class="loading-inline"><RefreshCw size={20} class="animate-spin-slow" /></div>
        {:else if dependencies.length === 0}
          <p class="section-hint">No dependencies were reported for this application.</p>
        {:else}
          {#if dependencyGroups.packages.length > 0}
            <section class="panel-section">
              <h3 class="section-title">Packages <span class="count">{dependencyGroups.packages.length}</span></h3>
              <div class="code-list">
                {#each dependencyGroups.packages as dependency (dependency)}
                  <div class="code-line">{dependency}</div>
                {/each}
              </div>
            </section>
          {/if}
          {#if dependencyGroups.system.length > 0}
            <section class="panel-section">
              <h3 class="section-title">System libraries <span class="count">{dependencyGroups.system.length}</span></h3>
              <div class="code-list">
                {#each dependencyGroups.system as dependency (dependency)}
                  <div class="code-line">{dependency}</div>
                {/each}
              </div>
            </section>
          {/if}
        {/if}
      {/if}
    </div>

    <footer class="panel-footer">
      {#if selectedAppForDetails.source === 'AppImage'}
        <Button
          variant="danger"
          style="width:100%; justify-content:center;"
          disabled={deletingAppImage || isUninstalling}
          onclick={() => confirmDeleteAppImage(selectedAppForDetails!)}
        >
          <Trash2 size={15} /> Delete AppImage
        </Button>
      {:else if !isRemovable(selectedAppForDetails)}
        <p class="footer-hint">
          This launcher is installed system-wide and no package owns it. Removing it needs an
          administrator.
        </p>
      {:else}
        <Button
          variant="danger"
          style="width:100%; justify-content:center;"
          disabled={isUninstalling || !selectedAppForDetails.package_id}
          onclick={() => confirmUninstall(selectedAppForDetails!)}
        >
          <Trash2 size={15} /> Uninstall application
        </Button>
      {/if}
    </footer>
  </aside>
{/if}

<!-- Removal progress drawer: the list stays visible while output streams in. -->
{#if progressOpen && uninstallingApp}
  <section class="progress-drawer" aria-label="Removal progress">
    <header class="drawer-header">
      <span class="drawer-title">
        {#if isUninstalling}
          <RefreshCw size={14} class="animate-spin-slow" />
          Removing {uninstallingApp.name}…
        {:else if uninstallSucceeded}
          <CheckCircle size={14} style="color:var(--color-success)" />
          Removed {uninstallingApp.name}
        {:else}
          <AlertTriangle size={14} style="color:var(--color-error)" />
          Removal failed
        {/if}
      </span>
      <button
        type="button"
        class="panel-close-btn"
        onclick={() => {
          progressOpen = false;
          uninstallLog = [];
          uninstallingApp = null;
        }}
        aria-label="Close progress"
      >
        <X size={16} />
      </button>
    </header>
    <div class="terminal-log" bind:this={logContainer}>
      {#each uninstallLog as line, index (index)}
        <div class="log-line">{line}</div>
      {/each}
    </div>
    <footer class="drawer-footer">
      {#if !isUninstalling && uninstallSucceeded && uninstallingApp.source === 'RPM'}
        <Button variant="outline" size="sm" onclick={viewInHistory}>
          <History size={13} /> View in History
        </Button>
      {/if}
      {#if !isUninstalling}
        <Button
          variant="primary"
          size="sm"
          onclick={() => {
            progressOpen = false;
            uninstallLog = [];
            uninstallingApp = null;
          }}
        >
          Done
        </Button>
      {/if}
    </footer>
  </section>
{/if}

<ContextMenu
  bind:isOpen={contextMenu.open}
  x={contextMenu.x}
  y={contextMenu.y}
  title={contextMenu.app?.name ?? ''}
  badge={contextMenu.app ? { text: sourceLabel(contextMenu.app.source), variant: contextBadgeVariant(contextMenu.app.source) } : undefined}
  items={contextMenuItems}
/>

<style>
  .module-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .scan-summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .duplicate-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin: 0;
    padding: 8px 14px;
    border: 1px solid rgba(245, 158, 11, 0.25);
    background: rgba(245, 158, 11, 0.05);
    border-radius: 8px;
    font-size: 12px;
  }

  .duplicate-text {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-primary);
  }

  .duplicate-text :global(svg) {
    color: var(--color-warning);
  }

  .controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .controls-left {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    flex: 1;
    min-width: 280px;
  }

  .controls-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .result-count {
    font-size: 11.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .sort-control {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .sort-direction {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-input);
    color: var(--color-text-secondary);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }

  .sort-direction:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 240px;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  .loading-inline {
    display: flex;
    justify-content: center;
    padding: 32px 0;
    color: var(--color-text-muted);
  }

  .th-sort-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: inherit;
    cursor: pointer;
  }

  .th-sort-btn:hover {
    color: var(--color-text-primary);
  }

  .sort-mark {
    color: var(--color-accent);
    font-size: 11px;
  }

  .col-name {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 220px;
  }

  .row-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    flex-shrink: 0;
    border-radius: 8px;
    background: var(--color-module-icon-bg, var(--color-bg-raised));
    border: 1px solid var(--color-module-icon-border, var(--color-border));
    color: var(--color-accent);
  }

  /* Real artwork is often transparent; a box behind it would only add noise. */
  .row-icon.has-image,
  .panel-app-icon.has-image {
    background: transparent;
    border-color: transparent;
  }

  .row-icon-image {
    width: 26px;
    height: 26px;
    object-fit: contain;
  }

  .panel-app-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border-radius: 6px;
    background: var(--color-module-icon-bg, var(--color-bg-raised));
    border: 1px solid var(--color-module-icon-border, var(--color-border));
    color: var(--color-accent);
  }

  .panel-app-icon img {
    width: 22px;
    height: 22px;
    object-fit: contain;
  }

  .row-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }

  .row-title {
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hidden-count {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    border-radius: 999px;
    padding: 1px 6px;
    line-height: 1.3;
  }

  .panel-note {
    margin: 0 0 16px;
    padding: 9px 12px;
    border: 1px solid rgba(245, 158, 11, 0.25);
    border-radius: 8px;
    background: rgba(245, 158, 11, 0.06);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--color-text-secondary);
  }

  .panel-note code {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-primary);
  }

  .row-flag {
    margin-left: 6px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    vertical-align: middle;
  }

  .footer-hint {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--color-text-muted);
    text-align: center;
  }

  .row-subtitle {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
  }

  :global(.app-row) {
    cursor: pointer;
  }

  :global(.app-row:focus-visible) {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }

  .col-date {
    white-space: nowrap;
    color: var(--color-text-secondary);
  }

  .col-actions {
    width: 48px;
    text-align: right;
  }

  /* The kebab menu styles its own items; the separator is ours. */
  .menu-divider {
    height: 1px;
    margin: 4px 2px;
    background: var(--color-border);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .mono-col {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    white-space: nowrap;
  }

  .duplicate-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 8px;
  }

  .duplicate-recommendation {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .duplicate-options {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .duplicate-option {
    flex: 1;
    min-width: 260px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-raised);
  }

  .duplicate-option-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .muted-small {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
    word-break: break-all;
  }

  /* ── Side panel ─────────────────────────────────────────────────────── */

  .panel-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 9998;
    animation: fade-in 0.18s ease-out both;
  }

  .side-panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: 460px;
    max-width: 92vw;
    background: var(--color-bg-card);
    border-left: 1px solid var(--color-border);
    z-index: 9999;
    display: flex;
    flex-direction: column;
    box-shadow: -12px 0 36px rgba(0, 0, 0, 0.4);
    animation: slide-in 0.22s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-raised);
  }

  .panel-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .panel-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .panel-close-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .panel-close-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.06));
  }

  .panel-tabs {
    display: flex;
    padding: 0 14px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-card);
  }

  .panel-tab-btn {
    padding: 11px 12px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .panel-tab-btn:hover {
    color: var(--color-text-primary);
  }

  .panel-tab-btn.active {
    color: var(--color-accent);
    border-bottom-color: var(--color-accent);
  }

  .panel-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px;
  }

  .panel-footer {
    padding: 12px 18px;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-raised);
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-bottom: 18px;
  }

  .detail-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .detail-item:first-child {
    grid-column: 1 / -1;
  }

  .detail-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .detail-value {
    font-size: 12.5px;
    color: var(--color-text-primary);
  }

  .break {
    word-break: break-all;
  }

  .panel-section {
    margin-bottom: 18px;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .count {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--color-bg-raised);
    color: var(--color-text-muted);
  }

  .section-hint {
    margin: 0 0 12px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--color-text-muted);
  }

  .detail-description {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .code-list {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-surface);
    max-height: 240px;
    overflow-y: auto;
    padding: 6px 0;
  }

  .code-line {
    padding: 3px 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  .code-line:hover {
    background: rgba(255, 255, 255, 0.03);
    color: var(--color-text-primary);
  }

  .appimage-card {
    padding: 14px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-raised);
  }

  .switch-row,
  .permission-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
  }

  .switch-row {
    border-top: 1px solid var(--color-border);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 12px;
  }

  .switch-label {
    display: block;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .switch-hint {
    display: block;
    margin-top: 2px;
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .permission-list :global(.permission-row + .permission-row) {
    border-top: 1px solid var(--color-border);
  }

  .button-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  /* ── Progress drawer ────────────────────────────────────────────────── */

  .progress-drawer {
    position: fixed;
    right: 20px;
    bottom: 20px;
    width: min(560px, calc(100vw - 32px));
    max-height: 48vh;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
    z-index: 10000;
    overflow: hidden;
    animation: drawer-in 0.2s cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-raised);
  }

  .drawer-title {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .terminal-log {
    flex: 1;
    min-height: 120px;
    overflow-y: auto;
    padding: 12px 14px;
    background: #0f111a;
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.5;
    color: #a6accd;
  }

  .log-line {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .drawer-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-raised);
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes slide-in {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }

  @keyframes drawer-in {
    from { opacity: 0; transform: translateY(12px); }
    to { opacity: 1; transform: translateY(0); }
  }

  :global(html.light-mode) .side-panel,
  :global(html.light-mode) .progress-drawer {
    background: #ffffff;
  }

  :global(html.light-mode) .panel-header,
  :global(html.light-mode) .panel-footer,
  :global(html.light-mode) .drawer-header,
  :global(html.light-mode) .drawer-footer {
    background: #f8fafc;
  }

  :global(html.light-mode) .code-list {
    background: #f1f5f9;
  }

  :global(html.light-mode) .code-line {
    color: #334155;
  }
</style>
