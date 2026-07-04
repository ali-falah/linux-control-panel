<script lang="ts">
  import { CheckCircle2, XCircle, Loader, Minus } from '@lucide/svelte';
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
  <!-- Left: last command -->
  <div class="status-left">
    {#if statusStore.busy}
      <div class="status-indicator busy">
        <Loader size={11} class="spin-icon" />
        <span class="cmd-text">{statusStore.busyLabel}</span>
      </div>
    {:else if statusStore.lastEntry}
      <div class="status-indicator">
        {#if statusStore.lastEntry.success}
          <CheckCircle2 size={11} class="icon-ok" />
        {:else}
          <XCircle size={11} class="icon-fail" />
        {/if}
        <span class="cmd-text" class:ok={statusStore.lastEntry.success} class:fail={!statusStore.lastEntry.success}>
          {statusStore.lastEntry.command}
        </span>
      </div>
    {:else}
      <div class="status-indicator">
        <Minus size={11} class="icon-idle" />
        <span class="cmd-text idle">Ready</span>
      </div>
    {/if}
  </div>

  <!-- Right: exit code + timestamp pill -->
  <div class="status-right">
    {#if statusStore.lastEntry}
      <span class="pill {statusStore.lastEntry.success ? 'pill-ok' : 'pill-fail'}">
        exit {statusStore.lastEntry.exitCode ?? '—'}
      </span>
      <span class="ts-pill">
        {formatTime(statusStore.lastEntry.timestamp)}
      </span>
    {:else}
      <span class="ts-pill muted">Fedora / RHEL</span>
    {/if}
  </div>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    padding: 0 16px;
    background: rgba(37, 40, 54, 0.4);
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 11px;
    font-family: var(--font-mono);
    flex-shrink: 0;
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    gap: 12px;
  }

  .status-left, .status-right {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .cmd-text {
    color: var(--color-text-muted);
    max-width: 480px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cmd-text.ok   { color: var(--color-success); }
  .cmd-text.fail { color: var(--color-error); }
  .cmd-text.idle { color: var(--color-text-muted); }

  /* pill badges on the right */
  .pill {
    padding: 1px 7px;
    border-radius: 20px;
    font-size: 10px;
    font-weight: 600;
    white-space: nowrap;
    letter-spacing: 0.02em;
  }
  .pill-ok   { background: var(--color-success-muted); color: var(--color-success); }
  .pill-fail { background: var(--color-error-muted);   color: var(--color-error); }

  .ts-pill {
    padding: 1px 7px;
    border-radius: 20px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--color-text-muted);
    font-size: 10px;
    white-space: nowrap;
  }
  .ts-pill.muted { color: var(--color-text-muted); }

  :global(.spin-icon) {
    animation: spin 1s linear infinite;
    color: var(--color-info);
    flex-shrink: 0;
  }
  :global(.icon-ok)   { color: var(--color-success); flex-shrink: 0; }
  :global(.icon-fail) { color: var(--color-error); flex-shrink: 0; }
  :global(.icon-idle) { color: var(--color-text-muted); flex-shrink: 0; }
</style>
