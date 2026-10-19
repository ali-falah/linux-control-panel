<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLButtonAttributes, 'onclick'> {
    active?: boolean;
    activeLabel?: string;
    inactiveLabel?: string;
    activeTitle?: string;
    inactiveTitle?: string;
    activeIcon?: any;
    inactiveIcon?: any;
    icon?: any;
    size?: 'sm' | 'md';
    class?: string;
    spinWhenActive?: boolean;
    onclick?: (e: MouseEvent, active: boolean) => void;
    children?: Snippet;
  }

  let {
    active = $bindable(false),
    activeLabel = '',
    inactiveLabel = '',
    activeTitle = '',
    inactiveTitle = '',
    activeIcon,
    inactiveIcon,
    icon,
    size = 'sm',
    class: className = '',
    spinWhenActive = false,
    onclick,
    children,
    disabled = false,
    ...rest
  }: Props = $props();

  function handleClick(e: MouseEvent) {
    if (disabled) return;
    active = !active;
    if (onclick) onclick(e, active);
  }

  let currentIcon = $derived(
    active ? (activeIcon || icon) : (inactiveIcon || icon)
  );

  let currentLabel = $derived(
    active ? (activeLabel || inactiveLabel) : (inactiveLabel || activeLabel)
  );

  let currentTitle = $derived(
    active ? (activeTitle || inactiveTitle) : (inactiveTitle || activeTitle)
  );
</script>

<button
  type="button"
  class="ui-toggle-button size-{size} {active ? 'active' : ''} {className}"
  {disabled}
  title={currentTitle}
  onclick={handleClick}
  {...rest}
>
  {#if children}
    {@render children()}
  {:else}
    {#if currentIcon}
      {@const IconComp = currentIcon}
      <IconComp size={size === 'sm' ? 12 : 14} class="toggle-btn-icon {active && spinWhenActive ? 'spinning animate-spin-slow' : ''}" />
    {/if}
    {#if currentLabel}
      <span class="toggle-btn-label">{currentLabel}</span>
    {/if}
  {/if}
</button>

<style>
  .ui-toggle-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 28px;
    padding: 0 9px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card, rgba(255, 255, 255, 0.04));
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s cubic-bezier(0.16, 1, 0.3, 1);
    user-select: none;
    flex-shrink: 0;
  }

  :global(.ui-toggle-button .toggle-btn-icon.spinning),
  :global(.ui-toggle-button .toggle-btn-icon.animate-spin-slow) {
    animation: spinSlow 1.5s linear infinite !important;
    transform-origin: center center;
    display: inline-block;
  }

  @keyframes spinSlow {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .ui-toggle-button.size-md {
    height: 32px;
    padding: 0 12px;
    font-size: 12px;
    border-radius: 7px;
  }

  .ui-toggle-button:hover:not(:disabled) {
    border-color: var(--color-border-hover, rgba(255, 255, 255, 0.2));
    color: var(--color-text-primary);
    background: var(--color-bg-hover, rgba(255, 255, 255, 0.08));
  }

  .ui-toggle-button.active {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    border-color: var(--color-accent);
    color: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent-glow, rgba(0, 218, 243, 0.2));
  }

  .ui-toggle-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  :global(html.light-mode) .ui-toggle-button {
    background: #FFFFFF;
    border-color: #E2E8F0;
    color: #475569;
  }

  :global(html.light-mode) .ui-toggle-button:hover:not(:disabled) {
    background: #F8FAFC;
    border-color: #CBD5E1;
    color: #0F172A;
  }

  :global(html.light-mode) .ui-toggle-button.active {
    background: var(--color-accent-muted, rgba(0, 218, 243, 0.12));
    border-color: var(--color-accent);
    color: var(--color-accent);
  }
</style>
