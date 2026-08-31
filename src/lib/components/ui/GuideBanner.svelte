<script lang="ts">
  import type { Component } from 'svelte';
  import { Sparkles } from '@lucide/svelte';

  interface Props {
    icon?: Component<any>;
    title: string;
    description: string;
    actionLabel?: string;
    actionIcon?: Component<any>;
    onAction?: () => void;
    variant?: 'info' | 'warning' | 'purple' | 'success';
    class?: string;
    style?: string;
  }

  let {
    icon: IconComponent = Sparkles,
    title,
    description,
    actionLabel = '',
    actionIcon: ActionIconComponent,
    onAction,
    variant = 'info',
    class: className = '',
    style = ''
  }: Props = $props();
</script>

<div class="guide-banner variant-{variant} {className}" {style}>
  <div class="guide-banner-left">
    <div class="guide-icon-box">
      <IconComponent size={18} />
    </div>
    <div class="guide-text-content">
      <strong class="guide-title">{title}</strong>
      <span class="guide-desc">{description}</span>
    </div>
  </div>

  {#if actionLabel && onAction}
    <button type="button" class="guide-action-btn" onclick={onAction}>
      {#if ActionIconComponent}
        <ActionIconComponent size={13} />
      {/if}
      <span>{actionLabel}</span>
    </button>
  {/if}
</div>

<style>
  .guide-banner {
    border-radius: 12px;
    padding: 14px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  .variant-info {
    background: rgba(0, 218, 243, 0.05);
    border: 1px solid rgba(0, 218, 243, 0.18);
    color: var(--color-text-primary);
  }

  :global(html.light-mode) .variant-info {
    background: #EFF6FF;
    border-color: #BFDBFE;
  }

  .variant-warning {
    background: rgba(245, 158, 11, 0.06);
    border: 1px solid rgba(245, 158, 11, 0.22);
  }

  :global(html.light-mode) .variant-warning {
    background: #FFFBEB;
    border-color: #FDE68A;
  }

  .variant-purple {
    background: rgba(168, 85, 247, 0.06);
    border: 1px solid rgba(168, 85, 247, 0.22);
  }

  :global(html.light-mode) .variant-purple {
    background: #FAF5FF;
    border-color: #E9D5FF;
  }

  .variant-success {
    background: rgba(16, 185, 129, 0.06);
    border: 1px solid rgba(16, 185, 129, 0.22);
  }

  :global(html.light-mode) .variant-success {
    background: #F0FDF4;
    border-color: #BBF7D0;
  }

  .guide-banner-left {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .guide-icon-box {
    color: var(--color-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .variant-warning .guide-icon-box { color: var(--color-warning); }
  .variant-purple .guide-icon-box { color: #A855F7; }
  .variant-success .guide-icon-box { color: var(--color-success); }

  .guide-text-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .guide-title {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .guide-desc {
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  :global(html.light-mode) .guide-desc {
    color: #475569;
  }

  .guide-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--color-accent);
    color: #000000;
    font-weight: 700;
    font-size: 12px;
    padding: 7px 14px;
    border-radius: 8px;
    border: none;
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  :global(html.light-mode) .guide-action-btn {
    background: var(--color-accent);
    color: #FFFFFF;
  }

  .guide-action-btn:hover {
    opacity: 0.9;
    transform: translateY(-1px);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  }
</style>
