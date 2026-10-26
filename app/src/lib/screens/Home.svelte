<script lang="ts">
  import JobCard from '../components/JobCard.svelte';
  import { getApp } from '../context';
  import { HOME_COPY } from '../copy';
  import { backupLine } from '../format';
  import type { BackupSummary } from '../ipc';

  const app = getApp();

  const rooted = $derived(app.deviceInfo?.rooted ?? false);

  /** The stock copy if there is one, else the newest copy of this tablet (shown as "not original"). */
  const shown = $derived.by((): BackupSummary | null => {
    if (app.stockBackup) return app.stockBackup;
    const serial = app.deviceInfo?.serial;
    const mine = app.backups.filter((b) => b.serial === serial);
    return mine.reduce<BackupSummary | null>((best, b) => (best && best.createdAt >= b.createdAt ? best : b), null);
  });

  /** Unroot needs an original copy; without one it could only fail. */
  const canUnroot = $derived(rooted && app.stockBackup !== null);
  const stockDesc = $derived(
    !rooted ? HOME_COPY.stock.metaNotRooted : canUnroot ? HOME_COPY.stock.meta : HOME_COPY.stock.metaNoStock,
  );

  function root() {
    if (rooted) app.navigate({ screen: 'alreadyRooted' });
    else void app.startJob('root', { backupId: app.stockBackup?.id });
  }
</script>

<section class="home">
  <h1>{HOME_COPY.heading}</h1>
  <div class="sub">
    <span class="line">{backupLine(shown)}</span>
    <button type="button" class="textlink" onclick={() => app.navigate({ screen: 'backups' })}>{HOME_COPY.backupsLink}</button>
  </div>

  <div class="choices">
    <JobCard
      title={HOME_COPY.root.title}
      desc={rooted ? HOME_COPY.root.metaRooted : HOME_COPY.root.meta}
      icon="root"
      size="hero"
      primary
      onclick={root}
    />
    <JobCard
      title={HOME_COPY.rescue.title}
      desc={HOME_COPY.rescue.meta}
      icon="rescue"
      size="hero"
      onclick={() => app.navigate({ screen: 'rescuePick' })}
    />
    <JobCard
      title={HOME_COPY.stock.title}
      desc={stockDesc}
      icon="stock"
      size="hero"
      disabled={!canUnroot}
      onclick={() => void app.startJob('unroot', { backupId: app.stockBackup?.id })}
    />
  </div>

  <button type="button" class="textlink foot" onclick={() => app.navigate({ screen: 'usbGuide', keepOpen: true })}>{HOME_COPY.helpLink}</button>
</section>

<style>
  .home {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: var(--space-6) var(--gutter) var(--space-5);
  }
  /* Heading, backup line and cards stay together, centred between the header and the help link. */
  h1 {
    font: var(--fs-display);
    letter-spacing: var(--track-display);
    margin-top: auto;
  }
  .sub {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2) var(--space-4);
    margin-top: var(--space-2);
  }
  .line {
    font: var(--fs-meta);
    color: var(--muted);
  }
  .sub .textlink {
    font: var(--fs-meta);
  }
  /* Mockup .B .choices: three equal cards right under the backup line, 10px apart. */
  .choices {
    margin: var(--space-5) 0 auto;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    grid-auto-rows: minmax(240px, auto);
    gap: 10px;
  }
  .foot {
    align-self: flex-start;
    margin-top: var(--space-4);
    color: var(--muted);
  }
</style>
