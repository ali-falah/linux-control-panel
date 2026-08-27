<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    checked?: boolean;
    onToggle?: (checked: boolean) => void;
    onchange?: (checked: boolean) => void;
  }

  let { 
    checked = $bindable(false),
    onToggle,
    onchange,
    class: className = '',
    disabled = false,
    ...rest 
  }: Props = $props();
  
  function handleClick(e: MouseEvent) {
    if (disabled) return;
    const nextVal = !checked;
    checked = nextVal;
    if (onToggle) onToggle(nextVal);
    if (onchange) onchange(nextVal);
  }
</script>

<button
  type="button"
  class="ui-toggle {checked ? 'on' : ''} {className}"
  {disabled}
  onclick={handleClick}
  role="switch"
  aria-checked={checked}
  {...rest}
>
  <span class="ui-toggle-thumb"></span>
</button>
