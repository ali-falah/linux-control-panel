<script lang="ts">
  import SearchBar from '../components/ui/SearchBar.svelte';
  import FilterChips from '../components/ui/FilterChips.svelte';
  import { tableFeatures } from '../actions/tableFeatures';
  import Button from '../components/ui/Button.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Table from '../components/ui/Table.svelte';
  import CodeEditor from '../components/CodeEditor.svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import KebabMenu from '../components/KebabMenu.svelte';
  import DnfDryRunModal from '../components/DnfDryRunModal.svelte';

  import { tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { History, RefreshCw, Undo2, Calendar, Package, Search } from '@lucide/svelte';
  import { Trash2, Info, ListTree, CheckCircle, Database, XCircle } from '@lucide/svelte';
  import { AlertTriangle, Lock, Ban, Sparkles, ShieldAlert, Copy, Terminal, WrapText } from '@lucide/svelte';
  import Select from '../components/ui/Select.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { statusStore } from '../stores/status.svelte.ts';
  import { dnfStore } from '../stores/dnfStore.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';

  // ─── Types ──────────────────────────────────────────────────────────────────

  interface DnfHistoryEntry {
    id: number;
    command: string;
    date: string;
    action: string;
    altered: number;
  }

  interface DnfHistoryPackage {
    action: string;
    nevra: string;
    repo: string;
  }

  interface DnfUpdateEntry {
    package: string;
    arch: string;
    version: string;
    repo: string;
    size: string;
  }

  interface DnfLockInfo {
    locked: boolean;
    pid: number | null;
    process_name: string | null;
    lock_path: string | null;
  }

  // ─── Tab State ───────────────────────────────────────────────────────────────

  type Tab = 'updates' | 'history' | 'packages' | 'maintenance' | 'logs';
  let activeTab = $state<Tab>('updates');

  // ─── Date Formatter ─────────────────────────────────────────────────────────

  function formatToLocalDate(dateStr: string): string {
    if (!dateStr) return '—';
    let iso = dateStr.trim();
    if (!iso.includes('T')) {
      iso = iso.replace(' ', 'T');
    }
    if (!iso.endsWith('Z') && !iso.includes('+')) {
      iso += 'Z';
    }
    const d = new Date(iso);
    if (isNaN(d.getTime())) return dateStr;

    const pad = (n: number) => n.toString().padStart(2, '0');
    const year = d.getFullYear();
    const month = pad(d.getMonth() + 1);
    const day = pad(d.getDate());
    let hoursNum = d.getHours();
    const ampm = hoursNum >= 12 ? 'PM' : 'AM';
    hoursNum = hoursNum % 12 || 12;
    const hours = pad(hoursNum);
    const mins = pad(d.getMinutes());
    const secs = pad(d.getSeconds());
    return `${year}-${month}-${day} ${hours}:${mins}:${secs} ${ampm}`;
  }

  // ─── DNF System Logs ─────────────────────────────────────────────────────────

  let dnfLogContent = $state('');
  let loadingLog = $state(false);
  let historyDetail = $state<DnfHistoryEntry | null>(null);
  let historyDetailOpen = $state(false);
  let historyPackages = $state<DnfHistoryPackage[]>([]);
  let historyPackagesLoading = $state(false);
  let historyActionFilter = $state<'all' | 'install' | 'upgrade' | 'erase'>('all');

  async function openHistoryDetails(entry: DnfHistoryEntry) {
    historyDetail = entry;
    historyDetailOpen = true;
    historyPackages = [];
    historyPackagesLoading = true;
    try {
      historyPackages = await invoke<DnfHistoryPackage[]>('dnf_history_info', { id: entry.id });
    } catch (e) {
      historyPackages = [];
      uiStore.addToast(`Could not read transaction #${entry.id}: ${e}`, 'error');
    } finally {
      historyPackagesLoading = false;
    }
  }
  let logPageSize = $state(100);
  let logCurrentPage = $state(1);
  let logFollow = $state(false);
  let logFollowTimer: ReturnType<typeof setInterval> | null = null;
  let logRawView = $state(false);
  let logWrap = $state(true);

  interface DnfLogRow {
    stamp: string;
    message: string;
    level: 'error' | 'warn' | 'info';
  }

  function dnfLogLevel(message: string): 'error' | 'warn' | 'info' {
    if (/curl error|\berror\b|failed|failure|critical/i.test(message)) return 'error';
    if (/warn|timeout|retry|deprecat/i.test(message)) return 'warn';
    return 'info';
  }

  function parseDnfLogLine(line: string): DnfLogRow {
    const match = line.match(/^\[([^\]]+)\][^ ]*\s+(.*?)\s+dnf5\[\d+\]:\s*(.*)$/);
    if (match) {
      return { stamp: match[1], message: match[3], level: dnfLogLevel(match[3]) };
    }
    return { stamp: '', message: line, level: 'info' };
  }

  let logQuery = $state('');
  let logLevel = $state<'all' | 'error' | 'warn' | 'info'>('all');

  let logLines = $derived(dnfLogContent ? dnfLogContent.split('\n') : []);

  /** Search + severity filter over the log before pagination. */
  let filteredLogLines = $derived.by(() => {
    const query = logQuery.trim().toLowerCase();
    return logLines.filter(line => {
      if (!line.trim()) return false;
      const lower = line.toLowerCase();
      if (query && !lower.includes(query)) return false;
      if (logLevel === 'error' && !/error|crit|emerg|fail/.test(lower)) return false;
      if (logLevel === 'warn' && !/warn|timeout|retry|deprecat/.test(lower)) return false;
      if (logLevel === 'info' && /error|crit|emerg|fail|warn/.test(lower)) return false;
      return true;
    });
  });

  let totalLogPages = $derived(Math.max(1, Math.ceil(filteredLogLines.length / logPageSize)));

  let paginatedLogContent = $derived(() => {
    if (logLines.length === 0) return 'No log file content — /var/log/dnf.log is empty or missing.';
    if (filteredLogLines.length === 0) return 'No log lines match this filter.';
    const start = (logCurrentPage - 1) * logPageSize;
    const end = start + logPageSize;
    return filteredLogLines.slice(start, end).join('\n');
  });

  function toggleLogFollow() {
    logFollow = !logFollow;
    if (logFollow) {
      logFollowTimer = setInterval(async () => {
        await loadDnfLog();
        logCurrentPage = totalLogPages;
      }, 5000);
    } else if (logFollowTimer) {
      clearInterval(logFollowTimer);
      logFollowTimer = null;
    }
  }

  function setLogPage(p: number) {
    if (p < 1) logCurrentPage = 1;
    else if (p > totalLogPages) logCurrentPage = totalLogPages;
    else logCurrentPage = p;
  }

  async function loadDnfLog() {
    loadingLog = true;
    dnfLogContent = '';
    logCurrentPage = 1;
    statusStore.setBusy('Loading DNF log…');
    try {
      dnfLogContent = await invoke('dnf_read_log');
      statusStore.setLastCommand('cat /var/log/dnf5.log', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load DNF log: ${e}`, 'error');
      dnfLogContent = `Error: ${e}`;
      statusStore.setLastCommand('cat /var/log/dnf5.log', 1, false);
    } finally {
      loadingLog = false;
      statusStore.clearBusy();
    }
  }

  $effect(() => {
    if (activeTab === 'logs' && !dnfLogContent) loadDnfLog();
  });

  // ─── Updates State ───────────────────────────────────────────────────────────

  let updates = $state<DnfUpdateEntry[]>([]);
  let selectedUpdates = new SvelteSet<string>();
  let loadingUpdates = $state(false);

  /**
   * Identity of an update row. A package name can appear several times (one
   * row per architecture), so the architecture has to be part of the key —
   * otherwise the selection collapses the rows and the counter lies.
   */
  function updateKey(u: DnfUpdateEntry): string {
    return `${u.package}.${u.arch}`;
  }

  /** Last 40 lines of upgradeOutput — what is shown in the non-scrollable terminal */
  let terminalLines = $derived(() => {
    const lines = dnfStore.upgradeOutput.split('\n');
    return lines.slice(Math.max(0, lines.length - 40)).join('\n');
  });

  let selectAllUpdates = $derived(
    updates.length > 0 && selectedUpdates.size === updates.length
  );
  const visibleLogRows = $derived(
    paginatedLogContent()
      .split('\n')
      .filter((line, index, all) => line.trim() !== '' || (index === 0 && all.length > 1))
      .map(parseDnfLogLine)
  );

  // ─── Update list filters ────────────────────────────────────────────────────

  let updateSearch = $state('');
  let repoFilter = $state('all');
  let archFilter = $state('all');
  let securityOnly = $state(false);
  let showFailedOnly = $state(false);

  /** The repositories that offer updates, with the number of packages each. */
  let repoCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const u of updates) counts.set(u.repo, (counts.get(u.repo) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  });

  let archList = $derived([...new Set(updates.map((u) => u.arch))].sort());

  /** The updates that pass the search, repository, architecture and security filter. */
  let filteredUpdates = $derived(
    updates.filter((u) => {
      if (repoFilter !== 'all' && u.repo !== repoFilter) return false;
      if (archFilter !== 'all' && u.arch !== archFilter) return false;
      if (securityOnly && !dnfStore.isSecurityUpdate(u.package, u.arch, u.version)) return false;
      const query = updateSearch.trim().toLowerCase();
      if (query && !`${u.package} ${u.version} ${u.repo} ${u.arch}`.toLowerCase().includes(query)) {
        return false;
      }
      return true;
    })
  );

  /** The download size of every available update. */
  let totalListSize = $derived(
    formatSize(updates.reduce((acc, u) => acc + parseSize(u.size), 0))
  );

  /** How many of the available updates are covered by a security advisory. */
  let securityUpdateCount = $derived(
    updates.filter((u) => dnfStore.isSecurityUpdate(u.package, u.arch, u.version)).length
  );

  // ─── Size helpers ─────────────────────────────────────────────────────────────

  function parseSize(sizeStr: string): number {
    const units: Record<string, number> = {
      'B': 1, 'K': 1024, 'M': 1024 ** 2, 'G': 1024 ** 3,
      'KiB': 1024, 'MiB': 1024 ** 2, 'GiB': 1024 ** 3, 'TiB': 1024 ** 4,
      'k': 1024, 'm': 1024 ** 2, 'g': 1024 ** 3,
    };
    const match = sizeStr.trim().match(/^([\d.]+)\s*([a-zA-Z]+)?$/);
    if (!match) return 0;
    return parseFloat(match[1]) * (units[match[2] || 'B'] || 1);
  }

  function formatSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  let totalSelectedSize = $derived(
    formatSize(
      updates
        .filter(u => selectedUpdates.has(updateKey(u)))
        .reduce((acc, u) => acc + parseSize(u.size), 0)
    )
  );


  // A machine with many updates would render hundreds of table rows at once.
  // The list is therefore filled step by step — the selection and the totals
  // always cover every update, only the rendering is limited.
  const UPDATE_PAGE_SIZE = 100;
  let visibleUpdateCount = $state(UPDATE_PAGE_SIZE);
  let visibleUpdates = $derived(filteredUpdates.slice(0, visibleUpdateCount));
  let hiddenUpdateCount = $derived(Math.max(0, filteredUpdates.length - visibleUpdateCount));

  // ─── Selection ───────────────────────────────────────────────────────────────

  function toggleSelectAll(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    if (checked) {
      for (const u of updates) {
        selectedUpdates.add(updateKey(u));
      }
    } else {
      selectedUpdates.clear();
    }
  }

  function toggleUpdateSelection(key: string) {
    if (selectedUpdates.has(key)) {
      selectedUpdates.delete(key);
    } else {
      selectedUpdates.add(key);
    }
  }

  // ─── Check Updates ─────────────────────────────────────────────────────────

  let lastCheckedAt = $state('');

  async function checkUpdates() {
    dnfStore.resetUpgradeView();
    loadingUpdates = true;
    statusStore.setBusy('Checking for updates…');
    try {
      await refreshUpdates();
      lastCheckedAt = new Date().toLocaleTimeString();
      // The advisories decide which updates are security relevant — they are
      // read in the background, the list is usable right away.
      void dnfStore.loadAdvisories();
    } finally {
      loadingUpdates = false;
      statusStore.clearBusy();
    }
  }

  /**
   * Re-read the available updates without leaving the upgrade view — used
   * after a finished transaction so the list and the counters no longer show
   * the packages that were just upgraded.
   */
  async function refreshUpdates() {
    visibleUpdateCount = UPDATE_PAGE_SIZE;
    try {
      updates = await invoke<DnfUpdateEntry[]>('dnf_check_updates');
      uiStore.availableUpdatesCount = updates.length;
      statusStore.setLastCommand('dnf check-update', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to check updates: ${e}`, 'error');
      statusStore.setLastCommand('dnf check-update', 1, false);
      updates = [];
      uiStore.availableUpdatesCount = 0;
    }
  }

  async function startUpgrade() {
    if (selectedUpdates.size === 0) return;
    const pkgs = Array.from(selectedUpdates).map(p => p.trim()).filter(Boolean);
    await dnfStore.requestUpgrade(pkgs);
  }

  /**
   * The tooltip of an update row: the full NEVRA (so the user can see the
   * epoch) and the advisories that recommend it.
   */
  function updateTooltip(u: DnfUpdateEntry): string {
    const cleanVersion = u.version.includes(':') ? u.version.split(':').slice(1).join(':') : u.version;
    const parts = [`${u.package}-${cleanVersion}.${u.arch}`, u.repo, u.size];
    const advisories = dnfStore.advisoriesFor(u.package, u.arch, u.version);
    if (advisories.length) {
      parts.push(
        advisories
          .map((a) => `${a.id}${a.severity ? ` (${a.severity})` : ''}`)
          .join(', ')
      );
    }
    return parts.join('\n');
  }

  /** Upgrade every available update, not only the ticked rows. */
  function startUpgradeAll() {
    uiStore.confirm(
      'Update all packages',
      `This upgrades all ${updates.length} available updates (${totalListSize} to download). The full transaction is shown before anything is changed.`,
      () => dnfStore.requestUpgrade([]),
      false
    );
  }

  // ─── History ─────────────────────────────────────────────────────────────────

  let history = $state<DnfHistoryEntry[]>([]);
  let loadingHistory = $state(false);
  let undoingId = $state<number | null>(null);
  let historySearch = $state('');

  const filteredHistory = $derived(
    history.filter(h =>
      h.command.toLowerCase().includes(historySearch.toLowerCase()) ||
      h.action.toLowerCase().includes(historySearch.toLowerCase())
    ).sort((a, b) => b.id - a.id)
  );

  const historyActionFilters = [
    { id: 'all', label: 'All' },
    { id: 'install', label: 'Install' },
    { id: 'upgrade', label: 'Upgrade' },
    { id: 'erase', label: 'Erase' },
  ] as const;

  /** Apply the action filter, then group the rows by calendar day. */
  let historyGroups = $derived.by(() => {
    const list = filteredHistory.filter(entry => {
      if (historyActionFilter === 'all') return true;
      return actionLabel(entry.action).toLowerCase() === historyActionFilter;
    });
    const groups = new Map<string, DnfHistoryEntry[]>();
    for (const entry of list) {
      const day = formatToLocalDate(entry.date).slice(0, 10);
      const bucket = groups.get(day);
      if (bucket) bucket.push(entry);
      else groups.set(day, [entry]);
    }
    return [...groups.entries()];
  });

  /** dnf history stores one letter: show the verb admins actually read. */
  function actionLabel(action: string): string {
    switch ((action || '').toUpperCase()) {
      case 'I': return 'Install';
      case 'U': return 'Upgrade';
      case 'E': return 'Erase';
      case 'R': return 'Reinstall';
      case 'D': return 'Downgrade';
      default: return action || 'Transaction';
    }
  }

  function actionBadge(action: string): string {
    // `dnf history` reports one letter; map that before the word forms.
    switch ((action || '').toUpperCase()) {
      case 'I': return 'badge-success';
      case 'U': return 'badge-info';
      case 'E': return 'badge-error';
      case 'D': return 'badge-warning';
      case 'R': return 'badge-info';
    }
    const a = action.toLowerCase();
    if (a.includes('install')) return 'badge-success';
    if (a.includes('remove') || a.includes('erase')) return 'badge-error';
    if (a.includes('update') || a.includes('upgrade')) return 'badge-info';
    if (a.includes('downgrade')) return 'badge-warning';
    return 'badge-muted';
  }

  async function loadHistory() {
    loadingHistory = true;
    statusStore.setBusy('Loading DNF history…');
    try {
      history = await invoke<DnfHistoryEntry[]>('list_dnf_history');
      statusStore.setLastCommand('dnf history list', 0, true);
    } catch (e) {
      uiStore.addToast(`Failed to load history: ${e}`, 'error');
      statusStore.setLastCommand('dnf history list', 1, false);
    } finally {
      loadingHistory = false;
      statusStore.clearBusy();
    }
  }

  function confirmUndo(entry: DnfHistoryEntry) {
    uiStore.confirm(
      'Rollback Transaction',
      `Are you sure you want to undo transaction #${entry.id}?\n\n"${entry.command || entry.action}" — ${entry.date}\n\nThis will ${entry.action.toLowerCase()} ${entry.altered} package(s). A polkit password prompt will appear.`,
      () => doUndo(entry.id),
      true
    );
  }

  async function doUndo(id: number, confirmed = false) {
    undoingId = id;
    statusStore.setBusy(`Undoing transaction #${id}…`);
    try {
      await invoke('undo_transaction', { id, confirmed });
      uiStore.addToast(`Successfully rolled back transaction #${id}`, 'success');
      statusStore.setLastCommand(`dnf history undo ${id} -y`, 0, true);
      loadHistory();
    } catch (e) {
      const message = String(e);
      if (message.startsWith('CONFIRMATION_REQUIRED:')) {
        // The helper's preview found the running kernel or a protected
        // package. Show its full package/risk list verbatim and retry the
        // same rollback with the explicit confirmation value.
        uiStore.confirm(
          'Rollback Requires Confirmation',
          message,
          () => doUndo(id, true),
          true
        );
        return;
      }
      uiStore.addToast(`Rollback failed: ${e}`, 'error');
      statusStore.setLastCommand(`dnf history undo ${id} -y`, 1, false);
    } finally {
      undoingId = null;
      statusStore.clearBusy();
    }
  }

  // ─── Find Packages ────────────────────────────────────────────────────────────

  let pkgQuery = $state('');
  let pkgOutput = $state('');
  let pkgLoading = $state(false);
  let pkgMode = $state<'search' | 'info' | 'versions'>('search');
  let pkgRaw = $state(false);

  interface PkgResult {
    name: string;
    arch: string;
    version: string;
    repo: string;
    summary: string;
    installed?: boolean;
  }

  interface DnfSearchResult extends PkgResult {
    installed: boolean;
  }

  let pkgSearchResults = $state<DnfSearchResult[]>([]);

  /** Turn dnf's human output into rows; the raw text stays one click away. */
  const pkgResults = $derived.by((): PkgResult[] => {
    const results: PkgResult[] = [];
    if (!pkgOutput || pkgOutput.startsWith('Error:')) return results;

    if (pkgMode === 'search') {
      for (const line of pkgOutput.split('\n')) {
        // dnf search puts two or more spaces (or a colon) between name.arch
        // and the summary; the summary itself contains single spaces.
        const match = line.match(/^\s*(\S+?)\.([a-z0-9_]+)(?:\s*:\s*|\s{2,})(.+)$/i);
        if (!match) continue;
        results.push({ name: match[1], arch: match[2], version: '', repo: '', summary: match[3].trim() });
      }
    } else if (pkgMode === 'versions') {
      for (const line of pkgOutput.split('\n')) {
        const cols = line.trim().split(/\s+/);
        if (cols.length < 3) continue;
        const nameArch = cols[0].match(/^(.+?)\.([a-z0-9_]+)$/i);
        if (!nameArch) continue;
        results.push({
          name: nameArch[1],
          arch: nameArch[2],
          version: cols[1],
          repo: cols[2],
          summary: '',
        });
      }
    } else {
      let current: Partial<PkgResult> | null = null;
      const commit = () => {
        if (current?.name) {
          results.push({ name: current.name, arch: current.arch ?? '', version: current.version ?? '', repo: current.repo ?? '', summary: current.summary ?? '' });
        }
        current = null;
      };
      for (const line of pkgOutput.split('\n')) {
        const trimmed = line.trim();
        if (/^(Installed|Available) Packages$/.test(trimmed)) { commit(); continue; }
        if (trimmed.startsWith('Name') && trimmed.includes(':')) {
          commit();
          current = { name: trimmed.split(':')[1].trim() };
        } else if (current && trimmed.startsWith('Version') && trimmed.includes(':')) {
          current.version = trimmed.split(':')[1].trim();
        } else if (current && trimmed.startsWith('Release') && trimmed.includes(':')) {
          current.version = `${current.version ?? ''}-${trimmed.split(':')[1].trim()}`;
        } else if (current && trimmed.startsWith('Architecture') && trimmed.includes(':')) {
          current.arch = trimmed.split(':')[1].trim();
        } else if (current && trimmed.startsWith('Repository') && trimmed.includes(':')) {
          current.repo = trimmed.split(':')[1].trim();
        } else if (current && trimmed.startsWith('Summary') && trimmed.includes(':')) {
          current.summary = trimmed.split(':').slice(1).join(':').trim();
        }
      }
      commit();
    }
    return results;
  });

  const pkgRows = $derived<PkgResult[]>(
    pkgMode === 'search' && pkgSearchResults.length > 0 ? pkgSearchResults : pkgResults
  );

  async function runPkgCmd(cmd: string) {
    if (!pkgQuery.trim()) return;
    pkgLoading = true;
    pkgOutput = '';
    statusStore.setBusy(`Running dnf ${cmd}…`);
    try {
      pkgMode = cmd as typeof pkgMode;
      pkgRaw = false;
      if (cmd === 'search') {
        try {
          pkgSearchResults = await invoke<DnfSearchResult[]>('dnf_search_packages_structured', {
            query: pkgQuery,
          });
        } catch {
          // repoquery is the richer path; fall back to plain search text.
          pkgSearchResults = [];
        }
        if (pkgSearchResults.length > 0) {
          pkgOutput = pkgSearchResults
            .map(row => `${row.name}.${row.arch}  ${row.version}  ${row.repo} — ${row.summary}`)
            .join('\n');
          statusStore.setLastCommand(`dnf repoquery --available ${pkgQuery}`, 0, true);
        } else {
          pkgOutput = await invoke('dnf_search_packages', { query: pkgQuery });
          statusStore.setLastCommand(`dnf search ${pkgQuery}`, 0, true);
        }
      } else if (cmd === 'info') {
        pkgSearchResults = [];
        pkgOutput = await invoke('dnf_package_info', { pkg: pkgQuery });
        statusStore.setLastCommand(`dnf info ${pkgQuery}`, 0, true);
      } else if (cmd === 'versions') {
        pkgSearchResults = [];
        pkgOutput = await invoke('dnf_list_versions', { pkg: pkgQuery });
        statusStore.setLastCommand(`dnf list --showduplicates ${pkgQuery}`, 0, true);
      }
    } catch (e) {
      pkgOutput = `Error: ${e}`;
      statusStore.setLastCommand(`dnf ${cmd}`, 1, false);
    } finally {
      pkgLoading = false;
      statusStore.clearBusy();
    }
  }

  // ─── Maintenance ─────────────────────────────────────────────────────────────

  let maintRunning = $state(false);
  let maintOutput = $state('');
  let maintLast = $state<{ name: string; at: string; ok: boolean } | null>(null);

  // Lock status
  let lockInfo = $state<DnfLockInfo | null>(null);
  let lockLoading = $state(false);
  let lockKillRunning = $state(false);

  async function checkLockStatus() {
    lockLoading = true;
    try {
      lockInfo = await invoke<DnfLockInfo>('dnf_check_lock_status');
    } catch (e) {
      lockInfo = null;
    } finally {
      lockLoading = false;
    }
  }

  async function killLock() {
    uiStore.confirm(
      'Remove Stale DNF Lock',
      'This will remove the stale DNF lock file.\n\nOnly proceed if you are CERTAIN no DNF process is running. Incorrect use may affect the package database.\n\ndnf check will run automatically after removal to verify integrity.',
      async () => {
        lockKillRunning = true;
        try {
          const result = await invoke<string>('dnf_kill_lock');
          uiStore.addToast('Lock removed. Running dnf check…', 'info');
          maintOutput = result;
          // Run integrity check automatically
          const checkOut = await invoke<string>('dnf_check');
          maintOutput += '\n\n--- dnf check output ---\n' + (checkOut || 'No issues found.');
          uiStore.addToast('Database integrity check complete.', 'success');
          await checkLockStatus();
        } catch (e) {
          uiStore.addToast(`Failed to remove lock: ${e}`, 'error');
          maintOutput = `Error: ${e}`;
        } finally {
          lockKillRunning = false;
        }
      },
      true
    );
  }

  function confirmMaintenance(cmdName: string, tauriCmd: string, warning: string) {
    uiStore.confirm(
      `Confirm ${cmdName}`,
      warning,
      () => runMaintenance(cmdName, tauriCmd),
      true
    );
  }

  async function runMaintenance(cmdName: string, tauriCmd: string) {
    maintRunning = true;
    maintOutput = `Running ${cmdName}…\nThis may take a moment.`;
    statusStore.setBusy(`Running ${cmdName}…`);
    try {
      const out = await invoke<string>(tauriCmd);
      maintLast = { name: cmdName, at: new Date().toLocaleTimeString(), ok: true };
      uiStore.addToast(`${cmdName} completed successfully`, 'success');
      statusStore.setLastCommand(cmdName, 0, true);
      maintOutput = out || 'Command completed with no output.';
    } catch (e) {
      maintLast = { name: cmdName, at: new Date().toLocaleTimeString(), ok: false };
      uiStore.addToast(`${cmdName} failed: ${e}`, 'error');
      statusStore.setLastCommand(cmdName, 1, false);
      maintOutput = `Error: ${e}`;
    } finally {
      maintRunning = false;
      statusStore.clearBusy();
    }
  }

  $effect(() => {
    if (activeTab !== 'logs' && logFollowTimer) {
      clearInterval(logFollowTimer);
      logFollowTimer = null;
      logFollow = false;
    }
  });

  // ─── Init ────────────────────────────────────────────────────────────────────

  $effect(() => {
    loadHistory();
    checkUpdates();
  });

  // After a finished transaction the list still shows the packages that were
  // just upgraded (and the counters still count them) — refresh it so the
  // page reflects what is actually still pending.
  $effect(() => {
    if (dnfStore.upgradeFinished) refreshUpdates();
  });

  $effect(() => {
    if (activeTab === 'maintenance') checkLockStatus();
  });
