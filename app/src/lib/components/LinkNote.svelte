<script lang="ts">
  import { OPEN_FAILED } from '../copy';

  /** A short note under links when the system browser couldn't open one of them (`failed`). */
  let { failed, urls }: { failed: string | null; urls: readonly string[] } = $props();
</script>

{#if failed !== null && urls.includes(failed)}
  <p class="linknote" role="status">{OPEN_FAILED}</p>
{/if}

<style>
  .linknote {
    margin: 0;
    font: var(--fs-meta);
  }
</style>
