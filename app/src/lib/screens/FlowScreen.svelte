<script lang="ts">
  import { PressIndicator, Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import Checklist from '../components/Checklist.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import PromptView from '../components/PromptView.svelte';
  import SegmentedProgress from '../components/SegmentedProgress.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import {
    CANCELLED_COPY, COMMON, FALLBACK_STEP, SAFETY_CHECKS, STEP_COPY, WRITING_NOTE, fill, stepLabel,
  } from '../copy';
  import { formatBytes } from '../format';
  import type { FlowState, StepId } from '../ipc';
  import CableGuide from './CableGuide.svelte';
  import DoneView from './DoneView.svelte';
  import FailureView from './FailureView.svelte';
  import RescueWait from './RescueWait.svelte';

  type Art = { tablet: 'blank' | 'logo' | 'content' } | { press: true } | { checks: true } | { icon: 'backup' } | null;

  const app = getApp();
  // Shell only renders this screen while a flow exists.
  const flow = $derived(app.flow as FlowState);

  const copy = $derived(STEP_COPY[flow.kind]?.[flow.step] ?? FALLBACK_STEP);
  const label = $derived(stepLabel(flow.kind, flow.stepIndex, flow.stepCount));
  const cable = $derived(flow.kind === 'rescue' && (flow.step === 'cableGuide' || flow.step === 'waitEdl'));

  const progress = $derived(app.progress);
  const meta = $derived.by(() => {
    if (!progress) return undefined;
    const { label: what, done, total } = progress;
    return total >= 1024 ? `${what} · ${formatBytes(done)} / ${formatBytes(total)}` : `${what} · ${done} / ${total}`;
  });

  const TABLET: Partial<Record<StepId, 'blank' | 'logo' | 'content'>> = {
    enterEdl: 'blank', write: 'blank', verifyWrite: 'blank', waitEdl: 'blank', cableGuide: 'blank',
    rebootToSystem: 'logo', reset: 'logo',
    checkDevice: 'content', patch: 'content', verifyRoot: 'content', pushScripts: 'content',
  };
  const picture = $derived.by((): Art => {
    const { kind, step } = flow;
    if (kind === 'root' && (step === 'readPartitions' || step === 'verifyBackup')) return { checks: true };
    if (step === 'waitBoot') return { press: true };
    if (step === 'pickImage' || step === 'stockCheck' || step === 'chooseSlot' || step === 'saveImport') return { icon: 'backup' };
    const screen = TABLET[step];
    return screen ? { tablet: screen } : null;
  });

  // The tablet is in rescue mode while the copy is made, so its details come from when it was last seen.
  const checks = $derived.by(() => {
    const info = app.deviceInfo ?? app.lastSeen;
    const verifying = flow.step === 'verifyBackup';
    return [
      ...(info
        ? [
            { label: fill(SAFETY_CHECKS.firmware, { firmwareLabel: info.firmwareLabel }), done: true },
            { label: fill(SAFETY_CHECKS.slot, { S: info.slot.toUpperCase() }), done: true },
          ]
        : []),
      { label: SAFETY_CHECKS.saved, done: verifying },
      { label: SAFETY_CHECKS.checked, done: false },
    ];
  });

  const imageName = $derived(app.lastStart?.options.imagePath?.split(/[\\/]/).pop());
</script>

{#if flow.status === 'done'}
  <DoneView {flow} />
{:else if flow.status === 'failed'}
  <FailureView {flow} />
{:else if flow.status === 'cancelled'}
  <StepFrame label={CANCELLED_COPY.label} heading={CANCELLED_COPY.heading} body={CANCELLED_COPY.body}>
    {#snippet art()}
      <Tablet screen="content" width={120} title={CANCELLED_COPY.heading} />
    {/snippet}
    {#snippet actions()}
      <Button onclick={() => app.dismissFlow()}>{CANCELLED_COPY.back}</Button>
    {/snippet}
  </StepFrame>
{:else if flow.kind === 'rescue' && flow.step === 'waitBoot'}
  <RescueWait {flow} />
{:else if cable && (!flow.prompt || flow.prompt.kind === 'cableGuide')}
  <!-- One branch for the guide with and without its prompt, so answering it doesn't remount it. -->
  <CableGuide />
{:else if flow.prompt}
  <PromptView
    prompt={flow.prompt}
    kind={flow.kind}
    device={app.device}
    found={app.edlFound}
    {imageName}
    disabled={app.inFlight}
    onAnswer={(id) => void app.answer(id)}
    onOpen={(url) => void app.openExternal(url)}
    onCopy={(text) => void app.platform.copyText(text).catch(() => {})}
  />
{:else}
  <StepFrame {label} heading={copy.heading} body={copy.body}>
    {#snippet art()}
      {#if picture && 'checks' in picture}
        <div class="checks"><Checklist items={checks} /></div>
      {:else if picture && 'tablet' in picture}
        <Tablet screen={picture.tablet} width={132} title={copy.heading} />
      {:else if picture && 'press' in picture}
        <PressIndicator width={132} title={copy.heading} />
      {:else if picture && 'icon' in picture}
        <DitherIcon name={picture.icon} size={96} cell={4} />
      {/if}
    {/snippet}

    <SegmentedProgress done={progress?.done} total={progress?.total} {meta} />

    {#snippet actions()}
      <!-- Cancel sits on the left, as in the cable guide; while writing, the note takes its place. -->
      <div class="lead">
        {#if flow.writing}
          <p class="writing" role="status"><span aria-hidden="true">●</span> {WRITING_NOTE}</p>
        {:else}
          <Button variant="secondary" disabled={app.inFlight} onclick={() => void app.cancel()}>{COMMON.cancel}</Button>
        {/if}
      </div>
    {/snippet}
  </StepFrame>
{/if}

<style>
  .checks {
    align-self: stretch;
    width: 100%;
    display: flex;
    align-items: flex-start;
    padding-top: var(--space-3);
  }
  .lead {
    display: flex;
    align-items: center;
    min-height: 48px;
    margin-right: auto;
  }
  .writing {
    display: flex;
    gap: var(--space-2);
    padding: var(--pad-chip);
    font: var(--fs-chip);
    background: var(--ink);
    color: var(--panel);
  }
</style>
