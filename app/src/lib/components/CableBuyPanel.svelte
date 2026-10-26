<script lang="ts">
  import { CABLE_BUY } from '../copy';
  import { DeepFlashCable } from '../art';
  import Button from './Button.svelte';
  import StepFrame from './StepFrame.svelte';

  let { onBack, onCopy }: { onBack: () => void; onCopy: (text: string) => void } = $props();
</script>

<StepFrame label={CABLE_BUY.label} heading={CABLE_BUY.heading} body={CABLE_BUY.body}>
  {#snippet art()}<DeepFlashCable title={CABLE_BUY.heading} highlight="button" />{/snippet}
  {#snippet actions()}
    <Button variant="secondary" onclick={onBack}>{CABLE_BUY.back}</Button>
    <Button variant="primary" onclick={() => onCopy(CABLE_BUY.copyValue)}>{CABLE_BUY.copy}</Button>
  {/snippet}
</StepFrame>
