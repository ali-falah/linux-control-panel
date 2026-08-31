<script lang="ts">
  import { Search, X } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  interface Props {
    value: string;
    placeholder?: string;
    disabled?: boolean;
    debounceMs?: number;
    count?: number;
    total?: number;
    class?: string;
    style?: string;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let { 
    value = $bindable(), 
    placeholder = 'Search...', 
    disabled = false,
    debounceMs = 120,
    count,
    total,
    class: className = '',
    style = '',
    onkeydown
  }: Props = $props();

  let internalInput = $state(value);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  // Keep internal input in sync if external value is changed programmatically
  $effect(() => {
    if (value !== internalInput && debounceTimer === null) {
      internalInput = value;
    }
  });

  function handleInput(e: Event & { currentTarget: HTMLInputElement }) {
    const newVal = e.currentTarget.value;
    internalInput = newVal;

    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      value = newVal;
      debounceTimer = null;
    }, debounceMs);
  }

  function handleClear() {
    internalInput = '';
    value = '';
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && internalInput) {
      e.stopPropagation();
      handleClear();
    }
    onkeydown?.(e);
  }

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });
</script>

<div class="search-bar-container {className}" {style}>
  <div class="search-bar-input-wrap">
    <Search size={13} class="search-icon" />
    <input 
      type="text"
      value={internalInput}
      oninput={handleInput}
      onkeydown={handleKeydown}
      {placeholder}
      {disabled}
      class="search-input"
    />
    {#if internalInput}
      <button type="button" class="clear-btn" onclick={handleClear} title="Clear search (Esc)">
        <X size={11} />
      </button>
    {/if}
  </div>

  {#if count !== undefined}
    <span class="search-counter-badge">
      {#if total !== undefined && total > 0}
        {count} of {total}
      {:else}
        {count} found
      {/if}
    </span>
  {/if}
</div>

<style>
  .search-bar-container {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    min-width: 0;
    box-sizing: border-box;
  }

  .search-bar-input-wrap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    width: 100%;
    flex: 1;
    min-width: 0;
    box-sizing: border-box;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 0 8px;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .search-bar-input-wrap {
    background: #FFFFFF;
    border-color: #CBD5E1;
  }

  .search-bar-input-wrap:focus-within {
    border-color: var(--color-accent) !important;
    box-shadow: 0 0 0 2px var(--color-accent-muted, rgba(16, 185, 129, 0.15)) !important;
  }

  .search-icon {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .search-input,
  .search-input:focus,
  :global(html.light-mode) .search-input,
  :global(html.light-mode) .search-input:focus {
    background: transparent !important;
    border: none !important;
    border-color: transparent !important;
    outline: none !important;
    box-shadow: none !important;
    height: 100%;
    font-size: 12px;
    color: var(--color-text-primary);
    flex: 1;
    width: 100%;
    min-width: 0;
    padding: 0;
  }

  :global(html.light-mode) .search-input {
    color: #0F172A !important;
  }

  .clear-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    border-radius: 50%;
    width: 15px;
    height: 15px;
    padding: 0;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.12s ease;
    flex-shrink: 0;
  }

  :global(html.light-mode) .clear-btn {
    background: #E2E8F0;
    color: #475569;
  }

  .clear-btn:hover {
    background: var(--color-error);
    color: #FFFFFF;
  }

  .search-counter-badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-text-muted);
    background: rgba(255, 255, 255, 0.05);
    padding: 0 6px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    border-radius: 5px;
    border: 1px solid var(--color-border);
    white-space: nowrap;
    flex-shrink: 0;
  }

  :global(html.light-mode) .search-counter-badge {
    background: #F1F5F9;
    border-color: #E2E8F0;
    color: #64748B;
  }
</style>
