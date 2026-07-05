<script lang="ts">
  import type { HTMLSelectAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';

  interface Props extends HTMLSelectAttributes {
    label?: string;
    id?: string;
    children?: Snippet;
  }

  let { 
    label,
    id = Math.random().toString(36).substring(2, 9),
    class: className = '',
    value = $bindable(),
    children,
    ...rest 
  }: Props = $props();
</script>

{#if label}
  <div style="display:flex; flex-direction:column; gap:4px; width:100%;">
    <label for={id} style="font-size:12px; color:var(--color-text-secondary);">{label}</label>
    <select {id} class="ui-select {className}" bind:value {...rest}>
      {@render children?.()}
    </select>
  </div>
{:else}
  <select {id} class="ui-select {className}" bind:value {...rest}>
    {@render children?.()}
  </select>
{/if}

<style>
  .ui-select {
    background-color: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--color-border);
    color: var(--color-text-primary);
    padding: 0 12px;
    border-radius: 8px;
    height: 40px;
    box-sizing: border-box;
    font-size: 13px;
    outline: none;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='rgba(255,255,255,0.5)' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
    background-repeat: no-repeat;
    background-position: right 12px center;
    background-size: 14px;
    padding-right: 32px;
    transition: all 0.2s;
  }

  .ui-select:hover:not(:disabled) {
    border-color: rgba(255, 255, 255, 0.15);
  }

  .ui-select:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px var(--color-accent-muted);
  }

  .ui-select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  :global(.ui-select option) {
    background-color: #1a1b26;
    color: #a6accd;
    font-size: 13px;
  }
</style>
