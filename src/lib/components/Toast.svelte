<script lang="ts">
  import { CheckCircle, XCircle, AlertTriangle, Info, X } from '@lucide/svelte';
  import type { Toast } from '../stores/ui.svelte.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  let { toast }: { toast: Toast } = $props();

  const icons = {
    success: CheckCircle,
    error: XCircle,
    warning: AlertTriangle,
    info: Info,
  };

  const styles = {
    success: 'toast-success',
    error: 'toast-error',
    warning: 'toast-warning',
    info: 'toast-info',
  };

  let dismissed = $state(false);

  function dismiss() {
    dismissed = true;
    setTimeout(() => uiStore.removeToast(toast.id), 300);
  }
</script>

<div
  class="toast {styles[toast.type]}"
  class:dismissed
  role="alert"
  aria-live="assertive"
>
  <div class="toast-icon">
    {#if toast.type === 'success'}<CheckCircle size={16} />
    {:else if toast.type === 'error'}<XCircle size={16} />
    {:else if toast.type === 'warning'}<AlertTriangle size={16} />
    {:else}<Info size={16} />
    {/if}
  </div>
  <span class="toast-message">{toast.message}</span>
  <button class="toast-close" onclick={dismiss} aria-label="Dismiss notification">
    <X size={14} />
  </button>
</div>

<style>
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid transparent;
    box-shadow: 0 8px 32px rgba(0,0,0,0.4), 0 2px 8px rgba(0,0,0,0.2);
    max-width: 380px;
    animation: toastIn 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) both;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    transition: opacity 0.3s ease, transform 0.3s ease;
  }

  .toast.dismissed {
    animation: toastOut 0.3s ease both;
  }

  .toast-success {
    background: rgba(15, 22, 15, 0.95);
    border-color: rgba(34, 197, 94, 0.25);
    color: var(--color-success);
  }
  .toast-error {
    background: rgba(22, 10, 10, 0.95);
    border-color: rgba(248, 113, 113, 0.25);
    color: var(--color-error);
  }
  .toast-warning {
    background: rgba(22, 18, 10, 0.95);
    border-color: rgba(251, 191, 36, 0.25);
    color: var(--color-warning);
  }
  .toast-info {
    background: rgba(10, 16, 22, 0.95);
    border-color: rgba(56, 189, 248, 0.25);
    color: var(--color-info);
  }

  .toast-icon {
    flex-shrink: 0;
    padding-top: 1px;
  }

  .toast-message {
    flex: 1;
    font-size: 13px;
    line-height: 1.5;
    color: var(--color-text-primary);
    word-break: break-word;
  }

  .toast-close {
    flex-shrink: 0;
    padding: 2px;
    border: none;
    background: none;
    cursor: pointer;
    color: var(--color-text-muted);
    border-radius: 4px;
    display: flex;
    align-items: center;
    transition: color 0.15s;
  }

  .toast-close:hover {
    color: var(--color-text-primary);
  }
</style>
