<script lang="ts">
  import Button from '../components/Button.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { BACKUPS_COPY, RESCUE_PICK_COPY } from '../copy';
  import { backupDate, groupBySerial } from '../format';
  import type { FlowOptions } from '../ipc';

  type Choice = { backupId: string } | { imagePath: string } | null;

  const app = getApp();
  const groups = $derived(groupBySerial(app.backups));
  const preselect = $derived(app.route.screen === 'rescuePick' ? app.route.preselect : undefined);

  function initial(): Choice {
    if (preselect?.backupId) return { backupId: preselect.backupId };
    const serial = app.lastStart?.serial ?? app.deviceInfo?.serial;
    const newest = groupBySerial(app.backups).find((g) => g.serial === serial)?.backups[0];
    return newest ? { backupId: newest.id } : null;
  }
  let choice = $state<Choice>(initial());

  const fileName = $derived(
    choice && 'imagePath' in choice ? (choice.imagePath.split(/[\\/]/).pop() ?? choice.imagePath) : null,
  );

  async function chooseFile() {
    const imagePath = await app.platform.pickBootImage();
    if (imagePath) choice = { imagePath };
  }

  function start() {
    if (!choice) return;
    const options: FlowOptions = { ...choice };
    // The preselected slot belongs to the preselected backup; any other choice lets the backend
    // decide (the backup's own slot, or the file name and a question).
    if (preselect?.slot && 'backupId' in choice && choice.backupId === preselect.backupId) options.slot = preselect.slot;
    void app.startJob('rescue', options);
  }
</script>

<StepFrame label={RESCUE_PICK_COPY.label} heading={RESCUE_PICK_COPY.heading} body={RESCUE_PICK_COPY.body}>
  {#snippet art()}
    <div class="list">
      {#if groups.length === 0}
        <p class="empty">{RESCUE_PICK_COPY.empty}</p>
      {/if}
      {#each groups as g (g.serial)}
        <section>
          <h2>{g.serial}</h2>
          {#each g.backups as b (b.id)}
            <label class="row">
              <input
                type="radio"
                name="backup"
                checked={choice !== null && 'backupId' in choice && choice.backupId === b.id}
                onchange={() => (choice = { backupId: b.id })}
              />
              <span class="meta">
                {backupDate(b)} · slot {b.slot.toUpperCase()} · {b.stock ? BACKUPS_COPY.original : BACKUPS_COPY.notOriginal}
              </span>
            </label>
          {/each}
        </section>
      {/each}
    </div>
  {/snippet}

  <div class="file">
    <Button variant="secondary" onclick={() => void chooseFile()}>{RESCUE_PICK_COPY.chooseFile}</Button>
    {#if fileName}<span class="name">{fileName}</span>{/if}
  </div>

  {#snippet actions()}
    <Button variant="secondary" onclick={() => app.navigate({ screen: 'home' })}>{RESCUE_PICK_COPY.back}</Button>
    <Button disabled={choice === null || app.inFlight} onclick={start}>{RESCUE_PICK_COPY.continue}</Button>
  {/snippet}
</StepFrame>

<style>
  .list {
    width: 100%;
    align-self: flex-start;
    max-height: 100%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  h2 {
    margin: 0 0 var(--space-2);
    font: 600 12px var(--font-mono);
    letter-spacing: var(--track-label);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-top: var(--border);
    cursor: pointer;
  }
  .meta,
  .empty {
    font: var(--fs-meta);
  }
  .empty {
    margin: 0;
  }
  input {
    appearance: none;
    flex: none;
    box-sizing: border-box;
    width: 20px;
    height: 20px;
    margin: 0;
    border: var(--border);
    border-radius: 0;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  input:checked::after {
    content: '';
    width: 10px;
    height: 10px;
    background: var(--ink);
  }
  input:focus-visible {
    outline: var(--border);
    outline-offset: 3px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .name {
    font: 500 13px var(--font-mono);
    overflow-wrap: anywhere;
  }
</style>
