<script lang="ts">
  import { Tablet } from '../art';
  import Button from '../components/Button.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import LinkNote from '../components/LinkNote.svelte';
  import LiveStatus from '../components/LiveStatus.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { GATE_COPY, fill, type GateId } from '../copy';
  import { newFirmwareIssueUrl } from '../links';

  let { gate }: { gate: GateId } = $props();

  const app = getApp();
  const copy = $derived(GATE_COPY[gate]);
  const info = $derived(app.deviceInfo);
  const vars = $derived({
    model: info?.model ?? '',
    battery: info?.battery ?? '',
    displayId: info?.displayId ?? '',
    incremental: info?.incremental ?? '',
  });
  /** A supported-device check failed: the tablet is fine, but Inkroot won't go on. */
  const blocked = $derived(gate === 'unknownModel' || gate === 'unknownFirmware' || gate === 'lowBattery');
  const screenArt = $derived(
    gate === 'unauthorized' ? 'prompt' : gate === 'none' || gate === 'edl' ? 'blank' : 'content',
  );

  function primary() {
    if (gate === 'none') app.navigate({ screen: 'usbGuide' });
    else if (gate === 'edl') app.navigate({ screen: 'rescuePick' });
  }

  const issueUrl = $derived(info ? newFirmwareIssueUrl(info.displayId, info.incremental) : null);

  function secondary() {
    if (gate === 'unknownFirmware' && issueUrl) void app.openExternal(issueUrl);
  }

  function link() {
    if (gate === 'none') app.navigate({ screen: 'rescuePick' });
    else if (gate === 'unauthorized') app.navigate({ screen: 'usbGuide' });
  }
</script>

<StepFrame label={copy.label} heading={copy.heading} body={fill(copy.body, vars)}>
  {#snippet art()}
    <div class="art">
      <Tablet screen={screenArt} title={copy.heading} />
      {#if blocked}<DitherIcon name="warning" size={56} />{/if}
    </div>
  {/snippet}

  {#if copy.meta}
    {#if gate === 'none'}
      <LiveStatus state="waiting" text={copy.meta} />
    {:else}
      <p class="meta">{fill(copy.meta, vars)}</p>
    {/if}
  {/if}
  {#if gate === 'unknownFirmware' && issueUrl}
    <LinkNote failed={app.openFailed} urls={[issueUrl]} />
  {/if}

  {#snippet actions()}
    {#if copy.link}
      <button type="button" class="textlink lead" onclick={link}>{copy.link}</button>
    {/if}
    {#if copy.secondary}
      <Button variant="secondary" onclick={secondary}>{copy.secondary}</Button>
    {/if}
    {#if copy.primary}
      <Button onclick={primary}>{copy.primary}</Button>
    {/if}
  {/snippet}
</StepFrame>

<style>
  /* A text link always sits on the left, buttons on the right. */
  .lead {
    margin-right: auto;
  }
  .art {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }
  .meta {
    font: var(--fs-meta);
    color: var(--muted);
  }
</style>
