<script lang="ts">
  import Button from '../components/Button.svelte';
  import JobCard from '../components/JobCard.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { ROOTED_COPY, fill } from '../copy';

  const app = getApp();
  const info = $derived(app.deviceInfo);
  const body = $derived(
    fill(ROOTED_COPY.body, {
      magiskVersion: info?.magiskVersion ?? '',
      svc: info?.bootServiceRunning ? ROOTED_COPY.svcRunning : ROOTED_COPY.svcStopped,
    }),
  );
  const noStock = $derived(app.stockBackup === null);
</script>

<div class="rooted">
  <StepFrame label={ROOTED_COPY.label} heading={ROOTED_COPY.heading} {body}>
    {#snippet art()}
      <div class="options">
        <JobCard
          title={ROOTED_COPY.reinstall.label}
          desc={ROOTED_COPY.reinstall.desc}
          icon="root"
          size="hero"
          onclick={() => void app.startJob('reinstallBootService', {})}
        />
        <JobCard
          title={ROOTED_COPY.stock.label}
          desc={ROOTED_COPY.stock.desc}
          icon="stock"
          size="hero"
          disabled={noStock}
          onclick={() => void app.startJob('unroot', { backupId: app.stockBackup?.id })}
        />
      </div>
    {/snippet}

    {#if noStock}
      <p class="note">{ROOTED_COPY.noStock}</p>
    {/if}

    {#snippet actions()}
      <Button variant="secondary" onclick={() => app.navigate({ screen: 'home' })}>{ROOTED_COPY.back}</Button>
      {#if noStock}
        <Button onclick={() => void app.importBackup()}>{ROOTED_COPY.importButton}</Button>
      {/if}
    {/snippet}
  </StepFrame>
</div>

<style>
  .rooted {
    display: contents;
  }
  /* Back stays on the left even when it is the only button. */
  .rooted :global(.actions > :first-child) {
    margin-right: auto;
  }
  .options {
    width: 100%;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .note {
    max-width: 68ch;
    padding: var(--space-3) var(--space-4);
    border: var(--border);
    font: var(--fs-meta);
  }
</style>
