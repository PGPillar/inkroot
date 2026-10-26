<script lang="ts">
  import { formatChip } from '../format';
  import type { DeviceStatus } from '../ipc';

  let { status }: { status: DeviceStatus } = $props();
</script>

<span class="chip" aria-live="polite">{formatChip(status)}</span>

<style>
  .chip {
    background: var(--ink);
    color: var(--panel);
    font: var(--fs-chip);
    padding: var(--pad-chip);
    white-space: nowrap;
  }
</style>
