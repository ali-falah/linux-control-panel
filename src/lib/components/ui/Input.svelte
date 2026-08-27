<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  interface Props extends HTMLInputAttributes {
    label?: string;
    id?: string;
  }

  let { 
    label,
    id = Math.random().toString(36).substring(2, 9),
    class: className = '',
    type = 'text',
    value = $bindable(),
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived(`input ${className}`.trim());
</script>

{#if label}
  <div style="display:flex; flex-direction:column; gap:4px; width:100%;">
    <label for={id} style="font-size:12px; color:var(--color-text-secondary);">{label}</label>
    <input {id} {type} class={computedClass} bind:value {...rest} />
  </div>
{:else}
  <input {id} {type} class={computedClass} bind:value {...rest} />
{/if}
