<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLTableElement> {
    children?: Snippet;
  }

  let { 
    children, 
    class: className = '',
    ...rest 
  }: Props = $props();
  
</script>

<div class="table-wrap" style="overflow-x: auto;">
  <table class={className} {...rest}>
    {@render children?.()}
  </table>
</div>
