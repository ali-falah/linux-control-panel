<script lang="ts">
  let {
    width = '100%',
    height = '20px',
    borderRadius = '4px',
    style = ''
  } = $props<{
    width?: string;
    height?: string;
    borderRadius?: string;
    style?: string;
  }>();
</script>

<div 
  class="skeleton-loader" 
  style="width: {width}; height: {height}; border-radius: {borderRadius}; {style}"
></div>

<style>
  .skeleton-loader {
    background: linear-gradient(
      90deg,
      var(--color-bg-raised) 0%,
      var(--color-bg-hover) 50%,
      var(--color-bg-raised) 100%
    );
    background-size: 200% 100%;
    animation: skeleton-loading 1.5s infinite linear;
    opacity: 0.7;
  }

  @keyframes skeleton-loading {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }
</style>
