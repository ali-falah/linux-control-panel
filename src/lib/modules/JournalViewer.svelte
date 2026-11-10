<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { 
    FileText, RefreshCw, X, ShieldAlert, ShieldCheck, Shield, Terminal, Key, Eraser,
    AlertTriangle, Sparkles, Copy, Download, Radio, Play, Square, Activity, Check, Code, 
    Sliders, Filter, Cpu, Layers, ExternalLink, ChevronRight, ChevronDown, FolderOpen
  } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import SideDrawer from '../components/SideDrawer.svelte';
  import Select from '../components/ui/Select.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';
  import DatePicker from '../components/ui/DatePicker.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import SearchBar from '../components/ui/SearchBar.svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';
  import { highlightSegments } from '../utils/highlight.ts';

  // raw state: a large journal page is replaced wholesale, never mutated in
  // place — deep reactivity would proxy every one of the 20k entries
  let logs = $state.raw<any[]>([]);
  let isLoading = $state(false);
  /** True when the selected range holds more entries than we fetched */
  let journalTruncated = $state(false);
  /** Wall-clock time of the last fetch (ms) */
  let journalFetchMs = $state(0);
  /** Total number of entries the current range holds (0 = not known yet) */
  let journalTotalInRange = $state(0);
  /** Timestamp of the oldest entry we have — used to page further back */
  let journalOldestTs = $state<string | null>(null);
  let journalOldestCursor = $state<string | null>(null);
  let isLoadingOlder = $state(false);
  let journalError = $state('');
  let journalLoaded = $state(false);
  let fetchGeneration = 0;
  let authGeneration = 0;
  let auditGeneration = 0;
  let threatGeneration = 0;
  let authLoaded = $state(false);
  let auditLoaded = $state(false);
  let threatsLoaded = $state(false);
  let rangeBeforeCustom = $state('1');
  let lastNonCustomRange = $state('1');
  /** How many entries a single fetch may return */
  let maxEntries = $state(20000);

  // Backend filters (re-fetch journalctl)
  let currentUnitFilter = $state('');
  let filterPriority = $state('all');
  let journalUserScope = $state(false);
  let journalCurrentBoot = $state(false);
  let timeRange = $state('1'); // Default to last 24 hours (1 day)
  let customStartDate = $state('');
  let customStartTime = $state('00:00');
  let customEndDate = $state('');
  let customEndTime = $state('23:59');
  let showCustomPopover = $state(false);

  let customRangeLabel = $derived(
    customStartDate
      ? `${formatDateLabel(customStartDate)} - ${formatDateLabel(customEndDate)}`
      : 'Custom Range...'
  );

  let customRangeValid = $derived.by(() => {
    const start = getCustomStart();
    const end = getCustomEnd();
    return Boolean(start && end && start < end);
  });

  $effect(() => {
    if (timeRange !== 'custom') lastNonCustomRange = timeRange;
  });

  // Sorting state
  let authSortKey = $state('timestamp');
  let authSortAsc = $state(false);

  let auditSortKey = $state('timestamp');
  let auditSortAsc = $state(false);

  // Live Streaming state
  let isLiveFollowing = $state(false);
  let unlistenLive: (() => void) | null = null;

  // Client-side live search (instant, no fetch)
  let searchQuery = $state('');

  let logContainer: HTMLElement;

  // New Tab state
  let activeTab = $state<'journal' | 'auth' | 'audit' | 'threats'>(
    uiStore.targetSubTab && ['journal', 'auth', 'audit', 'threats'].includes(uiStore.targetSubTab)
      ? (uiStore.targetSubTab as any)
      : 'journal'
  );
  if (uiStore.targetSubTab && ['journal', 'auth', 'audit', 'threats'].includes(uiStore.targetSubTab)) {
    uiStore.targetSubTab = null;
  }

  // Auth Events state
  let authEvents = $state<any[]>([]);
  let loadingAuth = $state(false);
  let authError = $state('');
  let authTotal = $state(0);
  let authTruncated = $state(false);

  // Command Audit state
  let auditLogs = $state<any[]>([]);
  let loadingAudit = $state(false);
  let auditError = $state('');
  let auditTotal = $state(0);
  let auditTruncated = $state(false);
  let auditdStatus = $state<{ installed: boolean; running: boolean; rules_configured: boolean } | null>(null);
  let installingAuditd = $state(false);
  let setupError = $state('');

  // Threats state
  let runtimeThreats = $state<any[]>([]);
  let threatRules = $state<any[]>([]);
  let loadingThreats = $state(false);
  let threatsError = $state('');

  const LOG_TABS = $derived([
    { id: 'journal', label: 'Journal' },
    { id: 'auth', label: 'Auth' },
    { id: 'audit', label: 'Audit' },
    { id: 'threats', label: 'Threats', count: runtimeThreats.length > 0 ? runtimeThreats.length : undefined },
  ]);

  function getCustomStart() {
    if (timeRange !== 'custom' || !customStartDate) return null;
    return `${customStartDate} ${customStartTime || '00:00'}:00`;
  }

  function getCustomEnd() {
    if (timeRange !== 'custom' || !customEndDate) return null;
    return `${customEndDate} ${customEndTime || '23:59'}:59`;
  }

  async function fetchLogs(opts: { appendOlder?: boolean } = {}) {
    const appendOlder = opts.appendOlder === true;
    const generation = ++fetchGeneration;
    const previousOldest = journalOldestTs;
    const previousCursor = journalOldestCursor;

    if (appendOlder) {
      isLoadingOlder = true;
    } else {
      isLoading = true;
      isLoadingOlder = false;
      journalError = '';
      journalTruncated = false;
      journalOldestTs = null;
      journalOldestCursor = null;
      journalTotalInRange = 0;
    }
    statusStore.setBusy('Fetching journal logs...');

    try {
      const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
      let sinceF = null;
      let untilF = null;
      if (timeRange === 'custom') {
        sinceF = getCustomStart();
        untilF = getCustomEnd();
      } else {
        sinceF = `${timeRange} days ago`;
      }

      // Paging further back: prefer the opaque journal cursor; fall back to
      // the oldest timestamp for older backends.
      const usingCursor = appendOlder && !!previousCursor;
      if (appendOlder && previousOldest && !usingCursor) {
        untilF = new Date(Number(previousOldest) / 1000).toISOString();
      }

      const unitF = currentUnitFilter.trim() || null;
      const page = await invoke<any>('get_journal_logs', {
        unitFilter: unitF,
        priority: prioF,
        sinceFilter: sinceF,
        untilFilter: untilF,
        maxEntries: maxEntries,
        userMode: journalUserScope,
        boot: journalCurrentBoot,
        cursor: usingCursor ? previousCursor : null
      });

      if (generation !== fetchGeneration) return;

      // The backend returns newest-first. `--until` is inclusive, so the
      // boundary entry is repeated on every page; drop it and anything newer.
      const incoming = page.entries || [];
      const entries = appendOlder && previousOldest && !usingCursor
        ? incoming.filter((entry: any) => Number(entry.__REALTIME_TIMESTAMP) < Number(previousOldest))
        : incoming;

      logs = appendOlder ? [...logs, ...entries] : entries;
      journalTruncated = !!page.truncated;
      if (page.oldest_ts) journalOldestTs = page.oldest_ts;
      if (page.oldest_cursor) journalOldestCursor = page.oldest_cursor;
      journalFetchMs = page.elapsed_ms || 0;
      journalLoaded = true;

      if (!appendOlder) {
        setTimeout(() => {
          const tableWrap = logContainer?.querySelector('.table-wrap') as HTMLElement | null;
          if (tableWrap) {
            tableWrap.scrollTop = 0;
            logScrollTop = 0;
            logContainerHeight = tableWrap.clientHeight || 700;
          }
        }, 50);

        // Count the whole range in the background — the table is already
        // usable, the exact total fills in when it arrives.
        countRange(unitF, prioF, sinceF, untilF, generation);

        if (page.truncated) {
          uiStore.addToast(
            `This range holds more entries — showing the most recent ${(page.entries || []).length.toLocaleString()}.`,
            'warning',
            5000,
            'Load older',
            () => loadOlderLogs()
          );
        }
      }

      statusStore.setLastCommand(
        `journalctl -n ${maxEntries} -o json ${prioF ? '-p ' + prioF : ''} ${sinceF ? '--since="' + sinceF + '"' : ''} ${untilF ? '--until="' + untilF + '"' : ''}`,
        0,
        true
      );
    } catch (e) {
      if (generation !== fetchGeneration) return;
      journalError = String(e);
      journalTruncated = false;
      journalTotalInRange = 0;
      statusStore.setLastCommand('journalctl', 1, false);
    } finally {
      if (generation === fetchGeneration) {
        isLoading = false;
        isLoadingOlder = false;
        statusStore.clearBusy();
      }
    }
  }

  /** Fetch the next page of older entries and append them. */
  async function loadOlderLogs() {
    if (isLoadingOlder) return;
    await fetchLogs({ appendOlder: true });
  }

  /**
   * Ask the backend how many entries the whole range holds. Runs in the
   * background so the table is usable immediately; the total simply fills in
   * when it arrives.
   */
  async function countRange(
    unitFilter: string | null,
    priority: number | null,
    sinceFilter: string | null,
    untilFilter: string | null,
    generation: number
  ) {
    try {
      const total = await invoke<number>('count_journal_entries', {
        unitFilter,
        priority,
        sinceFilter,
        untilFilter,
        userMode: journalUserScope,
        boot: journalCurrentBoot
      });
      if (generation !== fetchGeneration) return;
      journalTotalInRange = total || 0;
    } catch {
      if (generation === fetchGeneration) journalTotalInRange = 0;
    }
  }

  async function fetchAuthEvents() {
    const generation = ++authGeneration;
    loadingAuth = true;
    authError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      const page = await invoke<{ events: any[]; total: number; truncated: boolean }>('get_auth_events', {
        sinceDays: days,
        customStart: getCustomStart(),
        customEnd: getCustomEnd()
      });
      if (generation !== authGeneration) return;
      authEvents = page.events;
      authTotal = page.total;
      authTruncated = page.truncated;
      authLoaded = true;
    } catch (e: any) {
      if (generation !== authGeneration) return;
      authError = String(e);
    } finally {
      if (generation === authGeneration) loadingAuth = false;
    }
  }

  async function fetchAuditLogs() {
    const generation = ++auditGeneration;
    loadingAudit = true;
    auditError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      const [status, logs] = await Promise.all([
        invoke<any>('check_auditd_status').catch(() => null),
        invoke<{ events: any[]; total: number; truncated: boolean }>('get_command_audit_logs', {
          sinceDays: days,
          customStart: getCustomStart(),
          customEnd: getCustomEnd()
        })
      ]);
      if (generation !== auditGeneration) return;
      auditdStatus = status;
      auditLogs = logs.events;
      auditTotal = logs.total;
      auditTruncated = logs.truncated;
      auditLoaded = true;
    } catch (e: any) {
      if (generation !== auditGeneration) return;
      auditError = String(e);
    } finally {
      if (generation === auditGeneration) loadingAudit = false;
    }
  }

  async function handleSetupAuditd() {
    installingAuditd = true;
    setupError = '';
    try {
      await invoke('setup_auditd_rules');
      statusStore.setLastCommand("setup_auditd_rules", 0, true);
      await fetchAuditLogs();
    } catch (e: any) {
      console.error("Error setting up auditd:", e);
      setupError = String(e);
      statusStore.setLastCommand("setup_auditd_rules", 1, false);
    } finally {
      installingAuditd = false;
    }
  }

  async function fetchThreats() {
    const generation = ++threatGeneration;
    loadingThreats = true;
    threatsError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      const [threats, rules] = await Promise.all([
        invoke<any[]>('get_runtime_threats', {
          sinceDays: days,
          customStart: getCustomStart(),
          customEnd: getCustomEnd()
        }),
        invoke<any[]>('get_threat_rules').catch(() => [])
      ]);
      if (generation !== threatGeneration) return;
      runtimeThreats = threats;
      threatRules = rules;
      threatsLoaded = true;
    } catch (e: any) {
      if (generation !== threatGeneration) return;
      threatsError = String(e);
    } finally {
      if (generation === threatGeneration) loadingThreats = false;
    }
  }

  let popoverContainer = $state<HTMLDivElement | null>(null);

  function handleTabChange(tabId: string) {
    if (tabId !== 'journal' && tabId !== 'auth' && tabId !== 'audit' && tabId !== 'threats') return;
    activeTab = tabId;
    // Load a tab the first time it is opened — switching back is instant.
    // A range/level change calls refreshActiveTab() explicitly.
    if (tabId === 'journal' && !journalLoaded) fetchLogs();
    else if (tabId === 'auth' && !authLoaded) fetchAuthEvents();
    else if (tabId === 'audit' && !auditLoaded) fetchAuditLogs();
    else if (tabId === 'threats' && !threatsLoaded) fetchThreats();
  }

  function handleRangeChange() {
    if (timeRange === 'custom') {
      rangeBeforeCustom = lastNonCustomRange;
      // Delay by one tick so the triggering click doesn't bubble
      // to the document handler and immediately close the popover
      setTimeout(() => { showCustomPopover = true; }, 0);
    } else {
      showCustomPopover = false;
      refreshActiveTab();
    }
  }

  function applyCustomRange() {
    if (!customRangeValid) {
      uiStore.addToast('Choose a start and end date with start before end', 'warning');
      return;
    }
    showCustomPopover = false;
    refreshActiveTab();
  }

  function cancelCustomRange() {
    showCustomPopover = false;
    timeRange = rangeBeforeCustom;
  }

  function formatDateLabel(dateStr: string) {
    if (!dateStr) return '';
    const parts = dateStr.split('-');
    if (parts.length === 3) {
      return `${parts[1]}/${parts[2]}/${parts[0]}`; // MM/DD/YYYY
    }
    return dateStr;
  }

  function getPriorityColor(prio: string | number) {
    const p = parseInt(prio as string);
    if (isNaN(p)) return 'var(--color-accent)';
    if (p <= 3) return 'var(--color-error)';
    if (p === 4) return 'var(--color-warning)';
    if (p === 5 || p === 6) return 'var(--color-accent)';
    return 'var(--color-text-muted)';
  }

  // Stable click-outside and Escape handlers — registered once in onMount
  // We read showCustomPopover directly (Svelte $state is a live signal).

  function timestampValue(ts: string): number {
    if (!ts) return 0;
    const text = ts.includes('-') ? ts : `${new Date().getFullYear()} ${ts}`;
    const parsed = Date.parse(text);
    return Number.isNaN(parsed) ? 0 : parsed;
  }

  function sortIndicator(key: string, active: string, asc: boolean): string {
    return key === active ? (asc ? '▲' : '▼') : '↕';
  }

  function ariaSort(key: string, active: string, asc: boolean): 'ascending' | 'descending' | 'none' {
    if (key !== active) return 'none';
    return asc ? 'ascending' : 'descending';
  }

  function toggleAuthSort(key: string) {
    if (authSortKey === key) {
      authSortAsc = !authSortAsc;
    } else {
      authSortKey = key;
      authSortAsc = false;
    }
  }

  function toggleAuditSort(key: string) {
    if (auditSortKey === key) {
      auditSortAsc = !auditSortAsc;
    } else {
      auditSortKey = key;
      auditSortAsc = false;
    }
  }

  function clearAll() {
    if (activeTab === 'journal') logs = [];
    else if (activeTab === 'auth') authEvents = [];
    else if (activeTab === 'audit') auditLogs = [];
    else if (activeTab === 'threats') runtimeThreats = [];
    searchQuery = '';
  }

  /**
   * Apply deep-link filters coming from another module (Dashboard, Service
   * Manager …) exactly once — they are consumed here so that later state
   * changes never overwrite what the user typed.
   *
   * Only the three deep-link fields are tracked. Applying them (and the
   * fetch that follows) is untracked on purpose: an effect that reads and
   * writes the same state re-triggers itself and Svelte aborts the update
   * with "Maximum update depth exceeded".
   */
  let hasLoadedOnce = false;
  $effect(() => {
    const prio = uiStore.preAppliedJournalPriority;
    const search = uiStore.preAppliedJournalSearch;
    const unit = uiStore.preAppliedJournalUnit;

    untrack(() => {
      let applied = false;

      if (prio && prio !== 'all') {
        filterPriority = prio;
        applied = true;
      }
      if (search) {
        searchQuery = search;
        applied = true;
      }
      if (unit) {
        currentUnitFilter = unit;
        applied = true;
      }

      // consume the deep link so it is not applied again on the next visit
      uiStore.preAppliedJournalPriority = 'all';
      uiStore.preAppliedJournalSearch = '';
      uiStore.preAppliedJournalUnit = '';

      // remember the applied context — the page header shows it as a
      // breadcrumb, but the pre-applied fields are already cleared here
      if (applied) {
        uiStore.journalDeepLinkContext = { unit: unit ?? '', search: search ?? '' };
      }

      // The first run loads the page: either with the deep-linked filters or
      // the default tab. Later runs only refetch when a filter was applied.
      if (applied) {
        fetchLogs();
        hasLoadedOnce = true;
      } else if (!hasLoadedOnce) {
        refreshActiveTab();
        hasLoadedOnce = true;
      }
    });
  });

  // the deep-link context only belongs to this view
  onDestroy(() => {
    uiStore.journalDeepLinkContext = null;
  });

  onMount(() => {
    // Stable document listeners for popover close-on-outside-click and Escape
    function handleOutsideClick(e: MouseEvent) {
      if (!showCustomPopover) return;
      // If target was unmounted/detached during click handling (e.g. DatePicker calendar cell unmounted on select), ignore
      if (e.target && !document.body.contains(e.target as Node)) return;
      if (popoverContainer && popoverContainer.contains(e.target as Node)) return;
      // Also ignore clicks within the custom-range-container (the Select trigger area)
      const rangeContainer = document.querySelector('.custom-range-container');
      if (rangeContainer && rangeContainer.contains(e.target as Node)) return;
      showCustomPopover = false;
    }
    function handleGlobalKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        showCustomPopover = false;
      }
    }

    document.addEventListener('click', handleOutsideClick);
    document.addEventListener('keydown', handleGlobalKeyDown);

    return () => {
      document.removeEventListener('click', handleOutsideClick);
      document.removeEventListener('keydown', handleGlobalKeyDown);
      if (unlistenLive) {
        unlistenLive();
        unlistenLive = null;
      }
      invoke('stop_journal_live_stream').catch(() => {});
    };
  });

  let liveStreamKey = '';
  let liveRestartBusy = false;

  function currentLiveKey() {
    return `${filterPriority}|${currentUnitFilter}|${journalUserScope}|${journalCurrentBoot}`;
  }

  async function stopLiveStream() {
    if (unlistenLive) {
      unlistenLive();
      unlistenLive = null;
    }
    await invoke('stop_journal_live_stream').catch(() => {});
    liveStreamKey = '';
  }

  async function startLiveStream() {
    const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
    const unitF = currentUnitFilter.trim() || null;
    await invoke('start_journal_live_stream', {
      unitFilter: unitF,
      priority: prioF,
      userMode: journalUserScope,
      boot: journalCurrentBoot,
    });
    try {
      unlistenLive = await listen<any>('journal-live-log', (event) => {
        const parsed = event.payload;
        if (parsed) {
          logs = [parsed, ...logs.slice(0, 2500)];
        }
      });
    } catch (e) {
      await invoke('stop_journal_live_stream').catch(() => {});
      throw e;
    }
    liveStreamKey = currentLiveKey();
  }

  async function toggleLiveFollow() {
    if (isLiveFollowing) {
      isLiveFollowing = false;
      await stopLiveStream();
      uiStore.addToast('Live follow paused', 'info');
      return;
    }

    try {
      await startLiveStream();
      isLiveFollowing = true;
      uiStore.addToast('Live Journal streaming active', 'success');
    } catch (e) {
      await stopLiveStream();
      isLiveFollowing = false;
      uiStore.addToast(`Failed to start live stream: ${e}`, 'error');
    }
  }

  async function restartLiveStream() {
    if (!isLiveFollowing || liveRestartBusy) return;
    liveRestartBusy = true;
    try {
      await stopLiveStream();
      await startLiveStream();
    } catch (e) {
      await stopLiveStream();
      isLiveFollowing = false;
      uiStore.addToast(`Failed to restart live stream: ${e}`, 'error');
    } finally {
      liveRestartBusy = false;
    }
  }

  // Changing the level or unit filter must restart the stream with the new
  // arguments; the backend supersedes the old generation.
  $effect(() => {
    const key = `${filterPriority}|${currentUnitFilter}|${journalUserScope}|${journalCurrentBoot}`;
    if (!isLiveFollowing || liveRestartBusy || liveStreamKey === key) return;
    void restartLiveStream();
  });

  async function copyLog(log: LogItem) {
    const ts = formatTimestamp(log.__REALTIME_TIMESTAMP);
    const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel';
    const text = `[${ts}] [${unit}] [Priority: ${log.PRIORITY}] ${log.MESSAGE}`;
    try {
      await navigator.clipboard.writeText(text);
      uiStore.addToast('Log entry copied to clipboard', 'success');
    } catch (e) {
      uiStore.addToast(`Could not copy log entry: ${e}`, 'error');
    }
  }

  let showExportMenu = $state(false);
  let exportMenuContainer = $state<HTMLDivElement | null>(null);
  let lastExportedFile = $state<{ name: string; path: string } | null>(null);

  // Close export dropdown when clicking outside
  $effect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (!showExportMenu) return;
      if (exportMenuContainer && exportMenuContainer.contains(e.target as Node)) return;
      showExportMenu = false;
    }
    document.addEventListener('click', handleOutsideClick);
    return () => document.removeEventListener('click', handleOutsideClick);
  });

  function csvCell(value: unknown): string {
    return `"${String(value ?? '').replace(/"/g, '""')}"`;
  }

  function csvRowsForTab(tab: string, items: any[]): unknown[][] {
    if (tab === 'journal') {
      return [
        ['Time', 'Unit', 'Priority', 'Message'],
        ...items.map(l => [
          formatTimestamp(l.__REALTIME_TIMESTAMP),
          l._SYSTEMD_UNIT || l.SYSLOG_IDENTIFIER || 'kernel',
          l.PRIORITY,
          l.MESSAGE,
        ]),
      ];
    }
    if (tab === 'auth') {
      return [
        ['Time', 'User', 'Event Type', 'Source IP', 'Result', 'Details'],
        ...items.map(e => [e.timestamp, e.user, e.event_type, e.source_ip, e.result, e.details]),
      ];
    }
    if (tab === 'audit') {
      return [
        ['Time', 'User', 'Command', 'CWD', 'Result'],
        ...items.map(a => [a.timestamp, a.user, a.command, a.cwd, a.result]),
      ];
    }
    return [
      ['Time', 'Severity', 'Category', 'Title', 'Description'],
      ...items.map(t => [t.timestamp, t.severity, t.category, t.title, t.description]),
    ];
  }

  async function handleExportAll() {
    showExportMenu = false;
    if (timeRange === 'custom' && !customRangeValid) {
      uiStore.addToast('Choose a valid custom date range first', 'warning');
      return;
    }

    let items: any[] = [];
    let truncated = false;
    try {
      if (activeTab === 'journal') {
        const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
        const sinceF = timeRange === 'custom' ? getCustomStart() : `${timeRange} days ago`;
        const untilF = timeRange === 'custom' ? getCustomEnd() : null;
        const page = await invoke<any>('get_journal_logs', {
          unitFilter: currentUnitFilter.trim() || null,
          priority: prioF,
          sinceFilter: sinceF,
          untilFilter: untilF,
          maxEntries: 200000,
          userMode: journalUserScope,
          boot: journalCurrentBoot,
          cursor: null,
        });
        items = page.entries || [];
        truncated = !!page.truncated;
      } else if (activeTab === 'auth') {
        const isCustom = timeRange === 'custom';
        const page = await invoke<{ events: any[]; truncated: boolean }>('get_auth_events', {
          sinceDays: isCustom ? null : parseInt(timeRange),
          customStart: getCustomStart(),
          customEnd: getCustomEnd(),
          all: true,
        });
        items = page.events;
        truncated = page.truncated;
      } else if (activeTab === 'audit') {
        const isCustom = timeRange === 'custom';
        const page = await invoke<{ events: any[]; truncated: boolean }>('get_command_audit_logs', {
          sinceDays: isCustom ? null : parseInt(timeRange),
          customStart: getCustomStart(),
          customEnd: getCustomEnd(),
          all: true,
        });
        items = page.events;
        truncated = page.truncated;
      } else {
        items = runtimeThreats;
      }

      if (items.length === 0) {
        uiStore.addToast('No entries in the selected range', 'warning');
        return;
      }

      const content = csvRowsForTab(activeTab, items)
        .map(row => row.map(csvCell).join(','))
        .join('\n');
      const dateStr = new Date().toISOString().replace(/[:.]/g, '-');
      const filename = `${activeTab}-full-${dateStr}.csv`;
      const savedPath = await invoke<string>('export_journal_logs_to_file', { filename, content });
      lastExportedFile = { name: filename, path: savedPath };
      uiStore.addToast(
        `Exported ${items.length.toLocaleString()} entries to ${filename}${truncated ? ' (range still exceeds the 200k cap)' : ''}`,
        truncated ? 'warning' : 'success',
      );
    } catch (e) {
      uiStore.addToast(`Export failed: ${e}`, 'error');
    }
  }

  async function handleExportLogs(format: 'txt' | 'json' | 'csv', openFolder: boolean = false) {
    showExportMenu = false;
    let exportCount = 0;
    let content = '';
    const dateStr = new Date().toISOString().replace(/[:.]/g, '-');
    let filename = `journal-logs-${dateStr}.${format}`;

    if (activeTab === 'journal') {
      if (filteredLogs.length === 0) {
        uiStore.addToast('No journal logs to export', 'warning');
        return;
      }
      exportCount = filteredLogs.length;
      filename = `system-journal-${dateStr}.${format}`;
      if (format === 'json') {
        content = JSON.stringify(filteredLogs, null, 2);
      } else if (format === 'csv') {
        const rows = [
          ['Time', 'Unit', 'Priority', 'Message'],
          ...filteredLogs.map(l => [
            formatTimestamp(l.__REALTIME_TIMESTAMP),
            l._SYSTEMD_UNIT || l.SYSLOG_IDENTIFIER || 'kernel',
            l.PRIORITY,
            l.MESSAGE,
          ]),
        ];
        content = rows.map(row => row.map(csvCell).join(',')).join('\n');
      } else {
        content = filteredLogs.map(l => {
          const ts = formatTimestamp(l.__REALTIME_TIMESTAMP);
          const unit = l._SYSTEMD_UNIT || l.SYSLOG_IDENTIFIER || 'kernel';
          return `[${ts}] [${unit}] [Prio:${l.PRIORITY}] ${l.MESSAGE}`;
        }).join('\n');
      }
    } else if (activeTab === 'auth') {
      const items = filteredAuthEvents;
      if (items.length === 0) {
        uiStore.addToast('No auth events to export', 'warning');
        return;
      }
      exportCount = items.length;
      filename = `auth-events-${dateStr}.${format}`;
      if (format === 'json') {
        content = JSON.stringify(items, null, 2);
      } else if (format === 'csv') {
        const rows = [
          ['Time', 'User', 'Event Type', 'Source IP', 'Result', 'Details'],
          ...items.map(e => [e.timestamp, e.user, e.event_type, e.source_ip, e.result, e.details]),
        ];
        content = rows.map(row => row.map(csvCell).join(',')).join('\n');
      } else {
        content = items.map(e => `[${e.timestamp}] [${e.event_type}] [${e.user || 'unknown'}] [${e.result}] ${e.details || ''}`).join('\n');
      }
    } else if (activeTab === 'audit') {
      const items = filteredAuditLogs;
      if (items.length === 0) {
        uiStore.addToast('No audit logs to export', 'warning');
        return;
      }
      exportCount = items.length;
      filename = `audit-logs-${dateStr}.${format}`;
      if (format === 'json') {
        content = JSON.stringify(items, null, 2);
      } else if (format === 'csv') {
        const rows = [
          ['Time', 'User', 'Command', 'CWD', 'Result'],
          ...items.map(a => [a.timestamp, a.user, a.command, a.cwd, a.result]),
        ];
        content = rows.map(row => row.map(csvCell).join(',')).join('\n');
      } else {
        content = items.map(a => `[${a.timestamp}] [${a.user || 'unknown'}] [${a.result}] ${a.command || ''} (cwd: ${a.cwd || '—'})`).join('\n');
      }
    } else if (activeTab === 'threats') {
      const items = filteredThreats;
      if (items.length === 0) {
        uiStore.addToast('No threat entries to export', 'warning');
        return;
      }
      exportCount = items.length;
      filename = `threat-events-${dateStr}.${format}`;
      if (format === 'json') {
        content = JSON.stringify(items, null, 2);
      } else if (format === 'csv') {
        const rows = [
          ['Time', 'Severity', 'Category', 'Title', 'Description'],
          ...items.map(t => [t.timestamp, t.severity, t.category, t.title, t.description]),
        ];
        content = rows.map(row => row.map(csvCell).join(',')).join('\n');
      } else {
        content = items.map(t => `[${t.timestamp}] [${t.severity}] [${t.category}] ${t.title} — ${t.description}`).join('\n');
      }
    }

    try {
      // The backend writes the file straight into ~/Downloads — a browser
      // download on top of that would leave the user with two copies.
      const savedPath = await invoke<string>('export_journal_logs_to_file', { filename, content });
      lastExportedFile = { name: filename, path: savedPath };

      if (openFolder) {
        await invoke('reveal_in_file_manager', { path: savedPath });
        uiStore.addToast(`Exported ${exportCount} logs & opened Downloads folder`, 'success');
      } else {
        uiStore.addToast(`Exported ${exportCount} logs to ${filename}`, 'success');
      }
    } catch (e: any) {
      console.error('Failed to export logs:', e);
      uiStore.addToast(`Export failed: ${e}`, 'error');
    }
  }

  async function openLastExportedFolder() {
    if (lastExportedFile?.path) {
      try {
        await invoke('reveal_in_file_manager', { path: lastExportedFile.path });
      } catch (e) {
        console.error('Could not reveal in file manager:', e);
      }
    }
  }

  function formatTimestamp(us: string) {
    if (!us) return '';
    const date = new Date(parseInt(us) / 1000);
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) + ', ' +
      date.toLocaleTimeString('en-US', { hour12: true, hour: 'numeric', minute: '2-digit', second: '2-digit' });
  }

  function getPriorityClass(prio: string | number) {
    const p = parseInt(prio as string);
    if (isNaN(p)) return 'log-info';
    if (p <= 3) return 'log-error';
    if (p === 4) return 'log-warn';
    if (p === 5 || p === 6) return 'log-info';
    return 'log-debug';
  }

  // Highlight rendering: journal content is UNTRUSTED, so it is never put
  // through `{@html}`. The `mark` snippet renders plain, framework-escaped
  // text nodes and wraps literal query matches (case-insensitive) with
  // `<mark class="hl">`. See src/lib/utils/highlight.ts.

  interface LogItem {
    PRIORITY: string | number;
    __REALTIME_TIMESTAMP: string;
    _SYSTEMD_UNIT?: string;
    SYSLOG_IDENTIFIER?: string;
    MESSAGE: string;
    count?: number;
    _unique_id?: string;
    /** lower-cased searchable text, built once per fetch instead of per keystroke */
    _hay?: string;
    [key: string]: any;
  }

  /** Lower-cased haystack for one entry — built once, not on every keystroke. */
  function buildHaystack(log: any): string {
    const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || log._COMM || 'kernel';
    const msg = typeof log.MESSAGE === 'string' ? log.MESSAGE : JSON.stringify(log.MESSAGE || '');
    return `${msg} ${unit} ${log.SYSLOG_IDENTIFIER || ''} ${log._COMM || ''} ${log._PID || ''} ${log._EXE || ''}`.toLowerCase();
  }

  // Collapse consecutive duplicates
  let collapsedLogs = $derived.by(() => {
    const list: LogItem[] = [];
    let idx = 0;
    for (const log of logs) {
      if (!log) continue;
      const last = list[list.length - 1];
      const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel';
      const lastUnit = last ? (last._SYSTEMD_UNIT || last.SYSLOG_IDENTIFIER || 'kernel') : null;
      if (last && unit === lastUnit && log.MESSAGE === last.MESSAGE) {
        last.count = (last.count || 1) + 1;
        last.__REALTIME_TIMESTAMP = log.__REALTIME_TIMESTAMP;
      } else {
        idx++;
        const uniqueId = `${log.__REALTIME_TIMESTAMP || ''}_${unit}_${idx}`;
        list.push({ ...log, _unique_id: uniqueId, count: 1, _hay: buildHaystack(log) });
      }
    }
    return list;
  });

  // Instant client-side filters
  let filteredLogs = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return collapsedLogs;
    return collapsedLogs.filter(log => {
      if (!log) return false;
      // prebuilt haystack — no JSON.stringify on every keystroke
      return (log._hay ?? buildHaystack(log)).includes(q);
    });
  });

  // Virtual Scrolling State for Journal Logs Table
  let logScrollTop = $state(0);
  let logContainerHeight = $state(700);
  const LOG_OVERSCAN = 35;

  let logRowHeight = $derived(
    uiStore.tableDensity === 'compact' ? 19 : (uiStore.tableDensity === 'spacious' ? 39 : 28)
  );
  let journalFilteredCount = $derived(filteredLogs.length);

  let logStartIndex = $derived.by(() => {
    if (journalFilteredCount <= 60) return 0;
    const raw = Math.floor(logScrollTop / logRowHeight) - LOG_OVERSCAN;
    return Math.max(0, Math.min(raw, Math.max(0, journalFilteredCount - 1)));
  });

  let logVisibleCount = $derived.by(() => {
    if (journalFilteredCount <= 60) return journalFilteredCount;
    return Math.ceil(logContainerHeight / logRowHeight) + 2 * LOG_OVERSCAN;
  });

  let logEndIndex = $derived(
    journalFilteredCount <= 60
      ? journalFilteredCount
      : Math.min(journalFilteredCount, logStartIndex + logVisibleCount)
  );

  let visibleLogs = $derived(
    journalFilteredCount <= 60
      ? filteredLogs
      : filteredLogs.slice(logStartIndex, logEndIndex)
  );

  let logTopPadding = $derived(journalFilteredCount <= 60 ? 0 : logStartIndex * logRowHeight);
  let logBottomPadding = $derived(journalFilteredCount <= 60 ? 0 : Math.max(0, (journalFilteredCount - logEndIndex) * logRowHeight));

  function handleLogScroll(e: Event) {
    const target = e.currentTarget as HTMLElement;
    if (target) {
      logScrollTop = target.scrollTop;
      logContainerHeight = target.clientHeight || 700;
    }
  }

  // Structured Log Detail Drawer State
  let selectedLog = $state<any | null>(null);
  let isLogDrawerOpen = $state(false);
  let activeDrawerTab = $state<'fields' | 'json'>('fields');
  let copiedField = $state<string | null>(null);

  function openLogDrawer(log: any) {
    selectedLog = log;
    isLogDrawerOpen = true;
    activeDrawerTab = 'fields';
  }

  async function copyField(name: string, value: string) {
    try {
      await navigator.clipboard.writeText(value);
      copiedField = name;
      uiStore.addToast(`Copied ${name} to clipboard`, 'success', 2000);
      setTimeout(() => {
        if (copiedField === name) copiedField = null;
      }, 2000);
    } catch (e) {
      uiStore.addToast(`Could not copy ${name}: ${e}`, 'error');
    }
  }

  function filterByDrawerUnit(unitName: string) {
    currentUnitFilter = unitName;
    searchQuery = '';
    isLogDrawerOpen = false;
    activeTab = 'journal';
    fetchLogs();
    uiStore.addToast(`Filtering journal by unit: ${unitName}`, 'info');
  }

  function filterByDrawerPid(pid: string) {
    searchQuery = pid;
    isLogDrawerOpen = false;
    uiStore.addToast(`Filtering logs by PID: ${pid}`, 'info');
  }

  function getPriorityLabel(prio: string | number): string {
    const p = parseInt(prio as string);
    switch(p) {
      case 0: return 'Emergency';
      case 1: return 'Alert';
      case 2: return 'Critical';
      case 3: return 'Error';
      case 4: return 'Warning';
      case 5: return 'Notice';
      case 6: return 'Info';
      case 7: return 'Debug';
      default: return 'Info';
    }
  }

  function getStructuredLogFields(log: any) {
    if (!log) return [];
    const fields = [
      { key: '_PID', label: 'Process ID (PID)', val: log._PID, desc: 'Target process PID' },
      { key: '_COMM', label: 'Command Name', val: log._COMM, desc: 'Executable command name' },
      { key: '_EXE', label: 'Executable Path', val: log._EXE, desc: 'Absolute binary path' },
      { key: '_CMDLINE', label: 'Command Line', val: log._CMDLINE, desc: 'Full arguments string' },
      { key: '_SYSTEMD_UNIT', label: 'Systemd Unit', val: log._SYSTEMD_UNIT, desc: 'Associated systemd service unit' },
      { key: '_SYSTEMD_CGROUP', label: 'CGroup Path', val: log._SYSTEMD_CGROUP, desc: 'Control group hierarchy path' },
      { key: '_TRANSPORT', label: 'Transport', val: log._TRANSPORT, desc: 'Origin transport (journal, stdout, syslog, kernel)' },
      { key: 'SYSLOG_IDENTIFIER', label: 'Syslog Identifier', val: log.SYSLOG_IDENTIFIER, desc: 'Identifier in syslog stream' },
      { key: '_UID', label: 'User ID (UID)', val: log._UID, desc: 'Process user ID' },
      { key: '_GID', label: 'Group ID (GID)', val: log._GID, desc: 'Process group ID' },
      { key: '__REALTIME_TIMESTAMP', label: 'Timestamp (μs)', val: log.__REALTIME_TIMESTAMP, desc: 'Realtime timestamp in microseconds' },
      { key: '_HOSTNAME', label: 'Hostname', val: log._HOSTNAME, desc: 'Origin host machine' },
      { key: '_BOOT_ID', label: 'Boot ID', val: log._BOOT_ID, desc: 'Unique system boot session ID' },
      { key: '_MACHINE_ID', label: 'Machine ID', val: log._MACHINE_ID, desc: 'Unique OS installation machine ID' },
    ];
    return fields.filter(f => f.val !== undefined && f.val !== null && String(f.val).trim() !== '');
  }

  let filteredAuthEvents = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    let items = q ? authEvents.filter(ev =>
      (ev.user || '').toLowerCase().includes(q) ||
      (ev.event_type || '').toLowerCase().includes(q) ||
      (ev.source_ip || '').toLowerCase().includes(q) ||
      (ev.details || '').toLowerCase().includes(q)
    ) : [...authEvents];

    items.sort((a, b) => {
      if (authSortKey === 'timestamp') {
        const diff = timestampValue(a.timestamp) - timestampValue(b.timestamp);
        return authSortAsc ? diff : -diff;
      }
      const valA = String(a[authSortKey] ?? '');
      const valB = String(b[authSortKey] ?? '');
      return authSortAsc ? valA.localeCompare(valB) : valB.localeCompare(valA);
    });
    return items;
  });

  let filteredAuditLogs = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    let items = q ? auditLogs.filter(ev =>
      (ev.user || '').toLowerCase().includes(q) ||
      (ev.command || '').toLowerCase().includes(q) ||
      (ev.cwd || '').toLowerCase().includes(q)
    ) : [...auditLogs];

    items.sort((a, b) => {
      if (auditSortKey === 'timestamp') {
        const diff = timestampValue(a.timestamp) - timestampValue(b.timestamp);
        return auditSortAsc ? diff : -diff;
      }
      const valA = String(a[auditSortKey] ?? '');
      const valB = String(b[auditSortKey] ?? '');
      return auditSortAsc ? valA.localeCompare(valB) : valB.localeCompare(valA);
    });
    return items;
  });

  let filteredThreats = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return runtimeThreats;
    return runtimeThreats.filter(t =>
      (t.title || '').toLowerCase().includes(q) ||
      (t.description || '').toLowerCase().includes(q) ||
      (t.severity || '').toLowerCase().includes(q)
    );
  });

  let hasActiveSearch = $derived(searchQuery.trim().length > 0);
  let activeLogsCount = $derived.by(() => {
    if (activeTab === 'journal') return filteredLogs.length;
    if (activeTab === 'auth') return filteredAuthEvents.length;
    if (activeTab === 'audit') return filteredAuditLogs.length;
    return filteredThreats.length;
  });
  let totalLogsCount = $derived.by(() => {
    if (activeTab === 'journal') return collapsedLogs.length;
    if (activeTab === 'auth') return authEvents.length;
    if (activeTab === 'audit') return auditLogs.length;
    return runtimeThreats.length;
  });
  let activeTabLoading = $derived.by(() => {
    if (activeTab === 'journal') return isLoading;
    if (activeTab === 'auth') return loadingAuth;
    if (activeTab === 'audit') return loadingAudit;
    return loadingThreats;
  });
  function refreshActiveTab() {
    if (timeRange === 'custom' && !customRangeValid) {
      uiStore.addToast('Choose a valid custom date range first', 'warning');
      showCustomPopover = true;
      return;
    }
    if (activeTab === 'journal') fetchLogs();
    else if (activeTab === 'auth') fetchAuthEvents();
    else if (activeTab === 'audit') fetchAuditLogs();
    else if (activeTab === 'threats') fetchThreats();
  }
