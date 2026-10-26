<script lang="ts">
  import { Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { ACTION_LABELS, COMMON, ERROR_HEADINGS, STEP_FAILED_LABEL } from '../copy';
  import type { FlowError, FlowState } from '../ipc';

  let { flow }: { flow: FlowState } = $props();

  const app = getApp();
  const error = $derived(flow.error);
  // toFlowError passes any backend code through, so an unknown one gets the generic heading.
  const heading = $derived((error && Object.hasOwn(ERROR_HEADINGS, error.code) ? ERROR_HEADINGS[error.code] : null) ?? ERROR_HEADINGS.internal);

  /** What "Try again" restarts: the job the user started, if this failure belongs to it. */
  const retry = $derived.by(() => {
    const last = app.lastStart;
    if (!last) return null;
    // Reinstall can turn into a full root under the same flow, so its failure retries the reinstall.
    const same = last.kind === flow.kind || (last.kind === 'reinstallBootService' && flow.kind === 'root');
    return same ? last : null;
  });

  const action = $derived.by((): NonNullable<FlowError['suggestedAction']> | null => {
    const a = error?.suggestedAction ?? null;
    if (a === null || !Object.hasOwn(ACTION_LABELS, a)) return null;
    return a === 'retry' && !retry ? null : a;
  });

  function primary() {
    switch (action) {
      case 'retry': {
        const { kind, options } = retry!;
        app.dismissFlow();
        void app.startJob(kind, options);
        break;
      }
      case 'goToRescue':
        app.goToRescue();
        break;
      case 'importBackup':
        app.dismissFlow();
        void app.importBackup();
        break;
      default:
        app.dismissFlow();
    }
  }
</script>

<StepFrame label={STEP_FAILED_LABEL} {heading} body={error?.message}>
  {#snippet art()}
    <div class="art">
      <Tablet screen="blank" width={120} title={heading} />
      <DitherIcon name="warning" size={56} />
    </div>
  {/snippet}

  <p class="foot">{COMMON.failedFoot}</p>

  {#snippet actions()}
    <Button variant={action ? 'secondary' : 'primary'} onclick={() => app.dismissFlow()}>{COMMON.backToStart}</Button>
    {#if action}
      <Button onclick={primary}>{ACTION_LABELS[action]}</Button>
    {/if}
  {/snippet}
</StepFrame>

<style>
  .art {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }
  .foot {
    font: var(--fs-meta);
    color: var(--muted);
  }
</style>
