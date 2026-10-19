<script lang="ts">
  import { portal } from '../../actions/portal.ts';
  import type { Snippet } from 'svelte';

  export interface ContextMenuItem {
    label: string;
    icon?: any;
    action?: () => void;
    danger?: boolean;
    disabled?: boolean;
    divider?: boolean;
    color?: string;
    shortcut?: string;
  }

  let {
    isOpen = $bindable(false),
    x = 0,
    y = 0,
    title = '',
    subtitle = '',
    badge = undefined,
    icon: HeaderIcon = undefined,
    items = [],
    minWidth = '250px',
    maxWidth = '320px',
    headerSnippet = undefined,
    children = undefined,
    onclose = undefined,
  }: {
    isOpen: boolean;
    x: number;
    y: number;
    title?: string;
    subtitle?: string;
    badge?: { text: string; variant?: 'success' | 'info' | 'warning' | 'error' | 'muted'; color?: string };
    icon?: any;
    items?: ContextMenuItem[];
    minWidth?: string;
    maxWidth?: string;
    headerSnippet?: Snippet;
    children?: Snippet;
    onclose?: () => void;
  } = $props();

  let menuEl = $state<HTMLElement | null>(null);
  let clampedX = $state(x);
  let clampedY = $state(y);

  function close() {
    isOpen = false;
    onclose?.();
  }

  $effect(() => {
    if (isOpen) {
      // Calculate coordinates with boundary clamping
      const width = menuEl?.offsetWidth || 260;
      const height = menuEl?.offsetHeight || 340;
      const winW = typeof window !== 'undefined' ? window.innerWidth : 1200;
      const winH = typeof window !== 'undefined' ? window.innerHeight : 800;

      clampedX = Math.max(10, Math.min(x, winW - width - 14));
      clampedY = Math.max(10, Math.min(y, winH - height - 14));
    }
  });

  function handleItemClick(item: ContextMenuItem) {
    if (item.disabled || item.divider) return;
    close();
    item.action?.();
  }
</script>

<svelte:window 
  onclick={(e) => {
    if (isOpen && menuEl && !menuEl.contains(e.target as Node)) {
      close();
    }
  }}
  oncontextmenu={(e) => {
    if (isOpen && menuEl && !menuEl.contains(e.target as Node)) {
      close();
    }
  }}
  onkeydown={(e) => {
    if (isOpen && e.key === 'Escape') {
      close();
    }
  }}
/>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    bind:this={menuEl}
    class="custom-context-menu"
    style="position: fixed; left: {clampedX}px; top: {clampedY}px; z-index: 99999; min-width: {minWidth}; max-width: {maxWidth};"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.stopPropagation()}
    role="menu"
    tabindex="-1"
  >
    {#if headerSnippet}
      {@render headerSnippet()}
    {:else if title}
      <div class="context-menu-header">
        <div class="header-left">
          {#if HeaderIcon}
            <HeaderIcon size={14} class="header-icon" />
          {/if}
          <span class="header-title" title={title}>{title}</span>
        </div>
        {#if badge}
          <span 
            class="badge {badge.variant ? `badge-${badge.variant}` : 'badge-info'}" 
            style={badge.color ? `background: ${badge.color}22; color: ${badge.color};` : ''}
          >
            {badge.text}
          </span>
        {/if}
      </div>
      {#if subtitle}
        <div class="context-menu-subtitle">{subtitle}</div>
      {/if}
      <div class="context-menu-divider"></div>
    {/if}

    {#if children}
      {@render children()}
    {/if}

    {#if items && items.length > 0}
      <div class="context-menu-items">
        {#each items as item}
          {#if item.divider}
            <div class="context-menu-divider"></div>
          {:else}
            {@const ItemIcon = item.icon}
            <button
              type="button"
              class="context-menu-item"
              class:text-danger={item.danger}
              disabled={item.disabled}
              onclick={() => handleItemClick(item)}
              role="menuitem"
            >
              {#if ItemIcon}
                <ItemIcon size={14} style={item.color ? `color: ${item.color};` : ''} />
              {/if}
              <span class="item-label">{item.label}</span>
              {#if item.shortcut}
                <span class="item-shortcut">{item.shortcut}</span>
              {/if}
            </button>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .custom-context-menu {
    background: rgba(15, 23, 42, 0.95);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 6px;
    box-shadow: 0 16px 36px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: calc(100vh - 24px);
    overflow-y: auto;
    animation: contextMenuFade 0.12s cubic-bezier(0.16, 1, 0.3, 1);
  }

  :global(html.light-mode) .custom-context-menu {
    background: #FFFFFF !important;
    border-color: #E2E8F0 !important;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.14), 0 0 0 1px rgba(0, 0, 0, 0.05) !important;
  }

  @keyframes contextMenuFade {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-2px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .context-menu-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    gap: 8px;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }

  :global(.custom-context-menu .header-icon) {
    color: var(--color-accent);
    flex-shrink: 0;
  }

  .header-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .context-menu-subtitle {
    padding: 0 8px 6px;
    font-size: 11px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }

  .context-menu-divider {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0;
    opacity: 0.8;
  }

  .context-menu-items {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .context-menu-item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }

  :global(html.light-mode) .context-menu-item:hover:not(:disabled) {
    background: #F1F5F9 !important;
  }

  .context-menu-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .context-menu-item.text-danger {
    color: var(--color-error);
  }

  .context-menu-item.text-danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
  }

  .item-label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-shortcut {
    font-size: 10.5px;
    color: var(--color-text-muted);
    font-family: var(--font-mono);
    margin-left: 8px;
  }
</style>
