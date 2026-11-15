<script lang="ts" generics="T extends string">
  import ToggleButton from './ToggleButton.svelte';

  export interface FilterOption<T extends string = string> {
    id: T;
    label: string;
    /** Shown as a small pill after the label; omit for no counter. */
    count?: number;
    icon?: any;
    title?: string;
  }

  interface Props<T extends string> {
    options: FilterOption<T>[];
    value?: T;
    /** Accessible name of the group, e.g. "Filter repositories by state". */
    label?: string;
    size?: 'sm' | 'md';
    disabled?: boolean;
    class?: string;
    style?: string;
    onchange?: (id: T) => void;
  }

  let {
    options,
    value = $bindable(),
    label = 'Filters',
    size = 'sm',
    disabled = false,
    class: className = '',
    style = '',
    onchange,
  }: Props<T> = $props();

  function select(id: T) {
    if (disabled || value === id) return;
    value = id;
    onchange?.(id);
  }
</script>

<!-- Exclusive filter group: one option is always active, so these are pressed
     buttons, not tabs — the list they filter is not a tab panel. -->
<div class="filter-chips {className}" role="group" aria-label={label} {style}>
  {#each options as option (option.id)}
    {@const Icon = option.icon}
    <ToggleButton
      size={size}
      controlled
      {disabled}
      active={value === option.id}
      aria-pressed={value === option.id}
      title={option.title ?? `${option.label}${option.count !== undefined ? ` (${option.count})` : ''}`}
      onclick={() => select(option.id)}
    >
      {#if Icon}
        <Icon size={12} />
      {/if}
      {option.label}
      {#if option.count !== undefined}
        <span class="filter-count">{option.count}</span>
      {/if}
    </ToggleButton>
  {/each}
</div>

<style>
  .filter-chips {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .filter-count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 700;
    color: var(--color-text-muted);
    background: var(--color-bg-raised);
    border-radius: 999px;
    padding: 1px 6px;
    line-height: 1.3;
  }
</style>
