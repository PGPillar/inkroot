<script lang="ts">
  import { CHOOSE_SLOT_HINTS, EDL_NOT_FOUND_AUTO, PROMPT_COPY, fill } from '../copy';
  import type { DeviceStatus, FlowKind, Prompt } from '../ipc';
  import { DeepFlashCable, PressIndicator, Tablet, UsbAPort } from '../art';
  import Button from './Button.svelte';
  import CableBuyPanel from './CableBuyPanel.svelte';
  import CableFoot from './CableFoot.svelte';
  import DitherIcon from './DitherIcon.svelte';
  import StepFrame from './StepFrame.svelte';

  let {
    prompt,
    kind,
    device,
    found,
    imageName,
    disabled,
    onAnswer,
    onCopy,
  }: {
    prompt: Prompt;
    kind: FlowKind;
    device: DeviceStatus;
    /** The tablet is in rescue mode (or the flow is past waiting for it). Defaults to the live status. */
    found?: boolean;
    imageName?: string;
    disabled: boolean;
    onAnswer: (optionId: string) => void;
    onOpen: (url: string) => void;
    onCopy: (text: string) => void;
  } = $props();

  // Per-prompt state is derived from the prompt id, so a new prompt resets it in the same render.
  let answeredId = $state<string>();
  let buyId = $state<string>();
  let tickedId = $state<string>();
  const answered = $derived(answeredId === prompt.id);
  const showBuy = $derived(buyId === prompt.id);
  const ticked = $derived(tickedId === prompt.id);
  let root: HTMLElement | undefined = $state();

  // Focus the first enabled primary option when a prompt appears or the buy panel closes.
  $effect(() => {
    prompt.id;
    showBuy;
    root?.querySelector<HTMLButtonElement>('button.primary:not(:disabled)')?.focus();
  });

  const copy = $derived(prompt.kind === 'edlNotFound' && kind !== 'rescue' ? EDL_NOT_FOUND_AUTO : PROMPT_COPY[prompt.kind]);
  const options = $derived([
    ...prompt.options.filter((o) => o.emphasis !== 'danger'),
    ...prompt.options.filter((o) => o.emphasis === 'danger'),
  ]);

  const body = $derived.by(() => {
    if (prompt.kind !== 'chooseSlot') return copy.body;
    const fromName = imageName?.match(/boot_([ab])(?![a-z0-9])/i)?.[1];
    const slot = fromName ?? (device.kind === 'adb' ? device.info.slot : undefined);
    const hint = slot
      ? fill(fromName ? CHOOSE_SLOT_HINTS.fromName : CHOOSE_SLOT_HINTS.active, { S: slot.toUpperCase() })
      : CHOOSE_SLOT_HINTS.none;
    return fill(copy.body, { hint }).trim();
  });

  const label = (id: string, raw: string) => copy.options[id] ?? raw;
  const isDisabled = (id: string) =>
    disabled || answered || (prompt.kind === 'cableCheckConfirm' && id === 'confirmContinue' && !ticked);

  function choose(id: string) {
    if (isDisabled(id)) return;
    if (prompt.kind === 'cableCheck' && id === 'whereToGet') {
      buyId = prompt.id;
      return;
    }
    answeredId = prompt.id;
    onAnswer(id);
  }
</script>

{#if showBuy}
  <CableBuyPanel onBack={() => (buyId = undefined)} {onCopy} />
{:else}
  <div bind:this={root} class="prompt">
    <StepFrame label={copy.label} heading={copy.heading} body={body || undefined}>
      {#snippet art()}
        {#if prompt.kind === 'retryWrite'}
          <DeepFlashCable highlight="usbC" title={copy.heading} />
        {:else if prompt.kind === 'edlNotFound' && kind === 'rescue'}
          <UsbAPort variant="adapterCrossed" highlight="port" width={210} title={copy.bullets?.[1] ?? copy.heading} />
        {:else if prompt.kind === 'cableCheck'}
          <DeepFlashCable highlight="button" width={360} title={copy.heading} />
        {:else if prompt.kind === 'keepWaitingOrRescue'}
          <Tablet screen="logo" width={124} title={copy.heading} />
        {:else if prompt.kind === 'stillInEdl'}
          <PressIndicator width={132} seconds={10} title={copy.bullets?.[0] ?? copy.heading} />
        {:else if prompt.kind === 'installDriver' || prompt.kind === 'installUdev'}
          <DitherIcon name="info" size={64} cell={4} />
        {/if}
      {/snippet}
      {#snippet children()}
        {#if copy.bullets}
          <ul>{#each copy.bullets as b}<li>{b}</li>{/each}</ul>
        {/if}
        {#if prompt.kind === 'edlNotFound' && kind === 'rescue'}
          <CableFoot found={found ?? device.kind === 'edl'} />
        {/if}
        {#if copy.checkbox}
          <label class="tick"><input type="checkbox" checked={ticked} onchange={(e) => (tickedId = e.currentTarget.checked ? prompt.id : undefined)} {disabled} /> {copy.checkbox}</label>
        {/if}
      {/snippet}
      {#snippet actions()}
        <!-- DOM (and tab) order stays primary-first, danger-last (R9). The row is laid out right to left,
             like every other flow screen: the primary sits far right, danger far left. -->
        <div class="options">
          {#each options as o (o.id)}
            <span class="opt {o.emphasis}">
              <Button variant={o.emphasis} disabled={isDisabled(o.id)} onclick={() => choose(o.id)}>{label(o.id, o.label)}</Button>
            </span>
          {/each}
        </div>
      {/snippet}
    </StepFrame>
  </div>
{/if}

<style>
  .prompt {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .options {
    flex: 1;
    display: flex;
    flex-direction: row-reverse;
    align-items: center;
    gap: var(--space-3);
  }
  .opt.primary { order: 0; }
  .opt.secondary { order: 1; }
  .opt.danger {
    order: 2;
    margin-right: auto;
  }
  ul {
    margin: 0;
    padding-left: var(--space-5, 20px);
    max-width: 56ch;
    font: var(--fs-body);
  }
  .tick {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    font: var(--fs-body);
  }
  .tick input {
    width: 20px;
    height: 20px;
    accent-color: var(--ink);
  }
</style>
