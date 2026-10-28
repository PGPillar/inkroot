<script lang="ts">
  import AppHeader from './components/AppHeader.svelte';
  import CloseGuardDialog from './components/CloseGuardDialog.svelte';
  import DetailsLog from './components/DetailsLog.svelte';
  import RefreshTransition from './components/RefreshTransition.svelte';
  import { getApp } from './context';
  import { STEP_COPY, fill, type GateId } from './copy';
  import About from './screens/About.svelte';
  import AlreadyRooted from './screens/AlreadyRooted.svelte';
  import GateScreen from './screens/GateScreen.svelte';
  import Backups from './screens/Backups.svelte';
  import Home from './screens/Home.svelte';
  import RescuePick from './screens/RescuePick.svelte';
  import RiskNotice from './screens/RiskNotice.svelte';
  import UsbDebugGuide from './screens/UsbDebugGuide.svelte';
  import FlowScreen from './screens/FlowScreen.svelte';

  const app = getApp();

  const gate = $derived(app.screen.startsWith('gate-') ? (app.screen.slice(5) as GateId) : null);

  // The guide closes itself once the tablet is authorised; drop the route so it doesn't reopen later.
  $effect(() => {
    if (app.route.screen === 'usbGuide' && app.screen !== 'usbGuide') app.navigate({ screen: 'home' });
  });

  // The close guard only speaks while a write is under way; once it ends (done, failed or the
  // write step finished) the warning is stale, so put it away.
  $effect(() => {
    if (app.closeBlocked && !app.flow?.writing) app.closeBlocked = false;
  });

  const flow = $derived(app.flow);
  // The cable guide pages through its own panels (with their own refresh); the engine moving from
  // cableGuide to waitEdl underneath must not reset it.
  const cableGuide = $derived(
    flow?.kind === 'rescue' && flow.status === 'running' && (flow.step === 'cableGuide' || flow.step === 'waitEdl'),
  );
  // One e-ink refresh per screen, per step, per status and per prompt.
  const refreshKey = $derived(
    cableGuide ? 'flow:cableGuide' : `${app.screen}:${flow?.step ?? ''}:${flow?.status ?? ''}:${flow?.prompt?.id ?? ''}`,
  );

  const lines = $derived(flow ? app.logs.filter((l) => l.flowId === flow.flowId) : app.logs);
  const tech = $derived.by(() => {
    if (!flow) return undefined;
    if (flow.status === 'failed') return flow.error?.detail || undefined;
    const t = STEP_COPY[flow.kind]?.[flow.step]?.tech;
    const slot = app.lastStart?.options.slot ?? (app.deviceInfo ?? app.lastSeen)?.slot;
    return t && slot ? fill(t, { slot }) : t;
  });

  function home() {
    if (app.flow === null) app.navigate({ screen: 'home' });
  }
</script>

<div class="shell" inert={app.closeBlocked}>
  <AppHeader
    device={app.device}
    demo={app.info?.demo ?? false}
    navLocked={app.flow !== null}
    onHome={home}
    onNav={(s) => app.navigate({ screen: s })}
  />
  <main>
    <RefreshTransition key={refreshKey}>
      {#if app.screen === 'flow' && flow}
        <FlowScreen />
      {:else if gate}
        <GateScreen {gate} />
      {:else if app.screen === 'home'}
        <Home />
      {:else if app.screen === 'usbGuide'}
        <UsbDebugGuide />
      {:else if app.screen === 'alreadyRooted'}
        <AlreadyRooted />
      {:else if app.screen === 'risk'}
        <RiskNotice />
      {:else if app.screen === 'about'}
        <About />
      {:else if app.screen === 'backups'}
        <Backups />
      {:else if app.screen === 'rescuePick'}
        <RescuePick />
      {/if}
    </RefreshTransition>
  </main>
  <footer>
    <DetailsLog {lines} {tech} onCopy={() => app.copyLog()} />
  </footer>
</div>

{#if app.closeBlocked}
  <CloseGuardDialog onKeep={() => (app.closeBlocked = false)} onClose={() => void app.confirmClose()} />
{/if}

<style>
  .shell {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  /* Screens fill the space above the Details log; one that doesn't fit (Details open on a small
     window) scrolls here rather than squeezing its blocks on top of each other. */
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  main > :global(.flash) {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  footer {
    flex: none;
    padding: var(--space-1) var(--gutter) var(--space-2);
  }
</style>
