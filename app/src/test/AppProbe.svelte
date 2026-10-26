<script lang="ts">
  import { getApp } from '../lib/context';
  let { label = 'screen' }: { label?: string } = $props();
  const app = getApp();
</script>

<p>{label}: {app.screen}</p>
