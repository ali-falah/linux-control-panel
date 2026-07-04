<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    subtitle,
    icon: Icon = undefined,
    children = undefined,
  }: {
    title: string;
    subtitle: string;
    icon?: any;
    children?: Snippet;
  } = $props();
</script>

<div class="header-wrapper">
  <div class="page-header">
    <div class="breadcrumb">
      <span class="crumb-text">Control Panel</span>
      <span class="crumb-separator">&rsaquo;</span>
      <span class="crumb-text active">{title}</span>
      {#if subtitle}
        <span class="crumb-separator" style="margin: 0 4px; opacity: 0.3;">&mdash;</span>
        <span class="crumb-subtitle">{subtitle}</span>
      {/if}
    </div>
    <div class="header-actions">
      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
</div>

<style>
  .header-wrapper {
    display: flex;
    flex-direction: column;
    margin: -24px -24px 24px -24px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 24px;
    background: var(--color-bg-surface);
    border-bottom: 1px solid var(--color-border);
    border-radius: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 14px;
    font-weight: 600;
  }

  .crumb-text {
    color: var(--color-text-primary);
  }

  .crumb-text.active {
    color: var(--color-text-muted);
  }

  .crumb-separator {
    color: var(--color-text-muted);
    font-size: 16px;
    opacity: 0.5;
  }

  .crumb-subtitle {
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>
