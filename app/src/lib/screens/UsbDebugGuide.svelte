<script lang="ts">
  import { untrack } from 'svelte';
  import { MenuSketch, Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import LinkNote from '../components/LinkNote.svelte';
  import LiveStatus from '../components/LiveStatus.svelte';
  import RefreshTransition from '../components/RefreshTransition.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import {
    USB_GUIDE, USB_GUIDE_BACK, USB_GUIDE_CLOSE, USB_GUIDE_LABEL, USB_GUIDE_LIVE, fill, type GuideStep,
  } from '../copy';
  import { ACTIVITY_LAUNCHER_FDROID, ACTIVITY_LAUNCHER_PLAY } from '../links';

  type Panel = 'a1' | 'a2' | 'a3' | 'b1' | 'b2' | 'b3' | 'b4' | 'allow';
  type Sketch = 'onyxApps' | 'kebab' | 'toggle' | 'activityLauncher' | 'buildNumber' | 'devOptions';

  const app = getApp();

  const METHOD_A: Panel[] = ['a1', 'a2', 'a3', 'allow'];
  const METHOD_B: Panel[] = ['b1', 'b2', 'b3', 'b4', 'allow'];
  const SKETCH: Partial<Record<Panel, Sketch>> = {
    a1: 'onyxApps', a2: 'kebab', a3: 'toggle', b1: 'activityLauncher', b2: 'activityLauncher', b3: 'buildNumber', b4: 'devOptions',
  };
  const LINK_URLS = [ACTIVITY_LAUNCHER_FDROID, ACTIVITY_LAUNCHER_PLAY];

  let panel = $state<Panel>('a1');
  /** Panels visited before this one, for Back. */
  let history = $state<Panel[]>([]);

  const step = $derived(USB_GUIDE.find((s) => s.id === panel) as GuideStep);
  const methodB = $derived(panel.startsWith('b') || (panel === 'allow' && history.some((p) => p.startsWith('b'))));
  const path = $derived(methodB ? METHOD_B : METHOD_A);
  const label = $derived(fill(USB_GUIDE_LABEL, { i: path.indexOf(panel) + 1, n: path.length }));

  const live = $derived.by(() => {
    switch (app.device.kind) {
      case 'unauthorized':
        return { state: 'attention' as const, text: USB_GUIDE_LIVE.unauthorized };
      case 'adb':
        return { state: 'found' as const, text: USB_GUIDE_LIVE.found };
      default:
        return { state: 'waiting' as const, text: USB_GUIDE_LIVE.none };
    }
  });

  function go(to: Panel) {
    history.push(panel);
    panel = to;
  }

  /** The panel after this one in USB_GUIDE order, for a plain Next. */
  function next() {
    const i = USB_GUIDE.findIndex((s) => s.id === panel);
    go(USB_GUIDE[i + 1].id as Panel);
  }

  function back() {
    const prev = history.pop();
    if (prev) panel = prev;
  }

  function close() {
    app.navigate({ screen: 'home' });
  }

  // When the tablet starts asking to be allowed, show where to tap, whatever panel the user is on.
  // Only on the change to unauthorized, so Back from the allow panel still works while it waits.
  let lastKind: string | null = null;
  $effect(() => {
    const kind = app.device.kind;
    untrack(() => {
      if (kind === 'unauthorized' && lastKind !== 'unauthorized' && panel !== 'allow') go('allow');
      lastKind = kind;
    });
  });
</script>

<div class="guide">
  <RefreshTransition key={panel}>
    <StepFrame {label} heading={step.heading} body={step.body}>
      {#snippet art()}
        <figure>
          {#if step.header}<figcaption>{step.header}</figcaption>{/if}
          {#if panel === 'allow'}
            <Tablet screen="prompt" title={step.heading} />
          {:else}
            <MenuSketch variant={SKETCH[panel]} width={300} title={step.heading} />
          {/if}
        </figure>
      {/snippet}

      {#if step.links}
        <p class="links">
          {#each step.links as text, i (text)}
            <button type="button" class="textlink" onclick={() => void app.openExternal(LINK_URLS[i])}>{text}</button>
          {/each}
        </p>
        <LinkNote failed={app.openFailed} urls={LINK_URLS} />
      {/if}
      <LiveStatus state={live.state} text={live.text} />

      {#snippet actions()}
        {#if history.length === 0}
          <Button variant="secondary" onclick={close}>{USB_GUIDE_CLOSE}</Button>
        {:else}
          <Button variant="secondary" onclick={back}>{USB_GUIDE_BACK}</Button>
        {/if}
        {#if step.secondary}
          <Button variant="secondary" onclick={() => go(step.secondaryTo as Panel)}>{step.secondary}</Button>
        {/if}
        {#if step.primary}
          <Button onclick={() => (step.primaryTo ? go(step.primaryTo as Panel) : next())}>{step.primary}</Button>
        {:else if step.next}
          <Button onclick={next}>{step.next}</Button>
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
  /* Back / Close stays on the left even when it is the only button (the allow panel). */
  .guide :global(.actions > :first-child) {
    margin-right: auto;
  }
  figure {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
  }
  figcaption {
    font: var(--fs-chip);
    background: var(--ink);
    color: var(--panel);
    padding: var(--pad-chip);
  }
  .links {
    display: flex;
    gap: var(--space-4);
  }
</style>
