<script lang="ts">
  import type { Snippet } from 'svelte';

  let { key, children }: { key: unknown; children: Snippet } = $props();
</script>

{#key key}
  <div class="flash">{@render children()}</div>
{/key}
