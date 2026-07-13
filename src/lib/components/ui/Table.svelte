<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Action } from 'svelte/action';

  interface Props {
    children?: Snippet;
    tableAction?: Action<HTMLTableElement, any>;
    actionParams?: any;
    class?: string;
    style?: string;
    onscroll?: (e: Event) => void;
    [key: string]: any;
  }

  let { 
    children, 
    class: className = '',
    tableAction,
    actionParams,
    style = '',
    onscroll,
    ...rest 
  }: Props = $props();
</script>

<div class="table-wrap" {style} {onscroll}>
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
