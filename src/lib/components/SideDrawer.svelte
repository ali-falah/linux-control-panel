<script lang="ts">
  import { X } from '@lucide/svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Snippet } from 'svelte';
  import { portal } from '../actions/portal.ts';

  let {
    isOpen = $bindable(false),
    title,
    width = '500px',
    children = undefined,
    headerActions = undefined,
  }: {
    isOpen: boolean;
    title: string;
    width?: string;
    children?: Snippet;
    headerActions?: Snippet;
  } = $props();

  function close() {
    isOpen = false;
  }

  $effect(() => {
    if (isOpen) {
      const prevOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = prevOverflow;
      };
    }
  });
</script>

{#if isOpen}
  <div use:portal class="drawer-portal-root">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div 
      class="drawer-backdrop" 
      transition:fade={{ duration: 200 }} 
      onclick={close}
      onwheel={(e) => e.stopPropagation()}
    ></div>
    
    <div 
      class="drawer" 
      style="width: {width}; max-width: 100vw;"
      transition:fly={{ x: 500, duration: 300, easing: cubicOut }}
      onwheel={(e) => e.stopPropagation()}
    >
      <div class="drawer-header">
        <h2 class="drawer-title">{title}</h2>
        <div class="drawer-header-actions">
          {#if headerActions}
            {@render headerActions()}
          {/if}
          <button class="btn btn-icon btn-ghost" onclick={close} aria-label="Close">
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

  .drawer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px;
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .drawer-title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text-primary);
  }
  
  .drawer-header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .drawer-content {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
    display: flex;
    flex-direction: column;
  }
</style>
