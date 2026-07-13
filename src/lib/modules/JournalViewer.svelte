<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { FileText, RefreshCw, Search, X, Trash2 } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import { statusStore } from '../stores/status.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  let logs = $state<any[]>([]);
  let isLoading = $state(false);

  // Backend filters (re-fetch journalctl)
  let filterPriority = $state('all');
  let filterDate = $state('');
  let filterTime = $state('00:00');

  // Client-side live search (instant, no fetch)
  let searchQuery = $state('');

  let logContainer: HTMLElement;
  let dateInputRef: HTMLInputElement;
  let timeInputRef: HTMLInputElement;
  let searchInputRef: HTMLInputElement;

  function handleDateTimeChange(e: Event) {
    if (e.currentTarget instanceof HTMLInputElement) {
      const input = e.currentTarget;
      input.blur();
      input.disabled = true;
      setTimeout(() => { input.disabled = false; }, 100);
    }
    fetchLogs();
  }

  async function fetchLogs() {
    isLoading = true;
    statusStore.setBusy('Fetching journal logs...');
    try {
      const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
      const sinceF = filterDate ? `${filterDate} ${filterTime || '00:00'}:00` : null;

      const lines = await invoke<string[]>('get_journal_logs', {
        unitFilter: null,
        priority: prioF,
        sinceFilter: sinceF,
      });

      logs = lines.map(line => {
        try { return JSON.parse(line); } catch { return null; }
      }).filter(Boolean);

      setTimeout(() => {
        if (logContainer) logContainer.scrollTop = logContainer.scrollHeight;
      }, 50);

      statusStore.setLastCommand(
        `journalctl -n 100 -o json ${prioF ? '-p ' + prioF : ''} ${sinceF ? '--since="' + sinceF + '"' : ''}`,
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

  function clearAll() {
    logs = [];
    searchQuery = '';
  }

  onMount(() => {
    if (uiStore.preAppliedJournalPriority && uiStore.preAppliedJournalPriority !== 'all') {
      filterPriority = uiStore.preAppliedJournalPriority;
      uiStore.preAppliedJournalPriority = 'all';
    }
    fetchLogs();
  });

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

  // Instant client-side filter — searches message + unit simultaneously
  let filteredLogs = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return collapsedLogs;
    return collapsedLogs.filter(log => {
      const msg = (log.MESSAGE || '').toLowerCase();
      const unit = (log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel').toLowerCase();
      return msg.includes(q) || unit.includes(q);
    });
  });

  let hasActiveSearch = $derived(searchQuery.trim().length > 0);
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
        type="text"
        class="log-search-input"
        placeholder="Search by message or service…"
        autocomplete="off"
        spellcheck={false}
      />

      {#if hasActiveSearch}
        <span class="log-count-badge">{filteredLogs.length}/{collapsedLogs.length}</span>
        <button class="log-clear-btn" onclick={() => { searchQuery = ''; searchInputRef?.focus(); }} title="Clear">
          <X size={12} />
        </button>
      {/if}

      <span class="log-sep"></span>

      <!-- Level -->
      <select class="log-select" bind:value={filterPriority} onchange={fetchLogs}>
        <option value="all">All Levels</option>
        <option value="3">Error+</option>
        <option value="4">Warning+</option>
        <option value="6">Info+</option>
      </select>

      <span class="log-sep"></span>

      <!-- Since date/time -->
      <span class="log-label">Since:</span>
      <input
        type="date"
        bind:this={dateInputRef}
        value={filterDate}
        onchange={(e) => { filterDate = e.currentTarget.value; handleDateTimeChange(e); }}
        class="log-dt"
      />
      <input
        type="time"
        bind:this={timeInputRef}
        value={filterTime}
        onchange={(e) => { filterTime = e.currentTarget.value; handleDateTimeChange(e); }}
        class="log-dt"
      />

      <span class="log-sep"></span>

      <!-- Refresh -->
      <button class="log-action-btn" onclick={fetchLogs} disabled={isLoading} title="Refresh">
        <RefreshCw size={13} class={isLoading ? 'animate-spin-slow' : ''} />
        <span>Refresh</span>
      </button>

      <!-- Clear view -->
      <button class="log-action-btn log-action-danger" onclick={clearAll} title="Clear view">
        <Trash2 size={13} />
      </button>
    </div>
  </PageHeader>

  <div class="page-content log-viewer">
    <div class="log-container" bind:this={logContainer}>
      {#if filteredLogs.length === 0}
        <div class="empty-state">
          {#if isLoading}
            Fetching logs…
          {:else if hasActiveSearch && collapsedLogs.length > 0}
            No logs match <strong style="color:var(--color-text-primary); margin-left:4px;">"{searchQuery}"</strong>
          {:else}
            No logs found.
          {/if}
        </div>
      {/if}
      <table class="log-table">
        <tbody>
          {#each filteredLogs as log}
            {@const unit = log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel'}
            <tr class="log-row {getPriorityClass(log.PRIORITY)}">
              <td class="col-time">{formatTimestamp(log.__REALTIME_TIMESTAMP)}</td>
              <td class="col-unit" title={unit}>
                {@html highlight(unit, searchQuery)}
              </td>
              <td class="col-msg">
                {@html highlight(log.MESSAGE, searchQuery)}
                {#if (log.count ?? 1) > 1}
                  <span class="repeat-badge">×{log.count}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
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
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.07);
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
    color: #a29bfe;
    background: rgba(108, 92, 231, 0.12);
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
    background: rgba(255, 255, 255, 0.07);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .log-clear-btn:hover {
    background: rgba(255, 255, 255, 0.14);
    color: var(--color-text-primary);
  }

  /* Separator */
  .log-sep {
    width: 1px;
    height: 16px;
    background: rgba(255, 255, 255, 0.08);
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
  .log-select option { background: #1a1b26; color: var(--color-text-primary); }

  /* Date / time inputs */
  .log-dt {
    height: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: var(--color-text-secondary);
    font-size: 12px;
    font-family: var(--font-sans);
    padding: 0;
    color-scheme: dark;
    flex-shrink: 0;
  }
  .log-dt::-webkit-calendar-picker-indicator {
    filter: invert(1);
    opacity: 0.45;
    cursor: pointer;
  }
  .log-dt::-webkit-calendar-picker-indicator:hover { opacity: 0.9; }

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
    overflow-y: auto;
    background: #111216;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 12px;
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

  /* Priority Colors */
  .log-error .col-msg  { color: #ff7b72; font-weight: 500; }
  .log-error .col-unit { color: #ff7b72; }
  .log-warn  .col-msg  { color: #d29922; }
  .log-warn  .col-unit { color: #d29922; }
  .log-info  .col-msg  { color: #58a6ff; }
  .log-info  .col-unit { color: #58a6ff; }
  .log-debug .col-msg  { color: #484f58; }
  .log-debug .col-unit { color: #484f58; }

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

  /* Search highlight */
  :global(.hl) {
    background: rgba(253, 203, 110, 0.3);
    color: #fdcb6e;
    border-radius: 2px;
    padding: 0 1px;
    font-weight: 600;
  }
</style>
