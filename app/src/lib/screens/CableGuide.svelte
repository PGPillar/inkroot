<script lang="ts" module>
  /**
   * The panel each rescue flow is on, per app session, so a prompt that takes over the screen
   * ("not found yet") doesn't lose the user's place.
   */
  const remembered = new WeakMap<object, Map<string, number>>();
</script>

<script lang="ts">
  import CableScene, { type CableSceneId } from '../art/CableScene.svelte';
  import Button from '../components/Button.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import CableFoot from '../components/CableFoot.svelte';
  import RefreshTransition from '../components/RefreshTransition.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import {
    CABLE_BUY, CABLE_GUIDE, CABLE_GUIDE_LABEL, CABLE_GUIDE_NAV, COMMON, fill,
  } from '../copy';

  const app = getApp();
  const flowId = app.flow!.flowId;

  const SCENE: Record<string, CableSceneId> = { c1: 'cable', c2: 'port', c3: 'press', c4: 'plug', c5: 'power', c6: 'hold' };
  /** Drawn height of each scene, so every panel's art reads at about the same size. */
  const HEIGHT: Record<CableSceneId, number> = { cable: 150, port: 196, press: 236, plug: 224, power: 236, hold: 220 };
  const TIMER: Record<string, number> = { c5: 15, c6: 10 };
  /** "Hold the cable's button": where the sequence starts again after the tablet wasn't found. */
  const HOLD_PANEL = 2;

  // The guide only remounts after a prompt took over; then it starts again from holding the button.
  const places = remembered.get(app) ?? new Map<string, number>();
  remembered.set(app, places);
  let panel = $state(Math.min(places.get(flowId) ?? 0, HOLD_PANEL));
  $effect(() => void places.set(flowId, panel));

  const step = $derived(CABLE_GUIDE[panel]);
  const last = $derived(panel === CABLE_GUIDE.length - 1);
  const label = $derived(fill(CABLE_GUIDE_LABEL, { i: app.flow?.stepIndex ?? 2, p: panel + 1 }));
  const found = $derived(app.edlFound);

  // The hold timer counts down in whole seconds, in text only. It belongs to the panel it started on.
  let timer = $state<{ panel: number; left: number } | null>(null);
  let tick: ReturnType<typeof setInterval> | undefined;
  const timerHere = $derived(timer?.panel === panel ? timer : null);
  const seconds = $derived(TIMER[step.id] === undefined ? undefined : (timerHere?.left ?? TIMER[step.id]));

  function startTimer() {
    if (timerHere && timerHere.left > 0) return;
    clearInterval(tick);
    timer = { panel, left: TIMER[step.id] };
    tick = setInterval(() => {
      if (!timer) return;
      timer.left -= 1;
      if (timer.left <= 0) clearInterval(tick);
    }, 1000);
  }

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyTerms() {
    try {
      await app.platform.copyText(CABLE_BUY.copyValue);
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      // The search terms are on screen in the text above; nothing else to do.
    }
  }

  $effect(() => () => {
    clearInterval(tick);
    clearTimeout(copiedTimer);
  });

  function go(to: number) {
    panel = to;
  }

  function secondary() {
    if (step.id === 'c1') void copyTerms();
    else if (TIMER[step.id] !== undefined) startTimer();
  }

  const secondaryLabel = $derived.by(() => {
    if (step.id === 'c1' && copied) return COMMON.copied;
    if (timerHere) return timerHere.left > 0 ? fill(CABLE_GUIDE_NAV.timer, { n: timerHere.left }) : CABLE_GUIDE_NAV.timerDone;
    return step.secondary;
  });
</script>

<div class="guide">
  <RefreshTransition key={panel}>
    <StepFrame {label} heading={step.heading} body={step.body}>
      {#snippet art()}
        <CableScene scene={SCENE[step.id]} height={HEIGHT[SCENE[step.id]]} {seconds} title={step.heading} />
      {/snippet}

      {#if step.note}
        <p class="note"><DitherIcon name="info" size={18} cell={2} /><span>{step.note}</span></p>
      {/if}
      <CableFoot {found} />

      {#snippet actions()}
        <Button variant="secondary" disabled={app.inFlight} onclick={() => void app.cancel()}>{COMMON.cancel}</Button>
        {#if panel > 0}
          <Button variant="secondary" onclick={() => go(panel - 1)}>{CABLE_GUIDE_NAV.back}</Button>
        {/if}
        {#if step.secondary}
          <Button variant="secondary" onclick={secondary}>{secondaryLabel}</Button>
        {/if}
        {#if !last}
          <Button onclick={() => go(panel + 1)}>{step.primary ?? step.next}</Button>
        {/if}
      {/snippet}
    </StepFrame>
  </RefreshTransition>
</div>

<style>
  .guide {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .guide > :global(.flash) {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    max-width: 72ch;
    font: 400 14px/1.45 var(--font-sans);
    color: var(--muted);
  }
  .note :global(.ico) {
    margin-top: 1px;
    color: var(--ink);
  }
</style>
