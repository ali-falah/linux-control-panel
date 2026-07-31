<script lang="ts">
  import type { HTMLSelectAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';

  interface Props extends HTMLSelectAttributes {
    label?: string;
    id?: string;
    children?: Snippet;
  }

  let { 
    label,
    id = Math.random().toString(36).substring(2, 9),
    class: className = '',
    value = $bindable(),
    children,
    style = '',
    ...rest 
  }: Props = $props();

  let selectEl = $state<HTMLSelectElement | null>(null);
  let containerEl = $state<HTMLDivElement | null>(null);
  let options = $state<{ value: string; label: string }[]>([]);
  let isOpen = $state(false);

  function syncOptions() {
    if (selectEl) {
      options = Array.from(selectEl.options).map(opt => ({
        value: opt.value,
        label: opt.text
      }));
    }
  }

  $effect(() => {
    if (selectEl) {
      syncOptions();
      const observer = new MutationObserver(syncOptions);
      observer.observe(selectEl, { childList: true, subtree: true, characterData: true });
      return () => observer.disconnect();
    }
  });

  // Watch for click outside to close dropdown
  $effect(() => {
    function handleDocumentClick(e: MouseEvent) {
      if (containerEl && !containerEl.contains(e.target as Node)) {
        isOpen = false;
      }
    }
    document.addEventListener('click', handleDocumentClick);
    return () => document.removeEventListener('click', handleDocumentClick);
  });

  // Close dropdown on Escape key
  $effect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        isOpen = false;
      }
    }
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  });

  let activeLabel = $derived.by(() => {
    const activeOpt = options.find(o => o.value === value);
    return activeOpt ? activeOpt.label : (options[0]?.label || 'Select...');
  });

  function selectOption(optValue: string) {
    value = optValue;
    isOpen = false;
    if (selectEl) {
      selectEl.value = optValue;
      selectEl.dispatchEvent(new Event('change', { bubbles: true }));
    }
  }

  function handleTriggerKeydown(e: KeyboardEvent) {
    if (rest.disabled) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === ' ' || e.key === 'Enter') {
      e.preventDefault();
      isOpen = !isOpen;
    }
  }
</script>

<div bind:this={containerEl} class="ui-select-container {className}">
  <!-- Hidden native select for reactive binding and option extraction -->
  <select
    bind:this={selectEl}
    {id}
    bind:value
    style="position: absolute; opacity: 0; width: 0; height: 0; pointer-events: none; border: none; padding: 0; margin: 0; overflow: hidden;"
    disabled={rest.disabled}
    {...rest}
  >
    {@render children?.()}
  </select>

  {#if label}
    <label for={id} class="ui-select-label">{label}</label>
  {/if}

  <!-- Custom Trigger Button -->
  <button
    type="button"
    class="ui-select-trigger"
    class:open={isOpen}
    class:disabled={rest.disabled}
    onclick={() => { if (!rest.disabled) isOpen = !isOpen; }}
    onkeydown={handleTriggerKeydown}
    disabled={rest.disabled}
    role="combobox"
    aria-expanded={isOpen}
    aria-haspopup="listbox"
    aria-controls="{id}-listbox"
    aria-label={label || 'Select option'}
    {style}
  >
    <span class="ui-select-value">{activeLabel}</span>
    <span class="ui-select-chevron">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="chevron-icon">
        <polyline points="6 9 12 15 18 9"></polyline>
      </svg>
    </span>
  </button>

  <!-- Custom Option Popover list -->
  {#if isOpen}
    <div id="{id}-listbox" class="ui-select-dropdown" role="listbox" tabindex="-1">
      {#each options as opt}
        <button
          type="button"
          class="ui-select-option"
          class:selected={opt.value === value}
          onclick={() => selectOption(opt.value)}
          role="option"
          aria-selected={opt.value === value}
        >
          {opt.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ui-select-container {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    width: 100%;
    min-width: 100px;
    box-sizing: border-box;
  }

  .ui-select-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-text-muted);
    margin-bottom: 4px;
  }

  .ui-select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background-color: var(--color-bg-card);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    padding: 0 12px;
    border-radius: 8px;
    height: 36px;
    width: 100%;
    font-size: 12px;
    font-family: var(--font-sans);
    outline: none;
    cursor: pointer;
    box-sizing: border-box;
    transition: all 0.15s ease;
  }

  .ui-select-trigger:hover:not(.disabled) {
    border-color: var(--color-border-hover);
    background-color: var(--color-bg-hover);
  }

  .ui-select-trigger.open {
    border-color: var(--color-accent);
    box-shadow:
      0 0 0 2px var(--color-accent-muted),
      0 0 8px var(--color-accent-glow);
    background-color: var(--color-bg-card);
  }

  .ui-select-trigger.disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .ui-select-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }

  .ui-select-chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: transform 0.2s ease;
    margin-left: 8px;
  }
  
  .chevron-icon {
    width: 13px;
    height: 13px;
  }

  .ui-select-trigger.open .ui-select-chevron {
    transform: rotate(180deg);
    color: var(--color-accent);
  }

  .ui-select-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    margin-top: 4px;
    padding: 4px;
    background-color: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.25);
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 250px;
    overflow-y: auto;
  }

  .ui-select-option {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 8px 12px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-family: var(--font-sans);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }

  .ui-select-option:hover {
    background-color: var(--color-active-bg);
    color: var(--color-accent);
  }

  .ui-select-option.selected {
    background-color: var(--color-accent);
    color: var(--color-text-on-accent);
    font-weight: 600;
  }
</style>
