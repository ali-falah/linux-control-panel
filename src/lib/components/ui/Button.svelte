<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    variant?: 'primary' | 'danger' | 'ghost' | 'outline' | 'default' | 'secondary';
    size?: 'xs' | 'sm' | 'md' | 'lg';
    children?: Snippet;
    disabled?: boolean;
    loading?: boolean;
  }

  let { 
    variant = 'default', 
    size = 'md', 
    children, 
    class: className = '',
    disabled = false,
    loading = false,
    ...rest 
  }: Props = $props();
  
  let computedClass = $derived.by(() => {
    let classes = ['btn'];
    if (variant !== 'default') classes.push(`btn-${variant}`);
    if (size === 'sm' || size === 'xs') classes.push('btn-sm');
    if (size === 'lg') classes.push('btn-lg');
    if (disabled || loading) classes.push('btn-disabled');
    if (loading) classes.push('btn-loading');
    if (className) classes.push(String(className));
    return classes.join(' ');
  });
</script>

<button class={computedClass} disabled={disabled || loading} aria-disabled={disabled || loading} {...rest}>
  {@render children?.()}
</button>