</script>

<div class="module-page">
  {#snippet mark(text: string)}
    {#each highlightSegments(text, searchQuery) as seg, i (i)}{#if seg.hit}<mark class="hl">{seg.text}</mark>{:else}{seg.text}{/if}{/each}
  {/snippet}
  <PageHeader
    title="Journal Logs"
    subtitle="Live systemd journal, authentication, audit, and runtime-threat events"
    icon={FileText}
  >
    <TabGroup tabs={LOG_TABS} bind:activeTab={activeTab} onchange={handleTabChange} size="sm" class="header-tabs" />
    <Button variant="outline" size="sm" onclick={refreshActiveTab} disabled={activeTabLoading} title="Refresh the active tab">
      <RefreshCw size={13} class={activeTabLoading ? 'animate-spin-slow' : ''} />
      <span>Refresh</span>
    </Button>
  </PageHeader>


  <div class="log-viewer">
    <div class="log-controls">
        <div class="log-search-wrap">
          <SearchBar
            bind:value={searchQuery}
            count={hasActiveSearch ? activeLogsCount : undefined}
            total={hasActiveSearch ? totalLogsCount : undefined}
            placeholder={activeTab === 'auth' ? 'Search auth events…' : activeTab === 'audit' ? 'Search audit commands…' : activeTab === 'threats' ? 'Search threats…' : 'Search logs…'}
            ariaLabel="Search journal logs"
            style="width: 100%;"
          />
        </div>

        {#if currentUnitFilter}
          <div class="log-unit-chip">
            <span>Unit: <strong>{currentUnitFilter}</strong></span>
            <button
              type="button"
              onclick={() => { currentUnitFilter = ''; fetchLogs(); }}
              title="Clear unit filter"
              aria-label="Clear unit filter"
            >
              <X size={12} />
            </button>
          </div>
        {/if}

        <div class="log-filter-cluster">
                <div class="custom-range-container">
            <Select class="log-range-select {timeRange === 'custom' ? 'is-custom' : ''}" bind:value={timeRange} onchange={handleRangeChange} style="height: 28px;">
              <option value="1">Last 24 Hours</option>
              <option value="3">Last 3 Days</option>
              <option value="7">Last 7 Days</option>
              <option value="30">Last 30 Days</option>
              <option value="custom">{customRangeLabel}</option>
            </Select>

            {#if timeRange === 'custom' && showCustomPopover}
              <div bind:this={popoverContainer} class="custom-range-popover">
                <div class="popover-row">
                  <span class="popover-label">From</span>
                  <DatePicker bind:value={customStartDate} placeholder="Start date" />
                  <input
                    type="time"
                    bind:value={customStartTime}
                    class="log-dt"
                  />
                </div>
                <div class="popover-row" style="margin-top: 10px;">
                  <span class="popover-label">To</span>
                  <DatePicker bind:value={customEndDate} placeholder="End date" />
                  <input
                    type="time"
                    bind:value={customEndTime}
                    class="log-dt"
                  />
                </div>
                <div class="popover-actions">
                  <button
                    type="button"
                    class="popover-btn apply-btn"
                    disabled={!customRangeValid}
                    onclick={applyCustomRange}
                  >
                    Apply
                  </button>
                  <button
                    type="button"
                    class="popover-btn cancel-btn"
                    onclick={cancelCustomRange}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            {/if}
          </div>

          {#if activeTab === 'journal'}
            <Select
              class="log-level-select"
              bind:value={filterPriority}
              onchange={fetchLogs}
              ariaLabel="Journal level"
              style="height: 28px; width: 120px;"
            >
              <option value="all">All Levels</option>
              <option value="3">Error+</option>
              <option value="4">Warning+</option>
              <option value="6">Info+</option>
            </Select>

            <Button
              variant={journalCurrentBoot ? 'primary' : 'ghost'}
              size="sm"
              aria-pressed={journalCurrentBoot}
              onclick={() => { journalCurrentBoot = !journalCurrentBoot; fetchLogs(); }}
              title="Restrict the journal to the current boot"
            >
              This boot
            </Button>
            <Button
              variant={journalUserScope ? 'primary' : 'ghost'}
              size="sm"
              aria-pressed={journalUserScope}
              onclick={() => { journalUserScope = !journalUserScope; fetchLogs(); }}
              title="Show the current user's journal (journalctl --user)"
            >
              User journal
            </Button>
            <Button
              variant={isLiveFollowing ? 'primary' : 'ghost'}
              size="sm"
              aria-pressed={isLiveFollowing}
              onclick={toggleLiveFollow}
              title={isLiveFollowing ? 'Streaming new logs in real-time (Click to pause)' : 'Follow new logs in real-time'}
            >
              <span class="live-status-dot {isLiveFollowing ? 'pulsing' : ''}"></span>
              <span>Live</span>
            </Button>
          {/if}

          <span class="log-divider" aria-hidden="true"></span>

                  <div class="export-dropdown-container" bind:this={exportMenuContainer}>
            <button
              type="button"
              class="log-action-btn export-main-btn"
              onclick={() => showExportMenu = !showExportMenu}
              title="Export logs (Text, JSON, with option to open folder)"
            >
              <Download size={13} />
              <span>Export</span>
              <ChevronDown size={11} style="opacity:0.7; margin-left:1px;" />
            </button>

            {#if showExportMenu}
              <div class="export-dropdown-menu">
                <button type="button" class="export-menu-item" onclick={() => handleExportLogs('txt', false)}>
                  <FileText size={13} />
                  <span>Export as Text (.txt)</span>
                </button>
                <button type="button" class="export-menu-item" onclick={() => handleExportLogs('json', false)}>
                  <Code size={13} />
                  <span>Export as JSON (.json)</span>
                </button>
                <button type="button" class="export-menu-item" onclick={() => handleExportLogs('csv', false)}>
                  <FileText size={13} />
                  <span>Export as CSV (.csv, loaded rows)</span>
                </button>
                <button type="button" class="export-menu-item accent" onclick={handleExportAll}>
                  <FolderOpen size={13} />
                  <span>Export full range (CSV)</span>
                </button>
                <div class="export-menu-divider"></div>
                <button type="button" class="export-menu-item accent" onclick={() => handleExportLogs('txt', true)}>
                  <FolderOpen size={13} />
                  <span>Export Text &amp; Open Folder</span>
                </button>
              </div>
            {/if}
          </div>

          <Button variant="ghost" size="sm" onclick={clearAll} title="Clear loaded results" aria-label="Clear loaded log entries">
            <Eraser size={14} />
          </Button>
        </div>
    </div>

    <div class="log-container" bind:this={logContainer}>
      <!-- Tab 1: Journalctl logs -->
      {#if activeTab === 'journal'}
        {#if journalError}
          <div class="journal-alert" role="alert">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div class="journal-alert-text">
              <div class="journal-alert-title">Unable to read the journal</div>
              <div>{journalError}</div>
            </div>
            <Button variant="outline" size="sm" onclick={() => fetchLogs()}>Retry</Button>
          </div>
        {:else if filteredLogs.length === 0}
          {#if isLoading}
            <div class="log-skeleton" role="status" aria-label="Loading journal entries">
              {#each [1, 2, 3, 4, 5, 6, 7, 8] as row}
                <Skeleton height="18px" borderRadius="4px" />
              {/each}
            </div>
          {:else if hasActiveSearch && collapsedLogs.length > 0}
            <EmptyState
              icon={FileText}
              title="No Matching Logs"
              description={currentUnitFilter
                ? `No log messages matched "${searchQuery}" for unit ${currentUnitFilter}. Widen the range or clear the filters.`
                : `No log messages matched "${searchQuery}". Widen the range or clear the filters.`}
              actionLabel="Clear Search"
              onAction={() => { searchQuery = ''; }}
            />
          {:else}
            <EmptyState
              icon={FileText}
              title="No Logs in Selected Range"
              description={currentUnitFilter
                ? `No systemd journal messages found for ${currentUnitFilter} with the current time and level filters. Try a wider range.`
                : 'No systemd journal messages found for the current time and level filters.'}
              actionLabel="Refresh Logs"
              onAction={() => fetchLogs()}
            />
          {/if}
        {:else}
          <Table class="log-table" onscroll={handleLogScroll}>
            <thead>
              <tr>
                <th style="width: 150px;">Time</th>
                <th style="width: 180px;">Unit / Identifier</th>
                <th>Message</th>
                <th style="width: 96px; text-align: right;">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#if logTopPadding > 0}
                <tr style="height: {logTopPadding}px; padding: 0; margin: 0; border: none !important; line-height: 0; font-size: 0; pointer-events: none;"><td colspan="4" style="padding: 0; margin: 0; border: none !important; height: {logTopPadding}px; line-height: 0; font-size: 0;"></td></tr>
              {/if}
              {#each visibleLogs as log (log._unique_id || log.__REALTIME_TIMESTAMP + (log._SYSTEMD_UNIT || '') + (log.MESSAGE || ''))}
                {@const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel'}
                {@const hasMsg = Boolean(log.MESSAGE && String(log.MESSAGE).trim())}
                <tr
                  class="log-row {getPriorityClass(log.PRIORITY)}"
                  role="button"
                  tabindex="0"
                  aria-label={`Open details for ${unit}`}
                  onclick={() => openLogDrawer(log)}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openLogDrawer(log); } }}
                  style="cursor: pointer;"
                >
                  <td class="col-time" style="white-space: nowrap; color: var(--color-text-muted); font-size: 12px; font-family: var(--font-mono);">{formatTimestamp(log.__REALTIME_TIMESTAMP)}</td>
                  <td class="col-unit" title={unit} style="font-weight: 600; font-size: 12px; font-family: var(--font-mono); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                    {@render mark(unit)}
                  </td>
                  <td class="col-msg" style="color: var(--color-text-secondary); font-size: 12px; font-family: var(--font-mono);">
                    {@render mark(log.MESSAGE)}
                    {#if (log.count ?? 1) > 1}
                      <span class="repeat-badge">×{log.count}</span>
                    {/if}
                  </td>
                  <td style="text-align: right; white-space: nowrap;" onclick={(e) => e.stopPropagation()}>
                    <div style="display: inline-flex; align-items: center; gap: 4px;">
                      <button
                        type="button"
                        class="btn btn-ghost btn-sm"
                        onclick={(e) => { e.stopPropagation(); copyLog(log); }}
                        title="Copy log entry to clipboard"
                        style="padding: 2px 6px; font-size: 11px;"
                      >
                        <Copy size={11} />
                      </button>
                      {#if aiStore.enabled}
                        <button
                          type="button"
                          class="btn btn-outline btn-sm"
                          disabled={!hasMsg}
                          onclick={(e) => { e.stopPropagation(); hasMsg && aiStore.diagnoseLogError(String(log.MESSAGE), unit); }}
                          title={hasMsg ? "Diagnose log message with AI" : "Cannot diagnose empty log message"}
                          style="padding: 2px 8px; font-size: 11px; display: inline-flex; align-items: center; gap: 4px; opacity: {hasMsg ? 1 : 0.4}; cursor: {hasMsg ? 'pointer' : 'not-allowed'};"
                        >
                          <Sparkles size={11} style="color:var(--color-accent);" /> Diagnose
                        </button>
                      {/if}
                    </div>
                  </td>
                </tr>
              {/each}
              {#if logBottomPadding > 0}
                <tr style="height: {logBottomPadding}px; padding: 0; margin: 0; border: none !important; line-height: 0; font-size: 0; pointer-events: none;"><td colspan="4" style="padding: 0; margin: 0; border: none !important; height: {logBottomPadding}px; line-height: 0; font-size: 0;"></td></tr>
              {/if}
            </tbody>
          </Table>
          <!-- Table footer: entry count, fetch time, and paging for older entries -->
          {#if !isLoading && logs.length > 0}
            <div class="journal-table-footer">
              <div class="journal-footer-stats">
                <span class="journal-footer-count" title={hasActiveSearch ? 'Search covers the loaded entries only' : ''}>
                  <Layers size={12} />
                  {#if hasActiveSearch}
                    {filteredLogs.length.toLocaleString()} matches · {logs.length.toLocaleString()} loaded
                  {:else if journalTotalInRange}
                    {logs.length.toLocaleString()} of {journalTotalInRange.toLocaleString()} entries
                  {:else}
                    {logs.length.toLocaleString()} entries
                  {/if}
                </span>
                {#if hasActiveSearch && journalTruncated}
                  <span class="journal-footer-time">search covers loaded entries</span>
                {:else if journalFetchMs > 0}
                  <span class="journal-footer-time">fetched in {journalFetchMs} ms</span>
                {/if}
              </div>
              {#if journalTruncated}
                <button
                  type="button"
                  class="journal-footer-action"
                  onclick={loadOlderLogs}
                  disabled={isLoadingOlder}
                  title="Load the next batch of older entries"
                >
                  {#if isLoadingOlder}<RefreshCw size={12} class="animate-spin-slow" />{:else}<ChevronDown size={12} />{/if}
                  Load older entries
                </button>
              {/if}
            </div>
          {/if}
        {/if}

      <!-- Tab 2: Auth Events -->
      {:else if activeTab === 'auth'}
        {#if authError}
          <div class="journal-alert" role="alert">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div class="journal-alert-text">
              <div class="journal-alert-title">Unable to load authentication logs</div>
              <div>{authError}</div>
            </div>
            <Button variant="outline" size="sm" onclick={fetchAuthEvents}>Retry</Button>
          </div>
        {/if}
        {#if filteredAuthEvents.length === 0}
          <div class="empty-state">
            {#if loadingAuth}
              Fetching authentication logs…
            {:else if authError}
              Authentication log unavailable.
            {:else if hasActiveSearch}
              No auth events match <strong style="color:var(--color-text-primary); margin-left:4px;">"{searchQuery}"</strong>
            {:else}
              No auth events recorded.
            {/if}
          </div>
        {:else}
          {#if authTruncated}
            <div class="journal-note">Showing the newest {authEvents.length} of {authTotal} auth events in this range.</div>
          {/if}
          <Table class="log-table">
            <thead>
              <tr>
                <th aria-sort={ariaSort('timestamp', authSortKey, authSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuthSort('timestamp')}>Time <span aria-hidden="true">{sortIndicator('timestamp', authSortKey, authSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('user', authSortKey, authSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuthSort('user')}>User <span aria-hidden="true">{sortIndicator('user', authSortKey, authSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('event_type', authSortKey, authSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuthSort('event_type')}>Event Type <span aria-hidden="true">{sortIndicator('event_type', authSortKey, authSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('source_ip', authSortKey, authSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuthSort('source_ip')}>Source IP <span aria-hidden="true">{sortIndicator('source_ip', authSortKey, authSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('result', authSortKey, authSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuthSort('result')}>Result <span aria-hidden="true">{sortIndicator('result', authSortKey, authSortAsc)}</span></button>
                </th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredAuthEvents as ev}
                <tr class="log-row" style="border-bottom: 1px solid rgba(255, 255, 255, 0.03); font-size: 12px; font-family: var(--font-mono);">
                  <td style="padding: 8px 12px; white-space: nowrap; color: var(--color-text-muted);">{ev.timestamp}</td>
                  <td style="padding: 8px 12px; font-weight: 600; color: var(--color-text-primary);">
                    {@render mark(ev.user)}
                  </td>
                  <td style="padding: 8px 12px;">
                    <Badge variant={ev.event_type.includes('SSH') ? 'info' : (ev.event_type.includes('Sudo') ? 'warning' : 'muted')} style="font-size: 10px;">
                      {@render mark(ev.event_type)}
                    </Badge>
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary);">
                    {@render mark(ev.source_ip)}
                  </td>
                  <td style="padding: 8px 12px;">
                    <Badge variant={ev.result === 'Success' ? 'success' : 'error'} style="font-size: 10px;">
                      {ev.result}
                    </Badge>
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary); max-width: 400px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.details}>
                    {@render mark(ev.details)}
                  </td>
                </tr>
              {/each}
            </tbody>
          </Table>
        {/if}

      <!-- Tab 3: Command Audit -->
      {:else if activeTab === 'audit'}
        {#if auditError}
          <div class="journal-alert" role="alert">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div class="journal-alert-text">
              <div class="journal-alert-title">Unable to load audit logs</div>
              <div>{auditError}</div>
            </div>
            <Button variant="outline" size="sm" onclick={fetchAuditLogs}>Retry</Button>
          </div>
        {:else if setupError}
          <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; display: flex; align-items: center; gap: 10px;">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div>
              <div style="font-weight: 600; font-size: 13px; color: var(--color-error);">Setup Failed</div>
              <div style="font-size: 11px; color: var(--color-text-secondary); margin-top: 2px; font-family: var(--font-mono);">{setupError}</div>
            </div>
          </div>
        {:else if auditdStatus && (!auditdStatus.installed || !auditdStatus.running || !auditdStatus.rules_configured)}
          <div style="background: rgba(245, 158, 11, 0.08); border: 1px solid rgba(245, 158, 11, 0.2); border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; display: flex; justify-content: space-between; align-items: center; box-sizing: border-box; width: 100%;">
            <div style="display: flex; align-items: center; gap: 10px;">
              <ShieldAlert size={20} color="var(--color-warning)" style="flex-shrink: 0;" />
              <div>
                <div style="font-weight: 600; font-size: 13px; color: var(--color-warning);">Audit Daemon Rules Unconfigured</div>
                <div style="font-size: 11px; color: var(--color-text-secondary); margin-top: 2px;">
                  {#if !auditdStatus.installed}
                    auditd package is not installed on this system.
                  {:else if !auditdStatus.running}
                    auditd service is installed but not currently active.
                  {:else}
                    Control panel rule definitions are missing from /etc/audit/rules.d/
                  {/if}
                </div>
              </div>
            </div>
            <Button variant="accent" style="font-size: 11px; padding: 4px 12px; height: 28px;" onclick={handleSetupAuditd} disabled={installingAuditd}>
              {installingAuditd ? 'Configuring...' : 'Configure Auditd Rules'}
            </Button>
          </div>
        {/if}

        {#if filteredAuditLogs.length === 0}
          <div class="empty-state">
            {#if loadingAudit}
              Fetching command logs…
            {:else if auditError}
              Audit log unavailable.
            {:else if hasActiveSearch}
              No commands match <strong style="color:var(--color-text-primary); margin-left:4px;">"{searchQuery}"</strong>
            {:else}
              No audit command logs recorded.
            {/if}
          </div>
        {:else}
          {#if auditTruncated}
            <div class="journal-note">Showing the newest {auditLogs.length} of {auditTotal} audit events in this range.</div>
          {/if}
          <Table class="log-table">
            <thead>
              <tr>
                <th aria-sort={ariaSort('timestamp', auditSortKey, auditSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuditSort('timestamp')}>Time <span aria-hidden="true">{sortIndicator('timestamp', auditSortKey, auditSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('user', auditSortKey, auditSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuditSort('user')}>User <span aria-hidden="true">{sortIndicator('user', auditSortKey, auditSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('command', auditSortKey, auditSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuditSort('command')}>Command <span aria-hidden="true">{sortIndicator('command', auditSortKey, auditSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('cwd', auditSortKey, auditSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuditSort('cwd')}>CWD <span aria-hidden="true">{sortIndicator('cwd', auditSortKey, auditSortAsc)}</span></button>
                </th>
                <th aria-sort={ariaSort('result', auditSortKey, auditSortAsc)}>
                  <button type="button" class="journal-sort-btn" onclick={() => toggleAuditSort('result')}>Result <span aria-hidden="true">{sortIndicator('result', auditSortKey, auditSortAsc)}</span></button>
                </th>
              </tr>
            </thead>
            <tbody>
              {#each filteredAuditLogs as ev}
                <tr class="log-row" style="border-bottom: 1px solid rgba(255, 255, 255, 0.03); font-size: 12px; font-family: var(--font-mono);">
                  <td style="padding: 8px 12px; white-space: nowrap; color: var(--color-text-muted);">{ev.timestamp}</td>
                  <td style="padding: 8px 12px; font-weight: 600; color: var(--color-text-primary);">
                    {@render mark(ev.user)}
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-accent); max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.command}>
                    {@render mark(ev.command)}
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.cwd}>
                    {@render mark(ev.cwd)}
                  </td>
                  <td style="padding: 8px 12px;">
                    <Badge variant={ev.result === 'Success' ? 'success' : 'error'} style="font-size: 10px;">
                      {ev.result}
                    </Badge>
                  </td>
                </tr>
              {/each}
            </tbody>
          </Table>
        {/if}

      <!-- Tab 4: Threats -->
      {:else if activeTab === 'threats'}
        {#if threatsError}
          <div class="journal-alert" role="alert">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div class="journal-alert-text">
              <div class="journal-alert-title">Unable to run threat correlation</div>
              <div>{threatsError}</div>
            </div>
            <Button variant="outline" size="sm" onclick={fetchThreats}>Retry</Button>
          </div>
        {/if}
        {#if loadingThreats}
          <div class="empty-state">Analyzing runtime threats…</div>
        {:else if filteredThreats.length === 0 && hasActiveSearch}
          <EmptyState
            icon={ShieldCheck}
            title="No Matching Threats"
            description={`No threat findings matched "${searchQuery}". Clear the search or rescan the selected range.`}
            actionLabel="Clear Search"
            onAction={() => { searchQuery = ''; }}
          />
        {:else if filteredThreats.length === 0 && threatsError}
          <EmptyState
            icon={ShieldAlert}
            title="Threat scan unavailable"
            description="The correlation engine could not complete. Check the error above and retry."
            actionLabel="Retry Scan"
            onAction={fetchThreats}
          />
        {:else if filteredThreats.length === 0}
          <div style="display:flex; flex-direction:column; gap:16px; padding:12px 8px; box-sizing:border-box; width:100%;">
            <!-- Status Card -->
            <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:12px; padding:20px; display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:16px;">
              <div style="display:flex; align-items:center; gap:16px;">
                <div style="width:44px; height:44px; border-radius:50%; background:rgba(34,197,94,0.12); display:flex; align-items:center; justify-content:center; flex-shrink:0;">
                  <ShieldCheck size={26} color="var(--color-success)" />
                </div>
                <div>
                  <div style="font-size:15px; font-weight:700; color:var(--color-text-primary); display:flex; align-items:center; gap:8px;">
                    No Active Runtime Threats Detected
                    <span style="font-size:10px; font-weight:700; padding:2px 8px; border-radius:10px; background:rgba(34,197,94,0.15); color:var(--color-success); font-family:var(--font-mono);">PROTECTED</span>
                  </div>
                  <div style="font-size:12px; color:var(--color-text-muted); margin-top:2px;">
                    The real-time threat correlation engine evaluated authentication & audit logs for the selected range with 0 security violations.
                  </div>
                </div>
              </div>
              <Button variant="ghost" style="font-size:11px;" onclick={fetchThreats}>
                <RefreshCw size={13} class={loadingThreats ? 'animate-spin-slow' : ''} /> Rescan Logs
              </Button>
            </div>

            <!-- Active Safeguard Rules Grid -->
            <div style="background:var(--color-bg-raised); border:1px solid var(--color-border); border-radius:10px; padding:16px;">
              <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted); margin-bottom:12px;">
                Built-in Detection Rules
              </div>
              <div style="display:grid; grid-template-columns:repeat(auto-fit, minmax(260px, 1fr)); gap:10px;">
                {#if threatRules.length > 0}
                  {#each threatRules as rule (rule.id)}
                    <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                      <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                      <div>
                        <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">{rule.title}</div>
                        <div style="font-size:11px; color:var(--color-text-muted);">{rule.description}</div>
                      </div>
                    </div>
                  {/each}
                {:else}
                  <div style="font-size:11.5px; color:var(--color-text-muted);">Detection-rule metadata is unavailable.</div>
                {/if}
              </div>
            </div>
          </div>
        {:else}
          <div style="display:flex; flex-direction:column; gap:12px; padding: 8px; box-sizing: border-box; width: 100%;">
            {#each filteredThreats as threat}
              <div style="background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px; display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; box-sizing: border-box; width: 100%;">
                <div style="display:flex; flex-direction:column; gap:6px;">
                  <div style="display:flex; align-items:center; gap:8px;">
                    <ShieldAlert size={16} color={threat.severity === 'Critical' ? 'var(--color-error)' : 'var(--color-warning)'} />
                    <h3 style="font-size: 14px; font-weight: 700; margin: 0; color: var(--color-text-primary);">
                      {@render mark(threat.title)}
                    </h3>
                    <Badge variant={threat.severity === 'Critical' ? 'error' : 'warning'} style="font-size: 9px; text-transform: uppercase;">
                      {threat.severity}
                    </Badge>
                    <span style="font-size: 11px; color: var(--color-text-muted); font-family: var(--font-mono);">{threat.timestamp}</span>
                  </div>
                  <p style="font-size: 12px; color: var(--color-text-secondary); margin: 4px 0 0 0; line-height: 1.5;">
                    {@render mark(threat.description)}
                  </p>
                  <div style="margin-top: 8px;">
                    <Badge variant="muted" style="font-size: 9px; text-transform: uppercase; background: var(--color-bg-hover); color: var(--color-text-muted);">
                      {threat.category}
                    </Badge>
                  </div>
                </div>
                <Button 
                  variant="ghost" 
                  style="font-size: 11px; height: 26px; padding: 0 10px; border-color: var(--color-border);"
                  onclick={() => {
                    uiStore.securityCategoryFilter = 'Runtime Threats';
                    uiStore.securitySeverityFilter = 'all';
                    uiStore.setActiveTab('security-auditor');
                  }}
                >
                  Investigate
                </Button>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>

{#if selectedLog}
  <SideDrawer
    bind:isOpen={isLogDrawerOpen}
    title={selectedLog._SYSTEMD_UNIT || selectedLog.SYSLOG_IDENTIFIER || selectedLog._COMM || 'Log Entry'}
    subtitle={`${formatTimestamp(selectedLog.__REALTIME_TIMESTAMP)} · Priority ${selectedLog.PRIORITY} (${getPriorityLabel(selectedLog.PRIORITY)})`}
    width="580px"
  >
    {#snippet headerActions()}
      <div style="display: flex; align-items: center; gap: 6px; margin-right: 8px;">
        <Button
          variant={activeDrawerTab === 'fields' ? 'primary' : 'ghost'}
          size="sm"
          aria-pressed={activeDrawerTab === 'fields'}
          onclick={() => activeDrawerTab = 'fields'}
        >
          <Sliders size={12} /> Fields
        </Button>
        <Button
          variant={activeDrawerTab === 'json' ? 'primary' : 'ghost'}
          size="sm"
          aria-pressed={activeDrawerTab === 'json'}
          onclick={() => activeDrawerTab = 'json'}
        >
          <Code size={12} /> Raw JSON
        </Button>
        <Button
          variant="outline"
          size="sm"
          onclick={() => copyField('Raw JSON', JSON.stringify(selectedLog, null, 2))}
        >
          {#if copiedField === 'Raw JSON'}
            <Check size={12} style="color: var(--color-success);" /> Copied
          {:else}
            <Copy size={12} /> Copy JSON
          {/if}
        </Button>
      </div>
    {/snippet}

    <div class="log-drawer-body">
      <!-- Top Overview Banner -->
      <div class="drawer-overview-card {getPriorityClass(selectedLog.PRIORITY)}">
        <!-- Action Quick-Bar -->
        <div style="display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin-top: 10px; padding-top: 8px; border-top: 1px solid var(--color-border-subtle);">
          {#if selectedLog._SYSTEMD_UNIT || selectedLog.SYSLOG_IDENTIFIER}
            {@const u = selectedLog._SYSTEMD_UNIT || selectedLog.SYSLOG_IDENTIFIER}
            <button
              type="button"
              class="drawer-action-btn"
              onclick={() => filterByDrawerUnit(u)}
              title="Filter journal logs by {u}"
            >
              <Filter size={12} /> Filter by Unit
            </button>
          {/if}
          {#if selectedLog._PID}
            <button
              type="button"
              class="drawer-action-btn"
              onclick={() => filterByDrawerPid(String(selectedLog._PID))}
              title="Filter journal logs by PID {selectedLog._PID}"
            >
              <Terminal size={12} /> Filter by PID ({selectedLog._PID})
            </button>
          {/if}
          {#if aiStore.enabled && selectedLog.MESSAGE}
            <button
              type="button"
              class="drawer-action-btn ai-btn"
              onclick={() => aiStore.diagnoseLogError(String(selectedLog.MESSAGE), selectedLog._SYSTEMD_UNIT || selectedLog.SYSLOG_IDENTIFIER || 'kernel')}
              title="Diagnose log with AI"
            >
              <Sparkles size={12} style="color: var(--color-accent);" /> Diagnose with AI
            </button>
          {/if}
        </div>
      </div>

      {#if activeDrawerTab === 'fields'}
        <!-- Message Box -->
        <div class="drawer-section">
          <div class="drawer-section-title">
            <span>Log Message</span>
            <button 
              type="button" 
              class="field-copy-btn"
              onclick={() => copyField('Message', String(selectedLog.MESSAGE || ''))}
              title="Copy message"
            >
              {#if copiedField === 'Message'}<Check size={11} /> Copied{:else}<Copy size={11} /> Copy{/if}
            </button>
          </div>
          <div class="drawer-msg-box">
            {selectedLog.MESSAGE || '(Empty log message)'}
          </div>
        </div>

        <!-- Structured Metadata Table -->
        <div class="drawer-section">
          <div class="drawer-section-title">
            <span>Systemd Metadata</span>
          </div>

          <div class="drawer-fields-table">
            {#each getStructuredLogFields(selectedLog) as item}
              <div class="drawer-field-row">
                <div class="drawer-field-key" title={item.desc || item.key}>{item.label || item.key}</div>
                <div class="drawer-field-val" title={String(item.val)}>
                  <code>{String(item.val)}</code>
                </div>
                <button
                  type="button"
                  class="field-row-copy-btn"
                  onclick={() => copyField(item.key, String(item.val))}
                  title="Copy {item.key}"
                >
                  {#if copiedField === item.key}
                    <Check size={11} style="color: var(--color-success);" />
                  {:else}
                    <Copy size={11} />
                  {/if}
                </button>
              </div>
            {/each}
          </div>
        </div>
      {:else}
        <!-- Raw JSON Inspector -->
        <div class="drawer-section">
          <pre class="drawer-raw-json"><code>{JSON.stringify(selectedLog, null, 2)}</code></pre>
        </div>
      {/if}
    </div>
  </SideDrawer>
{/if}

<style>/* ── Journal controls row ───────────────────────────── */ .log-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    width: 100%;
  }.log-search-wrap {
    flex: 1 1 260px;
    min-width: 220px;
    max-width: 400px;
  }.log-filter-cluster {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
    margin-left: auto;
  }.log-unit-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    background: var(--color-accent-muted);
    border: 1px solid var(--color-accent);
    border-radius: 6px;
    font-size: 11.5px;
    color: var(--color-accent);
    flex-shrink: 0;
  }.log-unit-chip button {
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
  }.log-divider {
    width: 1px;
    height: 20px;
    background: var(--color-border);
    flex-shrink: 0;
  }
  @media (max-width: 1200px) {
    .log-search-wrap { flex: 1 1 100%; max-width: none; }
    .log-filter-cluster { margin-left: 0; }
  }
  @media (max-width: 900px) {
    .log-filter-cluster { width: 100%; }
  }
/* Date / time inputs */ .log-dt {
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    outline: none;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    padding: 4px 8px;
    height: 28px;
    box-sizing: border-box;
  }.log-dt::-webkit-calendar-picker-indicator {
    opacity: 0.45;
    cursor: pointer;
  }
  :global(html:not(.light-mode)) .log-dt::-webkit-calendar-picker-indicator {
    filter: invert(1);
  }.log-dt::-webkit-calendar-picker-indicator:hover { opacity: 0.9; }/* Custom Range Popover */ .custom-range-container {
    position: relative;
    display: inline-block;
  }.custom-range-popover {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 100;
    width: 310px;
    background: var(--color-bg-popover, var(--color-bg-card));
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 14px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.25);
  }.popover-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }.popover-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }/* Time input sits below the DatePicker trigger inside a popover row */ .popover-row .log-dt {
    width: 100%;
    box-sizing: border-box;
  }.popover-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
    border-top: 1px solid var(--color-border-subtle);
    padding-top: 10px;
  }.popover-btn {
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 6px;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }.apply-btn {
    background: var(--color-accent);
    color: var(--color-text-on-accent);
  }.apply-btn:hover:not(:disabled) {
    background: var(--color-accent);
    filter: brightness(1.08);
  }.apply-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }.cancel-btn {
    background: var(--color-bg-hover);
    color: var(--color-text-secondary);
  }.cancel-btn:hover {
    background: var(--color-active-bg);
    color: var(--color-text-primary);
  }/* Refresh / export action buttons */ .log-action-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border: none;
    border-radius: 6px;
    background: var(--color-bg-hover);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
  }.log-action-btn:hover {
    background: var(--color-active-bg);
    color: var(--color-text-primary);
  }.log-action-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }/* Export Dropdown Menu */ .export-dropdown-container {
    position: relative;
    display: inline-flex;
  }.export-main-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }.export-dropdown-menu {
    position: absolute;
    top: calc(100% + 5px);
    right: 0;
    z-index: 100;
    min-width: 185px;
    background: var(--color-bg-card, #0f172a);
    border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
    border-radius: 8px;
    padding: 5px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menu-fade 0.12s ease;
  }.export-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: var(--color-text-secondary);
    font-size: 11.5px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
    white-space: nowrap;
  }.export-menu-item:hover {
    background: var(--color-accent-muted);
    color: var(--color-text-primary);
  }.export-menu-item.accent {
    color: var(--color-accent);
    font-weight: 500;
  }.export-menu-item.accent:hover {
    background: var(--color-accent-muted);
  }.export-menu-divider {
    height: 1px;
    background: var(--color-border, rgba(255, 255, 255, 0.08));
    margin: 3px 0;
  }.export-open-folder-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    background: rgba(0, 218, 243, 0.12);
    border: 1px solid rgba(0, 218, 243, 0.3);
    color: var(--color-accent);
    border-radius: 6px;
    font-size: 11.5px;
    font-family: var(--font-sans);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }.export-open-folder-btn:hover {
    background: rgba(0, 218, 243, 0.22);
    color: #fff;
    border-color: rgba(0, 218, 243, 0.5);
  }/* ── Log viewer ──────────────────────────────────────── */ .log-viewer {
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 20px;
    margin-top: -14px;
    overflow: hidden;
    flex: 1;
    min-height: 0;
  }.log-container {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 12px;
  }
  /* ── Table footer (entry count + fetch time + paging) ───────── */
  .journal-table-footer {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 7px 12px;
    border-top: 1px solid var(--color-border);
    background: var(--color-bg-raised);
    font-size: 11.5px;
    color: var(--color-text-secondary);
    font-family: var(--font-sans);
    flex-shrink: 0;
  }

  .journal-footer-stats {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .journal-footer-count {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .journal-footer-time {
    color: var(--color-text-muted);
  }

  .journal-footer-action {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: auto;
    padding: 4px 10px;
    border: 1px solid var(--color-accent-glow);
    border-radius: 6px;
    background: var(--color-accent-muted);
    color: var(--color-accent);
    font-size: 11.5px;
    font-weight: 600;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: border-color 0.15s ease;
  }

  .journal-footer-action:hover:not(:disabled) {
    border-color: var(--color-accent);
  }

  .journal-footer-action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }:global(.log-container .table-wrap) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    box-shadow: none !important;
    border: none;
    border-radius: 0;
    background: transparent;
  }.empty-state {
    display: flex;
    justify-content: center;
    align-items: center;
    height: 100%;
    color: var(--color-text-muted);
    font-size: 13px;
    font-family: var(--font-sans);
  }.log-table {
    width: 100%;
    border-collapse: collapse;
  }
  .log-table :global(table) {
    table-layout: fixed;
    width: 100%;
  }.log-table :global(thead th) {
    position: sticky !important;
    top: 0 !important;
    z-index: 20 !important;
  }.log-row {
    border-bottom: 1px solid var(--color-border-subtle);
  }.log-row:hover { background: var(--color-bg-hover); }
  .log-row:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }.log-row td {
    padding: 4px 8px;
    vertical-align: top;
  }.col-time {
    width: 155px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }.col-unit {
    width: 145px;
    color: #8b949e;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.col-msg {
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .log-table .col-msg {
    white-space: nowrap;
  }/* Priority Colors (Dark Mode Default) */ .log-error .col-msg { color: #ff7b72; font-weight: 500; }.log-error .col-unit { color: #f87171 !important; }.log-warn .col-msg { color: #fbbf24; }.log-warn .col-unit { color: #f59e0b !important; }.log-info .col-msg { color: #38bdf8; }.log-info .col-unit { color: #38bdf8 !important; }.log-debug .col-msg { color: #94a3b8; }.log-debug .col-unit { color: #94a3b8 !important; }/* Priority Colors for Light Mode */ :global(html.light-mode) .log-error .col-unit { color: #dc2626 !important; }:global(html.light-mode) .log-warn .col-unit { color: #d97706 !important; }:global(html.light-mode) .log-info .col-unit { color: #0284c7 !important; }:global(html.light-mode) .log-debug .col-unit { color: #64748b !important; }:global(html.light-mode) .log-error .col-msg { color: #dc2626; }:global(html.light-mode) .log-warn .col-msg { color: #b45309; }:global(html.light-mode) .log-info .col-msg { color: #334155; }:global(html.light-mode) .log-debug .col-msg { color: #64748b; }.repeat-badge {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-secondary);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    margin-left: 8px;
    font-weight: bold;
    display: inline-block;
  }.live-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    display: inline-block;
  }.live-status-dot.pulsing {
    background: var(--color-success);
    box-shadow: 0 0 8px var(--color-success);
    animation: pulse-live 1.2s infinite;
  }
@keyframes pulse-live {0% { transform: scale(0.95); opacity: 0.8; }50% { transform: scale(1.3); opacity: 1; box-shadow: 0 0 10px var(--color-success); }100% { transform: scale(0.95); opacity: 0.8; }
}.log-row.log-error {
    border-left: 5px solid var(--color-error);
  }.log-row.log-warn {
    border-left: 5px solid var(--color-warning);
  }.log-row.log-info {
    border-left: 5px solid var(--color-accent);
  }.log-row.log-debug {
    border-left: 5px solid var(--color-border);
  }/* ── Log Detail Drawer Styles ─────────────────────────────────────────── */ .log-drawer-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
  }.drawer-overview-card {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 14px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2);
  }.drawer-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    border-radius: 5px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    font-size: 11px;
    font-family: var(--font-sans);
    cursor: pointer;
    transition: all 0.15s ease;
  }.drawer-action-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }.drawer-action-btn.ai-btn:hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }.drawer-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }.drawer-section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }.field-copy-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    font-size: 11px;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: color 0.15s ease;
  }.field-copy-btn:hover {
    color: var(--color-accent);
  }.drawer-msg-box {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    color: var(--color-text-primary);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 200px;
    overflow-y: auto;
  }.drawer-fields-table {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--color-bg-card);
  }.drawer-field-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: 12px;
  }.drawer-field-row:last-child {
    border-bottom: none;
  }.drawer-field-row:hover {
    background: var(--color-bg-hover);
  }.drawer-field-key {
    width: 140px;
    flex-shrink: 0;
    font-weight: 600;
    color: var(--color-text-secondary);
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }.drawer-field-val {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text-primary);
    white-space: normal;
    overflow-wrap: anywhere;
  }.drawer-field-val code {
    background: transparent;
    padding: 0;
    color: inherit;
    font-family: inherit;
  }.field-row-copy-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    border-radius: 4px;
    cursor: pointer;
    opacity: 0.6;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }.field-row-copy-btn:hover {
    opacity: 1;
    color: var(--color-accent);
    background: var(--color-bg-hover);
  }.drawer-raw-json {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 12px;
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 500px;
    overflow-y: auto;
    margin: 0;
  }  .hl {
    background: var(--color-accent-muted);
    color: var(--color-text-primary);
    border-radius: 3px;
    padding: 0 2px;
  }
  .journal-alert {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px;
    margin-bottom: 12px;
    border: 1px solid var(--color-error);
    border-radius: 8px;
    background: var(--color-error-muted, rgba(239, 68, 68, 0.08));
  }
  .journal-alert-text {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
    color: var(--color-text-secondary);
    overflow-wrap: anywhere;
  }
  .journal-alert-title {
    font-weight: 700;
    font-size: 12.5px;
    color: var(--color-error);
    margin-bottom: 2px;
  }
  .log-action-btn:focus-visible,
  .popover-btn:focus-visible,
  .journal-footer-action:focus-visible,
  .drawer-action-btn:focus-visible,
  .field-copy-btn:focus-visible,
  .field-row-copy-btn:focus-visible,
  .export-menu-item:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
  .journal-sort-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 0;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    text-transform: inherit;
    letter-spacing: inherit;
    cursor: pointer;
    text-align: inherit;
  }
  .journal-sort-btn:hover { color: var(--color-text-primary); }
  @media (prefers-reduced-motion: reduce) {
    .live-status-dot.pulsing { animation: none; }
  }
  .journal-note {
    padding: 8px 12px;
    margin-bottom: 8px;
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
    background: var(--color-bg-hover);
    color: var(--color-text-muted);
    font-size: 11.5px;
  }
  .log-skeleton {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px;
  }
  :global(.header-tabs) {
    box-shadow: none !important;
    background: transparent !important;
    border-color: transparent !important;
    padding: 0 !important;
  }
  :global(.log-range-select) {
    width: 130px;
    flex: 0 0 130px;
    transition: width 0.18s ease, flex-basis 0.18s ease;
  }
  :global(.log-range-select.is-custom) {
    width: 250px;
    flex: 0 0 250px;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.log-range-select) { transition: none; }
  }
  :global(.log-level-select) {
    width: 120px;
    flex: 0 0 120px;
  }
</style>
