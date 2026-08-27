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

<div class="table-wrap table-{activeDensity} {className}" {style} {onscroll}>
  {#if showDensityToggle}
    <div class="tbl-floating-density-pill" role="group" aria-label="Table Density Toggle">
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'compact' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('compact')}
        title="Compact Mode (28px row height, 11px font, max data density)"
      >
        <Rows4 size={12} />
      </button>
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'normal' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('normal')}
        title="Normal Mode (36px row height, 12px font, standard balanced view)"
      >
        <Rows3 size={12} />
      </button>
      <button
        type="button"
        class="tbl-floating-density-btn {activeDensity === 'spacious' ? 'active' : ''}"
        onclick={() => uiStore.setTableDensity('spacious')}
        title="Spacious Mode (44px row height, 13px font, comfortable relaxed view)"
      >
        <Rows2 size={12} />
      </button>
    </div>
  {/if}

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

<style>
  .table-wrap {
    position: relative;
  }

  .tbl-floating-density-pill {
    position: absolute;
    top: 6px;
    right: 8px;
    z-index: 30;
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
