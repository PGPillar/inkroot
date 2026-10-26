<script lang="ts">
  import Button from '../components/Button.svelte';
  import LinkNote from '../components/LinkNote.svelte';
  import StepFrame from '../components/StepFrame.svelte';
  import { getApp } from '../context';
  import { ABOUT_COPY, COMMON, fill } from '../copy';
  import { DISCLAIMER_URL, NOTICES_URL, REPO } from '../links';

  const app = getApp();
  const vars = $derived({ version: app.info?.version ?? '', backupRoot: app.info?.backupRoot ?? '' });
  const open = (url: string) => void app.openExternal(url);
</script>

<StepFrame label={ABOUT_COPY.label} heading={ABOUT_COPY.heading}>
  {#snippet art()}
    <div class="lines">
      {#each ABOUT_COPY.lines as line (line)}
        <p class:mono={line.includes('{')}>{fill(line, vars)}</p>
      {/each}
      <div class="links">
        <button type="button" class="textlink" onclick={() => open(DISCLAIMER_URL)}>{ABOUT_COPY.links.disclaimer}</button>
        <button type="button" class="textlink" onclick={() => open(NOTICES_URL)}>{ABOUT_COPY.links.licenses}</button>
        <button type="button" class="textlink" onclick={() => open(REPO)}>{ABOUT_COPY.links.source}</button>
      </div>
      <LinkNote failed={app.openFailed} urls={[DISCLAIMER_URL, NOTICES_URL, REPO]} />
    </div>
  {/snippet}

  {#snippet actions()}
    <Button variant="secondary" onclick={() => app.navigate({ screen: 'home' })}>{COMMON.backToStart}</Button>
  {/snippet}
</StepFrame>

<style>
  .lines {
    width: 100%;
    align-self: flex-start;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  p {
    margin: 0;
    max-width: 68ch;
    font: var(--fs-body);
  }
  p.mono {
    font: var(--fs-meta);
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .links {
    display: flex;
    gap: var(--space-5);
    margin-top: var(--space-3);
  }
</style>
