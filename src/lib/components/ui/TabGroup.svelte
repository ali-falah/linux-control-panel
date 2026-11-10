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

  let sizeClass = $derived(size === 'sm' ? 'size-sm' : '');

  function selectTab(id: string) {
    activeTab = id;
    onchange?.(id);
  }
</script>

<div class="tab-bar {sizeClass} {className}" {style}>
  {#each tabs as tab}
    {@const TabIcon = tab.icon}
    <button
      class="tab-btn {sizeClass} {activeTab === tab.id ? 'active' : ''}"
      onclick={() => selectTab(tab.id)}
      {disabled}
    >
      {#if TabIcon}
        <TabIcon size={13} />
      {/if}
      {tab.label}
      {#if tab.count !== undefined && tab.count > 0}
        <span class="tab-count" class:active-count={activeTab === tab.id}>{tab.count}</span>
      {/if}
    </button>
  {/each}
</div>
