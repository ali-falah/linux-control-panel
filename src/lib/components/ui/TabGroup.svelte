<script lang="ts">
  interface TabOption {
    id: string;
    label: string;
    count?: number; // optional badge count
  }

  interface Props {
    tabs: TabOption[];
    activeTab: string;
    disabled?: boolean;
    class?: string;
    style?: string;
  }

  let { 
    tabs, 
    activeTab = $bindable(), 
    disabled = false,
    class: className = '',
    style = ''
  }: Props = $props();
</script>

<div class="tab-group-container {className}" {style}>
  {#each tabs as tab}
    <button 
      class="tab-btn {activeTab === tab.id ? 'active' : ''}" 
      onclick={() => activeTab = tab.id}
      {disabled}
    >
      {tab.label}
      {#if tab.count !== undefined && tab.count > 0}
        <span class="badge" class:active-badge={activeTab === tab.id}>{tab.count}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tab-group-container {
    display: inline-flex;
    gap: 4px;
    background: rgba(0, 0, 0, 0.2);
    padding: 4px;
    border-radius: 8px;
    height: 40px;
    box-sizing: border-box;
    align-items: stretch;
  }

  .tab-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 16px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
    height: 100%;
    box-sizing: border-box;
    white-space: nowrap;
  }

  .tab-btn:hover:not(:disabled) {
    color: #fff;
  }

  .tab-btn.active {
    background: rgba(255,255,255,0.05);
    border-color: rgba(139, 92, 246, 0.3); /* subtle purple border */
    color: #fff;
    box-shadow: 0 2px 4px rgba(0,0,0,0.1);
  }

  .tab-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .badge {
    background: rgba(255,255,255,0.1);
    color: var(--color-text-muted);
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 4px;
    line-height: 1;
  }

  .active-badge {
    background: var(--color-accent-muted);
    color: var(--color-text-primary);
  }
</style>
