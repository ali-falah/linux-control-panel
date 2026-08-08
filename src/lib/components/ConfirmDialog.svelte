<script lang="ts">
  import { X, AlertTriangle } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { fade, fly } from 'svelte/transition';

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
  <div class="confirm-dialog-backdrop" transition:fade={{ duration: 150 }}>
    <!-- Backdrop dismiss -->
    <div 
      class="backdrop" 
      onclick={() => uiStore.closeConfirm()}
    ></div>
    
    <div class="modal" style="width: 400px; max-width: calc(100vw - 32px); position: relative; z-index: 100001" transition:fly={{ y: 20, duration: 250 }}>
      <div class="dialog-header">
        {#if uiStore.confirmDialog.danger}
          <div class="dialog-icon danger">
            <AlertTriangle size={20} />
          </div>
        {/if}
        <h2 id="confirm-title" class="dialog-title">{uiStore.confirmDialog.title}</h2>
        <button class="close-btn btn btn-icon btn-ghost" onclick={() => uiStore.closeConfirm()}>
          <X size={16} />
        </button>
      </div>

      <p class="dialog-message">{uiStore.confirmDialog.message}</p>

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
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
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
