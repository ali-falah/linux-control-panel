<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Action } from 'svelte/action';

  interface Props extends HTMLAttributes<HTMLTableElement> {
    children?: Snippet;
    tableAction?: Action<HTMLTableElement, any>;
    actionParams?: any;
  }

  let { 
    children, 
    class: className = '',
    tableAction,
    actionParams,
    ...rest 
  }: Props = $props();
  
</script>

<div class="table-wrap" style="overflow-x: auto; flex: 1; min-height: 0;">
  {#if tableAction}
    <table class={className} use:tableAction={actionParams} {...rest}>
      {@render children?.()}
    </table>
  {:else}
    <table class={className} {...rest}>
      {@render children?.()}
    </table>
  {/if}
</div>
