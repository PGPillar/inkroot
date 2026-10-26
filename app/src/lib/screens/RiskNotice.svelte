<script lang="ts">
  import Button from '../components/Button.svelte';
  import DitherIcon from '../components/DitherIcon.svelte';
  import LinkNote from '../components/LinkNote.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { RISK_COPY } from '../copy';
  import { DISCLAIMER_URL } from '../links';

  const app = getApp();
  let accepted = $state(false);
</script>

<StepFrame label={RISK_COPY.label} heading={RISK_COPY.heading} tone="danger">
  {#snippet art()}
    <div class="body">
      <DitherIcon name="warning" size={56} />
      <div class="text">
        <ul>
          {#each RISK_COPY.bullets as b (b)}<li>{b}</li>{/each}
        </ul>
        <label class="accept">
          <input type="checkbox" bind:checked={accepted} />
          <span>{RISK_COPY.checkbox}</span>
        </label>
      </div>
    </div>
  {/snippet}

  <LinkNote failed={app.openFailed} urls={[DISCLAIMER_URL]} />

  {#snippet actions()}
    <button type="button" class="textlink lead" onclick={() => void app.openExternal(DISCLAIMER_URL)}>
      {RISK_COPY.link}
    </button>
    <Button variant="secondary" onclick={() => app.navigate({ screen: 'home' })}>{RISK_COPY.back}</Button>
    <Button disabled={!accepted || app.inFlight} onclick={() => void app.acceptRisk()}>{RISK_COPY.continue}</Button>
  {/snippet}
</StepFrame>

<style>
  .lead {
    margin-right: auto;
  }
  .body {
    display: flex;
    gap: var(--space-5);
    align-items: flex-start;
    width: 100%;
  }
  .body :global(svg) {
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  ul {
    margin: 0;
    padding-left: 1.2em;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font: var(--fs-body);
    max-width: 62ch;
  }
  .accept {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    font: 600 15px var(--font-sans);
    cursor: pointer;
  }
  input {
    appearance: none;
    flex: none;
    box-sizing: border-box;
    width: 22px;
    height: 22px;
    margin: 0;
    border: var(--border);
    border-radius: 0;
    background: transparent;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  input:checked::after {
    content: '';
    width: 12px;
    height: 12px;
    background: var(--ink);
  }
  input:focus-visible {
    outline: var(--border);
    outline-offset: 3px;
  }
</style>
