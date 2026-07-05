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

<div class="search-bar-container {className}" {style}>
  <Search size={14} style="color:var(--color-text-muted); flex-shrink: 0;" />
  <input 
    type="text"
    bind:value
    {placeholder}
    {disabled}
    class="search-input"
  />
</div>

<style>
  .search-bar-container {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    padding: 0 12px;
    border-radius: 8px;
    height: 40px;
    box-sizing: border-box;
    min-width: 200px;
  }

  .search-input {
    background: transparent !important;
    border: none !important;
    outline: none !important;
    box-shadow: none !important;
    color: var(--color-text-primary);
    width: 100%;
    font-size: 13px;
    padding: 0 !important;
    height: 100%;
  }

  .search-input::placeholder {
    color: var(--color-text-muted);
  }
</style>
