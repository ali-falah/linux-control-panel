<script lang="ts">
  import SideDrawer from '../SideDrawer.svelte';

  let { onvisibility }: { onvisibility?: (open: boolean) => void } = $props();
  let open = $state(true);

  $effect(() => {
    onvisibility?.(open);
  });
</script>

<SideDrawer bind:isOpen={open} title="Unit Override" />
