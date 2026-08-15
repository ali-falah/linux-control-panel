<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { 
    FileText, RefreshCw, Search, X, Trash2, ShieldAlert, ShieldCheck, Shield, Terminal, Key, 
    AlertTriangle, Sparkles, Copy, Download, Radio, Play, Square, Activity
  } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Select from '../components/ui/Select.svelte';
  import TabGroup from '../components/ui/TabGroup.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import Table from '../components/ui/Table.svelte';
  import DatePicker from '../components/ui/DatePicker.svelte';
  import EmptyState from '../components/ui/EmptyState.svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { aiStore } from '../stores/aiStore.svelte.ts';

  let logs = $state<any[]>([]);
  let isLoading = $state(false);

  // Backend filters (re-fetch journalctl)
  let currentUnitFilter = $state(uiStore.preAppliedJournalUnit || '');
  if (uiStore.preAppliedJournalUnit) {
    uiStore.preAppliedJournalUnit = '';
  }
  let filterPriority = $state('all');
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
  let searchInputRef: HTMLInputElement;

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

  // Command Audit state
  let auditLogs = $state<any[]>([]);
  let loadingAudit = $state(false);
  let auditError = $state('');
  let auditdStatus = $state<{ installed: boolean; running: boolean; rules_configured: boolean } | null>(null);
  let installingAuditd = $state(false);
  let setupError = $state('');

  // Threats state
  let runtimeThreats = $state<any[]>([]);
  let loadingThreats = $state(false);
  let threatsError = $state('');

  const LOG_TABS = $derived([
    { id: 'journal', label: 'Journal' },
    { id: 'auth', label: 'Auth Events' },
    { id: 'audit', label: 'Command Audit' },
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

  async function fetchLogs() {
    isLoading = true;
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

      const unitF = currentUnitFilter.trim() || null;
      const lines = await invoke<string[]>('get_journal_logs', {
        unitFilter: unitF,
        priority: prioF,
        sinceFilter: sinceF,
        untilFilter: untilF,
      });

      logs = lines.map(line => {
        try { return JSON.parse(line); } catch { return null; }
      }).filter(Boolean).reverse();

      setTimeout(() => {
        if (logContainer) logContainer.scrollTop = 0;
        const tableWrap = logContainer?.querySelector('.table-wrap');
        if (tableWrap) tableWrap.scrollTop = 0;
      }, 50);

      statusStore.setLastCommand(
        `journalctl -n 2000 -o json ${prioF ? '-p ' + prioF : ''} ${sinceF ? '--since="' + sinceF + '"' : ''} ${untilF ? '--until="' + untilF + '"' : ''}`,
        0, true
      );
    } catch (e) {
      console.error('Error fetching journal logs:', e);
      statusStore.setLastCommand('journalctl', 1, false);
    } finally {
      isLoading = false;
      statusStore.clearBusy();
    }
  }

  async function fetchAuthEvents() {
    loadingAuth = true;
    authError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      authEvents = await invoke('get_auth_events', {
        sinceDays: days,
        customStart: getCustomStart(),
        customEnd: getCustomEnd()
      });
    } catch (e: any) {
      console.error("Error fetching auth events:", e);
      authError = String(e);
    } finally {
      loadingAuth = false;
    }
  }

  async function fetchAuditLogs() {
    loadingAudit = true;
    auditError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      const [status, logs] = await Promise.all([
        invoke<any>('check_auditd_status'),
        invoke<any[]>('get_command_audit_logs', {
          sinceDays: days,
          customStart: getCustomStart(),
          customEnd: getCustomEnd()
        })
      ]);
      auditdStatus = status;
      auditLogs = logs;
    } catch (e: any) {
      console.error("Error fetching audit logs:", e);
      auditError = String(e);
    } finally {
      loadingAudit = false;
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
    loadingThreats = true;
    threatsError = '';
    try {
      const isCustom = timeRange === 'custom';
      const days = isCustom ? null : parseInt(timeRange);
      runtimeThreats = await invoke('get_runtime_threats', {
        sinceDays: days,
        customStart: getCustomStart(),
        customEnd: getCustomEnd()
      });
    } catch (e: any) {
      console.error("Error fetching runtime threats:", e);
      threatsError = String(e);
    } finally {
      loadingThreats = false;
    }
  }

  let popoverContainer = $state<HTMLDivElement | null>(null);

  function handleTabChange(tabId: string) {
    activeTab = tabId;
    if (tabId === 'journal') fetchLogs();
    else if (tabId === 'auth') fetchAuthEvents();
    else if (tabId === 'audit') fetchAuditLogs();
    else if (tabId === 'threats') fetchThreats();
  }

  function handleRangeChange() {
    if (timeRange === 'custom') {
      // Delay by one tick so the triggering click doesn't bubble
      // to the document handler and immediately close the popover
      setTimeout(() => { showCustomPopover = true; }, 0);
    } else {
      showCustomPopover = false;
      refreshActiveTab();
    }
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

  onMount(() => {
    if (uiStore.preAppliedJournalPriority && uiStore.preAppliedJournalPriority !== 'all') {
      filterPriority = uiStore.preAppliedJournalPriority;
      uiStore.preAppliedJournalPriority = 'all';
    }
    if (uiStore.preAppliedJournalSearch) {
      searchQuery = uiStore.preAppliedJournalSearch;
      uiStore.preAppliedJournalSearch = '';
    }
    fetchLogs();
    fetchAuthEvents();
    fetchAuditLogs();
    fetchThreats();

    const handleTabSelect = (e: Event) => {
      const customEvent = e as CustomEvent;
      if (customEvent.detail) {
        handleTabChange(customEvent.detail);
      }
    };

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

    window.addEventListener('journal-tab-select', handleTabSelect);
    document.addEventListener('click', handleOutsideClick);
    document.addEventListener('keydown', handleGlobalKeyDown);

    return () => {
      window.removeEventListener('journal-tab-select', handleTabSelect);
      document.removeEventListener('click', handleOutsideClick);
      document.removeEventListener('keydown', handleGlobalKeyDown);
      if (unlistenLive) {
        unlistenLive();
        unlistenLive = null;
      }
      invoke('stop_journal_live_stream').catch(() => {});
    };
  });

  async function toggleLiveFollow() {
    isLiveFollowing = !isLiveFollowing;
    if (isLiveFollowing) {
      try {
        const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
        await invoke('start_journal_live_stream', { unitFilter: null, priority: prioF });
        unlistenLive = await listen<string>('journal-live-log', (event) => {
          try {
            const parsed = JSON.parse(event.payload);
            if (parsed) {
              logs = [parsed, ...logs.slice(0, 2500)];
            }
          } catch(err) {
            console.error(err);
          }
        });
        uiStore.addToast('Live Journal streaming active', 'success');
      } catch(e) {
        uiStore.addToast(`Failed to start live stream: ${e}`, 'error');
        isLiveFollowing = false;
      }
    } else {
      if (unlistenLive) {
        unlistenLive();
        unlistenLive = null;
      }
      await invoke('stop_journal_live_stream').catch(() => {});
      uiStore.addToast('Live follow paused', 'info');
    }
  }

  function copyLog(log: LogItem) {
    const ts = formatTimestamp(log.__REALTIME_TIMESTAMP);
    const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel';
    const text = `[${ts}] [${unit}] [Priority: ${log.PRIORITY}] ${log.MESSAGE}`;
    navigator.clipboard.writeText(text);
    uiStore.addToast('Log entry copied to clipboard', 'info');
  }

  function exportLogs(format: 'txt' | 'json') {
    if (filteredLogs.length === 0) {
      uiStore.addToast('No logs to export', 'warning');
      return;
    }
    let content = '';
    const filename = `journal-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.${format}`;
    if (format === 'json') {
      content = JSON.stringify(filteredLogs, null, 2);
    } else {
      content = filteredLogs.map(l => {
        const ts = formatTimestamp(l.__REALTIME_TIMESTAMP);
        const unit = l._SYSTEMD_UNIT || l.SYSLOG_IDENTIFIER || 'kernel';
        return `[${ts}] [${unit}] [Prio:${l.PRIORITY}] ${l.MESSAGE}`;
      }).join('\n');
    }

    const blob = new Blob([content], { type: format === 'json' ? 'application/json' : 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
    uiStore.addToast(`Exported ${filteredLogs.length} logs to ${filename}`, 'success');
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

  function escapeRegex(s: string) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  function highlight(text: string, q: string): string {
    if (!q || !text) return text || '';
    try {
      return text.replace(new RegExp(`(${escapeRegex(q)})`, 'gi'), '<mark class="hl">$1</mark>');
    } catch { return text; }
  }

  interface LogItem {
    PRIORITY: string | number;
    __REALTIME_TIMESTAMP: string;
    _SYSTEMD_UNIT?: string;
    SYSLOG_IDENTIFIER?: string;
    MESSAGE: string;
    count?: number;
  }

  // Collapse consecutive duplicates
  let collapsedLogs = $derived.by(() => {
    const list: LogItem[] = [];
    for (const log of logs) {
      if (!log) continue;
      const last = list[list.length - 1];
      const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel';
      const lastUnit = last ? (last._SYSTEMD_UNIT || last.SYSLOG_IDENTIFIER || 'kernel') : null;
      if (last && unit === lastUnit && log.MESSAGE === last.MESSAGE) {
        last.count = (last.count || 1) + 1;
        last.__REALTIME_TIMESTAMP = log.__REALTIME_TIMESTAMP;
      } else {
        list.push({ ...log, count: 1 });
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
      const msg = typeof log.MESSAGE === 'string' ? log.MESSAGE.toLowerCase() : JSON.stringify(log.MESSAGE || '').toLowerCase();
      const unit = String(log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || log._COMM || 'kernel').toLowerCase();
      const sysIdent = String(log.SYSLOG_IDENTIFIER || '').toLowerCase();
      const raw = JSON.stringify(log).toLowerCase();
      return msg.includes(q) || unit.includes(q) || sysIdent.includes(q) || raw.includes(q);
    });
  });

  let filteredAuthEvents = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    let items = q ? authEvents.filter(ev =>
      (ev.user || '').toLowerCase().includes(q) ||
      (ev.event_type || '').toLowerCase().includes(q) ||
      (ev.source_ip || '').toLowerCase().includes(q) ||
      (ev.details || '').toLowerCase().includes(q)
    ) : [...authEvents];

    items.sort((a, b) => {
      let valA = a[authSortKey as keyof typeof a] || '';
      let valB = b[authSortKey as keyof typeof b] || '';
      if (typeof valA === 'string') {
        return authSortAsc ? valA.localeCompare(valB) : valB.localeCompare(valA);
      }
      return authSortAsc ? (valA > valB ? 1 : -1) : (valA < valB ? 1 : -1);
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
      let valA = a[auditSortKey as keyof typeof a] || '';
      let valB = b[auditSortKey as keyof typeof b] || '';
      if (typeof valA === 'string') {
        return auditSortAsc ? valA.localeCompare(valB) : valB.localeCompare(valA);
      }
      return auditSortAsc ? (valA > valB ? 1 : -1) : (valA < valB ? 1 : -1);
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
    if (activeTab === 'journal') fetchLogs();
    else if (activeTab === 'auth') fetchAuthEvents();
    else if (activeTab === 'audit') fetchAuditLogs();
    else if (activeTab === 'threats') fetchThreats();
  }
</script>

<div class="module-page">
  <PageHeader title="Journal Logs" subtitle="Systemd journal log viewer" icon={FileText}>
    <!-- Single unified toolbar strip -->
    <div class="log-toolbar">
      <!-- Search icon -->
      <Search size={14} class="log-search-icon" />

      <!-- Text input: live search -->
      <input
        bind:this={searchInputRef}
        bind:value={searchQuery}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            const tw = logContainer?.querySelector('.table-wrap');
            if (tw) tw.scrollTop = 0;
          }
        }}
        type="text"
        class="log-search-input"
        placeholder="Search logs…"
        autocomplete="off"
        spellcheck={false}
      />

      {#if hasActiveSearch}
        <span class="log-count-badge">{activeLogsCount}/{totalLogsCount}</span>
        <button class="log-clear-btn" onclick={() => { searchQuery = ''; searchInputRef?.focus(); }} title="Clear search">
          <X size={14} />
        </button>
      {/if}

      {#if currentUnitFilter}
        <div style="display: flex; align-items: center; gap: 6px; padding: 2px 8px; background: rgba(0, 218, 243, 0.12); border: 1px solid rgba(0, 218, 243, 0.3); border-radius: 6px; font-size: 11.5px; color: var(--color-accent); flex-shrink: 0;">
          <span>Unit: <strong>{currentUnitFilter}</strong></span>
          <button 
            type="button" 
            onclick={() => { currentUnitFilter = ''; fetchLogs(); }} 
            title="Clear unit filter"
            style="background: transparent; border: none; color: var(--color-accent); cursor: pointer; padding: 0; display: flex; align-items: center;"
          >
            <X size={12} />
          </button>
        </div>
      {/if}

      <span class="log-sep"></span>

      <!-- Range Selector (applicable to all tabs) -->
      <span class="log-label">Range:</span>
      <div class="custom-range-container">
        <Select bind:value={timeRange} onchange={handleRangeChange} style="height: 28px; width: 130px;">
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
                onclick={() => {
                  showCustomPopover = false;
                  refreshActiveTab();
                }}
              >
                Apply
              </button>
              <button
                type="button"
                class="popover-btn cancel-btn"
                onclick={() => showCustomPopover = false}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
      </div>

      <!-- Live Follow Toggle -->
      <button 
        class="log-action-btn {isLiveFollowing ? 'live-following-btn' : ''}" 
        onclick={toggleLiveFollow} 
        title={isLiveFollowing ? 'Streaming new logs in real-time (Click to pause)' : 'Follow new logs in real-time'}
      >
        <span class="live-status-dot {isLiveFollowing ? 'pulsing' : ''}"></span>
        <span>{isLiveFollowing ? 'Live Following' : 'Live Follow'}</span>
      </button>

      <!-- Export Logs -->
      <button class="log-action-btn" onclick={() => exportLogs('txt')} title="Export filtered logs to .txt file">
        <Download size={13} />
        <span>Export</span>
      </button>

      <!-- Refresh -->
      <button class="log-action-btn" onclick={refreshActiveTab} disabled={activeTabLoading} title="Refresh">
        <RefreshCw size={13} class={activeTabLoading ? 'animate-spin-slow' : ''} />
        <span>Refresh</span>
      </button>

      <!-- Clear view -->
      <button class="log-action-btn log-action-danger" onclick={clearAll} title="Clear view">
        <Trash2 size={13} />
      </button>
    </div>
  </PageHeader>

  <div class="page-content log-viewer" style="display:flex; flex-direction:column; gap:16px;">
    <!-- Tab navigation and Level Selector row -->
    <div style="display: flex; align-items: center; justify-content: space-between; gap: 16px; width: 100%;">
      <TabGroup tabs={LOG_TABS} bind:activeTab={activeTab} onchange={handleTabChange} />

      {#if activeTab === 'journal'}
        <div style="display: flex; align-items: center; gap: 8px; flex-shrink: 0;">
          <span class="log-label" style="font-size: 11px;">Level:</span>
          <Select bind:value={filterPriority} onchange={fetchLogs} style="height: 28px; width: 110px;">
            <option value="all">All Levels</option>
            <option value="3">Error+</option>
            <option value="4">Warning+</option>
            <option value="6">Info+</option>
          </Select>
        </div>
      {/if}
    </div>

    <div class="log-container" bind:this={logContainer}>
      <!-- Tab 1: Journalctl logs -->
      {#if activeTab === 'journal'}
        {#if filteredLogs.length === 0}
          {#if isLoading}
            <div class="empty-state">Fetching logs…</div>
          {:else if hasActiveSearch && collapsedLogs.length > 0}
            <EmptyState 
              icon={FileText}
              title="No Matching Logs"
              description={`No log messages matched "${searchQuery}".`}
              actionLabel="Clear Search"
              onAction={() => { searchQuery = ''; }}
            />
          {:else}
            <EmptyState 
              icon={FileText}
              title="No Logs in Selected Range"
              description="No systemd journal messages found for the current time and level filters."
              actionLabel="Refresh Logs"
              onAction={fetchLogs}
            />
          {/if}
        {:else}
          <Table class="log-table">
            <thead>
              <tr style="border-bottom: 1px solid var(--color-border); font-size: 11px; text-transform: uppercase; color: var(--color-text-secondary); text-align: left;">
                <th style="padding: 8px 12px; font-weight: 600;">Time</th>
                <th style="padding: 8px 12px; font-weight: 600;">Unit / Identifier</th>
                <th style="padding: 8px 12px; font-weight: 600;">Message</th>
                <th style="padding: 8px 12px; font-weight: 600; text-align: right;">Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredLogs as log}
                {@const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel'}
                {@const hasMsg = Boolean(log.MESSAGE && String(log.MESSAGE).trim())}
                <tr class="log-row {getPriorityClass(log.PRIORITY)}">
                  <td class="col-time" style="padding: 8px 12px; white-space: nowrap; color: var(--color-text-muted); font-size: 12px; font-family: var(--font-mono);">{formatTimestamp(log.__REALTIME_TIMESTAMP)}</td>
                  <td class="col-unit" title={unit} style="padding: 8px 12px; font-weight: 600; font-size: 12px; font-family: var(--font-mono); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                    {@html highlight(unit, searchQuery)}
                  </td>
                  <td class="col-msg" style="padding: 8px 12px; color: var(--color-text-secondary); font-size: 12px; font-family: var(--font-mono);">
                    {@html highlight(log.MESSAGE, searchQuery)}
                    {#if (log.count ?? 1) > 1}
                      <span class="repeat-badge">×{log.count}</span>
                    {/if}
                  </td>
                  <td style="padding: 6px 12px; text-align: right; white-space: nowrap;">
                    <div style="display: inline-flex; align-items: center; gap: 4px;">
                      <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        onclick={() => copyLog(log)}
                        title="Copy log entry to clipboard"
                        style="padding: 2px 6px; font-size: 11px;"
                      >
                        <Copy size={11} />
                      </button>
                      {#if aiStore.enabled}
                        <button
                          type="button"
                          class="btn btn-outline btn-xs"
                          disabled={!hasMsg}
                          onclick={() => hasMsg && aiStore.diagnoseLogError(String(log.MESSAGE), unit)}
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
            </tbody>
          </Table>
        {/if}

      <!-- Tab 2: Auth Events -->
      {:else if activeTab === 'auth'}
        {#if authError && authError.includes('Root privileges')}
          <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; display: flex; align-items: center; gap: 10px;">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div>
              <div style="font-weight: 600; font-size: 13px; color: var(--color-error);">Root Access Required</div>
              <div style="font-size: 11px; color: var(--color-text-secondary); margin-top: 2px;">Enable Root mode via the <strong>Root: Off</strong> button in the status bar to read authentication logs.</div>
            </div>
          </div>
        {/if}
        {#if filteredAuthEvents.length === 0}
          <div class="empty-state">
            {#if loadingAuth}
              Fetching authentication logs…
            {:else if authError && authError.includes('Root privileges')}
              Enable Root mode in the status bar to view authentication logs.
            {:else if hasActiveSearch}
              No auth events match <strong style="color:var(--color-text-primary); margin-left:4px;">"{searchQuery}"</strong>
            {:else}
              No auth events recorded.
            {/if}
          </div>
        {:else}
          <Table class="log-table">
            <thead>
              <tr style="border-bottom: 1px solid var(--color-border); font-size: 11px; text-transform: uppercase; color: var(--color-text-secondary); text-align: left;">
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuthSort('timestamp')}>
                  Time {authSortKey === 'timestamp' ? (authSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuthSort('user')}>
                  User {authSortKey === 'user' ? (authSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuthSort('event_type')}>
                  Event Type {authSortKey === 'event_type' ? (authSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuthSort('source_ip')}>
                  Source IP {authSortKey === 'source_ip' ? (authSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuthSort('result')}>
                  Result {authSortKey === 'result' ? (authSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600;">Details</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredAuthEvents as ev}
                <tr class="log-row" style="border-bottom: 1px solid rgba(255, 255, 255, 0.03); font-size: 12px; font-family: var(--font-mono);">
                  <td style="padding: 8px 12px; white-space: nowrap; color: var(--color-text-muted);">{ev.timestamp}</td>
                  <td style="padding: 8px 12px; font-weight: 600; color: var(--color-text-primary);">
                    {@html highlight(ev.user, searchQuery)}
                  </td>
                  <td style="padding: 8px 12px;">
                    <Badge variant={ev.event_type.includes('SSH') ? 'info' : (ev.event_type.includes('Sudo') ? 'warning' : 'muted')} style="font-size: 10px;">
                      {@html highlight(ev.event_type, searchQuery)}
                    </Badge>
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary);">
                    {@html highlight(ev.source_ip, searchQuery)}
                  </td>
                  <td style="padding: 8px 12px;">
                    <Badge variant={ev.result === 'Success' ? 'success' : 'error'} style="font-size: 10px;">
                      {ev.result}
                    </Badge>
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary); max-width: 400px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.details}>
                    {@html highlight(ev.details, searchQuery)}
                  </td>
                </tr>
              {/each}
            </tbody>
          </Table>
        {/if}

      <!-- Tab 3: Command Audit -->
      {:else if activeTab === 'audit'}
        {#if auditError && auditError.includes('Root privileges')}
          <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; display: flex; align-items: center; gap: 10px;">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div>
              <div style="font-weight: 600; font-size: 13px; color: var(--color-error);">Root Access Required</div>
              <div style="font-size: 11px; color: var(--color-text-secondary); margin-top: 2px;">Enable Root mode via the <strong>Root: Off</strong> button in the status bar to read audit logs and configure auditd rules.</div>
            </div>
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
            {:else if auditError && auditError.includes('Root privileges')}
              Enable Root mode in the status bar to view command audit logs.
            {:else if hasActiveSearch}
              No commands match <strong style="color:var(--color-text-primary); margin-left:4px;">"{searchQuery}"</strong>
            {:else}
              No audit command logs recorded.
            {/if}
          </div>
        {:else}
          <Table class="log-table">
            <thead>
              <tr style="border-bottom: 1px solid var(--color-border); font-size: 11px; text-transform: uppercase; color: var(--color-text-secondary); text-align: left;">
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuditSort('timestamp')}>
                  Time {auditSortKey === 'timestamp' ? (auditSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuditSort('user')}>
                  User {auditSortKey === 'user' ? (auditSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuditSort('command')}>
                  Command {auditSortKey === 'command' ? (auditSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuditSort('cwd')}>
                  CWD {auditSortKey === 'cwd' ? (auditSortAsc ? '▲' : '▼') : ''}
                </th>
                <th style="padding: 8px 12px; font-weight: 600; cursor: pointer; user-select: none;" onclick={() => toggleAuditSort('result')}>
                  Result {auditSortKey === 'result' ? (auditSortAsc ? '▲' : '▼') : ''}
                </th>
              </tr>
            </thead>
            <tbody>
              {#each filteredAuditLogs as ev}
                <tr class="log-row" style="border-bottom: 1px solid rgba(255, 255, 255, 0.03); font-size: 12px; font-family: var(--font-mono);">
                  <td style="padding: 8px 12px; white-space: nowrap; color: var(--color-text-muted);">{ev.timestamp}</td>
                  <td style="padding: 8px 12px; font-weight: 600; color: var(--color-text-primary);">
                    {@html highlight(ev.user, searchQuery)}
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-accent); max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.command}>
                    {@html highlight(ev.command, searchQuery)}
                  </td>
                  <td style="padding: 8px 12px; color: var(--color-text-secondary); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title={ev.cwd}>
                    {@html highlight(ev.cwd, searchQuery)}
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
        {#if threatsError && threatsError.includes('Root privileges')}
          <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; display: flex; align-items: center; gap: 10px;">
            <ShieldAlert size={18} color="var(--color-error)" style="flex-shrink:0;" />
            <div>
              <div style="font-weight: 600; font-size: 13px; color: var(--color-error);">Root Access Required</div>
              <div style="font-size: 11px; color: var(--color-text-secondary); margin-top: 2px;">Enable Root mode via the <strong>Root: Off</strong> button in the status bar to run threat correlation analysis.</div>
            </div>
          </div>
        {/if}
        {#if filteredThreats.length === 0}
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
            <div style="background:rgba(0,0,0,0.2); border:1px solid var(--color-border); border-radius:10px; padding:16px;">
              <div style="font-size:11px; font-weight:700; text-transform:uppercase; letter-spacing:0.05em; color:var(--color-text-muted); margin-bottom:12px;">
                Active Correlation Rules & Safeguards (6 Active)
              </div>
              <div style="display:grid; grid-template-columns:repeat(auto-fit, minmax(260px, 1fr)); gap:10px;">
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Sudo Password Brute-Force</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Flags users failing sudo credentials ≥ 3 times. (Status: Clean)</div>
                  </div>
                </div>
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">SSH Inbound Attack Defense</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Detects IP-based SSH login dictionary attacks ≥ 5 attempts. (Status: Clean)</div>
                  </div>
                </div>
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">SELinux Disablement Watch</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Alerts immediately if setenforce 0 is executed. (Status: Clean)</div>
                  </div>
                </div>
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Firewall Ruleset Integrity</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Flags flush attacks (iptables -F or nft flush). (Status: Clean)</div>
                  </div>
                </div>
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Credential & Identity Protection</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Monitors unauthorized writes to /etc/passwd & shadow. (Status: Clean)</div>
                  </div>
                </div>
                <div style="background:var(--color-bg-card); border:1px solid var(--color-border); border-radius:8px; padding:10px 12px; display:flex; align-items:flex-start; gap:10px;">
                  <ShieldCheck size={16} color="var(--color-success)" style="margin-top:2px; flex-shrink:0;" />
                  <div>
                    <div style="font-size:12px; font-weight:600; color:var(--color-text-primary);">Direct Root Bypass Checks</div>
                    <div style="font-size:11px; color:var(--color-text-muted);">Flags elevated commands executed without standard sudo. (Status: Clean)</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        {:else}
          <div style="display:flex; flex-direction:column; gap:12px; padding: 8px; box-sizing: border-box; width: 100%;">
            {#each filteredThreats as threat}
              <div style="background: rgba(255, 255, 255, 0.02); border: 1px solid rgba(255, 255, 255, 0.06); border-radius: 8px; padding: 16px; display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; box-sizing: border-box; width: 100%;">
                <div style="display:flex; flex-direction:column; gap:6px;">
                  <div style="display:flex; align-items:center; gap:8px;">
                    <ShieldAlert size={16} color={threat.severity === 'Critical' ? 'var(--color-error)' : 'var(--color-warning)'} />
                    <h3 style="font-size: 14px; font-weight: 700; margin: 0; color: var(--color-text-primary);">
                      {@html highlight(threat.title, searchQuery)}
                    </h3>
                    <Badge variant={threat.severity === 'Critical' ? 'error' : 'warning'} style="font-size: 9px; text-transform: uppercase;">
                      {threat.severity}
                    </Badge>
                    <span style="font-size: 11px; color: var(--color-text-muted); font-family: var(--font-mono);">{threat.timestamp}</span>
                  </div>
                  <p style="font-size: 12px; color: var(--color-text-secondary); margin: 4px 0 0 0; line-height: 1.5;">
                    {@html highlight(threat.description, searchQuery)}
                  </p>
                  <div style="margin-top: 8px;">
                    <Badge variant="muted" style="font-size: 9px; text-transform: uppercase; background: rgba(255,255,255,0.05); color: var(--color-text-muted);">
                      {threat.category}
                    </Badge>
                  </div>
                </div>
                <Button 
                  variant="ghost" 
                  style="font-size: 11px; height: 26px; padding: 0 10px; border-color: rgba(255,255,255,0.15);" 
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

<style>
  /* ── Single unified toolbar strip ───────────────────── */
  .log-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 0 10px;
    width: 100%;
    box-sizing: border-box;
  }

  /* Search icon */
  .log-toolbar :global(.log-search-icon) {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  /* Text input */
  .log-search-input {
    flex: 1;
    min-width: 120px;
    height: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: var(--font-sans);
  }
  .log-search-input::placeholder {
    color: var(--color-text-muted);
  }

  /* Match count badge */
  .log-count-badge {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--color-accent);
    background: var(--color-accent-muted);
    padding: 1px 7px;
    border-radius: 10px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Clear search button */
  .log-clear-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    border: none;
    border-radius: 4px;
    background: var(--color-bg-hover);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .log-clear-btn:hover {
    background: var(--color-active-bg);
    color: var(--color-text-primary);
  }

  /* Separator */
  .log-sep {
    width: 1px;
    height: 16px;
    background: var(--color-border-subtle);
    flex-shrink: 0;
  }

  /* Small label */
  .log-label {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Level select */
  .log-select {
    height: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    appearance: none;
    -webkit-appearance: none;
    flex-shrink: 0;
  }
  .log-select option { background: var(--color-bg-card); color: var(--color-text-primary); }

  /* Date / time inputs */
  .log-dt {
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
  }
  .log-dt::-webkit-calendar-picker-indicator {
    filter: invert(1);
    opacity: 0.45;
    cursor: pointer;
  }
  .log-dt::-webkit-calendar-picker-indicator:hover { opacity: 0.9; }

  /* Custom Range Popover */
  .custom-range-container {
    position: relative;
    display: inline-block;
  }

  .custom-range-popover {
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
  }

  .popover-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .popover-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--color-text-muted);
    letter-spacing: 0.5px;
  }

  /* Time input sits below the DatePicker trigger inside a popover row */
  .popover-row .log-dt {
    width: 100%;
    box-sizing: border-box;
  }

  .popover-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    padding-top: 10px;
  }

  .popover-btn {
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 6px;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
  }

  .apply-btn {
    background: var(--color-accent);
    color: #0f172a;
  }

  .apply-btn:hover {
    background: #00b9cf;
  }

  .cancel-btn {
    background: rgba(255, 255, 255, 0.08);
    color: var(--color-text-secondary);
  }

  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: var(--color-text-primary);
  }

  /* Refresh / trash action buttons */
  .log-action-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border: none;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
  }
  .log-action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
  }
  .log-action-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .log-action-danger { padding: 0 6px; }
  .log-action-danger:hover {
    background: rgba(255, 118, 117, 0.12);
    color: var(--color-error);
  }

  /* ── Log viewer ──────────────────────────────────────── */
  .log-viewer {
    padding: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    height: 100%;
  }

  .log-container {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-card);
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 12px;
  }

  :global(.log-container .table-wrap) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .empty-state {
    display: flex;
    justify-content: center;
    align-items: center;
    height: 100%;
    color: var(--color-text-muted);
    font-size: 13px;
    font-family: var(--font-sans);
  }

  .log-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .log-table thead th {
    position: sticky !important;
    top: 0 !important;
    z-index: 20 !important;
  }

  .log-row {
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  }
  .log-row:hover { background: rgba(255, 255, 255, 0.04); }

  .log-row td {
    padding: 4px 8px;
    vertical-align: top;
  }

  .col-time {
    width: 155px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .col-unit {
    width: 145px;
    color: #8b949e;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .col-msg {
    color: #c9d1d9;
    word-break: break-word;
  }

  /* Priority Colors (Dark Mode Default) */
  .log-error .col-msg  { color: #ff7b72; font-weight: 500; }
  .log-error .col-unit { color: #f87171 !important; }
  .log-warn  .col-msg  { color: #fbbf24; }
  .log-warn  .col-unit { color: #f59e0b !important; }
  .log-info  .col-msg  { color: #38bdf8; }
  .log-info  .col-unit { color: #38bdf8 !important; }
  .log-debug .col-msg  { color: #94a3b8; }
  .log-debug .col-unit { color: #94a3b8 !important; }

  /* Priority Colors for Light Mode */
  :global(html.light-mode) .log-error .col-unit { color: #dc2626 !important; }
  :global(html.light-mode) .log-warn  .col-unit { color: #d97706 !important; }
  :global(html.light-mode) .log-info  .col-unit { color: #0284c7 !important; }
  :global(html.light-mode) .log-debug .col-unit { color: #64748b !important; }

  :global(html.light-mode) .log-error .col-msg { color: #dc2626; }
  :global(html.light-mode) .log-warn  .col-msg { color: #b45309; }
  :global(html.light-mode) .log-info  .col-msg { color: #334155; }
  :global(html.light-mode) .log-debug .col-msg { color: #64748b; }

  .repeat-badge {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-secondary);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    margin-left: 8px;
    font-weight: bold;
    display: inline-block;
  }

  .live-following-btn {
    background: rgba(16, 185, 129, 0.15) !important;
    color: var(--color-success) !important;
    border: 1px solid rgba(16, 185, 129, 0.3) !important;
  }

  .live-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-text-muted);
    display: inline-block;
  }

  .live-status-dot.pulsing {
    background: var(--color-success);
    box-shadow: 0 0 8px var(--color-success);
    animation: pulse-live 1.2s infinite;
  }

  @keyframes pulse-live {
    0% { transform: scale(0.95); opacity: 0.8; }
    50% { transform: scale(1.3); opacity: 1; box-shadow: 0 0 10px var(--color-success); }
    100% { transform: scale(0.95); opacity: 0.8; }
  }

  .log-row.log-error {
    border-left: 3px solid var(--color-error);
  }
  .log-row.log-warn {
    border-left: 3px solid var(--color-warning);
  }
  .log-row.log-info {
    border-left: 3px solid var(--color-accent);
  }
  .log-row.log-debug {
    border-left: 3px solid var(--color-border);
  }
</style>
