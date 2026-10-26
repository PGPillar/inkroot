<script lang="ts">
  import Button from '../components/Button.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { BACKUPS_COPY, fill } from '../copy';
  import { backupDate, groupBySerial, revealLabel } from '../format';
  import type { BackupSummary } from '../ipc';

  const app = getApp();
  const groups = $derived(groupBySerial(app.backups));
  const canBackUp = $derived(app.device.kind === 'adb' && app.device.gate === null);
  const os = $derived(app.info?.os ?? 'macos');

  function meta(b: BackupSummary): string {
    return fill(BACKUPS_COPY.rowMeta, {
      serial: b.serial,
      date: backupDate(b),
      S: b.slot.toUpperCase(),
      original: b.stock ? BACKUPS_COPY.original : BACKUPS_COPY.notOriginal,
      verified: b.verified ? BACKUPS_COPY.verified : BACKUPS_COPY.notVerified,
      imported: b.source === 'imported' ? BACKUPS_COPY.imported : '',
    });
  }
</script>

<StepFrame
  label={BACKUPS_COPY.label}
  heading={BACKUPS_COPY.heading}
  body={fill(BACKUPS_COPY.body, { backupRoot: app.info?.backupRoot ?? '' })}
>
  {#snippet art()}
    <div class="list">
      {#if groups.length === 0}
        <p class="empty">{BACKUPS_COPY.empty}</p>
      {/if}
      {#each groups as g (g.serial)}
        <section>
          <h2>{g.serial}</h2>
          {#each g.backups as b (b.id)}
            <div class="row">
              <span class="meta">{meta(b)}</span>
              <Button variant="secondary" onclick={() => void app.reveal(b.path)}>{revealLabel(os)}</Button>
            </div>
          {/each}
        </section>
      {/each}
    </div>
  {/snippet}

  {#if app.deviceInfo?.rooted}
    <p class="note">{BACKUPS_COPY.rootedNote}</p>
  {/if}
  {#if !canBackUp}
    <p class="hint">{BACKUPS_COPY.backUpDisabled}</p>
  {/if}

  {#snippet actions()}
    <Button variant="secondary" onclick={() => app.navigate({ screen: 'home' })}>{BACKUPS_COPY.back}</Button>
    <Button variant="secondary" onclick={() => void app.importBackup()}>{BACKUPS_COPY.importButton}</Button>
    <Button disabled={!canBackUp || app.inFlight} onclick={() => void app.startJob('backup', {})}>
      {BACKUPS_COPY.backUpNow}
    </Button>
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
    gap: var(--space-4);
  }
  h2 {
    margin: 0 0 var(--space-2);
    font: 600 12px var(--font-mono);
    letter-spacing: var(--track-label);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) 0;
    border-top: var(--border);
  }
  .row :global(.btn) {
    padding: 6px 12px;
    font-size: 13px;
  }
  .meta {
    font: var(--fs-meta);
  }
  .empty,
  .hint {
    margin: 0;
    font: var(--fs-meta);
  }
  .hint {
    color: var(--muted);
  }
  .note {
    margin: 0;
    max-width: 68ch;
    padding: var(--space-2) var(--space-4);
    border: var(--border);
    font: var(--fs-meta);
  }
</style>
