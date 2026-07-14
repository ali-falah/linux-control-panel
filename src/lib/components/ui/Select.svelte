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
    <label for={id} style="font-size:10px; font-weight:700; letter-spacing:0.06em; text-transform:uppercase; color:var(--color-text-muted);">{label}</label>
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
    background-color: rgba(1, 15, 31, 0.7);
    border: 1px solid #3b494c;
    color: var(--color-text-primary);
    padding: 0 32px 0 12px;
    border-radius: 8px;
    height: 36px;
    box-sizing: border-box;
    font-size: 12px;
    font-family: var(--font-sans);
    outline: none;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23849396' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
    background-repeat: no-repeat;
    background-position: right 10px center;
    background-size: 13px;
    transition: all 0.15s ease;
  }

  .ui-select:hover:not(:disabled) {
    border-color: rgba(0, 218, 243, 0.3);
    background-color: rgba(1, 15, 31, 0.9);
  }

  .ui-select:focus {
    border-color: var(--color-accent);
    box-shadow:
      inset 0 1px 3px rgba(0, 0, 0, 0.3),
      0 0 0 2px rgba(0, 218, 243, 0.10),
      0 0 8px rgba(0, 218, 243, 0.12);
    background-color: rgba(1, 15, 31, 0.9);
  }

  .ui-select:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  :global(.ui-select option) {
    background-color: #0d1c2d;
    color: #d4e4fa;
    font-size: 12px;
  }
</style>
