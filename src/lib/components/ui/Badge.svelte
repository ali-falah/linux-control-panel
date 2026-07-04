<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLSpanElement> {
    variant?: 'success' | 'error' | 'warning' | 'info' | 'muted' | 'outline';
    children?: Snippet;
  }

  let { 
    variant = 'muted',
    children, 
    class: className = '',
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived(`badge badge-${variant} ${className}`.trim());
</script>

<span class={computedClass} {...rest}>
  {@render children?.()}
</span>
