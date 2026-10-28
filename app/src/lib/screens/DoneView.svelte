<script lang="ts">
  import { Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import Checklist from '../components/Checklist.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import LinkNote from '../components/LinkNote.svelte';
  import LiveStatus from '../components/LiveStatus.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import {
    COMMON, DONE_COPY, DONE_IMPORT_RESCUE_ONLY, RESCUE_BACK, RESCUE_DONE, ROOT_MORE, ROOT_TIP, STEP_DONE_LABEL, fill,
  } from '../copy';
  import { revealLabel } from '../format';
  import type { FlowState } from '../ipc';
  import { BOOT_SERVICE_DOCS_URL } from '../links';

  let { flow }: { flow: FlowState } = $props();

  const app = getApp();
  const backup = $derived(flow.result?.backup);
  const rescueOnly = $derived(flow.kind === 'importBackup' && backup?.stock === false);
  // Rescue says how it ended; an older backend without an outcome reads as "not seen".
  const outcome = $derived(flow.kind === 'rescue' ? (flow.result?.rescueOutcome ?? 'notSeen') : null);
  const copy = $derived(
    outcome ? RESCUE_DONE[outcome] : rescueOnly ? DONE_IMPORT_RESCUE_ONLY : DONE_COPY[flow.kind],
  );
  const started = $derived(outcome === 'tabletBack' || outcome === 'homeScreenSeen');
  const rooted = $derived(outcome === 'tabletBack' ? flow.result?.device?.rooted : undefined);
  const body = $derived(
    fill(copy.body, {
      path: backup?.path ?? app.info?.backupRoot ?? '',
      magiskVersion: flow.result?.magiskVersion ?? app.deviceInfo?.magiskVersion ?? '',
    }),
  );
  const checks = $derived((rescueOnly ? [] : (DONE_COPY[flow.kind].checks ?? [])).map((label) => ({ label, done: true })));
  // Not seen during the wait, but the device poller may still spot it once USB debugging allows.
  const back = $derived(app.device.kind === 'adb');
</script>

<StepFrame label={STEP_DONE_LABEL} heading={copy.heading} {body}>
  {#snippet art()}
    {#if outcome === 'backInEdl'}
      <Tablet screen="blank" highlight="power" width={120} title={copy.heading} />
    {:else if outcome}
      <Tablet screen={started || back ? 'content' : 'logo'} width={120} title={copy.heading} />
    {:else if flow.kind === 'root'}
      <div class="split">
        <Checklist items={checks} />
        <aside class="tip">
          <DitherIcon name="info" size={28} cell={2} />
          <div>
            <h2>{ROOT_TIP.heading}</h2>
            <p>{ROOT_TIP.body}</p>
          </div>
        </aside>
      </div>
    {:else if checks.length > 0}
      <div class="split"><Checklist items={checks} /></div>
    {:else}
      <DitherIcon name={rescueOnly ? 'rescue' : 'check'} size={88} cell={4} />
    {/if}
  {/snippet}

  {#if rooted !== undefined}
    <p class="note">{rooted ? RESCUE_DONE.rooted : RESCUE_DONE.notRooted}</p>
  {:else if outcome === 'notSeen'}
    <p class="note">{RESCUE_DONE.notSeen.note}</p>
    {#if back}<LiveStatus state="found" text={RESCUE_BACK.found} />{/if}
  {:else if flow.kind === 'root'}
    <LinkNote failed={app.openFailed} urls={[BOOT_SERVICE_DOCS_URL]} />
  {/if}

  {#snippet actions()}
    {#if flow.kind === 'root'}
      <button type="button" class="textlink" onclick={() => void app.openExternal(BOOT_SERVICE_DOCS_URL)}>{ROOT_MORE}</button>
    {:else if outcome === 'backInEdl'}
      <Button variant="secondary" onclick={() => app.goToRescue()}>{RESCUE_DONE.again}</Button>
    {:else if flow.kind === 'backup' && backup && app.info}
      <Button variant="secondary" onclick={() => void app.reveal(backup.path)}>{revealLabel(app.info.os)}</Button>
    {/if}
    <Button onclick={() => app.dismissFlow()}>{COMMON.finish}</Button>
  {/snippet}
</StepFrame>

<style>
  .split {
    align-self: stretch;
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.35fr);
    gap: var(--space-6);
    align-items: start;
    padding-top: var(--space-3);
  }
  .tip {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-4);
    border: var(--border);
  }
  .tip :global(.ico) {
    color: var(--ink);
  }
  h2 {
    margin: 0 0 var(--space-1);
    font: 600 17px/1.3 var(--font-sans);
  }
  .note {
    margin: 0;
    max-width: 52ch;
    font: var(--fs-body);
  }
  .tip p {
    margin: 0;
    font: 400 14px/1.5 var(--font-sans);
  }
</style>
