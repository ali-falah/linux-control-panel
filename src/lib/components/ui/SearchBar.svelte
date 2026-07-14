<script lang="ts">
  import { Search } from '@lucide/svelte';

  interface Props {
    value: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    style?: string;
  }

  let { 
    value = $bindable(), 
    placeholder = 'Search...', 
    disabled = false,
    class: className = '',
    style = ''
  }: Props = $props();
</script>

<div class="search-bar {className}" {style}>
  <Search size={14} style="color:var(--color-text-muted); flex-shrink: 0;" />
  <input 
    type="text"
    bind:value
    {placeholder}
    {disabled}
  />
</div>
