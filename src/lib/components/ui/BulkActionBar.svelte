<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X, CheckSquare, Layers } from '@lucide/svelte';
  import { portal } from '../../actions/portal.ts';

  interface Props {
    selectedCount: number;
    itemLabel?: string;
    onclear: () => void;
    children?: Snippet;
  }

  let {
    selectedCount = 0,
    itemLabel = 'items',
    onclear,
    children
  }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (selectedCount > 0 && e.key === 'Escape') {
      onclear();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if selectedCount > 0}
  <aside 
    use:portal
    class="floating-bulk-bar" 
    role="toolbar" 
    aria-label="Bulk actions toolbar"
    aria-live="polite"
  >
    <!-- Left: Selected items counter & clear button -->
    <div class="bulk-counter-group">
      <div class="counter-badge">
        <CheckSquare size={13} class="text-accent" />
        <span class="count-num">{selectedCount}</span>
        <span class="count-label">{selectedCount === 1 ? itemLabel.replace(/s$/, '') : itemLabel} selected</span>
      </div>
      <button 
        type="button" 
        class="btn-clear-selection" 
        onclick={onclear}
        title="Deselect all (Esc)"
      >
        <X size={13} />
        <span>Deselect</span>
      </button>
    </div>

    <!-- Divider -->
    <div class="bulk-divider" aria-hidden="true"></div>

    <!-- Right: Custom Action Buttons -->
    <div class="bulk-actions-group">
      {#if children}
        {@render children()}
      {/if}
    </div>
  </aside>
{/if}

<style>
  .floating-bulk-bar {
    position: fixed;
    bottom: 36px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 9000;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    background: rgba(10, 22, 38, 0.88);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
    border: 1.5px solid rgba(0, 218, 243, 0.35);
    border-radius: 9999px;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5), 0 0 24px rgba(0, 218, 243, 0.18);
    animation: slideUpFloat 0.22s cubic-bezier(0.34, 1.56, 0.64, 1) both;
    max-width: calc(100vw - 48px);
  }

  @keyframes slideUpFloat {
    from {
      opacity: 0;
      transform: translate(-50%, 28px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0) scale(1);
    }
  }

  .bulk-counter-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .counter-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(0, 218, 243, 0.12);
    border-radius: 9999px;
    font-size: 12px;
    color: #e2e8f0;
    font-weight: 500;
  }

  .count-num {
    font-weight: 700;
    color: var(--color-accent, #00daf3);
  }

  .count-label {
    color: #cbd5e1;
    font-size: 11.5px;
  }

  .btn-clear-selection {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 9999px;
    padding: 3px 8px;
    font-size: 11px;
    font-weight: 500;
    color: #94a3b8;
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .btn-clear-selection:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.25);
  }

  .bulk-divider {
    width: 1px;
    height: 20px;
    background: rgba(255, 255, 255, 0.15);
  }

  .bulk-actions-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.floating-bulk-bar .btn-bulk-action) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 9999px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: none;
    transition: all 0.15s ease;
    font-family: inherit;
    white-space: nowrap;
  }

  :global(.floating-bulk-bar .btn-bulk-primary) {
    background: var(--color-accent, #00daf3);
    color: #020914;
    box-shadow: 0 0 12px rgba(0, 218, 243, 0.35);
  }

  :global(.floating-bulk-bar .btn-bulk-primary:hover) {
    filter: brightness(1.1);
    transform: translateY(-1px);
  }

  :global(.floating-bulk-bar .btn-bulk-danger) {
    background: rgba(225, 29, 72, 0.25);
    border: 1px solid rgba(244, 63, 94, 0.5);
    color: #fda4af;
    box-shadow: 0 0 10px rgba(225, 29, 72, 0.2);
  }

  :global(.floating-bulk-bar .btn-bulk-danger:hover) {
    background: #e11d48;
    color: #ffffff;
    border-color: #f43f5e;
    box-shadow: 0 0 16px rgba(225, 29, 72, 0.5);
  }

  :global(.floating-bulk-bar .btn-bulk-outline) {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.18);
    color: #f1f5f9;
  }

  :global(.floating-bulk-bar .btn-bulk-outline:hover) {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.3);
  }

  /* Light Theme Adaptive Override */
  :global(html.light-mode) .floating-bulk-bar {
    background: rgba(255, 255, 255, 0.94);
    border-color: rgba(0, 102, 204, 0.3);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.18), 0 0 20px rgba(0, 102, 204, 0.12);
  }

  :global(html.light-mode) .counter-badge {
    background: rgba(0, 102, 204, 0.08);
    color: #1e293b;
  }

  :global(html.light-mode) .count-label {
    color: #475569;
  }

  :global(html.light-mode) .btn-clear-selection {
    background: rgba(0, 0, 0, 0.05);
    border-color: rgba(0, 0, 0, 0.12);
    color: #64748b;
  }

  :global(html.light-mode) .btn-clear-selection:hover {
    background: rgba(0, 0, 0, 0.1);
    color: #0f172a;
  }

  :global(html.light-mode) .bulk-divider {
    background: rgba(0, 0, 0, 0.1);
  }

  :global(html.light-mode .floating-bulk-bar .btn-bulk-primary) {
    background: #0066cc;
    color: #ffffff;
  }

  :global(html.light-mode .floating-bulk-bar .btn-bulk-danger) {
    background: #dc2626;
    border: 1px solid #b91c1c;
    color: #ffffff !important;
    box-shadow: 0 2px 10px rgba(220, 38, 38, 0.35);
  }

  :global(html.light-mode .floating-bulk-bar .btn-bulk-danger:hover) {
    background: #b91c1c;
    color: #ffffff !important;
    box-shadow: 0 4px 14px rgba(185, 28, 28, 0.45);
    transform: translateY(-1px);
  }

  :global(html.light-mode .floating-bulk-bar .btn-bulk-outline) {
    background: #f1f5f9;
    border-color: #cbd5e1;
    color: #1e293b;
  }

  :global(html.light-mode .floating-bulk-bar .btn-bulk-outline:hover) {
    background: #e2e8f0;
  }
</style>
