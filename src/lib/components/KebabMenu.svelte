<script lang="ts">
  import { MoreVertical } from '@lucide/svelte';
  import type { Snippet } from 'svelte';
  import { portal } from '../actions/portal.ts';

  let { 
    children, 
    align = 'right',
    icon: TriggerIcon = MoreVertical,
    title = 'Actions',
    buttonClass = ''
  }: { 
    children?: Snippet; 
    align?: 'left' | 'right';
    icon?: any;
    title?: string;
    buttonClass?: string;
  } = $props();

  let isOpen = $state(false);
  let menuNode: HTMLElement;
  let triggerBtn: HTMLElement;
  let dropdownStyle = $state('');

  function updatePosition() {
    if (triggerBtn) {
      const rect = triggerBtn.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom;
      const spaceAbove = rect.top;
      const openUpwards = spaceBelow < 280 && spaceAbove > spaceBelow;

      let topStyle = '';
      if (openUpwards) {
        const bottom = Math.max(10, window.innerHeight - rect.top + 4);
        topStyle = `bottom: ${bottom}px; top: auto; max-height: ${Math.max(160, spaceAbove - 20)}px;`;
      } else {
        const top = Math.max(10, rect.bottom + 4);
        topStyle = `top: ${top}px; bottom: auto; max-height: ${Math.max(160, spaceBelow - 20)}px;`;
      }

      let leftRightStyle = '';
      if (align === 'right') {
        const right = Math.max(10, window.innerWidth - rect.right);
        leftRightStyle = `right: ${right}px; left: auto;`;
      } else {
        const left = Math.max(10, Math.min(rect.left, window.innerWidth - 290));
        leftRightStyle = `left: ${left}px; right: auto;`;
      }

      dropdownStyle = `position: fixed; ${topStyle} ${leftRightStyle} margin-top: 0; z-index: 99999; width: max-content; min-width: 180px; max-width: 290px; overflow-y: auto; overflow-x: hidden;`;
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
  <button 
    bind:this={triggerBtn} 
    class="trigger-btn btn btn-icon btn-ghost {buttonClass}" 
    onclick={toggleMenu} 
    aria-haspopup="menu" 
    aria-expanded={isOpen}
    title={title}
  >
    <TriggerIcon size={16} />
  </button>
  
  {#if isOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div 
      use:portal
      class="menu-dropdown animate-fade-slide" 
      style={dropdownStyle}
      onclick={closeMenu}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') closeMenu(); }}
      role="menu"
      tabindex="0"
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
    background: var(--color-bg-hover);
    color: var(--color-accent);
  }

  .menu-dropdown {
    position: fixed;
    padding: 5px !important;
    width: max-content;
    min-width: 180px;
    max-width: 280px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--color-bg-surface) !important;
    border: 1px solid var(--color-border) !important;
    border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4), 0 2px 8px rgba(0, 0, 0, 0.2) !important;
    z-index: 99999;
    transform-origin: top right;
  }

  :global(html.light-mode) .menu-dropdown {
    background: #ffffff !important;
    border-color: #cbd5e1 !important;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.15), 0 8px 10px -6px rgba(0, 0, 0, 0.08) !important;
  }
  
  :global(.menu-dropdown .menu-item) {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--color-text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease, color 0.12s ease;
    white-space: nowrap;
    font-family: inherit;
  }
  
  :global(.menu-dropdown .menu-item:hover) {
    background: var(--color-bg-hover);
    color: var(--color-text-primary);
  }
  
  :global(.menu-dropdown .menu-item.primary-action) {
    color: var(--color-accent);
    font-weight: 600;
  }
  
  :global(.menu-dropdown .menu-item.primary-action:hover) {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.1));
    color: var(--color-accent);
  }

  :global(.menu-dropdown .menu-item:disabled) {
    opacity: 0.45;
    cursor: not-allowed;
    pointer-events: none;
  }

  :global(.menu-dropdown .menu-divider) {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0;
    width: 100%;
  }
  
  :global(.menu-dropdown .menu-item.danger),
  :global(.menu-dropdown .menu-item.text-danger) {
    color: var(--color-error);
  }
  
  :global(.menu-dropdown .menu-item.danger:hover),
  :global(.menu-dropdown .menu-item.text-danger:hover) {
    background: rgba(244, 63, 94, 0.12);
    color: var(--color-error);
  }
</style>
