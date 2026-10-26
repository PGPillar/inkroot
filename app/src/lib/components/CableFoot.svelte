<script lang="ts">
  import { CABLE_GUIDE_FOOT } from '../copy';
  import LiveStatus from './LiveStatus.svelte';

  /** The rescue reassurance line and the live "waiting / found" status, under the cable guide and its prompts. */
  let { found }: { found: boolean } = $props();
</script>

<div class="foot">
  <p class="reassure">{CABLE_GUIDE_FOOT.reassurance}</p>
  <LiveStatus state={found ? 'found' : 'waiting'} text={found ? CABLE_GUIDE_FOOT.found : CABLE_GUIDE_FOOT.waiting} />
</div>

<style>
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-4);
    padding-top: var(--space-2);
    border-top: var(--border);
  }
  .reassure {
    margin: 0;
    font: 600 14px/1.4 var(--font-sans);
  }
  .foot :global(.live) {
    flex: none;
  }
</style>
