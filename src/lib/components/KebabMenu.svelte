<script lang="ts">
  import { MoreVertical } from '@lucide/svelte';
  import type { Snippet } from 'svelte';
  import { portal } from '../actions/portal.ts';

  let { children }: { children?: Snippet } = $props();

  let isOpen = $state(false);
  let menuNode: HTMLElement;
  let triggerBtn: HTMLElement;
  let dropdownStyle = $state('');

  function updatePosition() {
    if (triggerBtn) {
      const rect = triggerBtn.getBoundingClientRect();
      const top = rect.bottom + 4;
      const left = rect.right - 160;
      dropdownStyle = `position: fixed; top: ${top}px; left: ${left}px; margin-top: 0; z-index: 9999;`;
    }
  }

  function toggleMenu(e: MouseEvent) {
    e.stopPropagation();
    if (!isOpen) {
      updatePosition();
    }
    isOpen = !isOpen;
  }

  function closeMenu() {
    isOpen = false;
  }

  $effect(() => {
    const handleOutsideClick = (event: MouseEvent) => {
      if (menuNode && !menuNode.contains(event.target as Node)) {
        const isDropdownClick = (event.target as Element).closest('.menu-dropdown');
        if (!isDropdownClick) {
          isOpen = false;
        }
      }
    };
    if (isOpen) {
      document.addEventListener('click', handleOutsideClick);
      window.addEventListener('scroll', updatePosition, true);
      window.addEventListener('resize', updatePosition);
    }
    return () => {
      document.removeEventListener('click', handleOutsideClick);
      window.removeEventListener('scroll', updatePosition, true);
      window.removeEventListener('resize', updatePosition);
    };
  });
</script>

<div class="kebab-menu" bind:this={menuNode}>
  <button bind:this={triggerBtn} class="trigger-btn btn btn-icon btn-ghost" onclick={toggleMenu} aria-haspopup="menu" aria-expanded={isOpen}>
    <MoreVertical size={16} />
  </button>
  
  {#if isOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div 
      use:portal
      class="menu-dropdown card animate-fade-slide" 
      style={dropdownStyle}
      onclick={closeMenu}
      role="menu"
    >
      {#if children}
        {@render children()}
      {/if}
    </div>
  {/if}
</div>

<style>
  .kebab-menu {
    position: relative;
    display: inline-block;
  }

  .trigger-btn {
    padding: 6px;
    border-radius: 8px;
    color: var(--color-text-secondary);
  }

  .trigger-btn:hover, .trigger-btn[aria-expanded="true"] {
    background: rgba(255, 255, 255, 0.1);
    color: var(--color-text-primary);
  }

  .menu-dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    padding: 8px !important;
    min-width: 160px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    z-index: 50;
    transform-origin: top right;
  }
  
  :global(.menu-dropdown .menu-item) {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s ease;
  }
  
  :global(.menu-dropdown .menu-item:hover) {
    background: rgba(255, 255, 255, 0.08);
  }
  
  :global(.menu-dropdown .menu-item.danger) {
    color: var(--color-error);
  }
  
  :global(.menu-dropdown .menu-item.danger:hover) {
    background: var(--color-error-muted);
  }
</style>
