<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { ChevronDown, ChevronUp } from '@lucide/svelte';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    raised?: boolean;
    title?: string;
    icon?: any; // Lucide icon component type
    children?: Snippet;
    collapsible?: boolean;
    collapsed?: boolean;
    headerActions?: Snippet;
  }

  let { 
    raised = false,
    title,
    icon: Icon,
    children, 
    class: className = '',
    collapsible = false,
    collapsed = $bindable(false),
    headerActions,
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived(`card${raised ? '-raised' : ''} ${collapsible ? 'is-collapsible' : ''} ${collapsed ? 'is-collapsed' : ''} ${className}`.trim());
</script>

<div class={computedClass} {...rest}>
  {#if title || Icon}
    <div class="card-header-row">
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <h3 
        class="card-title-header" 
        onclick={() => { if (collapsible) collapsed = !collapsed; }} 
        style={collapsible ? 'cursor: pointer; user-select: none; margin-bottom: 0;' : ''}
      >
        {#if Icon}
          <Icon size={16} class="card-title-icon" />
        {/if}
        <span>{title}</span>
      </h3>
      <div class="card-header-actions">
        {#if headerActions}
          {@render headerActions()}
        {/if}
        {#if collapsible}
          <button 
            type="button" 
            class="card-collapse-btn" 
            onclick={(e) => { e.stopPropagation(); collapsed = !collapsed; }}
            title={collapsed ? 'Expand container' : 'Collapse container'}
            aria-label={collapsed ? 'Expand' : 'Collapse'}
          >
            {#if collapsed}
              <ChevronDown size={14} />
            {:else}
              <ChevronUp size={14} />
            {/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}
  {#if !collapsed}
    {@render children?.()}
  {/if}
</div>

<style>
  .card-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
    gap: 8px;
  }

  .card-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .card-collapse-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
    padding: 0;
  }

  .card-collapse-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
    border-color: var(--color-border-hover);
    transform: translateY(-0.5px);
  }

  :global(html.light-mode) .card-collapse-btn {
    background: #F1F5F9;
    border-color: #CBD5E1;
    color: #64748B;
  }

  :global(html.light-mode) .card-collapse-btn:hover {
    background: #E2E8F0;
    color: #1E293B;
  }
</style>
