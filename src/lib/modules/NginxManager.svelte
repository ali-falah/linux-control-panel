<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import {
    Server, Activity, Globe, FileCode, FolderOpen, FileText, Shield,
    Play, Square, RotateCcw, RefreshCw, CheckCircle, XCircle, AlertTriangle,
    Plus, Trash2, Eye, EyeOff, Upload, FolderPlus, Edit3, Download,
    ChevronRight, ChevronDown, Lock, Clock, ArchiveRestore, Save,
    TerminalSquare, Filter, Search
  } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';

  // ─── Types ────────────────────────────────────────────────────────────────

  interface NginxInstallInfo { installed: boolean; version: string; }
  interface NginxServiceStatus { active: boolean; status: string; since: string; sub_state: string; }
  interface NginxTestResult { passed: boolean; output: string; timestamp: string; }
  interface NginxSite { name: string; path: string; enabled: boolean; source: string; }
  interface NginxStats { sites_available: number; sites_enabled: number; sites_disabled: number; }
  interface NginxConfigFile { name: string; path: string; source: string; }
  interface NginxBackup { original_path: string; backup_path: string; timestamp: string; filename: string; }
  interface WwwEntry { name: string; path: string; is_dir: boolean; size: number; modified: string; children: WwwEntry[]; }
  interface SslCert { domain: string; cert_path: string; expiry: string; days_until_expiry: number; status: string; }
  interface NewSiteConfig {
    server_name: string; root_dir: string; port: number; is_proxy: boolean;
    proxy_url: string; index_file: string; enable_404: boolean; enable_50x: boolean;
  }

  // ─── State ────────────────────────────────────────────────────────────────

  let activeTab = $state<'overview'|'sites'|'editor'|'www'|'logs'|'ssl'>('overview');
  let installInfo = $state<NginxInstallInfo | null>(null);
  let loading = $state(true);
  let hasCertbot = $state(false);

  // Overview
  let serviceStatus = $state<NginxServiceStatus | null>(null);
  let testResult = $state<NginxTestResult | null>(null);
  let stats = $state<NginxStats | null>(null);
  let serviceLoading = $state(false);
  let testLoading = $state(false);

  // Sites
  let sites = $state<NginxSite[]>([]);
  let sitesLoading = $state(false);
  let showNewSiteForm = $state(false);
  let newSite = $state<NewSiteConfig>({
    server_name: '', root_dir: '/var/www/html', port: 80,
    is_proxy: false, proxy_url: '', index_file: 'index.html',
    enable_404: true, enable_50x: true,
  });
  let newSiteLoading = $state(false);
  let toggleLoadingFor = $state<string>('');

  // Config Editor
  let configs = $state<NginxConfigFile[]>([]);
  let selectedConfig = $state<NginxConfigFile | null>(null);
  let editorContent = $state('');
  let savedContent = $state('');
  let editorLoading = $state(false);
  let configSaving = $state(false);
  let showDiff = $state(false);
  let wordWrap = $state(true);
  let backups = $state<NginxBackup[]>([]);
  let showBackups = $state(false);
  let backupsLoading = $state(false);

  // WWW browser
  let wwwEntries = $state<WwwEntry[]>([]);
  let wwwLoading = $state(false);
  let expandedPaths = $state<Set<string>>(new Set());
  let selectedWwwEntry = $state<WwwEntry | null>(null);
  let wwwFileContent = $state('');
  let wwwFileLoading = $state(false);
  let renamingEntry = $state<WwwEntry | null>(null);
  let renameValue = $state('');
  let newDirParent = $state('');
  let newDirName = $state('');
  let showNewDirForm = $state(false);

  // Logs
  let logFiles = $state<string[]>([]);
  let selectedLog = $state('');
  let logContent = $state('');
  let logLoading = $state(false);
  let logFilter = $state('');
  let logAutoRefresh = $state(false);
  let logInterval: ReturnType<typeof setInterval> | null = null;

  // SSL
  let sslCerts = $state<SslCert[]>([]);
  let sslLoading = $state(false);
  let renewingCert = $state('');

  // Modals
  let showTestModal = $state(false);
  let modalTestResult = $state<NginxTestResult | null>(null);
  let showOutputModal = $state(false);
  let outputModalTitle = $state('');
  let outputModalContent = $state('');

  // ─── Init ──────────────────────────────────────────────────────────────────

  $effect(() => {
    init();
    return () => {
      if (logInterval) clearInterval(logInterval);
    };
  });

  async function init() {
    loading = true;
    try {
      installInfo = await invoke<NginxInstallInfo>('nginx_check_installed');
      if (installInfo.installed) {
        hasCertbot = await invoke<boolean>('nginx_check_certbot');
        await Promise.all([
          loadServiceStatus(),
          loadTestResult(),
          loadStats(),
        ]);
      }
    } catch (e) {
      uiStore.addToast(`Init error: ${e}`, 'error');
    } finally {
      loading = false;
    }
  }

  // ─── Overview ──────────────────────────────────────────────────────────────

  async function loadServiceStatus() {
    try {
      serviceStatus = await invoke<NginxServiceStatus>('nginx_service_status');
    } catch {}
  }

  async function loadTestResult() {
    try {
      testResult = await invoke<NginxTestResult>('nginx_test_config');
    } catch {}
  }

  async function loadStats() {
    try {
      stats = await invoke<NginxStats>('nginx_get_stats');
    } catch {}
  }

  async function doServiceAction(action: string) {
    serviceLoading = true;
    statusStore.setBusy(`Running: systemctl ${action} nginx…`);
    const toastId = uiStore.addToast(`nginx -t passed → running ${action}…`, 'info', 0);
    try {
      await invoke<string>('nginx_service_action', { action });
      uiStore.removeToast(toastId);
      uiStore.addToast(`nginx ${action} succeeded ✓`, 'success');
      statusStore.setLastCommand(`systemctl ${action} nginx`, 0, true);
      await loadServiceStatus();
    } catch (e) {
      uiStore.removeToast(toastId);
      uiStore.addToast(`nginx ${action} failed: ${e}`, 'error');
      statusStore.setLastCommand(`systemctl ${action} nginx`, 1, false);
    } finally {
      serviceLoading = false;
      statusStore.clearBusy();
    }
  }

  async function runTest() {
    testLoading = true;
    statusStore.setBusy('Running nginx -t…');
    try {
      testResult = await invoke<NginxTestResult>('nginx_test_config');
      statusStore.setLastCommand('nginx -t', testResult.passed ? 0 : 1, testResult.passed);
      showTestModal = true;
      modalTestResult = testResult;
    } catch (e) {
      uiStore.addToast(`Test failed: ${e}`, 'error');
    } finally {
      testLoading = false;
      statusStore.clearBusy();
    }
  }

  // ─── Sites ─────────────────────────────────────────────────────────────────

  async function loadSites() {
    sitesLoading = true;
    try {
      sites = await invoke<NginxSite[]>('nginx_list_sites');
    } catch (e) {
      uiStore.addToast(`Failed to load sites: ${e}`, 'error');
    } finally {
      sitesLoading = false;
    }
  }

  async function toggleSite(site: NginxSite) {
    const action = site.enabled ? 'disable' : 'enable';
    uiStore.confirm(
      `${action === 'enable' ? 'Enable' : 'Disable'} site`,
      `${action === 'enable' ? 'Enable' : 'Disable'} "${site.name}"? nginx -t will be run and nginx will reload if valid.`,
      async () => {
        toggleLoadingFor = site.name;
        const toastId = uiStore.addToast(`nginx -t checking…`, 'info', 0);
        try {
          const result = await invoke<NginxTestResult>('nginx_toggle_site', {
            name: site.name,
            enable: !site.enabled,
          });
          uiStore.removeToast(toastId);
          statusStore.setLastCommand('nginx -t', result.passed ? 0 : 1, result.passed);

          if (result.passed) {
            uiStore.addToast(`Site "${site.name}" ${action}d and nginx reloaded ✓`, 'success');
          } else {
            uiStore.addToast(`nginx -t failed — change reverted`, 'error');
            showOutputModal = true;
            outputModalTitle = 'nginx -t Failed — Change Reverted';
            outputModalContent = result.output;
          }
          await loadSites();
          await loadStats();
        } catch (e) {
          uiStore.removeToast(toastId);
          uiStore.addToast(`Toggle failed: ${e}`, 'error');
        } finally {
          toggleLoadingFor = '';
        }
      },
      action === 'disable',
    );
  }

  async function createSite() {
    if (!newSite.server_name.trim()) {
      uiStore.addToast('Server name is required', 'warning');
      return;
    }
    newSiteLoading = true;
    try {
      const path = await invoke<string>('nginx_create_site', { config: newSite });
      uiStore.addToast(`Site created at ${path} ✓`, 'success');
      showNewSiteForm = false;
      newSite = { server_name: '', root_dir: '/var/www/html', port: 80, is_proxy: false, proxy_url: '', index_file: 'index.html', enable_404: true, enable_50x: true };
      await loadSites();
      await loadStats();
    } catch (e) {
      uiStore.addToast(`Create site failed: ${e}`, 'error');
      showOutputModal = true;
      outputModalTitle = 'Site Creation Failed';
      outputModalContent = String(e);
    } finally {
      newSiteLoading = false;
    }
  }

  function confirmDeleteSite(site: NginxSite) {
    uiStore.confirm(
      'Delete Site',
      `Delete "${site.name}"? This removes the config file and its symlink. This cannot be undone.`,
      () => {
        uiStore.confirm(
          '⚠️ Confirm Delete',
          `Are you absolutely sure you want to permanently delete "${site.name}"?`,
          async () => {
            try {
              await invoke('nginx_delete_site', { name: site.name, path: site.path });
              uiStore.addToast(`Site "${site.name}" deleted`, 'success');
              await loadSites();
              await loadStats();
            } catch (e) {
              uiStore.addToast(`Delete failed: ${e}`, 'error');
            }
          },
          true,
        );
      },
      true,
    );
  }

  // ─── Config Editor ─────────────────────────────────────────────────────────

  async function loadConfigs() {
    editorLoading = true;
    try {
      configs = await invoke<NginxConfigFile[]>('nginx_list_configs');
    } catch (e) {
      uiStore.addToast(`Failed to load configs: ${e}`, 'error');
    } finally {
      editorLoading = false;
    }
  }

  async function selectConfig(cfg: NginxConfigFile) {
    editorLoading = true;
    selectedConfig = cfg;
    showDiff = false;
    try {
      const content = await invoke<string>('nginx_read_config', { path: cfg.path });
      editorContent = content;
      savedContent = content;
    } catch (e) {
      uiStore.addToast(`Failed to read config: ${e}`, 'error');
    } finally {
      editorLoading = false;
    }
  }

  async function saveConfig() {
    if (!selectedConfig) return;
    configSaving = true;
    const toastId = uiStore.addToast('Running nginx -t before saving…', 'info', 0);
    try {
      const result = await invoke<NginxTestResult>('nginx_write_config', {
        path: selectedConfig.path,
        content: editorContent,
      });
      uiStore.removeToast(toastId);
      statusStore.setLastCommand('nginx -t', result.passed ? 0 : 1, result.passed);

      if (result.passed) {
        uiStore.addToast('Config saved and nginx reloaded ✓', 'success');
        savedContent = editorContent;
        showDiff = false;
      } else {
        uiStore.addToast('nginx -t failed — file reverted from backup', 'error');
        showOutputModal = true;
        outputModalTitle = 'nginx -t Failed — File Reverted';
        outputModalContent = result.output;
        // Reload the reverted content
        const content = await invoke<string>('nginx_read_config', { path: selectedConfig.path });
        editorContent = content;
        savedContent = content;
      }
    } catch (e) {
      uiStore.removeToast(toastId);
      uiStore.addToast(`Save failed: ${e}`, 'error');
    } finally {
      configSaving = false;
    }
  }

  async function loadBackups() {
    backupsLoading = true;
    try {
      backups = await invoke<NginxBackup[]>('nginx_list_backups');
    } catch (e) {
      uiStore.addToast(`Failed to load backups: ${e}`, 'error');
    } finally {
      backupsLoading = false;
    }
  }

  async function restoreBackup(backup: NginxBackup) {
    uiStore.confirm(
      'Restore Backup',
      `Restore "${backup.filename}" to "${backup.original_path}"? Current file will be backed up first.`,
      async () => {
        try {
          const result = await invoke<NginxTestResult>('nginx_restore_backup', {
            backupPath: backup.backup_path,
            originalPath: backup.original_path,
          });
          if (result.passed) {
            uiStore.addToast('Backup restored and nginx reloaded ✓', 'success');
          } else {
            uiStore.addToast('Backup restored but nginx -t failed', 'warning');
            showOutputModal = true;
            outputModalTitle = 'nginx -t Result After Restore';
            outputModalContent = result.output;
          }
          if (selectedConfig && selectedConfig.path === backup.original_path) {
            const content = await invoke<string>('nginx_read_config', { path: selectedConfig.path });
            editorContent = content;
            savedContent = content;
          }
        } catch (e) {
          uiStore.addToast(`Restore failed: ${e}`, 'error');
        }
      },
    );
  }

  $effect(() => {
    if (activeTab === 'editor' && configs.length === 0) loadConfigs();
    if (activeTab === 'sites' && sites.length === 0) loadSites();
    if (activeTab === 'www' && wwwEntries.length === 0) loadWww();
    if (activeTab === 'logs' && logFiles.length === 0) loadLogFiles();
    if (activeTab === 'ssl' && sslCerts.length === 0 && hasCertbot) loadSslCerts();
  });

  // Diff helper
  function getDiff(): { type: 'add'|'remove'|'same'; text: string }[] {
    const oldLines = savedContent.split('\n');
    const newLines = editorContent.split('\n');
    const result: { type: 'add'|'remove'|'same'; text: string }[] = [];
    const max = Math.max(oldLines.length, newLines.length);
    for (let i = 0; i < max; i++) {
      const o = oldLines[i];
      const n = newLines[i];
      if (o === undefined) {
        result.push({ type: 'add', text: n });
      } else if (n === undefined) {
        result.push({ type: 'remove', text: o });
      } else if (o === n) {
        result.push({ type: 'same', text: o });
      } else {
        result.push({ type: 'remove', text: o });
        result.push({ type: 'add', text: n });
      }
    }
    return result;
  }

  // ─── WWW Browser ───────────────────────────────────────────────────────────

  async function loadWww() {
    wwwLoading = true;
    try {
      wwwEntries = await invoke<WwwEntry[]>('nginx_list_www');
    } catch (e) {
      uiStore.addToast(`Failed to load /var/www: ${e}`, 'error');
    } finally {
      wwwLoading = false;
    }
  }

  function toggleExpand(path: string) {
    const next = new Set(expandedPaths);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    expandedPaths = next;
  }

  async function viewWwwFile(entry: WwwEntry) {
    if (entry.is_dir) { toggleExpand(entry.path); return; }
    selectedWwwEntry = entry;
    wwwFileContent = '';
    wwwFileLoading = true;
    try {
      wwwFileContent = await invoke<string>('nginx_read_www_file', { path: entry.path });
    } catch (e) {
      wwwFileContent = String(e);
    } finally {
      wwwFileLoading = false;
    }
  }

  async function uploadFile(destDir: string) {
    try {
      const selected = await openDialog({ multiple: false, directory: false });
      if (!selected) return;
      const srcPath = typeof selected === 'string' ? selected : selected[0];
      await invoke('nginx_upload_www_file', { srcPath, destDir });
      uiStore.addToast('File uploaded ✓', 'success');
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Upload failed: ${e}`, 'error');
    }
  }

  async function createDir() {
    if (!newDirName.trim()) return;
    const full = `${newDirParent}/${newDirName}`.replace(/\/\//g, '/');
    try {
      await invoke('nginx_create_www_dir', { path: full });
      uiStore.addToast(`Directory created: ${full}`, 'success');
      showNewDirForm = false;
      newDirName = '';
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Create dir failed: ${e}`, 'error');
    }
  }

  function confirmDeleteWww(entry: WwwEntry) {
    uiStore.confirm(
      'Delete Entry',
      `Delete "${entry.name}"? This is permanent.`,
      () => {
        uiStore.confirm(
          '⚠️ Confirm Delete',
          `Permanently delete "${entry.path}"?`,
          async () => {
            try {
              await invoke('nginx_delete_www_entry', { path: entry.path });
              uiStore.addToast(`Deleted "${entry.name}"`, 'success');
              if (selectedWwwEntry?.path === entry.path) selectedWwwEntry = null;
              await loadWww();
            } catch (e) {
              uiStore.addToast(`Delete failed: ${e}`, 'error');
            }
          },
          true,
        );
      },
      true,
    );
  }

  async function doRename() {
    if (!renamingEntry || !renameValue.trim()) return;
    const parentDir = renamingEntry.path.substring(0, renamingEntry.path.lastIndexOf('/'));
    const newPath = `${parentDir}/${renameValue}`;
    try {
      await invoke('nginx_rename_www_entry', { oldPath: renamingEntry.path, newPath });
      uiStore.addToast('Renamed ✓', 'success');
      renamingEntry = null;
      await loadWww();
    } catch (e) {
      uiStore.addToast(`Rename failed: ${e}`, 'error');
    }
  }

  function formatSize(bytes: number): string {
    if (bytes === 0) return '—';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  // ─── Logs ──────────────────────────────────────────────────────────────────

  async function loadLogFiles() {
    try {
      logFiles = await invoke<string[]>('nginx_list_log_files');
      if (logFiles.length > 0 && !selectedLog) {
        selectedLog = logFiles[0];
        await loadLog();
      }
    } catch (e) {
      uiStore.addToast(`Failed to load log files: ${e}`, 'error');
    }
  }

  async function loadLog() {
    if (!selectedLog) return;
    logLoading = true;
    try {
      logContent = await invoke<string>('nginx_read_log', {
        path: selectedLog,
        lines: 200,
        filter: logFilter || null,
      });
    } catch (e) {
      logContent = String(e);
    } finally {
      logLoading = false;
    }
  }

  function toggleAutoRefresh() {
    logAutoRefresh = !logAutoRefresh;
    if (logAutoRefresh) {
      logInterval = setInterval(() => loadLog(), 5000);
    } else {
      if (logInterval) { clearInterval(logInterval); logInterval = null; }
    }
  }

  function confirmClearLog() {
    uiStore.confirm(
      'Clear Log',
      `Truncate "${selectedLog}"? This cannot be undone.`,
      async () => {
        try {
          await invoke('nginx_clear_log', { path: selectedLog });
          uiStore.addToast('Log cleared ✓', 'success');
          await loadLog();
        } catch (e) {
          uiStore.addToast(`Clear failed: ${e}`, 'error');
        }
      },
      true,
    );
  }

  async function exportLog() {
    const blob = new Blob([logContent], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = selectedLog.split('/').pop() ?? 'nginx.log';
    a.click();
    URL.revokeObjectURL(url);
  }

  // ─── SSL ───────────────────────────────────────────────────────────────────

  async function loadSslCerts() {
    sslLoading = true;
    try {
      sslCerts = await invoke<SslCert[]>('nginx_list_ssl_certs');
    } catch (e) {
      uiStore.addToast(`Failed to load certs: ${e}`, 'error');
    } finally {
      sslLoading = false;
    }
  }

  async function renewCert(domain: string) {
    uiStore.confirm(
      'Renew Certificate',
      `Run certbot renew for "${domain}"?`,
      async () => {
        renewingCert = domain;
        try {
          const output = await invoke<string>('nginx_renew_cert', { domain });
          uiStore.addToast(`Cert renewed for ${domain} ✓`, 'success');
          showOutputModal = true;
          outputModalTitle = 'certbot renew output';
          outputModalContent = output;
          await loadSslCerts();
        } catch (e) {
          uiStore.addToast(`Renewal failed: ${e}`, 'error');
          showOutputModal = true;
          outputModalTitle = 'certbot renew failed';
          outputModalContent = String(e);
        } finally {
          renewingCert = '';
        }
      },
    );
  }

  const tabDefs = $derived([
    { id: 'overview', label: 'Overview', icon: Activity },
    { id: 'sites',    label: 'Sites',    icon: Globe },
    { id: 'editor',   label: 'Config Editor', icon: FileCode },
    { id: 'www',      label: 'WWW Files', icon: FolderOpen },
    { id: 'logs',     label: 'Logs',     icon: FileText },
    ...(hasCertbot ? [{ id: 'ssl', label: 'SSL', icon: Lock }] : []),
  ] as { id: typeof activeTab; label: string; icon: any }[]);

  function hasChanges() {
    return editorContent !== savedContent;
  }
</script>

<!-- ─── Page ──────────────────────────────────────────────────────────── -->
<div class="module-page">
  <!-- Header -->
  <div class="module-header">
    <div class="module-icon">
      <Server size={24} />
    </div>
    <div>
      <h1 class="module-title">Nginx Manager</h1>
      <p class="module-subtitle">
        {#if loading}
          Checking nginx…
        {:else if installInfo?.installed}
          {installInfo.version} — Manage web server configs, sites, and files
        {:else}
          nginx is not installed on this system
        {/if}
      </p>
    </div>
  </div>

  {#if loading}
    <div class="center-state">
      <div class="spinner"></div>
      <span>Checking nginx installation…</span>
    </div>
  {:else if !installInfo?.installed}
    <!-- ─── Not Installed State ─── -->
    <div class="not-installed">
      <div class="ni-icon"><Server size={48} /></div>
      <h2>nginx is not installed on your system</h2>
      <p>Install nginx to use this module.</p>
      <div class="ni-cmds">
        <div class="ni-cmd">
          <span class="ni-cmd-label">Fedora / RHEL:</span>
          <code>sudo dnf install nginx</code>
        </div>
        <div class="ni-cmd">
          <span class="ni-cmd-label">Debian / Ubuntu:</span>
          <code>sudo apt install nginx</code>
        </div>
        <div class="ni-cmd">
          <span class="ni-cmd-label">Arch:</span>
          <code>sudo pacman -S nginx</code>
        </div>
      </div>
      <button class="btn btn-primary" onclick={init}>
        <RefreshCw size={14} /> Recheck
      </button>
    </div>
  {:else}
    <!-- ─── Tab Bar ─── -->
    <div class="tab-bar">
      {#each tabDefs as tab}
        <button
          class="tab-btn"
          class:active={activeTab === tab.id}
          onclick={() => (activeTab = tab.id)}
          id={`nginx-tab-${tab.id}`}
        >
          <tab.icon size={14} />
          {tab.label}
        </button>
      {/each}
    </div>

    <!-- ─── Tab Content ─── -->
    <div class="tab-content module-content-scroll">

      <!-- ══ OVERVIEW ══════════════════════════════════════════════════════ -->
      {#if activeTab === 'overview'}
        <div class="overview-grid">
          <!-- Service Status Card -->
          <div class="card ov-card">
            <div class="ov-card-header">
              <div class="ov-card-title">
                <Activity size={16} />
                Service Status
              </div>
              <div class="status-dot {serviceStatus?.active ? 'dot-active' : 'dot-inactive'}"></div>
            </div>
            {#if serviceStatus}
              <div class="service-status-badge badge {serviceStatus.active ? 'badge-success' : 'badge-error'}">
                {serviceStatus.status} — {serviceStatus.sub_state}
              </div>
              {#if serviceStatus.since}
                <p class="ov-since">Since: {serviceStatus.since}</p>
              {/if}
            {:else}
              <p class="ov-since">Loading…</p>
            {/if}
            <div class="service-btns">
              {#each [['start','Start',false], ['stop','Stop',true], ['restart','Restart',false], ['reload','Reload',false]] as [action, label, isDanger]}
                <button
                  class="btn btn-sm {isDanger ? 'btn-danger' : 'btn-outline'}"
                  onclick={() => doServiceAction(action as string)}
                  disabled={serviceLoading}
                  id={`nginx-svc-${action}`}
                >
                  {#if action === 'start'}<Play size={12} />
                  {:else if action === 'stop'}<Square size={12} />
                  {:else if action === 'restart'}<RotateCcw size={12} />
                  {:else}<RefreshCw size={12} />
                  {/if}
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <!-- Config Test Card -->
          <div class="card ov-card">
            <div class="ov-card-header">
              <div class="ov-card-title">
                <TerminalSquare size={16} />
                Config Test (nginx -t)
              </div>
              <button class="btn btn-sm btn-outline" onclick={runTest} disabled={testLoading} id="nginx-run-test">
                {#if testLoading}
                  <div class="spinner-sm"></div>
                {:else}
                  <RefreshCw size={12} />
                {/if}
                Run Test
              </button>
            </div>
            {#if testResult}
              <div class="test-result {testResult.passed ? 'test-pass' : 'test-fail'}">
                {#if testResult.passed}
                  <CheckCircle size={18} /> <span>Configuration OK</span>
                {:else}
                  <XCircle size={18} /> <span>Configuration Error</span>
                {/if}
              </div>
              <p class="ov-since">{testResult.timestamp}</p>
              <pre class="test-output">{testResult.output}</pre>
            {:else}
              <p class="ov-since">Run test to see result</p>
            {/if}
          </div>

          <!-- Stats Card -->
          <div class="card ov-card ov-stats-card">
            <div class="ov-card-header">
              <div class="ov-card-title">
                <Globe size={16} />
                Sites Overview
              </div>
              <button class="btn btn-sm btn-ghost" onclick={() => Promise.all([loadStats(), loadServiceStatus()])} id="nginx-refresh-stats">
                <RefreshCw size={12} />
              </button>
            </div>
            <div class="stats-grid">
              <div class="stat-item">
                <span class="stat-value">{stats?.sites_available ?? '—'}</span>
                <span class="stat-label">Available</span>
              </div>
              <div class="stat-item stat-enabled">
                <span class="stat-value">{stats?.sites_enabled ?? '—'}</span>
                <span class="stat-label">Enabled</span>
              </div>
              <div class="stat-item stat-disabled">
                <span class="stat-value">{stats?.sites_disabled ?? '—'}</span>
                <span class="stat-label">Disabled</span>
              </div>
            </div>
          </div>

          <!-- Version Card -->
          <div class="card ov-card">
            <div class="ov-card-header">
              <div class="ov-card-title"><Server size={16} />Version</div>
            </div>
            <div class="version-display">{installInfo.version}</div>
            <p class="ov-since">Installed at <code>/usr/sbin/nginx</code></p>
          </div>
        </div>

      <!-- ══ SITES ══════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'sites'}
        <div class="tab-section">
          <div class="section-header">
            <h3>Site Configurations</h3>
            <div class="header-actions">
              <button class="btn btn-ghost btn-sm" onclick={loadSites} id="nginx-refresh-sites">
                <RefreshCw size={13} /> Refresh
              </button>
              <button class="btn btn-primary btn-sm" onclick={() => (showNewSiteForm = true)} id="nginx-new-site">
                <Plus size={13} /> New Site
              </button>
            </div>
          </div>

          {#if showNewSiteForm}
            <div class="card new-site-form">
              <h4 class="form-title">Create New Site</h4>
              <div class="form-grid">
                <label class="form-field">
                  <span>Server Name (domain)</span>
                  <input type="text" bind:value={newSite.server_name} placeholder="example.com" id="nginx-site-name" />
                </label>
                <label class="form-field">
                  <span>Port</span>
                  <input type="number" bind:value={newSite.port} min={1} max={65535} id="nginx-site-port" />
                </label>
                <label class="form-field form-toggle">
                  <span>Reverse Proxy</span>
                  <div class="toggle-wrap">
                    <input type="checkbox" bind:checked={newSite.is_proxy} id="nginx-site-proxy" />
                    <label for="nginx-site-proxy" class="toggle-label"></label>
                  </div>
                </label>
                {#if newSite.is_proxy}
                  <label class="form-field form-full">
                    <span>Proxy Target URL</span>
                    <input type="text" bind:value={newSite.proxy_url} placeholder="http://localhost:3000" id="nginx-site-proxy-url" />
                  </label>
                {:else}
                  <label class="form-field">
                    <span>Root Directory</span>
                    <input type="text" bind:value={newSite.root_dir} placeholder="/var/www/html" id="nginx-site-root" />
                  </label>
                  <label class="form-field">
                    <span>Index File</span>
                    <input type="text" bind:value={newSite.index_file} id="nginx-site-index" />
                  </label>
                {/if}
                <label class="form-field form-check">
                  <input type="checkbox" bind:checked={newSite.enable_404} id="nginx-site-404" />
                  <span>Include 404 error page</span>
                </label>
                <label class="form-field form-check">
                  <input type="checkbox" bind:checked={newSite.enable_50x} id="nginx-site-50x" />
                  <span>Include 50x error pages</span>
                </label>
              </div>
              <div class="form-actions">
                <button class="btn btn-ghost" onclick={() => (showNewSiteForm = false)}>Cancel</button>
                <button class="btn btn-primary" onclick={createSite} disabled={newSiteLoading} id="nginx-create-site-submit">
                  {#if newSiteLoading}<div class="spinner-sm"></div>{/if}
                  Create Site
                </button>
              </div>
            </div>
          {/if}

          {#if sitesLoading}
            <div class="center-state"><div class="spinner"></div></div>
          {:else if sites.length === 0}
            <div class="empty-state">No site configurations found</div>
          {:else}
            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Source</th>
                    <th>Status</th>
                    <th>Path</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {#each sites as site}
                    <tr>
                      <td class="site-name">{site.name}</td>
                      <td><span class="badge badge-muted">{site.source}</span></td>
                      <td>
                        <span class="badge {site.enabled ? 'badge-success' : 'badge-error'}">
                          {site.enabled ? '● Enabled' : '○ Disabled'}
                        </span>
                      </td>
                      <td><code class="path-code">{site.path}</code></td>
                      <td>
                        <div class="row-actions">
                          {#if site.source === 'sites-available'}
                            <button
                              class="btn btn-sm {site.enabled ? 'btn-outline' : 'btn-primary'}"
                              onclick={() => toggleSite(site)}
                              disabled={toggleLoadingFor === site.name}
                              id={`nginx-toggle-${site.name}`}
                            >
                              {#if toggleLoadingFor === site.name}
                                <div class="spinner-sm"></div>
                              {:else if site.enabled}
                                <EyeOff size={12} />
                              {:else}
                                <Eye size={12} />
                              {/if}
                              {site.enabled ? 'Disable' : 'Enable'}
                            </button>
                          {/if}
                          <button
                            class="btn btn-sm btn-danger"
                            onclick={() => confirmDeleteSite(site)}
                            id={`nginx-delete-site-${site.name}`}
                          >
                            <Trash2 size={12} /> Delete
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>

      <!-- ══ CONFIG EDITOR ══════════════════════════════════════════════════ -->
      {:else if activeTab === 'editor'}
        <div class="editor-layout">
          <!-- File List -->
          <div class="editor-sidebar">
            <div class="editor-sidebar-header">
              <span>Files</span>
              <button class="btn btn-ghost btn-sm" onclick={loadConfigs} id="nginx-refresh-configs">
                <RefreshCw size={12} />
              </button>
            </div>

            {#if editorLoading && configs.length === 0}
              <div class="center-state"><div class="spinner-sm"></div></div>
            {:else}
              {#each ['nginx', 'conf.d', 'sites-available'] as src}
                {@const group = configs.filter(c => c.source === src)}
                {#if group.length > 0}
                  <div class="file-group-label">{src}</div>
                  {#each group as cfg}
                    <button
                      class="file-item"
                      class:selected={selectedConfig?.path === cfg.path}
                      onclick={() => selectConfig(cfg)}
                      id={`nginx-cfg-${cfg.name}`}
                    >
                      <FileCode size={12} />
                      <span>{cfg.name}</span>
                    </button>
                  {/each}
                {/if}
              {/each}
            {/if}

            <div class="editor-sidebar-sep"></div>
            <button class="btn btn-outline btn-sm sidebar-backup-btn" onclick={() => { showBackups = !showBackups; if (showBackups) loadBackups(); }} id="nginx-show-backups">
              <ArchiveRestore size={12} /> Backups ({backups.length})
            </button>
          </div>

          <!-- Editor Panel -->
          <div class="editor-main">
            {#if selectedConfig}
              <div class="editor-toolbar">
                <span class="editor-filename"><FileCode size={14} />{selectedConfig.name}</span>
                <div class="editor-tools">
                  <button class="btn btn-ghost btn-sm" onclick={() => (wordWrap = !wordWrap)} id="nginx-word-wrap">
                    {wordWrap ? 'Wrap: On' : 'Wrap: Off'}
                  </button>
                  {#if hasChanges()}
                    <button class="btn btn-ghost btn-sm" onclick={() => (showDiff = !showDiff)} id="nginx-show-diff">
                      {showDiff ? 'Hide Diff' : 'Show Diff'}
                    </button>
                  {/if}
                  <button
                    class="btn btn-primary btn-sm"
                    onclick={saveConfig}
                    disabled={configSaving || !hasChanges()}
                    id="nginx-save-config"
                  >
                    {#if configSaving}<div class="spinner-sm"></div>{:else}<Save size={12} />{/if}
                    Save
                  </button>
                </div>
              </div>

              {#if hasChanges()}
                <div class="unsaved-warning">
                  <AlertTriangle size={13} /> Unsaved changes — will run nginx -t before writing
                </div>
              {/if}

              {#if showDiff}
                <div class="diff-view">
                  {#each getDiff() as line}
                    <div class="diff-line diff-{line.type}">
                      <span class="diff-marker">{line.type === 'add' ? '+' : line.type === 'remove' ? '-' : ' '}</span>
                      <span class="diff-text">{line.text}</span>
                    </div>
                  {/each}
                </div>
              {:else}
                <textarea
                  class="code-editor"
                  class:wrap={wordWrap}
                  bind:value={editorContent}
                  spellcheck={false}
                  id="nginx-editor-textarea"
                ></textarea>
              {/if}
            {:else}
              <div class="editor-empty">
                <FileCode size={40} />
                <p>Select a config file to edit</p>
              </div>
            {/if}
          </div>

          <!-- Backups panel -->
          {#if showBackups}
            <div class="backups-panel">
              <div class="editor-sidebar-header">
                <span>Backups</span>
                <button class="btn btn-ghost btn-sm" onclick={() => (showBackups = false)}>✕</button>
              </div>
              {#if backupsLoading}
                <div class="center-state"><div class="spinner-sm"></div></div>
              {:else if backups.length === 0}
                <p class="empty-state">No backups yet</p>
              {:else}
                {#each backups as bk}
                  <div class="backup-item">
                    <div class="backup-name">{bk.filename}</div>
                    <div class="backup-ts">{bk.timestamp}</div>
                    <button class="btn btn-sm btn-outline" onclick={() => restoreBackup(bk)} id={`nginx-restore-${bk.filename}`}>
                      <ArchiveRestore size={11} /> Restore
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>

      <!-- ══ WWW FILES ══════════════════════════════════════════════════════ -->
      {:else if activeTab === 'www'}
        <div class="www-layout">
          <!-- Tree -->
          <div class="www-tree">
            <div class="www-tree-header">
              <span>/var/www</span>
              <div class="header-actions">
                <button class="btn btn-ghost btn-sm" onclick={loadWww} id="nginx-refresh-www"><RefreshCw size={12} /></button>
                <button class="btn btn-outline btn-sm" onclick={() => { showNewDirForm = !showNewDirForm; newDirParent = '/var/www'; }} id="nginx-new-dir">
                  <FolderPlus size={12} />
                </button>
              </div>
            </div>
            {#if showNewDirForm}
              <div class="new-dir-form">
                <input type="text" bind:value={newDirName} placeholder="folder-name" id="nginx-new-dir-name" />
                <button class="btn btn-primary btn-sm" onclick={createDir} id="nginx-create-dir">Create</button>
                <button class="btn btn-ghost btn-sm" onclick={() => (showNewDirForm = false)}>✕</button>
              </div>
            {/if}
            {#if wwwLoading}
              <div class="center-state"><div class="spinner-sm"></div></div>
            {:else if wwwEntries.length === 0}
              <div class="empty-state">/var/www is empty or does not exist</div>
            {:else}
              <div class="tree-list">
                {#each wwwEntries as entry}
                  {@render wwwTreeNode(entry, 0)}
                {/each}
              </div>
            {/if}
          </div>

          <!-- File Viewer -->
          <div class="www-viewer">
            {#if selectedWwwEntry && !selectedWwwEntry.is_dir}
              <div class="viewer-header">
                <span class="editor-filename"><FileText size={14} />{selectedWwwEntry.name}</span>
                <div class="header-actions">
                  <span class="badge badge-muted">{formatSize(selectedWwwEntry.size)}</span>
                  <button class="btn btn-danger btn-sm" onclick={() => confirmDeleteWww(selectedWwwEntry!)} id="nginx-delete-www-selected">
                    <Trash2 size={12} /> Delete
                  </button>
                </div>
              </div>
              {#if wwwFileLoading}
                <div class="center-state"><div class="spinner"></div></div>
              {:else}
                <pre class="file-view">{wwwFileContent}</pre>
              {/if}
            {:else}
              <div class="editor-empty">
                <FolderOpen size={40} />
                <p>Select a file to view its contents</p>
                <p class="ov-since">Binary files cannot be displayed</p>
              </div>
            {/if}
          </div>
        </div>

      <!-- ══ LOGS ══════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'logs'}
        <div class="tab-section">
          <div class="logs-toolbar">
            <select bind:value={selectedLog} onchange={loadLog} class="log-select" id="nginx-log-select">
              {#each logFiles as lf}
                <option value={lf}>{lf}</option>
              {/each}
            </select>
            <div class="log-filter">
              <Search size={13} />
              <input type="text" bind:value={logFilter} onchange={loadLog} placeholder="Filter…" id="nginx-log-filter" />
            </div>
            <button class="btn btn-outline btn-sm" onclick={loadLog} id="nginx-log-refresh">
              <RefreshCw size={13} /> Refresh
            </button>
            <button
              class="btn btn-sm {logAutoRefresh ? 'btn-primary' : 'btn-outline'}"
              onclick={toggleAutoRefresh}
              id="nginx-log-auto"
            >
              <Clock size={13} /> {logAutoRefresh ? 'Auto: On' : 'Auto: Off'}
            </button>
            <button class="btn btn-outline btn-sm" onclick={exportLog} id="nginx-log-export">
              <Download size={13} /> Export
            </button>
            <button class="btn btn-danger btn-sm" onclick={confirmClearLog} id="nginx-log-clear">
              <Trash2 size={13} /> Clear
            </button>
          </div>
          {#if logLoading}
            <div class="center-state"><div class="spinner"></div></div>
          {:else}
            <pre class="log-view">{logContent || '(empty)'}</pre>
          {/if}
        </div>

      <!-- ══ SSL ════════════════════════════════════════════════════════════ -->
      {:else if activeTab === 'ssl'}
        <div class="tab-section">
          <div class="section-header">
            <h3>SSL Certificates — Let's Encrypt</h3>
            <button class="btn btn-ghost btn-sm" onclick={loadSslCerts} id="nginx-refresh-ssl">
              <RefreshCw size={13} /> Refresh
            </button>
          </div>
          <div class="ssl-notice">
            <AlertTriangle size={14} />
            To issue a <strong>new</strong> certificate, use the terminal:
            <code>sudo certbot --nginx -d yourdomain.com</code>
          </div>
          {#if sslLoading}
            <div class="center-state"><div class="spinner"></div></div>
          {:else if sslCerts.length === 0}
            <div class="empty-state">No certificates found in /etc/letsencrypt/live/</div>
          {:else}
            <div class="ssl-grid">
              {#each sslCerts as cert}
                <div class="card ssl-card">
                  <div class="ssl-card-header">
                    <span class="ssl-domain"><Lock size={14} />{cert.domain}</span>
                    <span class="badge {cert.status === 'valid' ? 'badge-success' : cert.status === 'expiring' ? 'badge-warning' : 'badge-error'}">
                      {cert.status === 'valid' ? '✓ Valid' : cert.status === 'expiring' ? '⚠ Expiring' : '✗ Expired'}
                    </span>
                  </div>
                  <div class="ssl-meta">
                    <div>Expires: <strong>{cert.expiry}</strong></div>
                    <div class="ssl-days {cert.status}">
                      {cert.days_until_expiry >= 0 ? `${cert.days_until_expiry} days left` : `Expired ${Math.abs(cert.days_until_expiry)} days ago`}
                    </div>
                    <div class="ssl-expiry-bar">
                      <div class="ssl-expiry-fill {cert.status}" style="width: {Math.min(100, Math.max(0, (cert.days_until_expiry / 90) * 100))}%"></div>
                    </div>
                  </div>
                  <div class="ssl-actions">
                    <button
                      class="btn btn-outline btn-sm"
                      onclick={() => renewCert(cert.domain)}
                      disabled={renewingCert === cert.domain}
                      id={`nginx-renew-${cert.domain}`}
                    >
                      {#if renewingCert === cert.domain}<div class="spinner-sm"></div>{:else}<RefreshCw size={12} />{/if}
                      Renew
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- ─── WWW Tree Node Snippet ─────────────────────────────────────────────── -->
{#snippet wwwTreeNode(entry: WwwEntry, depth: number)}
  <div class="tree-node" style="padding-left: {depth * 16 + 8}px">
    <button
      class="tree-item {selectedWwwEntry?.path === entry.path ? 'tree-selected' : ''}"
      onclick={() => viewWwwFile(entry)}
      id={`nginx-www-${entry.name}-${depth}`}
    >
      {#if entry.is_dir}
        {#if expandedPaths.has(entry.path)}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
        <FolderOpen size={14} />
      {:else}
        <FileText size={14} />
      {/if}
      {#if renamingEntry?.path === entry.path}
        <input
          class="rename-input"
          bind:value={renameValue}
          onkeydown={(e) => { if (e.key === 'Enter') doRename(); if (e.key === 'Escape') renamingEntry = null; }}
          onclick={(e) => e.stopPropagation()}
          id={`nginx-rename-${entry.name}`}
        />
      {:else}
        <span class="tree-name">{entry.name}</span>
      {/if}
      {#if !entry.is_dir}
        <span class="tree-size">{formatSize(entry.size)}</span>
      {/if}
    </button>
    <div class="tree-actions">
      <button class="tree-action-btn" onclick={(e) => { e.stopPropagation(); renamingEntry = entry; renameValue = entry.name; }} title="Rename" id={`nginx-rename-btn-${entry.name}`}>
        <Edit3 size={11} />
      </button>
      {#if entry.is_dir}
        <button class="tree-action-btn" onclick={(e) => { e.stopPropagation(); uploadFile(entry.path); }} title="Upload file here" id={`nginx-upload-${entry.name}`}>
          <Upload size={11} />
        </button>
        <button class="tree-action-btn" onclick={(e) => { e.stopPropagation(); showNewDirForm = true; newDirParent = entry.path; }} title="New subfolder" id={`nginx-newdir-${entry.name}`}>
          <FolderPlus size={11} />
        </button>
      {/if}
      <button class="tree-action-btn danger" onclick={(e) => { e.stopPropagation(); confirmDeleteWww(entry); }} title="Delete" id={`nginx-del-www-${entry.name}`}>
        <Trash2 size={11} />
      </button>
    </div>
  </div>
  {#if entry.is_dir && expandedPaths.has(entry.path)}
    {#each entry.children as child}
      {@render wwwTreeNode(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

<!-- ─── Modals ─────────────────────────────────────────────────────────────── -->

<!-- nginx -t result modal -->
{#if showTestModal && modalTestResult}
  <div class="modal-backdrop"
    onclick={() => (showTestModal = false)}
    onkeydown={(e) => e.key === 'Escape' && (showTestModal = false)}
    role="dialog" aria-modal="true" tabindex="-1"
  >
    <div class="modal"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="document"
    >
      <div class="modal-header">
        <div class="modal-title {modalTestResult.passed ? 'modal-pass' : 'modal-fail'}">
          {#if modalTestResult.passed}
            <CheckCircle size={20} /> nginx -t Passed
          {:else}
            <XCircle size={20} /> nginx -t Failed
          {/if}
        </div>
        <span class="modal-ts">{modalTestResult.timestamp}</span>
      </div>
      <pre class="modal-output">{modalTestResult.output}</pre>
      <div class="modal-footer">
        <button class="btn btn-primary" onclick={() => (showTestModal = false)} id="nginx-close-test-modal">
          Close
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- General output modal -->
{#if showOutputModal}
  <div class="modal-backdrop"
    onclick={() => (showOutputModal = false)}
    onkeydown={(e) => e.key === 'Escape' && (showOutputModal = false)}
    role="dialog" aria-modal="true" tabindex="-1"
  >
    <div class="modal modal-wide"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="document"
    >
      <div class="modal-header">
        <div class="modal-title">{outputModalTitle}</div>
      </div>
      <pre class="modal-output">{outputModalContent}</pre>
      <div class="modal-footer">
        <button class="btn btn-primary" onclick={() => (showOutputModal = false)} id="nginx-close-output-modal">
          Close
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ─── Layout ─────────────────────────────────────────────────────────── */
  .module-page { overflow: hidden; }
  .tab-content { flex: 1; overflow-y: auto; padding: 0; }
  .tab-section { padding: 24px; display: flex; flex-direction: column; gap: 16px; }

  /* ─── Not Installed ──────────────────────────────────────────────────── */
  .not-installed {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    flex: 1;
    padding: 48px;
    text-align: center;
  }
  .ni-icon { color: var(--color-text-muted); }
  .not-installed h2 { font-size: 22px; color: var(--color-text-primary); margin: 0; }
  .not-installed p { color: var(--color-text-secondary); margin: 0; }
  .ni-cmds { display: flex; flex-direction: column; gap: 10px; margin: 8px 0; width: 100%; max-width: 500px; }
  .ni-cmd {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 12px 16px;
  }
  .ni-cmd-label { font-size: 12px; color: var(--color-text-muted); white-space: nowrap; }
  .ni-cmd code { font-family: var(--font-mono); font-size: 13px; color: var(--color-accent-soft); }

  /* ─── Tab Bar ────────────────────────────────────────────────────────── */
  .tab-bar {
    display: flex;
    gap: 4px;
    padding: 0 32px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }
  .tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-weight: 500;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
    margin-bottom: -1px;
  }
  .tab-btn:hover { color: var(--color-text-primary); }
  .tab-btn.active {
    color: var(--color-accent-soft);
    border-bottom-color: var(--color-accent);
  }

  /* ─── Spinners ───────────────────────────────────────────────────────── */
  .spinner {
    width: 24px; height: 24px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner-sm {
    width: 14px; height: 14px;
    border: 2px solid rgba(255,255,255,0.1);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }
  .center-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px;
    color: var(--color-text-muted);
  }
  .empty-state {
    padding: 32px;
    text-align: center;
    color: var(--color-text-muted);
    font-size: 13px;
  }

  /* ─── Overview ───────────────────────────────────────────────────────── */
  .overview-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    padding: 24px;
  }
  .ov-card { display: flex; flex-direction: column; gap: 12px; }
  .ov-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .ov-card-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .status-dot {
    width: 10px; height: 10px; border-radius: 50%;
    box-shadow: 0 0 8px currentColor;
  }
  .dot-active { background: var(--color-success); color: var(--color-success); animation: pulse 2s infinite; }
  .dot-inactive { background: var(--color-error); color: var(--color-error); }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
  .service-status-badge { font-size: 14px; padding: 6px 14px; }
  .ov-since { font-size: 11px; color: var(--color-text-muted); margin: 0; }
  .service-btns { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 4px; }
  .test-result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-radius: 10px;
    font-weight: 600;
  }
  .test-pass { background: var(--color-success-muted); color: var(--color-success); border: 1px solid rgba(16,185,129,0.2); }
  .test-fail { background: var(--color-error-muted); color: var(--color-error); border: 1px solid rgba(244,63,94,0.2); }
  .test-output { max-height: 80px; overflow: auto; font-size: 11px; }
  .ov-stats-card {}
  .stats-grid { display: flex; gap: 16px; }
  .stat-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 16px 12px;
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    border-radius: 12px;
  }
  .stat-value { font-size: 28px; font-weight: 700; color: var(--color-text-primary); line-height: 1; }
  .stat-label { font-size: 11px; color: var(--color-text-muted); text-transform: uppercase; margin-top: 4px; }
  .stat-enabled .stat-value { color: var(--color-success); }
  .stat-disabled .stat-value { color: var(--color-error); }
  .version-display {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-accent-soft);
    background: var(--color-bg-raised);
    border: 1px solid var(--color-border);
    padding: 10px 14px;
    border-radius: 8px;
  }

  /* ─── Section Headers ────────────────────────────────────────────────── */
  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .section-header h3 { margin: 0; font-size: 15px; font-weight: 600; }
  .header-actions { display: flex; gap: 8px; }
  .row-actions { display: flex; gap: 6px; }

  /* ─── Sites ──────────────────────────────────────────────────────────── */
  .site-name { font-weight: 600; color: var(--color-text-primary); }
  .path-code { font-size: 11px; color: var(--color-text-muted); }

  .new-site-form { margin-bottom: 8px; }
  .form-title { margin: 0 0 16px; font-size: 14px; font-weight: 600; }
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .form-field { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--color-text-secondary); }
  .form-field.form-full { grid-column: 1 / -1; }
  .form-field.form-check { flex-direction: row; align-items: center; gap: 8px; }
  .form-field.form-toggle { flex-direction: row; align-items: center; justify-content: space-between; }
  .form-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }

  /* Toggle switch */
  .toggle-wrap { position: relative; display: inline-flex; align-items: center; }
  .toggle-wrap input[type="checkbox"] {
    width: 36px; height: 20px; border-radius: 10px;
    background: var(--color-bg-hover);
    transition: background 0.2s;
    cursor: pointer;
  }
  .toggle-wrap input[type="checkbox"]:checked { background: var(--color-accent); }
  .toggle-wrap input[type="checkbox"]::after {
    content: ''; position: absolute;
    top: 3px; left: 3px;
    width: 14px; height: 14px;
    border-radius: 50%; background: white;
    transition: transform 0.2s;
    transform: none; border: none; border-width: 0;
  }
  .toggle-wrap input[type="checkbox"]:checked::after { transform: translateX(16px); }

  /* ─── Config Editor ──────────────────────────────────────────────────── */
  .editor-layout {
    display: flex;
    flex: 1;
    height: 100%;
    overflow: hidden;
  }
  .editor-sidebar {
    width: 220px;
    min-width: 220px;
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 8px 0;
    gap: 2px;
  }
  .editor-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 12px 8px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text-muted);
  }
  .editor-sidebar-sep { border-top: 1px solid var(--color-border); margin: 8px 12px; }
  .sidebar-backup-btn { margin: 0 12px 8px; }
  .file-group-label {
    padding: 4px 12px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.08em;
    margin-top: 6px;
  }
  .file-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-mono);
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file-item:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
  .file-item.selected { background: var(--color-active-bg); color: var(--color-accent-soft); }
  .editor-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 12px;
  }
  .editor-tools { display: flex; gap: 8px; align-items: center; }
  .editor-filename {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--color-text-primary);
    font-weight: 600;
  }
  .unsaved-warning {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    background: var(--color-warning-muted);
    color: var(--color-warning);
    font-size: 12px;
    border-bottom: 1px solid rgba(245,158,11,0.2);
  }
  .code-editor {
    flex: 1;
    resize: none;
    border: none;
    outline: none;
    background: rgba(0,0,0,0.25);
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.6;
    padding: 16px;
    overflow-y: auto;
  }
  .code-editor.wrap { white-space: pre-wrap; word-break: break-all; }
  .editor-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
  }
  .diff-view {
    flex: 1;
    overflow-y: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    background: rgba(0,0,0,0.2);
    padding: 12px 0;
  }
  .diff-line { display: flex; gap: 12px; padding: 1px 16px; }
  .diff-add { background: rgba(16,185,129,0.1); color: var(--color-success); }
  .diff-remove { background: rgba(244,63,94,0.1); color: var(--color-error); }
  .diff-same { color: var(--color-text-muted); }
  .diff-marker { width: 14px; flex-shrink: 0; font-weight: 700; }
  .diff-text { white-space: pre-wrap; word-break: break-all; }

  /* Backups panel */
  .backups-panel {
    width: 250px;
    min-width: 250px;
    border-left: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 0;
  }
  .backup-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }
  .backup-name { font-family: var(--font-mono); color: var(--color-text-primary); font-size: 11px; word-break: break-all; }
  .backup-ts { color: var(--color-text-muted); font-size: 10px; }

  /* ─── WWW Files ──────────────────────────────────────────────────────── */
  .www-layout { display: flex; height: 100%; overflow: hidden; }
  .www-tree {
    width: 280px;
    min-width: 280px;
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 8px 0;
  }
  .www-tree-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 12px 8px;
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-secondary);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 4px;
  }
  .new-dir-form {
    display: flex;
    gap: 6px;
    padding: 8px 12px;
    align-items: center;
    border-bottom: 1px solid var(--color-border);
  }
  .new-dir-form input { flex: 1; padding: 6px 10px; font-size: 12px; }
  .tree-list { display: flex; flex-direction: column; }
  .tree-node {
    display: flex;
    align-items: center;
    gap: 4px;
    position: relative;
  }
  .tree-item {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 0;
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    min-width: 0;
    transition: color 0.15s;
  }
  .tree-item:hover { color: var(--color-text-primary); }
  .tree-item.tree-selected { color: var(--color-accent-soft); }
  .tree-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tree-size { font-size: 10px; color: var(--color-text-muted); flex-shrink: 0; }
  .tree-actions {
    display: none;
    gap: 2px;
    padding-right: 8px;
    flex-shrink: 0;
  }
  .tree-node:hover .tree-actions { display: flex; }
  .tree-action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 4px;
    transition: all 0.15s;
  }
  .tree-action-btn:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
  .tree-action-btn.danger:hover { color: var(--color-error); }
  .rename-input { font-size: 12px; padding: 2px 6px; width: 120px; }
  .www-viewer {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .viewer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 12px;
  }
  .file-view {
    flex: 1;
    overflow: auto;
    margin: 0;
    padding: 16px;
    border: none;
    border-radius: 0;
    font-size: 12px;
    line-height: 1.6;
  }

  /* ─── Logs ───────────────────────────────────────────────────────────── */
  .logs-toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }
  .log-select {
    padding: 8px 14px;
    background: rgba(15, 15, 24, 0.9);
    border: 1px solid var(--color-border-hover);
    border-radius: 10px;
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: var(--font-mono);
    outline: none;
    cursor: pointer;
    flex: 1;
    min-width: 200px;
    appearance: auto;
    -webkit-appearance: auto;
    color-scheme: dark;
  }
  .log-select:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-accent-muted);
  }
  .log-select option {
    background: #0f0f18;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    font-size: 13px;
    padding: 8px 12px;
  }
  .log-select option:hover,
  .log-select option:checked {
    background: var(--color-accent-muted);
    color: var(--color-accent-soft);
  }
  .log-filter {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 6px 12px;
    color: var(--color-text-muted);
  }
  .log-filter input {
    width: 160px;
    border: none;
    background: transparent;
    color: var(--color-text-primary);
    font-size: 13px;
    outline: none;
    padding: 0;
  }
  .log-view {
    flex: 1;
    font-size: 11px;
    line-height: 1.5;
    background: rgba(0,0,0,0.3);
    border-radius: 12px;
    border: 1px solid var(--color-border);
    padding: 12px 16px;
    overflow: auto;
    max-height: calc(100vh - 280px);
    color: var(--color-text-secondary);
  }

  /* ─── SSL ────────────────────────────────────────────────────────────── */
  .ssl-notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: var(--color-info-muted);
    color: var(--color-info);
    border: 1px solid rgba(14,165,233,0.2);
    border-radius: 10px;
    font-size: 13px;
    flex-wrap: wrap;
  }
  .ssl-notice code { font-family: var(--font-mono); font-size: 12px; }
  .ssl-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; }
  .ssl-card { display: flex; flex-direction: column; gap: 12px; }
  .ssl-card-header { display: flex; align-items: center; justify-content: space-between; }
  .ssl-domain { display: flex; align-items: center; gap: 6px; font-weight: 600; font-size: 14px; }
  .ssl-meta { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--color-text-secondary); }
  .ssl-days { font-weight: 600; }
  .ssl-days.valid { color: var(--color-success); }
  .ssl-days.expiring { color: var(--color-warning); }
  .ssl-days.expired { color: var(--color-error); }
  .ssl-expiry-bar {
    height: 4px;
    background: rgba(255,255,255,0.08);
    border-radius: 2px;
    overflow: hidden;
  }
  .ssl-expiry-fill { height: 100%; border-radius: 2px; transition: width 0.5s ease; }
  .ssl-expiry-fill.valid { background: var(--color-success); }
  .ssl-expiry-fill.expiring { background: var(--color-warning); }
  .ssl-expiry-fill.expired { background: var(--color-error); }
  .ssl-actions { display: flex; justify-content: flex-end; }

  /* ─── Modals ─────────────────────────────────────────────────────────── */
  .modal-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
  .modal-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    font-weight: 700;
    color: var(--color-text-primary);
  }
  .modal-title.modal-pass { color: var(--color-success); }
  .modal-title.modal-fail { color: var(--color-error); }
  .modal-ts { font-size: 11px; color: var(--color-text-muted); }
  .modal-output {
    background: rgba(0,0,0,0.3);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 16px;
    font-size: 12px;
    line-height: 1.5;
    overflow: auto;
    max-height: 300px;
    max-width: 600px;
    min-width: 400px;
    color: var(--color-text-secondary);
  }
  .modal-footer { display: flex; justify-content: flex-end; margin-top: 20px; }
  .modal-wide .modal-output { max-width: 700px; min-width: 500px; }
</style>
