<script lang="ts">
  interface TabOption {
    id: string;
    label: string;
    count?: number;
    icon?: any;
  }

  interface Props {
    tabs: TabOption[];
    activeTab: string;
    disabled?: boolean;
    size?: 'sm' | 'md';
    class?: string;
    style?: string;
    onchange?: (tabId: string) => void;
  }

  let { 
    tabs, 
    activeTab = $bindable(), 
    disabled = false,
    size = 'md',
    class: className = '',
    style = '',
    onchange
  }: Props = $props();

  function selectTab(id: string) {
    activeTab = id;
    onchange?.(id);
  }
</script>

<div class="tab-bar size-{size} {className}" {style}>
  {#each tabs as tab}
    <button 
      class="tab-btn size-{size} {activeTab === tab.id ? 'active' : ''}" 
      onclick={() => selectTab(tab.id)}
      {disabled}
    >
      {#if tab.icon}
        {@const TabIcon = tab.icon}
        <TabIcon size={size === 'sm' ? 12 : 13} />
      {/if}
      <span>{tab.label}</span>
      {#if tab.count !== undefined && tab.count > 0}
        <span class="tab-count" class:active-count={activeTab === tab.id}>{tab.count}</span>
      {/if}
    </button>
  {/each}
</div>
