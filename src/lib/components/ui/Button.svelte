<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    variant?: 'primary' | 'danger' | 'ghost' | 'outline' | 'default';
    size?: 'sm' | 'md';
    children?: Snippet;
  }

  let { 
    variant = 'default', 
    size = 'md', 
    children, 
    class: className = '',
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived.by(() => {
    let classes = ['btn'];
    if (variant !== 'default') classes.push(`btn-${variant}`);
    if (size === 'sm') classes.push('btn-sm');
    if (className) classes.push(className);
    return classes.join(' ');
  });
</script>

<button class={computedClass} {...rest}>
  {@render children?.()}
</button>
