<script lang="ts">
  interface TabOption {
    id: string;
    label: string;
    count?: number;
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

<div class="tab-bar {className}" {style}>
  {#each tabs as tab}
    <button 
      class="tab-btn {activeTab === tab.id ? 'active' : ''}" 
      onclick={() => activeTab = tab.id}
      {disabled}
    >
      {tab.label}
      {#if tab.count !== undefined && tab.count > 0}
        <span class="tab-count" class:active-count={activeTab === tab.id}>{tab.count}</span>
      {/if}
    </button>
  {/each}
</div>
