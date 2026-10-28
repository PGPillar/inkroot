<script lang="ts">
  import { Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { RESCUE_WAIT, STEP_COPY, stepLabel } from '../copy';
  import type { FlowState } from '../ipc';

  let { flow }: { flow: FlowState } = $props();

  const app = getApp();
  const copy = $derived(STEP_COPY.rescue.waitBoot!);
  // The backend offers "I can see my home screen" while it watches; the button waits for that offer.
  const offer = $derived(flow.prompt?.kind === 'tabletStarting' ? flow.prompt : null);
</script>

<StepFrame label={stepLabel(flow.kind, flow.stepIndex, flow.stepCount)} heading={copy.heading} body={copy.body}>
  {#snippet art()}
    <div class="split">
      <ul class="steps" aria-label="Rescue progress">
        {#each RESCUE_WAIT.checklist as line, i (i)}
          <li class:pending={i === RESCUE_WAIT.checklist.length - 1}>{line}</li>
        {/each}
      </ul>
      <Tablet screen="logo" width={120} title={RESCUE_WAIT.checklist[2]} />
    </div>
  {/snippet}

  {#snippet actions()}
    <Button disabled={!offer || app.inFlight} onclick={() => void app.answer('homeScreen')}>{RESCUE_WAIT.done}</Button>
  {/snippet}
</StepFrame>

<style>
  .split {
    align-self: stretch;
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: var(--space-6);
    align-items: center;
    padding-top: var(--space-3);
  }
  .split :global(svg) {
    justify-self: center;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font: var(--fs-body);
  }
  .pending {
    opacity: var(--opacity-label);
  }
</style>
