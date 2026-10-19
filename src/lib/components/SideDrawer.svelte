<script lang="ts">
  import { X, Columns2, PanelRightClose } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Snippet } from 'svelte';
  import { portal } from '../actions/portal.ts';
  import { uiStore } from '../stores/ui.svelte.ts';

  let {
    isOpen = $bindable(false),
    title,
    subtitle = '',
    width = '500px',
    dockable = true,
    children = undefined,
    headerActions = undefined,
  }: {
    isOpen: boolean;
    title: string;
    subtitle?: string;
    width?: string;
    dockable?: boolean;
    children?: Snippet;
    headerActions?: Snippet;
  } = $props();

  let isWideScreen = $state(typeof window !== 'undefined' ? window.innerWidth >= 1100 : false);

  function close() {
    isOpen = false;
  }

  $effect(() => {
    if (typeof window !== 'undefined') {
      const onResize = () => {
        isWideScreen = window.innerWidth >= 1100;
      };
      window.addEventListener('resize', onResize);
      return () => window.removeEventListener('resize', onResize);
    }
  });

  $effect(() => {
    if (isOpen && (!dockable || !uiStore.isDrawerDocked || !isWideScreen)) {
      const prevOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = prevOverflow;
      };
    }
  });

  const isDockedActive = $derived(isOpen && dockable && uiStore.isDrawerDocked && isWideScreen);

  $effect(() => {
    if (typeof document !== 'undefined') {
      const shell = document.querySelector('.app-shell') as HTMLElement | null;
      if (shell) {
        if (isDockedActive) {
          const currentWidth = width || '500px';
          shell.style.setProperty('--docked-drawer-width', currentWidth);
          shell.classList.add('drawer-docked-active');
        } else {
          shell.classList.remove('drawer-docked-active');
          shell.style.removeProperty('--docked-drawer-width');
        }
      }
      return () => {
        if (typeof document !== 'undefined') {
          const s = document.querySelector('.app-shell') as HTMLElement | null;
          if (s) {
            s.classList.remove('drawer-docked-active');
            s.style.removeProperty('--docked-drawer-width');
          }
        }
      };
    }
  });
</script>

{#if isOpen}
  <div use:portal class="drawer-portal-root" class:is-docked={isDockedActive}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    {#if !isDockedActive}
      <div 
        class="drawer-backdrop" 
        transition:fade={{ duration: 180 }} 
        onclick={close}
        onwheel={(e) => e.stopPropagation()}
      ></div>
    {/if}
    
    <div 
      class="drawer" 
      class:docked-drawer={isDockedActive}
      style="width: {width || '500px'}; max-width: 100vw;"
      transition:fly={{ x: 500, duration: 250, easing: cubicOut }}
      onwheel={(e) => e.stopPropagation()}
    >
      <div class="drawer-header">
        <div class="drawer-title-group" title={subtitle ? `${title} — ${subtitle}` : title}>
          <div class="drawer-title-stack">
            <h2 class="drawer-title" title={title}>{title}</h2>
            {#if subtitle}
              <span class="drawer-subtitle truncate" title={subtitle}>{subtitle}</span>
            {/if}
          </div>
        </div>
        <div class="drawer-header-actions">
          {#if isDockedActive}
            <span class="badge badge-info split-view-badge" title="Drawer is currently docked in side-by-side split screen mode">SPLIT VIEW</span>
          {/if}
          {#if headerActions}
            {@render headerActions()}
          {/if}
          {#if dockable && isWideScreen}
            <button 
              type="button"
              class="drawer-icon-btn" 
              onclick={() => uiStore.toggleDrawerDocked()} 
              title={uiStore.isDrawerDocked ? "Undock Drawer (Overlay Mode)" : "Dock as Split Screen (Side-by-Side)"}
              aria-label="Toggle Dock Split View"
            >
              {#if uiStore.isDrawerDocked}
                <PanelRightClose size={15} style="color: var(--color-accent);" />
              {:else}
                <Columns2 size={15} />
              {/if}
            </button>
          {/if}
          <button type="button" class="drawer-icon-btn" onclick={close} aria-label="Close" title="Close Drawer">
            <X size={15} />
          </button>
        </div>
      </div>
      
      <div class="drawer-content">
        {#if children}
          {@render children()}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .drawer-portal-root {
    position: fixed;
    inset: 0;
    z-index: 99990;
    pointer-events: auto;
  }

  .drawer-portal-root.is-docked {
    pointer-events: none;
  }

  .drawer-portal-root.is-docked .drawer {
    pointer-events: auto;
  }

  .drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    z-index: 99991;
  }

  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    background: var(--color-bg-drawer, var(--color-bg-card));
    border-left: 1px solid var(--color-border);
    box-shadow: -12px 0 36px rgba(0, 0, 0, 0.4);
    z-index: 99992;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
  }

  .drawer.docked-drawer {
    box-shadow: -4px 0 20px rgba(0, 0, 0, 0.25);
    border-left: 1px solid var(--color-border);
  }

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
    gap: 12px;
  }

  .drawer-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    flex: 1;
    min-width: 0;
  }

  .drawer-title-stack {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    gap: 1px;
  }

  .drawer-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .drawer-subtitle {
    font-size: 11px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .split-view-badge {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.03em;
    padding: 0 7px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    flex-shrink: 0;
  }
  
  .drawer-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .drawer-icon-btn,
  :global(.drawer-icon-btn) {
    width: 28px;
    height: 28px;
    min-height: 28px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card, rgba(255, 255, 255, 0.05));
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .drawer-icon-btn:hover,
  :global(.drawer-icon-btn:hover) {
    border-color: var(--color-accent);
    color: var(--color-accent);
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.08));
  }

  :global(html.light-mode) .drawer-icon-btn,
  :global(html.light-mode .drawer-icon-btn) {
    background: #FFFFFF;
    border-color: #CBD5E1;
    color: #64748B;
  }

  :global(html.light-mode) .drawer-icon-btn:hover,
  :global(html.light-mode .drawer-icon-btn:hover) {
    background: #F1F5F9;
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .drawer-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
