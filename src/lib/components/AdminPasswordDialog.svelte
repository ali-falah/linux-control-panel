<script lang="ts">
  /**
   * Asks the user for the administrator password.
   *
   * The password is only kept in the local component state until it is
   * handed over to the backend, which pipes it into `sudo`'s stdin. It is
   * never logged, never stored and never sent to the frontend store.
   */
  import { X, ShieldCheck } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
  import { fade, fly } from 'svelte/transition';
  import { portal } from '../actions/portal.ts';
  import Input from './ui/Input.svelte';
  import Button from './ui/Button.svelte';

  let password = $state('');

  function close() {
    password = '';
    uiStore.cancelAdminPassword();
  }

  function submit() {
    const value = password;
    password = '';
    uiStore.submitAdminPassword(value);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  }
</script>

<svelte:window onkeydown={(e) => { if (uiStore.passwordDialog.isOpen && e.key === 'Escape') close(); }} />

{#if uiStore.passwordDialog.isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div use:portal class="confirm-dialog-backdrop" transition:fade={{ duration: 150 }}>
    <!-- Backdrop dismiss -->
    <div
      class="backdrop"
      onclick={() => close()}
    ></div>

    <div class="modal" style="width: 400px; max-width: calc(100vw - 32px); position: relative; z-index: 100001" transition:fly={{ y: 20, duration: 250 }}>
      <div class="dialog-header">
        <div class="dialog-icon">
          <ShieldCheck size={20} />
        </div>
        <h2 class="dialog-title">{uiStore.passwordDialog.title}</h2>
        <button class="close-btn btn btn-icon btn-ghost" onclick={() => close()}>
          <X size={16} />
        </button>
      </div>

      <p class="dialog-message">{uiStore.passwordDialog.message}</p>

      <div style="margin-top: 12px;">
        <Input
          label="Administrator password"
          type="password"
          bind:value={password}
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          placeholder="Your password"
          onkeydown={handleKeydown}
        />
      </div>

      <div class="dialog-actions">
        <button class="btn btn-ghost" onclick={() => close()}>
          Cancel
        </button>
        <Button variant="primary" size="sm" onclick={submit} disabled={password.length === 0}>
          Authenticate
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(2px);
  }
  .backdrop {
    position: absolute;
    inset: 0;
  }
  .modal {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 20px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  .dialog-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .dialog-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: rgba(37, 99, 235, 0.15);
    color: var(--color-accent);
    flex-shrink: 0;
  }
  .dialog-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
    flex: 1;
  }
  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 6px;
  }
  .close-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }
  .dialog-message {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--color-text-secondary);
    white-space: pre-line;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }
</style>
