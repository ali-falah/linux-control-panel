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
    width = '500px',
    dockable = true,
    children = undefined,
    headerActions = undefined,
  }: {
    isOpen: boolean;
    title: string;
    width?: string;
    dockable?: boolean;
    children?: Snippet;
    headerActions?: Snippet;
  } = $props();

  let isWideScreen = $state(typeof window !== 'undefined' ? window.innerWidth >= 1350 : false);

  function close() {
    isOpen = false;
  }

  $effect(() => {
    if (typeof window !== 'undefined') {
      const onResize = () => {
        isWideScreen = window.innerWidth >= 1350;
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
      style="width: {isDockedActive ? 'var(--docked-drawer-width, 540px)' : width}; max-width: 100vw;"
      transition:fly={{ x: 500, duration: 250, easing: cubicOut }}
      onwheel={(e) => e.stopPropagation()}
    >
      <div class="drawer-header">
        <div class="drawer-title-group">
          <h2 class="drawer-title">{title}</h2>
          {#if isDockedActive}
            <span class="badge badge-info" style="font-size: 10px; font-weight: 600;">SPLIT VIEW</span>
          {/if}
        </div>
        <div class="drawer-header-actions">
          {#if headerActions}
            {@render headerActions()}
          {/if}
          {#if dockable && isWideScreen}
            <button 
              type="button"
              class="btn btn-icon btn-ghost" 
              onclick={() => uiStore.toggleDrawerDocked()} 
              title={uiStore.isDrawerDocked ? "Undock Drawer (Overlay Mode)" : "Dock as Split Screen (Side-by-Side)"}
              aria-label="Toggle Dock Split View"
            >
              {#if uiStore.isDrawerDocked}
                <PanelRightClose size={18} style="color: var(--color-accent);" />
              {:else}
                <Columns2 size={18} />
              {/if}
            </button>
          {/if}
          <button type="button" class="btn btn-icon btn-ghost" onclick={close} aria-label="Close">
            <X size={20} />
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
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .drawer-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
  }

  .drawer-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  
  .drawer-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .drawer-content {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
    display: flex;
    flex-direction: column;
  }
</style>
