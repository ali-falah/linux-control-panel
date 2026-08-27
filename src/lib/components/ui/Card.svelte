<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    raised?: boolean;
    title?: string;
    icon?: any; // Lucide icon component type
    children?: Snippet;
  }

  let { 
    raised = false,
    title,
    icon: Icon,
    children, 
    class: className = '',
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived(`card${raised ? '-raised' : ''} ${className}`.trim());
</script>

<div class={computedClass} {...rest}>
  {#if title || Icon}
    <h3 class="card-title-header">
      {#if Icon}
        <Icon size={16} class="card-title-icon" />
      {/if}
      <span>{title}</span>
    </h3>
  {/if}
  {@render children?.()}
</div>
