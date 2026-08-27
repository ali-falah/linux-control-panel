<script lang="ts">
  import Toast from './Toast.svelte';
  import { uiStore } from '../stores/ui.svelte.ts';
</script>

<div class="toast-container" role="region" aria-label="Notifications" aria-live="polite">
  {#each uiStore.toasts as toast (toast.id)}
    <Toast {toast} />
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    bottom: 48px;
    right: 20px;
    z-index: 999999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }

  .toast-container :global(.toast) {
    pointer-events: auto;
  }
</style>
