<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Trash2, Filter, FileText, RefreshCw } from '@lucide/svelte';
  import PageHeader from '../components/PageHeader.svelte';
  import Input from '../components/ui/Input.svelte';
  import Select from '../components/ui/Select.svelte';
  import Button from '../components/ui/Button.svelte';
  import { statusStore } from '../stores/status.svelte.ts';

  let logs = $state<any[]>([]);
  let isLoading = $state(false);
  let filterUnit = $state('');
  let filterPriority = $state('all');
  let filterDate = $state('');
  let filterTime = $state('00:00');
  let logContainer: HTMLElement;

  let dateInputRef: HTMLInputElement;
  let timeInputRef: HTMLInputElement;

  function handleDateTimeChange(e: Event) {
    if (e.currentTarget instanceof HTMLInputElement) {
      e.currentTarget.blur(); // Force close the native WebKitGTK popup
    }
    fetchLogs();
  }

  function handleWindowClick(e: MouseEvent) {
    if (dateInputRef && e.target instanceof Node && !dateInputRef.contains(e.target)) {
      dateInputRef.blur();
    }
    if (timeInputRef && e.target instanceof Node && !timeInputRef.contains(e.target)) {
      timeInputRef.blur();
    }
  }

  async function fetchLogs() {
    isLoading = true;
    statusStore.setBusy('Fetching journal logs...');
    try {
      const unitF = filterUnit.trim() !== '' ? filterUnit.trim() : null;
      const prioF = filterPriority !== 'all' ? parseInt(filterPriority) : null;
      const sinceF = filterDate ? `${filterDate} ${filterTime || '00:00'}:00` : null;
      
      const lines = await invoke<string[]>('get_journal_logs', {
        unitFilter: unitF,
        priority: prioF,
        sinceFilter: sinceF,
      });

      logs = lines.map(line => {
        try {
          return JSON.parse(line);
        } catch {
          return null;
        }
      }).filter(Boolean);
      
      // Auto-scroll to bottom after render
      setTimeout(() => {
        if (logContainer) {
          logContainer.scrollTop = logContainer.scrollHeight;
        }
      }, 50);

      statusStore.setLastCommand(`journalctl -n 100 -o json ${unitF ? '-u '+unitF : ''} ${prioF ? '-p '+prioF : ''} ${sinceF ? '--since="'+sinceF+'"' : ''}`, 0, true);
    } catch (e) {
      console.error("Error fetching journal logs:", e);
      statusStore.setLastCommand(`journalctl`, 1, false);
    } finally {
      isLoading = false;
      statusStore.clearBusy();
    }
  }

  function clearLogs() {
    logs = [];
  }

  onMount(() => {
    fetchLogs();
  });

  function formatTimestamp(us: string) {
    if (!us) return '';
    const date = new Date(parseInt(us) / 1000);
    const dateStr = date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    const timeStr = date.toLocaleTimeString('en-US', { hour12: true, hour: 'numeric', minute: '2-digit', second: '2-digit' });
    return `${dateStr}, ${timeStr}`;
  }

  function getPriorityClass(prio: string | number) {
    const p = parseInt(prio as string);
    if (isNaN(p)) return 'log-info';
    if (p <= 3) return 'log-error';
    if (p === 4) return 'log-warn';
    if (p >= 7) return 'log-debug';
    return 'log-info';
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="module-page">
  <PageHeader title="Journal Logs" subtitle="Systemd journal log viewer" icon={FileText}>
    <div class="toolbar">
      <div class="filter-group">
        <Filter size={14} class="text-muted" />
        <Input 
          placeholder="Service (e.g. sshd)" 
          bind:value={filterUnit} 
          onkeydown={(e) => e.key === 'Enter' && fetchLogs()}
          style="width: 160px; height: 32px; background: transparent; border: none; padding-left: 4px;"
        />
        <Select bind:value={filterPriority} onchange={fetchLogs} style="height: 32px; background: transparent; border: none; border-left: 1px solid var(--color-border); border-right: 1px solid var(--color-border); border-radius: 0;">
          <option value="all">All Levels</option>
          <option value="3">Error & Above</option>
          <option value="4">Warning & Above</option>
          <option value="6">Info & Above</option>
        </Select>
        <div style="display:flex; align-items:center; padding-left: 8px; padding-right: 4px;">
          <span class="text-muted" style="font-size:12px; margin-right:6px;">Since:</span>
          <input 
            type="date" 
            bind:this={dateInputRef}
            bind:value={filterDate}
            onchange={handleDateTimeChange}
            class="datetime-input"
          />
          <input 
            type="time" 
            bind:this={timeInputRef}
            bind:value={filterTime}
            onchange={handleDateTimeChange}
            class="datetime-input"
            style="margin-left:4px;"
          />
        </div>
      </div>

      <div class="actions-group">
        <Button variant="ghost" onclick={fetchLogs} disabled={isLoading}>
          <RefreshCw size={14} class={isLoading ? 'animate-spin-slow' : ''} /> Refresh
        </Button>
        <Button variant="ghost" class="text-error hover-bg-error-light" onclick={clearLogs} title="Clear View">
          <Trash2 size={14} />
        </Button>
      </div>
    </div>
  </PageHeader>

  <div class="page-content log-viewer">
    <div class="log-container" bind:this={logContainer}>
      {#if logs.length === 0}
        <div class="empty-state">
          {#if isLoading}
            Fetching logs...
          {:else}
            No logs found.
          {/if}
        </div>
      {/if}
      <table class="log-table">
        <tbody>
          {#each logs as log}
            <tr class="log-row {getPriorityClass(log.PRIORITY)}">
              <td class="col-time">{formatTimestamp(log.__REALTIME_TIMESTAMP)}</td>
              <td class="col-unit">{log._SYSTEMD_UNIT || log.SYSLOG_IDENTIFIER || 'kernel'}</td>
              <td class="col-msg">{log.MESSAGE}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    gap: 16px;
    align-items: center;
  }

  .filter-group {
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(0, 0, 0, 0.2);
    padding: 0 4px 0 12px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
  }

  .datetime-input {
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 13px;
    font-family: var(--font-sans);
    outline: none;
    padding: 4px;
    color-scheme: dark;
  }
  
  .datetime-input::-webkit-calendar-picker-indicator {
    filter: invert(1);
    opacity: 0.6;
    cursor: pointer;
  }
  .datetime-input::-webkit-calendar-picker-indicator:hover {
    opacity: 1;
  }

  .actions-group {
    display: flex;
    gap: 6px;
  }

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
    font-family: var(--font-primary);
    font-size: 14px;
  }

  .log-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .log-row {
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  }

  .log-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .log-row td {
    padding: 4px 8px;
    vertical-align: top;
    word-break: break-all;
  }

  .col-time {
    width: 160px;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .col-unit {
    width: 140px;
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
  .log-error .col-msg { color: #ff7b72; font-weight: 500; }
  .log-error .col-unit { color: #ff7b72; }
  
  .log-warn .col-msg { color: #d29922; }
  .log-warn .col-unit { color: #d29922; }
  
  .log-debug .col-msg { color: #484f58; }
  .log-debug .col-unit { color: #484f58; }
</style>
