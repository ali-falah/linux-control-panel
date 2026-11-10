<script lang="ts">
  let {
    value = '',
    onchange,
  }: {
    value?: string;
    height?: string;
    onchange?: (value: string) => void;
  } = $props();
</script>

<textarea
  aria-label="unit content"
  {value}
  oninput={(e) => onchange?.(e.currentTarget.value)}
></textarea>
