<script lang="ts">
  import { untrack } from 'svelte';
  import { X, AlertTriangle } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { fade, fly } from 'svelte/transition';
  import { portal } from '../actions/portal.ts';

  let dialogEl = $state<HTMLElement | null>(null);

  // Focus moves into the dialog when it opens and back to whatever opened it
  // when it closes, so keyboard users are never left behind the backdrop.
  $effect(() => {
    if (!uiStore.confirmDialog.isOpen) return;
    const opener =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    untrack(() => dialogEl)
      ?.querySelector<HTMLElement>('.dialog-actions .btn-ghost')
      ?.focus();
    return () => opener?.focus?.();
  });

  // Keep Tab inside the dialog while it is modal.
  function trapTab(event: KeyboardEvent) {
    if (event.key !== 'Tab') return;
    const focusable = dialogEl?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    );
    if (!focusable || focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  async function handleConfirm() {
    const callback = uiStore.confirmDialog.onConfirm;
    uiStore.closeConfirm();
    if (callback) {
      await callback();
    }
  }
</script>

<svelte:window onkeydown={(e) => { if (uiStore.confirmDialog.isOpen && e.key === 'Escape') uiStore.closeConfirm(); }} />

{#if uiStore.confirmDialog.isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div use:portal class="confirm-dialog-backdrop" transition:fade={{ duration: 150 }}>
    <!-- Backdrop dismiss -->
    <div 
      class="backdrop" 
      onclick={() => uiStore.closeConfirm()}
    ></div>
    
    <div
      class="modal"
      style="width: 400px; max-width: calc(100vw - 32px); position: relative; z-index: 100001"
      transition:fly={{ y: 20, duration: 250 }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      tabindex="-1"
      bind:this={dialogEl}
      onkeydown={trapTab}
    >
      <div class="dialog-header">
        {#if uiStore.confirmDialog.danger}
          <div class="dialog-icon danger">
            <AlertTriangle size={20} />
          </div>
        {/if}
        <h2 id="confirm-title" class="dialog-title">{uiStore.confirmDialog.title}</h2>
        <button
          class="close-btn btn btn-icon btn-ghost"
          aria-label="Close"
          onclick={() => uiStore.closeConfirm()}
        >
          <X size={16} />
        </button>
      </div>

      <p id="confirm-message" class="dialog-message">{uiStore.confirmDialog.message}</p>

      <div class="dialog-actions">
        <button class="btn btn-ghost" onclick={() => uiStore.closeConfirm()}>
          Cancel
        </button>
        <button
          class="btn {uiStore.confirmDialog.danger ? 'btn-danger' : 'btn-primary'}"
          onclick={handleConfirm}
        >
          Confirm
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99999;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
  }

  .dialog-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  .dialog-icon {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog-icon.danger {
    background: var(--color-error-muted);
    color: var(--color-error);
  }

  .dialog-title {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
    color: var(--color-text-primary);
    flex: 1;
  }

  .close-btn {
    margin-left: auto;
  }

  .dialog-message {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 0 0 24px 0;
    white-space: pre-wrap;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
