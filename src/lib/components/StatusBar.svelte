<script lang="ts">
  import { Activity, CheckCircle, XCircle, Loader } from '@lucide/svelte';
  import { statusStore } from '../stores/status.svelte.ts';

  function formatTime(date: Date): string {
    return date.toLocaleTimeString('en-US', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  }
</script>

<footer class="status-bar" role="status" aria-live="polite">
  <div class="status-left">
    <div class="status-indicator" class:busy={statusStore.busy}>
      {#if statusStore.busy}
        <Loader size={12} class="spin-icon" />
        <span class="status-text busy">{statusStore.busyLabel}</span>
      {:else if statusStore.lastEntry}
        {#if statusStore.lastEntry.success}
          <CheckCircle size={12} style="color: var(--color-success)" />
        {:else}
          <XCircle size={12} style="color: var(--color-error)" />
        {/if}
        <span class="status-text" class:success={statusStore.lastEntry.success} class:fail={!statusStore.lastEntry.success}>
          {statusStore.lastEntry.command}
        </span>
      {:else}
        <Activity size={12} style="color: var(--color-text-muted)" />
        <span class="status-text muted">Ready</span>
      {/if}
    </div>
  </div>

  <div class="status-right">
    {#if statusStore.lastEntry}
      <span class="exit-code" class:ok={statusStore.lastEntry.success} class:fail={!statusStore.lastEntry.success}>
        exit {statusStore.lastEntry.exitCode ?? '—'}
      </span>
      <span class="timestamp">{formatTime(statusStore.lastEntry.timestamp)}</span>
    {:else}
      <span class="version">Fedora 40+ / RHEL 9+</span>
    {/if}
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    padding: 0 16px;
    background: var(--color-bg-surface);
    border-top: 1px solid var(--color-border);
    font-size: 11px;
    font-family: var(--font-mono);
    flex-shrink: 0;
  }

  .status-left, .status-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .status-text {
    color: var(--color-text-muted);
    max-width: 400px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-text.busy   { color: var(--color-info); }
  .status-text.success { color: var(--color-success); }
  .status-text.fail    { color: var(--color-error); }
  .status-text.muted   { color: var(--color-text-muted); }

  .exit-code {
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 10px;
  }
  .exit-code.ok   { background: var(--color-success-muted); color: var(--color-success); }
  .exit-code.fail { background: var(--color-error-muted); color: var(--color-error); }

  .timestamp, .version {
    color: var(--color-text-muted);
  }

  :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--color-info);
  }
</style>