</script>

<div class="module-page">
  <PageHeader title="Packages & Updates" subtitle="Manage packages, view history, and perform system upgrades" icon={Package}>
    {#if activeTab === 'history'}
      <button class="header-refresh-btn" onclick={loadHistory} disabled={loadingHistory}>
        <RefreshCw size={13} class={loadingHistory ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    {:else if activeTab === 'updates'}
      {#if lastCheckedAt}
        <span class="check-age">checked {lastCheckedAt}</span>
      {/if}
      <Button variant="outline" size="sm" onclick={checkUpdates} disabled={loadingUpdates}>
        <RefreshCw size={13} class={loadingUpdates ? 'animate-spin-slow' : ''} /> Check
      </Button>
    {:else if activeTab === 'packages'}
      <Button variant="primary" size="sm" onclick={() => runPkgCmd('search')} disabled={!pkgQuery || pkgLoading}>
        <Search size={13} /> Search
      </Button>
      <Button variant="outline" size="sm" onclick={() => runPkgCmd('info')} disabled={!pkgQuery || pkgLoading}>
        <Info size={13} /> Info
      </Button>
      <Button variant="outline" size="sm" onclick={() => runPkgCmd('versions')} disabled={!pkgQuery || pkgLoading}>
        <ListTree size={13} /> Versions
      </Button>
    {:else if activeTab === 'logs'}
      <button class="header-refresh-btn" onclick={loadDnfLog} disabled={loadingLog}>
        <RefreshCw size={13} class={loadingLog ? 'animate-spin-slow' : ''} /> Refresh
      </button>
    {:else if activeTab === 'maintenance'}
      <Button variant="outline" size="sm" onclick={checkLockStatus} disabled={lockLoading}>
        <RefreshCw size={13} class={lockLoading ? 'animate-spin-slow' : ''} /> Check Lock
      </Button>
    {/if}
  </PageHeader>

  <!-- Tab bar + actions row -->
  <div class="controls-row">
    <div class="tab-bar">
      {#each [['updates','Updates'],['history','Transaction History'],['packages','Find Packages'],['maintenance','Maintenance'],['logs','System Package Logs']] as [id, label]}
        <button class="tab-btn {activeTab === id ? 'active' : ''}" onclick={() => activeTab = id as Tab}>
          {label}
        </button>
      {/each}
    </div>

    <div class="tab-actions">
      {#if activeTab === 'history'}
        <SearchBar bind:value={historySearch} placeholder="Search history by command or action…" style="margin: 0; max-width: 320px;" />
        <FilterChips
          options={[...historyActionFilters]}
          bind:value={historyActionFilter}
          label="Filter transactions by action"
        />
      {:else if activeTab === 'updates'}
        {#if updates.length > 0}
          <Button variant="outline" size="sm" onclick={startUpgradeAll} disabled={dnfStore.isUpgrading}>
            <RefreshCw size={13} class={dnfStore.isUpgrading ? 'animate-spin-slow' : ''} />
            Update All
          </Button>
          <Button variant="primary" size="sm" onclick={startUpgrade} disabled={selectedUpdates.size === 0 || dnfStore.isUpgrading}>
            <RefreshCw size={13} class={dnfStore.isUpgrading ? 'animate-spin-slow' : ''} />
            Update {selectedUpdates.size} Package{selectedUpdates.size !== 1 ? 's' : ''} ({totalSelectedSize})
          </Button>
        {/if}
      {:else if activeTab === 'packages'}
        <SearchBar bind:value={pkgQuery} placeholder="Enter package name (e.g. htop)..." style="margin:0; width: 250px;" />
      {:else if activeTab === 'logs'}
        <span style="font-size:13px; color:var(--color-text-secondary);">reads /var/log/dnf.log</span>
      {/if}
    </div>
  </div>

  <!-- ── Updates Tab ─────────────────────────────────────────────────────────── -->
  {#if activeTab === 'updates'}
    <!-- Search, repository, architecture and security filter -->
    <div class="update-filters">
      <SearchBar
        bind:value={updateSearch}
        placeholder="Filter by package, version or repository…"
      />

      <Select bind:value={archFilter} size="sm" aria-label="Filter by architecture">
        <option value="all">All architectures</option>
        {#each archList as arch}
          <option value={arch}>{arch}</option>
        {/each}
      </Select>

      <button
        type="button"
        class="filter-toggle"
        class:active={securityOnly}
        onclick={() => (securityOnly = !securityOnly)}
        title="Show only updates that are covered by a security advisory"
      >
        <ShieldAlert size={13} />
        Security only
        {#if securityUpdateCount > 0}
          <span class="filter-count">{securityUpdateCount}</span>
        {/if}
      </button>

      <FilterChips
        options={[
          { id: 'all', label: 'All', count: updates.length },
          ...repoCounts.map(([repo, count]) => ({ id: repo, label: repo, count })),
        ]}
        bind:value={repoFilter}
        label="Filter updates by repository"
      />
    </div>

    <div class="card" style="display:flex; flex-direction:column; padding: 0; flex: 1; min-height: 0;">

      <!-- External Lock Warning Banner -->
      {#if dnfStore.lockInfo?.locked && !dnfStore.isUpgrading}
        <div style="padding: 12px 16px; background: rgba(245, 158, 11, 0.15); border-bottom: 1px solid rgba(245, 158, 11, 0.3); display:flex; align-items:center; justify-content:space-between;">
          <div style="display:flex; align-items:center; gap:8px; color:var(--color-warning); font-size:13px; font-weight:600;">
            <Lock size={15} />
            <span>DNF is locked by process '{dnfStore.lockInfo.process_name || 'unknown'}' (PID {dnfStore.lockInfo.pid}). Another transaction is in progress.</span>
          </div>
          <Button variant="outline" size="sm" onclick={() => activeTab = 'maintenance'}>
            Go to Maintenance
          </Button>
        </div>
      {/if}

      {#if dnfStore.isUpgrading || dnfStore.upgradeFinished}
        <!-- Terminal view (non-scrollable during upgrade) -->
        <div style="padding: 16px; display:flex; flex-direction:column; gap:12px; flex:1; min-height:0;">
          <div style="display:flex; align-items:center; justify-content:space-between; gap:12px;">
            <h3 style="margin:0; font-size:15px; font-weight:600; display:flex; align-items:center; gap:8px;">
              {#if dnfStore.isUpgrading}
                <RefreshCw size={15} class="animate-spin-slow" style="color:var(--color-accent)" />
                Upgrading Packages…
              {:else if dnfStore.upgradeSuccess}
                <CheckCircle size={15} style="color:var(--color-success)" />
                Upgrade Complete
              {:else}
                <XCircle size={15} style="color:var(--color-error)" />
                Upgrade Failed
              {/if}
            </h3>
            <div style="display:flex; align-items:center; gap:8px;">
              {#if dnfStore.isUpgrading}
                <Button
                  variant="outline"
                  size="sm"
                  onclick={() => dnfStore.cancelUpgrade()}
                  title="Closing the view does not stop dnf; this asks the process to terminate, which may not reach it through the privilege layer."
                >
                  <Ban size={13} /> Stop watching
                </Button>
              {:else}
                {#if aiStore.enabled}
                  <Button variant="outline" size="sm" onclick={() => aiStore.explainDnfConflict(terminalLines())} title="Analyze upgrade errors with AI">
                    <Sparkles size={13} style="color:var(--color-accent);" /> AI Explain Conflict
                  </Button>
                {/if}
                <Button
                  variant="outline"
                  size="sm"
                  onclick={() => { dnfStore.resetUpgradeView(); activeTab = 'history'; loadHistory(); }}
                >
                  <History size={13} /> View in History
                </Button>
                <Button variant="outline" size="sm" onclick={() => { dnfStore.resetUpgradeView(); checkUpdates(); }}>
                  Back to Package List
                </Button>
              {/if}
            </div>
          </div>

          <!-- Hang warning banner -->
          {#if dnfStore.hangWarning}
            <div class="hang-warning">
              <AlertTriangle size={14} />
              <span>No output for 60+ seconds — the upgrade may be downloading large packages or waiting on a slow mirror. You can stop watching or wait.</span>
            </div>
          {/if}

          <!-- Summary of the finished transaction -->
          {#if dnfStore.upgradeSummary}
            <div class="upgrade-summary">
              <div class="summary-item">
                <span class="summary-value">{dnfStore.upgradeSummary.applied}</span>
                <span class="summary-label">packages changed</span>
              </div>
              <div class="summary-item">
                <span class="summary-value">{dnfStore.upgradeSummary.upgrading}</span>
                <span class="summary-label">upgraded</span>
              </div>
              <div class="summary-item">
                <span class="summary-value">{dnfStore.upgradeSummary.skipped}</span>
                <span class="summary-label">skipped</span>
              </div>
              <div class="summary-item" class:has-errors={dnfStore.upgradeSummary.errors.length > 0}>
                <span class="summary-value">{dnfStore.upgradeSummary.errors.length}</span>
                <span class="summary-label">errors</span>
              </div>
              <div class="summary-spacer"></div>
              {#if dnfStore.upgradeSummary.errors.length > 0}
                <button
                  type="button"
                  class="summary-filter"
                  class:active={showFailedOnly}
                  onclick={() => (showFailedOnly = !showFailedOnly)}
                >
                  {showFailedOnly ? 'Show full log' : `Show failed only (${dnfStore.upgradeSummary.errors.length})`}
                </button>
              {/if}
            </div>
          {/if}

          <!-- Non-scrollable terminal: always shows last 40 lines -->
          <div class="upgrade-terminal">
            {#if showFailedOnly && (dnfStore.upgradeSummary?.errors.length ?? 0) > 0}
              {#each dnfStore.upgradeSummary?.errors ?? [] as line}
                <div class="error-line">{line}</div>
              {/each}
            {:else}
              {terminalLines()}
            {/if}
          </div>

          <!-- Post-upgrade: full scrollable log -->
          {#if dnfStore.upgradeFinished}
            <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
              <div style="padding:8px 14px; background:rgba(0,0,0,0.2); border-bottom:1px solid var(--color-border); font-size:11px; font-weight:600; color:var(--color-text-muted); text-transform:uppercase; letter-spacing:0.05em;">
                Full Upgrade Log
              </div>
              <CodeEditor value={dnfStore.upgradeOutput} readonly={true} height="100%" />
            </div>
          {/if}
        </div>

      {:else}
        <!-- Package list view -->
        <div style="flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden;">
          {#if loadingUpdates && updates.length === 0}
            <div style="padding:48px 32px; display:flex; flex-direction:column; align-items:center; gap:16px; color:var(--color-text-muted);">
              <RefreshCw size={24} class="animate-spin-slow" />
              <span>Checking for updates...</span>
            </div>
          {:else if updates.length === 0}
            <div class="empty-state" style="padding: 64px 32px;">
              <CheckCircle size={32} class="empty-state-icon" style="color:var(--color-success)" />
              <span style="font-size:16px; font-weight:600;">System is up to date</span>
            </div>
          {:else}
            <Table tableAction={tableFeatures} style="border:none; border-radius:0;">
              <thead>
                <tr>
                  <th style="width:40px; text-align:center;">
                    <input type="checkbox" checked={selectAllUpdates} onchange={toggleSelectAll} />
                  </th>
                  <th>Package</th>
                  <th>Version</th>
                  <th>Size</th>
                  <th>Arch</th>
                  <th>Repository</th>
                </tr>
              </thead>
              <tbody>
                {#each visibleUpdates as pkg}
                  <tr onclick={() => toggleUpdateSelection(updateKey(pkg))} style="cursor:pointer;" class:row-selected={selectedUpdates.has(updateKey(pkg))}>
                    <td style="text-align:center;" onclick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={selectedUpdates.has(updateKey(pkg))}
                        onchange={() => toggleUpdateSelection(updateKey(pkg))}
                      />
                    </td>
                    <td style="font-weight:500;">
                      <span title={updateTooltip(pkg)}>{pkg.package}</span>
                      {#if dnfStore.advisoriesFor(pkg.package, pkg.arch, pkg.version).length > 0}
                        {@const advisory = dnfStore.advisoriesFor(pkg.package, pkg.arch, pkg.version)[0]}
                        {@const advisoryLabel = advisory.kind === 'security'
                          ? 'Security'
                          : advisory.kind === 'bugfix'
                            ? 'Bugfix'
                            : advisory.kind === 'enhancement'
                              ? 'Enhancement'
                              : ''}
                        {#if advisoryLabel}
                          <span
                            class="badge {advisory.kind === 'security' ? 'badge-error' : 'badge-muted'}"
                            title={`${advisory.id}${advisory.severity ? ` (${advisory.severity})` : ''}`}
                          >
                            <ShieldAlert size={10} />
                            {advisoryLabel}
                          </span>
                        {/if}
                      {/if}
                    </td>
                    <td style="font-family:var(--font-mono); font-size:12px; white-space:nowrap;">{pkg.version}</td>
                    <td style="font-size:12px; font-weight:500; white-space:nowrap;">{pkg.size}</td>
                    <td style="color:var(--color-text-secondary);">{pkg.arch}</td>
                    <td><span class="badge badge-muted">{pkg.repo}</span></td>
                  </tr>
                {/each}
                {#if hiddenUpdateCount > 0}
                  <tr>
                    <td colspan="6" style="text-align:center; padding: 12px;">
                      <Button variant="outline" size="sm" onclick={() => visibleUpdateCount += UPDATE_PAGE_SIZE}>
                        Show {Math.min(hiddenUpdateCount, UPDATE_PAGE_SIZE)} more ({hiddenUpdateCount} hidden)
                      </Button>
                    </td>
                  </tr>
                {/if}
              </tbody>
            </Table>
          {/if}
        </div>
      {/if}
    </div>

  <!-- ── History Tab ────────────────────────────────────────────────────────── -->
  {:else if activeTab === 'history'}
    <div class="card" style="padding:0; display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden;">
      {#if loadingHistory && history.length === 0}
        <div style="padding:48px 32px; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:16px; color:var(--color-text-muted);">
          <RefreshCw size={24} class="animate-spin-slow" style="color:var(--color-accent)" />
          <span style="font-weight:500">Loading DNF history…</span>
        </div>
      {:else if historyGroups.length === 0}
        <div class="empty-state" style="padding: 64px 32px;">
          <History size={32} class="empty-state-icon" style="margin:0 auto 16px;" />
          <span style="font-size:16px; font-weight:600; color:var(--color-text-primary)">No History Found</span>
          <span style="color:var(--color-text-muted); margin-top:8px;">
            {historySearch ? 'No history matches your search.' : 'No DNF history found.'}
          </span>
        </div>
      {:else}
        <Table tableAction={tableFeatures} style="border:none; border-radius:0;">
          <thead>
            <tr>
              <th style="width:60px">ID</th>
              <th>Command</th>
              <th>Date &amp; Time</th>
              <th>Action</th>
              <th style="text-align:right">Altered</th>
              <th style="text-align:right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each historyGroups as [day, dayEntries] (day)}
              <tr class="history-day-row">
                <td colspan="6">{day}</td>
              </tr>
              {#each dayEntries as entry (entry.id)}
              <tr>
                <td><code style="font-size:11px; color:var(--color-text-accent)">#{entry.id}</code></td>
                <td><div style="font-weight:500; font-family:var(--font-mono); font-size:12px;">{entry.command || '—'}</div></td>
                <td>
                  <div style="display:flex; align-items:center; gap:6px; font-size:12px; color:var(--color-text-secondary);">
                    <Calendar size={12} /> {formatToLocalDate(entry.date)}
                  </div>
                </td>
                <td><span class="badge {actionBadge(entry.action)}" title={entry.action}>{actionLabel(entry.action)}</span></td>
                <td style="text-align:right; font-weight:500;">{entry.altered}</td>
                <td style="text-align:right">
                  <KebabMenu>
                    <button
                      class="menu-item"
                      onclick={(e) => { e.stopPropagation(); openHistoryDetails(entry); }}
                    >
                      <Info size={14} /> Transaction details
                    </button>
                    <button class="menu-item danger"
                      onclick={(e) => { e.stopPropagation(); confirmUndo(entry); }}
                      disabled={undoingId === entry.id}
                    >
                      {#if undoingId === entry.id}
                        <RefreshCw size={14} class="animate-spin-slow" /> Rolling back…
                      {:else}
                        <Undo2 size={14} /> Undo Transaction
                      {/if}
                    </button>
                  </KebabMenu>
                </td>
              </tr>
              {/each}
            {/each}
          </tbody>
        </Table>
      {/if}
    </div>

  <!-- ── Find Packages Tab ──────────────────────────────────────────────────── -->
  {:else if activeTab === 'packages'}
    <div class="card" style="display:flex; flex-direction:column; gap:12px; flex:1; min-height:0; overflow:hidden;">
      {#if pkgOutput.startsWith('Error:')}
        <div class="pkg-error" role="alert">
          <AlertTriangle size={15} />
          <span>{pkgOutput}</span>
        </div>
      {:else if pkgLoading}
        <div class="center-state"><div class="spinner"></div></div>
      {:else if pkgRows.length > 0 && !pkgRaw}
        <div class="pkg-results-bar">
          <span>{pkgRows.length} package{pkgRows.length === 1 ? '' : 's'}</span>
          <Button variant="ghost" size="xs" onclick={() => (pkgRaw = true)}>
            <Terminal size={12} /> Raw output
          </Button>
        </div>
        <div style="flex:1; min-height:0; overflow:auto;">
          <Table actionParams={{ sortable: false }}>
            <thead>
              <tr>
                <th style="width:26%">Package</th>
                <th style="width:10%">Arch</th>
                <th style="width:18%">Version</th>
                {#if pkgMode === 'search'}
                  <th style="width:10%">Installed</th>
                {/if}
                <th style="width:{pkgMode === 'search' ? '36%' : '38%'}">{pkgMode === 'versions' ? 'Repository' : 'Summary / Repository'}</th>
              </tr>
            </thead>
            <tbody>
              {#each pkgRows as pkg}
                <tr>
                  <td style="font-weight:600; font-family:var(--font-mono); font-size:12px;">
                    {pkg.name}
                    <button
                      type="button"
                      class="pkg-copy"
                      title="Copy package name"
                      onclick={() => { navigator.clipboard.writeText(pkg.name); uiStore.addToast(`Copied ${pkg.name}`, 'info', 2000); }}
                    >
                      <Copy size={11} />
                    </button>
                  </td>
                  <td style="color:var(--color-text-secondary);">{pkg.arch || '—'}</td>
                  <td style="font-family:var(--font-mono); font-size:11.5px;">{pkg.version || '—'}</td>
                  {#if pkgMode === 'search'}
                    <td>
                      {#if pkg.installed}
                        <span class="badge badge-success">Installed</span>
                      {:else}
                        <span class="badge badge-muted">Not installed</span>
                      {/if}
                    </td>
                  {/if}
                  <td style="color:var(--color-text-secondary);">{pkgMode === 'versions' ? (pkg.repo || '—') : (pkg.summary || pkg.repo || '—')}</td>
                </tr>
              {/each}
            </tbody>
          </Table>
        </div>
      {:else if !pkgOutput}
        <EmptyState
          icon={Search}
          title="Find a package"
          description="Type a name above and choose Search for matching packages, Info for details, or Versions for every available build."
          actionLabel="Try btop"
          onAction={() => { pkgQuery = 'btop'; runPkgCmd('search'); }}
        />
      {:else}
        {#if pkgRows.length > 0 && pkgRaw}
          <div class="pkg-results-bar">
            <span>raw dnf output</span>
            <Button variant="ghost" size="xs" onclick={() => (pkgRaw = false)}>Parsed view</Button>
          </div>
        {/if}
        <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
          <CodeEditor value={pkgOutput} readonly={true} height="100%" />
        </div>
      {/if}
    </div>

  <!-- ── Maintenance Tab ────────────────────────────────────────────────────── -->
  {:else if activeTab === 'maintenance'}
    <div class="card module-content-scroll" style="display:flex; flex-direction:column; gap:16px;">

      <!-- DNF Lock Status card -->
      <div class="maint-card {lockInfo?.locked ? 'maint-card-danger' : lockInfo?.lock_path ? 'maint-card-warning' : 'maint-card-neutral'}">
        <div class="maint-card-icon {lockInfo?.locked ? 'icon-danger' : lockInfo?.lock_path ? 'icon-warning' : 'icon-success'}">
          <Lock size={22} />
        </div>
        <div class="maint-card-body">
          <div class="maint-card-title">DNF Lock Status</div>
          {#if lockLoading}
            <div class="maint-card-desc">Checking lock status…</div>
          {:else if lockInfo?.locked}
            <div class="maint-card-desc" style="color:var(--color-error);">
              ⚠ DNF is locked by <strong>{lockInfo.process_name}</strong> (PID {lockInfo.pid})
              at <code style="font-size:11px">{lockInfo.lock_path}</code>
            </div>
            <div class="maint-card-desc">Another package operation is running. Wait for it to complete.</div>
          {:else if lockInfo?.lock_path}
            <div class="maint-card-desc" style="color:var(--color-warning);">
              Stale lock file found at <code style="font-size:11px">{lockInfo.lock_path}</code> — owning process is no longer running.
            </div>
            <Button variant="outline" size="sm" class="btn-warning" onclick={killLock} disabled={lockKillRunning || !lockInfo}>
              {#if lockKillRunning}
                <RefreshCw size={13} class="animate-spin-slow" /> Removing…
              {:else}
                <Trash2 size={13} /> Remove Stale Lock
              {/if}
            </Button>
          {:else}
            <div class="maint-card-desc">No lock — DNF is free to run.</div>
          {/if}
        </div>
      </div>

      <!-- Maintenance actions -->
      <div style="display:flex; flex-direction:column; gap:12px;">

        <!-- Clear Cache -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-danger">
            <Trash2 size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Clear DNF Cache</div>
            <div class="maint-card-desc">Removes all cached repository data and downloaded packages. Resolves "metadata doesn't match" errors.</div>
            <Button variant="outline" size="sm" class="btn-danger"
              onclick={() => confirmMaintenance('dnf clean all', 'dnf_clean_all', 'Are you sure you want to clear the DNF cache?\n\nThis will remove all cached repository data and downloaded packages. A polkit password prompt will appear.')}
              disabled={maintRunning}>
              <Trash2 size={13} /> Run dnf clean all
            </Button>
          </div>
        </div>

        <!-- Autoremove -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-warning">
            <Package size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Autoremove Unused Packages</div>
            <div class="maint-card-desc">Removes packages installed as dependencies but no longer needed. Review the output carefully before confirming.</div>
            <Button variant="outline" size="sm" class="btn-warning"
              onclick={() => confirmMaintenance('dnf autoremove', 'dnf_autoremove', 'Are you sure you want to autoremove unused packages?\n\nWARNING: This can sometimes remove critical packages if dependencies were mismanaged. Review the output carefully. A polkit password prompt will appear.')}
              disabled={maintRunning}>
              <Package size={13} /> Run dnf autoremove
            </Button>
          </div>
        </div>

        <!-- Check Health -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-success">
            <CheckCircle size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Check System Health</div>
            <div class="maint-card-desc">Checks the local RPM database for problems — duplicates, broken dependencies, and missing files.</div>
            <Button variant="outline" size="sm" onclick={() => runMaintenance('dnf check', 'dnf_check')} disabled={maintRunning}>
              <CheckCircle size={13} /> Run dnf check
            </Button>
          </div>
        </div>

        <!-- Makecache -->
        <div class="maint-card maint-card-neutral">
          <div class="maint-card-icon icon-info">
            <Database size={22} />
          </div>
          <div class="maint-card-body">
            <div class="maint-card-title">Refresh Metadata (Makecache)</div>
            <div class="maint-card-desc">Forces DNF to connect to repositories and download the latest package lists and metadata.</div>
            <Button variant="outline" size="sm" onclick={() => runMaintenance('dnf makecache', 'dnf_makecache_cmd')} disabled={maintRunning}>
              <Database size={13} /> Run dnf makecache
            </Button>
          </div>
        </div>

      </div>

      {#if maintOutput}
        {#if maintLast}
          <div class="maint-last-run">
            <span>Last run: <strong>{maintLast.name}</strong> at {maintLast.at}</span>
            <span class="badge {maintLast.ok ? 'badge-success' : 'badge-error'}">{maintLast.ok ? 'Succeeded' : 'Failed'}</span>
          </div>
        {/if}
        <div style="display:flex; flex-direction:column; height:280px; border:1px solid var(--color-border); border-radius:8px; overflow:hidden;">
          <CodeEditor value={maintOutput} readonly={true} height="100%" />
        </div>
      {/if}
    </div>

  <!-- ── DNF Logs Tab ───────────────────────────────────────────────────────── -->
  {:else if activeTab === 'logs'}
    <div class="dnf-logs-tab">
      <div class="dnf-log-controls">
        <SearchBar
          bind:value={logQuery}
          count={filteredLogLines.length}
          total={logLines.length}
          placeholder="Filter log lines…"
          style="margin:0; flex:1 1 280px; max-width:460px;"
        />
        <FilterChips
          options={[
            { id: 'all', label: 'All', count: logLines.filter(line => line.trim()).length },
            { id: 'error', label: 'Errors' },
            { id: 'warn', label: 'Warnings' },
            { id: 'info', label: 'Info' },
          ]}
          bind:value={logLevel}
          label="Filter log lines by severity"
        />
        <div class="dnf-log-actions">
          <Button variant={logRawView ? 'primary' : 'outline'} size="sm" onclick={() => (logRawView = !logRawView)}>
            {logRawView ? 'Raw' : 'Rows'}
          </Button>
          {#if !logRawView}
            <button
              type="button"
              class="dnf-log-icon-btn"
              class:active={logWrap}
              onclick={() => (logWrap = !logWrap)}
              title={logWrap ? 'Wrap long lines (on)' : 'Wrap long lines (off)'}
              aria-label="Toggle line wrapping"
              aria-pressed={logWrap}
            >
              <WrapText size={14} />
            </button>
          {/if}
          <Button variant={logFollow ? 'primary' : 'outline'} size="sm" onclick={toggleLogFollow}>
            {logFollow ? 'Following' : 'Follow live'}
          </Button>
        </div>
      </div>

      {#if logRawView}
        <div style="display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--color-border); border-radius:10px; overflow:hidden;">
          <CodeEditor value={paginatedLogContent()} readonly={true} height="100%" />
        </div>
      {:else}
        <div class="dnf-log-view" class:wrap={logWrap}>
          {#each visibleLogRows as row, index}
            <div class="dnf-log-row level-{row.level}">
              <span class="dnf-log-no" aria-hidden="true">{(logCurrentPage - 1) * logPageSize + index + 1}</span>
              <span class="dnf-log-time" title={row.stamp}>{row.stamp || '—'}</span>
              <span class="dnf-log-level">
                {#if row.level === 'error'}Error{:else if row.level === 'warn'}Warn{:else}Info{/if}
              </span>
              <span class="dnf-log-msg">{row.message}</span>
              <button
                type="button"
                class="dnf-log-copy"
                title="Copy this line"
                onclick={() => { navigator.clipboard.writeText(row.message); uiStore.addToast('Line copied', 'info', 2000); }}
              >
                <Copy size={11} />
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Pagination Toolbar -->
      {#if logLines.length > 0}
        <div class="pagination-bar">
          <div class="pagination-info">
            Showing {(logCurrentPage - 1) * logPageSize + 1}–{Math.min(logCurrentPage * logPageSize, filteredLogLines.length)} of {filteredLogLines.length.toLocaleString()} log lines
          </div>

          <div class="pagination-controls">
            <Button variant="outline" size="sm" onclick={() => setLogPage(1)} disabled={logCurrentPage === 1}>
              &laquo; First
            </Button>
            <Button variant="outline" size="sm" onclick={() => setLogPage(logCurrentPage - 1)} disabled={logCurrentPage === 1}>
              &lsaquo; Prev
            </Button>

            <span class="pagination-page-indicator">
              Page <strong>{logCurrentPage}</strong> of <strong>{totalLogPages}</strong>
            </span>

            <Button variant="outline" size="sm" onclick={() => setLogPage(logCurrentPage + 1)} disabled={logCurrentPage === totalLogPages}>
              Next &rsaquo;
            </Button>
            <Button variant="outline" size="sm" onclick={() => setLogPage(totalLogPages)} disabled={logCurrentPage === totalLogPages}>
              Last &raquo;
            </Button>

            <select
              value={logPageSize}
              onchange={(e) => { logPageSize = parseInt((e.target as HTMLSelectElement).value); logCurrentPage = 1; }}
              class="pagination-size-select"
            >
              <option value={50}>50 / page</option>
              <option value={100}>100 / page</option>
              <option value={200}>200 / page</option>
              <option value={500}>500 / page</option>
            </select>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
<SideDrawer
  bind:isOpen={historyDetailOpen}
  title={historyDetail ? `Transaction #${historyDetail.id}` : 'Transaction'}
  subtitle="dnf history entry"
  width="480px"
>
  {#if historyDetail}
    <div class="history-detail">
      <div class="history-detail-row">
        <span>Command</span>
        <code>{historyDetail.command || '—'}</code>
      </div>
      <div class="history-detail-row">
        <span>Date</span>
        <strong>{formatToLocalDate(historyDetail.date)}</strong>
      </div>
      <div class="history-detail-row">
        <span>Action</span>
        <span class="badge {actionBadge(historyDetail.action)}" title={historyDetail.action}>
          {actionLabel(historyDetail.action)}
        </span>
      </div>
      <div class="history-detail-row">
        <span>Packages altered</span>
        <strong>{historyDetail.altered}</strong>
      </div>
      <div class="history-detail-packages">
        <div class="history-detail-subhead">Packages altered</div>
        {#if historyPackagesLoading}
          <div class="history-detail-hint">Reading the transaction…</div>
        {:else if historyPackages.length === 0}
          <div class="history-detail-hint">dnf did not list packages for this transaction.</div>
        {:else}
          {#each historyPackages as pkg}
            <div class="history-package-row">
              <span class="badge {actionBadge(pkg.action)}">{pkg.action}</span>
              <code title={pkg.nevra}>{pkg.nevra}</code>
              <span class="history-package-repo">{pkg.repo}</span>
            </div>
          {/each}
        {/if}
      </div>

      <p class="history-detail-hint">
        A rollback reverts this transaction with `dnf history undo` — review the confirmation before applying it.
      </p>
      <Button
        variant="danger"
        size="sm"
        onclick={() => { historyDetailOpen = false; confirmUndo(historyDetail!); }}
        disabled={undoingId === historyDetail.id}
      >
        <Undo2 size={13} /> Roll back this transaction
      </Button>
    </div>
  {/if}
</SideDrawer>

<DnfDryRunModal onConfirm={() => selectedUpdates.clear()} />

<style>
  /* ── Update filters ──────────────────────────────────────────────────── */
  .update-filters {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 0 10px;
    flex-shrink: 0;
  }

  /* The select defaults to width:100% — in the filter row it has to keep its
     own width, otherwise it takes the whole row and pushes the other filters
     onto separate lines. */
  .update-filters :global(.ui-select-container) {
    width: 170px;
    flex: 0 0 auto;
  }

  .update-filters :global(.search-bar-container) {
    flex: 0 1 260px;
    min-width: 170px;
  }

  .filter-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 30px;
    padding: 0 10px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-input, rgba(255, 255, 255, 0.03));
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .filter-toggle:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  .filter-toggle.active {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }

  .filter-count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    padding: 0 5px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }

  .selection-summary {
    margin-left: auto;
    font-size: 12px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  /* ── Transaction summary ─────────────────────────────────────────────── */
  .upgrade-summary {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 10px 14px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-card);
    flex-shrink: 0;
  }

  .summary-item {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .summary-value {
    font-family: var(--font-mono);
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .summary-item.has-errors .summary-value {
    color: var(--color-error);
  }

  .summary-label {
    font-size: 10.5px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .summary-spacer {
    flex: 1;
  }

  .summary-filter {
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 11.5px;
    cursor: pointer;
  }

  .summary-filter:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  .summary-filter.active {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }

  .error-line {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-error);
    white-space: pre-wrap;
  }

  /* ── Pagination Bar ─────────────────────────────────────────────────── */
  .pagination-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 14px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-bg-card);
    flex-shrink: 0;
  }

  :global(html.light-mode) .pagination-bar {
    background: #FFFFFF;
    border-color: #E2E8F0;
  }

  .pagination-info {
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .pagination-controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .pagination-page-indicator {
    font-size: 12px;
    padding: 0 6px;
    color: var(--color-text-primary);
  }

  .pagination-size-select {
    background: var(--color-bg-input, rgba(0,0,0,0.2));
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 6px;
    cursor: pointer;
  }

  :global(html.light-mode) .pagination-size-select {
    background: #F8FAFC;
    border-color: #CBD5E1;
    color: #0F172A;
  }

  /* ── Non-scrollable upgrade terminal ─────────────────────────────────── */
  .upgrade-terminal {
    background: #0d0f14;
    color: #00ff88;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    padding: 14px 16px;
    border-radius: 8px;
    overflow: hidden;         /* NON-SCROLLABLE: intentional */
    white-space: pre-wrap;
    word-break: break-all;
    min-height: 220px;
    max-height: 340px;
    border: 1px solid rgba(0, 255, 136, 0.1);
    box-shadow: inset 0 2px 12px rgba(0, 0, 0, 0.4);
    flex-shrink: 0;
  }

  /* ── Hang warning banner ─────────────────────────────────────────────── */
  .hang-warning {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 14px;
    background: rgba(253, 203, 110, 0.1);
    border: 1px solid rgba(253, 203, 110, 0.25);
    border-radius: 8px;
    color: var(--color-warning);
    font-size: 12px;
    line-height: 1.5;
    flex-shrink: 0;
  }

  /* ── Maintenance cards ───────────────────────────────────────────────── */
  .maint-card {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    padding: 16px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    transition: border-color 0.15s ease;
  }
  .maint-card-neutral { border-color: var(--color-border); }
  .maint-card-danger  { border-color: rgba(239, 68, 68, 0.3); background: rgba(239, 68, 68, 0.04); }
  .maint-card-warning { border-color: rgba(253, 203, 110, 0.3); background: rgba(253, 203, 110, 0.04); }

  .maint-card-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .icon-danger  { background: rgba(239, 68, 68, 0.12);   color: var(--color-error); }
  .icon-warning { background: rgba(253, 203, 110, 0.12); color: var(--color-warning); }
  .icon-success { background: rgba(0, 210, 211, 0.12);   color: var(--color-success); }
  .icon-info    { background: rgba(9, 132, 227, 0.12);   color: var(--color-info); }

  .maint-card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .maint-card-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text-primary);
  }
  .maint-card-desc {
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin-bottom: 4px;
  }

  /* ── Empty state ──────────────────────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    color: var(--color-text-muted);
  }

  .pkg-results-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }

  .pkg-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--color-error-muted);
    color: var(--color-error);
    font-size: 12px;
  }

  .pkg-copy {
    margin-left: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 1px 3px;
    border-radius: 4px;
  }

  .pkg-copy:hover {
    color: var(--color-accent);
    background: var(--color-bg-hover);
  }

  .dnf-log-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    /* Its own surface, clearly apart from the log table below. */
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-card);
    margin-bottom: 12px;
  }

  .dnf-log-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }

  .dnf-log-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .dnf-log-icon-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }

  .dnf-log-icon-btn.active {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
  }

  .dnf-log-count {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .history-detail {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .history-detail-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }

  .history-detail-row code {
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-secondary);
  }

  .history-detail-hint {
    margin: 4px 0 0;
    font-size: 11.5px;
    color: var(--color-text-muted);
  }


  .history-day-row td {
    padding: 8px 12px;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    border-top: 1px solid var(--color-border);
  }

  .history-detail-packages {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--color-border);
    padding-top: 12px;
  }

  .history-detail-subhead {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }

  .history-package-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
  }

  .history-package-row code {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-secondary);
  }

  .history-package-repo {
    color: var(--color-text-muted);
    font-size: 10.5px;
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .maint-last-run {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
    color: var(--color-text-muted);
    padding: 6px 2px;
  }

  .dnf-log-view {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-bg-card);
    margin-top: 0;
    font-family: var(--font-mono);
    font-size: 11.5px;
  }

  .dnf-log-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--color-border-subtle);
    border-left: 2px solid transparent;
  }

  .dnf-log-row:last-child {
    border-bottom: none;
  }

  .dnf-log-row.level-error {
    border-left-color: var(--color-error);
    background: var(--color-error-muted);
  }

  .dnf-log-row.level-warn {
    border-left-color: var(--color-warning);
    background: var(--color-warning-muted);
  }

  .dnf-log-no {
    flex: 0 0 48px;
    text-align: right;
    color: var(--color-text-muted);
    opacity: 0.6;
    user-select: none;
  }

  .dnf-log-time {
    flex: 0 0 172px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .dnf-log-level {
    flex: 0 0 42px;
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
    padding-top: 1px;
  }

  .dnf-log-row.level-error .dnf-log-level { color: var(--color-error); }
  .dnf-log-row.level-warn .dnf-log-level { color: var(--color-warning); }

  .dnf-log-msg {
    flex: 1;
    min-width: 0;
    color: var(--color-text-secondary);
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dnf-log-view.wrap .dnf-log-msg {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .dnf-log-copy {
    flex-shrink: 0;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    padding: 0 2px;
  }

  .dnf-log-row:hover .dnf-log-copy {
    opacity: 1;
  }

  /* The tab itself is only a container: no card surface, so the toolbar and
     the log panel read as two separate surfaces. */
  .dnf-logs-tab {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: 12px;
    background: transparent;
    border: 0;
    padding: 0;
  }

  .check-age {
    color: var(--color-text-muted);
    font-size: 11.5px;
  }
</style>
