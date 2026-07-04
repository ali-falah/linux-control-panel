<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    checked?: boolean;
    onToggle?: (checked: boolean) => void;
  }

  let { 
    checked = false,
    onToggle,
    class: className = '',
    disabled = false,
    ...rest 
  }: Props = $props();
  
  function handleClick(e: MouseEvent) {
    if (disabled) return;
    if (onToggle) onToggle(!checked);
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
