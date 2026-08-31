<script lang="ts">
  import type { Component } from 'svelte';
  import { FolderOpen } from '@lucide/svelte';

  interface Props {
    icon?: Component<any>;
    title: string;
    description: string;
    actionLabel?: string;
    actionIcon?: Component<any>;
    onAction?: () => void;
    class?: string;
    style?: string;
  }

  let {
    icon: IconComponent = FolderOpen,
    title,
    description,
    actionLabel = '',
    actionIcon: ActionIconComponent,
    onAction,
    class: className = '',
    style = ''
  }: Props = $props();
</script>

<div class="empty-state-wrap {className}" {style}>
  <div class="empty-icon-wrap">
    <IconComponent size={36} />
  </div>

  <h4 class="empty-title">{title}</h4>
  <p class="empty-description">{description}</p>

  {#if actionLabel && onAction}
    <button type="button" class="empty-action-btn" onclick={onAction}>
      {#if ActionIconComponent}
        <ActionIconComponent size={14} />
      {/if}
      <span>{actionLabel}</span>
    </button>
  {/if}
</div>

<style>
  .empty-state-wrap {
    background: var(--color-bg-card);
    border: 1px dashed var(--color-border);
    border-radius: 12px;
    padding: 48px 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 8px;
    width: 100%;
    box-sizing: border-box;
  }

  :global(html.light-mode) .empty-state-wrap {
    background: #F8FAFC;
    border-color: #CBD5E1;
  }

  .empty-icon-wrap {
    color: var(--color-text-muted);
    opacity: 0.6;
    margin-bottom: 4px;
  }

  .empty-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .empty-description {
    margin: 0;
    font-size: 12.5px;
    color: var(--color-text-secondary);
    max-width: 420px;
    line-height: 1.5;
  }

  :global(html.light-mode) .empty-description {
    color: #64748B;
  }

  .empty-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--color-accent);
    color: #000000;
    font-weight: 700;
    font-size: 12.5px;
    padding: 8px 16px;
    border-radius: 8px;
    border: none;
    cursor: pointer;
    margin-top: 10px;
    transition: all 0.15s ease;
  }

  :global(html.light-mode) .empty-action-btn {
    background: var(--color-accent);
    color: #FFFFFF;
  }

  .empty-action-btn:hover {
    opacity: 0.9;
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }
</style>
