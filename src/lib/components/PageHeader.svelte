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

<div class="page-header">
  <div class="header-content">
    {#if Icon}
      <div class="header-icon">
        <Icon size={24} />
      </div>
    {/if}
    <div class="header-text">
      <h1 class="title">{title}</h1>
      <p class="subtitle">{subtitle}</p>
    </div>
  </div>
  <div class="header-actions">
    {#if children}
      {@render children()}
    {/if}
  </div>
</div>

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 24px 32px;
    background: rgba(15, 15, 24, 0.4);
    border-bottom: 1px solid var(--color-border);
    backdrop-filter: blur(12px);
    margin: -24px -24px 24px -24px;
  }
  
  .header-content {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  
  .header-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: linear-gradient(135deg, rgba(99,102,241,0.2), rgba(79,70,229,0.05));
    border: 1px solid rgba(99,102,241,0.3);
    color: var(--color-accent);
    box-shadow: 0 4px 12px rgba(0,0,0,0.1), inset 0 0 12px rgba(99,102,241,0.1);
  }
  
  .header-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  
  .title {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    color: var(--color-text-primary);
    letter-spacing: -0.01em;
  }
  
  .subtitle {
    margin: 0;
    font-size: 13px;
    color: var(--color-text-secondary);
  }
  
  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
</style>
