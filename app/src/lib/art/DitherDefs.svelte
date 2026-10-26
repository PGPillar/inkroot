<svelte:options namespace="svg" />

<script lang="ts">
  // A 1-bit checkerboard: two ink squares per tile. Highlighted art parts use fill="url(#id)".
  let { id, cell = 2 }: { id: string; cell?: 2 | 3 } = $props();
</script>

<defs>
  <pattern {id} width={cell * 2} height={cell * 2} patternUnits="userSpaceOnUse">
    <rect width={cell} height={cell} fill="currentColor" stroke="none" />
    <rect x={cell} y={cell} width={cell} height={cell} fill="currentColor" stroke="none" />
  </pattern>
</defs>
