<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    raised?: boolean;
    children?: Snippet;
  }

  let { 
    raised = false,
    children, 
    class: className = '',
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived(`card${raised ? '-raised' : ''} ${className}`.trim());
</script>

<div class={computedClass} {...rest}>
  {@render children?.()}
</div>
