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

  function handleKeydown(event: KeyboardEvent, index: number) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight' && event.key !== 'Home' && event.key !== 'End') {
      return;
    }
    event.preventDefault();
    let next = index;
    if (event.key === 'ArrowLeft') next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = tabs.length - 1;

    const tab = tabs[next];
    if (!tab) return;
    selectTab(tab.id);
    const buttons = (event.currentTarget as HTMLElement)
      .parentElement
      ?.querySelectorAll<HTMLButtonElement>('.tab-btn');
    buttons?.[next]?.focus();
  }
</script>

<div class="tab-bar {sizeClass} {className}" {style} role="tablist" aria-label="Tabs">
  {#each tabs as tab, index}
    {@const TabIcon = tab.icon}
    <button
      class="tab-btn {sizeClass} {activeTab === tab.id ? 'active' : ''}"
      role="tab"
      aria-selected={activeTab === tab.id}
      tabindex={activeTab === tab.id ? 0 : -1}
      onclick={() => selectTab(tab.id)}
      onkeydown={(e) => handleKeydown(e, index)}
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
