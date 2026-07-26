<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronLeft } from '@lucide/svelte';
  import { uiStore } from '../stores/ui.svelte.ts';

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
      {#if uiStore.canGoBack}
        <button class="back-btn" onclick={() => uiStore.goBack()} title="Go back">
          <ChevronLeft size={14} />
          <span>Back</span>
        </button>
        <span class="crumb-separator" style="opacity: 0.25;">›</span>
      {/if}
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
    margin: -24px -24px 20px -24px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 24px;
    background: var(--color-bg-surface);
    border-bottom: 1px solid var(--color-border);
    border-radius: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  /* Back button */
  .back-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px 8px 3px 4px;
    background: transparent;
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 6px;
    color: var(--color-text-muted);
    font-size: 11px;
    font-family: var(--font-sans);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    letter-spacing: 0.02em;
  }

  .back-btn:hover {
    background: rgba(0, 218, 243, 0.08);
    border-color: rgba(0, 218, 243, 0.25);
    color: var(--color-accent);
  }

  .crumb-text {
    color: var(--color-text-muted);
    font-weight: 500;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.03em;
  }

  .crumb-text.active {
    color: var(--color-text-primary);
    font-weight: 600;
    font-family: var(--font-sans);
    font-size: 12px;
    letter-spacing: 0;
  }

  .crumb-separator {
    color: var(--color-text-muted);
    font-size: 14px;
    opacity: 0.4;
    font-family: var(--font-mono);
  }

  .crumb-subtitle {
    color: var(--color-text-muted);
    font-weight: 400;
    font-size: 11px;
    font-family: var(--font-sans);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
