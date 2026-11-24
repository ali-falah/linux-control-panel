<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Action } from 'svelte/action';
  import { uiStore } from '../../stores/ui.svelte.ts';
  import { Rows4, Rows3, Rows2 } from '@lucide/svelte';

  interface Props {
    children?: Snippet;
    tableAction?: Action<HTMLTableElement, any>;
    actionParams?: any;
    class?: string;
    style?: string;
    onscroll?: (e: Event) => void;
    showDensityToggle?: boolean;
    density?: 'compact' | 'normal' | 'spacious';
    [key: string]: any;
  }

  let { 
    children, 
    class: className = '',
    tableAction,
    actionParams,
    style = '',
    onscroll,
    showDensityToggle = true,
    density,
    ...rest 
  }: Props = $props();

  let activeDensity = $derived(density ?? uiStore.tableDensity);
</script>

<div class="table-frame" {style}>
  <div class="table-wrap table-{activeDensity} {className}" {onscroll}>
    {#if tableAction}
      <table use:tableAction={actionParams} {...rest}>
        {@render children?.()}
      </table>
    {:else}
      <table {...rest}>
        {@render children?.()}
      </table>
    {/if}
  </div>
  {#if showDensityToggle}
    <!-- Outside the scrolling box, so it stays put while the rows move. -->
    <div class="tbl-floating-density-pill" role="group" aria-label="Table Density Toggle">
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'compact' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('compact')}
        aria-pressed={activeDensity === 'compact'}
        title="Compact Mode (max data density, 11px font)"
      >
        <Rows4 size={12} />
      </button>
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'normal' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('normal')}
        aria-pressed={activeDensity === 'normal'}
        title="Normal Mode (balanced density, 12px font)"
      >
        <Rows3 size={12} />
      </button>
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'spacious' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('spacious')}
        aria-pressed={activeDensity === 'spacious'}
        title="Spacious Mode (comfortable reading, 13px font)"
      >
        <Rows2 size={12} />
      </button>
    </div>
  {/if}
</div>

<style>
  .table-wrap {
    position: relative;
  }

  .tbl-floating-density-pill {
    position: absolute;
    top: 6px;
    right: 8px;
    z-index: 30;
    /* recessed until pointed at — 0.4 keeps it discoverable without
       competing with the data; keyboard focus lifts it too */
    opacity: 0.3;
    transition: opacity 0.15s ease;
    display: inline-flex;
    align-items: center;
    background: rgba(11, 23, 38, 0.85);
    backdrop-filter: blur(8px);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    padding: 2px;
    gap: 1px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  :global(html.light-mode) .tbl-floating-density-pill {
    background: rgba(255, 255, 255, 0.9);
    border-color: #CBD5E1;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.1);
  }

  .tbl-floating-density-pill:hover,
  .tbl-floating-density-pill:focus-within {
    opacity: 1;
  }

  .tbl-floating-density-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    color: var(--color-text-muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tbl-floating-density-btn:hover {
    color: var(--color-text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .tbl-floating-density-btn.active {
    color: var(--color-accent);
    background: rgba(0, 218, 243, 0.14);
    border-color: rgba(0, 218, 243, 0.3);
  }

  :global(html.light-mode) .tbl-floating-density-btn.active {
    color: #0284c7;
    background: rgba(2, 132, 199, 0.12);
    border-color: rgba(2, 132, 199, 0.3);
  }
</style>
