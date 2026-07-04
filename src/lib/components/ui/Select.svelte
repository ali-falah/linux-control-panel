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
  
  let computedClass = $derived(`input ${className}`.trim());
</script>

{#if label}
  <div style="display:flex; flex-direction:column; gap:4px; width:100%;">
    <label for={id} style="font-size:12px; color:var(--color-text-secondary);">{label}</label>
    <select {id} class={computedClass} bind:value {...rest}>
      {@render children?.()}
    </select>
  </div>
{:else}
  <select {id} class={computedClass} bind:value {...rest}>
    {@render children?.()}
  </select>
{/if}
